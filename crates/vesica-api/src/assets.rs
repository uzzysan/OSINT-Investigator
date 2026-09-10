//! Serves the built frontend (`web/dist/`) embedded into the binary, so
//! there is never a static directory on disk that can disagree with what
//! was actually built — see `03-architecture.md` section 3.5: "Built with
//! `vite build`, output embedded into the binary via `rust-embed`."
//!
//! `allow_missing = true` below is deliberate: it lets `cargo build` and
//! `cargo test` succeed on a checkout where `npm run build` hasn't been run
//! yet in `web/` (the frontend and the Rust workspace are built by separate
//! toolchains, and the ticket's own verification runs `cargo build` before
//! `npm run build`). When the frontend truly is missing, [`asset_handler`]
//! says so plainly instead of panicking or serving nothing with no
//! explanation.
//!
//! `rust-embed` itself draws the release/debug line: a `--release` binary
//! truly embeds `web/dist/`'s bytes at compile time (the single-binary,
//! "no disk dependency at runtime" property this exists for); an
//! unoptimized `cargo build`/`cargo run` instead reads the same
//! `web/dist/` straight off disk on every request, so editing a built asset
//! doesn't need a Rust rebuild to see. Both paths go through this one
//! `WebAssets`/`asset_handler` pair and the one `web/dist/` directory — there
//! is still no second, independent "static files" tree anything could
//! disagree with.

use axum::body::Body;
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../web/dist/"]
#[allow_missing = true]
struct WebAssets;

/// `GET /` and every other path not claimed by `/api/*`: serves the
/// matching file out of the embedded Vite build if there is one, otherwise
/// falls back to `index.html` — the SPA fallback a client-side router needs
/// (a deep link like `/cases/abc123` has no matching embedded file, but
/// must still return the app shell, not a 404).
pub(crate) async fn asset_handler(request: Request) -> Response {
    let path = request.uri().path().trim_start_matches('/');
    let candidate = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = WebAssets::get(candidate) {
        return file_response(candidate, file);
    }
    if let Some(index) = WebAssets::get("index.html") {
        return file_response("index.html", index);
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "web assets not built: run `npm run build` in web/, then rebuild the vesica binary.",
    )
        .into_response()
}

fn file_response(path: &str, file: rust_embed::EmbeddedFile) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(path))
        .body(Body::from(file.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// A small, explicit extension-to-MIME-type table rather than a general
/// "guess the content type" crate: a Vite build output is a known, bounded
/// set of file kinds, and getting this wrong (or silently falling back to
/// `application/octet-stream`, which some browsers refuse to execute as a
/// script/stylesheet) is exactly the kind of bug "correct content types"
/// exists to prevent.
fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::content_type;

    #[test]
    fn known_extensions_get_specific_mime_types() {
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            content_type("assets/index-abc123.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type("assets/index-abc123.css"),
            "text/css; charset=utf-8"
        );
        assert_eq!(content_type("favicon.svg"), "image/svg+xml");
    }

    #[test]
    fn unknown_extension_falls_back_to_octet_stream() {
        assert_eq!(
            content_type("assets/whatever.bin"),
            "application/octet-stream"
        );
        assert_eq!(
            content_type("no-extension-at-all"),
            "application/octet-stream"
        );
    }
}
