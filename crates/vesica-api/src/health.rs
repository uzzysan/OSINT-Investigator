//! Liveness and readiness — `06-http-api.md` section 6.2 "Operations" and
//! `03-architecture.md` section 3.8 "Observability":
//!
//! > `/api/v1/health` (liveness) and `/api/v1/ready` (migrations applied, DB
//! > writable, blob dir writable).
//!
//! The two answer different questions. Liveness answers "is the process
//! alive and able to respond at all" — a supervisor restarts the process
//! when this stops answering, so it must never depend on anything that
//! could itself be the thing that's broken (the database, the filesystem).
//! Readiness answers "can this instance actually do its job right now" — an
//! installer or a load balancer uses it to decide whether to send traffic,
//! and it is allowed to say no, with a reason.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::{Value, json};

use vesica_store::StoreError;

use crate::state::AppState;

/// `GET /api/v1/health`: liveness. Deliberately touches nothing but the
/// process itself — no database, no filesystem — because the whole point of
/// keeping it separate from readiness is that it stays true for exactly as
/// long as the process is alive and its request loop is not wedged. A
/// supervisor restarts the process on this failing; it says nothing about
/// whether the service can actually do useful work (that's `/ready`).
pub(crate) async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

/// `GET /api/v1/ready`: readiness. Runs three independent checks — schema
/// migrations are fully applied, the database accepts a real write, the
/// blob directory accepts a real write — and reports 200 only if every one
/// of them passed. On failure, returns 503 with a diagnostic body naming
/// exactly which check(s) failed and why, so an operator (or an installer's
/// post-install check) doesn't have to guess. A data directory with the
/// wrong ownership is the textbook case this exists for: the process is
/// alive (health still says so) but cannot do its job, and readiness is
/// where that gets caught instead of surfacing as a broken write later.
pub(crate) async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let migrations = check_migrations(&state).await;
    let database = check_database_writable(&state).await;
    let blobs = check_blob_dir_writable(&state).await;

    let all_ok = migrations.is_ok() && database.is_ok() && blobs.is_ok();

    let body = json!({
        "status": if all_ok { "ok" } else { "unready" },
        "checks": {
            "migrations": check_json(&migrations),
            "database_writable": check_json(&database),
            "blob_dir_writable": check_json(&blobs),
        }
    });

    let status = if all_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(body))
}

fn check_json(result: &Result<(), String>) -> Value {
    match result {
        Ok(()) => json!({ "ok": true }),
        Err(detail) => json!({ "ok": false, "detail": detail }),
    }
}

/// Confirms the database has every migration this build knows about
/// applied — not just that migrations ran successfully at some point in the
/// past (they did, or `vesica serve` would never have started; see
/// `vesica_store::Db::open`), but that the number recorded in the database
/// right now, read fresh off the reader pool, is the number this binary
/// expects. This also doubles as a read-connectivity probe: if the reader
/// pool can't reach the database at all, the query itself fails and that
/// surfaces as this check's error.
async fn check_migrations(state: &AppState) -> Result<(), String> {
    let current: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_migration")
        .fetch_one(state.db.read_pool())
        .await
        .map_err(|e| format!("could not read migration state: {e}"))?;

    if current < state.expected_migration_version {
        return Err(format!(
            "database is at migration {current}, this build expects {} — run `vesica migrate`",
            state.expected_migration_version
        ));
    }
    Ok(())
}

/// A fixed, obviously-synthetic row identity for the write probe below.
/// Chosen so a stray probe row would be unmistakable in a real case list —
/// it never should be, because the probe transaction is always rolled back,
/// but a diagnostic that fails safe if that ever changes is worth the
/// two constants.
const PROBE_CASE_ID: &str = "00000000000READINESSPROBE";
const PROBE_CASE_SLUG: &str = "__vesica_readiness_probe__";

/// Proves the database actually accepts a write right now, by attempting
/// one and rolling it back — not merely that the file opened successfully
/// at startup (`vesica_store::Db::open` already proved that, or the process
/// would not have reached this handler). This is what catches a data
/// directory whose ownership changed after the process started: the file
/// descriptor may still be open, but SQLite's own write path — acquiring
/// the write lock, appending to the WAL — fails the same way a genuinely
/// read-only database does.
///
/// The probe writes into `kase` (case) with a fixed, obviously-synthetic id
/// and slug (see [`PROBE_CASE_ID`]) inside an explicit transaction that is
/// unconditionally rolled back, so it never leaves a trace in real data —
/// not even transiently to a concurrent reader, since a reader pool
/// connection under WAL never observes an uncommitted write.
async fn check_database_writable(state: &AppState) -> Result<(), String> {
    state
        .db
        .write(|conn| {
            Box::pin(async move {
                sqlx::query("BEGIN IMMEDIATE")
                    .execute(&mut *conn)
                    .await
                    .map_err(StoreError::from)?;

                let insert = sqlx::query(
                    "INSERT INTO kase (id, slug, name, status, created_at, updated_at) \
                     VALUES (?, ?, 'readiness probe', 'active', '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z')",
                )
                .bind(PROBE_CASE_ID)
                .bind(PROBE_CASE_SLUG)
                .execute(&mut *conn)
                .await;

                // Roll back unconditionally: the probe must never persist
                // either way, and the connection must be back in
                // autocommit state before the writer task picks up the
                // next queued job regardless of whether the insert above
                // succeeded.
                let rollback = sqlx::query("ROLLBACK").execute(&mut *conn).await;

                insert.map_err(StoreError::from)?;
                rollback.map_err(StoreError::from)?;
                Ok(())
            })
        })
        .await
        .map_err(|e| format!("database is not writable: {e}"))
}

/// Proves the configured blob directory (`VESICA_BLOB_DIR`) actually
/// accepts a write from this process right now, by writing and then
/// removing a small probe file. Filesystem permission checks happen at
/// `open()`/`write()` time, not once at startup, so this genuinely reflects
/// the directory's current state — including a `chmod` applied after the
/// process started, which is exactly the "wrong ownership" scenario this
/// check exists to catch.
async fn check_blob_dir_writable(state: &AppState) -> Result<(), String> {
    let probe_path = state
        .blob_dir
        .join(format!(".vesica-ready-probe-{}", std::process::id()));

    let write_result = tokio::fs::write(&probe_path, b"vesica readiness probe").await;
    // Attempt to clean up regardless of whether the write succeeded — a
    // partially-written probe file must not linger in the blob directory.
    let _ = tokio::fs::remove_file(&probe_path).await;

    write_result.map_err(|e| {
        format!(
            "blob directory '{}' is not writable: {e}",
            state.blob_dir.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::Request;
    use tower::ServiceExt;

    use crate::config::AuthConfig;
    use crate::router::build_router;
    use crate::state::AppState;

    async fn state_with_blob_dir(blob_dir: std::path::PathBuf) -> AppState {
        let store = vesica_store::testing::test_store()
            .await
            .expect("test store");
        // Leak the TestStore so its TempDir (the database's own directory)
        // outlives the test body — dropping it would delete the sqlite
        // file out from under the router under test.
        let store: &'static vesica_store::testing::TestStore = Box::leak(Box::new(store));
        AppState::new(store.db.clone(), blob_dir, AuthConfig::Solo)
    }

    #[tokio::test]
    async fn health_returns_ok_without_a_working_database() {
        // A `Db` whose writer task has definitely stopped: build one, then
        // drop every clone of it. `health` must not care.
        let store = vesica_store::testing::test_store().await.unwrap();
        let db = store.db.clone();
        drop(store); // removes the directory the sqlite file lived in

        let blob_dir = tempfile::TempDir::new().unwrap();
        let state = AppState::new(db, blob_dir.path().to_path_buf(), AuthConfig::Solo);
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_reports_ok_when_everything_is_writable() {
        let blob_dir = tempfile::TempDir::new().unwrap();
        let state = state_with_blob_dir(blob_dir.path().to_path_buf()).await;
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/ready")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["checks"]["migrations"]["ok"], true);
        assert_eq!(json["checks"]["database_writable"]["ok"], true);
        assert_eq!(json["checks"]["blob_dir_writable"]["ok"], true);
    }

    #[tokio::test]
    async fn ready_reports_503_and_names_the_blob_dir_when_it_is_unwritable() {
        // A plain *file* sitting where the blob directory is configured to
        // be. Writing into it (`blob_dir.join(probe_name)`) always fails —
        // every path under it hits `ENOTDIR` — regardless of the running
        // user's privileges. That matters here specifically because this
        // suite (like a real `vesica.service` container) may run as root,
        // for whom ordinary permission bits (`chmod`) are not enforced at
        // all, so a `chmod`-based test would silently pass even when the
        // check it's meant to exercise is broken. A deployment's "data
        // directory has the wrong ownership" and this "not a directory at
        // all" both surface identically to the check below: an `io::Error`
        // readiness reports rather than panicking on.
        let parent = tempfile::TempDir::new().unwrap();
        let not_a_directory = parent.path().join("blobs");
        std::fs::write(&not_a_directory, b"not a directory").unwrap();

        let state = state_with_blob_dir(not_a_directory).await;
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/ready")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "unready");
        assert_eq!(json["checks"]["blob_dir_writable"]["ok"], false);
        let detail = json["checks"]["blob_dir_writable"]["detail"]
            .as_str()
            .unwrap();
        assert!(
            detail.contains("not writable"),
            "diagnostic should say the blob dir isn't writable: {detail}"
        );
        // The other two checks must still be reported, and still pass —
        // one failing check must not hide the others' results.
        assert_eq!(json["checks"]["migrations"]["ok"], true);
        assert_eq!(json["checks"]["database_writable"]["ok"], true);
    }

    #[tokio::test]
    async fn health_and_ready_never_require_a_token() {
        let blob_dir = tempfile::TempDir::new().unwrap();
        let store = vesica_store::testing::test_store().await.unwrap();
        let store: &'static vesica_store::testing::TestStore = Box::leak(Box::new(store));
        let state = AppState::new(
            store.db.clone(),
            blob_dir.path().to_path_buf(),
            AuthConfig::Token("shhh".to_string()),
        );
        let app = build_router(state);

        for path in ["/api/v1/health", "/api/v1/ready"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            // No Authorization header was sent at all, yet neither path is
            // rejected with 401 — that's the exemption under test.
            assert_ne!(
                response.status(),
                axum::http::StatusCode::UNAUTHORIZED,
                "{path} must be reachable without a token"
            );
        }
    }
}
