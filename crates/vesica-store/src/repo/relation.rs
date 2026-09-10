//! Persistence for `relation` and `relation_prop` — 04-data-model.md 4.2.1.
//!
//! The source-entity column is `source_id_` (trailing underscore) in the
//! schema so it cannot be confused with a reference to the `source`
//! (evidence) table; the Rust field stays `source_id`. `relation_prop` has
//! no date columns at all: a Relation's validity interval lives on the
//! `relation` row itself (`date_start` / `date_end` / `date_prec`), because
//! it belongs to the edge rather than to any one of its values.
//!
//! Free functions generic over a `sqlx` executor, not methods on a
//! repository struct, for the reason [`super::case`] and [`super::entity`]
//! spell out: the service layer has to run a read, the write it implies and
//! that write's `audit_log` row inside **one** transaction, and a `&self`
//! method that opens its own transaction cannot be composed into a larger
//! one. Ticket 0001-08 converted `case` and `audit`, 0001-09 converted
//! `entity`, and ticket 0001-10 converted this module for the same reason —
//! which is also what let `relation_prop` join it, since a Relation and its
//! Properties are written together or not at all.
//!
//! Nothing here filters `deleted_at` except [`list`] and [`touching`]: soft
//! deletion is a service-layer rule (restore has to find a row every other
//! read hides), and a store that silently hid rows would make restore
//! impossible to express.
//!
//! `relation_prop.source_id` belongs to [`super::source`] on exactly the
//! terms [`super::entity`] describes: the column is a derived cache of the
//! earliest citation on that value, so [`insert_prop`] still leaves it
//! null, and this module's part of the bargain is deleting a value's
//! citations when the value goes — [`delete_prop`], [`delete_props_named`]
//! and [`purge`] all do, in the same transaction.

use sqlx::FromRow;
use sqlx::SqliteConnection;
use sqlx::sqlite::SqliteExecutor;

use crate::error::StoreError;

/// Largest number of rows [`list`] and [`touching`] return, whatever they
/// are asked for. AGENT-BRIEF.md rule 5: every list is bounded.
pub const MAX_LIST_LIMIT: i64 = 1_000;

/// Largest number of `relation_prop` rows [`props`] returns for one
/// relation. A Property may repeat, so this is a row budget rather than a
/// Property count.
pub const MAX_PROPS_PER_RELATION: i64 = 1_000;

/// A `relation` row exactly as stored, including `deleted_at`.
///
/// This is a row, not a domain type: `confidence` is the raw string the
/// column holds, and the interval is three loose columns rather than a
/// parsed date. Turning either into something meaningful is the service
/// layer's job.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct RelationRow {
    pub id: String,
    pub case_id: String,
    pub schema_id: String,
    /// The source endpoint. Stored in `source_id_`; see the module docs.
    #[sqlx(rename = "source_id_")]
    pub source_id: String,
    pub target_id: String,
    /// Whether the edge reads one way. An undirected edge asserts no
    /// direction, and the service layer takes this from the relation
    /// Schema.
    pub directed: bool,
    /// Lower bound of the validity interval, ISO-8601 UTC. Null for a
    /// Relation with no interval, or one that is only known to have ended.
    pub date_start: Option<String>,
    /// Upper bound of the validity interval, ISO-8601 UTC. Null for an
    /// interval that is open at the far end — a directorship still held.
    pub date_end: Option<String>,
    /// `year` | `month` | `day` | `instant` — how much of the interval's
    /// bounds was actually known.
    pub date_prec: Option<String>,
    /// `confirmed` | `probable` | `hypothesis` — validated by the service
    /// layer, not here.
    pub confidence: String,
    /// A caption override. Null means the caption is derived from the
    /// Schema, which is the normal case for an edge.
    pub caption: Option<String>,
    pub created_at: String,
    pub created_by: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

/// A `relation_prop` row exactly as stored.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct RelationPropRow {
    pub id: i64,
    pub relation_id: String,
    /// Property name within the relation Schema.
    pub prop: String,
    /// Denormalised property type, as [`vesica_core::PropertyType::as_str`]
    /// spells it.
    pub ptype: String,
    /// Order of this value within its Property, from zero.
    pub seq: i64,
    /// Exactly as entered. Never rewritten.
    pub value: String,
    pub value_norm: Option<String>,
    pub value_num: Option<f64>,
    pub source_id: Option<String>,
}

/// A Property value about to be written. `id` is assigned by SQLite and
/// `source_id` is not settable here — see the module docs.
#[derive(Debug, Clone, PartialEq)]
pub struct NewRelationProp {
    pub relation_id: String,
    pub prop: String,
    pub ptype: String,
    pub seq: i64,
    pub value: String,
    pub value_norm: Option<String>,
    pub value_num: Option<f64>,
}

/// Which end of an edge a caller means when it asks for the Relations of
/// one Entity.
///
/// An **undirected** Relation matches all three, from either end: it
/// asserts no direction, so which column its endpoints happen to sit in is
/// an accident of how it was entered and must not decide whether a query
/// finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointRole {
    /// The Entity is the source of a directed Relation.
    Source,
    /// The Entity is the target of a directed Relation.
    Target,
    /// Either end. The usual ask.
    Either,
}

/// Which rows [`list`] returns. Soft-deleted Relations are never included,
/// whatever is asked for.
#[derive(Debug, Clone)]
pub struct RelationFilter {
    /// The case to list within. Relations belong to exactly one case, so
    /// this is not optional.
    pub case_id: String,
    /// `Some("Ownership")` to list one Schema only.
    pub schema_id: Option<String>,
    /// `Some("confirmed")` to list one confidence level only.
    pub confidence: Option<String>,
    /// `Some(false)` to list only undirected Relations, which is what makes
    /// them distinguishable from directed ones in a listing.
    pub directed: Option<bool>,
    /// `Some(entity_id)` to list only Relations touching that Entity.
    pub endpoint: Option<String>,
    /// Which end `endpoint` has to be. Ignored when `endpoint` is `None`.
    pub role: EndpointRole,
    /// Row budget, clamped to [`MAX_LIST_LIMIT`].
    pub limit: i64,
    /// Rows to skip, for paging.
    pub offset: i64,
}

impl RelationFilter {
    /// Every live Relation in one case, up to [`MAX_LIST_LIMIT`].
    pub fn for_case(case_id: impl Into<String>) -> Self {
        RelationFilter {
            case_id: case_id.into(),
            schema_id: None,
            confidence: None,
            directed: None,
            endpoint: None,
            role: EndpointRole::Either,
            limit: MAX_LIST_LIMIT,
            offset: 0,
        }
    }
}

/// The Relation with this id, whatever state it is in — live or
/// soft-deleted. `None` if no such row exists.
pub async fn find<'e, E>(exec: E, id: &str) -> Result<Option<RelationRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, RelationRow>(
        "SELECT id, case_id, schema_id, source_id_, target_id, directed, \
                date_start, date_end, date_prec, confidence, caption, \
                created_at, created_by, updated_at, deleted_at \
         FROM relation WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

/// Live Relations in one case, newest first, bounded by [`MAX_LIST_LIMIT`]
/// however large a `limit` is asked for.
///
/// Every optional filter is a bound parameter compared against itself
/// (`? IS NULL OR schema_id = ?`) rather than a `WHERE` clause assembled
/// from strings — AGENT-BRIEF.md rule 4. The endpoint clause is the same
/// idea with the role decided in Rust and bound as two flags, so that
/// "either end", "the source end" and "the target end" are one statement
/// rather than three.
pub async fn list<'e, E>(exec: E, filter: &RelationFilter) -> Result<Vec<RelationRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let (want_source, want_target) = match filter.role {
        EndpointRole::Source => (true, false),
        EndpointRole::Target => (false, true),
        EndpointRole::Either => (true, true),
    };

    let rows = sqlx::query_as::<_, RelationRow>(
        "SELECT id, case_id, schema_id, source_id_, target_id, directed, \
                date_start, date_end, date_prec, confidence, caption, \
                created_at, created_by, updated_at, deleted_at \
         FROM relation \
         WHERE case_id = ? AND deleted_at IS NULL \
           AND (? IS NULL OR schema_id = ?) \
           AND (? IS NULL OR confidence = ?) \
           AND (? IS NULL OR directed = ?) \
           AND (? IS NULL \
                OR (? = 1 AND source_id_ = ?) \
                OR (? = 1 AND target_id = ?) \
                OR (directed = 0 AND (source_id_ = ? OR target_id = ?))) \
         ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
    )
    .bind(&filter.case_id)
    .bind(&filter.schema_id)
    .bind(&filter.schema_id)
    .bind(&filter.confidence)
    .bind(&filter.confidence)
    .bind(filter.directed)
    .bind(filter.directed)
    .bind(&filter.endpoint)
    .bind(want_source)
    .bind(&filter.endpoint)
    .bind(want_target)
    .bind(&filter.endpoint)
    .bind(&filter.endpoint)
    .bind(&filter.endpoint)
    .bind(filter.limit.clamp(0, MAX_LIST_LIMIT))
    .bind(filter.offset.max(0))
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// Live Relations with this Entity at either end, oldest id first, bounded
/// by [`MAX_LIST_LIMIT`].
///
/// This is the cascade's read: deleting an Entity has to take every edge
/// touching it with it, and the ordering is stable so that a caller working
/// through a large fan-out page by page — soft-deleting as it goes, which
/// removes rows from this result — makes progress on every pass.
pub async fn touching<'e, E>(exec: E, entity_id: &str) -> Result<Vec<RelationRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RelationRow>(
        "SELECT id, case_id, schema_id, source_id_, target_id, directed, \
                date_start, date_end, date_prec, confidence, caption, \
                created_at, created_by, updated_at, deleted_at \
         FROM relation \
         WHERE deleted_at IS NULL AND (source_id_ = ? OR target_id = ?) \
         ORDER BY id ASC LIMIT ?",
    )
    .bind(entity_id)
    .bind(entity_id)
    .bind(MAX_LIST_LIMIT)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// Inserts a Relation exactly as given. Takes the write connection rather
/// than any executor so it cannot be run against the reader pool by
/// accident.
pub async fn insert(conn: &mut SqliteConnection, row: &RelationRow) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO relation \
         (id, case_id, schema_id, source_id_, target_id, directed, date_start, date_end, \
          date_prec, confidence, caption, created_at, created_by, updated_at, deleted_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.case_id)
    .bind(&row.schema_id)
    .bind(&row.source_id)
    .bind(&row.target_id)
    .bind(row.directed)
    .bind(&row.date_start)
    .bind(&row.date_end)
    .bind(&row.date_prec)
    .bind(&row.confidence)
    .bind(&row.caption)
    .bind(&row.created_at)
    .bind(&row.created_by)
    .bind(&row.updated_at)
    .bind(&row.deleted_at)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Overwrites every mutable column of the Relation with `row.id` from
/// `row`.
///
/// One statement covers recaption, confidence change, a new interval, soft
/// delete and restore, because all of those are "the service computed the
/// new state of this row; store it". `id`, `case_id`, `schema_id`,
/// `source_id_`, `target_id`, `created_at` and `created_by` are absent from
/// the `SET` list: an edge that changes which two things it joins is a
/// different edge, so they are immutable by construction rather than by
/// convention. Returns whether a row was there.
pub async fn save(conn: &mut SqliteConnection, row: &RelationRow) -> Result<bool, StoreError> {
    let result = sqlx::query(
        "UPDATE relation SET directed = ?, date_start = ?, date_end = ?, date_prec = ?, \
         confidence = ?, caption = ?, updated_at = ?, deleted_at = ? WHERE id = ?",
    )
    .bind(row.directed)
    .bind(&row.date_start)
    .bind(&row.date_end)
    .bind(&row.date_prec)
    .bind(&row.confidence)
    .bind(&row.caption)
    .bind(&row.updated_at)
    .bind(&row.deleted_at)
    .bind(&row.id)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Deletes the row outright. This is the purge path and it is irreversible:
/// every `relation_prop` row goes with it via `ON DELETE CASCADE`. Returns
/// whether a row was there to delete.
pub async fn purge(conn: &mut SqliteConnection, id: &str) -> Result<bool, StoreError> {
    // Citations first: no foreign key can cascade an untyped `target_id`,
    // and the statements that find them read rows this delete removes.
    super::source::purge_citations_of_relation(&mut *conn, id).await?;
    let result = sqlx::query("DELETE FROM relation WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Every stored value of one Relation, ordered by Property name then by the
/// order the values were entered in. Bounded by
/// [`MAX_PROPS_PER_RELATION`].
pub async fn props<'e, E>(exec: E, relation_id: &str) -> Result<Vec<RelationPropRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RelationPropRow>(
        "SELECT id, relation_id, prop, ptype, seq, value, value_norm, value_num, source_id \
         FROM relation_prop WHERE relation_id = ? ORDER BY prop ASC, seq ASC, id ASC LIMIT ?",
    )
    .bind(relation_id)
    .bind(MAX_PROPS_PER_RELATION)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// Appends one Property value and returns its assigned row id.
pub async fn insert_prop(
    conn: &mut SqliteConnection,
    new: &NewRelationProp,
) -> Result<i64, StoreError> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO relation_prop \
         (relation_id, prop, ptype, seq, value, value_norm, value_num) \
         VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&new.relation_id)
    .bind(&new.prop)
    .bind(&new.ptype)
    .bind(new.seq)
    .bind(&new.value)
    .bind(&new.value_norm)
    .bind(new.value_num)
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// Removes every value of one named Property of one Relation. Returns how
/// many rows went.
pub async fn delete_props_named(
    conn: &mut SqliteConnection,
    relation_id: &str,
    prop: &str,
) -> Result<u64, StoreError> {
    super::source::delete_named_prop_citations(
        &mut *conn,
        super::source::PropHost::Relation,
        relation_id,
        prop,
    )
    .await?;
    let result = sqlx::query("DELETE FROM relation_prop WHERE relation_id = ? AND prop = ?")
        .bind(relation_id)
        .bind(prop)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected())
}

/// Removes one specific Property value, by the row id [`props`] reported.
/// Returns whether a row was there.
pub async fn delete_prop(conn: &mut SqliteConnection, id: i64) -> Result<bool, StoreError> {
    super::source::delete_prop_citations(&mut *conn, super::source::PropHost::Relation, id).await?;
    let result = sqlx::query("DELETE FROM relation_prop WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids;
    use crate::testing::{seed_case, seed_entity, test_store};

    fn row(id: &str, case_id: &str, source: &str, target: &str) -> RelationRow {
        RelationRow {
            id: id.into(),
            case_id: case_id.into(),
            schema_id: "Ownership".into(),
            source_id: source.into(),
            target_id: target.into(),
            directed: true,
            date_start: None,
            date_end: None,
            date_prec: None,
            confidence: "probable".into(),
            caption: None,
            created_at: ids::now(),
            created_by: "human".into(),
            updated_at: ids::now(),
            deleted_at: None,
        }
    }

    async fn insert_one(db: &crate::Db, row: RelationRow) {
        db.transaction(move |conn| Box::pin(async move { insert(conn, &row).await }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn insert_then_find_round_trips_every_column() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let a = seed_entity(&store.db, &case.id, "A").await.unwrap();
        let b = seed_entity(&store.db, &case.id, "B").await.unwrap();

        let mut r = row("rel-1", &case.id, &a.id, &b.id);
        r.date_start = Some("2019-01-01T00:00:00.000Z".into());
        r.date_end = Some("2021-12-31T23:59:59.999Z".into());
        r.date_prec = Some("year".into());
        r.caption = Some("the shareholding".into());
        insert_one(&store.db, r.clone()).await;

        assert_eq!(find(store.db.read_pool(), "rel-1").await.unwrap(), Some(r));
        assert_eq!(find(store.db.read_pool(), "nope").await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_relation_to_a_missing_entity_is_rejected() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let a = seed_entity(&store.db, &case.id, "A").await.unwrap();

        let r = row("rel-1", &case.id, &a.id, "does-not-exist");
        let result = store
            .db
            .transaction(move |conn| Box::pin(async move { insert(conn, &r).await }))
            .await;
        assert!(result.is_err(), "the foreign key must be enforced");
    }

    #[tokio::test]
    async fn list_hides_deleted_rows_and_honours_every_filter() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let a = seed_entity(&store.db, &case.id, "A").await.unwrap();
        let b = seed_entity(&store.db, &case.id, "B").await.unwrap();
        let c = seed_entity(&store.db, &case.id, "C").await.unwrap();

        insert_one(&store.db, row("rel-ab", &case.id, &a.id, &b.id)).await;

        let mut undirected = row("rel-bc", &case.id, &b.id, &c.id);
        undirected.schema_id = "Associate".into();
        undirected.directed = false;
        undirected.confidence = "confirmed".into();
        insert_one(&store.db, undirected).await;

        let mut gone = row("rel-gone", &case.id, &a.id, &c.id);
        gone.deleted_at = Some(ids::now());
        insert_one(&store.db, gone).await;

        let all = list(store.db.read_pool(), &RelationFilter::for_case(&case.id))
            .await
            .unwrap();
        assert_eq!(all.len(), 2, "the deleted row must not appear");

        let owned = list(
            store.db.read_pool(),
            &RelationFilter {
                schema_id: Some("Ownership".into()),
                ..RelationFilter::for_case(&case.id)
            },
        )
        .await
        .unwrap();
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].id, "rel-ab");

        let symmetric = list(
            store.db.read_pool(),
            &RelationFilter {
                directed: Some(false),
                ..RelationFilter::for_case(&case.id)
            },
        )
        .await
        .unwrap();
        assert_eq!(symmetric.len(), 1);
        assert_eq!(symmetric[0].id, "rel-bc");

        let confirmed = list(
            store.db.read_pool(),
            &RelationFilter {
                confidence: Some("confirmed".into()),
                ..RelationFilter::for_case(&case.id)
            },
        )
        .await
        .unwrap();
        assert_eq!(confirmed.len(), 1);
    }

    #[tokio::test]
    async fn an_undirected_relation_is_found_from_either_end_whatever_role_is_asked_for() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let a = seed_entity(&store.db, &case.id, "A").await.unwrap();
        let b = seed_entity(&store.db, &case.id, "B").await.unwrap();

        insert_one(&store.db, row("rel-directed", &case.id, &a.id, &b.id)).await;
        let mut undirected = row("rel-symmetric", &case.id, &a.id, &b.id);
        undirected.directed = false;
        insert_one(&store.db, undirected).await;

        let by_role = |entity: String, role: EndpointRole| RelationFilter {
            endpoint: Some(entity),
            role,
            ..RelationFilter::for_case(&case.id)
        };

        // The directed edge is found at the end it actually has.
        let as_source = list(
            store.db.read_pool(),
            &by_role(a.id.clone(), EndpointRole::Source),
        )
        .await
        .unwrap();
        assert_eq!(as_source.len(), 2, "both edges start at A");

        let as_target = list(
            store.db.read_pool(),
            &by_role(a.id.clone(), EndpointRole::Target),
        )
        .await
        .unwrap();
        assert_eq!(
            as_target.len(),
            1,
            "only the undirected edge answers to a role it does not have"
        );
        assert_eq!(as_target[0].id, "rel-symmetric");

        let either = list(
            store.db.read_pool(),
            &by_role(b.id.clone(), EndpointRole::Either),
        )
        .await
        .unwrap();
        assert_eq!(either.len(), 2);
    }

    #[tokio::test]
    async fn list_is_bounded_even_when_a_larger_limit_is_asked_for() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let a = seed_entity(&store.db, &case.id, "A").await.unwrap();
        let b = seed_entity(&store.db, &case.id, "B").await.unwrap();
        for i in 0..3 {
            insert_one(&store.db, row(&format!("rel-{i}"), &case.id, &a.id, &b.id)).await;
        }
        let rows = list(
            store.db.read_pool(),
            &RelationFilter {
                limit: i64::MAX,
                ..RelationFilter::for_case(&case.id)
            },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 3, "clamping a huge limit must not error");
    }

    #[tokio::test]
    async fn properties_keep_their_order_and_go_with_their_relation() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let a = seed_entity(&store.db, &case.id, "A").await.unwrap();
        let b = seed_entity(&store.db, &case.id, "B").await.unwrap();
        insert_one(&store.db, row("rel-1", &case.id, &a.id, &b.id)).await;

        for (seq, value) in ["51", "49"].into_iter().enumerate() {
            let new = NewRelationProp {
                relation_id: "rel-1".into(),
                prop: "percentage".into(),
                ptype: "number".into(),
                seq: seq as i64,
                value: value.into(),
                value_norm: Some(value.into()),
                value_num: Some(value.parse().unwrap()),
            };
            store
                .db
                .transaction(move |conn| Box::pin(async move { insert_prop(conn, &new).await }))
                .await
                .unwrap();
        }

        let stored = props(store.db.read_pool(), "rel-1").await.unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].value, "51");
        assert_eq!(stored[1].value, "49");
        assert!(
            stored.iter().all(|p| p.source_id.is_none()),
            "source attachment is a later ticket and must stay untouched"
        );

        store
            .db
            .transaction(|conn| Box::pin(async move { purge(conn, "rel-1").await }))
            .await
            .unwrap();
        assert!(
            props(store.db.read_pool(), "rel-1")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn touching_finds_live_edges_at_either_end_and_purging_an_entity_takes_them() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let a = seed_entity(&store.db, &case.id, "A").await.unwrap();
        let b = seed_entity(&store.db, &case.id, "B").await.unwrap();
        let c = seed_entity(&store.db, &case.id, "C").await.unwrap();

        insert_one(&store.db, row("rel-ab", &case.id, &a.id, &b.id)).await;
        insert_one(&store.db, row("rel-ca", &case.id, &c.id, &a.id)).await;
        let mut gone = row("rel-gone", &case.id, &a.id, &c.id);
        gone.deleted_at = Some(ids::now());
        insert_one(&store.db, gone).await;

        let live = touching(store.db.read_pool(), &a.id).await.unwrap();
        assert_eq!(live.len(), 2, "either end counts, deleted rows do not");

        let entity_id = a.id.clone();
        store
            .db
            .transaction(move |conn| {
                Box::pin(async move { super::super::entity::purge(conn, &entity_id).await })
            })
            .await
            .unwrap();
        assert_eq!(
            find(store.db.read_pool(), "rel-ab").await.unwrap(),
            None,
            "the foreign key cascade takes the edge with the entity"
        );
    }

    #[tokio::test]
    async fn save_and_purge_report_whether_a_row_was_there() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let a = seed_entity(&store.db, &case.id, "A").await.unwrap();
        let b = seed_entity(&store.db, &case.id, "B").await.unwrap();
        insert_one(&store.db, row("rel-1", &case.id, &a.id, &b.id)).await;

        let mut changed = row("rel-1", &case.id, &a.id, &b.id);
        changed.confidence = "confirmed".into();
        changed.deleted_at = Some("2026-01-01T00:00:00.000Z".into());
        let updated = store
            .db
            .transaction(move |conn| Box::pin(async move { save(conn, &changed).await }))
            .await
            .unwrap();
        assert!(updated);

        let stored = find(store.db.read_pool(), "rel-1").await.unwrap().unwrap();
        assert_eq!(stored.confidence, "confirmed");
        assert!(stored.deleted_at.is_some());

        let ghost = row("gone", &case.id, &a.id, &b.id);
        let missing = store
            .db
            .transaction(move |conn| Box::pin(async move { save(conn, &ghost).await }))
            .await
            .unwrap();
        assert!(!missing, "saving a row that isn't there reports false");

        assert!(
            store
                .db
                .transaction(|conn| Box::pin(async move { purge(conn, "rel-1").await }))
                .await
                .unwrap()
        );
        assert!(
            !store
                .db
                .transaction(|conn| Box::pin(async move { purge(conn, "rel-1").await }))
                .await
                .unwrap()
        );
    }
}
