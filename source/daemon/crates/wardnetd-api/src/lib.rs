pub mod api;
pub mod openapi;
pub mod state;
pub mod web;

use utoipa::OpenApi;

/// Returns the generated `OpenAPI` document for the Wardnet daemon.
///
/// Exposed so external tools (e.g. the `dump_openapi` binary in Phase 2) can
/// produce a static spec file without booting the full HTTP stack.
///
/// Note: this returns only the schemas declared statically on [`openapi::ApiDoc`]
/// — tags, info metadata, and security schemes. Handler paths are merged into
/// the document at runtime by [`api::router`] via
/// `utoipa_axum::router::OpenApiRouter::split_for_parts`, so callers that need
/// the fully-populated spec (paths included) should go through the router.
#[must_use]
pub fn api_doc() -> utoipa::openapi::OpenApi {
    openapi::ApiDoc::openapi()
}

#[cfg(test)]
mod tests;
