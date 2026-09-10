//! Repository traits and their SQLite implementations.
//!
//! Two shapes live here, for a reason worth stating once.
//!
//! **Traits with one implementation** (`RelationRepo`, `SourceRepo`) are
//! the seam 03-architecture.md section 3.7 describes: "the `vesica-store`
//! repository trait is the seam behind which Postgres could be added. That
//! seam is deliberate but unused."
//!
//! **Modules of executor-generic functions** (`case`, `audit`, `entity`,
//! `tag`) are what that seam turns into once a use case has to write two
//! tables atomically. A
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
//! Ticket 0001-09 did the same to `entity` — which is also where
//! `entity_prop` and the tag tables arrived, since an entity, its property
//! values and its audit entry are one write — and added `tag`. The
//! remaining traits stay as ticket 0001-03 left them; the tickets that give
//! relations and sources their service-layer use cases will do to them what
//! 0001-08 did here.

pub mod audit;
pub mod case;
pub mod entity;
mod relation;
mod source;
pub mod tag;

pub use audit::{AuditEntry, NewAuditEntry};
pub use case::{CaseFilter, CaseRow};
pub use entity::{EntityFilter, EntityRow, NewProp, PropRow};
pub use relation::{NewRelation, Relation, RelationRepo, SqliteRelationRepo};
pub use source::{NewSource, Source, SourceRepo, SqliteSourceRepo};
pub use tag::{TagRow, TagUsage};
