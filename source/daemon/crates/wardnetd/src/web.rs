use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;

/// Embedded web UI assets compiled into the binary.
///
/// In debug mode, reads files from the filesystem (no rebuild needed
/// for UI changes). In release mode, all files are baked into the binary.
#[derive(Embed)]
#[folder = "../../../web-ui/dist"]
struct Assets;

/// Fallback handler that serves embedded static files.
///
/// Serves the requested file if it exists, otherwise falls back to
/// `index.html` for client-side SPA routing.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try the exact path first.
    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_ref())],
            file.data,
        )
            .into_response();
    }

    // Fall back to index.html for SPA routing.
    match Assets::get("index.html") {
        Some(file) => Html(file.data).into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
