//! Repository traits and their SQLite implementations.
//!
//! Each trait (`CaseRepo`, `EntityRepo`, `RelationRepo`, `SourceRepo`,
//! `AuditRepo`) is the seam 03-architecture.md section 3.7 describes: "the
//! `vesica-store` repository trait is the seam behind which Postgres could
//! be added. That seam is deliberate but unused." Accordingly there is
//! exactly one implementation of each trait here (`Sqlite*Repo`) — this
//! ticket does not add a second backend, it only makes sure the boundary
//! exists in a shape a second backend could actually implement.
//!
//! The methods here are deliberately not full CRUD: later tickets own the
//! service-level operations (validation, normalisation, entity resolution
//! enqueueing, audit logging on every mutation, and so on). This much exists
//! to prove the write/read plumbing — the single-writer channel, the reader
//! pool, foreign keys, ids and timestamps — end to end.

mod audit;
mod case;
mod entity;
mod relation;
mod source;

pub use audit::{AuditEntry, AuditRepo, NewAuditEntry, SqliteAuditRepo};
pub use case::{Case, CaseRepo, NewCase, SqliteCaseRepo};
pub use entity::{Entity, EntityRepo, NewEntity, SqliteEntityRepo};
pub use relation::{NewRelation, Relation, RelationRepo, SqliteRelationRepo};
pub use source::{NewSource, Source, SourceRepo, SqliteSourceRepo};
