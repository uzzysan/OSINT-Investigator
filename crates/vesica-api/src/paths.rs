//! Route paths shared between the router (which registers them) and the
//! auth middleware (which decides which of them skip the check) so the two
//! can never silently drift apart.

/// Liveness. Never requires authentication — see `auth.rs` for why.
pub(crate) const HEALTH_PATH: &str = "/api/v1/health";
/// Readiness. Never requires authentication — see `auth.rs` for why.
pub(crate) const READY_PATH: &str = "/api/v1/ready";
