use crate::vpn_provider::{
    ProviderAuthMethod, ProviderCredentials, ProviderInfo, ServerFilter, ServerInfo,
};

#[test]
fn credentials_variant_round_trip() {
    let creds = ProviderCredentials::Credentials {
        username: "user@example.com".to_owned(),
        password: "s3cret".to_owned(),
    };
    let json = serde_json::to_string(&creds).unwrap();
    assert!(json.contains(r#""type":"credentials""#));

    let parsed: ProviderCredentials = serde_json::from_str(&json).unwrap();
    match parsed {
        ProviderCredentials::Credentials { username, password } => {
            assert_eq!(username, "user@example.com");
            assert_eq!(password, "s3cret");
        }
        ProviderCredentials::Token { .. } => panic!("expected Credentials variant"),
    }
}

#[test]
fn token_variant_round_trip() {
    let creds = ProviderCredentials::Token {
        token: "abc123".to_owned(),
    };
    let json = serde_json::to_string(&creds).unwrap();
    assert!(json.contains(r#""type":"token""#));

    let parsed: ProviderCredentials = serde_json::from_str(&json).unwrap();
    match parsed {
        ProviderCredentials::Token { token } => {
            assert_eq!(token, "abc123");
        }
        ProviderCredentials::Credentials { .. } => panic!("expected Token variant"),
    }
}

#[test]
fn provider_info_with_all_fields() {
    let info = ProviderInfo {
        id: "nordvpn".to_owned(),
        name: "NordVPN".to_owned(),
        auth_methods: vec![ProviderAuthMethod::Credentials, ProviderAuthMethod::Token],
        icon_url: Some("https://example.com/icon.png".to_owned()),
        website_url: Some("https://nordvpn.com".to_owned()),
        credentials_hint: Some("Use your service credentials".to_owned()),
    };
    let json = serde_json::to_string(&info).unwrap();
    let parsed: ProviderInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.id, "nordvpn");
    assert_eq!(parsed.name, "NordVPN");
    assert_eq!(parsed.auth_methods.len(), 2);
    assert_eq!(
        parsed.icon_url.as_deref(),
        Some("https://example.com/icon.png")
    );
    assert_eq!(parsed.website_url.as_deref(), Some("https://nordvpn.com"));
}

#[test]
fn provider_info_without_optional_fields() {
    let info = ProviderInfo {
        id: "mullvad".to_owned(),
        name: "Mullvad".to_owned(),
        auth_methods: vec![ProviderAuthMethod::Token],
        icon_url: None,
        website_url: None,
        credentials_hint: None,
    };
    let json = serde_json::to_string(&info).unwrap();
    // Optional fields should be omitted from JSON
    assert!(!json.contains("icon_url"));
    assert!(!json.contains("website_url"));

    let parsed: ProviderInfo = serde_json::from_str(&json).unwrap();
    assert!(parsed.icon_url.is_none());
    assert!(parsed.website_url.is_none());
}

#[test]
fn server_filter_defaults() {
    let filter = ServerFilter::default();
    assert!(filter.country.is_none());
    assert!(filter.max_load.is_none());

    // Empty JSON object should deserialize to defaults
    let parsed: ServerFilter = serde_json::from_str("{}").unwrap();
    assert!(parsed.country.is_none());
    assert!(parsed.max_load.is_none());
}

#[test]
fn server_filter_with_values() {
    let filter = ServerFilter {
        country: Some("SE".to_owned()),
        max_load: Some(50),
    };
    let json = serde_json::to_string(&filter).unwrap();
    let parsed: ServerFilter = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.country.as_deref(), Some("SE"));
    assert_eq!(parsed.max_load, Some(50));
}

#[test]
fn server_info_round_trip() {
    let server = ServerInfo {
        id: "se142".to_owned(),
        name: "Sweden #142".to_owned(),
        country_code: "SE".to_owned(),
        city: Some("Stockholm".to_owned()),
        hostname: "se142.nordvpn.com".to_owned(),
        load: 23,
    };
    let json = serde_json::to_string(&server).unwrap();
    let parsed: ServerInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.id, "se142");
    assert_eq!(parsed.name, "Sweden #142");
    assert_eq!(parsed.country_code, "SE");
    assert_eq!(parsed.city.as_deref(), Some("Stockholm"));
    assert_eq!(parsed.hostname, "se142.nordvpn.com");
    assert_eq!(parsed.load, 23);
}

#[test]
fn server_info_without_city() {
    let server = ServerInfo {
        id: "us100".to_owned(),
        name: "United States #100".to_owned(),
        country_code: "US".to_owned(),
        city: None,
        hostname: "us100.nordvpn.com".to_owned(),
        load: 45,
    };
    let json = serde_json::to_string(&server).unwrap();
    assert!(!json.contains("city"));

    let parsed: ServerInfo = serde_json::from_str(&json).unwrap();
    assert!(parsed.city.is_none());
}

#[test]
fn auth_method_serializes_snake_case() {
    let cred = ProviderAuthMethod::Credentials;
    let token = ProviderAuthMethod::Token;

    assert_eq!(serde_json::to_string(&cred).unwrap(), r#""credentials""#);
    assert_eq!(serde_json::to_string(&token).unwrap(), r#""token""#);
}
