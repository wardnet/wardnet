use std::sync::Arc;

use async_trait::async_trait;
use wardnet_types::api::{
    CreateTunnelRequest, ListProvidersResponse, ListServersRequest, ListServersResponse,
    SetupProviderRequest, SetupProviderResponse, ValidateCredentialsRequest,
    ValidateCredentialsResponse,
};
use wardnet_types::vpn_provider::ServerFilter;

use crate::error::AppError;
use crate::service::TunnelService;
use crate::vpn_provider::VpnProvider;
use crate::vpn_provider_registry::VpnProviderRegistry;

/// VPN provider operations: listing, credential validation, server browsing,
/// and one-click tunnel setup.
#[async_trait]
pub trait ProviderService: Send + Sync {
    /// List all registered VPN providers.
    async fn list_providers(&self) -> Result<ListProvidersResponse, AppError>;

    /// Validate credentials against a specific provider.
    async fn validate_credentials(
        &self,
        provider_id: &str,
        request: ValidateCredentialsRequest,
    ) -> Result<ValidateCredentialsResponse, AppError>;

    /// List available servers from a provider, with optional filtering.
    async fn list_servers(
        &self,
        provider_id: &str,
        request: ListServersRequest,
    ) -> Result<ListServersResponse, AppError>;

    /// Set up a new tunnel through a provider: validate credentials, pick a
    /// server, generate config, and import the tunnel.
    async fn setup_tunnel(
        &self,
        provider_id: &str,
        request: SetupProviderRequest,
    ) -> Result<SetupProviderResponse, AppError>;
}

/// Default implementation of [`ProviderService`].
pub struct ProviderServiceImpl {
    registry: Arc<VpnProviderRegistry>,
    tunnel_service: Arc<dyn TunnelService>,
}

impl ProviderServiceImpl {
    /// Create a new provider service with the given registry and tunnel service.
    pub fn new(
        registry: Arc<VpnProviderRegistry>,
        tunnel_service: Arc<dyn TunnelService>,
    ) -> Self {
        Self {
            registry,
            tunnel_service,
        }
    }

    /// Look up a provider by ID, returning `AppError::NotFound` when absent.
    fn require_provider(&self, id: &str) -> Result<&Arc<dyn VpnProvider>, AppError> {
        self.registry
            .get(id)
            .ok_or_else(|| AppError::NotFound(format!("provider '{id}' not found")))
    }
}

#[async_trait]
impl ProviderService for ProviderServiceImpl {
    async fn list_providers(&self) -> Result<ListProvidersResponse, AppError> {
        Ok(ListProvidersResponse {
            providers: self.registry.list(),
        })
    }

    async fn validate_credentials(
        &self,
        provider_id: &str,
        request: ValidateCredentialsRequest,
    ) -> Result<ValidateCredentialsResponse, AppError> {
        let provider = self.require_provider(provider_id)?;

        let valid = provider
            .validate_credentials(&request.credentials)
            .await
            .map_err(AppError::Internal)?;

        let message = if valid {
            "credentials are valid".to_owned()
        } else {
            "credentials are invalid".to_owned()
        };

        Ok(ValidateCredentialsResponse { valid, message })
    }

    async fn list_servers(
        &self,
        provider_id: &str,
        request: ListServersRequest,
    ) -> Result<ListServersResponse, AppError> {
        let provider = self.require_provider(provider_id)?;

        let servers = provider
            .list_servers(&request.credentials, &request.filter)
            .await
            .map_err(AppError::Internal)?;

        Ok(ListServersResponse { servers })
    }

    async fn setup_tunnel(
        &self,
        provider_id: &str,
        request: SetupProviderRequest,
    ) -> Result<SetupProviderResponse, AppError> {
        let provider = self.require_provider(provider_id)?;
        let info = provider.info();

        // 1. Validate credentials.
        let valid = provider
            .validate_credentials(&request.credentials)
            .await
            .map_err(AppError::Internal)?;
        if !valid {
            return Err(AppError::Unauthorized(
                "invalid provider credentials".to_owned(),
            ));
        }

        // 2. Fetch servers filtered by country.
        let filter = ServerFilter {
            country: Some(request.country.clone()),
            max_load: None,
        };
        let servers = provider
            .list_servers(&request.credentials, &filter)
            .await
            .map_err(AppError::Internal)?;

        if servers.is_empty() {
            return Err(AppError::NotFound(format!(
                "no servers found for country '{}'",
                request.country
            )));
        }

        // 3. Select server: specific ID or lowest load.
        let server = if let Some(ref server_id) = request.server_id {
            servers
                .iter()
                .find(|s| s.id == *server_id)
                .cloned()
                .ok_or_else(|| {
                    AppError::NotFound(format!("server '{server_id}' not found"))
                })?
        } else {
            servers
                .iter()
                .min_by_key(|s| s.load)
                .cloned()
                .expect("servers is non-empty")
        };

        // 4. Generate WireGuard configuration.
        let config = provider
            .generate_config(&request.credentials, &server)
            .await
            .map_err(AppError::Internal)?;

        // 5. Build label.
        let label = request
            .label
            .unwrap_or_else(|| format!("{} - {}", info.name, server.name));

        // 6. Import the tunnel via TunnelService.
        let tunnel_request = CreateTunnelRequest {
            label,
            country_code: server.country_code.clone(),
            provider: Some(info.id.clone()),
            config,
        };
        let tunnel_response = self.tunnel_service.import_tunnel(tunnel_request).await?;

        Ok(SetupProviderResponse {
            tunnel: tunnel_response.tunnel,
            server,
            message: format!(
                "tunnel created via {} ({})",
                info.name, info.id
            ),
        })
    }
}
