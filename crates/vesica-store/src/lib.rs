//! `vesica-store`: SQLite persistence for Vesica.
//!
//! Owns the `sqlx` connection pool (one writer, many readers, WAL mode),
//! migrations run at startup, the repository types (`CaseRepo`, `EntityRepo`,
//! `RelationRepo`, `SourceRepo`, `BlobRepo`, `MatchRepo`, `AuditRepo`), the
//! FTS5 virtual table, and graph queries expressed as recursive CTEs
//! (neighbourhood expansion, shortest path, connected components). Depends
//! on `vesica-core` for domain types; no other crate in this workspace
//! depends on it except through the API and MCP layers. See
//! `03-architecture.md` section 3.4.
//!
//! This is currently a stub: ticket 0001-01 establishes the crate boundary.
//! Behaviour lands in later tickets.

#![forbid(unsafe_code)]
