use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::{Embed, EmbeddedFile};
use wardnetd_services::version::RELEASE_VERSION;

use crate::state::AppState;

/// User-facing PWA — served at `/app/`.
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
    /// Admin mobile PWA at `/admin-app/`. A **premium** surface — gated unless
    /// the box is entitled.
    AdminApp,
    /// Desktop admin website at `/admin/`. Always reachable (even when not
    /// entitled) so the operator can (re)subscribe.
    AdminSite,
    /// User-facing PWA at `/app/`. A **premium** surface — gated unless the
    /// box is entitled.
    ///
    /// Deliberately NOT at the origin root: a PWA whose manifest scope is `/`
    /// encloses every other surface, and Chrome refuses to install an app
    /// whose page sits inside an already-installed app's scope (a distinct
    /// manifest `id` does not override the containment check, and Android
    /// ignores `id` entirely). Sibling scopes — `/app/` next to `/admin-app/`
    /// — are what make the two PWAs installable side by side in either order.
    UserApp,
}

impl Surface {
    /// The path prefix (no slashes) each surface owns. Single source of
    /// truth: [`Self::classify`] matches on it and the serving arm strips it,
    /// so a surface path can never drift between the two.
    const fn prefix(self) -> &'static str {
        match self {
            Self::AdminApp => "admin-app",
            Self::AdminSite => "admin",
            Self::UserApp => "app",
        }
    }

    /// Classify a (leading-slash-trimmed) request path into its app surface
    /// plus the tree-relative asset path.
    ///
    /// The empty asset path is returned both for the canonical scope root
    /// (`app/`) and for the slashless form (`app`); the handler redirects the
    /// slashless form to the trailing-slash URL, because a document served at
    /// `/app` sits *outside* the PWA scope `/app/` (scope matching is string
    /// prefix), where the service worker never controls it and the install
    /// prompt can be withheld.
    ///
    /// `None` means the path matches no surface — the legacy root tree the
    /// user PWA occupied before it moved to `/app/`. Those are answered with
    /// a permanent redirect into `/app/` (see [`legacy_root_redirect`]) so
    /// pre-move bookmarks, deep links, and installed start URLs keep landing.
    fn classify(raw_path: &str) -> Option<(Self, &str)> {
        for surface in [Self::AdminApp, Self::AdminSite, Self::UserApp] {
            // Boundary-checked prefix match: "admin-app/x" must not match the
            // "admin" prefix, so the remainder has to be empty or start "/".
            if let Some(rest) = raw_path.strip_prefix(surface.prefix()) {
                if rest.is_empty() {
                    return Some((surface, ""));
                }
                if let Some(asset_path) = rest.strip_prefix('/') {
                    return Some((surface, asset_path));
                }
            }
        }
        None
    }

    /// Whether this surface is a premium app, blocked unless the box is
    /// entitled (on the wardnet provider and not suspended). The admin
    /// website is never blocked.
    ///
    /// Deliberately an exhaustive `match` (not `matches!`, which falls back to
    /// `_ => false`) so adding a new [`Surface`] variant fails to compile here
    /// until its premium-ness is explicitly decided, instead of silently
    /// defaulting to "not premium".
    fn is_premium(self) -> bool {
        match self {
            Self::AdminApp | Self::UserApp => true,
            Self::AdminSite => false,
        }
    }
}

/// Fallback handler that routes requests to one of the three embedded trees
/// based on path prefix, then serves static files with appropriate cache headers.
///
/// - Paths outside every surface (the user PWA's pre-move root tree) get a
///   permanent redirect into `/app/`, before any other handling; a slashless
///   scope root (`/app`) is canonicalized to its trailing-slash URL so the
///   document always sits inside its PWA scope.
/// - Unless the box is **entitled** (on the wardnet provider and not
///   suspended), the two premium surfaces (user PWA `/app/` and admin mobile
///   app `/admin-app/`) are short-circuited with a premium-required page; the
///   admin website `/admin/` stays reachable so the operator can always
///   (re)subscribe. (`/api/*` never reaches this fallback.) This only gates
///   *fresh* requests — an already-installed PWA is served from its
///   service-worker precache and never re-hits this handler, so `GET
///   /api/info`'s `entitled` field is what lets an installed app self-gate.
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

    let Some((surface, asset_path)) = Surface::classify(raw_path) else {
        return legacy_root_redirect(&uri);
    };

    // Slashless scope root (`/app`, `/admin-app`, `/admin`): canonicalize to
    // the trailing-slash URL instead of serving the shell in place, so every
    // document URL is inside its PWA scope (see `Surface::classify`).
    if raw_path.len() == surface.prefix().len() {
        let target = match uri.query() {
            Some(query) => format!("/{}/?{query}", surface.prefix()),
            None => format!("/{}/", surface.prefix()),
        };
        return permanent_redirect(&target);
    }

    // Entitlement gate — checked *before* the `.info`/304 shortcuts so a cached
    // build can't slip a premium surface past the block via `If-None-Match`. The
    // premium-required page is itself `no-store`, so becoming entitled clears
    // it without a hard refresh.
    if surface.is_premium() && !state.is_entitled() {
        return premium_required_response();
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

    let (file, fallback) = match surface {
        Surface::AdminApp => (
            AdminAppAssets::get(asset_path),
            AdminAppAssets::get("index.html"),
        ),
        Surface::AdminSite => (
            AdminSiteAssets::get(asset_path),
            AdminSiteAssets::get("index.html"),
        ),
        Surface::UserApp => (UserAssets::get(asset_path), UserAssets::get("index.html")),
    };
    serve_spa(file, fallback, asset_path, &etag)
}

/// A 308 with an explicit, bounded cache lifetime. Hand-rolled (rather than
/// `axum::response::Redirect::permanent`) because the Cache-Control header
/// matters: browsers cache header-less permanent redirects until eviction, so
/// an unbounded 308 would keep bouncing clients even after a future release
/// claims the redirected path for something real (a new surface,
/// `/.well-known/*`). One day keeps the permanent "update your bookmark"
/// semantics while capping how long a stale redirect can mask a new resource.
fn permanent_redirect(target: &str) -> Response {
    (
        StatusCode::PERMANENT_REDIRECT,
        [
            (header::LOCATION, target),
            (header::CACHE_CONTROL, "max-age=86400"),
        ],
    )
        .into_response()
}

/// Permanently redirect a legacy root-tree path into the user PWA's `/app/`
/// scope, preserving the rest of the path and any query string — `/` lands on
/// `/app/`, a pre-move deep link like `/dns?x=1` lands on `/app/dns?x=1`.
fn legacy_root_redirect(uri: &Uri) -> Response {
    let path_and_query = uri.path_and_query().map_or(uri.path(), |pq| pq.as_str());
    permanent_redirect(&format!("/app{path_and_query}"))
}

/// The page served at a premium surface when the box is not entitled — either
/// it never subscribed (free/BYO-domain) or a prior subscription is suspended.
/// Self-contained (no external assets — those would themselves be gated) and
/// `no-store` so it's never cached: the moment the box becomes entitled the
/// real app loads on the next navigation. Links to the admin website, which
/// stays reachable, so the operator can (re)subscribe.
///
/// Keep this copy in sync with the `PremiumRequired` React component in
/// `source/web/src/components/ConnectionGate.tsx` — that's the client-side
/// equivalent of this same "premium required" state (it fires for an
/// already-installed PWA whose shell is served from the service-worker
/// precache and never re-hits this handler).
fn premium_required_response() -> Response {
    const PREMIUM_REQUIRED_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>wardnet — premium required</title>
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
  <h1>This app requires wardnet Premium</h1>
  <p>The mobile apps are a Premium capability. Subscribe or check your
     subscription status from the admin website.
     Everything on your network keeps running locally — only these apps are paused.</p>
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
        PREMIUM_REQUIRED_HTML,
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
