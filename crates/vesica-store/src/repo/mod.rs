//! Persistence, one module per aggregate.
//!
//! Every one of them is a **module of executor-generic functions**, which
//! is what 03-architecture.md section 3.7's seam — "the `vesica-store`
//! repository trait is the seam behind which Postgres could be added; that
//! seam is deliberate but unused" — turns into once a use case has to write
//! two tables atomically. A
//! `&self` repository method opens and closes its own transaction, so a
//! service that must write a `kase` row and its `audit_log` row together —
//! ticket 0001-08's central requirement — cannot be built out of two of
//! them. The transaction therefore belongs to the caller
//! ([`crate::store::Db::transaction`]) and these functions run on whatever
//! executor it hands them: the reader pool for a plain read, the
//! transaction's connection for everything a mutation touches. The seam
//! still exists — it is the module boundary plus `Db::transaction` — and a
//! second backend would implement exactly the same two things.
//!
//! Ticket 0001-08 converted `case` and `audit`. Ticket 0001-09 did the same
//! to `entity` — which is also where `entity_prop` and the tag tables
//! arrived, since an entity, its property values and its audit entry are
//! one write — and added `tag`. Ticket 0001-10 did the same to `relation`,
//! which brought `relation_prop` with it for the same reason. Ticket
//! 0001-11 converted `source`, the last surviving trait, and brought
//! `citation` with it: a citation, the source it belongs to and the audit
//! entry recording either are one write. No repository trait remains.

pub mod audit;
pub mod case;
pub mod entity;
pub mod relation;
pub mod source;
pub mod tag;

pub use audit::{AuditEntry, NewAuditEntry};
pub use case::{CaseFilter, CaseRow};
pub use entity::{EntityFilter, EntityRow, NewProp, PropRow};
pub use relation::{EndpointRole, NewRelationProp, RelationFilter, RelationPropRow, RelationRow};
pub use source::{
    CitationDetailRow, CitationRow, CoverageKind, CoverageRow, NewCitation, PropHost, PropRef,
    SourceFilter, SourceRow, UnsourcedPropRow,
};
pub use tag::{TagRow, TagUsage};
