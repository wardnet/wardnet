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
