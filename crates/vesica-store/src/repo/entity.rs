//! Repository for `entity` — 04-data-model.md 4.2.1.
//!
//! This does not touch `entity_prop`: property read/write, normalisation
//! and validation belong to the domain-aware service layer a later ticket
//! builds on top of `vesica-core`. This is deliberately the thin plumbing
//! layer underneath that.

use sqlx::FromRow;

use crate::error::StoreError;
use crate::ids;
use crate::store::Db;

/// Fields a caller supplies when creating a new entity.
#[derive(Debug, Clone)]
pub struct NewEntity {
    pub case_id: String,
    pub schema_id: String,
    pub caption: String,
    /// `confirmed` | `probable` | `hypothesis` — validated by the service
    /// layer, not here.
    pub confidence: String,
    pub created_by: String,
}

/// An entity as stored.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Entity {
    pub id: String,
    pub case_id: String,
    pub schema_id: String,
    pub caption: String,
    pub confidence: String,
    pub created_at: String,
    pub created_by: String,
    pub updated_at: String,
}

/// Persistence for entities. See the `repo` module docs for why this trait
/// exists and why it has exactly one implementation.
// See the comment on `EntityRepo`'s `#[allow(async_fn_in_trait)]` below;
// the same reasoning applies to every repository trait in this module.
#[allow(async_fn_in_trait)]
pub trait EntityRepo: Send + Sync {
    /// Creates a new entity, generating its id and timestamps, and returns
    /// it as stored. Fails with [`StoreError::Database`] wrapping a
    /// foreign-key violation if `case_id` does not name an existing case.
    async fn create(&self, new: NewEntity) -> Result<Entity, StoreError>;

    /// Looks up an entity by id. `None` if it does not exist, is
    /// soft-deleted, or has been merged away.
    async fn get(&self, id: &str) -> Result<Option<Entity>, StoreError>;
}

/// SQLite-backed [`EntityRepo`].
pub struct SqliteEntityRepo {
    db: Db,
}

impl SqliteEntityRepo {
    pub fn new(db: Db) -> Self {
        SqliteEntityRepo { db }
    }
}

impl EntityRepo for SqliteEntityRepo {
    async fn create(&self, new: NewEntity) -> Result<Entity, StoreError> {
        let id = ids::new_id();
        let now = ids::now();

        self.db
            .write(move |conn| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO entity \
                         (id, case_id, schema_id, caption, confidence, created_at, created_by, updated_at) \
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&id)
                    .bind(&new.case_id)
                    .bind(&new.schema_id)
                    .bind(&new.caption)
                    .bind(&new.confidence)
                    .bind(&now)
                    .bind(&new.created_by)
                    .bind(&now)
                    .execute(&mut *conn)
                    .await?;

                    Ok(Entity {
                        id,
                        case_id: new.case_id,
                        schema_id: new.schema_id,
                        caption: new.caption,
                        confidence: new.confidence,
                        created_at: now.clone(),
                        created_by: new.created_by,
                        updated_at: now,
                    })
                })
            })
            .await
    }

    async fn get(&self, id: &str) -> Result<Option<Entity>, StoreError> {
        let entity = sqlx::query_as::<_, Entity>(
            "SELECT id, case_id, schema_id, caption, confidence, created_at, created_by, updated_at \
             FROM entity WHERE id = ? AND deleted_at IS NULL AND merged_into IS NULL",
        )
        .bind(id)
        .fetch_optional(self.db.read_pool())
        .await?;
        Ok(entity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testing::{seed_case, test_store};

    #[tokio::test]
    async fn create_then_get_round_trips_and_references_its_case() {
        let store = test_store().await.unwrap();
        let entities = SqliteEntityRepo::new(store.db.clone());

        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();

        let created = entities
            .create(NewEntity {
                case_id: case.id.clone(),
                schema_id: "Person".into(),
                caption: "Jan Kowalski".into(),
                confidence: "probable".into(),
                created_by: "human".into(),
            })
            .await
            .unwrap();

        assert_eq!(created.case_id, case.id);
        let fetched = entities.get(&created.id).await.unwrap();
        assert_eq!(fetched, Some(created));
    }

    #[tokio::test]
    async fn creating_an_entity_for_a_missing_case_is_rejected() {
        let store = test_store().await.unwrap();
        let entities = SqliteEntityRepo::new(store.db.clone());

        let result = entities
            .create(NewEntity {
                case_id: "does-not-exist".into(),
                schema_id: "Person".into(),
                caption: "Ghost".into(),
                confidence: "probable".into(),
                created_by: "human".into(),
            })
            .await;
        assert!(result.is_err());
    }
}
