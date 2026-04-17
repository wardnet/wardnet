use std::path::{Path, PathBuf};

use crate::config::{ApplicationConfiguration, LogFormat, LogRotation};

#[test]
fn defaults_when_file_missing() {
    let config = ApplicationConfiguration::load(Path::new("/tmp/wardnet-nonexistent-config.toml"))
        .expect("should return defaults");
    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 7411);
    assert_eq!(config.database.connection_string, "./wardnet.db");
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
        config.logging.to_filter_string(),
        "warn,wardnetd=info,wardnet_common=info"
    );
    assert_eq!(config.network.lan_interface, "eth0");
    assert_eq!(config.network.default_policy, "direct");
    assert_eq!(config.auth.session_expiry_hours, 24);
    assert_eq!(config.tunnel.keys_dir, PathBuf::from("/etc/wardnet/keys"));
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
fn is_provider_enabled_default_true() {
    let config = ApplicationConfiguration::default();
    // Providers not in the map should default to enabled.
    assert!(config.is_vpn_provider_enabled("nordvpn"));
    assert!(config.is_vpn_provider_enabled("unknown_provider"));
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
