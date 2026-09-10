//! The Entity use cases: create, read, list, update, edit properties, tag,
//! soft delete, restore and purge — plus the audit trail every one of them
//! writes.
//!
//! This is the act the whole application exists for: an analyst records a
//! finding. Choose a Schema, fill in Properties, and it becomes a node in
//! the Case.
//!
//! # What is guaranteed about a value
//!
//! - **The raw value is kept exactly as entered, forever.** Nothing here
//!   trims, cleans or corrects what a person actually typed; the normalised
//!   form is stored beside it and is derived, disposable and recomputable
//!   (`vesica-core`'s charter).
//! - **Properties repeat and keep their order.** A person has five phone
//!   numbers without anyone choosing which one is real, and they come back
//!   in the order they went in.
//! - **The Case's region decides what a local value means.** `601 234 567`
//!   in a Polish Case is `+48601234567`; the same digits in a Case with no
//!   region are refused with a message saying so, rather than guessed at.
//! - **A failed checksum is a warning, not a refusal.** The value is
//!   stored, flagged, and the flag comes back on the Entity — see
//!   [`Entity::warnings`]. An investigator copying a tax number off a
//!   scanned filing has to be able to record what the filing says.
//!
//! # Captions
//!
//! A caption is derived from the Schema's template (`"{name}"`) and can be
//! overridden when the analyst wants the node labelled their way. Nothing
//! stores a flag saying which of the two it is, because nothing needs to:
//! an override is recognised as a caption that is not what the Properties
//! would render. So editing the Properties of an Entity whose caption was
//! never touched re-derives it, and editing the Properties of one that was
//! deliberately named leaves that name alone. Clearing the caption
//! ([`Patch::Clear`]) puts it back under derivation.
//!
//! # Deletion, in two strengths
//!
//! - **Delete** — soft, reversible and hidden from every read. It takes
//!   every Relation touching the Entity with it, in the same transaction,
//!   so the graph never contains an edge to a node that is not there; a
//!   restore brings back exactly the edges that deletion took down. See
//!   [`super::relation`].
//! - **Purge** — separate, explicitly confirmed, irreversible, and it
//!   redacts the payloads of the Entity's own earlier audit entries because
//!   those payloads are copies of exactly the data the purge was asked to
//!   erase (`09-security-and-legal.md`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vesica_core::{
    DatePrecision, EntitySchema, NormalizeContext, Normalized, PropertyDef, PropertyType,
    PropertyValues, RelationSchema, WarningCode, render_entity_caption,
};
use vesica_store::SqliteConnection;
use vesica_store::repo::source as store_source;
use vesica_store::repo::{EntityFilter, EntityRow, NewAuditEntry, NewProp, PropRow};
use vesica_store::repo::{audit, case, entity, tag};

use super::case::{AuditAction, redaction_marker};
use super::error::{Actor, ServiceError};
use super::{Patch, Services};

/// What `audit_log.target_kind` says for an Entity.
pub(super) const TARGET_ENTITY: &str = "entity";

/// Longest a caption override may be. A caption is a label on a graph node,
/// not a note.
const MAX_CAPTION_LEN: usize = 200;

/// Longest a single Property value may be. Long enough for an address or a
/// paragraph of notes, short enough that the graph does not become a
/// document store.
const MAX_VALUE_LEN: usize = 4_000;

/// Most values one Property of one Entity may hold. Properties repeat by
/// design; this is the ceiling that keeps "repeat" from meaning
/// "unbounded". AGENT-BRIEF.md rule 5.
const MAX_VALUES_PER_PROPERTY: usize = 100;

/// How many `merged_into` hops [`Services::get_entity`] will follow before
/// deciding the chain is broken. Merges are rare and chains are short; a
/// cycle means the database was written around the service layer.
const MAX_MERGE_HOPS: usize = 8;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// How sure the analyst is that this Entity is real and is what it says.
///
/// Present from the first write and never optional in storage: a guess must
/// never be mistaken for a fact by the analyst who recorded it or by anyone
/// reading their report (`09-security-and-legal.md`, "Accuracy").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Established: documented, corroborated, or seen first-hand.
    Confirmed,
    /// The working assumption. The default, because most of what an
    /// investigation records starts here.
    #[default]
    Probable,
    /// A line of enquiry, recorded so it can be tested or discarded.
    Hypothesis,
}

impl Confidence {
    /// The string stored in `entity.confidence`.
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Confirmed => "confirmed",
            Confidence::Probable => "probable",
            Confidence::Hypothesis => "hypothesis",
        }
    }

    /// Every spelling a caller may send, for error messages.
    pub const ALL: [Confidence; 3] = [
        Confidence::Confirmed,
        Confidence::Probable,
        Confidence::Hypothesis,
    ];

    /// Reads a caller-supplied confidence, refusing anything else with a
    /// `fix` listing what is accepted.
    pub fn parse(value: &str) -> Result<Self, ServiceError> {
        Confidence::ALL
            .into_iter()
            .find(|c| c.as_str() == value.trim().to_ascii_lowercase())
            .ok_or_else(|| {
                ServiceError::validation(
                    "confidence",
                    format!("`{value}` is not a confidence level"),
                    format!("send one of: {}", spelled(&Confidence::ALL)),
                )
            })
    }

    fn from_stored(stored: &str) -> Result<Self, ServiceError> {
        Confidence::ALL
            .into_iter()
            .find(|c| c.as_str() == stored)
            .ok_or_else(|| {
                ServiceError::Inconsistent(format!(
                    "entity.confidence holds `{stored}`, which is none of {}",
                    spelled(&Confidence::ALL)
                ))
            })
    }
}

fn spelled(all: &[Confidence]) -> String {
    all.iter()
        .map(|c| format!("`{}`", c.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The span a partial date covers, and how much of it was actually known.
///
/// Both bounds are always present: "sometime in 2019" is the whole of 2019,
/// not 1 January 2019. `precision` is what a timeline reads to draw a
/// year-wide band rather than a false point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateSpan {
    /// ISO-8601 UTC lower bound.
    pub start: String,
    /// ISO-8601 UTC upper bound.
    pub end: String,
    pub precision: DatePrecision,
}

/// One recorded value of one Property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyValue {
    /// The Property's name within the Entity's Schema.
    pub prop: String,
    /// The Property's type, as the Schema declares it.
    pub ptype: PropertyType,
    /// Where this value sits among that Property's values, from zero.
    pub seq: usize,
    /// Exactly what was entered. Never rewritten.
    pub value: String,
    /// The normalised form, for exact matching.
    pub value_norm: Option<String>,
    /// The numeric reading, for a `number` Property.
    pub numeric: Option<f64>,
    /// The parsed span, for a `date` Property.
    pub date: Option<DateSpan>,
    /// The Source of this value's earliest citation, or `None` when
    /// nothing cites it — which is what "this fact is unsourced" means at
    /// value granularity. Derived from the citations; see
    /// [`super::source`].
    pub source_id: Option<String>,
    /// The handle that cites this exact value: pass it back as
    /// [`super::CiteTarget::PropertyValue`], and match it against
    /// [`super::CitationTarget::Property`] to see what a citation is about.
    /// `None` only on a value that has not been stored yet.
    pub value_ref: Option<String>,
}

/// Something that looked wrong about a value that was stored anyway.
///
/// The obvious case is a check digit that does not verify: a NIP with a
/// transposed digit is recorded, because the document really does say that,
/// and flagged, because it is probably a typo. Warnings are derived from
/// the raw values, not stored, so improving a checksum rule changes what is
/// reported without a migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyWarning {
    /// The Property whose value was flagged.
    pub prop: String,
    /// The raw value that was flagged.
    pub value: String,
    /// A stable machine-readable reason.
    pub code: WarningCode,
    /// What looked wrong, in words.
    pub message: String,
}

/// An Entity as the rest of the system sees it: the node, its Properties,
/// its tags and anything that looked odd about its values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    /// ULID, 26 characters.
    pub id: String,
    /// The Case this Entity belongs to. Exactly one, always.
    pub case_id: String,
    /// The Schema it was created from.
    pub schema_id: String,
    /// The one-line label: derived from the Schema's template, or an
    /// override.
    pub caption: String,
    pub confidence: Confidence,
    /// Every value, in Schema-declared Property order and then in entry
    /// order within each Property.
    pub properties: Vec<PropertyValue>,
    /// The tags attached to it, by name, in name order.
    pub tags: Vec<String>,
    /// Anything flagged about the values above. Empty is the normal case.
    pub warnings: Vec<PropertyWarning>,
    /// Where this Entity's claims come from: the Sources cited for the
    /// Entity as a whole and for its individual Property values. Empty
    /// means unsourced — permitted, and flagged by
    /// [`Services::unsourced_facts`].
    pub citations: super::Citations,
    /// ISO-8601 UTC.
    pub created_at: String,
    /// Who recorded it — the analyst, or a named agent.
    pub created_by: String,
    /// ISO-8601 UTC.
    pub updated_at: String,
    /// Set when the id that was asked for is not [`Entity::id`]: that id
    /// named an Entity that has since been merged away, and this is the
    /// survivor it resolved to. A link written in a report six months ago
    /// still works, and says why it landed somewhere else.
    pub merged_from: Option<String>,
}

impl Entity {
    /// This Entity's values as the caption renderer and the resolver want
    /// them: raw values by Property name, in entry order.
    pub fn property_values(&self) -> PropertyValues {
        let mut values: PropertyValues = BTreeMap::new();
        for value in &self.properties {
            values
                .entry(value.prop.clone())
                .or_default()
                .push(value.value.clone());
        }
        values
    }

    /// Every value recorded for one Property, in entry order.
    pub fn values_of(&self, prop: &str) -> Vec<&str> {
        self.properties
            .iter()
            .filter(|v| v.prop == prop)
            .map(|v| v.value.as_str())
            .collect()
    }
}

/// An Entity as a list shows it: the node without its values.
///
/// Lists deliberately do not carry Properties. A page of fifty Entities
/// with every value of each would be an unbounded amount of data behind a
/// bounded row count, and the caption is what a list, a graph node or a
/// search result actually displays. Ask for the Entity by id to get its
/// Properties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitySummary {
    pub id: String,
    pub case_id: String,
    pub schema_id: String,
    pub caption: String,
    pub confidence: Confidence,
    pub created_at: String,
    pub updated_at: String,
}

/// A page of the Entity list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityList {
    pub entities: Vec<EntitySummary>,
    /// Whether there were more rows than the budget allowed. AGENT-BRIEF.md
    /// rule 5: a bounded list says whether it was cut short.
    pub truncated: bool,
    /// The budget actually applied, after clamping.
    pub limit: usize,
    pub offset: usize,
}

/// Everything needed to record a finding.
#[derive(Debug, Clone, Default)]
pub struct CreateEntity {
    /// The id to give it. `None` generates one. A caller supplying its own
    /// ULID can build a whole subgraph locally and submit it in one piece;
    /// creating the same id twice is a conflict rather than two Entities.
    pub id: Option<String>,
    /// The Case it belongs to.
    pub case_id: String,
    /// The Schema to instantiate. Abstract Schemas are refused, with a
    /// pointer to the concrete ones.
    pub schema_id: String,
    /// The values, by Property name, in the order they should be kept.
    pub properties: PropertyValues,
    /// A caption to use instead of the derived one.
    pub caption: Option<String>,
    /// `None` means [`Confidence::Probable`].
    pub confidence: Option<Confidence>,
}

/// A change to an Entity's label or confidence. Properties are edited
/// separately — see [`EditProperties`] — because adding a phone number and
/// renaming a node are different acts and read differently in the audit
/// trail.
#[derive(Debug, Clone, Default)]
pub struct UpdateEntity {
    /// [`Patch::Set`] overrides the caption, [`Patch::Clear`] puts it back
    /// under derivation from the Properties, [`Patch::Keep`] leaves it.
    pub caption: Patch<String>,
    /// `None` leaves the confidence alone.
    pub confidence: Option<Confidence>,
}

/// A change to an Entity's Properties, in the three shapes an edit actually
/// takes.
///
/// They are applied in the order below, which is the only order in which
/// they compose predictably:
///
/// 1. `replace` — the named Properties lose every value they had and take
///    the ones given. A Property named with no values is cleared.
/// 2. `remove` — specific values disappear, matched on what was entered or
///    on its normalised form, so `+48 601 234 567` removes a value stored
///    as `0601234567` in a Polish Case.
/// 3. `add` — values are appended after whatever is already there, keeping
///    their order.
#[derive(Debug, Clone, Default)]
pub struct EditProperties {
    pub replace: PropertyValues,
    pub remove: PropertyValues,
    pub add: PropertyValues,
}

impl EditProperties {
    /// Whether this asks for nothing at all.
    pub(super) fn is_empty(&self) -> bool {
        self.replace.is_empty() && self.remove.is_empty() && self.add.is_empty()
    }
}

/// Which Entities [`Services::list_entities`] returns. Soft-deleted and
/// merged-away Entities are never included.
#[derive(Debug, Clone, Default)]
pub struct EntityQuery {
    /// The Case to list within.
    pub case_id: String,
    /// Only Entities of this Schema.
    pub schema_id: Option<String>,
    /// Only Entities at this confidence level.
    pub confidence: Option<Confidence>,
    /// Only Entities carrying this tag, matched case-insensitively.
    pub tag: Option<String>,
    /// Row budget. `None` uses [`super::ServiceConfig::max_page_size`];
    /// anything larger is clamped to it.
    pub limit: Option<usize>,
    /// Rows to skip.
    pub offset: usize,
}

/// A request to destroy one Entity.
#[derive(Debug, Clone)]
pub struct PurgeEntity {
    pub entity_id: String,
    /// Must be `true`. Purging cannot happen by accident, by omission, or
    /// by a transport defaulting a missing field to something convenient.
    pub confirm: bool,
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

impl Services {
    /// Records a finding: a new Entity of one Schema, with its Properties.
    ///
    /// Everything that can be checked without reading the database is
    /// checked before the transaction opens. Normalisation happens inside
    /// it, because what a value means depends on the Case's region and the
    /// Case is read there — a rejection rolls the transaction back having
    /// written nothing, which is the same promise by a different route.
    pub async fn create_entity(
        &self,
        request: CreateEntity,
        actor: &Actor,
    ) -> Result<Entity, ServiceError> {
        let schema = self
            .schemas()
            .require_instantiable(&request.schema_id)
            .map_err(|e| ServiceError::from_schema("schema_id", e))?
            .clone();
        let id = match request.id {
            Some(id) => validate_id(&id)?,
            None => vesica_store::new_id(),
        };
        let caption_override = validate_caption(request.caption.as_deref())?;
        let confidence = request.confidence.unwrap_or_default();
        let flat = check_shape(&schema, &request.properties)?;
        require_required(&schema, flat.iter().map(|v| v.prop.as_str()))?;

        let case_id = request.case_id;
        let now = vesica_store::now();
        let actor = actor.as_str().to_string();

        self.tx(move |conn| {
            Box::pin(async move {
                let ctx = live_case_context(conn, &case_id).await?;
                let prepared = normalize_all(flat, &ctx)?;

                let caption = match &caption_override {
                    Some(caption) => caption.clone(),
                    None => derive_caption(&schema, &prepared),
                };
                let row = EntityRow {
                    id,
                    case_id,
                    schema_id: schema.id.clone(),
                    caption,
                    confidence: confidence.as_str().to_string(),
                    extra: None,
                    created_at: now.clone(),
                    created_by: actor.clone(),
                    updated_at: now,
                    deleted_at: None,
                    merged_into: None,
                };

                let after = assemble(
                    row.clone(),
                    prepared
                        .iter()
                        .enumerate()
                        .map(|(index, value)| {
                            value.as_prop_row(&row.id, seq_within(&prepared, index))
                        })
                        .collect(),
                    Vec::new(),
                    &schema,
                    &ctx,
                    None,
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
                entity::insert(conn, &row)
                    .await
                    .map_err(|e| duplicate_id_conflict(&row.id, e))?;
                for (index, value) in prepared.iter().enumerate() {
                    let new = value.as_new_prop(&row.id, seq_within(&prepared, index));
                    entity::insert_prop(conn, &new).await?;
                }

                load(conn, row, &schema, &ctx, None).await
            })
        })
        .await
    }

    /// The Entity with this id, with its Properties and tags.
    ///
    /// An id that names an Entity which was merged away resolves to the
    /// surviving Entity, with [`Entity::merged_from`] saying which id was
    /// asked for. [`ServiceError::NotFound`] if there is no such Entity or
    /// it has been deleted.
    pub async fn get_entity(&self, id: &str) -> Result<Entity, ServiceError> {
        let pool = self.db().read_pool();
        let mut row = entity::find(pool, id)
            .await?
            .filter(|row| row.deleted_at.is_none())
            .ok_or_else(|| ServiceError::not_found(TARGET_ENTITY, id))?;

        let mut merged_from = None;
        let mut hops = 0;
        while let Some(survivor) = row.merged_into.clone() {
            hops += 1;
            if hops > MAX_MERGE_HOPS {
                return Err(ServiceError::Inconsistent(format!(
                    "`{id}` leads through more than {MAX_MERGE_HOPS} merges without \
                     reaching a surviving entity"
                )));
            }
            merged_from = Some(row.id.clone());
            row = entity::find(pool, &survivor)
                .await?
                .filter(|row| row.deleted_at.is_none())
                .ok_or_else(|| {
                    ServiceError::Inconsistent(format!(
                        "`{}` was merged into `{survivor}`, which no longer exists",
                        row.id
                    ))
                })?;
        }

        let schema = self.schema_of(&row)?.clone();
        let ctx = case_context(pool, &row.case_id).await?;
        let props = entity::props(pool, &row.id).await?;
        let tags = tag::for_entity(pool, &row.id).await?;
        let id = row.id.clone();
        let mut entity = assemble(row, props, tags, &schema, &ctx, merged_from)?;
        // Citations are two more queries, and the helper that runs them
        // needs one connection to run both on.
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| ServiceError::Store(vesica_store::StoreError::from(e)))?;
        entity.citations = super::source::citations_of_entity(&mut conn, &id).await?;
        Ok(entity)
    }

    /// A bounded page of Entities in one Case, newest first.
    pub async fn list_entities(&self, query: EntityQuery) -> Result<EntityList, ServiceError> {
        let limit = self.page_size(query.limit);
        let filter = EntityFilter {
            case_id: query.case_id,
            schema_id: query.schema_id,
            confidence: query.confidence.map(|c| c.as_str().to_string()),
            tag: query.tag.map(|t| t.trim().to_string()),
            // One more than the budget, to learn whether there was more
            // without a second count query.
            limit: (limit as i64).saturating_add(1),
            offset: query.offset as i64,
        };
        let mut rows = entity::list(self.db().read_pool(), &filter).await?;
        let truncated = rows.len() > limit;
        rows.truncate(limit);

        Ok(EntityList {
            entities: rows
                .into_iter()
                .map(summary)
                .collect::<Result<Vec<_>, _>>()?,
            truncated,
            limit,
            offset: query.offset,
        })
    }

    /// Changes an Entity's caption or confidence.
    pub async fn update_entity(
        &self,
        id: &str,
        request: UpdateEntity,
        actor: &Actor,
    ) -> Result<Entity, ServiceError> {
        let caption = match request.caption.resolve() {
            Some(Some(caption)) => Some(Some(require_caption(&caption)?)),
            Some(None) => Some(None),
            None => None,
        };
        let confidence = request.confidence;
        let id = id.to_string();
        let now = vesica_store::now();
        let actor = actor.as_str().to_string();
        let schemas = self.schemas_handle();

        self.tx(move |conn| {
            Box::pin(async move {
                let mut row = live_entity(conn, &id).await?;
                let schema = schema_for(&schemas, &row)?.clone();
                let ctx = case_context(&mut *conn, &row.case_id).await?;
                let props = entity::props(&mut *conn, &row.id).await?;
                let tags = tag::for_entity(&mut *conn, &row.id).await?;
                let mut before = assemble(
                    row.clone(),
                    props.clone(),
                    tags.clone(),
                    &schema,
                    &ctx,
                    None,
                )?;
                before.citations = super::source::citations_of_entity(conn, &before.id).await?;

                let caption_before = row.caption.clone();
                let confidence_before = row.confidence.clone();

                match &caption {
                    Some(Some(caption)) => row.caption = caption.clone(),
                    Some(None) => {
                        row.caption = render_entity_caption(&schema, &before.property_values())
                    }
                    None => {}
                }
                if let Some(confidence) = confidence {
                    row.confidence = confidence.as_str().to_string();
                }

                // Decide "did anything change" on the fields this use case
                // can actually change, and decide it BEFORE `updated_at` is
                // touched. Comparing the assembled Entities instead would
                // compare `updated_at` too, so a genuine no-op would look
                // like a change for no reason other than that the clock had
                // moved on since the row was written — which is to say
                // almost always, making the check silently useless and the
                // audit trail noisy with events that record nothing.
                if row.caption == caption_before && row.confidence == confidence_before {
                    return Ok(before);
                }

                row.updated_at = now;
                let mut after = assemble(row.clone(), props, tags, &schema, &ctx, None)?;
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
                entity::save(conn, &row).await?;
                Ok(after)
            })
        })
        .await
    }

    /// Adds, replaces and removes Property values in one act.
    ///
    /// The resulting Entity has to satisfy its Schema — a Property that may
    /// not repeat still may not repeat afterwards, and a required Property
    /// cannot be emptied — so an edit that would leave it invalid is
    /// refused whole rather than applied in part.
    ///
    /// If the caption was never overridden it is re-derived from the new
    /// values; if it was, it is left alone. See the module docs.
    pub async fn edit_entity_properties(
        &self,
        id: &str,
        request: EditProperties,
        actor: &Actor,
    ) -> Result<Entity, ServiceError> {
        let id = id.to_string();
        let now = vesica_store::now();
        let actor = actor.as_str().to_string();
        let schemas = self.schemas_handle();

        self.tx(move |conn| {
            Box::pin(async move {
                let mut row = live_entity(conn, &id).await?;
                let schema = schema_for(&schemas, &row)?.clone();
                let ctx = case_context(&mut *conn, &row.case_id).await?;
                let existing = entity::props(&mut *conn, &row.id).await?;
                let tags = tag::for_entity(&mut *conn, &row.id).await?;
                let mut before = assemble(
                    row.clone(),
                    existing.clone(),
                    tags.clone(),
                    &schema,
                    &ctx,
                    None,
                )?;
                before.citations = super::source::citations_of_entity(conn, &before.id).await?;

                if request.is_empty() {
                    return Ok(before);
                }

                let seen: Vec<ExistingValue> = existing.iter().map(ExistingValue::of).collect();
                let plan = plan_edit(&schema, &seen, &request, &ctx)?;
                if plan.is_noop() {
                    return Ok(before);
                }

                // The caption follows the values it was derived from,
                // unless somebody named this node deliberately.
                let derived_before = render_entity_caption(&schema, &before.property_values());
                let was_derived = row.caption == derived_before;

                let mut kept: Vec<PropRow> = existing
                    .iter()
                    .filter(|p| !plan.delete_rows.contains(&p.id))
                    .cloned()
                    .collect();
                kept.extend(
                    plan.inserts
                        .iter()
                        .map(|planned| planned.value.as_prop_row(&row.id, planned.seq)),
                );
                sort_props(&mut kept);

                if was_derived {
                    row.caption = render_entity_caption(&schema, &raw_values_of(&kept));
                }
                row.updated_at = now;

                let after = assemble(row.clone(), kept, tags, &schema, &ctx, None)?;
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
                    entity::delete_props_named(conn, &row.id, prop).await?;
                }
                for row_id in &plan.delete_rows {
                    entity::delete_prop(conn, *row_id).await?;
                }
                for planned in &plan.inserts {
                    let new = planned.value.as_new_prop(&row.id, planned.seq);
                    entity::insert_prop(conn, &new).await?;
                }
                entity::save(conn, &row).await?;

                load(conn, row, &schema, &ctx, None).await
            })
        })
        .await
    }

    /// Deletes an Entity, reversibly: it disappears from every read and
    /// [`Services::restore_entity`] brings it back exactly as it was, tags
    /// and Properties included.
    ///
    /// **Every Relation touching it goes too**, in the same transaction, so
    /// that the graph never contains an edge to a node that is not there
    /// (ticket 0001-10). Those cascaded deletions share a `batch_id` with
    /// this one — which is exactly what `04-data-model.md` says a batch is
    /// for, "groups related changes for undo" — so that restoring the Entity
    /// can bring back the edges *this* deletion took down and leave alone
    /// any that were deleted separately beforehand.
    ///
    /// Deleting an already-deleted Entity changes nothing and records
    /// nothing; deleting one that never existed is
    /// [`ServiceError::NotFound`].
    pub async fn delete_entity(&self, id: &str, actor: &Actor) -> Result<(), ServiceError> {
        let id = id.to_string();
        let now = vesica_store::now();
        let actor = actor.as_str().to_string();
        let schemas = self.schemas_handle();

        self.tx(move |conn| {
            Box::pin(async move {
                let row = entity::find(&mut *conn, &id)
                    .await?
                    .ok_or_else(|| ServiceError::not_found(TARGET_ENTITY, &id))?;
                if row.deleted_at.is_some() {
                    return Ok(());
                }
                let schema = schema_for(&schemas, &row)?.clone();
                let ctx = case_context(&mut *conn, &row.case_id).await?;
                let props = entity::props(&mut *conn, &row.id).await?;
                let tags = tag::for_entity(&mut *conn, &row.id).await?;
                let before = assemble(row.clone(), props, tags, &schema, &ctx, None)?;

                let mut deleted = row;
                deleted.deleted_at = Some(now.clone());
                deleted.updated_at = now.clone();

                let batch_id = vesica_store::new_id();
                let mut entry = entry(
                    &actor,
                    AuditAction::Delete,
                    &deleted.case_id,
                    &deleted.id,
                    snapshot(&before),
                    None,
                );
                entry.batch_id = Some(batch_id.clone());
                audit::record(conn, entry).await?;
                entity::save(conn, &deleted).await?;
                super::relation::cascade_delete(conn, &deleted.id, &now, &actor, &batch_id).await?;
                Ok(())
            })
        })
        .await
    }

    /// Brings a soft-deleted Entity back, along with the Relations that
    /// this Entity's deletion took down — and only those. A Relation
    /// deleted on its own beforehand belongs to a different batch and stays
    /// deleted, because the analyst deleted it for a reason of its own.
    ///
    /// Restoring one that is not deleted changes nothing and records
    /// nothing.
    pub async fn restore_entity(&self, id: &str, actor: &Actor) -> Result<Entity, ServiceError> {
        let id = id.to_string();
        let now = vesica_store::now();
        let actor = actor.as_str().to_string();
        let schemas = self.schemas_handle();

        self.tx(move |conn| {
            Box::pin(async move {
                let mut row = entity::find(&mut *conn, &id)
                    .await?
                    .ok_or_else(|| ServiceError::not_found(TARGET_ENTITY, &id))?;
                let schema = schema_for(&schemas, &row)?.clone();
                let ctx = case_context(&mut *conn, &row.case_id).await?;
                if row.deleted_at.is_none() {
                    return load(conn, row, &schema, &ctx, None).await;
                }

                // Which deletion is being undone, read before the row is
                // changed: its batch names the edges that went with it.
                let batch_id = audit::latest_batch_for(
                    &mut *conn,
                    TARGET_ENTITY,
                    &row.id,
                    AuditAction::Delete.as_str(),
                )
                .await?;

                row.deleted_at = None;
                row.updated_at = now.clone();

                let props = entity::props(&mut *conn, &row.id).await?;
                let tags = tag::for_entity(&mut *conn, &row.id).await?;
                let mut after = assemble(row.clone(), props, tags, &schema, &ctx, None)?;

                let mut entry = entry(
                    &actor,
                    AuditAction::Restore,
                    &row.case_id,
                    &row.id,
                    None,
                    snapshot(&after),
                );
                entry.batch_id = batch_id.clone();
                audit::record(conn, entry).await?;
                entity::save(conn, &row).await?;
                if let Some(batch_id) = batch_id {
                    super::relation::cascade_restore(conn, &batch_id, &now, &actor).await?;
                }
                after.citations = super::source::citations_of_entity(conn, &after.id).await?;
                Ok(after)
            })
        })
        .await
    }

    /// Destroys one Entity: its Properties, its tags and, by cascade, every
    /// Relation touching it. Irreversible.
    ///
    /// Refused unless [`PurgeEntity::confirm`] is set. An Entity need not be
    /// deleted first.
    ///
    /// # What the audit trail keeps
    ///
    /// The same bargain a Case purge strikes, at Entity scope: the purge's
    /// own entry records who purged what and when and carries no snapshot,
    /// and every earlier entry *about this Entity* keeps its id, timestamp,
    /// actor, action, target and batch while its `before`/`after` payload is
    /// replaced by a redaction marker — because those payloads are copies of
    /// exactly the data the purge was asked to erase
    /// (`09-security-and-legal.md`: the audit log retains "only the fact
    /// that a deletion occurred").
    ///
    /// See [`vesica_store::repo::audit::redact_target_payloads`], which with
    /// its case-scoped sibling is the whole of this codebase's ability to
    /// alter an audit entry.
    pub async fn purge_entity(
        &self,
        request: PurgeEntity,
        actor: &Actor,
    ) -> Result<(), ServiceError> {
        if !request.confirm {
            return Err(ServiceError::ConfirmationRequired {
                operation: "purging an entity",
                fix: "re-send the request with `confirm` set to true; \
                      to remove an entity reversibly instead, delete it"
                    .to_string(),
            });
        }

        let id = request.entity_id;
        let actor = actor.as_str().to_string();
        let marker = redaction_marker(&vesica_store::now());

        self.tx(move |conn| {
            Box::pin(async move {
                let row = entity::find(&mut *conn, &id)
                    .await?
                    .ok_or_else(|| ServiceError::not_found(TARGET_ENTITY, &id))?;

                // Recorded before the redaction so that the purge's own
                // entry — which has no payload — is not itself a candidate
                // for it, and reads as the last thing that happened here.
                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Purge,
                        &row.case_id,
                        &row.id,
                        None,
                        None,
                    ),
                )
                .await?;
                audit::redact_target_payloads(&mut *conn, TARGET_ENTITY, &row.id, &marker).await?;
                entity::purge(conn, &row.id).await?;
                Ok(())
            })
        })
        .await
    }

    /// The audit trail for one Entity, newest first, bounded.
    ///
    /// Like the Case trail this does not require the Entity to be visible:
    /// the trail's purpose is to outlive what it describes, and refusing to
    /// show the history of a deleted or purged Entity would make "prove the
    /// deletion was recorded" unanswerable. An unknown id yields an empty
    /// page rather than an error.
    pub async fn entity_audit(
        &self,
        id: &str,
        limit: Option<usize>,
    ) -> Result<super::AuditPage, ServiceError> {
        self.audit_for_target(TARGET_ENTITY, id, limit).await
    }

    fn schema_of(&self, row: &EntityRow) -> Result<&EntitySchema, ServiceError> {
        self.schemas().entity(&row.schema_id).ok_or_else(|| {
            ServiceError::Inconsistent(format!(
                "entity `{}` names schema `{}`, which the registry does not have",
                row.id, row.schema_id
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// One value that has passed the checks that do not need the Case.
#[derive(Debug, Clone)]
pub(super) struct FlatValue {
    pub(super) prop: String,
    pub(super) ptype: PropertyType,
    pub(super) raw: String,
}

/// One value, normalised against the Case's region.
///
/// Shared with the Relation use cases: a Relation's Properties are the same
/// kind of thing as an Entity's and go through exactly this pipeline —
/// [`check_shape`], [`require_required`], [`normalize_all`], [`plan_edit`] —
/// so that `+48 601 234 567` means the same thing on an edge as it does on a
/// node. Only the row type the value is finally written as differs, and that
/// is the caller's job.
#[derive(Debug, Clone)]
pub(super) struct Prepared {
    pub(super) prop: String,
    pub(super) ptype: PropertyType,
    pub(super) normalized: Normalized,
}

/// What the shared Property pipeline needs to know about a Schema.
///
/// Entity Schemas and relation Schemas declare their Properties in exactly
/// the same shape ([`PropertyDef`]), and ticket 0001-10 requires a
/// Relation's Properties to be "normalised on the same code path as Entity
/// Properties". This trait is that code path's one point of variation: a
/// second implementation of multiplicity checking, required-Property
/// checking or edit planning would be two places for the rules to drift
/// apart, which is the defect this exists to prevent.
pub(super) trait PropertyHost {
    /// The Schema's stable id, for error messages.
    fn schema_id(&self) -> &str;
    /// Every Property this Schema declares, in declaration order.
    fn declared(&self) -> &[PropertyDef];
    /// One Property by name.
    fn declared_property(&self, name: &str) -> Option<&PropertyDef>;
}

impl PropertyHost for EntitySchema {
    fn schema_id(&self) -> &str {
        &self.id
    }
    fn declared(&self) -> &[PropertyDef] {
        &self.properties
    }
    fn declared_property(&self, name: &str) -> Option<&PropertyDef> {
        self.property(name)
    }
}

impl PropertyHost for RelationSchema {
    fn schema_id(&self) -> &str {
        &self.id
    }
    fn declared(&self) -> &[PropertyDef] {
        &self.properties
    }
    fn declared_property(&self, name: &str) -> Option<&PropertyDef> {
        self.property(name)
    }
}

/// One value already stored, as the edit planner needs to see it.
///
/// The planner works on this rather than on a row type so that it plans an
/// edit to an Entity's Properties and an edit to a Relation's the same way,
/// even though the two are stored in tables with different columns. The
/// caller maps its own rows in and maps the plan's results back out.
#[derive(Debug, Clone)]
pub(super) struct ExistingValue {
    /// The stored row's id, so the plan can name a specific value to delete.
    pub(super) id: i64,
    pub(super) prop: String,
    pub(super) seq: i64,
    pub(super) value: String,
    pub(super) value_norm: Option<String>,
}

impl ExistingValue {
    fn of(row: &PropRow) -> Self {
        ExistingValue {
            id: row.id,
            prop: row.prop.clone(),
            seq: row.seq,
            value: row.value.clone(),
            value_norm: row.value_norm.clone(),
        }
    }
}

impl Prepared {
    /// The [`ExistingValue`] this value will look like once it is stored,
    /// for the planner's own bookkeeping. `id` is zero because nothing has
    /// been written yet and nothing reads it from here.
    fn as_existing(&self, seq: i64) -> ExistingValue {
        ExistingValue {
            id: 0,
            prop: self.prop.clone(),
            seq,
            value: self.normalized.raw.clone(),
            value_norm: Some(self.normalized.value.clone()),
        }
    }
}

impl Prepared {
    fn as_new_prop(&self, entity_id: &str, seq: i64) -> NewProp {
        let (date_start, date_end, date_prec) = match &self.normalized.date {
            Some(date) => (
                Some(date.lower_iso()),
                Some(date.upper_iso()),
                Some(date.precision().as_str().to_string()),
            ),
            None => (None, None, None),
        };
        NewProp {
            entity_id: entity_id.to_string(),
            prop: self.prop.clone(),
            ptype: self.ptype.as_str().to_string(),
            seq,
            value: self.normalized.raw.clone(),
            value_norm: Some(self.normalized.value.clone()),
            value_num: self.normalized.numeric,
            date_start,
            date_end,
            date_prec,
        }
    }

    /// The row this value will become, for building the "after" snapshot
    /// before the insert has happened. `id` is zero because SQLite has not
    /// assigned one yet and nothing reads it from here.
    fn as_prop_row(&self, entity_id: &str, seq: i64) -> PropRow {
        let new = self.as_new_prop(entity_id, seq);
        PropRow {
            id: 0,
            entity_id: new.entity_id,
            prop: new.prop,
            ptype: new.ptype,
            seq: new.seq,
            value: new.value,
            value_norm: new.value_norm,
            value_num: new.value_num,
            date_start: new.date_start,
            date_end: new.date_end,
            date_prec: new.date_prec,
            source_id: None,
        }
    }
}

/// Checks every value against the Schema without needing the Case: the
/// Property exists, it may hold this many values, and each one is a
/// plausible length. Returns the values flattened into Schema-declared
/// Property order, which is the order they are stored and read back in.
pub(super) fn check_shape(
    schema: &impl PropertyHost,
    values: &PropertyValues,
) -> Result<Vec<FlatValue>, ServiceError> {
    for name in values.keys() {
        if schema.declared_property(name).is_none() {
            return Err(unknown_property(schema, name));
        }
    }

    let mut flat = Vec::new();
    for def in schema.declared() {
        let Some(entered) = values.get(&def.name) else {
            continue;
        };
        check_multiplicity(schema, &def.name, entered.len())?;
        for raw in entered {
            if raw.chars().count() > MAX_VALUE_LEN {
                return Err(ServiceError::validation(
                    &def.name,
                    format!(
                        "a value of `{}` may be at most {MAX_VALUE_LEN} characters",
                        def.name
                    ),
                    format!(
                        "shorten the value to {MAX_VALUE_LEN} characters or fewer, or attach \
                         the long text as a source excerpt instead"
                    ),
                ));
            }
            flat.push(FlatValue {
                prop: def.name.clone(),
                ptype: def.ptype,
                raw: raw.clone(),
            });
        }
    }
    Ok(flat)
}

fn check_multiplicity(
    schema: &impl PropertyHost,
    prop: &str,
    count: usize,
) -> Result<(), ServiceError> {
    let Some(def) = schema.declared_property(prop) else {
        return Err(unknown_property(schema, prop));
    };
    if !def.multiple && count > 1 {
        return Err(ServiceError::validation(
            prop,
            format!(
                "`{prop}` of `{}` holds one value, and {count} were given",
                schema.schema_id()
            ),
            format!("send a single value for `{prop}`"),
        ));
    }
    if count > MAX_VALUES_PER_PROPERTY {
        return Err(ServiceError::validation(
            prop,
            format!(
                "`{prop}` may hold at most {MAX_VALUES_PER_PROPERTY} values, and {count} were given"
            ),
            format!(
                "send at most {MAX_VALUES_PER_PROPERTY} values for `{prop}`; if there are \
                 genuinely more, they probably belong on separate entities"
            ),
        ));
    }
    Ok(())
}

/// Checks that every required Property of the Schema has at least one value
/// in the state described by `present`.
pub(super) fn require_required<'a>(
    schema: &impl PropertyHost,
    present: impl Iterator<Item = &'a str>,
) -> Result<(), ServiceError> {
    let names: Vec<&str> = present.collect();
    for def in schema.declared() {
        if def.required && !names.contains(&def.name.as_str()) {
            return Err(ServiceError::validation(
                &def.name,
                format!(
                    "`{}` requires at least one `{}`",
                    schema.schema_id(),
                    def.name
                ),
                format!(
                    "include `{}` in the properties, e.g. {{\"{}\": [\"…\"]}}",
                    def.name, def.name
                ),
            ));
        }
    }
    Ok(())
}

fn unknown_property(schema: &impl PropertyHost, name: &str) -> ServiceError {
    let known: Vec<&str> = schema.declared().iter().map(|p| p.name.as_str()).collect();
    ServiceError::validation(
        name,
        format!("`{}` has no property named `{name}`", schema.schema_id()),
        format!("use one of: {}", known.join(", ")),
    )
}

/// Normalises every value against the Case's region. This is where a
/// national phone number becomes E.164, an email is folded, a date becomes
/// a span with a precision, and a bad check digit becomes a warning rather
/// than a refusal.
pub(super) fn normalize_all(
    flat: Vec<FlatValue>,
    ctx: &NormalizeContext,
) -> Result<Vec<Prepared>, ServiceError> {
    flat.into_iter()
        .map(|value| {
            let normalized = value
                .ptype
                .normalize(&value.raw, ctx)
                .map_err(|e| ServiceError::from_core(&value.prop, e))?;
            Ok(Prepared {
                prop: value.prop,
                ptype: value.ptype,
                normalized,
            })
        })
        .collect()
}

pub(super) fn validate_id(id: &str) -> Result<String, ServiceError> {
    let trimmed = id.trim();
    if vesica_store::is_valid_id(trimmed) {
        Ok(trimmed.to_string())
    } else {
        Err(ServiceError::validation(
            "id",
            format!("`{trimmed}` is not a ULID"),
            "supply a 26-character Crockford-base32 ULID, or omit the field \
             and one will be generated",
        ))
    }
}

pub(super) fn validate_caption(caption: Option<&str>) -> Result<Option<String>, ServiceError> {
    let Some(caption) = caption else {
        return Ok(None);
    };
    let trimmed = vesica_core::folding::collapse_whitespace(caption);
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(require_caption(&trimmed)?))
}

pub(super) fn require_caption(caption: &str) -> Result<String, ServiceError> {
    let trimmed = vesica_core::folding::collapse_whitespace(caption);
    if trimmed.is_empty() {
        return Err(ServiceError::validation(
            "caption",
            "a caption override cannot be empty",
            "send the label this node should carry, or clear the caption to \
             have it derived from the properties again",
        ));
    }
    if trimmed.chars().count() > MAX_CAPTION_LEN {
        return Err(ServiceError::validation(
            "caption",
            format!("a caption may be at most {MAX_CAPTION_LEN} characters"),
            format!("shorten the caption to {MAX_CAPTION_LEN} characters or fewer"),
        ));
    }
    Ok(trimmed)
}

// ---------------------------------------------------------------------------
// Editing properties
// ---------------------------------------------------------------------------

/// One value the plan will write: the normalised value and the `seq` it
/// takes within its Property. Turning it into a storable row is the
/// caller's job, because an Entity's Properties and a Relation's live in
/// tables with different columns.
#[derive(Debug, Clone)]
pub(super) struct PlannedValue {
    pub(super) value: Prepared,
    pub(super) seq: i64,
}

/// What an edit will do, worked out in full before anything is written so
/// that an edit which would leave the Entity or Relation invalid is refused
/// whole.
#[derive(Debug, Default)]
pub(super) struct EditPlan {
    /// Properties whose every value goes (a `replace`).
    pub(super) clear_props: Vec<String>,
    /// Individual stored rows that go (a `remove`).
    pub(super) delete_rows: Vec<i64>,
    /// Values to write, each with the `seq` it takes.
    pub(super) inserts: Vec<PlannedValue>,
}

impl EditPlan {
    pub(super) fn is_noop(&self) -> bool {
        self.clear_props.is_empty() && self.delete_rows.is_empty() && self.inserts.is_empty()
    }
}

/// Works out, without writing anything, what an [`EditProperties`] request
/// does to a set of stored values — and refuses the whole request if the
/// result would not satisfy its Schema.
///
/// Shared by the Entity and Relation use cases; see [`PropertyHost`].
pub(super) fn plan_edit(
    schema: &impl PropertyHost,
    existing: &[ExistingValue],
    request: &EditProperties,
    ctx: &NormalizeContext,
) -> Result<EditPlan, ServiceError> {
    for values in [&request.replace, &request.remove, &request.add] {
        for name in values.keys() {
            if schema.declared_property(name).is_none() {
                return Err(unknown_property(schema, name));
            }
        }
    }

    let mut plan = EditPlan::default();
    // Which stored rows survive, per property, in order.
    let mut surviving: BTreeMap<String, Vec<ExistingValue>> = BTreeMap::new();
    for row in existing {
        surviving
            .entry(row.prop.clone())
            .or_default()
            .push(row.clone());
    }

    // 1. Replace: the property loses everything it had.
    for prop in request.replace.keys() {
        if surviving.remove(prop).is_some_and(|rows| !rows.is_empty()) {
            plan.clear_props.push(prop.clone());
        }
    }

    // 2. Remove: specific values, matched on what was entered or on its
    //    normalised form.
    for (prop, values) in &request.remove {
        let Some(rows) = surviving.get_mut(prop) else {
            continue;
        };
        for raw in values {
            let normalized = schema
                .declared_property(prop)
                .and_then(|def| def.ptype.normalize(raw, ctx).ok())
                .map(|n| n.value);
            if let Some(index) = rows.iter().position(|row| {
                row.value == *raw || (normalized.is_some() && row.value_norm == normalized)
            }) {
                plan.delete_rows.push(rows.remove(index).id);
            }
        }
    }

    // 3. Add and the replacement values, appended after what survives.
    let mut incoming: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (prop, values) in request.replace.iter().chain(request.add.iter()) {
        incoming
            .entry(prop.clone())
            .or_default()
            .extend(values.iter().cloned());
    }

    for (prop, values) in &incoming {
        let surviving_count = surviving.get(prop).map_or(0, |rows| rows.len());
        check_multiplicity(schema, prop, surviving_count + values.len())?;
    }
    // A property that only lost values still has to satisfy its schema.
    for prop in request.remove.keys() {
        if !incoming.contains_key(prop) {
            let count = surviving.get(prop).map_or(0, |rows| rows.len());
            check_multiplicity(schema, prop, count)?;
        }
    }

    for (prop, values) in incoming {
        let ptype = schema
            .declared_property(&prop)
            .map(|def| def.ptype)
            .ok_or_else(|| unknown_property(schema, &prop))?;
        let flat: Vec<FlatValue> = values
            .into_iter()
            .map(|raw| FlatValue {
                prop: prop.clone(),
                ptype,
                raw,
            })
            .collect();
        let too_long = flat.iter().find(|v| v.raw.chars().count() > MAX_VALUE_LEN);
        if let Some(value) = too_long {
            return Err(ServiceError::validation(
                &value.prop,
                format!(
                    "a value of `{}` may be at most {MAX_VALUE_LEN} characters",
                    value.prop
                ),
                format!("shorten the value to {MAX_VALUE_LEN} characters or fewer"),
            ));
        }
        let prepared = normalize_all(flat, ctx)?;

        // Added values continue the Property's existing numbering rather
        // than restarting it, so `seq` stays the order the analyst entered
        // things in across separate edits.
        let base = surviving
            .get(&prop)
            .and_then(|rows| rows.iter().map(|row| row.seq).max())
            .map_or(0, |max| max + 1);
        for (offset, value) in prepared.into_iter().enumerate() {
            let seq = base + offset as i64;
            surviving
                .entry(prop.clone())
                .or_default()
                .push(value.as_existing(seq));
            plan.inserts.push(PlannedValue { value, seq });
        }
    }

    let present: Vec<&str> = surviving
        .iter()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(prop, _)| prop.as_str())
        .collect();
    require_required(schema, present.into_iter())?;

    Ok(plan)
}

// ---------------------------------------------------------------------------
// Assembly
// ---------------------------------------------------------------------------

/// Reads an Entity's Properties and tags back on whatever connection the
/// caller is using and builds the domain type — the "re-read inside the
/// transaction" step every mutating use case ends with.
async fn load(
    conn: &mut SqliteConnection,
    row: EntityRow,
    schema: &EntitySchema,
    ctx: &NormalizeContext,
    merged_from: Option<String>,
) -> Result<Entity, ServiceError> {
    let props = entity::props(&mut *conn, &row.id).await?;
    let tags = tag::for_entity(&mut *conn, &row.id).await?;
    let mut entity = assemble(row, props, tags, schema, ctx, merged_from)?;
    entity.citations = super::source::citations_of_entity(conn, &entity.id).await?;
    Ok(entity)
}

/// Turns stored rows into the domain type, deriving the warnings from the
/// raw values as it goes. Pure, so both the pooled read path and the
/// in-transaction write path use exactly the same assembly.
fn assemble(
    row: EntityRow,
    mut props: Vec<PropRow>,
    tags: Vec<vesica_store::repo::TagRow>,
    schema: &EntitySchema,
    ctx: &NormalizeContext,
    merged_from: Option<String>,
) -> Result<Entity, ServiceError> {
    sort_props_by_schema(&mut props, schema);

    let mut properties = Vec::with_capacity(props.len());
    let mut warnings = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();

    for prop in props {
        let ptype = prop.ptype.parse::<PropertyType>().map_err(|_| {
            ServiceError::Inconsistent(format!(
                "entity_prop.ptype holds `{}`, which is not a property type",
                prop.ptype
            ))
        })?;
        let seq = seen.entry(prop.prop.clone()).or_insert(0);
        let position = *seq;
        *seq += 1;

        // Warnings are derived, not stored: a checksum rule that improves
        // starts reporting differently without a migration. A value that no
        // longer normalises at all carries no warning rather than blocking
        // the read — the raw value is what matters and it is intact.
        if let Ok(normalized) = ptype.normalize(&prop.value, ctx) {
            warnings.extend(normalized.warnings.into_iter().map(|w| PropertyWarning {
                prop: prop.prop.clone(),
                value: prop.value.clone(),
                code: w.code,
                message: w.message,
            }));
        }

        let date = match (&prop.date_start, &prop.date_end, &prop.date_prec) {
            (Some(start), Some(end), Some(precision)) => {
                DatePrecision::parse(precision).map(|precision| DateSpan {
                    start: start.clone(),
                    end: end.clone(),
                    precision,
                })
            }
            _ => None,
        };

        // A row that has not been written yet has no id, and therefore no
        // handle a citation could name it by.
        let value_ref = (prop.id != 0)
            .then(|| store_source::prop_target_id(store_source::PropHost::Entity, prop.id));

        properties.push(PropertyValue {
            prop: prop.prop,
            ptype,
            seq: position,
            value: prop.value,
            value_norm: prop.value_norm,
            numeric: prop.value_num,
            date,
            source_id: prop.source_id,
            value_ref,
        });
    }

    Ok(Entity {
        confidence: Confidence::from_stored(&row.confidence)?,
        id: row.id,
        case_id: row.case_id,
        schema_id: row.schema_id,
        caption: row.caption,
        properties,
        tags: tags.into_iter().map(|t| t.name).collect(),
        warnings,
        // Filled by whoever is about to hand this Entity to a caller —
        // reading citations is a query, and `assemble` is pure. A snapshot
        // built for an audit entry does not need them and does not get
        // them.
        citations: super::Citations::default(),
        created_at: row.created_at,
        created_by: row.created_by,
        updated_at: row.updated_at,
        merged_from,
    })
}

fn summary(row: EntityRow) -> Result<EntitySummary, ServiceError> {
    Ok(EntitySummary {
        confidence: Confidence::from_stored(&row.confidence)?,
        id: row.id,
        case_id: row.case_id,
        schema_id: row.schema_id,
        caption: row.caption,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Orders values the way a reader expects them: Properties in the order the
/// Schema declares them, values within a Property in the order they were
/// entered. A Property the Schema no longer declares sorts last rather than
/// disappearing — the stored value is still the truth about what was
/// recorded.
fn sort_props_by_schema(props: &mut [PropRow], schema: &EntitySchema) {
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

fn sort_props(props: &mut [PropRow]) {
    props.sort_by(|a, b| {
        a.prop
            .cmp(&b.prop)
            .then_with(|| a.seq.cmp(&b.seq))
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn raw_values_of(props: &[PropRow]) -> PropertyValues {
    let mut values: PropertyValues = BTreeMap::new();
    for prop in props {
        values
            .entry(prop.prop.clone())
            .or_default()
            .push(prop.value.clone());
    }
    values
}

fn derive_caption(schema: &EntitySchema, prepared: &[Prepared]) -> String {
    let mut values: PropertyValues = BTreeMap::new();
    for value in prepared {
        values
            .entry(value.prop.clone())
            .or_default()
            .push(value.normalized.raw.clone());
    }
    render_entity_caption(schema, &values)
}

/// The `seq` a value takes within its own Property, given the flattened
/// order every value was prepared in.
pub(super) fn seq_within(prepared: &[Prepared], index: usize) -> i64 {
    prepared[..index]
        .iter()
        .filter(|earlier| earlier.prop == prepared[index].prop)
        .count() as i64
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Builds the domain type for a row already read inside a transaction,
/// resolving its Schema and its Case's region on the way. The convenience
/// the tag use cases reach for, so they do not each re-derive the two
/// things every Entity read needs.
pub(super) async fn load_row(
    conn: &mut SqliteConnection,
    row: EntityRow,
    schemas: &std::sync::Arc<vesica_core::SchemaRegistry>,
) -> Result<Entity, ServiceError> {
    let schema = schema_for(schemas, &row)?.clone();
    let ctx = case_context(&mut *conn, &row.case_id).await?;
    load(conn, row, &schema, &ctx, None).await
}

/// Reads an Entity that must be visible, inside the caller's transaction,
/// so that a read-modify-write sees the row it is about to overwrite. A
/// merged-away Entity is not editable: the surviving one is.
pub(super) async fn live_entity(
    conn: &mut SqliteConnection,
    id: &str,
) -> Result<EntityRow, ServiceError> {
    let row = entity::find(&mut *conn, id)
        .await?
        .filter(|row| row.deleted_at.is_none())
        .ok_or_else(|| ServiceError::not_found(TARGET_ENTITY, id))?;
    if let Some(survivor) = &row.merged_into {
        return Err(ServiceError::Conflict {
            message: format!("`{id}` was merged into `{survivor}` and can no longer be changed"),
            fix: format!("make the change on `{survivor}`, which is the surviving entity"),
        });
    }
    Ok(row)
}

/// The normalisation context for values recorded in a Case that must exist
/// and be visible. This is what the Case's default region is *for*.
pub(super) async fn live_case_context(
    conn: &mut SqliteConnection,
    case_id: &str,
) -> Result<NormalizeContext, ServiceError> {
    let row = case::find(&mut *conn, case_id)
        .await?
        .filter(|row| row.deleted_at.is_none())
        .ok_or_else(|| ServiceError::not_found("case", case_id))?;
    Ok(context_of(row.default_region.as_deref()))
}

/// As [`live_case_context`], for a Case that is already known to exist
/// because something inside it was just read.
pub(super) async fn case_context<'e, E>(
    exec: E,
    case_id: &str,
) -> Result<NormalizeContext, ServiceError>
where
    E: sqlx::sqlite::SqliteExecutor<'e>,
{
    let region = case::find(exec, case_id)
        .await?
        .and_then(|row| row.default_region);
    Ok(context_of(region.as_deref()))
}

fn context_of(region: Option<&str>) -> NormalizeContext {
    match region {
        Some(region) => NormalizeContext::with_region(region),
        None => NormalizeContext::new(),
    }
}

fn schema_for<'r>(
    schemas: &'r std::sync::Arc<vesica_core::SchemaRegistry>,
    row: &EntityRow,
) -> Result<&'r EntitySchema, ServiceError> {
    schemas.entity(&row.schema_id).ok_or_else(|| {
        ServiceError::Inconsistent(format!(
            "entity `{}` names schema `{}`, which the registry does not have",
            row.id, row.schema_id
        ))
    })
}

/// Renders an Entity for an audit entry's `before`/`after`.
///
/// The warnings are left out: they are derived from the values, which are
/// in the snapshot, so recording them would be recording the same fact
/// twice and pinning today's checksum rules into history.
pub(super) fn snapshot(entity: &Entity) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "id": entity.id,
        "case_id": entity.case_id,
        "schema_id": entity.schema_id,
        "caption": entity.caption,
        "confidence": entity.confidence,
        "properties": entity.property_values(),
        "tags": entity.tags,
    }))
    .ok()
}

pub(super) fn entry(
    actor: &str,
    action: AuditAction,
    case_id: &str,
    entity_id: &str,
    before: Option<String>,
    after: Option<String>,
) -> NewAuditEntry {
    NewAuditEntry {
        actor: actor.to_string(),
        action: action.as_str().to_string(),
        case_id: Some(case_id.to_string()),
        target_kind: TARGET_ENTITY.to_string(),
        target_id: entity_id.to_string(),
        before,
        after,
        batch_id: None,
    }
}

/// Turns the primary-key violation a caller-supplied duplicate id causes
/// into something that names the problem, and leaves every other store
/// error alone.
pub(super) fn duplicate_id_conflict(id: &str, e: vesica_store::StoreError) -> ServiceError {
    let message = e.to_string();
    if message.contains("UNIQUE constraint failed") {
        ServiceError::Conflict {
            message: format!("`{id}` is already taken by another entity"),
            fix: "omit the id and one will be generated, or supply an id that is not in use"
                .to_string(),
        }
    } else {
        ServiceError::Store(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vesica_core::SchemaRegistry;

    fn person() -> EntitySchema {
        SchemaRegistry::builtin().entity("Person").unwrap().clone()
    }

    fn values(pairs: &[(&str, &[&str])]) -> PropertyValues {
        pairs
            .iter()
            .map(|(k, vs)| {
                (
                    (*k).to_string(),
                    vs.iter().map(|v| (*v).to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn a_confidence_the_code_does_not_know_is_refused_with_the_ones_it_does() {
        assert_eq!(
            Confidence::parse("Confirmed").unwrap(),
            Confidence::Confirmed
        );
        assert_eq!(Confidence::default(), Confidence::Probable);
        let e = Confidence::parse("maybe").unwrap_err();
        assert!(e.fix().unwrap().contains("hypothesis"));
        assert!(matches!(
            Confidence::from_stored("who knows"),
            Err(ServiceError::Inconsistent(_))
        ));
    }

    #[test]
    fn an_unknown_property_names_itself_and_lists_the_real_ones() {
        let schema = person();
        let e = check_shape(&schema, &values(&[("phon", &["x"])])).unwrap_err();
        match &e {
            ServiceError::Validation { field, message, .. } => {
                assert_eq!(field, "phon");
                assert!(message.contains("Person"));
            }
            other => panic!("expected a validation error, got {other:?}"),
        }
        assert!(e.fix().unwrap().contains("phone"));
    }

    #[test]
    fn a_property_that_does_not_repeat_says_so_when_it_is_given_two_values() {
        let schema = person();
        let e = check_shape(&schema, &values(&[("birthDate", &["2019", "2020"])])).unwrap_err();
        match &e {
            ServiceError::Validation { field, .. } => assert_eq!(field, "birthDate"),
            other => panic!("expected a validation error, got {other:?}"),
        }
        assert!(e.fix().unwrap().contains("single value"));
    }

    #[test]
    fn values_come_back_in_the_order_the_schema_declares_the_properties() {
        let schema = person();
        let flat = check_shape(
            &schema,
            &values(&[("phone", &["1", "2"]), ("name", &["Jan"])]),
        )
        .unwrap();
        let order: Vec<&str> = flat.iter().map(|v| v.prop.as_str()).collect();
        assert_eq!(order, vec!["name", "phone", "phone"]);
        assert_eq!(flat[1].raw, "1", "values keep the order they arrived in");
    }

    #[test]
    fn a_missing_required_property_says_which_one_and_what_to_send() {
        let schema = person();
        let e = require_required(&schema, ["phone"].into_iter()).unwrap_err();
        match &e {
            ServiceError::Validation { field, .. } => assert_eq!(field, "name"),
            other => panic!("expected a validation error, got {other:?}"),
        }
        assert!(e.fix().unwrap().contains("name"));
        assert!(require_required(&schema, ["name"].into_iter()).is_ok());
    }

    #[test]
    fn a_value_longer_than_the_ceiling_is_refused() {
        let schema = person();
        let long = "x".repeat(MAX_VALUE_LEN + 1);
        let e = check_shape(&schema, &values(&[("notes", &[long.as_str()])])).unwrap_err();
        assert!(e.fix().is_some());
    }

    #[test]
    fn a_caption_override_is_collapsed_bounded_and_never_empty() {
        assert_eq!(
            validate_caption(Some("  The   Driver "))
                .unwrap()
                .as_deref(),
            Some("The Driver")
        );
        assert_eq!(validate_caption(Some("   ")).unwrap(), None);
        assert!(require_caption("  ").is_err());
        assert!(require_caption(&"x".repeat(MAX_CAPTION_LEN + 1)).is_err());
    }

    #[test]
    fn a_supplied_id_must_be_a_ulid() {
        let id = vesica_store::new_id();
        assert_eq!(validate_id(&id).unwrap(), id);
        assert!(validate_id("entity-1").is_err());
    }
}
