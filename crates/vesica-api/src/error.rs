//! Errors [`crate::serve`] can return. Kept separate from `vesica_store::StoreError`
//! (and any future service-layer error) so the binary can match on exactly
//! the outcomes a supervisor cares about — "the port is taken" is a distinct
//! failure mode from "the database couldn't be opened", and the operator
//! message should say which.

use std::net::SocketAddr;

use thiserror::Error;

/// Everything that can stop [`crate::serve`] before or during the server's
/// run. The `vesica` binary matches on this to produce the specific,
/// actionable startup error the ticket asks for (never a silent port hop).
#[derive(Debug, Error)]
pub enum ServeError {
    /// The configured `(bind_addr, port)` is already bound by another
    /// process. Per `08-deployment.md`, moving to a different port is an
    /// installer-time decision only — a running service that silently moved
    /// would break every agent and browser bookmark pointed at the
    /// configured port, so this is fatal, not retried.
    #[error(
        "address {0} is already in use — another process is bound to this port. \
         Stop it, or change VESICA_PORT and VESICA_PUBLIC_URL, then restart. \
         Vesica does not scan for a free port itself: see 08-deployment.md."
    )]
    AddrInUse(SocketAddr),

    /// Binding failed for a reason other than the address being in use
    /// (e.g. no permission to bind a privileged port, or an unreachable
    /// address).
    #[error("failed to bind {0}: {1}")]
    Bind(SocketAddr, #[source] std::io::Error),

    /// The server's TCP accept loop itself returned an error (as opposed to
    /// a per-connection error, which axum handles per-request).
    #[error("server error: {0}")]
    Serve(#[source] std::io::Error),
}
