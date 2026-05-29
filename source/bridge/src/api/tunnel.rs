use std::sync::Arc;

use axum::extract::{Path, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::AuthenticatedInstall;
use crate::error::ApiError;
use crate::state::AppState;

/// Register the tunnel WebSocket route.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(tunnel_connect))
}

#[utoipa::path(
    get,
    path = "/v1/installs/{id}/tunnel",
    tag = "tunnel",
    description = "Upgrade to a reverse-tunnel WebSocket connection. \
                   \n\n\
                   The Pi dials this endpoint and keeps the connection open. \
                   The bridge uses the WebSocket to forward inbound TLS connections \
                   arriving at the SNI demuxer — the Pi decrypts them locally so the \
                   private key never leaves the device. \
                   \n\n\
                   **Frame protocol** (all frames are binary WebSocket messages): \
                   \n\
                   - `CONNECT (0x01)`: bridge→pi, 7 bytes: `[type, conn_id:u32be, dest_port:u16be]` \
                   - `READY   (0x02)`: pi→bridge, 5 bytes: `[type, conn_id:u32be]` \
                   - `DATA    (0x03)`: both directions, 5+N bytes: `[type, conn_id:u32be, payload…]` \
                   - `CLOSE   (0x04)`: both directions, 5 bytes: `[type, conn_id:u32be]` \
                   - `PING    (0x05)`: pi→bridge, 5 bytes (conn_id=0) \
                   - `PONG    (0x06)`: bridge→pi, 5 bytes (conn_id=0)",
    params(
        ("id" = String, Path, description = "Installation UUID"),
    ),
    responses(
        (status = 101, description = "Switching protocols — tunnel established"),
        (status = 401, description = "Authentication required or invalid"),
        (status = 403, description = "Bearer token does not match install ID"),
    ),
)]
// Auth note: this endpoint sits under /v1/installs/ so the auth_layer middleware
// enforces the full Ed25519 signed-request flow (bearer token lookup, timestamp
// window, signature verification over "GET\n{path}\n{ts}\nhex-sha256("")", and
// replay-cache check) before the handler runs. AuthenticatedInstall extracts the
// pre-verified Install from request extensions.
pub async fn tunnel_connect(
    State(state): State<AppState>,
    Path(id): Path<String>,
    AuthenticatedInstall(install): AuthenticatedInstall,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ApiError> {
    if install.id != id {
        return Err(ApiError::Forbidden(
            "bearer token does not match the requested install ID".to_string(),
        ));
    }

    let registry: Arc<crate::tunnel::TunnelRegistry> = state.tunnel_registry();
    let name = install.name.clone();
    let install_id = install.id.clone();

    tracing::info!(install_id = %install_id, name = %name, "tunnel connection established");

    Ok(
        ws.on_upgrade(move |socket| {
            crate::tunnel::handler::run(socket, install_id, name, registry)
        }),
    )
}
