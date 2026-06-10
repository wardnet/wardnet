use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::{Embed, EmbeddedFile};
use wardnetd_services::version::RELEASE_VERSION;

/// User-facing PWA — served at `/` (all paths not matched by the blocks below).
#[derive(Embed)]
#[folder = "../../../user-app/dist"]
struct UserAssets;

/// Admin mobile PWA — served at `/admin-app/`.
#[derive(Embed)]
#[folder = "../../../admin-app/dist"]
struct AdminAppAssets;

/// Desktop admin site — served at `/admin/`.
#[derive(Embed)]
#[folder = "../../../admin-site/dist"]
struct AdminSiteAssets;

/// Fallback handler that routes requests to one of the three embedded trees
/// based on path prefix, then serves static files with appropriate cache headers.
///
/// - Content-hashed assets under `/assets/` in each tree get
///   `Cache-Control: immutable` (safe to cache indefinitely because a new
///   build produces new filenames).
/// - `index.html` and every other non-hashed path get `Cache-Control: no-cache`
///   so the browser always revalidates after a daemon upgrade.
/// - Every response carries an `ETag` tied to `RELEASE_VERSION`.
///   A matching `If-None-Match` yields a 304 Not Modified.
pub async fn static_handler(uri: Uri, req_headers: HeaderMap) -> Response {
    let raw_path = uri.path().trim_start_matches('/');
    let etag = format!("\"{RELEASE_VERSION}\"");

    // 304 shortcut: skip the body when the client already has this build.
    if req_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag)
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag.as_str())]).into_response();
    }

    if raw_path.starts_with("admin-app/") || raw_path == "admin-app" {
        let asset_path = raw_path.strip_prefix("admin-app/").unwrap_or("");
        serve_spa(
            AdminAppAssets::get(asset_path),
            AdminAppAssets::get("index.html"),
            asset_path,
            &etag,
        )
    } else if raw_path.starts_with("admin/") || raw_path == "admin" {
        let asset_path = raw_path.strip_prefix("admin/").unwrap_or("");
        serve_spa(
            AdminSiteAssets::get(asset_path),
            AdminSiteAssets::get("index.html"),
            asset_path,
            &etag,
        )
    } else {
        serve_spa(
            UserAssets::get(raw_path),
            UserAssets::get("index.html"),
            raw_path,
            &etag,
        )
    }
}

/// Serve a file from an embedded tree, falling back to `index.html` for SPA routing.
///
/// `path` is the tree-relative path (prefix already stripped), used only for
/// MIME detection and determining whether the asset is content-hashed.
fn serve_spa(
    file: Option<EmbeddedFile>,
    fallback: Option<EmbeddedFile>,
    path: &str,
    etag: &str,
) -> Response {
    // Content-hashed assets under /assets/ are immutable at a given URL;
    // everything else must revalidate so upgrades take effect without a hard-refresh.
    let cache_ctrl = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    if let Some(file) = file {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, mime.as_ref()),
                (header::CACHE_CONTROL, cache_ctrl),
                (header::ETAG, etag),
            ],
            file.data,
        )
            .into_response();
    }

    // SPA fallback: serve index.html for all unmatched client-side routes.
    match fallback {
        Some(file) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
                (header::ETAG, etag),
            ],
            file.data,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
