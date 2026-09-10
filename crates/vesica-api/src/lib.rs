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
//! Ticket 0001-04 gives this crate its first real behaviour: the axum
//! server itself, liveness (`GET /api/v1/health`) and readiness
//! (`GET /api/v1/ready`), the embedded frontend, the solo/token auth
//! middleware seam, and graceful shutdown. The REST surface for cases,
//! entities, relations, etc. (`06-http-api.md` sections 6.2's other tables)
//! lands in later tickets alongside the service layer they call.

#![forbid(unsafe_code)]

mod assets;
mod auth;
mod config;
mod error;
mod health;
mod paths;
mod router;
mod shutdown;
mod state;

pub use config::{ApiConfig, AuthConfig};
pub use error::ServeError;

use state::AppState;

/// Runs the Vesica HTTP server until it receives `SIGINT`/`SIGTERM`, then
/// drains in-flight requests and closes `db` cleanly before returning.
///
/// Binds `(config.bind_addr, config.port)` and serves:
/// - `/api/v1/health`, `/api/v1/ready` (this ticket),
/// - the embedded frontend at `/` with an SPA fallback,
/// - all behind the one auth middleware layer `06-http-api.md` section 6.4
///   describes, applied once here rather than per-handler.
///
/// Returns [`ServeError::AddrInUse`] rather than moving to a different port
/// when the configured one is already taken — per `08-deployment.md`,
/// picking a different port is an installer-time decision only, because a
/// service that silently moves breaks every agent and browser bookmark
/// pointed at the configured port. The caller (the `vesica` binary) is
/// responsible for turning that into a non-zero exit with a clear message;
/// this function only reports what happened.
pub async fn serve(config: ApiConfig, db: vesica_store::Db) -> Result<(), ServeError> {
    let addr = std::net::SocketAddr::new(config.bind_addr, config.port);

    // Opportunistic, not required: if the blob directory doesn't exist yet
    // (a fresh install), create it so the common case just works. If this
    // fails — including the "already exists but this process can't write
    // into it" case, since `create_dir_all` treats an existing directory as
    // success regardless of its permissions — the server still starts.
    // `GET /api/v1/ready` is where an unwritable blob directory gets
    // reported, on every request, for as long as it stays broken; refusing
    // to start here would turn "alive but unready" into "won't start at
    // all", which is a worse failure mode for an operator to diagnose (no
    // process to `curl` against at all) and is not what the ticket asks
    // for.
    if let Err(e) = std::fs::create_dir_all(&config.blob_dir) {
        tracing::warn!(
            blob_dir = %config.blob_dir.display(),
            error = %e,
            "could not create the blob directory at startup; continuing — \
             GET /api/v1/ready will report this on every check until it's fixed"
        );
    }

    let state = AppState::new(db.clone(), config.blob_dir.clone(), config.auth);
    let app = router::build_router(state);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            return Err(ServeError::AddrInUse(addr));
        }
        Err(e) => return Err(ServeError::Bind(addr, e)),
    };

    tracing::info!(%addr, "vesica listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown::wait_for_shutdown_signal())
        .await
        .map_err(ServeError::Serve)?;

    tracing::info!("in-flight requests drained; closing the database");
    // Closes the reader pool's connections. The write connection closes
    // when its last `Db` clone (this one) drops below, which drops the
    // writer task's channel sender; the writer task then sees its channel
    // closed, exits its loop, and drops the connection it owns — see
    // `vesica_store::writer`.
    db.read_pool().close().await;
    drop(db);

    Ok(())
}
