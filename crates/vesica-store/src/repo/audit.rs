//! Repository for `audit_log` — 04-data-model.md 4.2.6.

use sqlx::FromRow;

use crate::error::StoreError;
use crate::ids;
use crate::store::Db;

/// Maximum rows [`AuditRepo::list_for_target`] returns, regardless of what
/// is asked for. AGENT-BRIEF.md rule 5: every list is bounded.
pub const MAX_LIST_LIMIT: i64 = 1_000;

/// Fields a caller supplies when recording a new audit entry.
#[derive(Debug, Clone)]
pub struct NewAuditEntry {
    /// `'human'` or an agent name.
    pub actor: String,
    /// `create` | `update` | `delete` | `merge` | `link` | `import`.
    pub action: String,
    pub case_id: Option<String>,
    /// `entity` | `relation` | `case` | ... — whatever kind of row `target_id`
    /// names.
    pub target_kind: String,
    pub target_id: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub batch_id: Option<String>,
}

/// An audit entry as stored.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct AuditEntry {
    pub id: i64,
    pub at: String,
    pub actor: String,
    pub action: String,
    pub case_id: Option<String>,
    pub target_kind: String,
    pub target_id: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub batch_id: Option<String>,
}

/// Persistence for the audit trail. See the `repo` module docs for why this
/// trait exists and why it has exactly one implementation.
// See the comment on `CaseRepo`'s `#[allow(async_fn_in_trait)]` in case.rs.
#[allow(async_fn_in_trait)]
pub trait AuditRepo: Send + Sync {
    /// Records a new audit entry and returns it, including its assigned id.
    async fn record(&self, new: NewAuditEntry) -> Result<AuditEntry, StoreError>;

    /// The most recent entries for one target, newest first, bounded by
    /// [`MAX_LIST_LIMIT`] regardless of `limit`.
    async fn list_for_target(
        &self,
        target_kind: &str,
        target_id: &str,
        limit: i64,
    ) -> Result<Vec<AuditEntry>, StoreError>;
}

/// SQLite-backed [`AuditRepo`].
pub struct SqliteAuditRepo {
    db: Db,
}

impl SqliteAuditRepo {
    pub fn new(db: Db) -> Self {
        SqliteAuditRepo { db }
    }
}

impl AuditRepo for SqliteAuditRepo {
    async fn record(&self, new: NewAuditEntry) -> Result<AuditEntry, StoreError> {
        let at = ids::now();

        self.db
            .write(move |conn| {
                Box::pin(async move {
                    let id: i64 = sqlx::query_scalar(
                        "INSERT INTO audit_log (at, actor, action, case_id, target_kind, target_id, before, after, batch_id) \
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
                         RETURNING id",
                    )
                    .bind(&at)
                    .bind(&new.actor)
                    .bind(&new.action)
                    .bind(&new.case_id)
                    .bind(&new.target_kind)
                    .bind(&new.target_id)
                    .bind(&new.before)
                    .bind(&new.after)
                    .bind(&new.batch_id)
                    .fetch_one(&mut *conn)
                    .await?;

                    Ok(AuditEntry {
                        id,
                        at,
                        actor: new.actor,
                        action: new.action,
                        case_id: new.case_id,
                        target_kind: new.target_kind,
                        target_id: new.target_id,
                        before: new.before,
                        after: new.after,
                        batch_id: new.batch_id,
                    })
                })
            })
            .await
    }

    async fn list_for_target(
        &self,
        target_kind: &str,
        target_id: &str,
        limit: i64,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        let limit = limit.clamp(1, MAX_LIST_LIMIT);
        let entries = sqlx::query_as::<_, AuditEntry>(
            "SELECT id, at, actor, action, case_id, target_kind, target_id, before, after, batch_id \
             FROM audit_log WHERE target_kind = ? AND target_id = ? ORDER BY at DESC, id DESC LIMIT ?",
        )
        .bind(target_kind)
        .bind(target_id)
        .bind(limit)
        .fetch_all(self.db.read_pool())
        .await?;
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::test_store;

    #[tokio::test]
    async fn record_then_list_for_target_returns_newest_first() {
        let store = test_store().await.unwrap();
        let audit = SqliteAuditRepo::new(store.db.clone());

        for action in ["create", "update", "update"] {
            audit
                .record(NewAuditEntry {
                    actor: "human".into(),
                    action: action.into(),
                    case_id: None,
                    target_kind: "entity".into(),
                    target_id: "entity-1".into(),
                    before: None,
                    after: None,
                    batch_id: None,
                })
                .await
                .unwrap();
        }

        let entries = audit
            .list_for_target("entity", "entity-1", 10)
            .await
            .unwrap();
        assert_eq!(entries.len(), 3);
        // Newest-first: the third recorded row (id highest) comes first.
        assert!(entries[0].id >= entries[1].id);
        assert!(entries[1].id >= entries[2].id);
    }

    #[tokio::test]
    async fn list_for_target_is_bounded_even_when_a_larger_limit_is_requested() {
        let store = test_store().await.unwrap();
        let audit = SqliteAuditRepo::new(store.db.clone());

        for _ in 0..5 {
            audit
                .record(NewAuditEntry {
                    actor: "human".into(),
                    action: "create".into(),
                    case_id: None,
                    target_kind: "entity".into(),
                    target_id: "entity-1".into(),
                    before: None,
                    after: None,
                    batch_id: None,
                })
                .await
                .unwrap();
        }

        let entries = audit
            .list_for_target("entity", "entity-1", 2)
            .await
            .unwrap();
        assert_eq!(entries.len(), 2);
    }
}
