//! Tests for [`crate::config_restore`].
//!
//! Two concerns live here. The merge tests pin the behaviour a restore
//! relies on: deploy-time keys come from the live machine, everything else
//! comes from the bundle. The classification test is the guard that keeps
//! the first set honest as the config grows — see
//! [`every_config_key_is_classified`].

use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::{
    AdminConfig, ApplicationConfiguration, AuthConfig, DatabaseConfig, DatabaseProvider,
    DdnsWardnetConfig, DetectionConfig, EnabledMetrics, HealthConfig, LogFormat, LogRotation,
    LoggingConfig, MdnsConfig, NetworkConfig, OtelConfig, OtelLogsConfig, OtelMetricsConfig,
    OtelTracesConfig, PyroscopeConfig, SecretStoreConfig, ServerConfig, TestConfig, TunnelConfig,
    UpdateConfig, VpnProvidersConfig, WatchdogConfig,
};
use crate::config_restore::{ConfigRestoreError, DEPLOY_TIME_ONLY_KEYS, preserve_deploy_time_keys};

/// Config keys a backup bundle *may* set: the user's own settings, which
/// travel with their data. The counterpart to
/// [`DEPLOY_TIME_ONLY_KEYS`] — between them the two lists must name every
/// key in the config, which is what
/// [`every_config_key_is_classified`] checks.
const RESTORABLE_KEYS: &[&str] = &[
    "server.host",
    "server.port",
    "server.https_port",
    "server.http_redirect_port",
    "logging.format",
    "logging.level",
    "logging.filters",
    "logging.rotation",
    "logging.max_log_files",
    "logging.max_recent_errors",
    "logging.broadcast_capacity",
    "logging.ui_suppressed_targets",
    "logging.journal_suppressed_targets",
    "network.default_policy",
    "auth.session_expiry_hours",
    "auth.remember_me_expiry_hours",
    // Bootstrap credentials, used only when the restored database holds no
    // admin at all. An admin session can already create an admin through
    // the API, so a bundle carrying these grants nothing it did not have.
    "admin.username",
    "admin.password",
    "tunnel.idle_timeout_secs",
    "tunnel.health_check_interval_secs",
    "tunnel.stats_interval_secs",
    "tunnel.latency_probe_interval_secs",
    "tunnel.latency_probe_target",
    "tunnel.test_probe_url",
    "tunnel.speed_test_url",
    "tunnel.speed_test_latency_samples",
    "tunnel.speed_test_parallel_streams",
    "tunnel.speed_test_warmup_ms",
    "tunnel.speed_test_measure_ms",
    "detection.enabled",
    "detection.departure_timeout_secs",
    "detection.batch_flush_interval_secs",
    "detection.departure_scan_interval_secs",
    "detection.arp_scan_interval_secs",
    // Telemetry export. Repointing it at a collector of the attacker's
    // choosing leaks nothing an admin session cannot already read from the
    // live log stream, so it stays a user setting.
    "otel.enabled",
    "otel.endpoint",
    "otel.service_name",
    "otel.interval_secs",
    "otel.traces.enabled",
    "otel.logs.enabled",
    "otel.metrics.enabled",
    "otel.metrics.enabled_metrics.system_cpu_utilization",
    "otel.metrics.enabled_metrics.system_memory_usage",
    "otel.metrics.enabled_metrics.system_temperature",
    "otel.metrics.enabled_metrics.system_network_io",
    "otel.metrics.enabled_metrics.wardnet_device_count",
    "otel.metrics.enabled_metrics.wardnet_tunnel_count",
    "otel.metrics.enabled_metrics.wardnet_tunnel_active_count",
    "otel.metrics.enabled_metrics.wardnet_uptime_seconds",
    "otel.metrics.enabled_metrics.wardnet_db_size_bytes",
    "otel.metrics.enabled_metrics.wardnet_disk_free_bytes",
    "vpn_providers.enabled",
    "pyroscope.enabled",
    "pyroscope.endpoint",
    "mdns.enabled",
    "mdns.hostname",
    "health.refresh_interval_secs",
    "health.failure_threshold",
    "health.check_timeout_secs",
    "watchdog.hardware_timeout_secs",
    "watchdog.pet_interval_secs",
    "watchdog.soft_enabled",
];

/// A config with **every** field written out explicitly and every optional
/// field populated, so serialising it yields the full set of config keys.
///
/// The struct literals are the point: they carry no `..Default::default()`,
/// so adding a field anywhere in the config tree fails to compile here
/// before it can slip past [`every_config_key_is_classified`] unclassified.
/// Open-ended maps (`logging.filters`, `vpn_providers.enabled`) are left
/// empty — their keys are user data, not config fields.
#[allow(clippy::too_many_lines)] // One literal per config field — length is the point.
fn probe_config() -> ApplicationConfiguration {
    ApplicationConfiguration {
        server: ServerConfig {
            host: "0.0.0.0".to_owned(),
            port: 7411,
            https_port: 443,
            http_redirect_port: 80,
        },
        database: DatabaseConfig {
            provider: DatabaseProvider::Sqlite,
            connection_string: "/var/lib/wardnet/wardnet.db".to_owned(),
        },
        logging: LoggingConfig {
            format: LogFormat::Json,
            level: "info".to_owned(),
            filters: HashMap::new(),
            path: PathBuf::from("/var/log/wardnet/wardnetd.log"),
            rotation: LogRotation::Daily,
            max_log_files: 7,
            max_recent_errors: 15,
            broadcast_capacity: 256,
            ui_suppressed_targets: Vec::new(),
            journal_suppressed_targets: Vec::new(),
        },
        network: NetworkConfig {
            lan_interface: "eth0".to_owned(),
            default_policy: "direct".to_owned(),
        },
        auth: AuthConfig {
            session_expiry_hours: 24,
            remember_me_expiry_hours: 720,
        },
        admin: Some(AdminConfig {
            username: "admin".to_owned(),
            password: "hunter2".to_owned(),
        }),
        tunnel: TunnelConfig {
            idle_timeout_secs: 600,
            health_check_interval_secs: 10,
            stats_interval_secs: 5,
            latency_probe_interval_secs: 60,
            latency_probe_target: "1.1.1.1".to_owned(),
            test_probe_url: "https://1.1.1.1/cdn-cgi/trace".to_owned(),
            speed_test_url: "https://speed.cloudflare.com/__down".to_owned(),
            speed_test_latency_samples: 5,
            speed_test_parallel_streams: 4,
            speed_test_warmup_ms: 1000,
            speed_test_measure_ms: 4000,
        },
        detection: DetectionConfig {
            enabled: true,
            departure_timeout_secs: 300,
            batch_flush_interval_secs: 30,
            departure_scan_interval_secs: 60,
            arp_scan_interval_secs: 60,
        },
        otel: OtelConfig {
            enabled: false,
            endpoint: "http://localhost:4317".to_owned(),
            service_name: "wardnetd".to_owned(),
            interval_secs: 10,
            traces: OtelTracesConfig { enabled: true },
            logs: OtelLogsConfig { enabled: true },
            metrics: OtelMetricsConfig {
                enabled: true,
                enabled_metrics: EnabledMetrics {
                    system_cpu_utilization: true,
                    system_memory_usage: true,
                    system_temperature: true,
                    system_network_io: true,
                    wardnet_device_count: true,
                    wardnet_tunnel_count: true,
                    wardnet_tunnel_active_count: true,
                    wardnet_uptime_seconds: true,
                    wardnet_db_size_bytes: true,
                    wardnet_disk_free_bytes: true,
                },
            },
        },
        vpn_providers: VpnProvidersConfig {
            enabled: HashMap::new(),
            nordvpn_api_url: Some("https://api.nordvpn.com".to_owned()),
        },
        ddns_wardnet: DdnsWardnetConfig {
            gateway_url: Some("https://api.wardnet.network".to_owned()),
            region_gateway_url: Some("https://eu.wardnet.network".to_owned()),
            region_health_url: Some("https://eu.wardnet.network/health".to_owned()),
        },
        pyroscope: PyroscopeConfig {
            enabled: false,
            endpoint: "http://localhost:4040".to_owned(),
        },
        update: UpdateConfig {
            manifest_base_url: "https://releases.wardnet.network".to_owned(),
            check_interval_secs: 21_600,
            live_binary_path: PathBuf::from("/usr/local/bin/wardnetd"),
            staging_dir: PathBuf::from("/var/lib/wardnet/updates"),
            require_signature: true,
            http_timeout_secs: 60,
            allow_edge_channel: false,
        },
        mdns: MdnsConfig {
            enabled: true,
            hostname: Some("wardnet".to_owned()),
        },
        health: HealthConfig {
            refresh_interval_secs: 5,
            failure_threshold: 3,
            check_timeout_secs: 2,
        },
        watchdog: WatchdogConfig {
            enabled: true,
            device_path: PathBuf::from("/dev/watchdog"),
            hardware_timeout_secs: 15,
            pet_interval_secs: 5,
            soft_enabled: true,
        },
        test: TestConfig {
            stub_tunnel_backends: false,
        },
        secret_store: Some(SecretStoreConfig::FileSystem {
            path: PathBuf::from("/var/lib/wardnet/secrets"),
        }),
        pidfile_path: PathBuf::from("/run/wardnetd/wardnetd.pid"),
    }
}

/// Every dotted path in `value` that holds a scalar, an array, or an empty
/// table — i.e. the config's leaves, the granularity classification works at.
fn leaf_paths(value: &toml::Value, prefix: &str, out: &mut Vec<String>) {
    match value {
        toml::Value::Table(table) if !table.is_empty() => {
            for (key, child) in table {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                leaf_paths(child, &path, out);
            }
        }
        _ => out.push(prefix.to_owned()),
    }
}

/// Whether `key` names `path` itself or one of its ancestors.
fn covers(key: &str, path: &str) -> bool {
    path == key
        || path
            .strip_prefix(key)
            .is_some_and(|rest| rest.starts_with('.'))
}

fn probe_leaves() -> Vec<String> {
    let value = toml::Value::try_from(probe_config()).expect("probe config should serialise");
    let mut leaves = Vec::new();
    leaf_paths(&value, "", &mut leaves);
    leaves
}

// ---------------------------------------------------------------------------
// Classification coverage
// ---------------------------------------------------------------------------

#[test]
fn every_config_key_is_classified() {
    let unclassified: Vec<String> = probe_leaves()
        .into_iter()
        .filter(|path| {
            !DEPLOY_TIME_ONLY_KEYS
                .iter()
                .chain(RESTORABLE_KEYS)
                .any(|key| covers(key, path))
        })
        .collect();

    assert!(
        unclassified.is_empty(),
        "config keys with no restore classification: {unclassified:?}\n\
         Decide whether a backup bundle may set each one. Machine-scoped keys \
         (update path, sandboxed filesystem locations, hardware identity, \
         test-harness overrides) go in DEPLOY_TIME_ONLY_KEYS in \
         config_restore.rs; user settings go in RESTORABLE_KEYS in this file.",
    );
}

#[test]
fn no_key_is_classified_both_ways() {
    let both: Vec<&&str> = DEPLOY_TIME_ONLY_KEYS
        .iter()
        .filter(|key| {
            RESTORABLE_KEYS
                .iter()
                .any(|other| covers(other, key) || covers(key, other))
        })
        .collect();
    assert!(both.is_empty(), "keys classified both ways: {both:?}");
}

#[test]
fn no_classified_key_is_stale() {
    let leaves = probe_leaves();
    let stale: Vec<&&str> = DEPLOY_TIME_ONLY_KEYS
        .iter()
        .chain(RESTORABLE_KEYS)
        .filter(|key| !leaves.iter().any(|path| covers(key, path)))
        .collect();
    assert!(
        stale.is_empty(),
        "classified keys that no longer exist in the config (renamed or removed?): {stale:?}",
    );
}

// ---------------------------------------------------------------------------
// The keys the gate exists for
// ---------------------------------------------------------------------------

#[test]
fn bundle_cannot_enable_the_edge_channel() {
    // Live box has never opted into edge: the key is simply absent.
    let merged = preserve_deploy_time_keys(
        "[network]\nlan_interface = \"eth0\"\n",
        "[network]\nlan_interface = \"eth0\"\n\n[update]\nallow_edge_channel = true\n",
    )
    .unwrap();

    let table: toml::Table = merged.toml.parse().unwrap();
    assert!(
        table.get("update").is_none(),
        "the bundle's [update] section should be gone entirely: {}",
        merged.toml,
    );
    assert_eq!(merged.overridden, vec!["update.allow_edge_channel"]);
}

#[test]
fn bundle_cannot_turn_the_edge_channel_off_either() {
    // The gate is not one-directional: the live machine's value wins
    // whichever way it points.
    let merged = preserve_deploy_time_keys(
        "[update]\nallow_edge_channel = true\n",
        "[update]\nallow_edge_channel = false\n",
    )
    .unwrap();

    let table: toml::Table = merged.toml.parse().unwrap();
    assert_eq!(
        table["update"]["allow_edge_channel"].as_bool(),
        Some(true),
        "live value should have been kept: {}",
        merged.toml,
    );
}

#[test]
fn bundle_cannot_repoint_the_update_source() {
    let merged = preserve_deploy_time_keys(
        "[update]\nmanifest_base_url = \"https://releases.wardnet.network\"\n",
        "[update]\nmanifest_base_url = \"https://attacker.example\"\n",
    )
    .unwrap();

    let table: toml::Table = merged.toml.parse().unwrap();
    assert_eq!(
        table["update"]["manifest_base_url"].as_str(),
        Some("https://releases.wardnet.network"),
    );
    assert_eq!(merged.overridden, vec!["update.manifest_base_url"]);
}

#[test]
fn bundle_cannot_disable_signature_verification() {
    let merged = preserve_deploy_time_keys("", "[update]\nrequire_signature = false\n").unwrap();
    let table: toml::Table = merged.toml.parse().unwrap();
    assert!(
        table.get("update").is_none(),
        "require_signature should fall back to its compiled default: {}",
        merged.toml,
    );
}

#[test]
fn bundle_cannot_point_the_box_at_a_stand_in_cloud() {
    let merged = preserve_deploy_time_keys(
        "",
        "[ddns_wardnet]\ngateway_url = \"https://attacker.example\"\n",
    )
    .unwrap();
    let table: toml::Table = merged.toml.parse().unwrap();
    assert!(table.get("ddns_wardnet").is_none(), "{}", merged.toml);
    assert_eq!(merged.overridden, vec!["ddns_wardnet.gateway_url"]);
}

// ---------------------------------------------------------------------------
// Everything else still restores
// ---------------------------------------------------------------------------

#[test]
fn ordinary_settings_restore_from_the_bundle() {
    let merged = preserve_deploy_time_keys(
        "[logging]\nlevel = \"info\"\n\n[detection]\nenabled = true\n",
        "[logging]\nlevel = \"debug\"\n\n[detection]\nenabled = false\n\n[update]\nallow_edge_channel = true\n",
    )
    .unwrap();

    let table: toml::Table = merged.toml.parse().unwrap();
    assert_eq!(table["logging"]["level"].as_str(), Some("debug"));
    assert_eq!(table["detection"]["enabled"].as_bool(), Some(false));
    assert!(table.get("update").is_none());
}

#[test]
fn a_bundle_that_changes_nothing_passes_through_verbatim() {
    // Comments and key order are the operator's, and install.sh writes
    // several — re-serialising would silently drop them.
    let config = "# written by install.sh\n[network]\nlan_interface = \"eth0\"\n\n[update]\n# opted in with root\nallow_edge_channel = true\n";
    let merged = preserve_deploy_time_keys(config, config).unwrap();

    assert_eq!(merged.toml, config);
    assert!(merged.overridden.is_empty());
}

#[test]
fn keys_this_daemon_does_not_know_survive_the_merge() {
    // A bundle from a newer build carries sections we have no struct for.
    // The merge is untyped precisely so they are not dropped on the floor.
    let merged = preserve_deploy_time_keys(
        "",
        "[from_the_future]\nknob = 1\n\n[update]\nallow_edge_channel = true\nfuture_knob = 2\n",
    )
    .unwrap();

    let table: toml::Table = merged.toml.parse().unwrap();
    assert_eq!(table["from_the_future"]["knob"].as_integer(), Some(1));
    assert_eq!(table["update"]["future_knob"].as_integer(), Some(2));
    assert!(
        table["update"].get("allow_edge_channel").is_none(),
        "the gated key should still have been stripped: {}",
        merged.toml,
    );
}

#[test]
fn the_secret_store_table_is_replaced_wholesale() {
    let merged = preserve_deploy_time_keys(
        "[secret_store]\nprovider = \"file_system\"\npath = \"/var/lib/wardnet/secrets\"\n",
        "[secret_store]\nprovider = \"file_system\"\npath = \"/tmp/attacker\"\n",
    )
    .unwrap();

    let table: toml::Table = merged.toml.parse().unwrap();
    assert_eq!(
        table["secret_store"]["path"].as_str(),
        Some("/var/lib/wardnet/secrets"),
    );
    assert_eq!(merged.overridden, vec!["secret_store"]);
}

#[test]
fn a_non_table_where_a_section_belongs_does_not_stop_the_merge() {
    // The bundle is untrusted input; `update = "surprise"` must not be a
    // way to smuggle the section past the merge.
    let merged = preserve_deploy_time_keys(
        "[update]\nallow_edge_channel = false\n",
        "update = \"surprise\"\n",
    )
    .unwrap();

    let table: toml::Table = merged.toml.parse().unwrap();
    assert_eq!(table["update"]["allow_edge_channel"].as_bool(), Some(false));
}

#[test]
fn an_empty_live_config_strips_every_deploy_time_key() {
    let bundle = "[update]\nallow_edge_channel = true\nmanifest_base_url = \"https://attacker.example\"\n\n[test]\nstub_tunnel_backends = true\n";
    let merged = preserve_deploy_time_keys("", bundle).unwrap();

    let table: toml::Table = merged.toml.parse().unwrap();
    assert!(
        table.is_empty(),
        "expected nothing left, got {}",
        merged.toml
    );
}

#[test]
fn a_top_level_key_that_sorts_after_a_section_still_round_trips() {
    // TOML puts every bare key in the table it was last opened under, so a
    // top-level scalar emitted after a `[section]` header would silently be
    // re-read as part of that section. `pidfile_path` sorts after
    // `network`, which is exactly that trap.
    let merged = preserve_deploy_time_keys(
        "pidfile_path = \"/run/wardnetd/wardnetd.pid\"\n",
        "[network]\nlan_interface = \"eth0\"\n",
    )
    .unwrap();

    let table: toml::Table = merged.toml.parse().unwrap();
    assert_eq!(
        table["pidfile_path"].as_str(),
        Some("/run/wardnetd/wardnetd.pid"),
        "pidfile_path was swallowed by a section: {}",
        merged.toml,
    );
}

// ---------------------------------------------------------------------------
// Failure modes
// ---------------------------------------------------------------------------

#[test]
fn an_unparseable_bundle_config_is_rejected() {
    let err = preserve_deploy_time_keys("", "this is not toml").unwrap_err();
    assert!(matches!(err, ConfigRestoreError::BundleConfig(_)), "{err}");
}

#[test]
fn an_unparseable_live_config_is_rejected() {
    // Fail closed: with no live values to read, letting the bundle's copy
    // through would hand it the deploy-time keys.
    let err =
        preserve_deploy_time_keys("this is not toml", "[update]\nallow_edge_channel = true\n")
            .unwrap_err();
    assert!(matches!(err, ConfigRestoreError::LiveConfig(_)), "{err}");
}
