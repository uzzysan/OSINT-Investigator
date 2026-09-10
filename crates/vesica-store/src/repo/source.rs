//! Repository for `source` — 04-data-model.md 4.2.2 (evidence: where a fact
//! came from). Attachments, blobs and citations belong to a later ticket;
//! this is just enough to create and look up a source record itself.

use sqlx::FromRow;

use crate::error::StoreError;
use crate::ids;
use crate::store::Db;

/// Fields a caller supplies when recording a new source.
#[derive(Debug, Clone)]
pub struct NewSource {
    pub case_id: String,
    /// `url` | `file` | `manual` | `agent`.
    pub kind: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub created_by: String,
}

/// A source as stored.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Source {
    pub id: String,
    pub case_id: String,
    pub kind: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub created_at: String,
    pub created_by: String,
}

/// Persistence for sources. See the `repo` module docs for why this trait
/// exists and why it has exactly one implementation.
// See the comment on `EntityRepo`'s `#[allow(async_fn_in_trait)]` below;
// the same reasoning applies to every repository trait in this module.
#[allow(async_fn_in_trait)]
pub trait SourceRepo: Send + Sync {
    /// Creates a new source, generating its id and timestamp, and returns it
    /// as stored. Fails with [`StoreError::Database`] wrapping a
    /// foreign-key violation if `case_id` does not name an existing case.
    async fn create(&self, new: NewSource) -> Result<Source, StoreError>;

    /// Looks up a source by id.
    async fn get(&self, id: &str) -> Result<Option<Source>, StoreError>;
}

/// SQLite-backed [`SourceRepo`].
pub struct SqliteSourceRepo {
    db: Db,
}

impl SqliteSourceRepo {
    pub fn new(db: Db) -> Self {
        SqliteSourceRepo { db }
    }
}

impl SourceRepo for SqliteSourceRepo {
    async fn create(&self, new: NewSource) -> Result<Source, StoreError> {
        let id = ids::new_id();
        let now = ids::now();

        self.db
            .write(move |conn| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO source (id, case_id, kind, title, url, created_at, created_by) \
                         VALUES (?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&id)
                    .bind(&new.case_id)
                    .bind(&new.kind)
                    .bind(&new.title)
                    .bind(&new.url)
                    .bind(&now)
                    .bind(&new.created_by)
                    .execute(&mut *conn)
                    .await?;

                    Ok(Source {
                        id,
                        case_id: new.case_id,
                        kind: new.kind,
                        title: new.title,
                        url: new.url,
                        created_at: now,
                        created_by: new.created_by,
                    })
                })
            })
            .await
    }

    async fn get(&self, id: &str) -> Result<Option<Source>, StoreError> {
        let source = sqlx::query_as::<_, Source>(
            "SELECT id, case_id, kind, title, url, created_at, created_by FROM source WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.db.read_pool())
        .await?;
        Ok(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testing::{seed_case, test_store};

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let store = test_store().await.unwrap();
        let sources = SqliteSourceRepo::new(store.db.clone());

        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();

        let created = sources
            .create(NewSource {
                case_id: case.id.clone(),
                kind: "url".into(),
                title: Some("Companies register filing".into()),
                url: Some("https://example.test/filing/1".into()),
                created_by: "agent:vesica-mcp".into(),
            })
            .await
            .unwrap();

        let fetched = sources.get(&created.id).await.unwrap();
        assert_eq!(fetched, Some(created));
    }
}
