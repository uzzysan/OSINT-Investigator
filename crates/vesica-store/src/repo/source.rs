//! Persistence for `source` and `citation` — 04-data-model.md 4.2.2
//! (evidence: where a fact came from, and what it is offered as evidence
//! for).
//!
//! Free functions generic over a `sqlx` executor, not methods on a
//! repository struct, for the reason [`super::case`], [`super::entity`] and
//! [`super::relation`] spell out: the service layer has to run a read, the
//! write it implies and that write's `audit_log` row inside **one**
//! transaction, and a `&self` method that opens its own transaction cannot
//! be composed into a larger one. Ticket 0001-08 converted `case` and
//! `audit`, 0001-09 `entity`, 0001-10 `relation`, and ticket 0001-11
//! converted this module — the last repository trait in the codebase — for
//! the same reason. `citation` lives here rather than in a module of its
//! own because a citation has no life apart from its source: it is created
//! with one, cascades with one, and every question worth asking about a
//! source is really a question about what cites it.
//!
//! # How a citation names a property value
//!
//! `citation.target_kind` is one of `entity`, `relation`, `prop` or
//! `observation` (04-data-model.md). For `entity` and `relation` the
//! `target_id` is that row's ULID and there is nothing to decide.
//!
//! For `prop` there is: `entity_prop` and `relation_prop` are separate
//! tables with separate `INTEGER PRIMARY KEY` sequences, so `5` names a row
//! in each and a bare row id would be ambiguous. The `target_id` of a
//! property citation is therefore `"<host>:<row id>"` — `entity:412`,
//! `relation:77` — built by [`prop_target_id`] and read back by
//! [`parse_prop_target_id`]. The alternative, splitting `prop` into two
//! `target_kind` values, would have put a vocabulary in the database that
//! `04-data-model.md` does not describe.
//!
//! Nothing here ever assembles SQL from those strings: the host prefix is a
//! **bound parameter** concatenated to the row id by SQLite (`? || p.id`),
//! so AGENT-BRIEF.md rule 4 holds even where a query has to match a
//! composite key.
//!
//! # `entity_prop.source_id` is a cache, `citation` is the truth
//!
//! `entity_prop.source_id` and `relation_prop.source_id` hold exactly one
//! source per value; `citation` can hold many. They are reconciled by
//! making the column **derived**: it always holds the source of that
//! value's *earliest surviving citation*, or null when the value has none.
//! [`refresh_prop_source`] is the only function that computes it and every
//! path that changes a property value's citations calls it in the same
//! transaction. That is what lets "which values are unsourced" be an
//! indexed `source_id IS NULL` rather than a correlated subquery over every
//! value in a case, while leaving `citation` free to record as many claims
//! about one value as an analyst wants to make.
//!
//! # Dangling citations
//!
//! `citation.target_id` is polymorphic text, so no foreign key can point it
//! at `entity_prop`, and SQLite recycles `INTEGER PRIMARY KEY` values: a
//! citation left behind by a deleted property value could later be read as
//! a citation of an unrelated new one. Every path that destroys a citable
//! row therefore deletes that row's citations explicitly, in the same
//! transaction — [`delete_prop_citations`], [`delete_named_prop_citations`],
//! [`purge_citations_of_entity`] and [`purge_citations_of_relation`], the
//! last two called from [`super::entity::purge`] and
//! [`super::relation::purge`]. A source's own deletion needs no such help:
//! `citation.source_id` is a real foreign key and cascades.

use sqlx::FromRow;
use sqlx::SqliteConnection;
use sqlx::sqlite::SqliteExecutor;

use crate::error::StoreError;

/// Largest number of rows any list function here returns, whatever it is
/// asked for. AGENT-BRIEF.md rule 5: every list is bounded.
pub const MAX_LIST_LIMIT: i64 = 1_000;

/// Largest number of rows a coverage count will look at before giving up
/// and saying so.
///
/// Counting sourced against unsourced facts is the one question here whose
/// natural implementation is "read the whole case", which is precisely the
/// shape a caller can weaponise. [`coverage`] instead counts inside a
/// bounded subquery and reports [`CoverageRow::capped`] when it hit this
/// ceiling, so the answer is either exact or honestly labelled as a floor.
pub const MAX_COUNT_SCAN: i64 = 10_000;

/// `citation.target_kind` for an entity.
pub const TARGET_ENTITY: &str = "entity";
/// `citation.target_kind` for a relation.
pub const TARGET_RELATION: &str = "relation";
/// `citation.target_kind` for one value of one property.
pub const TARGET_PROP: &str = "prop";

/// Which table a property value lives in — the half of a property
/// citation's target that its row id alone cannot say. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropHost {
    /// The value belongs to an entity: `entity_prop`.
    Entity,
    /// The value belongs to a relation: `relation_prop`.
    Relation,
}

impl PropHost {
    /// The prefix this host contributes to a property citation's
    /// `target_id`, without its separator.
    pub fn as_str(self) -> &'static str {
        match self {
            PropHost::Entity => TARGET_ENTITY,
            PropHost::Relation => TARGET_RELATION,
        }
    }

    /// The literal `"<host>:"` prefix, which is what the queries bind.
    fn prefix(self) -> String {
        format!("{}:", self.as_str())
    }
}

/// The `citation.target_id` naming one property value.
pub fn prop_target_id(host: PropHost, prop_row_id: i64) -> String {
    format!("{}:{}", host.as_str(), prop_row_id)
}

/// Reads a `target_id` written by [`prop_target_id`] back into its parts.
/// `None` if it is not of that shape, which for a `prop` citation means the
/// database was written around this module.
pub fn parse_prop_target_id(target_id: &str) -> Option<(PropHost, i64)> {
    let (host, id) = target_id.split_once(':')?;
    let host = match host {
        TARGET_ENTITY => PropHost::Entity,
        TARGET_RELATION => PropHost::Relation,
        _ => return None,
    };
    Some((host, id.parse().ok()?))
}

/// A `source` row exactly as stored.
///
/// This is a row, not a domain type: `kind` is the raw string the column
/// holds and `meta` is unparsed JSON. Both mean something only to the
/// service layer.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct SourceRow {
    pub id: String,
    pub case_id: String,
    /// `url` | `file` | `manual` | `agent` — validated by the service
    /// layer, not here.
    pub kind: String,
    pub title: Option<String>,
    /// The locator exactly as it was given. Never rewritten.
    pub url: Option<String>,
    /// The normalised locator, which is what makes the same page cited
    /// twice one source rather than two.
    pub url_norm: Option<String>,
    /// A quotation from the source, in the source's own words.
    pub excerpt: Option<String>,
    /// When the source was observed, ISO-8601 UTC. Deliberately distinct
    /// from `created_at`, which is when the row was typed.
    pub retrieved_at: Option<String>,
    pub created_at: String,
    pub created_by: String,
    /// JSON. Holds a file source's reference; otherwise null.
    pub meta: Option<String>,
}

/// A `citation` row exactly as stored.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct CitationRow {
    pub id: i64,
    pub source_id: String,
    /// `entity` | `relation` | `prop` | `observation`.
    pub target_kind: String,
    /// The cited row: a ULID, or `"<host>:<row id>"` for a property value.
    pub target_id: String,
    pub note: Option<String>,
    pub created_at: String,
}

/// A citation about to be written. `id` and `created_at` are assigned by
/// [`insert_citation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCitation {
    pub source_id: String,
    pub target_kind: String,
    pub target_id: String,
    pub note: Option<String>,
}

/// A citation read back with the cited property value resolved, where there
/// is one to resolve.
///
/// Every citation read path returns this one shape so that the service
/// layer has a single mapping to write. For an `entity` or `relation`
/// citation the three trailing fields are null; for a `prop` citation they
/// name the value, which is what lets a source listing say *which* phone
/// number it was cited for rather than just "a property value".
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct CitationDetailRow {
    pub id: i64,
    pub source_id: String,
    pub target_kind: String,
    pub target_id: String,
    pub note: Option<String>,
    pub created_at: String,
    /// The entity or relation the cited value belongs to.
    pub host_id: Option<String>,
    /// The property the cited value belongs to.
    pub prop: Option<String>,
    /// The cited value itself.
    pub value: Option<String>,
}

/// One property value, looked up by the row id inside a citation target.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct PropRef {
    pub id: i64,
    /// The entity or relation it belongs to.
    pub host_id: String,
    pub prop: String,
    pub value: String,
}

/// One property value with no source, for the unsourced-facts report.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct UnsourcedPropRow {
    pub id: i64,
    /// The entity or relation the value belongs to.
    pub host_id: String,
    pub prop: String,
    pub seq: i64,
    pub value: String,
}

/// A sourced-versus-unsourced count over one kind of fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
pub struct CoverageRow {
    /// How many of the rows looked at had at least one source.
    pub sourced: i64,
    /// How many rows were looked at in total.
    pub total: i64,
    /// Whether the count stopped at [`MAX_COUNT_SCAN`] — in which case
    /// both numbers are floors, not totals.
    pub capped: bool,
}

/// Which sources [`list`] returns.
#[derive(Debug, Clone)]
pub struct SourceFilter {
    /// The case to list within. Sources belong to exactly one case, so this
    /// is not optional.
    pub case_id: String,
    /// `Some("url")` to list one kind only.
    pub kind: Option<String>,
    /// Row budget, clamped to [`MAX_LIST_LIMIT`].
    pub limit: i64,
    /// Rows to skip, for paging.
    pub offset: i64,
}

impl SourceFilter {
    /// Every source in one case, up to [`MAX_LIST_LIMIT`].
    pub fn for_case(case_id: impl Into<String>) -> Self {
        SourceFilter {
            case_id: case_id.into(),
            kind: None,
            limit: MAX_LIST_LIMIT,
            offset: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

/// The source with this id. Sources have no `deleted_at`: deleting one is a
/// real delete, because a source that is not there is exactly what "this
/// fact is unsourced again" means.
pub async fn find<'e, E>(exec: E, id: &str) -> Result<Option<SourceRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, SourceRow>(
        "SELECT id, case_id, kind, title, url, url_norm, excerpt, retrieved_at, \
                created_at, created_by, meta \
         FROM source WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

/// The source in one case whose normalised locator is `url_norm`, if there
/// is one. This is what keeps the same page cited twice from becoming two
/// sources.
pub async fn find_by_url_norm<'e, E>(
    exec: E,
    case_id: &str,
    url_norm: &str,
) -> Result<Option<SourceRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, SourceRow>(
        "SELECT id, case_id, kind, title, url, url_norm, excerpt, retrieved_at, \
                created_at, created_by, meta \
         FROM source WHERE case_id = ? AND url_norm = ? \
         ORDER BY created_at ASC, id ASC LIMIT 1",
    )
    .bind(case_id)
    .bind(url_norm)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

/// Sources in one case, newest first, bounded by [`MAX_LIST_LIMIT`] however
/// large a `limit` is asked for. The optional filter is a bound parameter
/// compared against itself rather than a `WHERE` clause assembled from
/// strings — AGENT-BRIEF.md rule 4.
pub async fn list<'e, E>(exec: E, filter: &SourceFilter) -> Result<Vec<SourceRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, SourceRow>(
        "SELECT id, case_id, kind, title, url, url_norm, excerpt, retrieved_at, \
                created_at, created_by, meta \
         FROM source WHERE case_id = ? AND (? IS NULL OR kind = ?) \
         ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
    )
    .bind(&filter.case_id)
    .bind(&filter.kind)
    .bind(&filter.kind)
    .bind(filter.limit.clamp(0, MAX_LIST_LIMIT))
    .bind(filter.offset.max(0))
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// Inserts a source exactly as given. Takes the write connection rather
/// than any executor so it cannot be run against the reader pool by
/// accident.
pub async fn insert(conn: &mut SqliteConnection, row: &SourceRow) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO source \
         (id, case_id, kind, title, url, url_norm, excerpt, retrieved_at, \
          created_at, created_by, meta) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.case_id)
    .bind(&row.kind)
    .bind(&row.title)
    .bind(&row.url)
    .bind(&row.url_norm)
    .bind(&row.excerpt)
    .bind(&row.retrieved_at)
    .bind(&row.created_at)
    .bind(&row.created_by)
    .bind(&row.meta)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Deletes a source. Its citations go with it by `ON DELETE CASCADE` and
/// the facts it was cited for are left exactly as they are — which is the
/// whole point: a fact whose source is withdrawn is still a fact, it is
/// just unsourced again.
///
/// Call [`release_props_of_source`] **first**: it is what moves any
/// property value whose cached source is this one onto its next surviving
/// citation, and it cannot be done afterwards because the citations it
/// reads will have cascaded away. Returns whether a row was there.
pub async fn delete(conn: &mut SqliteConnection, id: &str) -> Result<bool, StoreError> {
    let result = sqlx::query("DELETE FROM source WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

// ---------------------------------------------------------------------------
// Citations
// ---------------------------------------------------------------------------

/// Appends a citation and returns it, including its assigned id and
/// timestamp.
pub async fn insert_citation(
    conn: &mut SqliteConnection,
    new: &NewCitation,
) -> Result<CitationRow, StoreError> {
    let created_at = crate::ids::now();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO citation (source_id, target_kind, target_id, note, created_at) \
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&new.source_id)
    .bind(&new.target_kind)
    .bind(&new.target_id)
    .bind(&new.note)
    .bind(&created_at)
    .fetch_one(&mut *conn)
    .await?;

    Ok(CitationRow {
        id,
        source_id: new.source_id.clone(),
        target_kind: new.target_kind.clone(),
        target_id: new.target_id.clone(),
        note: new.note.clone(),
        created_at,
    })
}

/// The citation with this id.
pub async fn find_citation<'e, E>(exec: E, id: i64) -> Result<Option<CitationRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, CitationRow>(
        "SELECT id, source_id, target_kind, target_id, note, created_at \
         FROM citation WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

/// Removes one citation. Returns whether a row was there.
pub async fn delete_citation(conn: &mut SqliteConnection, id: i64) -> Result<bool, StoreError> {
    let result = sqlx::query("DELETE FROM citation WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Whether one source is already cited for one target — so that citing the
/// same source for the same fact twice is a no-op rather than a second
/// claim.
pub async fn find_citation_of<'e, E>(
    exec: E,
    source_id: &str,
    target_kind: &str,
    target_id: &str,
) -> Result<Option<CitationRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, CitationRow>(
        "SELECT id, source_id, target_kind, target_id, note, created_at \
         FROM citation WHERE source_id = ? AND target_kind = ? AND target_id = ? \
         ORDER BY id ASC LIMIT 1",
    )
    .bind(source_id)
    .bind(target_kind)
    .bind(target_id)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

/// Every citation of one target, oldest first, bounded. Used for `entity`
/// and `relation` targets, whose ids need no resolving, so the value fields
/// come back null.
pub async fn citations_for_target<'e, E>(
    exec: E,
    target_kind: &str,
    target_id: &str,
    limit: i64,
) -> Result<Vec<CitationDetailRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, CitationDetailRow>(
        "SELECT id, source_id, target_kind, target_id, note, created_at, \
                NULL AS host_id, NULL AS prop, NULL AS value \
         FROM citation WHERE target_kind = ? AND target_id = ? ORDER BY id ASC LIMIT ?",
    )
    .bind(target_kind)
    .bind(target_id)
    .bind(limit.clamp(0, MAX_LIST_LIMIT))
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// One property value by the row id a citation target names, so that a
/// citation request phrased as a `value_ref` can be checked against the
/// value it claims to be about.
pub async fn find_prop<'e, E>(
    exec: E,
    host: PropHost,
    prop_row_id: i64,
) -> Result<Option<PropRef>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let sql = match host {
        PropHost::Entity => {
            "SELECT id, entity_id AS host_id, prop, value FROM entity_prop WHERE id = ?"
        }
        PropHost::Relation => {
            "SELECT id, relation_id AS host_id, prop, value FROM relation_prop WHERE id = ?"
        }
    };
    let row = sqlx::query_as::<_, PropRef>(sql)
        .bind(prop_row_id)
        .fetch_optional(exec)
        .await?;
    Ok(row)
}

/// Everything one source is cited for, oldest first, bounded, with any
/// cited property value resolved in the same query. This is the "with what
/// cites it" half of the source listing.
pub async fn citations_for_source<'e, E>(
    exec: E,
    source_id: &str,
    limit: i64,
) -> Result<Vec<CitationDetailRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, CitationDetailRow>(
        "SELECT c.id, c.source_id, c.target_kind, c.target_id, c.note, c.created_at, \
                COALESCE(ep.entity_id, rp.relation_id) AS host_id, \
                COALESCE(ep.prop, rp.prop) AS prop, \
                COALESCE(ep.value, rp.value) AS value \
         FROM citation c \
         LEFT JOIN entity_prop ep ON c.target_kind = ? AND c.target_id = ? || ep.id \
         LEFT JOIN relation_prop rp ON c.target_kind = ? AND c.target_id = ? || rp.id \
         WHERE c.source_id = ? ORDER BY c.id ASC LIMIT ?",
    )
    .bind(TARGET_PROP)
    .bind(PropHost::Entity.prefix())
    .bind(TARGET_PROP)
    .bind(PropHost::Relation.prefix())
    .bind(source_id)
    .bind(limit.clamp(0, MAX_LIST_LIMIT))
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// How many times one source is cited, counted up to `cap` so that a source
/// cited a great many times cannot turn a listing into a scan.
pub async fn citation_count<'e, E>(exec: E, source_id: &str, cap: i64) -> Result<i64, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (SELECT 1 FROM citation WHERE source_id = ? LIMIT ?)",
    )
    .bind(source_id)
    .bind(cap.clamp(0, MAX_COUNT_SCAN))
    .fetch_one(exec)
    .await?;
    Ok(count)
}

/// Every property-value citation belonging to one entity, with enough of
/// the value to say which one it is. Oldest first within each property,
/// bounded.
pub async fn prop_citations_for_entity<'e, E>(
    exec: E,
    entity_id: &str,
    limit: i64,
) -> Result<Vec<CitationDetailRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, CitationDetailRow>(
        "SELECT c.id, c.source_id, c.target_kind, c.target_id, c.note, c.created_at, \
                p.entity_id AS host_id, p.prop AS prop, p.value AS value \
         FROM citation c JOIN entity_prop p ON c.target_id = ? || p.id \
         WHERE c.target_kind = ? AND p.entity_id = ? \
         ORDER BY p.prop ASC, p.seq ASC, c.id ASC LIMIT ?",
    )
    .bind(PropHost::Entity.prefix())
    .bind(TARGET_PROP)
    .bind(entity_id)
    .bind(limit.clamp(0, MAX_LIST_LIMIT))
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// As [`prop_citations_for_entity`], for a relation's own property values.
pub async fn prop_citations_for_relation<'e, E>(
    exec: E,
    relation_id: &str,
    limit: i64,
) -> Result<Vec<CitationDetailRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, CitationDetailRow>(
        "SELECT c.id, c.source_id, c.target_kind, c.target_id, c.note, c.created_at, \
                p.relation_id AS host_id, p.prop AS prop, p.value AS value \
         FROM citation c JOIN relation_prop p ON c.target_id = ? || p.id \
         WHERE c.target_kind = ? AND p.relation_id = ? \
         ORDER BY p.prop ASC, p.seq ASC, c.id ASC LIMIT ?",
    )
    .bind(PropHost::Relation.prefix())
    .bind(TARGET_PROP)
    .bind(relation_id)
    .bind(limit.clamp(0, MAX_LIST_LIMIT))
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Keeping the cached source on a property value true
// ---------------------------------------------------------------------------

/// Recomputes one property value's cached `source_id` from its citations:
/// the earliest surviving one, or null when it has none.
///
/// Every path that adds or removes a property citation ends here, in the
/// same transaction, which is what makes the column safe to read as "is
/// this value sourced". See the module docs.
pub async fn refresh_prop_source(
    conn: &mut SqliteConnection,
    host: PropHost,
    prop_row_id: i64,
) -> Result<(), StoreError> {
    let target_id = prop_target_id(host, prop_row_id);
    let sql = match host {
        PropHost::Entity => {
            "UPDATE entity_prop SET source_id = \
               (SELECT c.source_id FROM citation c \
                 WHERE c.target_kind = ? AND c.target_id = ? ORDER BY c.id ASC LIMIT 1) \
             WHERE id = ?"
        }
        PropHost::Relation => {
            "UPDATE relation_prop SET source_id = \
               (SELECT c.source_id FROM citation c \
                 WHERE c.target_kind = ? AND c.target_id = ? ORDER BY c.id ASC LIMIT 1) \
             WHERE id = ?"
        }
    };
    sqlx::query(sql)
        .bind(TARGET_PROP)
        .bind(&target_id)
        .bind(prop_row_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Moves every property value whose cached source is `source_id` onto its
/// next surviving citation — or to null when that was its only one — in one
/// statement per property table.
///
/// This is what has to happen **before** a source is deleted. Afterwards is
/// too late: the source's citations cascade with it, so nothing would be
/// left to say which of the value's remaining claims should take over, and
/// a value with two sources would come back unsourced when only one of them
/// was withdrawn.
pub async fn release_props_of_source(
    conn: &mut SqliteConnection,
    source_id: &str,
) -> Result<(), StoreError> {
    for (sql, host) in [
        (
            "UPDATE entity_prop SET source_id = \
               (SELECT c.source_id FROM citation c \
                 WHERE c.target_kind = ? AND c.target_id = ? || entity_prop.id \
                   AND c.source_id <> ? ORDER BY c.id ASC LIMIT 1) \
             WHERE source_id = ?",
            PropHost::Entity,
        ),
        (
            "UPDATE relation_prop SET source_id = \
               (SELECT c.source_id FROM citation c \
                 WHERE c.target_kind = ? AND c.target_id = ? || relation_prop.id \
                   AND c.source_id <> ? ORDER BY c.id ASC LIMIT 1) \
             WHERE source_id = ?",
            PropHost::Relation,
        ),
    ] {
        sqlx::query(sql)
            .bind(TARGET_PROP)
            .bind(host.prefix())
            .bind(source_id)
            .bind(source_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Removing citations whose target is going away
// ---------------------------------------------------------------------------

/// Deletes the citations of one property value, for when the value itself
/// is being deleted. See the module docs on dangling citations.
pub async fn delete_prop_citations(
    conn: &mut SqliteConnection,
    host: PropHost,
    prop_row_id: i64,
) -> Result<u64, StoreError> {
    let result = sqlx::query("DELETE FROM citation WHERE target_kind = ? AND target_id = ?")
        .bind(TARGET_PROP)
        .bind(prop_target_id(host, prop_row_id))
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected())
}

/// Deletes the citations of every value of one named property of one entity
/// or relation, for when all of them are being replaced at once.
pub async fn delete_named_prop_citations(
    conn: &mut SqliteConnection,
    host: PropHost,
    host_id: &str,
    prop: &str,
) -> Result<u64, StoreError> {
    let sql = match host {
        PropHost::Entity => {
            "DELETE FROM citation WHERE target_kind = ? AND target_id IN \
               (SELECT ? || id FROM entity_prop WHERE entity_id = ? AND prop = ?)"
        }
        PropHost::Relation => {
            "DELETE FROM citation WHERE target_kind = ? AND target_id IN \
               (SELECT ? || id FROM relation_prop WHERE relation_id = ? AND prop = ?)"
        }
    };
    let result = sqlx::query(sql)
        .bind(TARGET_PROP)
        .bind(host.prefix())
        .bind(host_id)
        .bind(prop)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected())
}

/// Deletes every citation that a purge of one entity would otherwise leave
/// pointing at nothing: the entity's own, its property values', and those of
/// the relations that go with it by cascade.
///
/// Called from [`super::entity::purge`], before the row goes, because after
/// it the rows these statements read are gone.
pub async fn purge_citations_of_entity(
    conn: &mut SqliteConnection,
    entity_id: &str,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM citation WHERE target_kind = ? AND target_id = ?")
        .bind(TARGET_ENTITY)
        .bind(entity_id)
        .execute(&mut *conn)
        .await?;

    sqlx::query(
        "DELETE FROM citation WHERE target_kind = ? AND target_id IN \
           (SELECT ? || id FROM entity_prop WHERE entity_id = ?)",
    )
    .bind(TARGET_PROP)
    .bind(PropHost::Entity.prefix())
    .bind(entity_id)
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "DELETE FROM citation WHERE target_kind = ? AND target_id IN \
           (SELECT id FROM relation WHERE source_id_ = ? OR target_id = ?)",
    )
    .bind(TARGET_RELATION)
    .bind(entity_id)
    .bind(entity_id)
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "DELETE FROM citation WHERE target_kind = ? AND target_id IN \
           (SELECT ? || rp.id FROM relation_prop rp JOIN relation r ON r.id = rp.relation_id \
             WHERE r.source_id_ = ? OR r.target_id = ?)",
    )
    .bind(TARGET_PROP)
    .bind(PropHost::Relation.prefix())
    .bind(entity_id)
    .bind(entity_id)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

/// As [`purge_citations_of_entity`], for a relation: its own citations and
/// those of its property values. Called from [`super::relation::purge`].
pub async fn purge_citations_of_relation(
    conn: &mut SqliteConnection,
    relation_id: &str,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM citation WHERE target_kind = ? AND target_id = ?")
        .bind(TARGET_RELATION)
        .bind(relation_id)
        .execute(&mut *conn)
        .await?;

    sqlx::query(
        "DELETE FROM citation WHERE target_kind = ? AND target_id IN \
           (SELECT ? || id FROM relation_prop WHERE relation_id = ?)",
    )
    .bind(TARGET_PROP)
    .bind(PropHost::Relation.prefix())
    .bind(relation_id)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// What is unsourced
// ---------------------------------------------------------------------------

/// Live entities in one case with no citation of their own, newest first,
/// bounded.
pub async fn unsourced_entities<'e, E>(
    exec: E,
    case_id: &str,
    limit: i64,
) -> Result<Vec<String>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT e.id FROM entity e \
         WHERE e.case_id = ? AND e.deleted_at IS NULL AND e.merged_into IS NULL \
           AND NOT EXISTS (SELECT 1 FROM citation c \
                            WHERE c.target_kind = ? AND c.target_id = e.id) \
         ORDER BY e.created_at DESC, e.id DESC LIMIT ?",
    )
    .bind(case_id)
    .bind(TARGET_ENTITY)
    .bind(limit.clamp(0, MAX_LIST_LIMIT))
    .fetch_all(exec)
    .await?;
    Ok(ids)
}

/// Live relations in one case with no citation of their own, newest first,
/// bounded.
pub async fn unsourced_relations<'e, E>(
    exec: E,
    case_id: &str,
    limit: i64,
) -> Result<Vec<String>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT r.id FROM relation r \
         WHERE r.case_id = ? AND r.deleted_at IS NULL \
           AND NOT EXISTS (SELECT 1 FROM citation c \
                            WHERE c.target_kind = ? AND c.target_id = r.id) \
         ORDER BY r.created_at DESC, r.id DESC LIMIT ?",
    )
    .bind(case_id)
    .bind(TARGET_RELATION)
    .bind(limit.clamp(0, MAX_LIST_LIMIT))
    .fetch_all(exec)
    .await?;
    Ok(ids)
}

/// Property values in one case with no source, for one host kind, bounded.
///
/// Reads the cached `source_id` rather than the citation table, which is
/// the whole reason that cache exists — see the module docs.
pub async fn unsourced_props<'e, E>(
    exec: E,
    host: PropHost,
    case_id: &str,
    limit: i64,
) -> Result<Vec<UnsourcedPropRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let sql = match host {
        PropHost::Entity => {
            "SELECT p.id, p.entity_id AS host_id, p.prop, p.seq, p.value \
             FROM entity_prop p JOIN entity e ON e.id = p.entity_id \
             WHERE e.case_id = ? AND e.deleted_at IS NULL AND e.merged_into IS NULL \
               AND p.source_id IS NULL \
             ORDER BY p.entity_id ASC, p.prop ASC, p.seq ASC, p.id ASC LIMIT ?"
        }
        PropHost::Relation => {
            "SELECT p.id, p.relation_id AS host_id, p.prop, p.seq, p.value \
             FROM relation_prop p JOIN relation r ON r.id = p.relation_id \
             WHERE r.case_id = ? AND r.deleted_at IS NULL AND p.source_id IS NULL \
             ORDER BY p.relation_id ASC, p.prop ASC, p.seq ASC, p.id ASC LIMIT ?"
        }
    };
    let rows = sqlx::query_as::<_, UnsourcedPropRow>(sql)
        .bind(case_id)
        .bind(limit.clamp(0, MAX_LIST_LIMIT))
        .fetch_all(exec)
        .await?;
    Ok(rows)
}

/// What proportion of one kind of fact in one case carries a source.
///
/// The count runs inside a subquery bounded by `cap`, so the work is
/// bounded whatever the size of the case; when the ceiling is reached
/// [`CoverageRow::capped`] says so and both numbers are floors. There is no
/// unbounded variant, for the reason AGENT-BRIEF.md rule 5 gives.
pub async fn coverage<'e, E>(
    exec: E,
    kind: CoverageKind,
    case_id: &str,
    cap: i64,
) -> Result<CoverageRow, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let cap = cap.clamp(1, MAX_COUNT_SCAN);
    let sql = match kind {
        CoverageKind::Entities => {
            "SELECT COALESCE(SUM(sourced), 0) AS sourced, COUNT(*) AS total FROM ( \
               SELECT EXISTS (SELECT 1 FROM citation c \
                               WHERE c.target_kind = ? AND c.target_id = e.id) AS sourced \
               FROM entity e \
               WHERE e.case_id = ? AND e.deleted_at IS NULL AND e.merged_into IS NULL \
               LIMIT ?)"
        }
        CoverageKind::Relations => {
            "SELECT COALESCE(SUM(sourced), 0) AS sourced, COUNT(*) AS total FROM ( \
               SELECT EXISTS (SELECT 1 FROM citation c \
                               WHERE c.target_kind = ? AND c.target_id = r.id) AS sourced \
               FROM relation r \
               WHERE r.case_id = ? AND r.deleted_at IS NULL \
               LIMIT ?)"
        }
        CoverageKind::EntityProps => {
            "SELECT COALESCE(SUM(sourced), 0) AS sourced, COUNT(*) AS total FROM ( \
               SELECT (? IS NOT NULL AND p.source_id IS NOT NULL) AS sourced \
               FROM entity_prop p JOIN entity e ON e.id = p.entity_id \
               WHERE e.case_id = ? AND e.deleted_at IS NULL AND e.merged_into IS NULL \
               LIMIT ?)"
        }
        CoverageKind::RelationProps => {
            "SELECT COALESCE(SUM(sourced), 0) AS sourced, COUNT(*) AS total FROM ( \
               SELECT (? IS NOT NULL AND p.source_id IS NOT NULL) AS sourced \
               FROM relation_prop p JOIN relation r ON r.id = p.relation_id \
               WHERE r.case_id = ? AND r.deleted_at IS NULL \
               LIMIT ?)"
        }
    };
    // The first bound value is the citation `target_kind` for the two
    // citation-driven counts, and an ignored non-null placeholder for the
    // two that read the cached column — so that all four statements take
    // the same three parameters in the same order.
    let discriminator = match kind {
        CoverageKind::Entities => TARGET_ENTITY,
        CoverageKind::Relations => TARGET_RELATION,
        CoverageKind::EntityProps | CoverageKind::RelationProps => TARGET_PROP,
    };
    let (sourced, total): (i64, i64) = sqlx::query_as(sql)
        .bind(discriminator)
        .bind(case_id)
        .bind(cap)
        .fetch_one(exec)
        .await?;
    Ok(CoverageRow {
        sourced,
        total,
        capped: total >= cap,
    })
}

/// Which kind of fact [`coverage`] is counting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageKind {
    Entities,
    Relations,
    EntityProps,
    RelationProps,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ids;
    use crate::repo::entity::{self, EntityRow, NewProp};
    use crate::testing::{seed_case, test_store};

    fn source_row(id: &str, case_id: &str, url: Option<&str>) -> SourceRow {
        SourceRow {
            id: id.into(),
            case_id: case_id.into(),
            kind: "url".into(),
            title: Some("Companies register filing".into()),
            url: url.map(str::to_string),
            url_norm: url.map(str::to_string),
            excerpt: None,
            retrieved_at: Some("2024-05-01T09:00:00Z".into()),
            created_at: ids::now(),
            created_by: "agent:vesica-mcp".into(),
            meta: None,
        }
    }

    fn entity_row(id: &str, case_id: &str) -> EntityRow {
        EntityRow {
            id: id.into(),
            case_id: case_id.into(),
            schema_id: "Person".into(),
            caption: "Jan Kowalski".into(),
            confidence: "probable".into(),
            extra: None,
            created_at: ids::now(),
            created_by: "human".into(),
            updated_at: ids::now(),
            deleted_at: None,
            merged_into: None,
        }
    }

    fn prop(entity_id: &str, value: &str) -> NewProp {
        NewProp {
            entity_id: entity_id.into(),
            prop: "phone".into(),
            ptype: "phone".into(),
            seq: 0,
            value: value.into(),
            value_norm: Some(value.into()),
            value_num: None,
            date_start: None,
            date_end: None,
            date_prec: None,
        }
    }

    #[tokio::test]
    async fn a_source_round_trips_and_is_found_again_by_its_normalised_url() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let row = source_row(
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            &case.id,
            Some("https://a.test"),
        );

        let inserted = row.clone();
        store
            .db
            .transaction(move |conn| Box::pin(async move { insert(conn, &inserted).await }))
            .await
            .unwrap();

        assert_eq!(
            find(store.db.read_pool(), &row.id).await.unwrap(),
            Some(row.clone())
        );
        assert_eq!(
            find_by_url_norm(store.db.read_pool(), &case.id, "https://a.test")
                .await
                .unwrap()
                .map(|s| s.id),
            Some(row.id)
        );
        assert!(
            find_by_url_norm(store.db.read_pool(), "another-case", "https://a.test")
                .await
                .unwrap()
                .is_none(),
            "url dedup is scoped to one case"
        );
    }

    #[tokio::test]
    async fn a_property_values_cached_source_follows_its_citations() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let case_id = case.id.clone();

        let prop_id = store
            .db
            .transaction(move |conn| {
                Box::pin(async move {
                    insert(conn, &source_row("src-a", &case_id, Some("https://a.test"))).await?;
                    insert(conn, &source_row("src-b", &case_id, Some("https://b.test"))).await?;
                    entity::insert(conn, &entity_row("ent-1", &case_id)).await?;
                    entity::insert_prop(conn, &prop("ent-1", "+48601234567")).await
                })
            })
            .await
            .unwrap();

        let target = prop_target_id(PropHost::Entity, prop_id);
        for source in ["src-a", "src-b"] {
            let new = NewCitation {
                source_id: source.into(),
                target_kind: TARGET_PROP.into(),
                target_id: target.clone(),
                note: None,
            };
            store
                .db
                .transaction(move |conn| {
                    Box::pin(async move {
                        insert_citation(conn, &new).await?;
                        refresh_prop_source(conn, PropHost::Entity, prop_id).await
                    })
                })
                .await
                .unwrap();
        }

        let cached = |db: crate::Db| async move {
            entity::props(db.read_pool(), "ent-1").await.unwrap()[0]
                .source_id
                .clone()
        };
        assert_eq!(
            cached(store.db.clone()).await.as_deref(),
            Some("src-a"),
            "the cache holds the earliest citation's source"
        );

        // Withdrawing the primary source moves the value onto the other
        // claim rather than making it look unsourced.
        store
            .db
            .transaction(|conn| {
                Box::pin(async move {
                    release_props_of_source(conn, "src-a").await?;
                    delete(conn, "src-a").await
                })
            })
            .await
            .unwrap();
        assert_eq!(cached(store.db.clone()).await.as_deref(), Some("src-b"));

        // Withdrawing the last one leaves the value intact and unsourced.
        store
            .db
            .transaction(|conn| {
                Box::pin(async move {
                    release_props_of_source(conn, "src-b").await?;
                    delete(conn, "src-b").await
                })
            })
            .await
            .unwrap();
        assert_eq!(cached(store.db.clone()).await, None);
        assert_eq!(
            entity::props(store.db.read_pool(), "ent-1").await.unwrap()[0].value,
            "+48601234567",
            "the fact itself survives its source"
        );
    }

    #[tokio::test]
    async fn purging_an_entity_takes_the_citations_of_everything_that_goes_with_it() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let case_id = case.id.clone();

        let prop_id = store
            .db
            .transaction(move |conn| {
                Box::pin(async move {
                    insert(conn, &source_row("src-a", &case_id, Some("https://a.test"))).await?;
                    entity::insert(conn, &entity_row("ent-1", &case_id)).await?;
                    entity::insert_prop(conn, &prop("ent-1", "+48601234567")).await
                })
            })
            .await
            .unwrap();

        let target = prop_target_id(PropHost::Entity, prop_id);
        store
            .db
            .transaction(move |conn| {
                Box::pin(async move {
                    for (kind, id) in [(TARGET_ENTITY, "ent-1".to_string()), (TARGET_PROP, target)]
                    {
                        insert_citation(
                            conn,
                            &NewCitation {
                                source_id: "src-a".into(),
                                target_kind: kind.into(),
                                target_id: id,
                                note: None,
                            },
                        )
                        .await?;
                    }
                    Ok::<_, StoreError>(())
                })
            })
            .await
            .unwrap();

        assert_eq!(
            citation_count(store.db.read_pool(), "src-a", MAX_COUNT_SCAN)
                .await
                .unwrap(),
            2
        );

        store
            .db
            .transaction(|conn| Box::pin(async move { entity::purge(conn, "ent-1").await }))
            .await
            .unwrap();

        assert_eq!(
            citation_count(store.db.read_pool(), "src-a", MAX_COUNT_SCAN)
                .await
                .unwrap(),
            0,
            "a purge must not leave citations pointing at rows that are gone"
        );
    }

    #[tokio::test]
    async fn coverage_counts_are_capped_and_say_so() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let case_id = case.id.clone();

        store
            .db
            .transaction(move |conn| {
                Box::pin(async move {
                    insert(conn, &source_row("src-a", &case_id, Some("https://a.test"))).await?;
                    for i in 0..3 {
                        entity::insert(conn, &entity_row(&format!("ent-{i}"), &case_id)).await?;
                    }
                    insert_citation(
                        conn,
                        &NewCitation {
                            source_id: "src-a".into(),
                            target_kind: TARGET_ENTITY.into(),
                            target_id: "ent-0".into(),
                            note: None,
                        },
                    )
                    .await?;
                    Ok::<_, StoreError>(())
                })
            })
            .await
            .unwrap();

        let full = coverage(
            store.db.read_pool(),
            CoverageKind::Entities,
            &case.id,
            MAX_COUNT_SCAN,
        )
        .await
        .unwrap();
        assert_eq!((full.sourced, full.total, full.capped), (1, 3, false));

        let capped = coverage(store.db.read_pool(), CoverageKind::Entities, &case.id, 2)
            .await
            .unwrap();
        assert_eq!(capped.total, 2);
        assert!(capped.capped, "a count that hit its ceiling must say so");
    }

    #[test]
    fn a_property_citations_target_names_its_table_as_well_as_its_row() {
        assert_eq!(prop_target_id(PropHost::Entity, 412), "entity:412");
        assert_eq!(prop_target_id(PropHost::Relation, 412), "relation:412");
        assert_ne!(
            prop_target_id(PropHost::Entity, 412),
            prop_target_id(PropHost::Relation, 412),
            "the two property tables share a row-id space and must not collide"
        );
        assert_eq!(
            parse_prop_target_id("relation:77"),
            Some((PropHost::Relation, 77))
        );
        assert_eq!(parse_prop_target_id("77"), None);
        assert_eq!(parse_prop_target_id("observation:1"), None);
    }
}
