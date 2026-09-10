//! Repository for `relation` — 04-data-model.md 4.2.1. The source-entity
//! column is `source_id_` (trailing underscore) in the schema so it cannot
//! be confused with a reference to the `source` (evidence) table; the Rust
//! field stays `source_id`.

use sqlx::FromRow;

use crate::error::StoreError;
use crate::ids;
use crate::store::Db;

/// Fields a caller supplies when creating a new relation.
#[derive(Debug, Clone)]
pub struct NewRelation {
    pub case_id: String,
    pub schema_id: String,
    pub source_id: String,
    pub target_id: String,
    pub directed: bool,
    pub confidence: String,
    pub created_by: String,
}

/// A relation as stored.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Relation {
    pub id: String,
    pub case_id: String,
    pub schema_id: String,
    #[sqlx(rename = "source_id_")]
    pub source_id: String,
    pub target_id: String,
    pub directed: bool,
    pub confidence: String,
    pub created_at: String,
    pub created_by: String,
    pub updated_at: String,
}

/// Persistence for relations. See the `repo` module docs for why this trait
/// exists and why it has exactly one implementation.
// See the comment on `CaseRepo`'s `#[allow(async_fn_in_trait)]` in case.rs.
#[allow(async_fn_in_trait)]
pub trait RelationRepo: Send + Sync {
    /// Creates a new relation, generating its id and timestamps, and
    /// returns it as stored. Fails with [`StoreError::Database`] wrapping a
    /// foreign-key violation if `case_id`, `source_id` or `target_id` do not
    /// name existing rows.
    async fn create(&self, new: NewRelation) -> Result<Relation, StoreError>;

    /// Looks up a relation by id. `None` if it does not exist or is
    /// soft-deleted.
    async fn get(&self, id: &str) -> Result<Option<Relation>, StoreError>;
}

/// SQLite-backed [`RelationRepo`].
pub struct SqliteRelationRepo {
    db: Db,
}

impl SqliteRelationRepo {
    pub fn new(db: Db) -> Self {
        SqliteRelationRepo { db }
    }
}

impl RelationRepo for SqliteRelationRepo {
    async fn create(&self, new: NewRelation) -> Result<Relation, StoreError> {
        let id = ids::new_id();
        let now = ids::now();

        self.db
            .write(move |conn| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO relation \
                         (id, case_id, schema_id, source_id_, target_id, directed, confidence, created_at, created_by, updated_at) \
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&id)
                    .bind(&new.case_id)
                    .bind(&new.schema_id)
                    .bind(&new.source_id)
                    .bind(&new.target_id)
                    .bind(new.directed)
                    .bind(&new.confidence)
                    .bind(&now)
                    .bind(&new.created_by)
                    .bind(&now)
                    .execute(&mut *conn)
                    .await?;

                    Ok(Relation {
                        id,
                        case_id: new.case_id,
                        schema_id: new.schema_id,
                        source_id: new.source_id,
                        target_id: new.target_id,
                        directed: new.directed,
                        confidence: new.confidence,
                        created_at: now.clone(),
                        created_by: new.created_by,
                        updated_at: now,
                    })
                })
            })
            .await
    }

    async fn get(&self, id: &str) -> Result<Option<Relation>, StoreError> {
        let relation = sqlx::query_as::<_, Relation>(
            "SELECT id, case_id, schema_id, source_id_, target_id, directed, confidence, created_at, created_by, updated_at \
             FROM relation WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(self.db.read_pool())
        .await?;
        Ok(relation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{CaseRepo, EntityRepo, NewCase, NewEntity, SqliteCaseRepo, SqliteEntityRepo};
    use crate::testing::test_store;

    #[tokio::test]
    async fn create_then_get_round_trips_and_references_its_endpoints() {
        let store = test_store().await.unwrap();
        let cases = SqliteCaseRepo::new(store.db.clone());
        let entities = SqliteEntityRepo::new(store.db.clone());
        let relations = SqliteRelationRepo::new(store.db.clone());

        let case = cases
            .create(NewCase {
                slug: "op-lighthouse".into(),
                name: "Operation Lighthouse".into(),
                description: None,
                color: None,
                default_region: None,
            })
            .await
            .unwrap();

        let a = entities
            .create(NewEntity {
                case_id: case.id.clone(),
                schema_id: "Person".into(),
                caption: "A".into(),
                confidence: "probable".into(),
                created_by: "human".into(),
            })
            .await
            .unwrap();
        let b = entities
            .create(NewEntity {
                case_id: case.id.clone(),
                schema_id: "Company".into(),
                caption: "B".into(),
                confidence: "probable".into(),
                created_by: "human".into(),
            })
            .await
            .unwrap();

        let created = relations
            .create(NewRelation {
                case_id: case.id.clone(),
                schema_id: "Ownership".into(),
                source_id: a.id.clone(),
                target_id: b.id.clone(),
                directed: true,
                confidence: "confirmed".into(),
                created_by: "human".into(),
            })
            .await
            .unwrap();

        assert_eq!(created.source_id, a.id);
        assert_eq!(created.target_id, b.id);
        assert!(created.directed);

        let fetched = relations.get(&created.id).await.unwrap();
        assert_eq!(fetched, Some(created));
    }

    #[tokio::test]
    async fn creating_a_relation_to_a_missing_entity_is_rejected() {
        let store = test_store().await.unwrap();
        let cases = SqliteCaseRepo::new(store.db.clone());
        let entities = SqliteEntityRepo::new(store.db.clone());
        let relations = SqliteRelationRepo::new(store.db.clone());

        let case = cases
            .create(NewCase {
                slug: "op-lighthouse".into(),
                name: "Operation Lighthouse".into(),
                description: None,
                color: None,
                default_region: None,
            })
            .await
            .unwrap();
        let a = entities
            .create(NewEntity {
                case_id: case.id.clone(),
                schema_id: "Person".into(),
                caption: "A".into(),
                confidence: "probable".into(),
                created_by: "human".into(),
            })
            .await
            .unwrap();

        let result = relations
            .create(NewRelation {
                case_id: case.id,
                schema_id: "Ownership".into(),
                source_id: a.id,
                target_id: "does-not-exist".into(),
                directed: true,
                confidence: "confirmed".into(),
                created_by: "human".into(),
            })
            .await;
        assert!(result.is_err());
    }
}
