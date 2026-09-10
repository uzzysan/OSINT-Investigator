//! Persistence for `audit_log` — 04-data-model.md 4.2.6.
//!
//! Append an entry, read entries back, and — exactly once, for exactly one
//! reason — redact the payload of entries whose subject has been erased.
//! **No entry is ever deleted, and no field other than `before`/`after` is
//! ever modified**, here or anywhere else in the crate.
//!
//! Spec 0001 says "The table has no update or delete path in the code at
//! all; append-only is enforced by the absence of the capability, not by a
//! convention." That was written before it was reconciled with
//! `09-security-and-legal.md`, whose data-protection table settles what
//! purging owes a data subject:
//!
//! > Right of access / erasure — Global search finds every mention of a
//! > person across all cases; purge removes an entity and its properties,
//! > with the audit log retaining **only the fact that a deletion
//! > occurred**.
//!
//! The two rules meet at redaction rather than deletion. A purge strips the
//! personal data out of the payload columns and leaves everything else — id,
//! timestamp, actor, action, target, batch — exactly as recorded, so the
//! history still proves what happened, when and at whose hand, and nothing
//! is ever removed from it. Append-only holds in the sense that matters;
//! [`redact_case_payloads`] is the single, documented exception, and it must
//! not be reused for anything else.
//!
//! [`record`] takes the write connection the caller is already running a
//! transaction on rather than opening its own, because an audit entry that
//! can commit separately from the change it describes is not an audit trail.
//! See the module docs on [`super::case`] for why the transaction belongs to
//! the caller.

use sqlx::FromRow;
use sqlx::SqliteConnection;
use sqlx::sqlite::SqliteExecutor;

use crate::error::StoreError;
use crate::ids;

/// Maximum rows the list functions return, regardless of what is asked for.
/// AGENT-BRIEF.md rule 5: every list is bounded.
pub const MAX_LIST_LIMIT: i64 = 1_000;

/// Fields a caller supplies when recording a new audit entry. `at` and `id`
/// are assigned by [`record`].
#[derive(Debug, Clone)]
pub struct NewAuditEntry {
    /// `'human'` or an agent name.
    pub actor: String,
    /// `create` | `update` | `delete` | `merge` | `link` | `import` and the
    /// case-lifecycle verbs the service layer adds to that set.
    pub action: String,
    pub case_id: Option<String>,
    /// `entity` | `relation` | `case` | ... — whatever kind of row
    /// `target_id` names.
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

/// Appends an entry and returns it, including its assigned id and
/// timestamp. Runs on the connection it is given, so it lands — or rolls
/// back — with whatever change it describes.
pub async fn record(
    conn: &mut SqliteConnection,
    new: NewAuditEntry,
) -> Result<AuditEntry, StoreError> {
    let at = ids::now();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO audit_log \
         (at, actor, action, case_id, target_kind, target_id, before, after, batch_id) \
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
}

/// Replaces the `before`/`after` payloads of every entry belonging to one
/// case with `marker`, leaving every other column of every entry untouched.
/// Returns how many rows were changed.
///
/// # This is the only code in Vesica that modifies an existing audit entry
///
/// It exists solely to satisfy the erasure obligation in
/// `09-security-and-legal.md` — see this module's docs — and it must not be
/// reused for anything else. There is no general "update an audit row"
/// capability and there must never be one: the whole value of the trail is
/// that nobody, including this program, can rewrite what it says happened.
/// What this function removes is the *contents* of the thing that was
/// erased, never the record that the erasure happened.
///
/// Two details that are the difference between doing this properly and
/// doing it approximately:
///
/// - The scope is the **case**, not just entries whose target *is* the case.
///   A purge cascades through every Entity, Relation and Source inside it,
///   so those entries' payloads describe data that no longer exists either,
///   and the obligation covers them identically.
/// - A column that was already null stays null. Overwriting it with the
///   marker would tell a later reader that something was erased here when in
///   fact nothing was ever recorded — which is precisely the distinction the
///   marker exists to draw.
pub async fn redact_case_payloads(
    conn: &mut SqliteConnection,
    case_id: &str,
    marker: &str,
) -> Result<u64, StoreError> {
    let result = sqlx::query(
        "UPDATE audit_log \
            SET before = CASE WHEN before IS NULL THEN NULL ELSE ? END, \
                after  = CASE WHEN after  IS NULL THEN NULL ELSE ? END \
          WHERE case_id = ? AND (before IS NOT NULL OR after IS NOT NULL)",
    )
    .bind(marker)
    .bind(marker)
    .bind(case_id)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected())
}

/// Replaces the `before`/`after` payloads of every entry about one target
/// with `marker`, leaving every other column of every entry untouched.
/// Returns how many rows were changed.
///
/// # The second, narrower half of the one exception
///
/// This is [`redact_case_payloads`] scoped to a single row rather than a
/// whole case, and it exists for exactly the same obligation:
/// `09-security-and-legal.md` says "purge removes an entity and its
/// properties, with the audit log retaining only the fact that a deletion
/// occurred". Purging one Entity therefore owes its data subject the same
/// erasure a Case purge owes, and reusing the case-scoped function for it
/// would erase the payloads of every *other* entity in the case as well —
/// destroying history nobody asked to erase in the name of erasing history
/// somebody did.
///
/// Everything the case-scoped function's documentation says applies here
/// verbatim: no entry is deleted, no column other than `before`/`after` is
/// touched, a column that was already null stays null, and this must not be
/// reused for anything that is not an erasure. Between them these two
/// functions are the whole of Vesica's ability to alter an audit entry, and
/// there must never be a third.
pub async fn redact_target_payloads(
    conn: &mut SqliteConnection,
    target_kind: &str,
    target_id: &str,
    marker: &str,
) -> Result<u64, StoreError> {
    let result = sqlx::query(
        "UPDATE audit_log \
            SET before = CASE WHEN before IS NULL THEN NULL ELSE ? END, \
                after  = CASE WHEN after  IS NULL THEN NULL ELSE ? END \
          WHERE target_kind = ? AND target_id = ? \
            AND (before IS NOT NULL OR after IS NOT NULL)",
    )
    .bind(marker)
    .bind(marker)
    .bind(target_kind)
    .bind(target_id)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected())
}

/// The most recent entries for one target, newest first, bounded by
/// [`MAX_LIST_LIMIT`] regardless of `limit`.
pub async fn list_for_target<'e, E>(
    exec: E,
    target_kind: &str,
    target_id: &str,
    limit: i64,
) -> Result<Vec<AuditEntry>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let entries = sqlx::query_as::<_, AuditEntry>(
        "SELECT id, at, actor, action, case_id, target_kind, target_id, before, after, batch_id \
         FROM audit_log WHERE target_kind = ? AND target_id = ? \
         ORDER BY at DESC, id DESC LIMIT ?",
    )
    .bind(target_kind)
    .bind(target_id)
    .bind(limit.clamp(0, MAX_LIST_LIMIT))
    .fetch_all(exec)
    .await?;
    Ok(entries)
}

/// The `batch_id` of the most recent entry recording `action` against one
/// target, if that entry carried one.
///
/// This is how a cascade is undone. Deleting an Entity soft-deletes every
/// Relation touching it, and every one of those changes is recorded under
/// the same `batch_id` as the Entity's own delete entry — which is exactly
/// what `04-data-model.md` says a batch is for: "Groups related changes (an
/// import, a merge) for undo". Restoring the Entity then asks this function
/// which batch took it down and [`targets_in_batch`] what else went with
/// it, so that a Relation deleted independently *before* the Entity — a
/// different batch, or none — stays deleted.
///
/// Reading the trail to drive a restore does not weaken its append-only
/// rule: nothing here writes.
pub async fn latest_batch_for<'e, E>(
    exec: E,
    target_kind: &str,
    target_id: &str,
    action: &str,
) -> Result<Option<String>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let batch: Option<Option<String>> = sqlx::query_scalar(
        "SELECT batch_id FROM audit_log \
         WHERE target_kind = ? AND target_id = ? AND action = ? \
         ORDER BY at DESC, id DESC LIMIT 1",
    )
    .bind(target_kind)
    .bind(target_id)
    .bind(action)
    .fetch_optional(exec)
    .await?;
    Ok(batch.flatten())
}

/// The ids of every target of one kind that one batch recorded `action`
/// against, oldest first, bounded by [`MAX_LIST_LIMIT`].
///
/// See [`latest_batch_for`] for what this is for.
pub async fn targets_in_batch<'e, E>(
    exec: E,
    batch_id: &str,
    target_kind: &str,
    action: &str,
) -> Result<Vec<String>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT target_id FROM audit_log \
         WHERE batch_id = ? AND target_kind = ? AND action = ? \
         ORDER BY id ASC LIMIT ?",
    )
    .bind(batch_id)
    .bind(target_kind)
    .bind(action)
    .bind(MAX_LIST_LIMIT)
    .fetch_all(exec)
    .await?;
    Ok(ids)
}

/// The most recent entries touching anything in one case, newest first,
/// bounded by [`MAX_LIST_LIMIT`] regardless of `limit`. Entries whose
/// `case_id` is null (targets that belong to no case) are not included.
pub async fn list_for_case<'e, E>(
    exec: E,
    case_id: &str,
    limit: i64,
) -> Result<Vec<AuditEntry>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let entries = sqlx::query_as::<_, AuditEntry>(
        "SELECT id, at, actor, action, case_id, target_kind, target_id, before, after, batch_id \
         FROM audit_log WHERE case_id = ? ORDER BY at DESC, id DESC LIMIT ?",
    )
    .bind(case_id)
    .bind(limit.clamp(0, MAX_LIST_LIMIT))
    .fetch_all(exec)
    .await?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::test_store;

    fn entry(action: &str) -> NewAuditEntry {
        NewAuditEntry {
            actor: "human".into(),
            action: action.into(),
            case_id: Some("case-1".into()),
            target_kind: "entity".into(),
            target_id: "entity-1".into(),
            before: None,
            after: None,
            batch_id: None,
        }
    }

    async fn record_one(db: &crate::Db, new: NewAuditEntry) -> AuditEntry {
        db.transaction(move |conn| Box::pin(async move { record(conn, new).await }))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn record_then_list_for_target_returns_newest_first() {
        let store = test_store().await.unwrap();
        for action in ["create", "update", "update"] {
            record_one(&store.db, entry(action)).await;
        }

        let entries = list_for_target(store.db.read_pool(), "entity", "entity-1", 10)
            .await
            .unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].id >= entries[1].id);
        assert!(entries[1].id >= entries[2].id);
    }

    #[tokio::test]
    async fn list_for_case_finds_entries_across_target_kinds() {
        let store = test_store().await.unwrap();
        record_one(&store.db, entry("create")).await;
        let mut other_kind = entry("update");
        other_kind.target_kind = "case".into();
        other_kind.target_id = "case-1".into();
        record_one(&store.db, other_kind).await;
        let mut other_case = entry("create");
        other_case.case_id = Some("case-2".into());
        record_one(&store.db, other_case).await;

        let entries = list_for_case(store.db.read_pool(), "case-1", 10)
            .await
            .unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn the_lists_are_bounded_even_when_a_larger_limit_is_requested() {
        let store = test_store().await.unwrap();
        for _ in 0..5 {
            record_one(&store.db, entry("create")).await;
        }

        let entries = list_for_target(store.db.read_pool(), "entity", "entity-1", 2)
            .await
            .unwrap();
        assert_eq!(entries.len(), 2);
        let entries = list_for_case(store.db.read_pool(), "case-1", 2)
            .await
            .unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn a_batch_names_everything_one_change_took_down_and_nothing_else() {
        let store = test_store().await.unwrap();

        // An earlier, independent deletion of one relation.
        let mut alone = entry("delete");
        alone.target_kind = "relation".into();
        alone.target_id = "rel-alone".into();
        alone.batch_id = Some("batch-earlier".into());
        record_one(&store.db, alone).await;

        // Then the entity goes, taking two relations with it.
        let mut entity = entry("delete");
        entity.batch_id = Some("batch-cascade".into());
        record_one(&store.db, entity).await;
        for id in ["rel-a", "rel-b"] {
            let mut cascaded = entry("delete");
            cascaded.target_kind = "relation".into();
            cascaded.target_id = id.into();
            cascaded.batch_id = Some("batch-cascade".into());
            record_one(&store.db, cascaded).await;
        }

        let batch = latest_batch_for(store.db.read_pool(), "entity", "entity-1", "delete")
            .await
            .unwrap();
        assert_eq!(batch.as_deref(), Some("batch-cascade"));

        let targets = targets_in_batch(store.db.read_pool(), "batch-cascade", "relation", "delete")
            .await
            .unwrap();
        assert_eq!(targets, vec!["rel-a".to_string(), "rel-b".to_string()]);

        // An entry with no batch, and an action nobody recorded, both come
        // back empty rather than guessing.
        record_one(&store.db, entry("update")).await;
        assert_eq!(
            latest_batch_for(store.db.read_pool(), "entity", "entity-1", "update")
                .await
                .unwrap(),
            None
        );
        assert!(
            targets_in_batch(store.db.read_pool(), "batch-cascade", "relation", "restore")
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// An entry with a payload, so redaction has something to remove.
    fn entry_with_payload(action: &str) -> NewAuditEntry {
        let mut e = entry(action);
        e.before = Some(r#"{"name":"Jan Kowalski"}"#.into());
        e.after = Some(r#"{"name":"Jan Nowak"}"#.into());
        e
    }

    #[tokio::test]
    async fn redaction_removes_payloads_and_nothing_else() {
        let store = test_store().await.unwrap();
        let with_payload = record_one(&store.db, entry_with_payload("update")).await;
        let without_payload = record_one(&store.db, entry("create")).await;

        let changed = store
            .db
            .transaction(|conn| {
                Box::pin(async move { redact_case_payloads(conn, "case-1", "REDACTED").await })
            })
            .await
            .unwrap();
        assert_eq!(
            changed, 1,
            "only the entry that had a payload was rewritten"
        );

        let entries = list_for_case(store.db.read_pool(), "case-1", 10)
            .await
            .unwrap();
        let redacted = entries
            .iter()
            .find(|e| e.id == with_payload.id)
            .expect("the entry itself must still be there");
        assert_eq!(redacted.before.as_deref(), Some("REDACTED"));
        assert_eq!(redacted.after.as_deref(), Some("REDACTED"));
        // Everything that proves what happened survives verbatim.
        assert_eq!(redacted.at, with_payload.at);
        assert_eq!(redacted.actor, with_payload.actor);
        assert_eq!(redacted.action, with_payload.action);
        assert_eq!(redacted.target_kind, with_payload.target_kind);
        assert_eq!(redacted.target_id, with_payload.target_id);
        assert_eq!(redacted.batch_id, with_payload.batch_id);

        let untouched = entries.iter().find(|e| e.id == without_payload.id).unwrap();
        assert_eq!(
            (untouched.before.as_deref(), untouched.after.as_deref()),
            (None, None),
            "a column that never held anything must not claim to have been erased"
        );
    }

    #[tokio::test]
    async fn redaction_touches_only_the_case_it_was_asked_about() {
        let store = test_store().await.unwrap();
        let mine = record_one(&store.db, entry_with_payload("update")).await;
        let mut other = entry_with_payload("update");
        other.case_id = Some("case-2".into());
        let theirs = record_one(&store.db, other).await;

        store
            .db
            .transaction(|conn| {
                Box::pin(async move { redact_case_payloads(conn, "case-1", "REDACTED").await })
            })
            .await
            .unwrap();

        let ours = list_for_case(store.db.read_pool(), "case-1", 10)
            .await
            .unwrap();
        assert_eq!(ours[0].before.as_deref(), Some("REDACTED"));

        let others = list_for_case(store.db.read_pool(), "case-2", 10)
            .await
            .unwrap();
        assert_eq!(
            others[0].before, theirs.before,
            "another case's trail is none of this purge's business"
        );
        assert_eq!(others[0].after, theirs.after);
        assert_ne!(ours[0].id, others[0].id);
        assert_eq!(mine.case_id.as_deref(), Some("case-1"));
    }

    #[tokio::test]
    async fn a_rolled_back_redaction_leaves_the_payloads_alone() {
        // Redaction happens inside the purge's transaction, so a purge that
        // fails after redacting must leave the trail exactly as it was —
        // otherwise a failed erasure would destroy evidence without
        // destroying the data it was meant to erase.
        let store = test_store().await.unwrap();
        let original = record_one(&store.db, entry_with_payload("update")).await;

        let outcome: Result<u64, StoreError> = store
            .db
            .transaction(|conn| {
                Box::pin(async move {
                    redact_case_payloads(conn, "case-1", "REDACTED").await?;
                    Err(StoreError::Migration("induced failure".into()))
                })
            })
            .await;
        assert!(outcome.is_err());

        let entries = list_for_case(store.db.read_pool(), "case-1", 10)
            .await
            .unwrap();
        assert_eq!(entries[0].before, original.before);
        assert_eq!(entries[0].after, original.after);
    }

    #[tokio::test]
    async fn an_entry_recorded_in_a_rolled_back_transaction_does_not_survive() {
        // The guarantee the service layer's audit trail rests on: the entry
        // commits with the transaction it was written in, or not at all.
        let store = test_store().await.unwrap();
        let outcome: Result<(), StoreError> = store
            .db
            .transaction(|conn| {
                Box::pin(async move {
                    record(conn, entry("create")).await?;
                    Err(StoreError::Migration("induced failure".into()))
                })
            })
            .await;
        assert!(outcome.is_err());

        let entries = list_for_target(store.db.read_pool(), "entity", "entity-1", 10)
            .await
            .unwrap();
        assert!(entries.is_empty(), "the rolled-back entry must be gone");
    }
}
