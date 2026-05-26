use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;
use wardnetd_services::version::RELEASE_VERSION;

/// Embedded web UI assets compiled into the binary.
///
/// In debug mode, reads files from the filesystem (no rebuild needed
/// for UI changes). In release mode, all files are baked into the binary.
#[derive(Embed)]
#[folder = "../../../admin-app/web/dist"]
struct Assets;

/// Fallback handler that serves embedded static files with appropriate cache headers.
///
/// - Content-hashed assets under `/assets/` get `Cache-Control: immutable` (safe to
///   cache indefinitely because a new build produces new filenames).
/// - `index.html` and any other top-level path get `Cache-Control: no-cache` so the
///   browser always revalidates after a daemon upgrade.
/// - Every response includes an `ETag` tied to the daemon's `RELEASE_VERSION`.
///   A matching `If-None-Match` yields a 304 Not Modified.
pub async fn static_handler(uri: Uri, req_headers: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    let etag = format!("\"{}\"", RELEASE_VERSION);

    // 304 shortcut: skip the body when the client already has this build.
    if req_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag)
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag.as_str())]).into_response();
    }

    // Content-hashed assets under /assets/ are immutable at a given URL;
    // everything else must revalidate so upgrades take effect without a hard-refresh.
    let cache_ctrl = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, mime.as_ref()),
                (header::CACHE_CONTROL, cache_ctrl),
                (header::ETAG, etag.as_str()),
            ],
            file.data,
        )
            .into_response();
    }

    // SPA fallback: serve index.html for all unmatched client-side routes.
    match Assets::get("index.html") {
        Some(file) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
                (header::ETAG, etag.as_str()),
            ],
            file.data,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
