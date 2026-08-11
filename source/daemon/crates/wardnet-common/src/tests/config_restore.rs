//! Tests for [`crate::config_restore`].
//!
//! Two concerns live here. The merge tests pin the behaviour a restore
//! relies on: deploy-time keys come from the live machine, everything else
//! comes from the bundle. The classification test is the guard that keeps
//! the first set honest as the config grows — see
//! [`every_config_key_is_classified`].

use crate::config::{
    AdminConfig, AnomaliesConfig, ApplicationConfiguration, AuthConfig, DatabaseConfig,
    DdnsWardnetConfig, DetectionConfig, EnabledMetrics, HealthConfig, LoggingConfig, MdnsConfig,
    NetworkConfig, OtelConfig, OtelLogsConfig, OtelMetricsConfig, OtelTracesConfig,
    PyroscopeConfig, ServerConfig, TestConfig, TunnelConfig, UpdateConfig, VpnProvidersConfig,
    WatchdogConfig,
};
use crate::config_restore::{
    ConfigRestoreError, DEPLOY_TIME_ONLY_KEYS, check_bundle_config, preserve_deploy_time_keys,
};

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
    "anomalies.reevaluate_interval_secs",
    "anomalies.detect_timeout_secs",
    "anomalies.enabled",
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

/// The full set of config field paths, taken from **serde's own** field
/// lists rather than from a serialised value.
///
/// The distinction matters: `toml` drops an `Option` that is `None`, so
/// enumerating a sample config would silently omit any optional field left
/// unset — and five config fields are already `Option`. Asking serde what it
/// expects, via the `deny_unknown_fields` rejection message, sees every
/// declared field regardless of the value it would hold.
///
/// One entry per struct in the tree. A new *field* is picked up
/// automatically; a new nested *struct* shows up as an unclassified leaf
/// until it is either registered here or classified as a whole table.
fn declared_paths() -> Vec<String> {
    let mut paths = Vec::new();
    let mut add = |prefix: &str, names: Vec<String>| {
        for name in names {
            paths.push(if prefix.is_empty() {
                name
            } else {
                format!("{prefix}.{name}")
            });
        }
    };

    add("", declared_field_names::<ApplicationConfiguration>());
    add("server", declared_field_names::<ServerConfig>());
    add("database", declared_field_names::<DatabaseConfig>());
    add("logging", declared_field_names::<LoggingConfig>());
    add("network", declared_field_names::<NetworkConfig>());
    add("auth", declared_field_names::<AuthConfig>());
    add("admin", declared_field_names::<AdminConfig>());
    add("tunnel", declared_field_names::<TunnelConfig>());
    add("detection", declared_field_names::<DetectionConfig>());
    add("otel", declared_field_names::<OtelConfig>());
    add("otel.traces", declared_field_names::<OtelTracesConfig>());
    add("otel.logs", declared_field_names::<OtelLogsConfig>());
    add("otel.metrics", declared_field_names::<OtelMetricsConfig>());
    add(
        "otel.metrics.enabled_metrics",
        declared_field_names::<EnabledMetrics>(),
    );
    add(
        "vpn_providers",
        declared_field_names::<VpnProvidersConfig>(),
    );
    add("anomalies", declared_field_names::<AnomaliesConfig>());
    add("ddns_wardnet", declared_field_names::<DdnsWardnetConfig>());
    add("pyroscope", declared_field_names::<PyroscopeConfig>());
    add("update", declared_field_names::<UpdateConfig>());
    add("mdns", declared_field_names::<MdnsConfig>());
    add("health", declared_field_names::<HealthConfig>());
    add("watchdog", declared_field_names::<WatchdogConfig>());
    add("test", declared_field_names::<TestConfig>());
    // `secret_store` is an internally-tagged enum, which serde refuses to
    // combine with `deny_unknown_fields`, so it has no field list to read.
    // It is classified as a whole table, which covers every variant's fields
    // by construction — see DEPLOY_TIME_ONLY_KEYS.

    paths
}

/// Field names serde declares for `T`, read out of the `deny_unknown_fields`
/// rejection message.
///
/// Serde words the list three ways depending on how many fields there are —
/// ``expected `a` ``, ``expected `a` or `b` ``, ``expected one of `a`, `b`,
/// `c` `` — so this takes everything after `expected` and pulls out the
/// backtick-quoted names rather than matching any one phrasing.
///
/// A hack, but a self-checking one: if the message ever stops naming fields
/// this way the parse fails loudly here rather than quietly returning nothing
/// and letting [`every_config_key_is_classified`] pass on an empty set.
fn declared_field_names<T: serde::de::DeserializeOwned>() -> Vec<String> {
    const PROBE_KEY: &str = "wardnet_unknown_field_probe";

    let message = toml::from_str::<T>(&format!("{PROBE_KEY} = 0"))
        .err()
        .expect("a deny_unknown_fields struct must reject an unknown key")
        .to_string();
    // Everything before `expected` mentions the probe key itself, in
    // backticks — start after it so the probe cannot be read as a field.
    let listed = message
        .split_once("expected ")
        .unwrap_or_else(|| {
            panic!("serde's unknown-field message no longer lists fields: {message}")
        })
        .1;
    let names: Vec<String> = listed
        .split('`')
        .skip(1)
        .step_by(2)
        .map(ToOwned::to_owned)
        .collect();
    assert!(
        !names.is_empty() && !names.iter().any(String::is_empty),
        "no field names parsed out of: {message}",
    );
    names
}

/// Whether `key` names `path` itself or one of its ancestors.
fn covers(key: &str, path: &str) -> bool {
    path == key
        || path
            .strip_prefix(key)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// The declared paths that hold a value rather than a nested struct — the
/// granularity classification works at.
fn config_leaves() -> Vec<String> {
    let all = declared_paths();
    all.iter()
        .filter(|path| {
            let prefix = format!("{path}.");
            !all.iter().any(|other| other.starts_with(&prefix))
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Classification coverage
// ---------------------------------------------------------------------------

#[test]
fn every_config_key_is_classified() {
    let unclassified: Vec<String> = config_leaves()
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
    let leaves = config_leaves();
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
fn the_merge_keeps_the_file_minimal_rather_than_dumping_every_default() {
    // Round-tripping through `ApplicationConfiguration` would emit all ~80
    // fields; the operator wrote five lines and should get five back.
    let merged = preserve_deploy_time_keys(
        "[update]\nallow_edge_channel = true\n",
        "[logging]\nlevel = \"debug\"\n",
    )
    .unwrap();

    let table: toml::Table = merged.toml.parse().unwrap();
    assert_eq!(
        table.keys().collect::<Vec<_>>(),
        vec!["logging", "update"],
        "only the sections that were written should appear: {}",
        merged.toml,
    );
    assert_eq!(table["logging"]["level"].as_str(), Some("debug"));
    assert_eq!(table["update"]["allow_edge_channel"].as_bool(), Some(true));
}

#[test]
fn spelling_out_a_default_is_not_treated_as_a_change() {
    // The live file omits `allow_edge_channel`; the bundle writes it out at
    // the value omission already means. Nothing has changed, so this must not
    // re-serialise the file or log the bundle as trying to move the box.
    let bundle = "# operator notes\n[update]\nallow_edge_channel = false\n";
    let merged = preserve_deploy_time_keys("[logging]\nlevel = \"info\"\n", bundle).unwrap();

    assert!(
        merged.overridden.is_empty(),
        "explicit default reported as an override: {:?}",
        merged.overridden,
    );
    assert_eq!(merged.toml, bundle, "the comment should have survived");
}

#[test]
fn omitting_a_key_the_live_file_spells_out_is_not_a_change_either() {
    // The mirror image: the live file writes the default, the bundle omits
    // it. Both resolve to the same value, so the bundle passes through.
    let bundle = "[logging]\nlevel = \"debug\"\n";
    let merged = preserve_deploy_time_keys("[update]\nrequire_signature = true\n", bundle).unwrap();

    assert!(merged.overridden.is_empty(), "{:?}", merged.overridden);
    assert_eq!(merged.toml, bundle);
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
fn a_bundle_that_puts_a_scalar_where_a_section_belongs_is_rejected() {
    // `update = "surprise"` is valid TOML but not a config the daemon can
    // load, so it never reaches the merge at all.
    let err = preserve_deploy_time_keys(
        "[update]\nallow_edge_channel = false\n",
        "update = \"surprise\"\n",
    )
    .unwrap_err();
    assert!(matches!(err, ConfigRestoreError::BundleConfig(_)), "{err}");
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
    //
    // The live value is deliberately *not* the default, so the merge has a
    // real change to make and takes the re-serialising path where the hazard
    // lives.
    let merged = preserve_deploy_time_keys(
        "pidfile_path = \"/run/wardnetd/custom.pid\"\n",
        "[network]\nlan_interface = \"eth0\"\n",
    )
    .unwrap();

    assert_eq!(merged.overridden, vec!["pidfile_path"]);
    let table: toml::Table = merged.toml.parse().unwrap();
    assert_eq!(
        table["pidfile_path"].as_str(),
        Some("/run/wardnetd/custom.pid"),
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

#[test]
fn a_bundle_with_a_section_this_daemon_cannot_load_is_rejected() {
    // `ApplicationConfiguration` is deny_unknown_fields and a restore is
    // followed by a mandatory restart, so writing this would leave the box
    // in a systemd restart loop with no UI left to fix it from.
    let err = preserve_deploy_time_keys("", "[from_the_future]\nknob = 1\n").unwrap_err();
    assert!(matches!(err, ConfigRestoreError::BundleConfig(_)), "{err}");

    // Same for a known section that grew a key we do not have.
    let err = preserve_deploy_time_keys("", "[update]\nfuture_knob = 2\n").unwrap_err();
    assert!(matches!(err, ConfigRestoreError::BundleConfig(_)), "{err}");
}

#[test]
fn a_bundle_with_a_wrongly_typed_value_is_rejected() {
    let err = preserve_deploy_time_keys("", "[server]\nport = \"not a number\"\n").unwrap_err();
    assert!(matches!(err, ConfigRestoreError::BundleConfig(_)), "{err}");
}

#[test]
fn a_bundle_config_that_is_not_utf8_is_rejected() {
    let err = check_bundle_config(&[0xFF, 0xFE, 0xFD]).unwrap_err();
    assert!(matches!(err, ConfigRestoreError::BundleNotUtf8(_)), "{err}");
    assert!(err.blames_the_bundle());
}

#[test]
fn a_loadable_bundle_config_passes_the_check() {
    let text = check_bundle_config(b"[logging]\nlevel = \"debug\"\n").unwrap();
    assert_eq!(text, "[logging]\nlevel = \"debug\"\n");
}

#[test]
fn a_broken_live_config_is_not_blamed_on_the_bundle() {
    // The caller maps this to an internal error rather than a rejected
    // upload, because the machine is what is wrong, not the bundle.
    let err =
        preserve_deploy_time_keys("this is not toml", "[update]\nallow_edge_channel = true\n")
            .unwrap_err();
    assert!(!err.blames_the_bundle(), "{err}");
}
