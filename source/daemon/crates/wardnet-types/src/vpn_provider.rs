use serde::{Deserialize, Serialize};

/// Supported authentication methods for a VPN provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAuthMethod {
    /// Username + password (service credentials).
    Credentials,
    /// Opaque access token.
    Token,
}

/// Metadata about a registered VPN provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Unique machine identifier (e.g. "nordvpn").
    pub id: String,
    /// Human-readable display name (e.g. "NordVPN").
    pub name: String,
    /// Authentication methods this provider supports.
    pub auth_methods: Vec<ProviderAuthMethod>,
    /// URL to the provider's icon/logo for UI display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// URL to the provider's website.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
}

/// Credentials submitted by the admin for provider operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderCredentials {
    /// Username/password pair (service credentials).
    Credentials {
        /// Service username.
        username: String,
        /// Service password.
        password: String,
    },
    /// Access token.
    Token {
        /// The access token value.
        token: String,
    },
}

/// Filters for server listing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerFilter {
    /// ISO 3166-1 alpha-2 country code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// Maximum server load percentage (0-100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_load: Option<u8>,
}

/// Information about a single VPN server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Provider-specific server identifier.
    pub id: String,
    /// Human-readable server name (e.g. "Sweden #142").
    pub name: String,
    /// ISO 3166-1 alpha-2 country code.
    pub country_code: String,
    /// City name if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// Server hostname (e.g. "se142.nordvpn.com").
    pub hostname: String,
    /// Current load percentage (0-100).
    pub load: u8,
}
