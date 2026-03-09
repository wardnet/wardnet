use std::path::{Path, PathBuf};

use crate::config::{Config, LogFormat, LogRotation};

#[test]
fn defaults_when_file_missing() {
    let config = Config::load(Path::new("/tmp/wardnet-nonexistent-config.toml"))
        .expect("should return defaults");
    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 7411);
    assert_eq!(config.database.path, PathBuf::from("./wardnet.db"));
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
        "warn,wardnetd=info,wardnet_types=info"
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
