//! `vesica`: the Vesica binary.
//!
//! Owns configuration (environment plus `.env`), wiring of the API and MCP
//! layers into one axum router, and the CLI subcommands (`serve`, `mcp`,
//! `migrate`, `rescan-matches`, `backup`, `check`, `import`, `export`). See
//! `03-architecture.md` section 3.4.
//!
//! This is currently a stub: ticket 0001-01 establishes the crate boundary.
//! `main` prints the crate version and exits; subcommands land in later
//! tickets.

#![forbid(unsafe_code)]

fn main() {
    println!("vesica {}", env!("CARGO_PKG_VERSION"));
}
