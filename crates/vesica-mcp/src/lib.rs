//! `vesica-mcp`: the MCP server for Vesica.
//!
//! Exposes tools, resources and prompts (via `rmcp`, the official Rust MCP
//! SDK — added in the ticket that implements this crate) so AI agents can
//! browse case data to build reports, or add and update entities to build
//! knowledge. Calls the same service layer as `vesica-api`: depends on
//! `vesica-store`, `vesica-resolve`, `vesica-import` and `vesica-report`.
//! Runs over stdio via `vesica mcp`, and is mounted at `/mcp` by the HTTP
//! server. See `03-architecture.md` section 3.3/3.4.
//!
//! This is currently a stub: ticket 0001-01 establishes the crate boundary.
//! Behaviour lands in later tickets.

#![forbid(unsafe_code)]
