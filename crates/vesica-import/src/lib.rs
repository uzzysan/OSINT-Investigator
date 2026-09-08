//! `vesica-import`: bulk ingestion for Vesica.
//!
//! CSV, XLSX and FollowTheMoney (FtM) import with user-configurable column
//! mapping and a dry-run mode that reports what would be written without
//! writing it. Depends on `vesica-core` for the domain types and validation
//! rules import mapping must satisfy. See `03-architecture.md` section 3.4.
//!
//! This is currently a stub: ticket 0001-01 establishes the crate boundary.
//! Behaviour lands in later tickets.

#![forbid(unsafe_code)]
