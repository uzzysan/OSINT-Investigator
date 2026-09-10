//! Assembles the whole HTTP surface: the versioned API under `/api`, the
//! embedded frontend everywhere else, and the one auth layer wrapping both
//! — see `auth.rs` for why that layer sits here, outside every route,
//! rather than inside any individual handler.

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use tower_http::trace::TraceLayer;

use crate::assets::asset_handler;
use crate::auth::require_auth;
use crate::health::{health, ready};
use crate::state::AppState;

/// Builds the complete router: `/api/v1/health` and `/api/v1/ready` today
/// (this ticket); every other `/api/*` path 404s with a JSON problem body
/// rather than falling through to the SPA fallback, since an agent hitting
/// a typo'd REST path should get a clear "no such endpoint", not an HTML
/// page. Everything outside `/api` is the embedded frontend, with the SPA
/// fallback `asset_handler` itself implements.
pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/v1/health", get(health))
        .route("/v1/ready", get(ready))
        .fallback(api_not_found);

    Router::new()
        .nest("/api", api)
        .fallback(asset_handler)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn api_not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "type": "https://osint.invalid/errors/not-found",
            "title": "No such API endpoint",
            "status": 404,
        })),
    )
}
