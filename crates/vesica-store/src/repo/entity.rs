//! Persistence for `entity` and `entity_prop` — 04-data-model.md 4.2.1.
//!
//! Free functions generic over a `sqlx` executor, not methods on a
//! repository struct, for the reason [`super::case`] spells out: the
//! service layer has to run a read, the write it implies and that write's
//! `audit_log` row inside **one** transaction, and a `&self` method that
//! opens its own transaction cannot be composed into a larger one. Ticket
//! 0001-08 converted `case` and `audit`; ticket 0001-09 converted this
//! module for the same reason, which is also what let `entity_prop` join it
//! — an entity and its properties are written together or not at all.
//!
//! Nothing here filters `deleted_at` except [`list`]: soft deletion is a
//! service-layer rule (restore has to find a row every other read hides),
//! and a store that silently hid rows would make restore impossible to
//! express. [`list`] additionally hides entities that were merged away,
//! because a merged-away entity is no longer a node of its own — it still
//! resolves by id through [`find`], which is how an id quoted in an old
//! report keeps working.
//!
//! `entity_prop.source_id` is written by nothing here. Attaching a Source
//! to an individual property value is ticket 0001-11's; until then the
//! column stays null, and [`insert_prop`] deliberately leaves it alone so
//! that a later ticket owns it outright.

use sqlx::FromRow;
use sqlx::SqliteConnection;
use sqlx::sqlite::SqliteExecutor;

use crate::error::StoreError;

/// Largest number of rows [`list`] returns, whatever it is asked for.
/// AGENT-BRIEF.md rule 5: every list is bounded.
pub const MAX_LIST_LIMIT: i64 = 1_000;

/// Largest number of `entity_prop` rows [`props`] returns for one entity.
/// A property may repeat, so this is a row budget rather than a property
/// count; it exists so that a single pathological entity cannot make a read
/// unbounded.
pub const MAX_PROPS_PER_ENTITY: i64 = 1_000;

/// An `entity` row exactly as stored, including `deleted_at` and
/// `merged_into`. This is a row, not a domain type: `confidence` is the raw
/// string the column holds and turning it into something meaningful is the
/// service layer's job.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct EntityRow {
    pub id: String,
    pub case_id: String,
    pub schema_id: String,
    pub caption: String,
    /// `confirmed` | `probable` | `hypothesis` — validated by the service
    /// layer, not here.
    pub confidence: String,
    /// JSON blob for custom or unmapped fields. Untouched by this ticket.
    pub extra: Option<String>,
    pub created_at: String,
    pub created_by: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    /// The surviving entity's id, once this one has been merged away.
    /// Merging itself is a later ticket; the column is read here so that a
    /// merged-away id still resolves.
    pub merged_into: Option<String>,
}

/// An `entity_prop` row exactly as stored.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct PropRow {
    pub id: i64,
    pub entity_id: String,
    /// Property name within the schema.
    pub prop: String,
    /// Denormalised property type, as [`vesica_core::PropertyType::as_str`]
    /// spells it.
    pub ptype: String,
    /// Order of this value within its property, from zero.
    pub seq: i64,
    /// Exactly as entered. Never rewritten.
    pub value: String,
    pub value_norm: Option<String>,
    pub value_num: Option<f64>,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
    pub date_prec: Option<String>,
    pub source_id: Option<String>,
}

/// A property value about to be written. `id` is assigned by SQLite and
/// `source_id` is not settable here — see the module docs.
#[derive(Debug, Clone, PartialEq)]
pub struct NewProp {
    pub entity_id: String,
    pub prop: String,
    pub ptype: String,
    pub seq: i64,
    pub value: String,
    pub value_norm: Option<String>,
    pub value_num: Option<f64>,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
    pub date_prec: Option<String>,
}

/// Which rows [`list`] returns. Soft-deleted and merged-away entities are
/// never included, whatever is asked for.
#[derive(Debug, Clone)]
pub struct EntityFilter {
    /// The case to list within. Entities belong to exactly one case, so
    /// this is not optional.
    pub case_id: String,
    /// `Some("Person")` to list one schema only.
    pub schema_id: Option<String>,
    /// `Some("confirmed")` to list one confidence level only.
    pub confidence: Option<String>,
    /// `Some("suspect")` to list only entities carrying that tag. Matched
    /// case-insensitively, as tag lookup is everywhere else.
    pub tag: Option<String>,
    /// Row budget, clamped to [`MAX_LIST_LIMIT`].
    pub limit: i64,
    /// Rows to skip, for paging.
    pub offset: i64,
}

impl EntityFilter {
    /// Every live entity in one case, up to [`MAX_LIST_LIMIT`].
    pub fn for_case(case_id: impl Into<String>) -> Self {
        EntityFilter {
            case_id: case_id.into(),
            schema_id: None,
            confidence: None,
            tag: None,
            limit: MAX_LIST_LIMIT,
            offset: 0,
        }
    }
}

/// The entity with this id, whatever state it is in — live, soft-deleted or
/// merged away. `None` if no such row exists.
pub async fn find<'e, E>(exec: E, id: &str) -> Result<Option<EntityRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, EntityRow>(
        "SELECT id, case_id, schema_id, caption, confidence, extra, \
                created_at, created_by, updated_at, deleted_at, merged_into \
         FROM entity WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

/// Live entities in one case, newest first, bounded by [`MAX_LIST_LIMIT`]
/// however large a `limit` is asked for.
///
/// Every optional filter is a bound parameter compared against itself
/// (`? IS NULL OR schema_id = ?`) rather than a `WHERE` clause assembled
/// from strings — AGENT-BRIEF.md rule 4.
pub async fn list<'e, E>(exec: E, filter: &EntityFilter) -> Result<Vec<EntityRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, EntityRow>(
        "SELECT id, case_id, schema_id, caption, confidence, extra, \
                created_at, created_by, updated_at, deleted_at, merged_into \
         FROM entity \
         WHERE case_id = ? AND deleted_at IS NULL AND merged_into IS NULL \
           AND (? IS NULL OR schema_id = ?) \
           AND (? IS NULL OR confidence = ?) \
           AND (? IS NULL OR EXISTS ( \
                 SELECT 1 FROM entity_tag et JOIN tag t ON t.id = et.tag_id \
                  WHERE et.entity_id = entity.id AND t.name = ? COLLATE NOCASE)) \
         ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
    )
    .bind(&filter.case_id)
    .bind(&filter.schema_id)
    .bind(&filter.schema_id)
    .bind(&filter.confidence)
    .bind(&filter.confidence)
    .bind(&filter.tag)
    .bind(&filter.tag)
    .bind(filter.limit.clamp(0, MAX_LIST_LIMIT))
    .bind(filter.offset.max(0))
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// Inserts an entity exactly as given. Takes the write connection rather
/// than any executor so it cannot be run against the reader pool by
/// accident.
pub async fn insert(conn: &mut SqliteConnection, row: &EntityRow) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO entity \
         (id, case_id, schema_id, caption, confidence, extra, created_at, created_by, \
          updated_at, deleted_at, merged_into) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.case_id)
    .bind(&row.schema_id)
    .bind(&row.caption)
    .bind(&row.confidence)
    .bind(&row.extra)
    .bind(&row.created_at)
    .bind(&row.created_by)
    .bind(&row.updated_at)
    .bind(&row.deleted_at)
    .bind(&row.merged_into)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Overwrites every mutable column of the entity with `row.id` from `row`.
///
/// One statement covers recaption, confidence change, soft delete, restore
/// and merge, because all of those are "the service computed the new state
/// of this row; store it". `id`, `case_id`, `schema_id`, `created_at` and
/// `created_by` are absent from the `SET` list: they are immutable by
/// construction, not by convention. Returns whether a row was there.
pub async fn save(conn: &mut SqliteConnection, row: &EntityRow) -> Result<bool, StoreError> {
    let result = sqlx::query(
        "UPDATE entity SET caption = ?, confidence = ?, extra = ?, updated_at = ?, \
         deleted_at = ?, merged_into = ? WHERE id = ?",
    )
    .bind(&row.caption)
    .bind(&row.confidence)
    .bind(&row.extra)
    .bind(&row.updated_at)
    .bind(&row.deleted_at)
    .bind(&row.merged_into)
    .bind(&row.id)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Deletes the row outright. This is the purge path and it is irreversible:
/// every `entity_prop`, `entity_tag`, `relation` and timeline row that
/// references the entity goes with it via `ON DELETE CASCADE`, which is why
/// `foreign_keys = ON` on every connection is load-bearing rather than
/// hygiene. Returns whether a row was there to delete.
pub async fn purge(conn: &mut SqliteConnection, id: &str) -> Result<bool, StoreError> {
    let result = sqlx::query("DELETE FROM entity WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Every stored value of one entity, ordered by property name then by the
/// order the values were entered in. Bounded by [`MAX_PROPS_PER_ENTITY`].
pub async fn props<'e, E>(exec: E, entity_id: &str) -> Result<Vec<PropRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, PropRow>(
        "SELECT id, entity_id, prop, ptype, seq, value, value_norm, value_num, \
                date_start, date_end, date_prec, source_id \
         FROM entity_prop WHERE entity_id = ? ORDER BY prop ASC, seq ASC, id ASC LIMIT ?",
    )
    .bind(entity_id)
    .bind(MAX_PROPS_PER_ENTITY)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// Appends one property value and returns its assigned row id.
pub async fn insert_prop(conn: &mut SqliteConnection, new: &NewProp) -> Result<i64, StoreError> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO entity_prop \
         (entity_id, prop, ptype, seq, value, value_norm, value_num, date_start, date_end, date_prec) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&new.entity_id)
    .bind(&new.prop)
    .bind(&new.ptype)
    .bind(new.seq)
    .bind(&new.value)
    .bind(&new.value_norm)
    .bind(new.value_num)
    .bind(&new.date_start)
    .bind(&new.date_end)
    .bind(&new.date_prec)
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// Removes every value of one named property of one entity. Returns how
/// many rows went.
pub async fn delete_props_named(
    conn: &mut SqliteConnection,
    entity_id: &str,
    prop: &str,
) -> Result<u64, StoreError> {
    let result = sqlx::query("DELETE FROM entity_prop WHERE entity_id = ? AND prop = ?")
        .bind(entity_id)
        .bind(prop)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected())
}

/// Removes one specific property value, by the row id [`props`] reported.
/// Returns whether a row was there.
pub async fn delete_prop(conn: &mut SqliteConnection, id: i64) -> Result<bool, StoreError> {
    let result = sqlx::query("DELETE FROM entity_prop WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids;
    use crate::testing::{seed_case, test_store};

    fn row(id: &str, case_id: &str, caption: &str) -> EntityRow {
        EntityRow {
            id: id.into(),
            case_id: case_id.into(),
            schema_id: "Person".into(),
            caption: caption.into(),
            confidence: "probable".into(),
            extra: None,
            created_at: ids::now(),
            created_by: "human".into(),
            updated_at: ids::now(),
            deleted_at: None,
            merged_into: None,
        }
    }

    async fn insert_one(db: &crate::Db, row: EntityRow) {
        db.transaction(move |conn| Box::pin(async move { insert(conn, &row).await }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn insert_then_find_round_trips_every_column() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let mut r = row("01ARZ3NDEKTSV4RRFFQ69G5FAV", &case.id, "Jan Kowalski");
        r.extra = Some(r#"{"legacyId":"7"}"#.into());
        insert_one(&store.db, r.clone()).await;

        assert_eq!(find(store.db.read_pool(), &r.id).await.unwrap(), Some(r));
        assert_eq!(find(store.db.read_pool(), "nope").await.unwrap(), None);
    }

    #[tokio::test]
    async fn an_entity_for_a_missing_case_is_rejected() {
        let store = test_store().await.unwrap();
        let r = row("01ARZ3NDEKTSV4RRFFQ69G5FAV", "does-not-exist", "Ghost");
        let result = store
            .db
            .transaction(move |conn| Box::pin(async move { insert(conn, &r).await }))
            .await;
        assert!(result.is_err(), "the foreign key must be enforced");
    }

    #[tokio::test]
    async fn list_hides_deleted_and_merged_rows_and_honours_its_filters() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();

        insert_one(&store.db, row("id-live", &case.id, "Live")).await;

        let mut company = row("id-company", &case.id, "Acme");
        company.schema_id = "Company".into();
        company.confidence = "confirmed".into();
        insert_one(&store.db, company).await;

        let mut deleted = row("id-deleted", &case.id, "Gone");
        deleted.deleted_at = Some(ids::now());
        insert_one(&store.db, deleted).await;

        let mut merged = row("id-merged", &case.id, "Duplicate");
        merged.merged_into = Some("id-live".into());
        insert_one(&store.db, merged).await;

        let filter = EntityFilter::for_case(&case.id);
        let all = list(store.db.read_pool(), &filter).await.unwrap();
        assert_eq!(all.len(), 2, "deleted and merged rows must not appear");

        let people = list(
            store.db.read_pool(),
            &EntityFilter {
                schema_id: Some("Person".into()),
                ..EntityFilter::for_case(&case.id)
            },
        )
        .await
        .unwrap();
        assert_eq!(people.len(), 1);
        assert_eq!(people[0].id, "id-live");

        let confirmed = list(
            store.db.read_pool(),
            &EntityFilter {
                confidence: Some("confirmed".into()),
                ..EntityFilter::for_case(&case.id)
            },
        )
        .await
        .unwrap();
        assert_eq!(confirmed.len(), 1);
        assert_eq!(confirmed[0].id, "id-company");
    }

    #[tokio::test]
    async fn list_is_bounded_even_when_a_larger_limit_is_asked_for() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        for i in 0..3 {
            insert_one(&store.db, row(&format!("id-{i}"), &case.id, "X")).await;
        }
        let rows = list(
            store.db.read_pool(),
            &EntityFilter {
                limit: i64::MAX,
                ..EntityFilter::for_case(&case.id)
            },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 3, "clamping a huge limit must not error");
    }

    #[tokio::test]
    async fn properties_keep_their_order_and_go_with_their_entity() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        insert_one(&store.db, row("id-live", &case.id, "Jan")).await;

        for (seq, value) in ["+48601234567", "+48601234568"].into_iter().enumerate() {
            let new = NewProp {
                entity_id: "id-live".into(),
                prop: "phone".into(),
                ptype: "phone".into(),
                seq: seq as i64,
                value: value.into(),
                value_norm: Some(value.into()),
                value_num: None,
                date_start: None,
                date_end: None,
                date_prec: None,
            };
            store
                .db
                .transaction(move |conn| Box::pin(async move { insert_prop(conn, &new).await }))
                .await
                .unwrap();
        }

        let stored = props(store.db.read_pool(), "id-live").await.unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].value, "+48601234567");
        assert_eq!(stored[1].value, "+48601234568");
        assert!(
            stored.iter().all(|p| p.source_id.is_none()),
            "source attachment is a later ticket and must stay untouched"
        );

        store
            .db
            .transaction(|conn| Box::pin(async move { purge(conn, "id-live").await }))
            .await
            .unwrap();
        assert!(
            props(store.db.read_pool(), "id-live")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn save_and_purge_report_whether_a_row_was_there() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        insert_one(&store.db, row("id-a", &case.id, "A")).await;

        let mut changed = row("id-a", &case.id, "Renamed");
        changed.confidence = "confirmed".into();
        let updated = store
            .db
            .transaction(move |conn| Box::pin(async move { save(conn, &changed).await }))
            .await
            .unwrap();
        assert!(updated);

        let stored = find(store.db.read_pool(), "id-a").await.unwrap().unwrap();
        assert_eq!(stored.caption, "Renamed");
        assert_eq!(stored.confidence, "confirmed");

        let ghost = row("gone", &case.id, "G");
        let missing = store
            .db
            .transaction(move |conn| Box::pin(async move { save(conn, &ghost).await }))
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
        assert!(
            !store
                .db
                .transaction(|conn| Box::pin(async move { purge(conn, "id-a").await }))
                .await
                .unwrap()
        );
    }
}
