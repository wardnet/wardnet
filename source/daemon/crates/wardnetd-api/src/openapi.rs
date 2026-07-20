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
use wardnetd_services::version::RELEASE_VERSION;

/// Root `OpenAPI` document for the Wardnet daemon.
///
/// Carries the document metadata (title, description, license), the tag list
/// used to group endpoints in the generated UI, the document-level security
/// default, and — through [`SecurityAddon`] — the `session_cookie` and
/// `bearer_auth` security schemes it references.
///
/// The `security(...)` block below applies to every operation that does not
/// declare its own: admin-gated handlers need no per-operation annotation,
/// and a handler that forgets one is documented as authenticated (the safe
/// direction) instead of silently shipping as public. Deliberately
/// unauthenticated endpoints opt out visibly with `security(())` in their
/// `#[utoipa::path]` attribute.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Wardnet API",
        // Pin the spec version to the public-facing CalVer that
        // wardnetd-services::version::RELEASE_VERSION exposes (read from
        // the workspace-root CALVER file at compile time). Using the
        // CalVer here matches what the daemon serves via /api/info and
        // what the user sees in the web UI, so the spec, the API
        // response, and the UI all surface the same release identifier.
        version = RELEASE_VERSION,
        description = "Self-hosted network privacy gateway — REST API for device, \
                       tunnel, routing, DHCP, DNS, and update management.",
        license(name = "GPL-3.0-or-later")
    ),
    modifiers(&SecurityAddon),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
    tags(
        (name = "auth", description = "Session login / logout"),
        (name = "users", description = "Authenticated admin identity"),
        (name = "setup", description = "First-run setup wizard"),
        (name = "info", description = "Unauthenticated daemon info"),
        (name = "health", description = "Unauthenticated liveness/readiness probe"),
        (name = "devices", description = "Device discovery and routing"),
        (name = "network", description = "LAN interface status and gateway discovery"),
        (name = "network-zones", description = "Network Zones: device policy buckets"),
        (name = "zone-exceptions", description = "Cross-zone exceptions: admin-granted allowances between isolated zones"),
        (name = "rule-requests", description = "Device-initiated firewall rule requests and admin decisions"),
        (name = "tunnels", description = "WireGuard tunnel lifecycle"),
        (name = "inbound-wg", description = "Inbound WireGuard remote-access server and peers"),
        (name = "providers", description = "VPN provider integration"),
        (name = "routing", description = "Domain routing profiles: per-domain egress selection over tunnels/providers"),
        (name = "dhcp", description = "DHCP server configuration, leases, and reservations"),
        (name = "dns", description = "DNS resolver, ad-blocking, filters"),
        (name = "ddns", description = "Dynamic DNS: bridge / BYOD-Cloudflare registration"),
        (name = "tls", description = "Daemon-owned TLS certificate provisioning"),
        (name = "system", description = "Runtime status and logs"),
        (name = "jobs", description = "Background job status"),
        (name = "stats", description = "Generic pre-aggregating time-series and top-N stats"),
        (name = "update", description = "Auto-update and rollback"),
        (name = "push", description = "Web Push subscriptions and VAPID key"),
        (name = "backup", description = "Encrypted configuration export / import and snapshots"),
    )
)]
pub struct ApiDoc;

/// Declares the two authentication mechanisms the document-level security
/// default references by name:
///
/// - `session_cookie` — the `wardnet_session` cookie issued by `POST /api/auth/login`.
/// - `bearer_auth` — an opaque API key supplied via `Authorization: Bearer <key>`.
///
/// Both are registered as reusable components; [`ApiDoc`]'s `security(...)`
/// block applies them to every operation that doesn't override it.
struct SecurityAddon;

/// Wardnet logo lockup (mark + WARDNET wordmark, dark-surface variant) served
/// from `/api/docs/logo.svg` and injected at the top of Scalar's sidebar.
/// Vendored from the design system's `@wardnet/brand` package
/// (`assets/dist/wardnet-logo-dark.svg`) — the same lockup the admin web UI
/// renders via `<Logo variant="dark" />`. Current pin: `@wardnet/brand@0.1.0`;
/// refresh by re-copying that file from the published package (`npm pack
/// @wardnet/brand` → `package/assets/dist/wardnet-logo-dark.svg`) when the
/// brand assets change.
pub const LOGO_SVG: &[u8] = include_bytes!("../assets/wardnet-logo-dark.svg");

/// Vendored `@scalar/api-reference` standalone bundle, served from
/// `/api/docs/scalar.js`. Embedding the bundle (instead of loading from a
/// CDN) keeps the admin docs page working on an air-gapped Pi and removes
/// a supply-chain surface — a compromised `cdn.jsdelivr.net` would
/// otherwise execute arbitrary JS inside the admin session. Pinned to the
/// upstream version listed below; refresh with:
///
/// ```sh
/// curl -sL "https://cdn.jsdelivr.net/npm/@scalar/api-reference@<version>/dist/browser/standalone.js" \
///   -o source/daemon/crates/wardnetd-api/assets/vendor/scalar-api-reference.js
/// ```
///
/// Current pin: `1.52.5`.
///
/// Lives under `assets/vendor/` because it is third-party code we ship
/// verbatim and never patch. Linters skip `vendor/` by convention (bulwark's
/// bundled `ESLint` config ignores `**/vendor/**`) — without that, this one
/// minified bundle produces ~3000 findings that are neither ours to fix nor
/// meaningful, drowning out real findings in code we actually write.
pub const SCALAR_JS: &[u8] = include_bytes!("../assets/vendor/scalar-api-reference.js");

/// HTML for the `/api/docs` Scalar UI.
///
/// The `@scalar/api-reference` bundle is served from `/api/docs/scalar.js`
/// (the [`SCALAR_JS`] const, vendored into the daemon binary) rather than
/// loaded from a CDN, so the admin docs page works air-gapped and doesn't
/// rely on an external origin whose compromise would run JS inside the
/// admin session. The `<style>` block retargets Scalar's built-in
/// `--scalar-sidebar-*` CSS variables to Wardnet's palette so the sidebar
/// matches the admin app; all other Scalar surfaces use its default theme.
///
/// The API spec is loaded from `/api/openapi.json` at runtime — all three
/// routes (`/api/docs`, `/api/docs/scalar.js`, `/api/openapi.json`) are
/// admin-gated on the server side, so an unauthenticated visitor sees a
/// 401 on any of them.
pub const SCALAR_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Wardnet API</title>
  <style>
    /* Wardnet design-system tokens (@wardnet/styles — packages/styles/styles.css).
       These are the exact brand values, not approximations. The sidebar uses
       the DS "chrome on dark surfaces" palette (the Ink sidebar), so the
       values below are the light-theme --side-* tokens plus the Emerald
       accent. Kept in lockstep with wardnet-cloud's api-docs-site.
       Current pin: @wardnet/styles@0.1.0; refresh by re-copying the --side-*
       values from that package's styles.css when the tokens change. */
    :root {
      --wn-side-bg: #11152b; /* --side-bg (Ink) */
      --wn-side-line: #2a3155; /* --side-line */
      --wn-side-ink: #c9cce0; /* --side-ink */
      --wn-side-ink-2: #7e859b; /* --side-ink-2 (Mist, dark) */
      --wn-side-ink-active: #ffffff; /* --side-ink-active */
      --wn-side-active-bg: #1b2140; /* --side-active-bg (Ink 2) */
      --wn-accent: #12b981; /* --accent (Wardnet Emerald) */
    }
    /* Dark-theme --side-* set from the same styles.css ([data-theme="dark"]
       block; the omitted tokens keep their light-theme values there too).
       Scalar stamps .dark-mode on its app root from the system preference,
       so scoping the overrides there keeps the docs sidebar matched to the
       admin app's sidebar in both themes. */
    .dark-mode {
      --wn-side-bg: #080b16;
      --wn-side-line: #1c2444;
      --wn-side-ink: #b6bbcf;
    }
    /* Modern Scalar attaches the sidebar CSS vars to its `.dark-mode`
       / `.light-mode` scope on the app root, not to a nested `.sidebar`
       container. Targeting the same scope lets us override at equal
       specificity without needing `!important`. */
    .dark-mode,
    .light-mode {
      --scalar-sidebar-background-1: var(--wn-side-bg);
      --scalar-sidebar-color-1: var(--wn-side-ink);
      --scalar-sidebar-color-2: var(--wn-side-ink-2);
      --scalar-sidebar-color-active: var(--wn-accent);
      --scalar-sidebar-item-hover-background: var(--wn-side-active-bg);
      --scalar-sidebar-item-hover-color: var(--wn-side-ink-active);
      --scalar-sidebar-item-active-background: var(--wn-side-active-bg);
      --scalar-sidebar-border-color: var(--wn-side-line);
      --scalar-sidebar-search-background: var(--wn-side-active-bg);
      --scalar-sidebar-search-border-color: var(--wn-side-line);
      --scalar-sidebar-search-color: var(--wn-side-ink);
    }
    /* Brand block prepended into Scalar's sidebar by the script below.
       Scalar doesn't ship a top-of-sidebar logo config yet
       (github.com/scalar/scalar/discussions/914); DOM injection is the
       documented workaround. Mirrors wardnet-cloud's api-docs-site nav
       (logo lockup + subtitle) minus its service/version pickers, which
       don't apply to the single self-hosted daemon spec. */
    .wardnet-nav {
      display: flex;
      flex-direction: column;
      gap: 0.85rem;
      padding: 1rem 0.75rem 0.875rem;
      border-bottom: 1px solid var(--wn-side-line);
    }
    .wardnet-logo {
      width: 132px;
      height: auto;
      display: block;
    }
    .wardnet-subtitle {
      font-size: 0.72rem;
      font-weight: 600;
      letter-spacing: 0.06em;
      text-transform: uppercase;
      color: var(--wn-side-ink-2);
      margin-top: -0.2rem;
    }
  </style>
</head>
<body>
  <div id="wardnet-api-docs"></div>
  <script src="/api/docs/scalar.js"></script>
  <script>
    // Scalar 2.x programmatic API — gives us access to nested config keys
    // (`agent`, `showDeveloperTools`, `sources`) that the `data-configuration`
    // attribute can't round-trip cleanly. The Ask-AI composer is disabled
    // because Wardnet is a privacy tool and we don't want to ship an LLM
    // widget in our own docs. `/favicon-32.png` is the same file the admin
    // SPA uses, served by the daemon's rust-embed static handler. The
    // standalone bundle exposes the factory under `window.Scalar`.
    Scalar.createApiReference('#wardnet-api-docs', {
      url: '/api/openapi.json',
      favicon: '/favicon-32.png',
      agent: { disabled: true },
      mcp: { disabled: true },
      showDeveloperTools: 'never',
      hideDarkModeToggle: true,
      hideClientButton: true,
    });

    // Inject the Wardnet brand block at the top of Scalar's sidebar. The
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
        if (!sidebar || sidebar.querySelector('.wardnet-nav')) return;
        const nav = document.createElement('div');
        nav.className = 'wardnet-nav';
        nav.innerHTML =
          '<img class="wardnet-logo" src="/api/docs/logo.svg" alt="Wardnet" />' +
          '<div class="wardnet-subtitle">API Reference</div>';
        sidebar.prepend(nav);
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
