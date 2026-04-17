use std::fmt::Write as _;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use wardnet_common::vpn_provider::{
    CountryInfo, ProviderAuthMethod, ProviderCredentials, ProviderInfo, ServerFilter, ServerInfo,
};

use crate::vpn::provider::VpnProvider;

/// Abstraction over `NordVPN`'s HTTP API for testability.
///
/// Real implementation calls `api.nordvpn.com`; tests inject a mock.
#[async_trait]
pub trait NordVpnApi: Send + Sync {
    /// Validate credentials against `NordVPN`'s API.
    async fn validate_credentials(&self, credentials: &ProviderCredentials)
    -> anyhow::Result<bool>;

    /// Fetch the list of available countries with their numeric IDs.
    async fn list_countries(&self) -> anyhow::Result<Vec<NordCountryInfo>>;

    /// Fetch recommended servers, optionally filtered by country.
    async fn list_servers(&self, filter: &NordServerFilter) -> anyhow::Result<Vec<NordServer>>;

    /// Fetch a single server by its hostname (e.g. `pt131.nordvpn.com`).
    /// Used for dedicated IP servers not in recommendations.
    async fn get_server_by_hostname(&self, hostname: &str) -> anyhow::Result<NordServer>;

    /// Get a `WireGuard` private key for the authenticated user.
    async fn get_wireguard_private_key(
        &self,
        credentials: &ProviderCredentials,
    ) -> anyhow::Result<String>;
}

/// Country entry from the `NordVPN` countries endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct NordCountryInfo {
    /// Numeric country ID used by the `NordVPN` API.
    pub id: u64,
    /// Human-readable country name.
    pub name: String,
    /// ISO 3166-1 alpha-2 country code.
    pub code: String,
}

/// Filter for `NordVPN` server listing.
#[derive(Debug, Clone)]
pub struct NordServerFilter {
    /// Numeric `NordVPN` country ID to filter by.
    pub country_id: Option<u64>,
    /// Maximum number of servers to return.
    pub limit: usize,
}

/// `NordVPN` server from the recommendations API.
#[derive(Debug, Clone, Deserialize)]
pub struct NordServer {
    /// Provider-internal server ID.
    pub id: u64,
    /// Human-readable server name.
    pub name: String,
    /// Server hostname (e.g. `us1234.nordvpn.com`).
    pub hostname: String,
    /// Current load percentage (0-100).
    pub load: u8,
    /// Server IP address.
    pub station: String,
    /// Geographic locations for this server.
    pub locations: Vec<NordLocation>,
    /// Supported VPN technologies.
    pub technologies: Vec<NordTechnology>,
}

/// Geographic location associated with a `NordVPN` server.
#[derive(Debug, Clone, Deserialize)]
pub struct NordLocation {
    /// Country information.
    pub country: NordCountry,
}

/// Country metadata from the `NordVPN` API.
#[derive(Debug, Clone, Deserialize)]
pub struct NordCountry {
    /// Numeric country ID.
    pub id: u64,
    /// ISO 3166-1 alpha-2 country code.
    pub code: String,
    /// City metadata nested inside the country.
    #[serde(default)]
    pub city: Option<NordCity>,
}

/// City metadata nested inside a `NordVPN` country.
#[derive(Debug, Clone, Deserialize)]
pub struct NordCity {
    /// Human-readable city name.
    pub name: String,
}

/// VPN technology supported by a `NordVPN` server.
#[derive(Debug, Clone, Deserialize)]
pub struct NordTechnology {
    /// Technology ID.
    pub id: u64,
    /// Technology identifier string (e.g. `wireguard_udp`).
    pub identifier: String,
    /// Key-value metadata (e.g. public keys).
    #[serde(default)]
    pub metadata: Vec<NordMetadata>,
}

/// A key-value metadata entry on a `NordVPN` technology.
#[derive(Debug, Clone, Deserialize)]
pub struct NordMetadata {
    /// Metadata field name.
    pub name: String,
    /// Metadata field value.
    pub value: String,
}

/// Response from the `NordVPN` credentials endpoint containing a `WireGuard` private key.
#[derive(Deserialize)]
struct NordCredentialsResponse {
    nordlynx_private_key: String,
}

/// HTTP client for `NordVPN`'s REST API using async reqwest.
pub struct HttpNordVpnApi {
    /// Shared HTTP client for connection pooling.
    client: reqwest::Client,
    /// Base URL for the `NordVPN` API (overridable for tests).
    base_url: String,
}

impl Default for HttpNordVpnApi {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpNordVpnApi {
    /// Create a client pointing at the production `NordVPN` API.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.nordvpn.com".to_string(),
        }
    }

    /// Create with a custom base URL (for integration testing).
    #[must_use]
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
        }
    }
}

#[async_trait]
impl NordVpnApi for HttpNordVpnApi {
    async fn validate_credentials(
        &self,
        credentials: &ProviderCredentials,
    ) -> anyhow::Result<bool> {
        let response = match credentials {
            ProviderCredentials::Token { token } => {
                self.client
                    .get(format!("{}/v1/users/services", self.base_url))
                    .basic_auth("token", Some(token))
                    .send()
                    .await?
            }
            ProviderCredentials::Credentials { .. } => {
                return Err(anyhow::anyhow!(
                    "NordVPN no longer supports service credentials; use a token instead"
                ));
            }
        };

        match response.status().as_u16() {
            200 => Ok(true),
            400 | 401 | 403 => Ok(false),
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(anyhow::anyhow!(
                    "NordVPN credential validation failed with status {status}: {body}"
                ))
            }
        }
    }

    async fn list_countries(&self) -> anyhow::Result<Vec<NordCountryInfo>> {
        let url = format!("{}/v1/servers/countries", self.base_url);
        let countries: Vec<NordCountryInfo> = self.client.get(&url).send().await?.json().await?;
        Ok(countries)
    }

    async fn get_server_by_hostname(&self, hostname: &str) -> anyhow::Result<NordServer> {
        let url = format!(
            "{}/v1/servers?filters[hostname]={hostname}&limit=1",
            self.base_url
        );
        let servers: Vec<NordServer> = self.client.get(&url).send().await?.json().await?;
        servers
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("server not found: {hostname}"))
    }

    async fn list_servers(&self, filter: &NordServerFilter) -> anyhow::Result<Vec<NordServer>> {
        let mut url = format!(
            "{}/v1/servers/recommendations?filters[servers_technologies][identifier]=wireguard_udp&limit={}",
            self.base_url, filter.limit
        );

        if let Some(id) = filter.country_id {
            let _ = write!(url, "&filters[country_id]={id}");
        }

        let servers: Vec<NordServer> = self.client.get(&url).send().await?.json().await?;
        Ok(servers)
    }

    async fn get_wireguard_private_key(
        &self,
        credentials: &ProviderCredentials,
    ) -> anyhow::Result<String> {
        match credentials {
            ProviderCredentials::Token { token } => {
                // NordVPN uses HTTP Basic auth with "token" as username and the
                // access token as password (equivalent to `curl -u token:TOKEN`).
                let resp: NordCredentialsResponse = self
                    .client
                    .get(format!("{}/v1/users/services/credentials", self.base_url))
                    .basic_auth("token", Some(token))
                    .send()
                    .await?
                    .json()
                    .await?;
                Ok(resp.nordlynx_private_key)
            }
            ProviderCredentials::Credentials { .. } => Err(anyhow::anyhow!(
                "NordVPN no longer supports service credentials; use a token instead"
            )),
        }
    }
}

/// `NordVPN` provider implementation.
///
/// Translates VPN provider trait operations into `NordVPN`-specific API calls
/// via an injected `NordVpnApi` abstraction.
pub struct NordVpnProvider {
    /// The API client used for all `NordVPN` HTTP operations.
    api: Arc<dyn NordVpnApi>,
}

impl NordVpnProvider {
    /// Create a new `NordVPN` provider backed by the given API client.
    #[must_use]
    pub fn new(api: Arc<dyn NordVpnApi>) -> Self {
        Self { api }
    }

    /// Convert a `NordServer` into a [`ServerInfo`].
    fn to_server_info(s: &NordServer) -> ServerInfo {
        ServerInfo {
            id: s.id.to_string(),
            name: s.name.clone(),
            country_code: s
                .locations
                .first()
                .map(|l| l.country.code.to_uppercase())
                .unwrap_or_default(),
            city: s
                .locations
                .first()
                .and_then(|l| l.country.city.as_ref())
                .map(|c| c.name.clone()),
            hostname: s.hostname.clone(),
            load: s.load,
        }
    }

    /// Extract the `WireGuard` public key from a `NordServer`'s technology metadata.
    pub(crate) fn extract_wg_public_key(server: &NordServer) -> anyhow::Result<String> {
        let wg_tech = server
            .technologies
            .iter()
            .find(|t| t.identifier == "wireguard_udp")
            .ok_or_else(|| {
                anyhow::anyhow!("server {} does not support WireGuard", server.hostname)
            })?;

        let public_key = wg_tech
            .metadata
            .iter()
            .find(|m| m.name == "public_key")
            .ok_or_else(|| {
                anyhow::anyhow!("server {} has no WireGuard public key", server.hostname)
            })?;

        Ok(public_key.value.clone())
    }
}

#[async_trait]
impl VpnProvider for NordVpnProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "nordvpn".to_string(),
            name: "NordVPN".to_string(),
            auth_methods: vec![ProviderAuthMethod::Token],
            icon_url: Some("https://nordvpn.com/favicon.ico".to_string()),
            website_url: Some("https://nordvpn.com".to_string()),
            credentials_hint: Some(
                "Get your access token from https://my.nordaccount.com/dashboard/nordvpn/manual-configuration/"
                    .to_string(),
            ),
        }
    }

    async fn validate_credentials(
        &self,
        credentials: &ProviderCredentials,
    ) -> anyhow::Result<bool> {
        self.api.validate_credentials(credentials).await
    }

    async fn list_countries(
        &self,
        _credentials: &ProviderCredentials,
    ) -> anyhow::Result<Vec<CountryInfo>> {
        let countries = self.api.list_countries().await?;
        Ok(countries
            .into_iter()
            .map(|c| CountryInfo {
                code: c.code,
                name: c.name,
            })
            .collect())
    }

    async fn list_servers(
        &self,
        _credentials: &ProviderCredentials,
        filter: &ServerFilter,
    ) -> anyhow::Result<Vec<ServerInfo>> {
        // Resolve ISO country code to the numeric ID the NordVPN API requires.
        let country_id = if let Some(ref code) = filter.country {
            let countries = self.api.list_countries().await?;
            countries
                .iter()
                .find(|c| c.code.eq_ignore_ascii_case(code))
                .map(|c| c.id)
        } else {
            None
        };

        let nord_filter = NordServerFilter {
            country_id,
            limit: 20,
        };

        let servers = self.api.list_servers(&nord_filter).await?;

        let results: Vec<ServerInfo> = servers
            .iter()
            .filter(|s| filter.max_load.is_none_or(|max| s.load <= max))
            .map(Self::to_server_info)
            .collect();

        Ok(results)
    }

    async fn resolve_server(
        &self,
        _credentials: &ProviderCredentials,
        hostname: &str,
    ) -> anyhow::Result<Option<ServerInfo>> {
        let full_hostname = if hostname.contains('.') {
            hostname.to_string()
        } else {
            format!("{hostname}.nordvpn.com")
        };

        // Extract country code from hostname (e.g. "pt131.nordvpn.com" → "PT").
        let country_code = full_hostname
            .split('.')
            .next()
            .and_then(|h| {
                let letters: String = h.chars().take_while(char::is_ascii_alphabetic).collect();
                if letters.len() == 2 {
                    Some(letters.to_uppercase())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        // Use the recommendations endpoint to get a server with the WireGuard
        // public key. The hostname filter API (`filters[hostname]`) is unreliable
        // and returns wrong servers for dedicated IPs and some regions.
        let country_id = if country_code.is_empty() {
            None
        } else {
            let countries = self.api.list_countries().await?;
            countries
                .iter()
                .find(|c| c.code.eq_ignore_ascii_case(&country_code))
                .map(|c| c.id)
        };

        let filter = NordServerFilter {
            country_id,
            limit: 1,
        };
        let servers = self.api.list_servers(&filter).await?;
        let reference_server = servers.first().ok_or_else(|| {
            anyhow::anyhow!("no WireGuard servers found for country '{country_code}'")
        })?;

        // Build ServerInfo with the user's requested hostname, but take the
        // country/city metadata from the reference server.
        Ok(Some(ServerInfo {
            id: String::new(),
            name: full_hostname.clone(),
            country_code: reference_server
                .locations
                .first()
                .map(|l| l.country.code.to_uppercase())
                .unwrap_or(country_code),
            city: reference_server
                .locations
                .first()
                .and_then(|l| l.country.city.as_ref())
                .map(|c| c.name.clone()),
            hostname: full_hostname,
            load: reference_server.load,
        }))
    }

    async fn generate_config(
        &self,
        credentials: &ProviderCredentials,
        server: &ServerInfo,
    ) -> anyhow::Result<String> {
        let private_key = self.api.get_wireguard_private_key(credentials).await?;

        // Get the WireGuard public key from a recommended server in the same
        // country. All servers in a region share the same WireGuard key group.
        // We avoid the hostname filter API which is unreliable for dedicated IPs.
        let country_id = if server.country_code.is_empty() {
            None
        } else {
            let countries = self.api.list_countries().await?;
            countries
                .iter()
                .find(|c| c.code.eq_ignore_ascii_case(&server.country_code))
                .map(|c| c.id)
        };

        let filter = NordServerFilter {
            country_id,
            limit: 1,
        };
        let servers = self.api.list_servers(&filter).await?;
        let reference_server = servers.first().ok_or_else(|| {
            anyhow::anyhow!(
                "no WireGuard servers found for country '{}'",
                server.country_code
            )
        })?;
        let public_key = Self::extract_wg_public_key(reference_server)?;

        Ok(format!(
            "[Interface]\n\
             PrivateKey = {private_key}\n\
             Address = 10.5.0.2/32\n\
             DNS = 103.86.96.100, 103.86.99.100\n\
             \n\
             [Peer]\n\
             PublicKey = {public_key}\n\
             Endpoint = {}:51820\n\
             AllowedIPs = 0.0.0.0/0, ::/0\n\
             PersistentKeepalive = 25\n",
            server.hostname
        ))
    }
}
