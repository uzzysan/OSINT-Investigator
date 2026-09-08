//! `vesica-resolve`: cross-case entity resolution for Vesica.
//!
//! Computes blocking keys, shortlists candidates, and scores similarity for
//! dirty entities fed through a durable queue. Never mutates entities —
//! it only produces `match_candidate` proposals for a human or agent to
//! confirm. Depends on `vesica-core` for the normalised values it scores
//! against. See `05-entity-resolution.md` and `03-architecture.md`
//! section 3.4.
//!
//! This is currently a stub: ticket 0001-01 establishes the crate boundary.
//! Behaviour lands in later tickets.

#![forbid(unsafe_code)]
