use std::sync::Arc;

use async_trait::async_trait;
use wardnet_common::api::{
    CreateTunnelRequest, ListCountriesResponse, ListProvidersResponse, ListServersRequest,
    ListServersResponse, SetupProviderRequest, SetupProviderResponse, ValidateCredentialsRequest,
    ValidateCredentialsResponse,
};
use wardnet_common::vpn_provider::ServerFilter;

use crate::TunnelService;
use crate::auth_context;
use crate::error::AppError;
use crate::vpn::provider::VpnProvider;
use crate::vpn::registry::VpnProviderRegistry;

/// VPN provider operations: listing, credential validation, server browsing,
/// and one-click tunnel setup.
#[async_trait]
pub trait VpnProviderService: Send + Sync {
    /// List all registered VPN providers.
    async fn list_providers(&self) -> Result<ListProvidersResponse, AppError>;

    /// Validate credentials against a specific provider.
    async fn validate_credentials(
        &self,
        provider_id: &str,
        request: ValidateCredentialsRequest,
    ) -> Result<ValidateCredentialsResponse, AppError>;

    /// List countries where a provider has servers.
    async fn list_countries(&self, provider_id: &str) -> Result<ListCountriesResponse, AppError>;

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

/// Default implementation of [`VpnProviderService`].
pub struct VpnProviderServiceImpl {
    registry: Arc<VpnProviderRegistry>,
    tunnel_service: Arc<dyn TunnelService>,
}

impl VpnProviderServiceImpl {
    /// Create a new provider service with the given registry and tunnel service.
    pub fn new(registry: Arc<VpnProviderRegistry>, tunnel_service: Arc<dyn TunnelService>) -> Self {
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
impl VpnProviderService for VpnProviderServiceImpl {
    async fn list_providers(&self) -> Result<ListProvidersResponse, AppError> {
        auth_context::require_admin()?;

        Ok(ListProvidersResponse {
            providers: self.registry.list(),
        })
    }

    async fn validate_credentials(
        &self,
        provider_id: &str,
        request: ValidateCredentialsRequest,
    ) -> Result<ValidateCredentialsResponse, AppError> {
        auth_context::require_admin()?;

        let provider = self.require_provider(provider_id)?;

        let (valid, message) = match provider.validate_credentials(&request.credentials).await {
            Ok(true) => (true, "credentials are valid".to_owned()),
            Ok(false) => (false, "credentials are invalid".to_owned()),
            Err(e) => (false, e.to_string()),
        };

        Ok(ValidateCredentialsResponse { valid, message })
    }

    async fn list_countries(&self, provider_id: &str) -> Result<ListCountriesResponse, AppError> {
        auth_context::require_admin()?;

        let provider = self.require_provider(provider_id)?;

        // NordVPN's country list is public; we pass a dummy credential since
        // the trait signature requires it for providers that need auth.
        let dummy = wardnet_common::vpn_provider::ProviderCredentials::Token {
            token: String::new(),
        };

        let countries = provider
            .list_countries(&dummy)
            .await
            .map_err(AppError::Internal)?;

        Ok(ListCountriesResponse { countries })
    }

    async fn list_servers(
        &self,
        provider_id: &str,
        request: ListServersRequest,
    ) -> Result<ListServersResponse, AppError> {
        auth_context::require_admin()?;

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
        auth_context::require_admin()?;

        let provider = self.require_provider(provider_id)?;
        let info = provider.info();

        // 1. Validate credentials.
        let valid = provider
            .validate_credentials(&request.credentials)
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        if !valid {
            return Err(AppError::Unauthorized(
                "invalid provider credentials".to_owned(),
            ));
        }

        // 2. Resolve server: hostname path (dedicated IP) or list+select path.
        let server = if let Some(ref hostname) = request.hostname {
            // Dedicated IP / direct hostname path.
            provider
                .resolve_server(&request.credentials, hostname)
                .await
                .map_err(AppError::Internal)?
                .ok_or_else(|| {
                    AppError::BadRequest(format!(
                        "provider '{}' does not support hostname-based server resolution",
                        info.id
                    ))
                })?
        } else {
            // Standard flow: list servers filtered by country, then select.
            let filter = ServerFilter {
                country: request.country.clone(),
                max_load: None,
            };
            let servers = provider
                .list_servers(&request.credentials, &filter)
                .await
                .map_err(AppError::Internal)?;

            if servers.is_empty() {
                let country_label = request.country.as_deref().unwrap_or("any");
                return Err(AppError::NotFound(format!(
                    "no servers found for country '{country_label}'"
                )));
            }

            if let Some(ref server_id) = request.server_id {
                servers
                    .iter()
                    .find(|s| s.id == *server_id)
                    .cloned()
                    .ok_or_else(|| AppError::NotFound(format!("server '{server_id}' not found")))?
            } else {
                servers
                    .iter()
                    .min_by_key(|s| s.load)
                    .cloned()
                    .expect("servers is non-empty")
            }
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
            message: format!("tunnel created via {} ({})", info.name, info.id),
        })
    }
}
