//! The one access-control middleware layer from `06-http-api.md` section
//! 6.4, applied once to the whole router in `router.rs` rather than left to
//! each handler to decide for itself:
//!
//! > The middleware layer that enforces this is written once and applied to
//! > every route including `/mcp`, so introducing real user accounts later
//! > is a change inside that layer rather than across the codebase.
//!
//! Two modes: `solo` (no check — the deployment trusts its network) and
//! `token` (a bearer token, compared in constant time). Liveness and
//! readiness are the only paths exempted, and only because a supervisor
//! must always be able to reach them; see [`require_auth`].

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::config::AuthConfig;
use crate::paths::{HEALTH_PATH, READY_PATH};
use crate::state::AppState;

const BEARER_PREFIX: &str = "Bearer ";

/// The single auth gate every request passes through (see `router.rs`,
/// where this is applied as one outer layer, not per-route). No handler
/// decides for itself whether a request is authenticated.
pub(crate) async fn require_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    // Health and readiness stay unauthenticated in every auth mode: a
    // process supervisor (systemd, a container orchestrator, an installer's
    // post-install check) must always be able to ask "are you alive" and
    // "can you serve", without first needing a credential it may have no
    // way to obtain out-of-band. This is the ticket's explicit requirement,
    // and it is the only exemption — every other route, present or future
    // (including `/mcp` once a later ticket mounts it under this same
    // router), goes through the check below.
    let path = request.uri().path();
    if path == HEALTH_PATH || path == READY_PATH {
        return next.run(request).await;
    }

    match state.auth.as_ref() {
        AuthConfig::Solo => next.run(request).await,
        AuthConfig::Token(expected) => {
            if token_matches(request.headers(), expected) {
                next.run(request).await
            } else {
                unauthorized()
            }
        }
    }
}

/// Extracts the bearer token from `Authorization` (if present and
/// well-formed) and compares it to `expected` in constant time.
fn token_matches(headers: &HeaderMap, expected: &str) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let Some(presented) = value.strip_prefix(BEARER_PREFIX) else {
        return false;
    };
    constant_time_eq(presented.as_bytes(), expected.as_bytes())
}

/// Compares two byte strings without branching on *where* they first
/// differ, so a failed comparison takes the same time regardless of how
/// many leading bytes matched — the property `06-http-api.md` section 6.4
/// requires ("checked in constant time"). Lengths are compared first and
/// may return early on mismatch (a token's length is not secret; only its
/// content is) — but once lengths are equal, every byte is folded through
/// XOR into one accumulator rather than compared with `==` and an early
/// `return false`, so there is no timing signal for *where* a mismatch
/// occurred.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "type": "https://osint.invalid/errors/unauthorized",
            "title": "Missing or invalid bearer token",
            "status": 401,
            "detail": "VESICA_AUTH_MODE=token requires an 'Authorization: Bearer <token>' header matching VESICA_API_TOKEN on every request.",
            "fix": "Send 'Authorization: Bearer <token>' with the exact value configured in VESICA_API_TOKEN.",
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_tokens_match() {
        assert!(constant_time_eq(b"a-shared-secret", b"a-shared-secret"));
    }

    #[test]
    fn different_content_same_length_does_not_match() {
        assert!(!constant_time_eq(b"a-shared-secret", b"b-shared-secret"));
    }

    #[test]
    fn different_length_does_not_match() {
        assert!(!constant_time_eq(b"short", b"a-lot-longer"));
    }

    #[test]
    fn empty_does_not_match_non_empty() {
        assert!(!constant_time_eq(b"", b"x"));
    }

    #[test]
    fn two_empties_match() {
        assert!(constant_time_eq(b"", b""));
    }
}
