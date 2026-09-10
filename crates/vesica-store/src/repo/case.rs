//! Repository for `kase` — 04-data-model.md 4.2.1. The table is spelled
//! `kase` because `case` is a reserved word in SQL; the Rust names stay
//! `Case`/`CaseRepo` since Rust has no such restriction.

use sqlx::FromRow;

use crate::error::StoreError;
use crate::ids;
use crate::store::Db;

/// Fields a caller supplies when opening a new case.
#[derive(Debug, Clone)]
pub struct NewCase {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub default_region: Option<String>,
}

/// A case as stored.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Case {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub status: String,
    pub default_region: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Persistence for cases. See the `repo` module docs for why this trait
/// exists and why it has exactly one implementation.
// `async fn` in a public trait doesn't pin down `Send` on the returned
// future, which matters once a trait is called through `dyn`. These traits
// are deliberately not dyn-dispatched anywhere yet (see the `repo` module
// docs — the Postgres seam is unused), and every call site here is a
// concrete `Sqlite*Repo`, so the future is always `Send` in practice.
#[allow(async_fn_in_trait)]
pub trait CaseRepo: Send + Sync {
    /// Creates a new case, generating its id and timestamps, and returns it
    /// as stored.
    async fn create(&self, new: NewCase) -> Result<Case, StoreError>;

    /// Looks up a case by id. `None` if it does not exist or is
    /// soft-deleted.
    async fn get(&self, id: &str) -> Result<Option<Case>, StoreError>;
}

/// SQLite-backed [`CaseRepo`].
pub struct SqliteCaseRepo {
    db: Db,
}

impl SqliteCaseRepo {
    pub fn new(db: Db) -> Self {
        SqliteCaseRepo { db }
    }
}

impl CaseRepo for SqliteCaseRepo {
    async fn create(&self, new: NewCase) -> Result<Case, StoreError> {
        let id = ids::new_id();
        let now = ids::now();

        self.db
            .write(move |conn| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO kase \
                         (id, slug, name, description, color, status, default_region, created_at, updated_at) \
                         VALUES (?, ?, ?, ?, ?, 'active', ?, ?, ?)",
                    )
                    .bind(&id)
                    .bind(&new.slug)
                    .bind(&new.name)
                    .bind(&new.description)
                    .bind(&new.color)
                    .bind(&new.default_region)
                    .bind(&now)
                    .bind(&now)
                    .execute(&mut *conn)
                    .await?;

                    Ok(Case {
                        id,
                        slug: new.slug,
                        name: new.name,
                        description: new.description,
                        color: new.color,
                        status: "active".to_string(),
                        default_region: new.default_region,
                        created_at: now.clone(),
                        updated_at: now,
                    })
                })
            })
            .await
    }

    async fn get(&self, id: &str) -> Result<Option<Case>, StoreError> {
        let case = sqlx::query_as::<_, Case>(
            "SELECT id, slug, name, description, color, status, default_region, created_at, updated_at \
             FROM kase WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(self.db.read_pool())
        .await?;
        Ok(case)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::test_store;

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let store = test_store().await.unwrap();
        let repo = SqliteCaseRepo::new(store.db.clone());

        let created = repo
            .create(NewCase {
                slug: "op-lighthouse".into(),
                name: "Operation Lighthouse".into(),
                description: Some("test case".into()),
                color: Some("#3b82f6".into()),
                default_region: Some("PL".into()),
            })
            .await
            .unwrap();

        assert_eq!(created.id.len(), 26, "ids are 26-char ULIDs");
        assert_eq!(created.status, "active");

        let fetched = repo.get(&created.id).await.unwrap();
        assert_eq!(fetched, Some(created));
    }

    #[tokio::test]
    async fn get_of_a_missing_id_is_none_not_an_error() {
        let store = test_store().await.unwrap();
        let repo = SqliteCaseRepo::new(store.db.clone());
        assert_eq!(repo.get("does-not-exist").await.unwrap(), None);
    }
}
