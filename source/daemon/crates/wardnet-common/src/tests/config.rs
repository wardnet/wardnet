use std::path::{Path, PathBuf};

use crate::config::{
    AdminConfig, AnomaliesConfig, ApplicationConfiguration, LogFormat, LogRotation, LoggingConfig,
    SecretStoreConfig, TunnelConfig,
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

/// Cloudflare's `__down` endpoint serves the payload only while `bytes` stays
/// under 100 MB — at or above that it answers `403` with no body, which the
/// throughput tester surfaces as a non-success status, failing every stream
/// ("all parallel download streams failed"). Shipping a default the endpoint
/// refuses breaks the speed test on every box that hasn't overridden the URL,
/// so pin the default below the cap.
#[test]
fn default_speed_test_url_requests_a_payload_cloudflare_will_serve() {
    /// Smallest `bytes` value observed to return 403 rather than a payload.
    const CLOUDFLARE_DOWN_BYTES_CAP: u64 = 100_000_000;

    let config = TunnelConfig::default();
    let bytes: u64 = config
        .speed_test_url
        .split_once("bytes=")
        .expect("default speed_test_url should carry a bytes= query parameter")
        .1
        .parse()
        .expect("bytes= should be a plain integer");

    assert!(
        bytes < CLOUDFLARE_DOWN_BYTES_CAP,
        "default speed_test_url requests {bytes} bytes, at/above Cloudflare's \
         {CLOUDFLARE_DOWN_BYTES_CAP}-byte cap — the endpoint will 403 and every \
         stream will fail",
    );
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
fn stub_tunnel_backends_defaults_off() {
    // A production config never carries a `[test]` section; the seam must stay
    // disabled by default so the real WireGuard / HTTP / ICMP backends are used.
    let config = ApplicationConfiguration::default();
    assert!(!config.test.stub_tunnel_backends);
}

#[test]
fn load_test_stub_tunnel_backends_override() {
    let dir = std::env::temp_dir().join("wardnet-config-test-section");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("wardnet-test-section.toml");
    std::fs::write(
        &path,
        r"
[test]
stub_tunnel_backends = true
",
    )
    .unwrap();

    let config = ApplicationConfiguration::load(&path).unwrap();
    assert!(config.test.stub_tunnel_backends);

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

// ── journal slice predicate ─────────────────────────────────────────────────

use tracing::Level;

fn journal_defaults() -> Vec<String> {
    LoggingConfig::default().journal_suppressed_targets
}

fn journal_info_defaults() -> Vec<String> {
    LoggingConfig::default().journal_info_targets
}

/// INFO and below never reach the journal from an ordinary target — that is
/// what the log file is for.
#[test]
fn journal_excludes_below_warn() {
    for level in [Level::INFO, Level::DEBUG, Level::TRACE] {
        assert!(
            !LoggingConfig::journal_allows(
                level,
                "wardnetd_services::routing::service",
                &journal_defaults(),
                &journal_info_defaults(),
            ),
            "{level} should not reach the journal"
        );
    }
}

/// The daily housekeeping summaries are the exception: their absence is the
/// signal, and a level-only filter cannot express that. Both runners report
/// success at INFO, so without this the journal looks the same whether the
/// database is being maintained or has quietly stopped shrinking.
#[test]
fn journal_includes_daily_maintenance_summaries_at_info() {
    for target in [
        "wardnetd_services::db_maintenance_runner",
        "wardnetd_services::dns::query_log_runner",
    ] {
        assert!(
            LoggingConfig::journal_allows(
                Level::INFO,
                target,
                &journal_defaults(),
                &journal_info_defaults(),
            ),
            "{target} INFO should reach the journal"
        );
    }
}

/// Raising INFO is target-scoped, not level-scoped: DEBUG from the very same
/// module stays in the log file, or a `--log-level debug` run would flood the
/// journal.
#[test]
fn journal_info_targets_do_not_admit_debug() {
    for level in [Level::DEBUG, Level::TRACE] {
        assert!(
            !LoggingConfig::journal_allows(
                level,
                "wardnetd_services::db_maintenance_runner",
                &journal_defaults(),
                &journal_info_defaults(),
            ),
            "{level} from an INFO-raised target should not reach the journal"
        );
    }
}

/// Suppression wins over inclusion. An operator who silences a target has said
/// the more specific thing, and the alternative quietly re-admits exactly what
/// they asked to be rid of.
#[test]
fn journal_suppression_beats_info_inclusion() {
    let both = vec!["wardnetd_services::db_maintenance_runner".to_owned()];
    assert!(!LoggingConfig::journal_allows(
        Level::INFO,
        "wardnetd_services::db_maintenance_runner",
        &both,
        &both,
    ));
    // ERROR still passes: suppression is a noise filter, never a blind spot.
    assert!(LoggingConfig::journal_allows(
        Level::ERROR,
        "wardnetd_services::db_maintenance_runner",
        &both,
        &both,
    ));
}

/// The events an operator acts on do reach the journal.
#[test]
fn journal_includes_actionable_warnings_and_errors() {
    for (level, target) in [
        (Level::WARN, "wardnetd_services::routing::service"),
        (Level::WARN, "wardnetd::route_monitor"),
        // sqlx slow-statement / slow-acquire warnings are deliberately kept:
        // they are the first visible symptom of the database degrading, which
        // is exactly the class of problem the journal slice exists to surface.
        (Level::WARN, "sqlx::query"),
        (Level::WARN, "sqlx::pool::acquire"),
        (Level::ERROR, "wardnetd_services::dns_filter::runner"),
        // Suppression is scoped to WARN-level noise; a genuine ERROR from a
        // suppressed target still gets through.
        (Level::ERROR, "hickory_resolver::recursor::handle"),
    ] {
        assert!(
            LoggingConfig::journal_allows(
                level,
                target,
                &journal_defaults(),
                &journal_info_defaults(),
            ),
            "{level} {target} should reach the journal"
        );
    }
}

/// The per-lookup DNS noise does not.
///
/// This was 97% of all WARN+ traffic measured on a live gateway — one warning
/// per failed recursive lookup, i.e. an ordinary negative DNS answer. Letting
/// it through would put ~30k lines/day into the journal and bury everything
/// above.
#[test]
fn journal_suppresses_per_query_dns_noise_at_warn() {
    for target in [
        "hickory_resolver::recursor::handle",
        "hickory_resolver::recursor::error",
        "hickory_resolver",
        "wardnetd::dns::pipeline",
    ] {
        assert!(
            !LoggingConfig::journal_allows(
                Level::WARN,
                target,
                &journal_defaults(),
                &journal_info_defaults(),
            ),
            "{target} WARN should be suppressed"
        );
    }
}

/// Suppression is prefix-based, so one entry covers a module subtree — but must
/// not swallow an unrelated target that merely shares a leading substring.
#[test]
fn journal_suppression_matches_on_prefix_only() {
    let suppressed = vec!["hickory_resolver".to_owned()];
    assert!(!LoggingConfig::journal_allows(
        Level::WARN,
        "hickory_resolver::recursor::handle",
        &suppressed,
        &[],
    ));
    assert!(LoggingConfig::journal_allows(
        Level::WARN,
        "wardnetd::dns::pipeline",
        &suppressed,
        &[],
    ));
    assert!(LoggingConfig::journal_allows(
        Level::WARN,
        "hickory_proto::op",
        &suppressed,
        &[],
    ));
}

/// An empty list degrades to a plain level filter, so an operator can opt back
/// into the full firehose.
#[test]
fn journal_empty_suppression_list_admits_all_warnings() {
    assert!(LoggingConfig::journal_allows(
        Level::WARN,
        "hickory_resolver::recursor::handle",
        &[],
        &[],
    ));
    assert!(!LoggingConfig::journal_allows(
        Level::INFO,
        "wardnetd::route_monitor",
        &[],
        &[],
    ));
}

/// An empty prefix would match every target. A stray `""` in either list is a
/// config typo, not a request to journal the entire log stream.
#[test]
fn journal_ignores_empty_prefixes() {
    let empty_entry = vec![String::new()];
    assert!(LoggingConfig::journal_allows(
        Level::WARN,
        "wardnetd::route_monitor",
        &empty_entry,
        &[],
    ));
    assert!(!LoggingConfig::journal_allows(
        Level::INFO,
        "wardnetd::route_monitor",
        &[],
        &empty_entry,
    ));
}

/// A `0` interval is not a useful setting and is actively harmful: the engine
/// reschedules at `Instant::now() + interval`, so zero makes `sleep_until`
/// return immediately and spins `reevaluate_all` — a `list_open` plus one
/// detector call per open anomaly — in a tight loop. A zero detector timeout
/// makes every sweep time out instantly instead. Both are floored.
#[test]
fn anomaly_intervals_are_floored_at_one_second() {
    let zeroed = AnomaliesConfig {
        reevaluate_interval_secs: 0,
        detect_timeout_secs: 0,
        ..AnomaliesConfig::default()
    };

    assert_eq!(
        zeroed.reevaluate_interval(),
        std::time::Duration::from_secs(1)
    );
    assert_eq!(zeroed.detect_timeout(), std::time::Duration::from_secs(1));
}

/// The floor must not clamp ordinary values — only rescue the degenerate one.
#[test]
fn anomaly_intervals_pass_through_above_the_floor() {
    let config = AnomaliesConfig {
        reevaluate_interval_secs: 60,
        detect_timeout_secs: 30,
        ..AnomaliesConfig::default()
    };

    assert_eq!(
        config.reevaluate_interval(),
        std::time::Duration::from_mins(1)
    );
    assert_eq!(config.detect_timeout(), std::time::Duration::from_secs(30));

    let defaults = AnomaliesConfig::default();
    assert_eq!(
        defaults.reevaluate_interval(),
        std::time::Duration::from_mins(1)
    );
    assert_eq!(
        defaults.detect_timeout(),
        std::time::Duration::from_secs(30)
    );
}
