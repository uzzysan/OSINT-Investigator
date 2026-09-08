//! `vesica-core`: the pure domain layer of Vesica.
//!
//! This crate owns the schema registry, property types and value
//! normalisation, caption derivation, ID generation and validation errors
//! that every other crate builds on. It performs no I/O — no database, no
//! filesystem, no network — so the domain rules stay testable without a
//! running server, and so the matcher and the write path can never disagree
//! about what a normalised value looks like. See `03-architecture.md`
//! section 3.4 for the full crate charter.
//!
//! This is currently a stub: ticket 0001-01 establishes the crate boundary
//! and its dependency-free manifest. Behaviour lands in later tickets.

#![forbid(unsafe_code)]
