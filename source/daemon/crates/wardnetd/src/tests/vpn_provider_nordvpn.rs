use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use wardnet_types::vpn_provider::{
    ProviderAuthMethod, ProviderCredentials, ServerFilter, ServerInfo,
};

use crate::vpn_provider::VpnProvider;
use crate::vpn_provider_nordvpn::{
    NordCity, NordCountry, NordCountryInfo, NordLocation, NordMetadata, NordServer,
    NordServerFilter, NordTechnology, NordVpnApi, NordVpnProvider,
};

/// Mock implementation of `NordVpnApi` for unit testing.
struct MockNordVpnApi {
    validate_result: Mutex<Result<bool, String>>,
    countries: Mutex<Vec<NordCountryInfo>>,
    servers: Mutex<Vec<NordServer>>,
    server_by_hostname: Mutex<Option<NordServer>>,
    private_key_result: Mutex<Result<String, String>>,
    /// Records the hostname passed to `get_server_by_hostname`.
    last_hostname_query: Mutex<Option<String>>,
}

impl MockNordVpnApi {
    fn new() -> Self {
        Self {
            validate_result: Mutex::new(Ok(true)),
            countries: Mutex::new(vec![
                NordCountryInfo {
                    id: 208,
                    name: "Sweden".to_string(),
                    code: "SE".to_string(),
                },
                NordCountryInfo {
                    id: 228,
                    name: "United States".to_string(),
                    code: "US".to_string(),
                },
                NordCountryInfo {
                    id: 81,
                    name: "Germany".to_string(),
                    code: "DE".to_string(),
                },
            ]),
            servers: Mutex::new(Vec::new()),
            server_by_hostname: Mutex::new(None),
            private_key_result: Mutex::new(Ok("test-private-key".to_string())),
            last_hostname_query: Mutex::new(None),
        }
    }
}

#[async_trait]
impl NordVpnApi for MockNordVpnApi {
    async fn validate_credentials(
        &self,
        _credentials: &ProviderCredentials,
    ) -> anyhow::Result<bool> {
        let result = self.validate_result.lock().await;
        match result.as_ref() {
            Ok(v) => Ok(*v),
            Err(e) => Err(anyhow::anyhow!("{e}")),
        }
    }

    async fn get_server_by_hostname(&self, hostname: &str) -> anyhow::Result<NordServer> {
        *self.last_hostname_query.lock().await = Some(hostname.to_string());
        let guard = self.server_by_hostname.lock().await;
        guard
            .clone()
            .ok_or_else(|| anyhow::anyhow!("server not found: {hostname}"))
    }

    async fn list_countries(&self) -> anyhow::Result<Vec<NordCountryInfo>> {
        let countries = self.countries.lock().await;
        Ok(countries.clone())
    }

    async fn list_servers(&self, _filter: &NordServerFilter) -> anyhow::Result<Vec<NordServer>> {
        let servers = self.servers.lock().await;
        Ok(servers.clone())
    }

    async fn get_wireguard_private_key(
        &self,
        _credentials: &ProviderCredentials,
    ) -> anyhow::Result<String> {
        let result = self.private_key_result.lock().await;
        match result.as_ref() {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(anyhow::anyhow!("{e}")),
        }
    }
}

/// Build a sample `NordServer` with `WireGuard` technology and public key metadata.
fn sample_server(hostname: &str, load: u8, country_code: &str) -> NordServer {
    sample_server_with_city(hostname, load, country_code, None)
}

/// Build a sample `NordServer` with `WireGuard` technology, public key metadata, and optional city.
fn sample_server_with_city(
    hostname: &str,
    load: u8,
    country_code: &str,
    city: Option<&str>,
) -> NordServer {
    NordServer {
        id: 1234,
        name: format!(
            "{country_code} #{}",
            hostname.split('.').next().unwrap_or("1")
        ),
        hostname: hostname.to_string(),
        load,
        station: "1.2.3.4".to_string(),
        locations: vec![NordLocation {
            country: NordCountry {
                id: 0,
                code: country_code.to_string(),
                city: city.map(|c| NordCity {
                    name: c.to_string(),
                }),
            },
        }],
        technologies: vec![NordTechnology {
            id: 35,
            identifier: "wireguard_udp".to_string(),
            metadata: vec![NordMetadata {
                name: "public_key".to_string(),
                value: "dGVzdC1wdWJsaWMta2V5".to_string(),
            }],
        }],
    }
}

/// Build a `NordServer` without `WireGuard` technology.
fn server_without_wg(hostname: &str) -> NordServer {
    NordServer {
        id: 5678,
        name: "NoWG Server".to_string(),
        hostname: hostname.to_string(),
        load: 10,
        station: "5.6.7.8".to_string(),
        locations: vec![NordLocation {
            country: NordCountry {
                id: 228,
                code: "US".to_string(),
                city: None,
            },
        }],
        technologies: vec![NordTechnology {
            id: 3,
            identifier: "openvpn_udp".to_string(),
            metadata: vec![],
        }],
    }
}

/// Build a `NordServer` with `WireGuard` technology but no public key metadata.
fn server_wg_no_key(hostname: &str) -> NordServer {
    NordServer {
        id: 9999,
        name: "WG No Key".to_string(),
        hostname: hostname.to_string(),
        load: 20,
        station: "9.8.7.6".to_string(),
        locations: vec![NordLocation {
            country: NordCountry {
                id: 81,
                code: "DE".to_string(),
                city: None,
            },
        }],
        technologies: vec![NordTechnology {
            id: 35,
            identifier: "wireguard_udp".to_string(),
            metadata: vec![],
        }],
    }
}

fn token_credentials() -> ProviderCredentials {
    ProviderCredentials::Token {
        token: "test-token".to_string(),
    }
}

#[tokio::test]
async fn info_returns_correct_metadata() {
    let mock = Arc::new(MockNordVpnApi::new());
    let provider = NordVpnProvider::new(mock);

    let info = provider.info();
    assert_eq!(info.id, "nordvpn");
    assert_eq!(info.name, "NordVPN");
    assert_eq!(info.auth_methods, vec![ProviderAuthMethod::Token]);
    assert_eq!(
        info.icon_url.as_deref(),
        Some("https://nordvpn.com/favicon.ico")
    );
    assert_eq!(info.website_url.as_deref(), Some("https://nordvpn.com"));
}

#[tokio::test]
async fn validate_credentials_delegates_to_api() {
    let mock = Arc::new(MockNordVpnApi::new());
    // Default validate_result is Ok(true)
    let provider = NordVpnProvider::new(mock);

    let result = provider.validate_credentials(&token_credentials()).await;
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[tokio::test]
async fn validate_credentials_returns_false() {
    let mock = MockNordVpnApi::new();
    *mock.validate_result.lock().await = Ok(false);
    let provider = NordVpnProvider::new(Arc::new(mock));

    let result = provider.validate_credentials(&token_credentials()).await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[tokio::test]
async fn validate_credentials_propagates_error() {
    let mock = MockNordVpnApi::new();
    *mock.validate_result.lock().await = Err("connection refused".to_string());
    let provider = NordVpnProvider::new(Arc::new(mock));

    let result = provider.validate_credentials(&token_credentials()).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("connection refused")
    );
}

#[tokio::test]
async fn list_servers_converts_nord_servers() {
    let mock = MockNordVpnApi::new();
    *mock.servers.lock().await = vec![
        sample_server_with_city("se142.nordvpn.com", 25, "SE", Some("Stockholm")),
        sample_server("us1001.nordvpn.com", 40, "US"),
    ];
    let provider = NordVpnProvider::new(Arc::new(mock));

    let filter = ServerFilter::default();
    let servers = provider
        .list_servers(&token_credentials(), &filter)
        .await
        .unwrap();

    assert_eq!(servers.len(), 2);
    assert_eq!(servers[0].hostname, "se142.nordvpn.com");
    assert_eq!(servers[0].country_code, "SE");
    assert_eq!(servers[0].load, 25);
    assert_eq!(servers[0].city.as_deref(), Some("Stockholm"));
    assert_eq!(servers[1].hostname, "us1001.nordvpn.com");
    assert_eq!(servers[1].country_code, "US");
    assert!(servers[1].city.is_none());
}

#[tokio::test]
async fn list_servers_filters_by_max_load() {
    let mock = MockNordVpnApi::new();
    *mock.servers.lock().await = vec![
        sample_server("low.nordvpn.com", 10, "SE"),
        sample_server("mid.nordvpn.com", 50, "SE"),
        sample_server("high.nordvpn.com", 90, "SE"),
    ];
    let provider = NordVpnProvider::new(Arc::new(mock));

    let filter = ServerFilter {
        country: None,
        max_load: Some(50),
    };
    let servers = provider
        .list_servers(&token_credentials(), &filter)
        .await
        .unwrap();

    assert_eq!(servers.len(), 2);
    assert!(servers.iter().all(|s| s.load <= 50));
}

#[tokio::test]
async fn list_servers_handles_empty() {
    let mock = Arc::new(MockNordVpnApi::new());
    // servers default to empty
    let provider = NordVpnProvider::new(mock);

    let filter = ServerFilter::default();
    let servers = provider
        .list_servers(&token_credentials(), &filter)
        .await
        .unwrap();

    assert!(servers.is_empty());
}

#[tokio::test]
async fn generate_config_produces_valid_wireguard() {
    let mock = MockNordVpnApi::new();
    *mock.server_by_hostname.lock().await = Some(sample_server("se142.nordvpn.com", 25, "SE"));
    *mock.private_key_result.lock().await = Ok("YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo=".to_string());
    let provider = NordVpnProvider::new(Arc::new(mock));

    let server_info = ServerInfo {
        id: "1234".to_string(),
        name: "SE #se142".to_string(),
        country_code: "SE".to_string(),
        city: None,
        hostname: "se142.nordvpn.com".to_string(),
        load: 25,
    };

    let config_str = provider
        .generate_config(&token_credentials(), &server_info)
        .await
        .unwrap();

    // Verify it parses as a valid WireGuard config
    let parsed = wardnet_types::wireguard_config::parse(&config_str).unwrap();
    assert_eq!(
        parsed.interface.private_key,
        "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo="
    );
    assert_eq!(parsed.interface.address, vec!["10.5.0.2/16"]);
    assert_eq!(parsed.interface.dns, vec!["103.86.96.100", "103.86.99.100"]);
    assert_eq!(parsed.peers.len(), 1);
    assert_eq!(parsed.peers[0].public_key, "dGVzdC1wdWJsaWMta2V5");
    assert_eq!(
        parsed.peers[0].endpoint.as_deref(),
        Some("se142.nordvpn.com:51820")
    );
    assert_eq!(parsed.peers[0].allowed_ips, vec!["0.0.0.0/0", "::/0"]);
    assert_eq!(parsed.peers[0].persistent_keepalive, Some(25));
}

#[tokio::test]
async fn generate_config_no_wireguard_tech() {
    let mock = MockNordVpnApi::new();
    *mock.server_by_hostname.lock().await = Some(server_without_wg("nowg.nordvpn.com"));
    let provider = NordVpnProvider::new(Arc::new(mock));

    let server_info = ServerInfo {
        id: "5678".to_string(),
        name: "NoWG Server".to_string(),
        country_code: "US".to_string(),
        city: None,
        hostname: "nowg.nordvpn.com".to_string(),
        load: 10,
    };

    let result = provider
        .generate_config(&token_credentials(), &server_info)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("does not support WireGuard"), "got: {err}");
}

#[tokio::test]
async fn generate_config_no_public_key() {
    let mock = MockNordVpnApi::new();
    *mock.server_by_hostname.lock().await = Some(server_wg_no_key("nokey.nordvpn.com"));
    let provider = NordVpnProvider::new(Arc::new(mock));

    let server_info = ServerInfo {
        id: "9999".to_string(),
        name: "WG No Key".to_string(),
        country_code: "DE".to_string(),
        city: None,
        hostname: "nokey.nordvpn.com".to_string(),
        load: 20,
    };

    let result = provider
        .generate_config(&token_credentials(), &server_info)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("no WireGuard public key"), "got: {err}");
}

#[tokio::test]
async fn generate_config_api_key_error() {
    let mock = MockNordVpnApi::new();
    *mock.server_by_hostname.lock().await = Some(sample_server("se142.nordvpn.com", 25, "SE"));
    *mock.private_key_result.lock().await = Err("API key retrieval failed".to_string());
    let provider = NordVpnProvider::new(Arc::new(mock));

    let server_info = ServerInfo {
        id: "1234".to_string(),
        name: "SE #se142".to_string(),
        country_code: "SE".to_string(),
        city: None,
        hostname: "se142.nordvpn.com".to_string(),
        load: 25,
    };

    let result = provider
        .generate_config(&token_credentials(), &server_info)
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("API key retrieval failed")
    );
}

#[tokio::test]
async fn generate_config_server_not_found() {
    let mock = MockNordVpnApi::new();
    // server_by_hostname is None by default, so get_server_by_hostname will error.
    let provider = NordVpnProvider::new(Arc::new(mock));

    let server_info = ServerInfo {
        id: "1234".to_string(),
        name: "Missing Server".to_string(),
        country_code: "SE".to_string(),
        city: None,
        hostname: "missing.nordvpn.com".to_string(),
        load: 25,
    };

    let result = provider
        .generate_config(&token_credentials(), &server_info)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not found"), "got: {err}");
}

#[tokio::test]
async fn extract_wg_public_key_success() {
    let server = sample_server("test.nordvpn.com", 10, "US");
    let key = NordVpnProvider::extract_wg_public_key(&server).unwrap();
    assert_eq!(key, "dGVzdC1wdWJsaWMta2V5");
}

#[tokio::test]
async fn extract_wg_public_key_missing_tech() {
    let server = server_without_wg("test.nordvpn.com");
    let result = NordVpnProvider::extract_wg_public_key(&server);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("does not support WireGuard")
    );
}

#[tokio::test]
async fn extract_wg_public_key_missing_metadata() {
    let server = server_wg_no_key("test.nordvpn.com");
    let result = NordVpnProvider::extract_wg_public_key(&server);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("no WireGuard public key")
    );
}

#[tokio::test]
async fn list_servers_resolves_country_code_to_id() {
    let mock = MockNordVpnApi::new();
    *mock.servers.lock().await = vec![sample_server_with_city(
        "se142.nordvpn.com",
        25,
        "SE",
        Some("Stockholm"),
    )];
    let provider = NordVpnProvider::new(Arc::new(mock));

    // Filtering by country code "SE" should resolve to numeric ID 208 internally
    // and still return the correct results.
    let filter = ServerFilter {
        country: Some("SE".to_string()),
        max_load: None,
    };
    let servers = provider
        .list_servers(&token_credentials(), &filter)
        .await
        .unwrap();

    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].hostname, "se142.nordvpn.com");
    assert_eq!(servers[0].country_code, "SE");
    assert_eq!(servers[0].city.as_deref(), Some("Stockholm"));
}

#[tokio::test]
async fn list_servers_with_unknown_country_code_passes_none() {
    let mock = MockNordVpnApi::new();
    *mock.servers.lock().await = vec![sample_server("xx1.nordvpn.com", 10, "XX")];
    let provider = NordVpnProvider::new(Arc::new(mock));

    // "XX" is not in our country list, so country_id should be None (no filter).
    let filter = ServerFilter {
        country: Some("XX".to_string()),
        max_load: None,
    };
    let servers = provider
        .list_servers(&token_credentials(), &filter)
        .await
        .unwrap();

    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].hostname, "xx1.nordvpn.com");
}

#[tokio::test]
async fn generate_config_rejects_credentials_auth_via_api() {
    let mock = MockNordVpnApi::new();
    *mock.server_by_hostname.lock().await = Some(sample_server("se142.nordvpn.com", 25, "SE"));
    // Simulate the real API behavior: Credentials auth returns an error.
    *mock.private_key_result.lock().await =
        Err("NordVPN no longer supports service credentials; use a token instead".to_string());
    let provider = NordVpnProvider::new(Arc::new(mock));

    let server_info = ServerInfo {
        id: "1234".to_string(),
        name: "SE #se142".to_string(),
        country_code: "SE".to_string(),
        city: None,
        hostname: "se142.nordvpn.com".to_string(),
        load: 25,
    };

    let creds = ProviderCredentials::Credentials {
        username: "user".to_string(),
        password: "pass".to_string(),
    };

    let result = provider.generate_config(&creds, &server_info).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("no longer supports service credentials"),
        "got: {err}"
    );
}

#[tokio::test]
async fn list_servers_case_insensitive_country_code() {
    let mock = MockNordVpnApi::new();
    *mock.servers.lock().await = vec![sample_server("se142.nordvpn.com", 25, "SE")];
    let provider = NordVpnProvider::new(Arc::new(mock));

    // Use lowercase "se" -- should resolve to the same country ID 208.
    let filter = ServerFilter {
        country: Some("se".to_string()),
        max_load: None,
    };
    let servers = provider
        .list_servers(&token_credentials(), &filter)
        .await
        .unwrap();

    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].hostname, "se142.nordvpn.com");
}

#[tokio::test]
async fn list_servers_no_country_filter_skips_country_lookup() {
    let mock = MockNordVpnApi::new();
    *mock.servers.lock().await = vec![
        sample_server("se1.nordvpn.com", 10, "SE"),
        sample_server("us1.nordvpn.com", 20, "US"),
    ];
    let provider = NordVpnProvider::new(Arc::new(mock));

    let filter = ServerFilter {
        country: None,
        max_load: None,
    };
    let servers = provider
        .list_servers(&token_credentials(), &filter)
        .await
        .unwrap();

    assert_eq!(servers.len(), 2);
}

#[tokio::test]
async fn list_servers_server_without_locations_has_empty_country_code() {
    let mock = MockNordVpnApi::new();
    let mut server = sample_server("noloc.nordvpn.com", 10, "SE");
    server.locations.clear();
    *mock.servers.lock().await = vec![server];
    let provider = NordVpnProvider::new(Arc::new(mock));

    let filter = ServerFilter::default();
    let servers = provider
        .list_servers(&token_credentials(), &filter)
        .await
        .unwrap();

    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].country_code, "");
    assert!(servers[0].city.is_none());
}

#[tokio::test]
async fn get_server_by_hostname_returns_server() {
    let mock = MockNordVpnApi::new();
    let nord_server = sample_server_with_city("pt131.nordvpn.com", 15, "PT", Some("Lisbon"));
    *mock.server_by_hostname.lock().await = Some(nord_server);
    let mock = Arc::new(mock);
    let provider = NordVpnProvider::new(mock);

    let result = provider
        .resolve_server(&token_credentials(), "pt131.nordvpn.com")
        .await
        .unwrap();

    let server = result.expect("should return Some");
    assert_eq!(server.hostname, "pt131.nordvpn.com");
    assert_eq!(server.country_code, "PT");
    assert_eq!(server.city.as_deref(), Some("Lisbon"));
    assert_eq!(server.load, 15);
}

#[tokio::test]
async fn resolve_server_normalizes_short_hostname() {
    let mock = MockNordVpnApi::new();
    *mock.server_by_hostname.lock().await = Some(sample_server("pt131.nordvpn.com", 15, "PT"));
    let mock = Arc::new(mock);
    let provider = NordVpnProvider::new(Arc::clone(&mock) as Arc<dyn NordVpnApi>);

    let _result = provider
        .resolve_server(&token_credentials(), "pt131")
        .await
        .unwrap();

    // Verify the API was queried with the full hostname.
    let queried = mock.last_hostname_query.lock().await;
    assert_eq!(queried.as_deref(), Some("pt131.nordvpn.com"));
}

#[tokio::test]
async fn resolve_server_full_hostname() {
    let mock = MockNordVpnApi::new();
    *mock.server_by_hostname.lock().await = Some(sample_server("pt131.nordvpn.com", 15, "PT"));
    let mock = Arc::new(mock);
    let provider = NordVpnProvider::new(Arc::clone(&mock) as Arc<dyn NordVpnApi>);

    let result = provider
        .resolve_server(&token_credentials(), "pt131.nordvpn.com")
        .await
        .unwrap();

    assert!(result.is_some());

    // Verify the API was queried with the exact hostname (no modification).
    let queried = mock.last_hostname_query.lock().await;
    assert_eq!(queried.as_deref(), Some("pt131.nordvpn.com"));
}

#[tokio::test]
async fn resolve_server_not_found() {
    let mock = MockNordVpnApi::new();
    // server_by_hostname is None, so get_server_by_hostname returns an error.
    let provider = NordVpnProvider::new(Arc::new(mock));

    let result = provider
        .resolve_server(&token_credentials(), "nonexistent.nordvpn.com")
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not found"), "got: {err}");
}

#[tokio::test]
async fn generate_config_uses_direct_hostname_lookup() {
    let mock = MockNordVpnApi::new();
    *mock.server_by_hostname.lock().await = Some(sample_server("se142.nordvpn.com", 25, "SE"));
    *mock.private_key_result.lock().await = Ok("YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo=".to_string());
    let mock = Arc::new(mock);
    let provider = NordVpnProvider::new(Arc::clone(&mock) as Arc<dyn NordVpnApi>);

    let server_info = ServerInfo {
        id: "1234".to_string(),
        name: "SE #se142".to_string(),
        country_code: "SE".to_string(),
        city: None,
        hostname: "se142.nordvpn.com".to_string(),
        load: 25,
    };

    let config_str = provider
        .generate_config(&token_credentials(), &server_info)
        .await
        .unwrap();

    // Verify the hostname query was made (direct lookup, not list_servers).
    let queried = mock.last_hostname_query.lock().await;
    assert_eq!(queried.as_deref(), Some("se142.nordvpn.com"));

    // Verify config is valid.
    let parsed = wardnet_types::wireguard_config::parse(&config_str).unwrap();
    assert_eq!(parsed.peers[0].public_key, "dGVzdC1wdWJsaWMta2V5");
}
