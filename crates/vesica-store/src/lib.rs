//! `vesica-store`: SQLite persistence for Vesica.
//!
//! Owns:
//! - [`migrate`]: numbered SQL migrations, applied transactionally at
//!   startup by [`store::Db::open`].
//! - [`store::Db`]: the concurrency model from `03-architecture.md` section
//!   3.7 — a single writer task fed by a channel, plus a pool of readers,
//!   both with WAL/foreign-keys/synchronous/busy-timeout pragmas set on
//!   every connection. This is why there is no retry logic anywhere in this
//!   crate: write/write lock contention is structurally impossible, not
//!   merely unlikely.
//! - [`repo`]: persistence for each aggregate — `repo::case` and
//!   `repo::audit` as transaction-scoped functions (so a change and its
//!   audit entry commit together), `EntityRepo`, `RelationRepo` and
//!   `SourceRepo` still as traits with one SQLite implementation each.
//! - [`graph`]: the bounded neighbourhood-traversal recursive CTE.
//! - [`testing`]: the shared harness later tickets use to stand up a real
//!   store in a test.
//!
//! Depends on `vesica-core` for domain types; no other crate in this
//! workspace depends on it except through the API and MCP layers.
//!
//! Not yet here (later tickets): `entity_prop`/`relation_prop` read-write
//! (needs `vesica-core` validation/normalisation), `BlobRepo`, `MatchRepo`,
//! FTS5 query helpers (the triggers that keep `entity_fts` in sync already
//! exist as of migration `0005_search.sql` and need no application code),
//! shortest-path/connected-components traversals beyond the neighbourhood
//! expansion in [`graph`].

#![forbid(unsafe_code)]

mod error;
pub mod graph;
pub mod ids;
pub mod migrate;
pub mod repo;
pub mod store;
pub mod testing;
mod writer;

pub use error::StoreError;
pub use ids::{is_valid_id, new_id, now};
pub use migrate::{EMBEDDED_MIGRATIONS, Migration, MigrationOutcome};
pub use sqlx::SqliteConnection;
pub use store::Db;
pub use writer::BoxFuture;
