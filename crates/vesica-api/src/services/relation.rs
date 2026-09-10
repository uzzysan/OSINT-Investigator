//! The Relation use cases: connect two Entities, read and list the edges,
//! change them, delete and restore them — plus the audit trail every one of
//! them writes.
//!
//! An Entity records what an investigator found. A Relation records how two
//! findings are connected, which is where a pile of facts becomes a graph.
//!
//! # What a Relation is
//!
//! - **Typed.** The relation Schema decides what the edge means, which
//!   Properties it may carry, whether it has a direction and whether it has
//!   a validity interval. Nothing here hardcodes a Schema id: the registry
//!   is the catalogue and the catalogue is provisional (AGENT-BRIEF.md).
//! - **Directed, or deliberately not.** `directed` comes from the Schema:
//!   `Ownership` reads one way — "owns" and "is owned by" are the same edge
//!   read from two ends — while an association between two people asserts no
//!   direction at all. An undirected Relation is found from either end
//!   whichever end a query names, because which column its endpoints landed
//!   in is an accident of data entry, not a claim.
//! - **Bounded in time.** A Relation may carry a validity interval with an
//!   explicit precision, so a directorship that ended in 2021 is not shown
//!   as current and one known only to have existed "in 2019" says exactly
//!   that. Partial dates are honoured end to end: `2019` is the whole of
//!   2019 and is never widened into 1 January.
//! - **Carrying its own Properties**, normalised on exactly the same code
//!   path as an Entity's — see [`super::entity::PropertyHost`]. A
//!   shareholding records its percentage on the edge rather than in a note.
//! - **Inside one Case.** Both endpoints must belong to the Relation's Case.
//!   Linking across Cases is what entity resolution is for, in a later
//!   milestone; until then it is refused with an error that says so.
//!
//! # Endpoint constraints guide, they do not forbid
//!
//! A relation Schema names the entity Schemas that make sense at each end.
//! `04-data-model.md` section 4.1.3 is explicit that this is **advisory**:
//! the UI offers the sensible combinations, but an explicit override is
//! allowed and flagged, "because investigations routinely encounter things
//! the ontology did not anticipate". So an endpoint the Schema did not
//! expect produces an [`EndpointWarning`], never a refusal.
//!
//! Those warnings are **derived on every read**, not stored, exactly as an
//! Entity's checksum warnings are: there is no column for them, and there
//! should not be. A warning is a judgement made by today's ontology about
//! data that is unchanged — so when the catalogue grows a Schema that makes
//! the combination sensible, the warning stops being reported without a
//! migration and without anybody rewriting history. What *is* recorded
//! permanently is the fact each endpoint's Schema was, which is in the audit
//! snapshot and in the rows themselves.
//!
//! # Deleting an Entity takes its Relations with it
//!
//! The graph never contains an edge to nothing, so
//! [`Services::delete_entity`] cascades into every Relation touching the
//! Entity, and [`Services::restore_entity`] brings back the ones *that*
//! deletion took down — identified by the `batch_id` they share with the
//! Entity's own delete entry, which is what `04-data-model.md` says a batch
//! is for. A Relation deleted on its own beforehand stays deleted, because
//! it belongs to a different batch.

use serde::{Deserialize, Serialize};
use vesica_core::{
    DatePrecision, EndpointSpec, EntitySchema, NormalizeContext, PartialDate, PropertyType,
    PropertyValues, RelationSchema, SchemaRegistry, render_relation_caption,
};
use vesica_store::SqliteConnection;
use vesica_store::repo::relation::{EndpointRole as StoreEndpointRole, RelationFilter};
use vesica_store::repo::source as store_source;
use vesica_store::repo::{EntityRow, NewAuditEntry, NewRelationProp, RelationPropRow, RelationRow};
use vesica_store::repo::{audit, entity, relation};

use super::case::AuditAction;
use super::entity::{
    ExistingValue, Prepared, case_context, check_shape, duplicate_id_conflict, live_case_context,
    normalize_all, plan_edit, require_caption, require_required, seq_within, validate_caption,
    validate_id,
};
use super::error::{Actor, ServiceError};
use super::{Confidence, EditProperties, Patch, PropertyValue, PropertyWarning, Services};

/// What `audit_log.target_kind` says for a Relation.
pub(super) const TARGET_RELATION: &str = "relation";

/// How many passes the delete cascade will make over one Entity's edges
/// before deciding something is wrong. Each pass soft-deletes up to
/// [`vesica_store::repo::relation::MAX_LIST_LIMIT`] Relations and the next
/// pass sees only what is still live, so this is a ceiling on fan-out, not
/// on work: it permits an Entity with a million edges and refuses to spin
/// forever if a pass ever fails to make progress.
const MAX_CASCADE_PASSES: usize = 1_000;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// One end of a Relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Endpoint {
    /// Where a directed edge starts — the owner, the director, the employee.
    Source,
    /// Where it points — the asset, the company, the employer.
    Target,
}

impl Endpoint {
    fn as_str(self) -> &'static str {
        match self {
            Endpoint::Source => "source_id",
            Endpoint::Target => "target_id",
        }
    }
}

/// Which end a listing means when it asks for one Entity's Relations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EndpointRole {
    /// Only where the Entity is the source of a directed Relation.
    Source,
    /// Only where it is the target of a directed Relation.
    Target,
    /// Either end. The default, and what "the Relations of this node" means
    /// to anyone drawing a graph.
    #[default]
    Either,
}

impl EndpointRole {
    fn to_store(self) -> StoreEndpointRole {
        match self {
            EndpointRole::Source => StoreEndpointRole::Source,
            EndpointRole::Target => StoreEndpointRole::Target,
            EndpointRole::Either => StoreEndpointRole::Either,
        }
    }
}

/// A stable, machine-readable reason a Relation was flagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RelationWarningCode {
    /// An endpoint holds an Entity of a Schema the relation Schema does not
    /// list for that end. The Relation was recorded anyway — the constraint
    /// is advisory (`04-data-model.md` 4.1.3).
    EndpointSchemaUnexpected,
}

/// An endpoint the relation Schema did not expect, recorded rather than
/// refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointWarning {
    /// Which end was unexpected.
    pub endpoint: Endpoint,
    /// A stable machine-readable reason.
    pub code: RelationWarningCode,
    /// The Entity sitting there.
    pub entity_id: String,
    /// The Schema that Entity has.
    pub schema_id: String,
    /// The Schemas the relation Schema lists for this end.
    pub expected: Vec<String>,
    /// What looked wrong, in words.
    pub message: String,
}

/// When a Relation was valid, and how precisely that is known.
///
/// Either bound may be absent and they mean different things: no `start` is
/// "we do not know when this began", no `end` is "as far as we know it has
/// not ended". `precision` is what a timeline reads to draw a year-wide
/// band rather than a false point, and it is the **coarsest** of the two
/// bounds — an interval running from `2015-03` to `2021` is known to the
/// year, because claiming month precision for it would assert something the
/// evidence does not support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interval {
    /// ISO-8601 UTC lower bound: the first instant the Relation could have
    /// been valid.
    pub start: Option<String>,
    /// ISO-8601 UTC upper bound: the last instant it could have been valid.
    pub end: Option<String>,
    pub precision: DatePrecision,
}

/// A validity interval as a caller states it.
///
/// Bounds are ISO-8601 or a prefix of one — `2019`, `2019-03`,
/// `2019-03-14`, `2019-03-14T09:00:00Z` — and the precision of what is sent
/// is the precision that is stored. Slash- and dot-separated dates are
/// refused here, deliberately: `03/04/2019` is two different days depending
/// on who typed it, and unlike a date Property there is no column keeping
/// the text a reader could check the guess against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetInterval {
    /// When it began.
    pub start: Option<String>,
    /// When it ended. Absent means "still, as far as is known".
    pub end: Option<String>,
    /// The precision to record. `None` takes the coarsest of the bounds as
    /// written, which is almost always what is meant. Given explicitly it
    /// may only be the same or coarser: an interval written `2019` cannot be
    /// recorded as known to the day.
    pub precision: Option<DatePrecision>,
}

impl SetInterval {
    fn is_empty(&self) -> bool {
        self.start.is_none() && self.end.is_none()
    }
}

/// A Relation as the rest of the system sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    /// ULID, 26 characters.
    pub id: String,
    /// The Case this Relation belongs to, which is also both endpoints'.
    pub case_id: String,
    /// The relation Schema it was created from.
    pub schema_id: String,
    /// The Entity at the source end.
    pub source_id: String,
    /// The Entity at the target end.
    pub target_id: String,
    /// Whether the edge reads one way. `false` means the two ends are
    /// interchangeable and no arrowhead should be drawn.
    pub directed: bool,
    /// The one-line label: the Schema's rendered caption, or an override.
    pub caption: String,
    pub confidence: Confidence,
    /// When this was valid, if that is recorded at all.
    pub interval: Option<Interval>,
    /// Every value, in Schema-declared Property order and then in entry
    /// order within each Property.
    pub properties: Vec<PropertyValue>,
    /// Anything flagged about the values above. Empty is the normal case.
    pub warnings: Vec<PropertyWarning>,
    /// Endpoints the Schema did not expect. Empty is the normal case; a
    /// non-empty list is guidance, not a defect.
    pub endpoint_warnings: Vec<EndpointWarning>,
    /// Where this Relation's claims come from: the Sources cited for the
    /// edge as a whole and for its individual Property values. Empty means
    /// unsourced — permitted, and flagged by
    /// [`Services::unsourced_facts`].
    pub citations: super::Citations,
    /// ISO-8601 UTC.
    pub created_at: String,
    /// Who recorded it — the analyst, or a named agent.
    pub created_by: String,
    /// ISO-8601 UTC.
    pub updated_at: String,
}

impl Relation {
    /// This Relation's values as the caption renderer wants them: raw
    /// values by Property name, in entry order.
    pub fn property_values(&self) -> PropertyValues {
        let mut values: PropertyValues = std::collections::BTreeMap::new();
        for value in &self.properties {
            values
                .entry(value.prop.clone())
                .or_default()
                .push(value.value.clone());
        }
        values
    }

    /// Whether this Relation touches the given Entity at either end.
    pub fn touches(&self, entity_id: &str) -> bool {
        self.source_id == entity_id || self.target_id == entity_id
    }
}

/// A Relation as a list shows it: the edge without its values.
///
/// Lists deliberately do not carry Properties, for the reason
/// [`super::EntitySummary`] does not: a page of edges with every value of
/// each is an unbounded amount of data behind a bounded row count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationSummary {
    pub id: String,
    pub case_id: String,
    pub schema_id: String,
    pub source_id: String,
    pub target_id: String,
    pub directed: bool,
    pub caption: String,
    pub confidence: Confidence,
    pub interval: Option<Interval>,
    pub created_at: String,
    pub updated_at: String,
}

/// A page of the Relation list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationList {
    pub relations: Vec<RelationSummary>,
    /// Whether there were more rows than the budget allowed. AGENT-BRIEF.md
    /// rule 5.
    pub truncated: bool,
    /// The budget actually applied, after clamping.
    pub limit: usize,
    pub offset: usize,
}

/// Everything needed to connect two Entities.
#[derive(Debug, Clone, Default)]
pub struct CreateRelation {
    /// The id to give it. `None` generates one, so an agent can build a
    /// whole subgraph locally and submit it in one piece.
    pub id: Option<String>,
    /// The Case it belongs to. Both endpoints must be in it.
    pub case_id: String,
    /// The relation Schema to instantiate.
    pub schema_id: String,
    /// The Entity at the source end.
    pub source_id: String,
    /// The Entity at the target end.
    pub target_id: String,
    /// The values, by Property name, in the order they should be kept.
    pub properties: PropertyValues,
    /// A caption to use instead of the Schema's.
    pub caption: Option<String>,
    /// `None` means [`Confidence::Probable`].
    pub confidence: Option<Confidence>,
    /// When the Relation was valid. Only for a Schema that declares an
    /// interval.
    pub interval: Option<SetInterval>,
}

/// A change to a Relation's label, confidence or interval.
///
/// Its Schema and its two endpoints are absent by design: an edge that
/// changes which two things it joins is a different edge. Delete it and
/// record the right one, so the audit trail says that is what happened.
#[derive(Debug, Clone, Default)]
pub struct UpdateRelation {
    /// [`Patch::Set`] overrides the caption, [`Patch::Clear`] puts it back
    /// under derivation from the Schema, [`Patch::Keep`] leaves it.
    pub caption: Patch<String>,
    /// `None` leaves the confidence alone.
    pub confidence: Option<Confidence>,
    /// [`Patch::Set`] records a validity interval, [`Patch::Clear`] removes
    /// the one that was there, [`Patch::Keep`] leaves it.
    pub interval: Patch<SetInterval>,
}

/// Which Relations [`Services::list_relations`] returns. Soft-deleted
/// Relations are never included.
#[derive(Debug, Clone, Default)]
pub struct RelationQuery {
    /// The Case to list within.
    pub case_id: String,
    /// Only Relations of this Schema.
    pub schema_id: Option<String>,
    /// Only Relations at this confidence level.
    pub confidence: Option<Confidence>,
    /// `Some(false)` lists only the undirected ones, which is how a caller
    /// tells them apart without reading every row.
    pub directed: Option<bool>,
    /// Only Relations touching this Entity.
    pub endpoint: Option<String>,
    /// Which end `endpoint` must be. An undirected Relation answers to all
    /// three, from either end.
    pub role: EndpointRole,
    /// Row budget. `None` uses [`super::ServiceConfig::max_page_size`].
    pub limit: Option<usize>,
    /// Rows to skip.
    pub offset: usize,
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

impl Services {
    /// Connects two Entities: a new Relation of one Schema, with its
    /// Properties and its interval.
    ///
    /// Everything that can be checked without reading the database is
    /// checked before the transaction opens. The endpoints are read inside
    /// it, because whether they exist, are visible and are in this Case is a
    /// fact about the database at the moment of writing.
    pub async fn create_relation(
        &self,
        request: CreateRelation,
        actor: &Actor,
    ) -> Result<Relation, ServiceError> {
        let schema = require_relation_schema(self.schemas(), &request.schema_id)?.clone();
        let id = match request.id {
            Some(id) => validate_id(&id)?,
            None => vesica_store::new_id(),
        };
        let caption_override = validate_caption(request.caption.as_deref())?;
        let confidence = request.confidence.unwrap_or_default();
        let flat = check_shape(&schema, &request.properties)?;
        require_required(&schema, flat.iter().map(|v| v.prop.as_str()))?;
        let interval = match &request.interval {
            Some(interval) => parse_interval(&schema, interval)?,
            None => None,
        };

        let case_id = request.case_id;
        let source_id = request.source_id;
        let target_id = request.target_id;
        let now = vesica_store::now();
        let actor = actor.as_str().to_string();
        let schemas = self.schemas_handle();

        self.tx(move |conn| {
            Box::pin(async move {
                let ctx = live_case_context(conn, &case_id).await?;
                let source = endpoint_entity(conn, Endpoint::Source, &source_id, &case_id).await?;
                let target = endpoint_entity(conn, Endpoint::Target, &target_id, &case_id).await?;
                let prepared = normalize_all(flat, &ctx)?;

                let row = RelationRow {
                    id,
                    case_id,
                    schema_id: schema.id.clone(),
                    source_id: source.id.clone(),
                    target_id: target.id.clone(),
                    // Directedness is a property of the relation type, not
                    // of one instance of it: whether "owns" reads one way is
                    // settled by the ontology, so it is taken from the
                    // Schema and never from the request.
                    directed: schema.directed,
                    date_start: interval.as_ref().and_then(|i| i.start.clone()),
                    date_end: interval.as_ref().and_then(|i| i.end.clone()),
                    date_prec: interval.as_ref().map(|i| i.precision.as_str().to_string()),
                    confidence: confidence.as_str().to_string(),
                    caption: caption_override,
                    created_at: now.clone(),
                    created_by: actor.clone(),
                    updated_at: now,
                    deleted_at: None,
                };

                let props: Vec<RelationPropRow> = prepared
                    .iter()
                    .enumerate()
                    .map(|(index, value)| as_prop_row(value, &row.id, seq_within(&prepared, index)))
                    .collect();
                let after = assemble(
                    row.clone(),
                    props,
                    &schema,
                    &ctx,
                    &schemas,
                    &source,
                    &target,
                )?;

                // Audit first, change second: if the insert fails — a
                // duplicate id, a constraint, a disk error — the whole
                // transaction goes, audit entry included.
                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Create,
                        &row.case_id,
                        &row.id,
                        None,
                        snapshot(&after),
                    ),
                )
                .await?;
                relation::insert(conn, &row)
                    .await
                    .map_err(|e| duplicate_id_conflict(&row.id, e))?;
                for (index, value) in prepared.iter().enumerate() {
                    let new = as_new_prop(value, &row.id, seq_within(&prepared, index));
                    relation::insert_prop(conn, &new).await?;
                }

                load(conn, row, &schema, &ctx, &schemas).await
            })
        })
        .await
    }

    /// The Relation with this id, with its Properties.
    /// [`ServiceError::NotFound`] if there is no such Relation or it has
    /// been deleted.
    pub async fn get_relation(&self, id: &str) -> Result<Relation, ServiceError> {
        let pool = self.db().read_pool();
        let row = relation::find(pool, id)
            .await?
            .filter(|row| row.deleted_at.is_none())
            .ok_or_else(|| ServiceError::not_found(TARGET_RELATION, id))?;

        let schema = self.relation_schema_of(&row)?.clone();
        let ctx = case_context(pool, &row.case_id).await?;
        let props = relation::props(pool, &row.id).await?;
        let source = read_endpoint(pool, &row.source_id).await?;
        let target = read_endpoint(pool, &row.target_id).await?;
        let relation_id = row.id.clone();
        let mut relation = assemble(
            row,
            props,
            &schema,
            &ctx,
            &self.schemas_handle(),
            &source,
            &target,
        )?;
        // Citations are two more queries, and the helper that runs them
        // needs one connection to run both on.
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| ServiceError::Store(vesica_store::StoreError::from(e)))?;
        relation.citations = super::source::citations_of_relation(&mut conn, &relation_id).await?;
        Ok(relation)
    }

    /// A bounded page of Relations in one Case, newest first.
    ///
    /// Filter by Schema, by confidence, by directedness, and by an
    /// endpoint — at either end or at one named end. An undirected Relation
    /// is returned for a source-end or target-end query from *either* of its
    /// ends: it asserts no direction, so which column its endpoints happen
    /// to sit in must not decide whether a query finds it.
    pub async fn list_relations(&self, query: RelationQuery) -> Result<RelationList, ServiceError> {
        let limit = self.page_size(query.limit);
        let filter = RelationFilter {
            case_id: query.case_id,
            schema_id: query.schema_id,
            confidence: query.confidence.map(|c| c.as_str().to_string()),
            directed: query.directed,
            endpoint: query.endpoint,
            role: query.role.to_store(),
            // One more than the budget, to learn whether there was more
            // without a second count query.
            limit: (limit as i64).saturating_add(1),
            offset: query.offset as i64,
        };
        let mut rows = relation::list(self.db().read_pool(), &filter).await?;
        let truncated = rows.len() > limit;
        rows.truncate(limit);

        let mut relations = Vec::with_capacity(rows.len());
        for row in rows {
            let schema = self.relation_schema_of(&row)?;
            relations.push(summary(row, schema)?);
        }

        Ok(RelationList {
            relations,
            truncated,
            limit,
            offset: query.offset,
        })
    }

    /// Changes a Relation's caption, confidence or validity interval.
    pub async fn update_relation(
        &self,
        id: &str,
        request: UpdateRelation,
        actor: &Actor,
    ) -> Result<Relation, ServiceError> {
        let caption = match request.caption.resolve() {
            Some(Some(caption)) => Some(Some(require_caption(&caption)?)),
            Some(None) => Some(None),
            None => None,
        };
        let interval_request = request.interval.resolve();
        let confidence = request.confidence;
        let id = id.to_string();
        let now = vesica_store::now();
        let actor = actor.as_str().to_string();
        let schemas = self.schemas_handle();

        self.tx(move |conn| {
            Box::pin(async move {
                let mut row = live_relation(conn, &id).await?;
                let schema = relation_schema_for(&schemas, &row)?.clone();
                let ctx = case_context(&mut *conn, &row.case_id).await?;
                let props = relation::props(&mut *conn, &row.id).await?;
                let source = read_endpoint(&mut *conn, &row.source_id).await?;
                let target = read_endpoint(&mut *conn, &row.target_id).await?;
                let mut before = assemble(
                    row.clone(),
                    props.clone(),
                    &schema,
                    &ctx,
                    &schemas,
                    &source,
                    &target,
                )?;
                before.citations = super::source::citations_of_relation(conn, &before.id).await?;

                let was = (
                    row.caption.clone(),
                    row.confidence.clone(),
                    row.date_start.clone(),
                    row.date_end.clone(),
                    row.date_prec.clone(),
                );

                if let Some(caption) = &caption {
                    row.caption = caption.clone();
                }
                if let Some(confidence) = confidence {
                    row.confidence = confidence.as_str().to_string();
                }
                match &interval_request {
                    Some(Some(request)) => {
                        let interval = parse_interval(&schema, request)?;
                        row.date_start = interval.as_ref().and_then(|i| i.start.clone());
                        row.date_end = interval.as_ref().and_then(|i| i.end.clone());
                        row.date_prec = interval.as_ref().map(|i| i.precision.as_str().to_string());
                    }
                    Some(None) => {
                        row.date_start = None;
                        row.date_end = None;
                        row.date_prec = None;
                    }
                    None => {}
                }

                // "Did anything change" is decided on the fields this use
                // case can change, and decided BEFORE `updated_at` moves —
                // see the same reasoning in `update_entity`.
                let now_is = (
                    row.caption.clone(),
                    row.confidence.clone(),
                    row.date_start.clone(),
                    row.date_end.clone(),
                    row.date_prec.clone(),
                );
                if was == now_is {
                    return Ok(before);
                }

                row.updated_at = now;
                let mut after = assemble(
                    row.clone(),
                    props,
                    &schema,
                    &ctx,
                    &schemas,
                    &source,
                    &target,
                )?;
                after.citations = before.citations.clone();

                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Update,
                        &row.case_id,
                        &row.id,
                        snapshot(&before),
                        snapshot(&after),
                    ),
                )
                .await?;
                relation::save(conn, &row).await?;
                Ok(after)
            })
        })
        .await
    }

    /// Adds, replaces and removes a Relation's Property values in one act,
    /// on the same code path as [`Services::edit_entity_properties`].
    ///
    /// The resulting Relation has to satisfy its Schema, so an edit that
    /// would leave it invalid is refused whole rather than applied in part.
    pub async fn edit_relation_properties(
        &self,
        id: &str,
        request: EditProperties,
        actor: &Actor,
    ) -> Result<Relation, ServiceError> {
        let id = id.to_string();
        let now = vesica_store::now();
        let actor = actor.as_str().to_string();
        let schemas = self.schemas_handle();

        self.tx(move |conn| {
            Box::pin(async move {
                let mut row = live_relation(conn, &id).await?;
                let schema = relation_schema_for(&schemas, &row)?.clone();
                let ctx = case_context(&mut *conn, &row.case_id).await?;
                let existing = relation::props(&mut *conn, &row.id).await?;
                let source = read_endpoint(&mut *conn, &row.source_id).await?;
                let target = read_endpoint(&mut *conn, &row.target_id).await?;
                let mut before = assemble(
                    row.clone(),
                    existing.clone(),
                    &schema,
                    &ctx,
                    &schemas,
                    &source,
                    &target,
                )?;
                before.citations = super::source::citations_of_relation(conn, &before.id).await?;

                if request.is_empty() {
                    return Ok(before);
                }

                let seen: Vec<ExistingValue> = existing.iter().map(as_existing).collect();
                let plan = plan_edit(&schema, &seen, &request, &ctx)?;
                if plan.is_noop() {
                    return Ok(before);
                }

                let mut kept: Vec<RelationPropRow> = existing
                    .iter()
                    .filter(|p| !plan.delete_rows.contains(&p.id))
                    .cloned()
                    .collect();
                kept.extend(
                    plan.inserts
                        .iter()
                        .map(|planned| as_prop_row(&planned.value, &row.id, planned.seq)),
                );
                row.updated_at = now;

                let after = assemble(row.clone(), kept, &schema, &ctx, &schemas, &source, &target)?;
                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Update,
                        &row.case_id,
                        &row.id,
                        snapshot(&before),
                        snapshot(&after),
                    ),
                )
                .await?;

                for prop in &plan.clear_props {
                    relation::delete_props_named(conn, &row.id, prop).await?;
                }
                for row_id in &plan.delete_rows {
                    relation::delete_prop(conn, *row_id).await?;
                }
                for planned in &plan.inserts {
                    let new = as_new_prop(&planned.value, &row.id, planned.seq);
                    relation::insert_prop(conn, &new).await?;
                }
                relation::save(conn, &row).await?;

                load(conn, row, &schema, &ctx, &schemas).await
            })
        })
        .await
    }

    /// Deletes a Relation, reversibly: it disappears from every read and
    /// [`Services::restore_relation`] brings it back exactly as it was,
    /// Properties included.
    ///
    /// Deleting an already-deleted Relation changes nothing and records
    /// nothing; deleting one that never existed is
    /// [`ServiceError::NotFound`].
    pub async fn delete_relation(&self, id: &str, actor: &Actor) -> Result<(), ServiceError> {
        let id = id.to_string();
        let now = vesica_store::now();
        let actor = actor.as_str().to_string();
        let schemas = self.schemas_handle();

        self.tx(move |conn| {
            Box::pin(async move {
                let row = relation::find(&mut *conn, &id)
                    .await?
                    .ok_or_else(|| ServiceError::not_found(TARGET_RELATION, &id))?;
                if row.deleted_at.is_some() {
                    return Ok(());
                }
                let schema = relation_schema_for(&schemas, &row)?.clone();
                let ctx = case_context(&mut *conn, &row.case_id).await?;
                let props = relation::props(&mut *conn, &row.id).await?;
                let source = read_endpoint(&mut *conn, &row.source_id).await?;
                let target = read_endpoint(&mut *conn, &row.target_id).await?;
                let before = assemble(
                    row.clone(),
                    props,
                    &schema,
                    &ctx,
                    &schemas,
                    &source,
                    &target,
                )?;

                let mut deleted = row;
                deleted.deleted_at = Some(now.clone());
                deleted.updated_at = now;

                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Delete,
                        &deleted.case_id,
                        &deleted.id,
                        snapshot(&before),
                        None,
                    ),
                )
                .await?;
                relation::save(conn, &deleted).await?;
                Ok(())
            })
        })
        .await
    }

    /// Brings a soft-deleted Relation back. Restoring one that is not
    /// deleted changes nothing and records nothing.
    ///
    /// An edge to nothing is not a graph, so a Relation whose endpoints are
    /// not both visible cannot be restored: restore the Entity first, which
    /// brings back the Relations that Entity's deletion took down anyway.
    pub async fn restore_relation(
        &self,
        id: &str,
        actor: &Actor,
    ) -> Result<Relation, ServiceError> {
        let id = id.to_string();
        let now = vesica_store::now();
        let actor = actor.as_str().to_string();
        let schemas = self.schemas_handle();

        self.tx(move |conn| {
            Box::pin(async move {
                let mut row = relation::find(&mut *conn, &id)
                    .await?
                    .ok_or_else(|| ServiceError::not_found(TARGET_RELATION, &id))?;
                let schema = relation_schema_for(&schemas, &row)?.clone();
                let ctx = case_context(&mut *conn, &row.case_id).await?;
                if row.deleted_at.is_none() {
                    return load(conn, row, &schema, &ctx, &schemas).await;
                }

                for (end, entity_id) in [
                    (Endpoint::Source, row.source_id.clone()),
                    (Endpoint::Target, row.target_id.clone()),
                ] {
                    require_live_endpoint(&mut *conn, end, &entity_id).await?;
                }

                row.deleted_at = None;
                row.updated_at = now;

                let props = relation::props(&mut *conn, &row.id).await?;
                let source = read_endpoint(&mut *conn, &row.source_id).await?;
                let target = read_endpoint(&mut *conn, &row.target_id).await?;
                let mut after = assemble(
                    row.clone(),
                    props,
                    &schema,
                    &ctx,
                    &schemas,
                    &source,
                    &target,
                )?;

                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Restore,
                        &row.case_id,
                        &row.id,
                        None,
                        snapshot(&after),
                    ),
                )
                .await?;
                relation::save(conn, &row).await?;
                after.citations = super::source::citations_of_relation(conn, &after.id).await?;
                Ok(after)
            })
        })
        .await
    }

    /// The audit trail for one Relation, newest first, bounded.
    ///
    /// Like the Case and Entity trails this does not require the Relation to
    /// be visible: the trail's purpose is to outlive what it describes.
    pub async fn relation_audit(
        &self,
        id: &str,
        limit: Option<usize>,
    ) -> Result<super::AuditPage, ServiceError> {
        self.audit_for_target(TARGET_RELATION, id, limit).await
    }

    fn relation_schema_of(&self, row: &RelationRow) -> Result<&RelationSchema, ServiceError> {
        self.schemas().relation(&row.schema_id).ok_or_else(|| {
            ServiceError::Inconsistent(format!(
                "relation `{}` names schema `{}`, which the registry does not have",
                row.id, row.schema_id
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// The cascade
// ---------------------------------------------------------------------------

/// Soft-deletes every live Relation touching one Entity, recording each one
/// under `batch_id` so that restoring the Entity can find exactly these
/// again. Runs on the caller's transaction: the Entity's deletion and every
/// edge it takes with it commit together or not at all.
///
/// The `before` payload is the Relation's row rather than the row plus its
/// Properties. A cascade can touch a very large number of edges, and the
/// Properties are untouched by a soft delete and come back with the
/// Relation — so reading them all back would buy nothing and cost one query
/// per edge.
pub(super) async fn cascade_delete(
    conn: &mut SqliteConnection,
    entity_id: &str,
    now: &str,
    actor: &str,
    batch_id: &str,
) -> Result<usize, ServiceError> {
    let mut deleted = 0usize;
    for _ in 0..MAX_CASCADE_PASSES {
        let rows = relation::touching(&mut *conn, entity_id).await?;
        if rows.is_empty() {
            return Ok(deleted);
        }
        for row in rows {
            let mut entry = entry(
                actor,
                AuditAction::Delete,
                &row.case_id,
                &row.id,
                cascade_snapshot(&row, entity_id),
                None,
            );
            entry.batch_id = Some(batch_id.to_string());
            audit::record(&mut *conn, entry).await?;

            let mut gone = row;
            gone.deleted_at = Some(now.to_string());
            gone.updated_at = now.to_string();
            relation::save(&mut *conn, &gone).await?;
            deleted += 1;
        }
    }
    Err(ServiceError::Inconsistent(format!(
        "deleting `{entity_id}` still found relations to cascade into after \
         {MAX_CASCADE_PASSES} passes"
    )))
}

/// Restores the Relations one delete batch took down — and only those.
///
/// A Relation deleted on its own before the Entity went belongs to a
/// different batch (or to none) and is deliberately left alone: the analyst
/// deleted it for a reason that has nothing to do with this Entity, and
/// undoing that decision silently would be the tool overruling them.
///
/// A Relation whose *other* endpoint is itself deleted is also left alone,
/// because the graph must never contain an edge to nothing. Restoring that
/// other Entity does not bring the edge back either — the edge belongs to
/// this batch, not that one — so it is restored explicitly with
/// [`Services::restore_relation`] once both ends are visible again.
pub(super) async fn cascade_restore(
    conn: &mut SqliteConnection,
    batch_id: &str,
    now: &str,
    actor: &str,
) -> Result<usize, ServiceError> {
    let ids = audit::targets_in_batch(
        &mut *conn,
        batch_id,
        TARGET_RELATION,
        AuditAction::Delete.as_str(),
    )
    .await?;

    let mut restored = 0usize;
    for id in ids {
        let Some(row) = relation::find(&mut *conn, &id).await? else {
            continue;
        };
        if row.deleted_at.is_none() {
            continue;
        }
        if !endpoints_visible(&mut *conn, &row).await? {
            continue;
        }

        let mut back = row;
        back.deleted_at = None;
        back.updated_at = now.to_string();

        let mut entry = entry(
            actor,
            AuditAction::Restore,
            &back.case_id,
            &back.id,
            None,
            cascade_snapshot(&back, ""),
        );
        entry.batch_id = Some(batch_id.to_string());
        audit::record(&mut *conn, entry).await?;
        relation::save(&mut *conn, &back).await?;
        restored += 1;
    }
    Ok(restored)
}

async fn endpoints_visible(
    conn: &mut SqliteConnection,
    row: &RelationRow,
) -> Result<bool, ServiceError> {
    for id in [&row.source_id, &row.target_id] {
        let live = entity::find(&mut *conn, id)
            .await?
            .is_some_and(|e| e.deleted_at.is_none());
        if !live {
            return Ok(false);
        }
    }
    Ok(true)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// The relation Schema with this id, or a refusal listing the ones there
/// are. The catalogue is data and is expected to be replaced wholesale, so
/// the list comes from the registry rather than from anything written here.
fn require_relation_schema<'r>(
    registry: &'r SchemaRegistry,
    id: &str,
) -> Result<&'r RelationSchema, ServiceError> {
    registry.relation(id).ok_or_else(|| {
        let known: Vec<&str> = registry.relations().map(|s| s.id.as_str()).collect();
        ServiceError::validation(
            "schema_id",
            format!("`{id}` is not a relation schema"),
            format!("use one of: {}", known.join(", ")),
        )
    })
}

/// Reads a validity interval a caller asked for, refusing anything the
/// Schema or the calendar cannot support.
fn parse_interval(
    schema: &RelationSchema,
    request: &SetInterval,
) -> Result<Option<Interval>, ServiceError> {
    if request.is_empty() {
        return Ok(None);
    }
    if !schema.interval {
        return Err(ServiceError::validation(
            "interval",
            format!("`{}` does not carry a validity interval", schema.id),
            "omit `interval`, or record this on a relation schema that declares \
             one; if the dates belong to the relation itself, they may belong on \
             one of its properties instead",
        ));
    }

    let start = parse_bound("interval.start", request.start.as_deref())?;
    let end = parse_bound("interval.end", request.end.as_deref())?;

    if let (Some(start), Some(end)) = (&start, &end)
        && start.lower_bound() > end.upper_bound()
    {
        return Err(ServiceError::validation(
            "interval",
            format!(
                "the interval starts at `{}` and ends at `{}`, which is earlier",
                start.text(),
                end.text()
            ),
            "send an `end` that is not before `start`, or omit `end` if the \
             relation has not ended",
        ));
    }

    // The coarsest bound governs: an interval from `2015-03` to `2021` is
    // known to the year, because month precision would assert something the
    // second bound does not support.
    let written = [start.as_ref(), end.as_ref()]
        .into_iter()
        .flatten()
        .map(|d| d.precision())
        .min()
        .unwrap_or(DatePrecision::Day);

    let precision = match request.precision {
        None => written,
        Some(asked) if asked <= written => asked,
        Some(asked) => {
            return Err(ServiceError::validation(
                "interval.precision",
                format!(
                    "the bounds are only known to `{}`, so the interval cannot be \
                     recorded as known to `{}`",
                    written.as_str(),
                    asked.as_str()
                ),
                format!(
                    "omit `precision` to record `{}`, or send bounds written to \
                     the precision claimed",
                    written.as_str()
                ),
            ));
        }
    };

    Ok(Some(Interval {
        start: start.map(|d| d.lower_iso()),
        end: end.map(|d| d.upper_iso()),
        precision,
    }))
}

/// One end of an interval, as ISO-8601 or a prefix of it.
fn parse_bound(field: &str, text: Option<&str>) -> Result<Option<PartialDate>, ServiceError> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    let parsed = PartialDate::parse_iso(text).map_err(|e| {
        ServiceError::validation(
            field,
            format!("`{text}` is not an ISO-8601 date: {e}"),
            "send `2019`, `2019-03`, `2019-03-14` or `2019-03-14T09:00:00Z` — \
             a partial date is honoured exactly as written",
        )
    })?;
    Ok(Some(parsed.date))
}

/// Reads an endpoint that must exist, be visible and be in this Case.
///
/// Cross-Case links are refused here rather than stored and flagged.
/// Resolving one investigation's Person against another's is a later
/// milestone with its own review step (`0001-foundation-and-domain-core.md`
/// on merging), and an edge that quietly straddles two Cases would leak one
/// investigation's contents into the other's graph before any of that
/// exists.
async fn endpoint_entity(
    conn: &mut SqliteConnection,
    end: Endpoint,
    entity_id: &str,
    case_id: &str,
) -> Result<EntityRow, ServiceError> {
    let field = end.as_str();
    let row = entity::find(&mut *conn, entity_id)
        .await?
        .filter(|row| row.deleted_at.is_none())
        .ok_or_else(|| {
            ServiceError::validation(
                field,
                format!("there is no visible entity `{entity_id}`"),
                "create the entity first, or send the id of one that exists and \
                 has not been deleted",
            )
        })?;

    if let Some(survivor) = &row.merged_into {
        return Err(ServiceError::validation(
            field,
            format!("`{entity_id}` was merged into `{survivor}` and is no longer a node"),
            format!("use `{survivor}`, which is the surviving entity"),
        ));
    }

    if row.case_id != case_id {
        return Err(ServiceError::validation(
            field,
            format!(
                "`{entity_id}` belongs to case `{}`, and this relation is being \
                 recorded in case `{case_id}`",
                row.case_id
            ),
            "both endpoints of a relation must be in the relation's own case; \
             record the entity in this case, or record the relation in the case \
             the entity is already in — linking across cases is not supported",
        ));
    }

    Ok(row)
}

/// As [`endpoint_entity`], for a restore that only needs to know the
/// endpoint is visible again.
async fn require_live_endpoint(
    conn: &mut SqliteConnection,
    end: Endpoint,
    entity_id: &str,
) -> Result<(), ServiceError> {
    let live = entity::find(&mut *conn, entity_id)
        .await?
        .is_some_and(|row| row.deleted_at.is_none());
    if live {
        return Ok(());
    }
    Err(ServiceError::Conflict {
        message: format!("`{entity_id}`, this relation's {end:?} endpoint, is deleted"),
        fix: format!(
            "restore entity `{entity_id}` first — the graph must never contain an \
             edge to a deleted node (field `{}`)",
            end.as_str()
        ),
    })
}

/// Reads an endpoint that is known to exist because a Relation references
/// it. A missing row means the foreign key was bypassed.
async fn read_endpoint<'e, E>(exec: E, entity_id: &str) -> Result<EntityRow, ServiceError>
where
    E: sqlx::sqlite::SqliteExecutor<'e>,
{
    entity::find(exec, entity_id).await?.ok_or_else(|| {
        ServiceError::Inconsistent(format!(
            "a relation names entity `{entity_id}`, which does not exist"
        ))
    })
}

/// Reads a Relation that must be visible, inside the caller's transaction,
/// so that a read-modify-write sees the row it is about to overwrite.
async fn live_relation(conn: &mut SqliteConnection, id: &str) -> Result<RelationRow, ServiceError> {
    relation::find(&mut *conn, id)
        .await?
        .filter(|row| row.deleted_at.is_none())
        .ok_or_else(|| ServiceError::not_found(TARGET_RELATION, id))
}

/// Whether the Schema expected this Entity at this end, and the warning if
/// it did not.
///
/// An endpoint spec lists entity Schema ids, and an Entity satisfies it if
/// its own Schema or any Schema it inherits from is on the list — which is
/// what makes `[Thing]` mean "anything" and `[LegalEntity]` mean "a person
/// or a company". A spec that lists nothing constrains nothing.
fn endpoint_warning(
    end: Endpoint,
    spec: &EndpointSpec,
    entity: &EntityRow,
    schema: Option<&EntitySchema>,
) -> Option<EndpointWarning> {
    if spec.schemas.is_empty() {
        return None;
    }
    let satisfied = spec.schemas.iter().any(|allowed| {
        *allowed == entity.schema_id
            || schema.is_some_and(|s| s.ancestors.iter().any(|a| a == allowed))
    });
    if satisfied {
        return None;
    }
    Some(EndpointWarning {
        endpoint: end,
        code: RelationWarningCode::EndpointSchemaUnexpected,
        entity_id: entity.id.clone(),
        schema_id: entity.schema_id.clone(),
        expected: spec.schemas.clone(),
        message: format!(
            "the `{}` end expects {}, and `{}` is a `{}` — recorded anyway, because \
             the schema's endpoint constraints are guidance",
            spec.label,
            spec.schemas.join(" or "),
            entity.id,
            entity.schema_id
        ),
    })
}

// ---------------------------------------------------------------------------
// Assembly
// ---------------------------------------------------------------------------

/// Reads a Relation's Properties and endpoints back on whatever connection
/// the caller is using and builds the domain type — the "re-read inside the
/// transaction" step every mutating use case ends with.
async fn load(
    conn: &mut SqliteConnection,
    row: RelationRow,
    schema: &RelationSchema,
    ctx: &NormalizeContext,
    schemas: &std::sync::Arc<SchemaRegistry>,
) -> Result<Relation, ServiceError> {
    let props = relation::props(&mut *conn, &row.id).await?;
    let source = read_endpoint(&mut *conn, &row.source_id).await?;
    let target = read_endpoint(&mut *conn, &row.target_id).await?;
    let mut relation = assemble(row, props, schema, ctx, schemas, &source, &target)?;
    relation.citations = super::source::citations_of_relation(conn, &relation.id).await?;
    Ok(relation)
}

/// Turns stored rows into the domain type, deriving the warnings as it
/// goes. Pure, so the pooled read path and the in-transaction write path use
/// exactly the same assembly.
#[allow(clippy::too_many_arguments)]
fn assemble(
    row: RelationRow,
    mut props: Vec<RelationPropRow>,
    schema: &RelationSchema,
    ctx: &NormalizeContext,
    schemas: &std::sync::Arc<SchemaRegistry>,
    source: &EntityRow,
    target: &EntityRow,
) -> Result<Relation, ServiceError> {
    sort_props_by_schema(&mut props, schema);

    let mut properties = Vec::with_capacity(props.len());
    let mut warnings = Vec::new();
    let mut seen: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for prop in props {
        let ptype = prop.ptype.parse::<PropertyType>().map_err(|_| {
            ServiceError::Inconsistent(format!(
                "relation_prop.ptype holds `{}`, which is not a property type",
                prop.ptype
            ))
        })?;
        let seq = seen.entry(prop.prop.clone()).or_insert(0);
        let position = *seq;
        *seq += 1;

        // Warnings are derived, not stored — the same bargain the Entity
        // use cases strike, for the same reason.
        if let Ok(normalized) = ptype.normalize(&prop.value, ctx) {
            warnings.extend(normalized.warnings.into_iter().map(|w| PropertyWarning {
                prop: prop.prop.clone(),
                value: prop.value.clone(),
                code: w.code,
                message: w.message,
            }));
        }

        properties.push(PropertyValue {
            prop: prop.prop,
            ptype,
            seq: position,
            value: prop.value,
            value_norm: prop.value_norm,
            numeric: prop.value_num,
            // `relation_prop` has no date columns: a Relation's dates are
            // its interval, which lives on the relation row itself.
            date: None,
            source_id: prop.source_id,
            // A row that has not been written yet has no id, and therefore
            // no handle a citation could name it by.
            value_ref: (prop.id != 0)
                .then(|| store_source::prop_target_id(store_source::PropHost::Relation, prop.id)),
        });
    }

    let endpoint_warnings = [
        endpoint_warning(
            Endpoint::Source,
            &schema.source,
            source,
            schemas.entity(&source.schema_id),
        ),
        endpoint_warning(
            Endpoint::Target,
            &schema.target,
            target,
            schemas.entity(&target.schema_id),
        ),
    ]
    .into_iter()
    .flatten()
    .collect();

    let values = raw_values_of(&properties);
    Ok(Relation {
        caption: caption_of(&row, schema, &values),
        confidence: Confidence::parse(&row.confidence).map_err(|_| {
            ServiceError::Inconsistent(format!(
                "relation.confidence holds `{}`, which is not a confidence level",
                row.confidence
            ))
        })?,
        interval: interval_of(&row)?,
        id: row.id,
        case_id: row.case_id,
        schema_id: row.schema_id,
        source_id: row.source_id,
        target_id: row.target_id,
        directed: row.directed,
        properties,
        warnings,
        endpoint_warnings,
        // Filled by whoever is about to hand this Relation to a caller —
        // reading citations is a query, and `assemble` is pure.
        citations: super::Citations::default(),
        created_at: row.created_at,
        created_by: row.created_by,
        updated_at: row.updated_at,
    })
}

fn summary(row: RelationRow, schema: &RelationSchema) -> Result<RelationSummary, ServiceError> {
    Ok(RelationSummary {
        // A list carries no Property values, so a caption that a template
        // would have rendered from them falls back to the Schema's label —
        // which is what an edge in a graph is labelled with anyway.
        caption: caption_of(&row, schema, &PropertyValues::new()),
        confidence: Confidence::parse(&row.confidence).map_err(|_| {
            ServiceError::Inconsistent(format!(
                "relation.confidence holds `{}`, which is not a confidence level",
                row.confidence
            ))
        })?,
        interval: interval_of(&row)?,
        id: row.id,
        case_id: row.case_id,
        schema_id: row.schema_id,
        source_id: row.source_id,
        target_id: row.target_id,
        directed: row.directed,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// The stored override, or what the Schema renders.
///
/// A Relation's caption column is nullable, which is what an override is:
/// null means "derived", so nothing has to guess whether a stored string was
/// somebody's deliberate label — the difference the Entity use cases have to
/// infer is recorded here.
fn caption_of(row: &RelationRow, schema: &RelationSchema, values: &PropertyValues) -> String {
    match &row.caption {
        Some(caption) => caption.clone(),
        None => render_relation_caption(schema, values),
    }
}

fn interval_of(row: &RelationRow) -> Result<Option<Interval>, ServiceError> {
    if row.date_start.is_none() && row.date_end.is_none() {
        return Ok(None);
    }
    let precision = row
        .date_prec
        .as_deref()
        .and_then(DatePrecision::parse)
        .ok_or_else(|| {
            ServiceError::Inconsistent(format!(
                "relation `{}` has interval bounds but `date_prec` holds `{}`, \
                 which is not a precision",
                row.id,
                row.date_prec.as_deref().unwrap_or("null")
            ))
        })?;
    Ok(Some(Interval {
        start: row.date_start.clone(),
        end: row.date_end.clone(),
        precision,
    }))
}

fn raw_values_of(properties: &[PropertyValue]) -> PropertyValues {
    let mut values: PropertyValues = std::collections::BTreeMap::new();
    for value in properties {
        values
            .entry(value.prop.clone())
            .or_default()
            .push(value.value.clone());
    }
    values
}

/// Orders values the way a reader expects them: Properties in the order the
/// Schema declares them, values within a Property in the order they were
/// entered.
fn sort_props_by_schema(props: &mut [RelationPropRow], schema: &RelationSchema) {
    let order = |name: &str| {
        schema
            .properties
            .iter()
            .position(|def| def.name == name)
            .unwrap_or(usize::MAX)
    };
    props.sort_by(|a, b| {
        order(&a.prop)
            .cmp(&order(&b.prop))
            .then_with(|| a.prop.cmp(&b.prop))
            .then_with(|| a.seq.cmp(&b.seq))
            .then_with(|| a.id.cmp(&b.id))
    });
}

// ---------------------------------------------------------------------------
// The shared property pipeline, in relation-row terms
// ---------------------------------------------------------------------------

/// The row a normalised value becomes in `relation_prop`. The value itself
/// was prepared by the Entity pipeline; only the shape of the row differs,
/// and it differs by exactly the date columns a Relation does not have.
fn as_new_prop(value: &Prepared, relation_id: &str, seq: i64) -> NewRelationProp {
    NewRelationProp {
        relation_id: relation_id.to_string(),
        prop: value.prop.clone(),
        ptype: value.ptype.as_str().to_string(),
        seq,
        value: value.normalized.raw.clone(),
        value_norm: Some(value.normalized.value.clone()),
        value_num: value.normalized.numeric,
    }
}

/// The same value as a row, for building a snapshot before the insert has
/// happened. `id` is zero because SQLite has not assigned one yet.
fn as_prop_row(value: &Prepared, relation_id: &str, seq: i64) -> RelationPropRow {
    let new = as_new_prop(value, relation_id, seq);
    RelationPropRow {
        id: 0,
        relation_id: new.relation_id,
        prop: new.prop,
        ptype: new.ptype,
        seq: new.seq,
        value: new.value,
        value_norm: new.value_norm,
        value_num: new.value_num,
        source_id: None,
    }
}

fn as_existing(row: &RelationPropRow) -> ExistingValue {
    ExistingValue {
        id: row.id,
        prop: row.prop.clone(),
        seq: row.seq,
        value: row.value.clone(),
        value_norm: row.value_norm.clone(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn relation_schema_for<'r>(
    schemas: &'r std::sync::Arc<SchemaRegistry>,
    row: &RelationRow,
) -> Result<&'r RelationSchema, ServiceError> {
    schemas.relation(&row.schema_id).ok_or_else(|| {
        ServiceError::Inconsistent(format!(
            "relation `{}` names schema `{}`, which the registry does not have",
            row.id, row.schema_id
        ))
    })
}

/// Renders a Relation for an audit entry's `before`/`after`.
///
/// The warnings are left out — they are derived from what is in the
/// snapshot — but each endpoint's Schema is included, because that is the
/// fact an endpoint warning is a judgement about, and it is the one thing a
/// later reader could not otherwise recover once an Entity has changed.
fn snapshot(relation: &Relation) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "id": relation.id,
        "case_id": relation.case_id,
        "schema_id": relation.schema_id,
        "source_id": relation.source_id,
        "target_id": relation.target_id,
        "directed": relation.directed,
        "caption": relation.caption,
        "confidence": relation.confidence,
        "interval": relation.interval,
        "properties": relation.property_values(),
        "endpoint_schemas": relation
            .endpoint_warnings
            .iter()
            .map(|w| (w.endpoint, w.schema_id.clone()))
            .collect::<Vec<_>>(),
    }))
    .ok()
}

/// The `before` payload a cascaded delete records: the row, plus which
/// Entity's deletion took this edge down. Properties are left out — see
/// [`cascade_delete`].
fn cascade_snapshot(row: &RelationRow, cascaded_from: &str) -> Option<String> {
    let mut payload = serde_json::json!({
        "id": row.id,
        "case_id": row.case_id,
        "schema_id": row.schema_id,
        "source_id": row.source_id,
        "target_id": row.target_id,
        "directed": row.directed,
        "confidence": row.confidence,
        "caption": row.caption,
    });
    if !cascaded_from.is_empty() {
        payload["cascaded_from"] = serde_json::json!(cascaded_from);
    }
    serde_json::to_string(&payload).ok()
}

fn entry(
    actor: &str,
    action: AuditAction,
    case_id: &str,
    relation_id: &str,
    before: Option<String>,
    after: Option<String>,
) -> NewAuditEntry {
    NewAuditEntry {
        actor: actor.to_string(),
        action: action.as_str().to_string(),
        case_id: Some(case_id.to_string()),
        target_kind: TARGET_RELATION.to_string(),
        target_id: relation_id.to_string(),
        before,
        after,
        batch_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vesica_core::SchemaRegistry;

    fn schema(id: &str) -> RelationSchema {
        SchemaRegistry::builtin().relation(id).unwrap().clone()
    }

    fn entity_row(id: &str, schema_id: &str) -> EntityRow {
        EntityRow {
            id: id.into(),
            case_id: "case".into(),
            schema_id: schema_id.into(),
            caption: "X".into(),
            confidence: "probable".into(),
            extra: None,
            created_at: "now".into(),
            created_by: "human".into(),
            updated_at: "now".into(),
            deleted_at: None,
            merged_into: None,
        }
    }

    #[test]
    fn an_unknown_relation_schema_is_refused_with_the_catalogue() {
        let registry = SchemaRegistry::builtin();
        let e = require_relation_schema(&registry, "Ownershp").unwrap_err();
        match &e {
            ServiceError::Validation { field, .. } => assert_eq!(field, "schema_id"),
            other => panic!("expected a validation error, got {other:?}"),
        }
        assert!(e.fix().unwrap().contains("Ownership"));
    }

    #[test]
    fn an_interval_takes_the_coarsest_precision_of_its_bounds() {
        let interval = parse_interval(
            &schema("Directorship"),
            &SetInterval {
                start: Some("2015-03".into()),
                end: Some("2021".into()),
                precision: None,
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(interval.precision, DatePrecision::Year);
        assert_eq!(
            interval.start.as_deref(),
            Some("2015-03-01T00:00:00Z"),
            "a partial date keeps its own lower bound"
        );
        assert_eq!(
            interval.end.as_deref(),
            Some("2021-12-31T23:59:59.999999999Z"),
            "`2021` is the whole of 2021, never 1 January"
        );
    }

    #[test]
    fn an_open_ended_interval_is_a_start_with_no_end() {
        let interval = parse_interval(
            &schema("Employment"),
            &SetInterval {
                start: Some("2019".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(interval.precision, DatePrecision::Year);
        assert!(interval.end.is_none(), "still employed, as far as is known");
    }

    #[test]
    fn a_precision_finer_than_the_bounds_support_is_refused() {
        let e = parse_interval(
            &schema("Employment"),
            &SetInterval {
                start: Some("2019".into()),
                end: None,
                precision: Some(DatePrecision::Day),
            },
        )
        .unwrap_err();
        match &e {
            ServiceError::Validation { field, .. } => assert_eq!(field, "interval.precision"),
            other => panic!("expected a validation error, got {other:?}"),
        }
        assert!(e.fix().unwrap().contains("year"));

        // Coarser than written is a deliberate statement of uncertainty and
        // is allowed.
        let coarse = parse_interval(
            &schema("Employment"),
            &SetInterval {
                start: Some("2019-03-14".into()),
                end: None,
                precision: Some(DatePrecision::Year),
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(coarse.precision, DatePrecision::Year);
    }

    #[test]
    fn an_interval_on_a_schema_that_has_none_is_refused_with_a_fix() {
        let e = parse_interval(
            &schema("Associate"),
            &SetInterval {
                start: Some("2019".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.fix().is_some());
        // An empty interval on such a schema is not an error: it asks for
        // nothing.
        assert!(
            parse_interval(&schema("Associate"), &SetInterval::default())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn an_end_before_its_start_is_refused() {
        let e = parse_interval(
            &schema("Directorship"),
            &SetInterval {
                start: Some("2021".into()),
                end: Some("2019".into()),
                precision: None,
            },
        )
        .unwrap_err();
        assert!(e.fix().unwrap().contains("end"));
    }

    #[test]
    fn a_slash_date_is_refused_because_nothing_would_record_which_reading_was_taken() {
        let e = parse_bound("interval.start", Some("03/04/2019")).unwrap_err();
        assert!(e.fix().unwrap().contains("2019-03-14"));
        assert!(parse_bound("interval.start", Some("  ")).unwrap().is_none());
    }

    #[test]
    fn an_endpoint_is_satisfied_by_an_ancestor_and_flagged_when_it_is_not() {
        let registry = SchemaRegistry::builtin();
        let ownership = schema("Ownership");
        let person = entity_row("e1", "Person");

        // `Person` inherits `LegalEntity`, which the owner end lists.
        assert!(
            endpoint_warning(
                Endpoint::Source,
                &ownership.source,
                &person,
                registry.entity("Person")
            )
            .is_none()
        );

        // A directorship expects a Person at the source end; a Company
        // there is recorded and flagged, never refused.
        let directorship = schema("Directorship");
        let company = entity_row("e2", "Company");
        let warning = endpoint_warning(
            Endpoint::Source,
            &directorship.source,
            &company,
            registry.entity("Company"),
        )
        .expect("an unexpected endpoint is flagged");
        assert_eq!(warning.endpoint, Endpoint::Source);
        assert_eq!(warning.code, RelationWarningCode::EndpointSchemaUnexpected);
        assert_eq!(warning.schema_id, "Company");
        assert!(warning.expected.contains(&"Person".to_string()));
    }
}
