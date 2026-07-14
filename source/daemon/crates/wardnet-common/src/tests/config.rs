use std::path::{Path, PathBuf};

use crate::config::{
    AdminConfig, ApplicationConfiguration, LogFormat, LogRotation, SecretStoreConfig,
};

#[test]
fn defaults_when_file_missing() {
    let config = ApplicationConfiguration::load(Path::new("/tmp/wardnet-nonexistent-config.toml"))
        .expect("should return defaults");
    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 7411);
    assert_eq!(
        config.database.connection_string,
        "/var/lib/wardnet/wardnet.db"
    );
    assert_eq!(config.logging.format, LogFormat::Console);
    assert_eq!(config.logging.level, "info");
    assert_eq!(
        config.logging.path,
        PathBuf::from("/var/log/wardnet/wardnetd.log")
    );
    assert!(matches!(config.logging.rotation, LogRotation::Daily));
    assert_eq!(config.logging.max_log_files, 7);
    assert!(config.logging.filters.is_empty());
    assert_eq!(
        config.logging.ui_suppressed_targets,
        vec!["hickory_resolver::recursor".to_owned()]
    );
    assert_eq!(
        config.logging.to_filter_string(),
        "warn,wardnetd=info,wardnet_common=info,netlink_packet_route::link::buffer_tool=error"
    );
    assert_eq!(config.network.lan_interface, "eth0");
    assert_eq!(config.network.default_policy, "direct");
    assert_eq!(config.auth.session_expiry_hours, 24);
    assert!(config.secret_store.is_none());
    assert_eq!(config.tunnel.idle_timeout_secs, 600);
    assert_eq!(config.tunnel.health_check_interval_secs, 10);
    assert_eq!(config.tunnel.stats_interval_secs, 5);
    assert!(config.detection.enabled);
    assert_eq!(config.detection.departure_timeout_secs, 300);
    assert_eq!(config.detection.batch_flush_interval_secs, 30);
    assert_eq!(config.detection.departure_scan_interval_secs, 60);
    assert_eq!(config.detection.arp_scan_interval_secs, 60);
    assert!(!config.otel.enabled);
    assert_eq!(config.otel.endpoint, "http://localhost:4317");
    assert_eq!(config.otel.service_name, "wardnetd");
}

#[test]
fn load_from_toml_file() {
    let dir = std::env::temp_dir().join("wardnet-config-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("wardnet-test.toml");
    std::fs::write(
        &path,
        r#"
[server]
host = "127.0.0.1"
port = 8080

[vpn_providers.enabled]
nordvpn = false
"#,
    )
    .unwrap();

    let config = ApplicationConfiguration::load(&path).unwrap();
    assert_eq!(config.server.host, "127.0.0.1");
    assert_eq!(config.server.port, 8080);
    assert!(!config.is_vpn_provider_enabled("nordvpn"));

    // Clean up.
    let _ = std::fs::remove_file(&path);
}

#[test]
fn load_secret_store_file_system_section() {
    let dir = std::env::temp_dir().join("wardnet-config-secret-store-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("wardnet-secret-store.toml");
    std::fs::write(
        &path,
        r#"
[secret_store]
provider = "file_system"
path = "/var/lib/wardnet/secrets"
"#,
    )
    .unwrap();

    let config = ApplicationConfiguration::load(&path).unwrap();
    match config.secret_store.as_ref().expect("secret_store parsed") {
        SecretStoreConfig::FileSystem { path } => {
            assert_eq!(path, &PathBuf::from("/var/lib/wardnet/secrets"));
        }
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn unknown_top_level_key_is_rejected() {
    let dir = std::env::temp_dir().join("wardnet-config-unknown-top-level-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("wardnet-unknown-top-level.toml");
    // `databse` is a typo of the `[database]` section: it must fail loudly,
    // naming the offending key, rather than being silently dropped.
    std::fs::write(
        &path,
        r#"
[server]
port = 8080

[databse]
connection_string = "/tmp/x.db"
"#,
    )
    .unwrap();

    let err =
        ApplicationConfiguration::load(&path).expect_err("unknown top-level key must be rejected");
    assert!(
        err.to_string().contains("databse"),
        "error should name the offending key, got: {err}"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn unknown_nested_key_is_rejected() {
    let dir = std::env::temp_dir().join("wardnet-config-unknown-nested-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("wardnet-unknown-nested.toml");
    // `porrt` is a typo of `port` inside an otherwise-valid section.
    std::fs::write(
        &path,
        r"
[server]
porrt = 8080
",
    )
    .unwrap();

    let err =
        ApplicationConfiguration::load(&path).expect_err("unknown nested key must be rejected");
    assert!(
        err.to_string().contains("porrt"),
        "error should name the offending key, got: {err}"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn is_provider_enabled_default_true() {
    let config = ApplicationConfiguration::default();
    // Providers not in the map should default to enabled.
    assert!(config.is_vpn_provider_enabled("nordvpn"));
    assert!(config.is_vpn_provider_enabled("unknown_provider"));
}

#[test]
fn nordvpn_api_url_defaults_to_none() {
    let config = ApplicationConfiguration::default();
    assert!(config.vpn_providers.nordvpn_api_url.is_none());
}

#[test]
fn load_nordvpn_api_url_override() {
    let dir = std::env::temp_dir().join("wardnet-config-nordvpn-url-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("wardnet-nordvpn-url.toml");
    std::fs::write(
        &path,
        r#"
[vpn_providers]
nordvpn_api_url = "http://10.92.0.52:8080"
"#,
    )
    .unwrap();

    let config = ApplicationConfiguration::load(&path).unwrap();
    assert_eq!(
        config.vpn_providers.nordvpn_api_url.as_deref(),
        Some("http://10.92.0.52:8080")
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn ddns_wardnet_overrides_default_to_none() {
    let config = ApplicationConfiguration::default();
    assert!(config.ddns_wardnet.gateway_url.is_none());
    assert!(config.ddns_wardnet.region_gateway_url.is_none());
    assert!(config.ddns_wardnet.region_health_url.is_none());
}

#[test]
fn load_ddns_wardnet_overrides() {
    let dir = std::env::temp_dir().join("wardnet-config-ddns-wardnet-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("wardnet-ddns-wardnet.toml");
    std::fs::write(
        &path,
        r#"
[ddns_wardnet]
gateway_url = "http://10.92.0.53:8080"
region_gateway_url = "http://10.92.0.53:8080"
region_health_url = "http://10.92.0.53:8080/ddns/v1/health"
"#,
    )
    .unwrap();

    let config = ApplicationConfiguration::load(&path).unwrap();
    assert_eq!(
        config.ddns_wardnet.gateway_url.as_deref(),
        Some("http://10.92.0.53:8080")
    );
    assert_eq!(
        config.ddns_wardnet.region_gateway_url.as_deref(),
        Some("http://10.92.0.53:8080")
    );
    assert_eq!(
        config.ddns_wardnet.region_health_url.as_deref(),
        Some("http://10.92.0.53:8080/ddns/v1/health")
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn is_provider_enabled_explicit_false() {
    let mut config = ApplicationConfiguration::default();
    config
        .vpn_providers
        .enabled
        .insert("nordvpn".to_owned(), false);
    assert!(!config.is_vpn_provider_enabled("nordvpn"));
}

#[test]
fn is_provider_enabled_explicit_true() {
    let mut config = ApplicationConfiguration::default();
    config
        .vpn_providers
        .enabled
        .insert("nordvpn".to_owned(), true);
    assert!(config.is_vpn_provider_enabled("nordvpn"));
}

#[test]
fn to_filter_string_with_overrides() {
    let mut config = ApplicationConfiguration::default();
    config.logging.level = "debug".to_owned();
    config
        .logging
        .filters
        .insert("sqlx".to_owned(), "trace".to_owned());

    let filter = config.logging.to_filter_string();
    assert!(filter.contains("wardnetd=debug"));
    assert!(filter.contains("wardnet_common=debug"));
    assert!(filter.contains("sqlx=trace"));
}

#[test]
fn admin_config_debug_redacts_password() {
    let cfg = AdminConfig {
        username: "bootstrap-admin".to_owned(),
        password: "bootstrap-hunter2".to_owned(),
    };
    let rendered = format!("{cfg:?}");
    assert!(rendered.contains("bootstrap-admin"));
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains("bootstrap-hunter2"));
}
