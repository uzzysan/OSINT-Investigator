//! `vesica-report`: case report rendering for Vesica.
//!
//! Renders a case's entities, relations and timeline to Markdown, JSON or
//! FollowTheMoney (FtM) output for hand-off to a human or another tool.
//! Depends on `vesica-core` for the domain types it renders. See
//! `03-architecture.md` section 3.3/3.4.
//!
//! This is currently a stub: ticket 0001-01 establishes the crate boundary.
//! Behaviour lands in later tickets.

#![forbid(unsafe_code)]
