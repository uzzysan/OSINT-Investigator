//! Persistence for `kase` — 04-data-model.md 4.2.1. The table is spelled
//! `kase` because `case` is a reserved word in SQL; the Rust names stay
//! `Case`/`CaseRow` since Rust has no such restriction.
//!
//! Everything here is a free function generic over a `sqlx` executor rather
//! than a method on a repository struct, because the service layer has to
//! run a read, the write it implies and the write's `audit_log` row inside
//! **one** transaction (ticket 0001-08: "every mutation writes an audit
//! entry ... in the same transaction as the change"). A `&self` method that
//! opens its own transaction cannot be composed into a larger one, so the
//! transaction is owned by the caller — see [`crate::store::Db::transaction`]
//! — and these functions accept whatever executor it is running on. The same
//! function therefore serves a pooled read (`db.read_pool()`) and a read
//! inside a transaction (`&mut *conn`) with no duplicated SQL.
//!
//! No function here filters `deleted_at` except [`list`]: soft deletion is a
//! service-layer rule (restore has to find a row that every other read must
//! hide), and a store that silently hid rows would make restore impossible
//! to express.

use sqlx::FromRow;
use sqlx::SqliteConnection;
use sqlx::sqlite::SqliteExecutor;

use crate::error::StoreError;

/// Largest number of rows [`list`] will return, whatever it is asked for.
/// AGENT-BRIEF.md rule 5: every list is bounded.
pub const MAX_LIST_LIMIT: i64 = 1_000;

/// A `kase` row exactly as stored, including `deleted_at`. This is a row,
/// not a domain type: `status` is the raw string the column holds, and
/// turning it into something meaningful is the service layer's job.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct CaseRow {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub status: String,
    pub default_region: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

/// Which rows [`list`] returns. Soft-deleted rows are never included; that
/// is not a filter option because "list the deleted ones" is not a use case
/// anything in the spec has.
#[derive(Debug, Clone)]
pub struct CaseFilter {
    /// `Some("active")`, `Some("archived")`, or `None` for both.
    pub status: Option<String>,
    /// Row budget, clamped to [`MAX_LIST_LIMIT`].
    pub limit: i64,
    /// Rows to skip, for paging.
    pub offset: i64,
}

impl Default for CaseFilter {
    fn default() -> Self {
        CaseFilter {
            status: None,
            limit: MAX_LIST_LIMIT,
            offset: 0,
        }
    }
}

/// The case with this id, soft-deleted or not. `None` if no such row exists.
pub async fn find<'e, E>(exec: E, id: &str) -> Result<Option<CaseRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, CaseRow>(
        "SELECT id, slug, name, description, color, status, default_region, \
                created_at, updated_at, deleted_at \
         FROM kase WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

/// The case with this slug, soft-deleted or not. `None` if no such row
/// exists.
pub async fn find_by_slug<'e, E>(exec: E, slug: &str) -> Result<Option<CaseRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, CaseRow>(
        "SELECT id, slug, name, description, color, status, default_region, \
                created_at, updated_at, deleted_at \
         FROM kase WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

/// Live (not soft-deleted) cases, newest first, bounded by
/// [`MAX_LIST_LIMIT`] however large a `limit` is asked for.
///
/// The optional status filter is a bound parameter compared against itself
/// (`? IS NULL OR status = ?`) rather than a `WHERE` clause assembled from
/// strings — AGENT-BRIEF.md rule 4.
pub async fn list<'e, E>(exec: E, filter: &CaseFilter) -> Result<Vec<CaseRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, CaseRow>(
        "SELECT id, slug, name, description, color, status, default_region, \
                created_at, updated_at, deleted_at \
         FROM kase \
         WHERE deleted_at IS NULL AND (? IS NULL OR status = ?) \
         ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
    )
    .bind(&filter.status)
    .bind(&filter.status)
    .bind(filter.limit.clamp(0, MAX_LIST_LIMIT))
    .bind(filter.offset.max(0))
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// Whether any case — live, archived or soft-deleted — already holds this
/// slug. Soft-deleted rows count: `kase.slug` is `UNIQUE` across the whole
/// table, so a slug a deleted case still holds is not available.
pub async fn slug_is_taken<'e, E>(exec: E, slug: &str) -> Result<bool, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM kase WHERE slug = ? LIMIT 1")
        .bind(slug)
        .fetch_optional(exec)
        .await?;
    Ok(found.is_some())
}

/// Inserts a case exactly as given. Takes the write connection rather than
/// any executor so it cannot be run against the reader pool by accident.
pub async fn insert(conn: &mut SqliteConnection, row: &CaseRow) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO kase \
         (id, slug, name, description, color, status, default_region, created_at, updated_at, deleted_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.slug)
    .bind(&row.name)
    .bind(&row.description)
    .bind(&row.color)
    .bind(&row.status)
    .bind(&row.default_region)
    .bind(&row.created_at)
    .bind(&row.updated_at)
    .bind(&row.deleted_at)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Overwrites every mutable column of the case with `row.id` from `row`.
///
/// One statement covers rename, recolour, region change, archive, soft
/// delete and restore, because all of those are "the service computed the
/// new state of this row; store it". `id` and `created_at` are absent from
/// the `SET` list: they are immutable by construction, not by convention.
/// Returns whether a row was actually there to update.
pub async fn save(conn: &mut SqliteConnection, row: &CaseRow) -> Result<bool, StoreError> {
    let result = sqlx::query(
        "UPDATE kase SET slug = ?, name = ?, description = ?, color = ?, status = ?, \
         default_region = ?, updated_at = ?, deleted_at = ? WHERE id = ?",
    )
    .bind(&row.slug)
    .bind(&row.name)
    .bind(&row.description)
    .bind(&row.color)
    .bind(&row.status)
    .bind(&row.default_region)
    .bind(&row.updated_at)
    .bind(&row.deleted_at)
    .bind(&row.id)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Deletes the row outright. This is the purge path and it is irreversible:
/// every `entity`, `relation`, `source`, `tag` and `saved_view` that
/// references the case goes with it via `ON DELETE CASCADE`, which is why
/// `foreign_keys = ON` on every connection is load-bearing rather than
/// hygiene. Returns whether a row was there to delete.
pub async fn purge(conn: &mut SqliteConnection, id: &str) -> Result<bool, StoreError> {
    let result = sqlx::query("DELETE FROM kase WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::test_store;

    fn row(id: &str, slug: &str, name: &str) -> CaseRow {
        CaseRow {
            id: id.into(),
            slug: slug.into(),
            name: name.into(),
            description: None,
            color: None,
            status: "active".into(),
            default_region: None,
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-01-01T00:00:00.000Z".into(),
            deleted_at: None,
        }
    }

    async fn insert_one(db: &crate::Db, row: CaseRow) {
        db.transaction(move |conn| Box::pin(async move { insert(conn, &row).await }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn insert_then_find_round_trips_every_column() {
        let store = test_store().await.unwrap();
        let mut r = row("01ARZ3NDEKTSV4RRFFQ69G5FAV", "op-lighthouse", "Lighthouse");
        r.description = Some("a description".into());
        r.color = Some("#3b82f6".into());
        r.default_region = Some("PL".into());
        insert_one(&store.db, r.clone()).await;

        assert_eq!(find(store.db.read_pool(), &r.id).await.unwrap(), Some(r));
    }

    #[tokio::test]
    async fn find_of_a_missing_id_is_none_not_an_error() {
        let store = test_store().await.unwrap();
        assert_eq!(find(store.db.read_pool(), "nope").await.unwrap(), None);
    }

    #[tokio::test]
    async fn list_excludes_soft_deleted_rows_and_honours_the_status_filter() {
        let store = test_store().await.unwrap();
        insert_one(&store.db, row("id-a", "a", "A")).await;

        let mut archived = row("id-b", "b", "B");
        archived.status = "archived".into();
        insert_one(&store.db, archived).await;

        let mut deleted = row("id-c", "c", "C");
        deleted.deleted_at = Some("2026-01-02T00:00:00.000Z".into());
        insert_one(&store.db, deleted).await;

        let all = list(store.db.read_pool(), &CaseFilter::default())
            .await
            .unwrap();
        assert_eq!(all.len(), 2, "the soft-deleted row must not appear");

        let active = list(
            store.db.read_pool(),
            &CaseFilter {
                status: Some("active".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "id-a");
    }

    #[tokio::test]
    async fn list_is_bounded_even_when_a_larger_limit_is_asked_for() {
        let store = test_store().await.unwrap();
        for i in 0..3 {
            insert_one(&store.db, row(&format!("id-{i}"), &format!("s-{i}"), "X")).await;
        }
        let rows = list(
            store.db.read_pool(),
            &CaseFilter {
                limit: i64::MAX,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 3, "clamping a huge limit must not error");
    }

    #[tokio::test]
    async fn slug_is_taken_counts_soft_deleted_rows_too() {
        let store = test_store().await.unwrap();
        let mut deleted = row("id-a", "taken", "A");
        deleted.deleted_at = Some("2026-01-02T00:00:00.000Z".into());
        insert_one(&store.db, deleted).await;

        assert!(slug_is_taken(store.db.read_pool(), "taken").await.unwrap());
        assert!(!slug_is_taken(store.db.read_pool(), "free").await.unwrap());
    }

    #[tokio::test]
    async fn purging_a_case_takes_everything_inside_it() {
        // The service layer's purge relies on `ON DELETE CASCADE` doing this
        // rather than deleting children itself, so the cascade is worth
        // proving here: it is SQL behaviour, and it only holds while
        // `foreign_keys = ON` is set on the write connection.
        use crate::repo::{EntityRepo, NewEntity, SqliteEntityRepo};

        let store = test_store().await.unwrap();
        let case = crate::testing::seed_case(&store.db, "op-lighthouse")
            .await
            .unwrap();
        let entities = SqliteEntityRepo::new(store.db.clone());
        let entity = entities
            .create(NewEntity {
                case_id: case.id.clone(),
                schema_id: "Person".into(),
                caption: "Jan Kowalski".into(),
                confidence: "probable".into(),
                created_by: "human".into(),
            })
            .await
            .unwrap();
        assert!(entities.get(&entity.id).await.unwrap().is_some());

        let id = case.id.clone();
        store
            .db
            .transaction(move |conn| Box::pin(async move { purge(conn, &id).await }))
            .await
            .unwrap();

        assert_eq!(find(store.db.read_pool(), &case.id).await.unwrap(), None);
        assert_eq!(
            entities.get(&entity.id).await.unwrap(),
            None,
            "the entity must go with its case"
        );
    }

    #[tokio::test]
    async fn save_and_purge_report_whether_a_row_was_there() {
        let store = test_store().await.unwrap();
        insert_one(&store.db, row("id-a", "a", "A")).await;

        let mut renamed = row("id-a", "a", "Renamed");
        renamed.status = "archived".into();
        let updated = store
            .db
            .transaction(move |conn| Box::pin(async move { save(conn, &renamed).await }))
            .await
            .unwrap();
        assert!(updated);

        let stored = find(store.db.read_pool(), "id-a").await.unwrap().unwrap();
        assert_eq!(stored.name, "Renamed");
        assert_eq!(stored.status, "archived");

        let missing = store
            .db
            .transaction(|conn| Box::pin(async move { save(conn, &row("gone", "g", "G")).await }))
            .await
            .unwrap();
        assert!(!missing, "saving a row that isn't there reports false");

        assert!(
            store
                .db
                .transaction(|conn| Box::pin(async move { purge(conn, "id-a").await }))
                .await
                .unwrap()
        );
        assert_eq!(find(store.db.read_pool(), "id-a").await.unwrap(), None);
        assert!(
            !store
                .db
                .transaction(|conn| Box::pin(async move { purge(conn, "id-a").await }))
                .await
                .unwrap()
        );
    }
}
