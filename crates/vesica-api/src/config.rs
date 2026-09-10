//! What [`crate::serve`] needs to start the HTTP server, decoupled from the
//! `vesica` binary's own `Config` type. `vesica-api` is a library other
//! crates (and, eventually, other binaries) can depend on; it must not
//! reach back up to the composition root for its configuration shape. The
//! `vesica` binary builds an [`ApiConfig`] from its own `Config` at startup
//! — see `crates/vesica/src/main.rs`.

use std::fmt;
use std::net::IpAddr;
use std::path::PathBuf;

/// Everything [`crate::serve`] needs that isn't the already-open [`vesica_store::Db`].
#[derive(Clone)]
pub struct ApiConfig {
    /// Address to bind the HTTP listener to.
    pub bind_addr: IpAddr,
    /// Port to bind the HTTP listener to.
    pub port: u16,
    /// Directory attachments (photos, scans, documents) are stored under.
    /// The readiness check proves this is actually writable by the running
    /// process — see `06-http-api.md` section 6.2 "Operations".
    pub blob_dir: PathBuf,
    /// The access-control mode enforced by the single auth middleware layer
    /// — see `06-http-api.md` section 6.4.
    pub auth: AuthConfig,
}

/// The access-control mode from `06-http-api.md` section 6.4: `solo` trusts
/// the network the API is bound to and performs no check; `token` requires
/// a bearer token, compared in constant time, on every request except
/// health and readiness.
#[derive(Clone)]
pub enum AuthConfig {
    /// No authentication. The default; expected to be paired with binding
    /// to `127.0.0.1` or another trusted network.
    Solo,
    /// `Authorization: Bearer <token>` is required and checked in constant
    /// time against this value.
    Token(String),
}

impl fmt::Debug for AuthConfig {
    /// Never renders the token itself, even in a `{:?}` of a value it's
    /// nested inside — the same discipline `vesica::config::Redacted`
    /// applies to it before it ever reaches here.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthConfig::Solo => f.write_str("Solo"),
            AuthConfig::Token(_) => f.write_str("Token(**REDACTED**)"),
        }
    }
}
