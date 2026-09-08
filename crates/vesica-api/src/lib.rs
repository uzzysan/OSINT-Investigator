//! `vesica-api`: the axum HTTP layer for Vesica.
//!
//! Thin handlers that parse requests, call into a service function, and
//! serialise the result. DTOs are kept separate from domain types so the
//! wire format can stay stable while the domain evolves. Serves the REST
//! surface, SSE change events, and attachment upload/download. Depends on
//! `vesica-store`, `vesica-resolve`, `vesica-import` and `vesica-report` —
//! the same service layer `vesica-mcp` calls, so everything the API can do,
//! MCP can do too. See `03-architecture.md` section 3.3/3.4 and
//! `06-http-api.md`.
//!
//! This is currently a stub: ticket 0001-01 establishes the crate boundary.
//! Behaviour lands in later tickets.

#![forbid(unsafe_code)]
