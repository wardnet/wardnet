use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::{Embed, EmbeddedFile};
use wardnetd_services::version::RELEASE_VERSION;

use crate::state::AppState;

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

/// Which embedded app a request resolves to, by path prefix.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Surface {
    /// Admin mobile PWA at `/admin-app/`. A **premium** surface — gated while
    /// suspended.
    AdminApp,
    /// Desktop admin website at `/admin/`. Always reachable (even suspended) so
    /// the operator can resubscribe.
    AdminSite,
    /// User-facing PWA at `/` (everything else). A **premium** surface — gated
    /// while suspended.
    UserApp,
}

impl Surface {
    /// Classify a (leading-slash-trimmed) request path into its app surface.
    fn classify(raw_path: &str) -> Self {
        if raw_path.starts_with("admin-app/") || raw_path == "admin-app" {
            Self::AdminApp
        } else if raw_path.starts_with("admin/") || raw_path == "admin" {
            Self::AdminSite
        } else {
            Self::UserApp
        }
    }

    /// Whether this surface is a premium app blocked while the subscription is
    /// suspended. The admin website is never blocked.
    fn is_premium(self) -> bool {
        matches!(self, Self::AdminApp | Self::UserApp)
    }
}

/// Fallback handler that routes requests to one of the three embedded trees
/// based on path prefix, then serves static files with appropriate cache headers.
///
/// - When the wardnet subscription is **suspended**, the two premium surfaces
///   (user PWA `/` and admin mobile app `/admin-app/`) are short-circuited with a
///   suspended page; the admin website `/admin/` stays reachable so the operator
///   can always resubscribe. (`/api/*` never reaches this fallback.)
/// - Content-hashed assets under `/assets/` in each tree get
///   `Cache-Control: immutable` (safe to cache indefinitely because a new
///   build produces new filenames).
/// - `index.html` and every other non-hashed path get `Cache-Control: no-cache`
///   so the browser always revalidates after a daemon upgrade.
/// - Every response carries an `ETag` tied to `RELEASE_VERSION`.
///   A matching `If-None-Match` yields a 304 Not Modified.
pub async fn static_handler(
    State(state): State<AppState>,
    uri: Uri,
    req_headers: HeaderMap,
) -> Response {
    let raw_path = uri.path().trim_start_matches('/');
    let etag = format!("\"{RELEASE_VERSION}\"");

    let surface = Surface::classify(raw_path);

    // Suspended gate — checked *before* the `.info`/304 shortcuts so a cached
    // build can't slip a premium surface past the block via `If-None-Match`. The
    // suspended page is itself `no-store`, so restoring the subscription clears
    // it without a hard refresh.
    if surface.is_premium() && state.is_suspended() {
        return suspended_response();
    }

    // `.info` is the git-tracked sentinel that keeps each `dist/` directory
    // present so this crate compiles before `make build-web` has produced real
    // assets (see `source/*/dist/.info`). It is embedded but must never be
    // served — fall through to a plain 404 for any request targeting it.
    if raw_path == ".info" || raw_path.ends_with("/.info") {
        return StatusCode::NOT_FOUND.into_response();
    }

    // 304 shortcut: skip the body when the client already has this build.
    if req_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag)
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag.as_str())]).into_response();
    }

    match surface {
        Surface::AdminApp => {
            let asset_path = raw_path.strip_prefix("admin-app/").unwrap_or("");
            serve_spa(
                AdminAppAssets::get(asset_path),
                AdminAppAssets::get("index.html"),
                asset_path,
                &etag,
            )
        }
        Surface::AdminSite => {
            let asset_path = raw_path.strip_prefix("admin/").unwrap_or("");
            serve_spa(
                AdminSiteAssets::get(asset_path),
                AdminSiteAssets::get("index.html"),
                asset_path,
                &etag,
            )
        }
        Surface::UserApp => serve_spa(
            UserAssets::get(raw_path),
            UserAssets::get("index.html"),
            raw_path,
            &etag,
        ),
    }
}

/// The page served at a premium surface while the subscription is suspended.
/// Self-contained (no external assets — those would themselves be gated) and
/// `no-store` so it's never cached: the moment the subscription is restored the
/// real app loads on the next navigation. Links to the admin website, which
/// stays reachable, so the operator can resubscribe.
fn suspended_response() -> Response {
    const SUSPENDED_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>wardnet — subscription paused</title>
<style>
  body { font-family: system-ui, sans-serif; background: #0b0d12; color: #e6e9ef;
         display: grid; place-items: center; min-height: 100vh; margin: 0; padding: 1.5rem; }
  main { max-width: 28rem; text-align: center; }
  h1 { font-size: 1.4rem; margin: 0 0 0.75rem; }
  p { line-height: 1.6; color: #aab2c0; }
  a { display: inline-block; margin-top: 1.25rem; padding: 0.6rem 1.1rem;
      background: #4c6fff; color: #fff; text-decoration: none; border-radius: 0.5rem; }
</style>
</head>
<body>
<main>
  <h1>Your wardnet subscription is paused</h1>
  <p>This app is unavailable until the subscription on this network is active again.
     Everything on your network keeps running locally — only remote access is paused.</p>
  <a href="/admin/">Manage subscription</a>
</main>
</body>
</html>"#;

    (
        StatusCode::FORBIDDEN,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        SUSPENDED_HTML,
    )
        .into_response()
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
