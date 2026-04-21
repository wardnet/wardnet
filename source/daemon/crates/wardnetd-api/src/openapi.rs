//! `OpenAPI` document builder.
//!
//! The [`ApiDoc`] struct is the schema root — utoipa's `#[derive(OpenApi)]`
//! collects metadata and declares the reusable security schemes that handlers
//! reference by name in their `#[utoipa::path(security(...))]` blocks.
//!
//! Handler paths themselves are not listed here: each module under `api/`
//! registers its annotated handlers through `utoipa_axum::router::OpenApiRouter`,
//! which merges them into the `ApiDoc` at startup via `split_for_parts()`.

// The `#[derive(OpenApi)]` macro expansion uses `for_each` in code generated
// by utoipa — we can't control that, so suppress the pedantic lint here.
#![allow(clippy::needless_for_each)]

use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

/// Root `OpenAPI` document for the Wardnet daemon.
///
/// Carries the document metadata (title, description, license), the tag list
/// used to group endpoints in the generated UI, and — through
/// [`SecurityAddon`] — the `session_cookie` and `bearer_auth` security schemes
/// referenced by every admin-gated handler.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Wardnet API",
        description = "Self-hosted network privacy gateway — REST API for device, \
                       tunnel, routing, DHCP, DNS, and update management.",
        license(name = "MIT")
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "auth", description = "Session login / logout"),
        (name = "setup", description = "First-run setup wizard"),
        (name = "info", description = "Unauthenticated daemon info"),
        (name = "devices", description = "Device discovery and routing"),
        (name = "tunnels", description = "WireGuard tunnel lifecycle"),
        (name = "providers", description = "VPN provider integration"),
        (name = "dhcp", description = "DHCP server configuration, leases, and reservations"),
        (name = "dns", description = "DNS resolver, ad-blocking, filters"),
        (name = "system", description = "Runtime status and logs"),
        (name = "jobs", description = "Background job status"),
        (name = "update", description = "Auto-update and rollback"),
    )
)]
pub struct ApiDoc;

/// Declares the two authentication mechanisms every admin-gated handler
/// references by name:
///
/// - `session_cookie` — the `wardnet_session` cookie issued by `POST /api/auth/login`.
/// - `bearer_auth` — an opaque API key supplied via `Authorization: Bearer <key>`.
///
/// Both are registered as reusable components so handlers only need to list
/// the scheme name in their `security(...)` block.
struct SecurityAddon;

/// Wardnet logo served from `/api/docs/logo.png` and referenced by Scalar's
/// `logoUrl` config so the docs page shows the brand mark at the top of the
/// sidebar. Shared with the web UI — `include_bytes!` pulls the single
/// canonical copy so the daemon rebuilds whenever the asset changes.
pub const LOGO_PNG: &[u8] = include_bytes!("../../../../web-ui/src/assets/logo.png");

/// HTML for the `/api/docs` Scalar UI.
///
/// Pulled in via Scalar's public CDN script (`@scalar/api-reference`), with a
/// `<style>` block that retargets Scalar's built-in `--scalar-sidebar-*` CSS
/// variables to Wardnet's palette (deep indigo background, green accent).
/// The overrides are scoped to both `.dark-mode .sidebar` and `.light-mode
/// .sidebar` so the branded sidebar survives theme toggles. All other Scalar
/// surfaces use its default theme.
///
/// The API spec is loaded from `/api/openapi.json` at runtime — both routes
/// are admin-gated on the server side, so an unauthenticated visitor sees a
/// 401 on either call.
pub const SCALAR_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Wardnet API</title>
  <style>
    /* Modern Scalar attaches the sidebar CSS vars to its `.dark-mode`
       / `.light-mode` scope on the app root, not to a nested `.sidebar`
       container. Targeting the same scope lets us override at equal
       specificity without needing `!important`. */
    .dark-mode,
    .light-mode {
      --scalar-sidebar-background-1: oklch(0.2 0.1 275);
      --scalar-sidebar-color-1: oklch(0.9 0.01 240);
      --scalar-sidebar-color-2: oklch(0.9 0.01 240 / 0.55);
      --scalar-sidebar-color-active: oklch(0.72 0.16 145);
      --scalar-sidebar-item-hover-background: oklch(0.26 0.1 275);
      --scalar-sidebar-item-hover-color: oklch(0.95 0.005 240);
      --scalar-sidebar-item-active-background: oklch(0.26 0.1 275);
      --scalar-sidebar-border-color: oklch(1 0 0 / 10%);
      --scalar-sidebar-search-background: oklch(0.26 0.1 275);
      --scalar-sidebar-search-border-color: oklch(1 0 0 / 10%);
      --scalar-sidebar-search-color: oklch(0.9 0.01 240);
    }
    /* Custom brand mark prepended into Scalar's sidebar by the script below.
       Scalar doesn't ship a top-of-sidebar logo config yet
       (github.com/scalar/scalar/discussions/914); DOM injection is the
       documented workaround. The container matches the sidebar palette
       and spacing so the logo feels native. */
    /* Mirror the admin web UI sidebar header exactly:
       container `p-4` + `gap-2.5`; logo 28px; title `text-lg` bold
       `tracking-tight`, color `--primary` (green). Kept in lockstep so the
       docs page feels like a continuation of the admin app. */
    .wardnet-brand {
      display: flex;
      align-items: center;
      gap: 0.625rem;
      padding: 1rem;
    }
    .wardnet-brand img {
      width: 28px;
      height: 28px;
      border-radius: 0.375rem;
    }
    .wardnet-brand span {
      font-size: 1.125rem;
      font-weight: 700;
      letter-spacing: -0.025em;
      color: oklch(0.72 0.16 145);
    }
  </style>
</head>
<body>
  <div id="wardnet-api-docs"></div>
  <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
  <script>
    // Scalar 2.x programmatic API — gives us access to nested config keys
    // (`agent`, `showDeveloperTools`, `sources`) that the `data-configuration`
    // attribute can't round-trip cleanly. The Ask-AI composer is disabled
    // because Wardnet is a privacy tool and we don't want to ship an LLM
    // widget in our own docs. `/favicon-32.png` is the same file the admin
    // SPA uses, served by the daemon's rust-embed static handler.
    //
    // `logoUrl` is a per-source property in modern Scalar (not top-level),
    // so the brand logo lives inside the `sources` entry rather than on the
    // config root. The standalone bundle exposes the factory under
    // `window.Scalar`.
    Scalar.createApiReference('#wardnet-api-docs', {
      url: '/api/openapi.json',
      favicon: '/favicon-32.png',
      agent: { disabled: true },
      mcp: { disabled: true },
      showDeveloperTools: 'never',
      hideDarkModeToggle: true,
      hideClientButton: true,
    });

    // Inject the Wardnet brand mark at the top of Scalar's sidebar. The
    // top-of-sidebar logo isn't a config knob upstream
    // (see github.com/scalar/scalar/discussions/914 — PR #4215 tracks it),
    // so we watch for the sidebar element to mount and prepend our own node.
    // The observer auto-disconnects after the first successful insertion.
    (function injectBrand() {
      // Scalar's sidebar root uses the `t-doc__sidebar` class (verified
      // against the bundle — generic `.sidebar` selectors match smaller
      // children like the search placeholder and inject in the wrong place).
      const observer = new MutationObserver(() => {
        const sidebar = document.querySelector('.t-doc__sidebar');
        if (!sidebar || sidebar.querySelector('.wardnet-brand')) return;
        const brand = document.createElement('div');
        brand.className = 'wardnet-brand';
        brand.innerHTML =
          '<img src="/api/docs/logo.png" alt="" /><span>Wardnet</span>';
        sidebar.prepend(brand);
        observer.disconnect();
      });
      observer.observe(document.body, { childList: true, subtree: true });
    })();
  </script>
</body>
</html>"#;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session_cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("wardnet_session"))),
        );
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("opaque")
                    .build(),
            ),
        );
    }
}
