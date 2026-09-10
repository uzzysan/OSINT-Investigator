//! The graceful-shutdown trigger `axum::serve(..).with_graceful_shutdown`
//! waits on: either signal tells the server to stop accepting new
//! connections and let in-flight requests finish. Unix-only (`SIGTERM`
//! alongside `SIGINT`/Ctrl-C) because that's the only platform Vesica ships
//! on — see `08-deployment.md` (a `systemd` unit, which sends `SIGTERM` to
//! stop a service).

pub(crate) async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        // Only failure mode is the OS refusing to hand us a signal
        // listener at all, which nothing sensible can recover from; a
        // request currently in flight is better served by shutting down
        // cleanly than by this task silently returning and leaving nothing
        // watching for Ctrl-C.
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install the Ctrl-C (SIGINT) handler");
    };

    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install the SIGTERM handler")
            .recv()
            .await;
    };

    tokio::select! {
        () = ctrl_c => tracing::info!("received SIGINT, shutting down"),
        () = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
