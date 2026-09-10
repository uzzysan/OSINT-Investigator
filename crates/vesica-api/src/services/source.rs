//! Provenance: Sources, citations, and the flagging of facts that have
//! neither.
//!
//! A **Source** is a record of where something was read: a page, a file
//! somebody holds, a note an analyst wrote, a run an agent made. It belongs
//! to exactly one Case, like everything else, and it records **when it was
//! retrieved** — which is not when the row was typed. A filing read in
//! March and entered in July is dated March, because that is when it said
//! what it said.
//!
//! A **citation** attaches a Source to a fact. The fact can be an Entity,
//! a Relation, or one value of one Property, because that is the
//! granularity claims are actually made at: the company registration came
//! from the registry extract and the phone number came from somewhere else,
//! and the difference is the first thing anyone asks about when a
//! conclusion is challenged (`09-security-and-legal.md`: provenance is what
//! makes a report defensible).
//!
//! Facts without a Source are **permitted and flagged**. Capture has to
//! stay fast, so nothing here refuses a fact for having no evidence behind
//! it; [`Services::unsourced_facts`] and [`Services::source_coverage`] are
//! how the gaps stay visible.
//!
//! # Two columns that had to be made to agree
//!
//! `entity_prop.source_id` and `relation_prop.source_id` hold **one**
//! source per value. The `citation` table holds **many**. Left alone they
//! would drift, and the answer to "is this value sourced" would depend on
//! which one you asked.
//!
//! The `citation` table is authoritative. The columns are a derived cache
//! of the *earliest surviving* citation on that value, recomputed by
//! [`vesica_store::repo::source::refresh_prop_source`] in the same
//! transaction as every change to that value's citations. Nothing else
//! writes them. That is what lets "which values are unsourced" be an
//! indexed `source_id IS NULL` — bounded, cheap, and safe to run on a Case
//! of any size — while a value keeps as many recorded claims as an analyst
//! cares to make about it.
//!
//! # A citation does not survive the value it cites
//!
//! `entity_prop.id` is a SQLite rowid and a Property edit is a delete and
//! an insert, not an update: replacing a phone number gives it a new row
//! and a new id. A citation of the old value is therefore deleted with it,
//! in the same transaction (see [`vesica_store::repo::entity::delete_prop`]).
//!
//! This is deliberate rather than convenient. A changed value is a
//! different claim, and silently carrying provenance across the change
//! would attach a source to a statement it never made — the one failure
//! mode a provenance feature must not have. The cost is that re-entering a
//! value re-sources it, and the alternative — keeping the citation on a
//! value that no longer exists — is worse in a way nobody would notice
//! until it mattered.
//!
//! # Blobs are not here
//!
//! `blob` and `attachment` exist in migration 0002 and stay empty. Spec
//! 0001 puts them out of scope in as many words: "Sources are recorded;
//! files are not uploaded, hashed or stored. M4." A `file` Source here
//! records a **reference** — a filename, a path, an evidence-bag number —
//! and nothing reads any bytes.

use serde::{Deserialize, Serialize};
use vesica_core::{DateStyle, PartialDate, PropertyType};
use vesica_store::SqliteConnection;
use vesica_store::repo::source::{
    self, CitationDetailRow, CoverageKind, CoverageRow, NewCitation, PropHost, SourceFilter,
    SourceRow,
};
use vesica_store::repo::{audit, case, entity, relation};

use super::Services;
use super::case::AuditAction;
use super::entity::{PropertyWarning, TARGET_ENTITY, validate_id};
use super::error::{Actor, ServiceError};
use super::relation::TARGET_RELATION;

/// What `audit_log.target_kind` says for a Source.
pub(super) const TARGET_SOURCE: &str = "source";

/// Longest a Source title may be. A title is a label on a piece of
/// evidence, not the evidence.
const MAX_TITLE_LEN: usize = 300;

/// Longest a quoted excerpt may be. Long enough for the paragraph that
/// actually says the thing, short enough that the evidence table does not
/// become a document store — which is what `blob` is for, in M4.
const MAX_EXCERPT_LEN: usize = 4_000;

/// Longest a file reference may be: a path, a filename, an evidence-bag
/// number.
const MAX_REFERENCE_LEN: usize = 1_000;

/// Longest the note on a citation may be.
const MAX_NOTE_LEN: usize = 1_000;

/// Most citations returned with one Entity or Relation, per level. A read
/// of a node carries at most this many of its own citations and this many
/// of its Property values' — bounded, with [`Citations::truncated`] saying
/// when the ceiling was reached. AGENT-BRIEF.md rule 5.
const MAX_CITATIONS_PER_TARGET: usize = 200;

/// Most citations listed against one Source in a Source listing. The count
/// beside them ([`SourceUsage::citation_count`]) is what says how many
/// there really are.
const MAX_CITATIONS_LISTED: usize = 20;

/// Where the `meta` JSON of a `file` Source keeps its reference.
const META_REFERENCE: &str = "reference";

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// What kind of thing a Source is.
///
/// The four the ticket names: a link, a file reference, a manual note, an
/// agent run. [`SourceKind::Link`] is stored as `url`, which is the
/// spelling `04-data-model.md` gives the column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// A page on the web. Requires a URL, which is normalised so that the
    /// same page cited twice is one Source.
    #[serde(rename = "url")]
    Link,
    /// A file the analyst holds — recorded by reference, not ingested. See
    /// the module docs.
    File,
    /// Something an analyst asserts from their own knowledge or from
    /// somewhere the system cannot address.
    Manual,
    /// A run of an agent: what it did, and when.
    Agent,
}

impl SourceKind {
    /// The string stored in `source.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Link => "url",
            SourceKind::File => "file",
            SourceKind::Manual => "manual",
            SourceKind::Agent => "agent",
        }
    }

    /// Every kind, for error messages.
    pub const ALL: [SourceKind; 4] = [
        SourceKind::Link,
        SourceKind::File,
        SourceKind::Manual,
        SourceKind::Agent,
    ];

    /// Reads a caller-supplied kind. `link` is accepted as a spelling of
    /// `url` because that is the word the domain uses for it; everything
    /// else is refused with a `fix` listing what is accepted.
    pub fn parse(value: &str) -> Result<Self, ServiceError> {
        let folded = value.trim().to_ascii_lowercase();
        if folded == "link" {
            return Ok(SourceKind::Link);
        }
        SourceKind::ALL
            .into_iter()
            .find(|k| k.as_str() == folded)
            .ok_or_else(|| {
                ServiceError::validation(
                    "kind",
                    format!("`{value}` is not a source kind"),
                    format!("send one of: {}", spelled()),
                )
            })
    }

    fn from_stored(stored: &str) -> Result<Self, ServiceError> {
        SourceKind::ALL
            .into_iter()
            .find(|k| k.as_str() == stored)
            .ok_or_else(|| {
                ServiceError::Inconsistent(format!(
                    "source.kind holds `{stored}`, which is none of {}",
                    spelled()
                ))
            })
    }
}

fn spelled() -> String {
    SourceKind::ALL
        .iter()
        .map(|k| format!("`{}`", k.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A Source as the rest of the system sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// ULID, 26 characters.
    pub id: String,
    /// The Case it belongs to. Exactly one, always.
    pub case_id: String,
    pub kind: SourceKind,
    pub title: Option<String>,
    /// The locator exactly as it was given. Never rewritten.
    pub url: Option<String>,
    /// The normalised locator: lower-cased scheme and host, no default
    /// port, no fragment, no tracking parameters. Two Sources for the same
    /// page in one Case are one Source because of this field.
    pub url_norm: Option<String>,
    /// A `file` Source's reference — a filename, a path, an evidence-bag
    /// number. Recorded, not read: see the module docs.
    pub reference: Option<String>,
    /// A quotation, in the source's own words.
    pub excerpt: Option<String>,
    /// When the Source was observed, ISO-8601 UTC. Distinct from
    /// [`Source::created_at`], which is when it was written down.
    pub retrieved_at: Option<String>,
    /// ISO-8601 UTC.
    pub created_at: String,
    /// Who recorded it — the analyst, or a named agent.
    pub created_by: String,
    /// Anything flagged about the locator — a URL given without a scheme,
    /// say. Derived on read from the raw value, never stored, exactly as an
    /// Entity's Property warnings are.
    pub warnings: Vec<PropertyWarning>,
}

/// What a citation points at.
///
/// A Property value is named by its `value_ref`, an opaque handle that
/// [`super::PropertyValue::value_ref`] also carries — that is how a caller
/// matches a citation to the value it is about without depending on how
/// either is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CitationTarget {
    /// The whole Entity: everything it asserts came from here.
    Entity { entity_id: String },
    /// The whole Relation.
    Relation { relation_id: String },
    /// One value of one Property, of an Entity or of a Relation.
    Property {
        /// The handle [`super::PropertyValue::value_ref`] reports.
        value_ref: String,
        /// The Entity or Relation the value belongs to.
        host_id: String,
        /// The Property's name within its Schema.
        prop: String,
        /// The value itself, so a reader can see what was cited without a
        /// second lookup.
        value: String,
    },
    /// Something this version does not know how to describe — a citation
    /// of a timeline observation, whose table arrives with the timeline in
    /// M4. Passed through rather than refused, so that a Source listing
    /// does not start failing the day something else learns to cite.
    Other {
        target_kind: String,
        target_id: String,
    },
}

/// One recorded claim that a Source supports a fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// The citation's own id, which is what removes it again.
    pub id: i64,
    pub source_id: String,
    pub target: CitationTarget,
    /// Why this Source supports this fact, where the analyst said.
    pub note: Option<String>,
    /// ISO-8601 UTC.
    pub created_at: String,
}

/// The citations of one thing, bounded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citations {
    /// Oldest first: the first citation of a fact is the one its cached
    /// primary source comes from.
    pub items: Vec<Citation>,
    /// Whether there were more than the budget allowed. AGENT-BRIEF.md
    /// rule 5.
    pub truncated: bool,
}

impl Citations {
    /// Whether anything cites this at all — the flag that "unsourced but
    /// permitted" is about.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// A Source together with what cites it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceUsage {
    pub source: Source,
    /// A bounded sample of the citations, oldest first.
    pub citations: Citations,
    /// How many citations there are, counted up to a ceiling. Equal to
    /// `citations.items.len()` whenever nothing was truncated.
    pub citation_count: usize,
}

/// A page of the Source list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceList {
    pub sources: Vec<SourceUsage>,
    /// Whether there were more rows than the budget allowed.
    pub truncated: bool,
    /// The budget actually applied, after clamping.
    pub limit: usize,
    pub offset: usize,
}

/// Everything needed to record where something was read.
#[derive(Debug, Clone, Default)]
pub struct CreateSource {
    /// The id to give it. `None` generates one — the same bargain
    /// [`super::CreateEntity::id`] offers, so an agent can build a Source
    /// and its citations locally and submit them together.
    pub id: Option<String>,
    /// The Case it belongs to.
    pub case_id: String,
    /// `None` means [`SourceKind::Manual`]: an analyst asserting something
    /// with no locator is the only kind that needs no other field.
    pub kind: Option<SourceKind>,
    pub title: Option<String>,
    /// Required for [`SourceKind::Link`].
    pub url: Option<String>,
    /// Required for [`SourceKind::File`].
    pub reference: Option<String>,
    pub excerpt: Option<String>,
    /// When it was observed. Anything [`PartialDate`] reads — `2024`,
    /// `2024-05-01`, `2024-05-01T09:00:00Z`. Stored as a single instant at
    /// the start of whatever span was given, so a coarse value is a
    /// documented approximation rather than a false precision.
    pub retrieved_at: Option<String>,
}

/// Which Sources [`Services::list_sources`] returns.
#[derive(Debug, Clone, Default)]
pub struct SourceQuery {
    /// The Case to list within.
    pub case_id: String,
    /// Only Sources of this kind.
    pub kind: Option<SourceKind>,
    /// Row budget. `None` uses [`super::ServiceConfig::max_page_size`];
    /// anything larger is clamped to it.
    pub limit: Option<usize>,
    /// Rows to skip.
    pub offset: usize,
}

/// Which fact a Source is being cited for.
#[derive(Debug, Clone)]
pub enum CiteTarget {
    /// The whole Entity.
    Entity { entity_id: String },
    /// The whole Relation.
    Relation { relation_id: String },
    /// One Property value, named by the handle a read reported. This is
    /// the exact form: it names one stored value and cannot be
    /// misinterpreted.
    PropertyValue { value_ref: String },
    /// One Property value of an Entity, named by where it sits.
    EntityProperty {
        entity_id: String,
        prop: String,
        /// The value's position among that Property's values, as
        /// [`super::PropertyValue::seq`] reports it. `None` means "the only
        /// value", and is refused if the Property holds more than one.
        seq: Option<usize>,
    },
    /// One Property value of a Relation, named by where it sits.
    RelationProperty {
        relation_id: String,
        prop: String,
        seq: Option<usize>,
    },
}

/// A request to record that a Source supports a fact.
#[derive(Debug, Clone)]
pub struct CiteSource {
    pub source_id: String,
    pub target: CiteTarget,
    /// Why, in the analyst's words. Optional: most citations say all they
    /// need to say by existing.
    pub note: Option<String>,
}

/// A fact with no Source, as the unsourced report names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsourcedValue {
    /// The handle that cites it: pass this straight back as
    /// [`CiteTarget::PropertyValue`].
    pub value_ref: String,
    /// The Entity or Relation the value belongs to.
    pub host_id: String,
    pub prop: String,
    pub value: String,
}

/// What in one Case has no Source behind it.
///
/// Each list is bounded independently and says whether it was cut short,
/// because "there are more unsourced Entities than this" is exactly the
/// thing a caller must not mistake for "these are all of them".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsourcedFacts {
    pub case_id: String,
    /// Entities with no citation of their own, newest first.
    pub entities: Vec<String>,
    pub entities_truncated: bool,
    /// Relations with no citation of their own, newest first.
    pub relations: Vec<String>,
    pub relations_truncated: bool,
    /// Property values of Entities and of Relations with no Source.
    pub property_values: Vec<UnsourcedValue>,
    pub property_values_truncated: bool,
    /// The budget applied to each list, after clamping.
    pub limit: usize,
}

/// How much of one kind of fact carries a Source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub sourced: usize,
    pub unsourced: usize,
    pub total: usize,
    /// Whether the count stopped at its ceiling. When true every number
    /// here is a floor, not a total — the honest answer to a question that
    /// would otherwise be a full scan of the Case. AGENT-BRIEF.md rule 5.
    pub capped: bool,
}

impl Coverage {
    fn of(row: CoverageRow) -> Self {
        Coverage {
            sourced: row.sourced.max(0) as usize,
            unsourced: (row.total - row.sourced).max(0) as usize,
            total: row.total.max(0) as usize,
            capped: row.capped,
        }
    }

    fn plus(self, other: Coverage) -> Self {
        Coverage {
            sourced: self.sourced + other.sourced,
            unsourced: self.unsourced + other.unsourced,
            total: self.total + other.total,
            capped: self.capped || other.capped,
        }
    }
}

/// Sourced against unsourced, for one Case, at each of the three levels a
/// claim can be made at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCoverage {
    /// Entities carrying a citation of their own. An Entity whose values
    /// are all sourced but which nothing cites as a whole counts as
    /// unsourced here and sourced under
    /// [`SourceCoverage::property_values`] — the three levels are counted
    /// separately because they are separate claims.
    pub entities: Coverage,
    pub relations: Coverage,
    /// Property values of Entities and Relations together.
    pub property_values: Coverage,
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

impl Services {
    /// Records where something was read.
    ///
    /// A `url` Source whose normalised locator already names a Source in
    /// this Case **returns that Source** rather than creating a second one,
    /// and records nothing: "the same page cited twice is one Source"
    /// (spec 0001, user story 39) is a rule about what the Case contains,
    /// not a warning to the caller. That also makes an agent that submits
    /// the same page twice idempotent rather than duplicating its own
    /// evidence.
    pub async fn create_source(
        &self,
        request: CreateSource,
        actor: &Actor,
    ) -> Result<Source, ServiceError> {
        let kind = request.kind.unwrap_or(SourceKind::Manual);
        let id = match request.id {
            Some(id) => validate_id(&id)?,
            None => vesica_store::new_id(),
        };
        let title = optional_text("title", request.title.as_deref(), MAX_TITLE_LEN)?;
        let excerpt = optional_text("excerpt", request.excerpt.as_deref(), MAX_EXCERPT_LEN)?;
        let reference =
            optional_text("reference", request.reference.as_deref(), MAX_REFERENCE_LEN)?;
        let url = optional_text("url", request.url.as_deref(), MAX_EXCERPT_LEN)?;

        let (url, url_norm) = match &url {
            Some(raw) => {
                let normalized = PropertyType::Url
                    .normalize(raw, &vesica_core::NormalizeContext::new())
                    .map_err(|e| ServiceError::from_core("url", e))?;
                (Some(normalized.raw.clone()), Some(normalized.value))
            }
            None => (None, None),
        };

        require_kind_fields(kind, url.as_deref(), reference.as_deref())?;
        if title.is_none() && url.is_none() && reference.is_none() && excerpt.is_none() {
            return Err(ServiceError::validation(
                "title",
                "a source has to say what it is",
                "give at least one of `title`, `url`, `reference` or `excerpt`",
            ));
        }

        let meta = reference.as_ref().and_then(|reference| {
            serde_json::to_string(&serde_json::json!({ META_REFERENCE: reference })).ok()
        });
        let case_id = request.case_id;
        let retrieved = request.retrieved_at;
        let now = vesica_store::now();
        let actor = actor.as_str().to_string();

        self.tx(move |conn| {
            Box::pin(async move {
                let case = case::find(&mut *conn, &case_id)
                    .await?
                    .filter(|row| row.deleted_at.is_none())
                    .ok_or_else(|| ServiceError::not_found("case", &case_id))?;
                let retrieved_at =
                    parse_retrieved_at(retrieved.as_deref(), case.default_region.as_deref())?;

                // The same page, already recorded here, is this page.
                if let Some(url_norm) = &url_norm
                    && let Some(existing) =
                        source::find_by_url_norm(&mut *conn, &case_id, url_norm).await?
                {
                    return assemble(existing);
                }

                let row = SourceRow {
                    id,
                    case_id,
                    kind: kind.as_str().to_string(),
                    title,
                    url,
                    url_norm,
                    excerpt,
                    retrieved_at,
                    created_at: now,
                    created_by: actor.clone(),
                    meta,
                };
                let created = assemble(row.clone())?;

                // Audit first, change second, for the reason the module
                // docs of `super` give.
                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Create,
                        &row.case_id,
                        TARGET_SOURCE,
                        &row.id,
                        None,
                        snapshot(&created),
                    ),
                )
                .await?;
                source::insert(conn, &row)
                    .await
                    .map_err(|e| super::entity::duplicate_id_conflict(&row.id, e))?;
                Ok(created)
            })
        })
        .await
    }

    /// The Source with this id.
    pub async fn get_source(&self, id: &str) -> Result<Source, ServiceError> {
        let row = source::find(self.db().read_pool(), id)
            .await?
            .ok_or_else(|| ServiceError::not_found(TARGET_SOURCE, id))?;
        assemble(row)
    }

    /// A bounded page of the Sources in one Case, newest first, each with
    /// a bounded sample of what cites it and a count of how many citations
    /// it really has.
    pub async fn list_sources(&self, query: SourceQuery) -> Result<SourceList, ServiceError> {
        let limit = self.page_size(query.limit);
        let filter = SourceFilter {
            case_id: query.case_id,
            kind: query.kind.map(|k| k.as_str().to_string()),
            // One more than the budget, to learn whether there was more
            // without a second count query.
            limit: (limit as i64).saturating_add(1),
            offset: query.offset as i64,
        };
        let pool = self.db().read_pool();
        let mut rows = source::list(pool, &filter).await?;
        let truncated = rows.len() > limit;
        rows.truncate(limit);

        let mut sources = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.id.clone();
            let source = assemble(row)?;
            let citations = citations_of_source(pool, &id, MAX_CITATIONS_LISTED).await?;
            let citation_count =
                source::citation_count(pool, &id, source::MAX_COUNT_SCAN).await? as usize;
            sources.push(SourceUsage {
                source,
                citations,
                citation_count,
            });
        }

        Ok(SourceList {
            sources,
            truncated,
            limit,
            offset: query.offset,
        })
    }

    /// Records that a Source supports a fact: an Entity, a Relation, or one
    /// value of one Property.
    ///
    /// The Source and the fact must be in the same Case — a Case is a
    /// boundary (spec 0001, user story 21), and evidence that crosses one
    /// silently would make that boundary a suggestion. Citing the same
    /// Source for the same fact twice returns the citation that is already
    /// there rather than recording the claim twice.
    pub async fn cite_source(
        &self,
        request: CiteSource,
        actor: &Actor,
    ) -> Result<Citation, ServiceError> {
        let note = optional_text("note", request.note.as_deref(), MAX_NOTE_LEN)?;
        let source_id = request.source_id;
        let target = request.target;
        let actor = actor.as_str().to_string();

        self.tx(move |conn| {
            Box::pin(async move {
                let source = source::find(&mut *conn, &source_id)
                    .await?
                    .ok_or_else(|| ServiceError::not_found(TARGET_SOURCE, &source_id))?;
                let resolved = resolve_target(conn, &target).await?;

                if resolved.case_id != source.case_id {
                    return Err(ServiceError::Conflict {
                        message: format!(
                            "source `{source_id}` belongs to case `{}` and the fact it \
                             would be cited for belongs to case `{}`",
                            source.case_id, resolved.case_id
                        ),
                        fix: "record the source in the same case as the fact it supports; \
                              a case is a boundary and evidence does not cross one"
                            .to_string(),
                    });
                }

                // Citing the same source for the same fact twice is one
                // claim, not two.
                if let Some(existing) = source::find_citation_of(
                    &mut *conn,
                    &source_id,
                    resolved.target_kind,
                    &resolved.target_id,
                )
                .await?
                {
                    return Ok(Citation {
                        id: existing.id,
                        source_id: existing.source_id,
                        target: resolved.described(),
                        note: existing.note,
                        created_at: existing.created_at,
                    });
                }

                let new = NewCitation {
                    source_id: source_id.clone(),
                    target_kind: resolved.target_kind.to_string(),
                    target_id: resolved.target_id.clone(),
                    note,
                };
                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Cite,
                        &source.case_id,
                        resolved.audit_kind,
                        &resolved.audit_id,
                        None,
                        citation_snapshot(&source_id, &resolved, new.note.as_deref()),
                    ),
                )
                .await?;
                let row = source::insert_citation(conn, &new).await?;
                if let Some((host, prop_row_id)) = resolved.prop {
                    source::refresh_prop_source(conn, host, prop_row_id).await?;
                }

                Ok(Citation {
                    id: row.id,
                    source_id: row.source_id,
                    target: resolved.described(),
                    note: row.note,
                    created_at: row.created_at,
                })
            })
        })
        .await
    }

    /// Withdraws one citation. The fact stays; it is simply no longer
    /// claimed to come from that Source.
    pub async fn remove_citation(&self, id: i64, actor: &Actor) -> Result<(), ServiceError> {
        let actor = actor.as_str().to_string();

        self.tx(move |conn| {
            Box::pin(async move {
                let row = source::find_citation(&mut *conn, id)
                    .await?
                    .ok_or_else(|| ServiceError::not_found("citation", id.to_string()))?;
                let source = source::find(&mut *conn, &row.source_id)
                    .await?
                    .ok_or_else(|| ServiceError::not_found(TARGET_SOURCE, &row.source_id))?;
                let (audit_kind, audit_id) = audit_target_of(&mut *conn, &row).await?;

                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Uncite,
                        &source.case_id,
                        audit_kind,
                        &audit_id,
                        serde_json::to_string(&serde_json::json!({
                            "source_id": row.source_id,
                            "target_kind": row.target_kind,
                            "target_id": row.target_id,
                            "note": row.note,
                        }))
                        .ok(),
                        None,
                    ),
                )
                .await?;
                source::delete_citation(conn, id).await?;
                if row.target_kind == source::TARGET_PROP
                    && let Some((host, prop_row_id)) = source::parse_prop_target_id(&row.target_id)
                {
                    source::refresh_prop_source(conn, host, prop_row_id).await?;
                }
                Ok(())
            })
        })
        .await
    }

    /// Destroys a Source. **The facts it was cited for are untouched** and
    /// become unsourced again — or fall back to whatever else was cited for
    /// them, where an analyst recorded more than one claim about a value.
    ///
    /// There is no soft delete here and no confirmation gate. `source` has
    /// no `deleted_at`, and deleting a Source destroys no finding: it
    /// removes the assertion that a finding came from somewhere, which is
    /// exactly what an analyst who has discovered that a source was wrong
    /// wants to be able to do. The audit entry records that it happened,
    /// and what the Source was.
    pub async fn delete_source(&self, id: &str, actor: &Actor) -> Result<(), ServiceError> {
        let id = id.to_string();
        let actor = actor.as_str().to_string();

        self.tx(move |conn| {
            Box::pin(async move {
                let row = source::find(&mut *conn, &id)
                    .await?
                    .ok_or_else(|| ServiceError::not_found(TARGET_SOURCE, &id))?;
                let before = assemble(row.clone())?;

                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Delete,
                        &row.case_id,
                        TARGET_SOURCE,
                        &row.id,
                        snapshot(&before),
                        None,
                    ),
                )
                .await?;
                // Before the delete, not after: the citations that say
                // which other source a value should fall back to cascade
                // away with this one.
                source::release_props_of_source(conn, &row.id).await?;
                source::delete(conn, &row.id).await?;
                Ok(())
            })
        })
        .await
    }

    /// The audit trail for one Source, newest first, bounded.
    pub async fn source_audit(
        &self,
        id: &str,
        limit: Option<usize>,
    ) -> Result<super::AuditPage, ServiceError> {
        self.audit_for_target(TARGET_SOURCE, id, limit).await
    }

    /// Which Entities, Relations and Property values in one Case have no
    /// Source.
    ///
    /// Facts without a Source are permitted — capture has to stay fast —
    /// so this is the report that keeps the gaps visible rather than a list
    /// of errors. Each of the three lists is bounded separately and says
    /// whether it was cut short.
    pub async fn unsourced_facts(
        &self,
        query: UnsourcedQuery,
    ) -> Result<UnsourcedFacts, ServiceError> {
        let limit = self.page_size(query.limit);
        let budget = (limit as i64).saturating_add(1);
        let pool = self.db().read_pool();
        let case_id = query.case_id;

        let mut entities = source::unsourced_entities(pool, &case_id, budget).await?;
        let entities_truncated = entities.len() > limit;
        entities.truncate(limit);

        let mut relations = source::unsourced_relations(pool, &case_id, budget).await?;
        let relations_truncated = relations.len() > limit;
        relations.truncate(limit);

        let mut property_values = Vec::new();
        let mut property_values_truncated = false;
        for host in [PropHost::Entity, PropHost::Relation] {
            let rows = source::unsourced_props(pool, host, &case_id, budget).await?;
            property_values_truncated |= rows.len() > limit;
            for row in rows.into_iter().take(limit) {
                property_values.push(UnsourcedValue {
                    value_ref: source::prop_target_id(host, row.id),
                    host_id: row.host_id,
                    prop: row.prop,
                    value: row.value,
                });
            }
        }

        Ok(UnsourcedFacts {
            case_id,
            entities,
            entities_truncated,
            relations,
            relations_truncated,
            property_values,
            property_values_truncated,
            limit,
        })
    }

    /// How much of one Case's evidence is actually sourced, at each of the
    /// three levels a claim can be made at.
    ///
    /// The counts are bounded work: each runs inside a subquery capped at
    /// [`vesica_store::repo::source::MAX_COUNT_SCAN`] rows and says so via
    /// [`Coverage::capped`], so asking for them can never turn into a full
    /// scan of an arbitrarily large Case.
    pub async fn source_coverage(&self, case_id: &str) -> Result<SourceCoverage, ServiceError> {
        let pool = self.db().read_pool();
        let cap = source::MAX_COUNT_SCAN;
        let count = |kind| source::coverage(pool, kind, case_id, cap);

        Ok(SourceCoverage {
            entities: Coverage::of(count(CoverageKind::Entities).await?),
            relations: Coverage::of(count(CoverageKind::Relations).await?),
            property_values: Coverage::of(count(CoverageKind::EntityProps).await?)
                .plus(Coverage::of(count(CoverageKind::RelationProps).await?)),
        })
    }
}

/// Which Case's unsourced facts [`Services::unsourced_facts`] reports.
#[derive(Debug, Clone, Default)]
pub struct UnsourcedQuery {
    pub case_id: String,
    /// Row budget for each of the three lists. `None` uses
    /// [`super::ServiceConfig::max_page_size`].
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// Reading citations back onto what they are about
// ---------------------------------------------------------------------------

/// Every citation of one Entity: its own, and its Property values'.
///
/// Bounded at each level, with [`Citations::truncated`] set if either hit
/// its ceiling.
pub(super) async fn citations_of_entity(
    conn: &mut SqliteConnection,
    entity_id: &str,
) -> Result<Citations, ServiceError> {
    let budget = (MAX_CITATIONS_PER_TARGET as i64).saturating_add(1);
    let own =
        source::citations_for_target(&mut *conn, source::TARGET_ENTITY, entity_id, budget).await?;
    let props = source::prop_citations_for_entity(&mut *conn, entity_id, budget).await?;
    Ok(collect(own, props))
}

/// Every citation of one Relation: its own, and its Property values'.
pub(super) async fn citations_of_relation(
    conn: &mut SqliteConnection,
    relation_id: &str,
) -> Result<Citations, ServiceError> {
    let budget = (MAX_CITATIONS_PER_TARGET as i64).saturating_add(1);
    let own =
        source::citations_for_target(&mut *conn, source::TARGET_RELATION, relation_id, budget)
            .await?;
    let props = source::prop_citations_for_relation(&mut *conn, relation_id, budget).await?;
    Ok(collect(own, props))
}

fn collect(own: Vec<CitationDetailRow>, props: Vec<CitationDetailRow>) -> Citations {
    let truncated = own.len() > MAX_CITATIONS_PER_TARGET || props.len() > MAX_CITATIONS_PER_TARGET;
    let items = own
        .into_iter()
        .take(MAX_CITATIONS_PER_TARGET)
        .chain(props.into_iter().take(MAX_CITATIONS_PER_TARGET))
        .map(citation_of)
        .collect();
    Citations { items, truncated }
}

async fn citations_of_source<'e, E>(
    exec: E,
    source_id: &str,
    limit: usize,
) -> Result<Citations, ServiceError>
where
    E: sqlx::sqlite::SqliteExecutor<'e>,
{
    let mut rows =
        source::citations_for_source(exec, source_id, (limit as i64).saturating_add(1)).await?;
    let truncated = rows.len() > limit;
    rows.truncate(limit);
    Ok(Citations {
        items: rows.into_iter().map(citation_of).collect(),
        truncated,
    })
}

fn citation_of(row: CitationDetailRow) -> Citation {
    let target = match row.target_kind.as_str() {
        source::TARGET_ENTITY => CitationTarget::Entity {
            entity_id: row.target_id,
        },
        source::TARGET_RELATION => CitationTarget::Relation {
            relation_id: row.target_id,
        },
        source::TARGET_PROP => match (row.host_id, row.prop, row.value) {
            (Some(host_id), Some(prop), Some(value)) => CitationTarget::Property {
                value_ref: row.target_id,
                host_id,
                prop,
                value,
            },
            // A property citation whose value is gone. Nothing should be
            // able to produce one — every path that deletes a value deletes
            // its citations — so it is reported rather than hidden.
            _ => CitationTarget::Other {
                target_kind: row.target_kind,
                target_id: row.target_id,
            },
        },
        _ => CitationTarget::Other {
            target_kind: row.target_kind,
            target_id: row.target_id,
        },
    };
    Citation {
        id: row.id,
        source_id: row.source_id,
        target,
        note: row.note,
        created_at: row.created_at,
    }
}

// ---------------------------------------------------------------------------
// Resolving what is being cited
// ---------------------------------------------------------------------------

/// A citation request's target, after it has been found and checked.
struct Resolved {
    /// The Case the fact belongs to, for the same-Case check.
    case_id: String,
    /// `citation.target_kind`.
    target_kind: &'static str,
    /// `citation.target_id`.
    target_id: String,
    /// Which Entity or Relation the audit entry is filed against. For a
    /// Property value that is its host, so that "the phone number on this
    /// person was sourced" appears in that person's trail rather than
    /// nowhere a reader would look.
    audit_kind: &'static str,
    audit_id: String,
    /// Set for a Property value: whose cached source has to be recomputed.
    prop: Option<(PropHost, i64)>,
    /// Set for a Property value: the host, name and value, for the
    /// citation this returns.
    detail: Option<(String, String, String)>,
}

impl Resolved {
    fn described(&self) -> CitationTarget {
        match (&self.detail, self.target_kind) {
            (Some((host_id, prop, value)), _) => CitationTarget::Property {
                value_ref: self.target_id.clone(),
                host_id: host_id.clone(),
                prop: prop.clone(),
                value: value.clone(),
            },
            (None, source::TARGET_ENTITY) => CitationTarget::Entity {
                entity_id: self.target_id.clone(),
            },
            (None, source::TARGET_RELATION) => CitationTarget::Relation {
                relation_id: self.target_id.clone(),
            },
            (None, kind) => CitationTarget::Other {
                target_kind: kind.to_string(),
                target_id: self.target_id.clone(),
            },
        }
    }
}

async fn resolve_target(
    conn: &mut SqliteConnection,
    target: &CiteTarget,
) -> Result<Resolved, ServiceError> {
    match target {
        CiteTarget::Entity { entity_id } => {
            let row = live_entity_row(conn, entity_id).await?;
            Ok(Resolved {
                case_id: row.case_id,
                target_kind: source::TARGET_ENTITY,
                target_id: row.id.clone(),
                audit_kind: TARGET_ENTITY,
                audit_id: row.id,
                prop: None,
                detail: None,
            })
        }
        CiteTarget::Relation { relation_id } => {
            let row = live_relation_row(conn, relation_id).await?;
            Ok(Resolved {
                case_id: row.case_id,
                target_kind: source::TARGET_RELATION,
                target_id: row.id.clone(),
                audit_kind: TARGET_RELATION,
                audit_id: row.id,
                prop: None,
                detail: None,
            })
        }
        CiteTarget::PropertyValue { value_ref } => {
            let (host, prop_row_id) = source::parse_prop_target_id(value_ref).ok_or_else(|| {
                ServiceError::validation(
                    "value_ref",
                    format!("`{value_ref}` does not name a property value"),
                    "send the `value_ref` a property value was read back with, \
                     e.g. `entity:412`",
                )
            })?;
            let found = source::find_prop(&mut *conn, host, prop_row_id)
                .await?
                .ok_or_else(|| ServiceError::not_found("property value", value_ref.clone()))?;
            prop_resolved(
                conn,
                host,
                prop_row_id,
                found.host_id,
                found.prop,
                found.value,
            )
            .await
        }
        CiteTarget::EntityProperty {
            entity_id,
            prop,
            seq,
        } => {
            let row = live_entity_row(conn, entity_id).await?;
            let values = entity::props(&mut *conn, &row.id).await?;
            let chosen = pick(
                values
                    .iter()
                    .filter(|value| value.prop == *prop)
                    .map(|value| (value.id, value.value.clone()))
                    .collect(),
                prop,
                *seq,
                entity_id,
            )?;
            prop_resolved(
                conn,
                PropHost::Entity,
                chosen.0,
                row.id,
                prop.clone(),
                chosen.1,
            )
            .await
        }
        CiteTarget::RelationProperty {
            relation_id,
            prop,
            seq,
        } => {
            let row = live_relation_row(conn, relation_id).await?;
            let values = relation::props(&mut *conn, &row.id).await?;
            let chosen = pick(
                values
                    .iter()
                    .filter(|value| value.prop == *prop)
                    .map(|value| (value.id, value.value.clone()))
                    .collect(),
                prop,
                *seq,
                relation_id,
            )?;
            prop_resolved(
                conn,
                PropHost::Relation,
                chosen.0,
                row.id,
                prop.clone(),
                chosen.1,
            )
            .await
        }
    }
}

/// Assembles the resolved form of a Property-value target, reading the
/// host's Case so that the same-Case check has something to compare.
async fn prop_resolved(
    conn: &mut SqliteConnection,
    host: PropHost,
    prop_row_id: i64,
    host_id: String,
    prop: String,
    value: String,
) -> Result<Resolved, ServiceError> {
    let (case_id, audit_kind) = match host {
        PropHost::Entity => (
            live_entity_row(conn, &host_id).await?.case_id,
            TARGET_ENTITY,
        ),
        PropHost::Relation => (
            live_relation_row(conn, &host_id).await?.case_id,
            TARGET_RELATION,
        ),
    };
    Ok(Resolved {
        case_id,
        target_kind: source::TARGET_PROP,
        target_id: source::prop_target_id(host, prop_row_id),
        audit_kind,
        audit_id: host_id.clone(),
        prop: Some((host, prop_row_id)),
        detail: Some((host_id, prop, value)),
    })
}

/// Picks the value a caller meant out of one Property's values, in the
/// order a read reports them.
fn pick(
    values: Vec<(i64, String)>,
    prop: &str,
    seq: Option<usize>,
    host_id: &str,
) -> Result<(i64, String), ServiceError> {
    if values.is_empty() {
        return Err(ServiceError::validation(
            "prop",
            format!("`{host_id}` has no value recorded for `{prop}`"),
            format!("record a value for `{prop}` before citing a source for it"),
        ));
    }
    match seq {
        Some(seq) => values.get(seq).cloned().ok_or_else(|| {
            ServiceError::validation(
                "seq",
                format!(
                    "`{prop}` of `{host_id}` has {} value(s), so there is none at position {seq}",
                    values.len()
                ),
                format!(
                    "send a `seq` between 0 and {}, as the property value was read back with",
                    values.len() - 1
                ),
            )
        }),
        None if values.len() == 1 => Ok(values[0].clone()),
        None => Err(ServiceError::validation(
            "seq",
            format!(
                "`{prop}` of `{host_id}` holds {} values, so which one is meant has to be said",
                values.len()
            ),
            format!(
                "send `seq`, the value's position from 0 to {}, or the `value_ref` the \
                 value was read back with",
                values.len() - 1
            ),
        )),
    }
}

/// Which Entity or Relation a stored citation is ultimately about, for its
/// audit entry. A Property citation is filed against its host.
async fn audit_target_of(
    conn: &mut SqliteConnection,
    row: &vesica_store::repo::CitationRow,
) -> Result<(&'static str, String), ServiceError> {
    if row.target_kind == source::TARGET_PROP {
        if let Some((host, prop_row_id)) = source::parse_prop_target_id(&row.target_id)
            && let Some(found) = source::find_prop(&mut *conn, host, prop_row_id).await?
        {
            let kind = match host {
                PropHost::Entity => TARGET_ENTITY,
                PropHost::Relation => TARGET_RELATION,
            };
            return Ok((kind, found.host_id));
        }
        return Ok((TARGET_SOURCE, row.source_id.clone()));
    }
    if row.target_kind == source::TARGET_RELATION {
        return Ok((TARGET_RELATION, row.target_id.clone()));
    }
    if row.target_kind == source::TARGET_ENTITY {
        return Ok((TARGET_ENTITY, row.target_id.clone()));
    }
    Ok((TARGET_SOURCE, row.source_id.clone()))
}

async fn live_entity_row(
    conn: &mut SqliteConnection,
    id: &str,
) -> Result<vesica_store::repo::EntityRow, ServiceError> {
    let row = entity::find(&mut *conn, id)
        .await?
        .filter(|row| row.deleted_at.is_none() && row.merged_into.is_none())
        .ok_or_else(|| ServiceError::not_found(TARGET_ENTITY, id))?;
    Ok(row)
}

async fn live_relation_row(
    conn: &mut SqliteConnection,
    id: &str,
) -> Result<vesica_store::repo::RelationRow, ServiceError> {
    let row = relation::find(&mut *conn, id)
        .await?
        .filter(|row| row.deleted_at.is_none())
        .ok_or_else(|| ServiceError::not_found(TARGET_RELATION, id))?;
    Ok(row)
}

// ---------------------------------------------------------------------------
// Validation and assembly
// ---------------------------------------------------------------------------

fn require_kind_fields(
    kind: SourceKind,
    url: Option<&str>,
    reference: Option<&str>,
) -> Result<(), ServiceError> {
    match kind {
        SourceKind::Link if url.is_none() => Err(ServiceError::validation(
            "url",
            "a `url` source is a page, and needs the page's address",
            "send `url`, e.g. `https://example.com/filing/1` — or record it as \
             `manual` if there is no address",
        )),
        SourceKind::File if reference.is_none() => Err(ServiceError::validation(
            "reference",
            "a `file` source is a file somebody holds, and needs a reference to it",
            "send `reference`: a filename, a path or an evidence-bag number. \
             Files themselves are not uploaded",
        )),
        _ => Ok(()),
    }
}

fn optional_text(
    field: &str,
    value: Option<&str>,
    max: usize,
) -> Result<Option<String>, ServiceError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > max {
        return Err(ServiceError::validation(
            field,
            format!("`{field}` may be at most {max} characters"),
            format!("shorten `{field}` to {max} characters or fewer"),
        ));
    }
    Ok(Some(trimmed.to_string()))
}

/// Reads the moment a Source was observed.
///
/// Anything [`PartialDate`] accepts, read with the Case's own date style so
/// that `01.05.2024` means in a Polish Case what it means everywhere else
/// in that Case. Stored as the **start** of whatever span was given, since
/// the column holds one instant: `2024` becomes the first moment of 2024, a
/// documented approximation rather than a fabricated precision.
fn parse_retrieved_at(
    value: Option<&str>,
    region: Option<&str>,
) -> Result<Option<String>, ServiceError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = PartialDate::parse(trimmed, DateStyle::for_region(region)).map_err(|e| {
        ServiceError::validation(
            "retrieved_at",
            e.to_string(),
            format!("{} — this is when the source was observed", e.fix()),
        )
    })?;
    Ok(Some(parsed.date.lower_iso()))
}

/// Turns a stored row into the domain type, deriving the locator's warnings
/// from the raw value as it goes — the same bargain an Entity's Property
/// warnings strike, so that improving a normalisation rule changes what is
/// reported without a migration.
fn assemble(row: SourceRow) -> Result<Source, ServiceError> {
    let kind = SourceKind::from_stored(&row.kind)?;
    let warnings = match &row.url {
        Some(raw) => PropertyType::Url
            .normalize(raw, &vesica_core::NormalizeContext::new())
            .map(|normalized| {
                normalized
                    .warnings
                    .into_iter()
                    .map(|warning| PropertyWarning {
                        prop: "url".to_string(),
                        value: raw.clone(),
                        code: warning.code,
                        message: warning.message,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        None => Vec::new(),
    };

    Ok(Source {
        id: row.id,
        case_id: row.case_id,
        kind,
        title: row.title,
        url: row.url,
        url_norm: row.url_norm,
        reference: reference_of(row.meta.as_deref()),
        excerpt: row.excerpt,
        retrieved_at: row.retrieved_at,
        created_at: row.created_at,
        created_by: row.created_by,
        warnings,
    })
}

fn reference_of(meta: Option<&str>) -> Option<String> {
    let meta: serde_json::Value = serde_json::from_str(meta?).ok()?;
    Some(meta.get(META_REFERENCE)?.as_str()?.to_string())
}

/// Renders a Source for an audit entry's `before`/`after`. The warnings are
/// left out: they are derived from the locator, which is in the snapshot.
fn snapshot(source: &Source) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "id": source.id,
        "case_id": source.case_id,
        "kind": source.kind,
        "title": source.title,
        "url": source.url,
        "url_norm": source.url_norm,
        "reference": source.reference,
        "excerpt": source.excerpt,
        "retrieved_at": source.retrieved_at,
    }))
    .ok()
}

fn citation_snapshot(source_id: &str, resolved: &Resolved, note: Option<&str>) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "source_id": source_id,
        "target_kind": resolved.target_kind,
        "target_id": resolved.target_id,
        "note": note,
    }))
    .ok()
}

#[allow(clippy::too_many_arguments)]
fn entry(
    actor: &str,
    action: AuditAction,
    case_id: &str,
    target_kind: &str,
    target_id: &str,
    before: Option<String>,
    after: Option<String>,
) -> vesica_store::repo::NewAuditEntry {
    vesica_store::repo::NewAuditEntry {
        actor: actor.to_string(),
        action: action.as_str().to_string(),
        case_id: Some(case_id.to_string()),
        target_kind: target_kind.to_string(),
        target_id: target_id.to_string(),
        before,
        after,
        batch_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_the_code_does_not_know_is_refused_with_the_ones_it_does() {
        assert_eq!(SourceKind::parse("URL").unwrap(), SourceKind::Link);
        assert_eq!(SourceKind::parse("link").unwrap(), SourceKind::Link);
        assert_eq!(SourceKind::parse(" agent ").unwrap(), SourceKind::Agent);
        let e = SourceKind::parse("hunch").unwrap_err();
        assert!(e.fix().unwrap().contains("manual"));
        assert!(matches!(
            SourceKind::from_stored("hunch"),
            Err(ServiceError::Inconsistent(_))
        ));
    }

    #[test]
    fn each_kind_is_refused_without_the_field_that_makes_it_that_kind() {
        let e = require_kind_fields(SourceKind::Link, None, None).unwrap_err();
        assert!(e.fix().unwrap().contains("url"));
        let e = require_kind_fields(SourceKind::File, None, None).unwrap_err();
        assert!(
            e.fix().unwrap().contains("not uploaded"),
            "the fix has to say a file reference is a reference"
        );
        assert!(require_kind_fields(SourceKind::Manual, None, None).is_ok());
        assert!(require_kind_fields(SourceKind::Agent, None, None).is_ok());
        assert!(require_kind_fields(SourceKind::Link, Some("https://a.test"), None).is_ok());
    }

    #[test]
    fn a_retrieval_time_is_read_at_whatever_precision_it_was_given() {
        assert_eq!(
            parse_retrieved_at(Some("2024-05-01T09:30:00Z"), None).unwrap(),
            Some("2024-05-01T09:30:00Z".to_string())
        );
        assert_eq!(
            parse_retrieved_at(Some("2024"), None).unwrap(),
            Some("2024-01-01T00:00:00Z".to_string()),
            "a coarse retrieval time is pinned to the start of its span"
        );
        assert_eq!(parse_retrieved_at(Some("   "), None).unwrap(), None);
        let e = parse_retrieved_at(Some("last tuesday"), None).unwrap_err();
        assert!(e.fix().unwrap().contains("observed"));
    }

    #[test]
    fn choosing_a_value_out_of_a_repeating_property_says_what_went_wrong() {
        let two = vec![(1, "a".to_string()), (2, "b".to_string())];
        assert_eq!(pick(two.clone(), "phone", Some(1), "ent").unwrap().0, 2);

        let e = pick(two.clone(), "phone", None, "ent").unwrap_err();
        assert!(e.fix().unwrap().contains("seq"));
        let e = pick(two, "phone", Some(9), "ent").unwrap_err();
        assert!(e.fix().unwrap().contains("between 0 and 1"));

        assert_eq!(
            pick(vec![(7, "only".to_string())], "phone", None, "ent")
                .unwrap()
                .0,
            7
        );
        let e = pick(Vec::new(), "phone", None, "ent").unwrap_err();
        assert!(e.fix().unwrap().contains("record a value"));
    }

    #[test]
    fn a_file_reference_survives_the_round_trip_through_meta() {
        let meta = serde_json::json!({ META_REFERENCE: "bag-17/scan-3.pdf" }).to_string();
        assert_eq!(
            reference_of(Some(&meta)).as_deref(),
            Some("bag-17/scan-3.pdf")
        );
        assert_eq!(reference_of(None), None);
        assert_eq!(reference_of(Some("not json")), None);
    }
}
