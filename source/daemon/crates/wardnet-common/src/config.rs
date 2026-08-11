use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Top-level application configuration.
///
/// Loaded from a TOML file by the daemon, or constructed with defaults
/// by the mock server. All sub-crates receive this via dependency injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApplicationConfiguration {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
    pub network: NetworkConfig,
    pub auth: AuthConfig,
    pub admin: Option<AdminConfig>,
    pub tunnel: TunnelConfig,
    pub detection: DetectionConfig,
    pub otel: OtelConfig,
    pub vpn_providers: VpnProvidersConfig,
    /// Overrides for the wardnet-cloud gateway URLs the DDNS wardnet provider
    /// talks to. See [`DdnsWardnetConfig`].
    pub ddns_wardnet: DdnsWardnetConfig,
    pub pyroscope: PyroscopeConfig,
    pub update: UpdateConfig,
    pub mdns: MdnsConfig,
    /// Health-monitor settings: how often the `HealthMonitor` refreshes its
    /// snapshot, how many consecutive failures debounce a component to DOWN,
    /// and the per-check timeout. See issue #214.
    pub health: HealthConfig,
    /// Watchdog settings: the hardware `/dev/watchdog` device and pet cadence
    /// plus the health-gated soft (`sd_notify`) restart toggle. See issue #214.
    pub watchdog: WatchdogConfig,
    /// Test-only backend overrides. **Never set in production.**
    ///
    /// Off by default and not written by `install.sh`; the end-to-end
    /// compose stack sets `[test] stub_tunnel_backends = true` so the tunnel
    /// speed-test / tunnel-test path returns deterministic numbers with no
    /// real `WireGuard` interface or internet egress. See [`TestConfig`].
    pub test: TestConfig,
    /// Secret-store configuration. **Optional.**
    ///
    /// When absent, no local secret storage is available: tunnels that
    /// require a `WireGuard` private key and backup features that require
    /// stored credentials will refuse to operate. Device detection, DHCP,
    /// DNS, and read-only admin endpoints still work.
    ///
    /// Future external providers (`HashiCorp` Vault, Azure Key Vault, AWS
    /// Secrets Manager) will plug in as additional variants of
    /// [`SecretStoreConfig`] behind the same `SecretStore` trait.
    pub secret_store: Option<SecretStoreConfig>,
    /// Path to the PID file written on startup and removed on clean exit.
    ///
    /// The daemon writes its process ID to this file immediately after
    /// binding its listen socket. Operators and tooling can use
    /// `kill -TERM $(cat /run/wardnetd/wardnetd.pid)` to trigger a graceful
    /// shutdown without relying on service-manager process tracking.
    /// The default lives under `/run/wardnetd/` because the systemd unit
    /// runs as User=wardnet and that directory is created (and owned) by
    /// systemd's `RuntimeDirectory=wardnetd` setting; the bare `/run`
    /// tmpfs is root-owned and not writable by the daemon.
    #[serde(default = "default_pidfile_path")]
    pub pidfile_path: PathBuf,
}

impl Default for ApplicationConfiguration {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
            logging: LoggingConfig::default(),
            network: NetworkConfig::default(),
            auth: AuthConfig::default(),
            admin: None,
            tunnel: TunnelConfig::default(),
            detection: DetectionConfig::default(),
            otel: OtelConfig::default(),
            vpn_providers: VpnProvidersConfig::default(),
            ddns_wardnet: DdnsWardnetConfig::default(),
            pyroscope: PyroscopeConfig::default(),
            update: UpdateConfig::default(),
            mdns: MdnsConfig::default(),
            health: HealthConfig::default(),
            watchdog: WatchdogConfig::default(),
            test: TestConfig::default(),
            secret_store: None,
            pidfile_path: default_pidfile_path(),
        }
    }
}

fn default_pidfile_path() -> PathBuf {
    PathBuf::from("/run/wardnetd/wardnetd.pid")
}

impl ApplicationConfiguration {
    /// Load configuration from the given TOML file path. If the file does not
    /// exist, returns default configuration.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            tracing::info!(?path, "config file not found, using defaults");
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        tracing::info!(?path, "loaded configuration");
        Ok(config)
    }

    /// Check whether a VPN provider is enabled. Returns `true` unless the
    /// provider is explicitly set to `false` in the `[vpn_providers.enabled]` table.
    #[must_use]
    pub fn is_vpn_provider_enabled(&self, id: &str) -> bool {
        self.vpn_providers.enabled.get(id).copied().unwrap_or(true)
    }
}

/// HTTP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub host: String,
    /// Plain-HTTP port — the pre-provisioning fallback surface and the LAN admin
    /// API before a real cert is loaded. Never guarded.
    pub port: u16,
    /// HTTPS port the daemon terminates TLS on. Always bound (with a placeholder
    /// self-signed cert until a real one is issued); requests are 503-guarded
    /// until TLS is provisioned.
    pub https_port: u16,
    /// Plain-HTTP port that 308-redirects to HTTPS.
    pub http_redirect_port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_owned(),
            port: 7411,
            https_port: 443,
            http_redirect_port: 80,
        }
    }
}

/// Supported database providers.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseProvider {
    #[default]
    Sqlite,
}

/// Database configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DatabaseConfig {
    /// Database provider. Only `sqlite` is supported for now.
    pub provider: DatabaseProvider,
    /// Connection string. For `SQLite` this is the file path.
    ///
    /// Defaults to the absolute `/var/lib/wardnet/wardnet.db` (matching the
    /// systemd `ReadWritePaths=/var/lib/wardnet` and what `deploy/install.sh`
    /// writes) so a zero-config daemon under `WorkingDirectory=/` doesn't try
    /// to create `/wardnet.db`. Explicit relative paths are still honoured and
    /// resolved against the working directory via [`Self::to_file_path`].
    pub connection_string: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            provider: DatabaseProvider::Sqlite,
            connection_string: "/var/lib/wardnet/wardnet.db".to_owned(),
        }
    }
}

impl DatabaseConfig {
    /// Return the connection string as an absolute [`PathBuf`].
    ///
    /// Resolves relative paths against the process working directory so that
    /// callers like the backup service and the disk-space probe get a path
    /// that can be matched against absolute mount points. Returns `None` for
    /// the `:memory:` sentinel (in-memory `SQLite`, no file on disk).
    #[must_use]
    pub fn to_file_path(&self) -> Option<std::path::PathBuf> {
        let s = &self.connection_string;
        if s == ":memory:" {
            return None;
        }
        let p = std::path::Path::new(s);
        if p.is_absolute() {
            Some(p.to_path_buf())
        } else {
            std::env::current_dir().ok().map(|cwd| cwd.join(p))
        }
    }
}

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    Console,
    Json,
}

/// Log file rotation frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogRotation {
    /// Rotate log files every hour.
    Hourly,
    /// Rotate log files every day (default).
    Daily,
    /// Never rotate — single log file.
    Never,
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    /// Log output format (console or json).
    pub format: LogFormat,
    /// Default log level for wardnet crates.
    pub level: String,
    /// Per-crate log level overrides.
    pub filters: std::collections::HashMap<String, String>,
    /// Path to the log file.
    pub path: PathBuf,
    /// How often to rotate the log file.
    pub rotation: LogRotation,
    /// Maximum number of rotated log files to keep.
    pub max_log_files: usize,
    /// Maximum number of recent diagnostics kept in the ring buffer that backs
    /// the dashboard's recent-errors panel.
    pub max_recent_errors: usize,
    /// Channel capacity for the WebSocket log broadcast.
    pub broadcast_capacity: usize,
    /// Tracing targets hidden from the admin-facing live-log stream (the
    /// WebSocket log view).
    ///
    /// Matched as a prefix against the event target, so
    /// `hickory_resolver::recursor` also covers
    /// `hickory_resolver::recursor::handle`. Entries here are **not** filtered
    /// out of the log file or the `OTel` exporters — the full detail stays on
    /// disk for debugging; this only keeps the admin log view free of events an
    /// admin cannot act on.
    pub ui_suppressed_targets: Vec<String>,
    /// Tracing targets kept out of the WARN-and-above slice mirrored to stderr
    /// (and therefore out of journald / `systemctl status`).
    ///
    /// Matched as a prefix, exactly like [`Self::ui_suppressed_targets`], and
    /// likewise **not** filtered from the log file or the `OTel` exporters —
    /// the full detail stays on disk.
    ///
    /// This list exists because WARN is not a reliable "an operator should look
    /// at this" signal in practice. Measured over one day on a live gateway:
    /// 31,860 WARN+ events, of which 31,040 (97%) were
    /// `hickory_resolver::recursor` logging one warning per failed recursive
    /// lookup — an ordinary negative DNS answer — and a further 579 were the
    /// same event surfacing through our own resolution pipeline. Mirroring
    /// those verbatim would push ~30k lines/day into the journal, evicting
    /// other units' logs and burying the ~30 genuinely actionable events
    /// (routing/route-monitor/sqlx) they are supposed to make visible.
    pub journal_suppressed_targets: Vec<String>,
    /// Tracing targets whose INFO events are mirrored to stderr (and therefore
    /// into journald) alongside the WARN-and-above slice.
    ///
    /// Matched as a prefix, exactly like [`Self::journal_suppressed_targets`],
    /// which takes precedence when a target appears in both.
    ///
    /// The journal slice is otherwise WARN-and-above, on the reasoning that
    /// anything an operator has to act on is at least a warning. Periodic
    /// housekeeping breaks that assumption: a run that reclaims nothing and a
    /// run that never happened both emit no warning, so a healthy-looking
    /// journal is exactly what a stalled maintenance job produces. The only
    /// evidence that separates the two is the successful-run record — an INFO
    /// event by every other measure.
    ///
    /// Membership is therefore reserved for targets that emit a bounded,
    /// countable number of INFO events per day. The two here run once per
    /// calendar day and log a single summary line each; anything per-request or
    /// per-query belongs in the log file, not the journal.
    pub journal_info_targets: Vec<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            format: LogFormat::Console,
            level: "info".to_owned(),
            filters: std::collections::HashMap::new(),
            path: PathBuf::from("/var/log/wardnet/wardnetd.log"),
            rotation: LogRotation::Daily,
            max_log_files: 7,
            max_recent_errors: 15,
            broadcast_capacity: 256,
            // Warns once per failed recursive lookup ("lookup error: no records
            // found ..."). On a busy resolver that is one warning per client
            // query for an ordinary negative DNS answer — it would drown the
            // live log view. Unlike the netlink noise silenced in
            // `to_filter_string`, this is worth keeping on disk: it names the
            // query that failed, so it is genuinely useful when debugging
            // resolution.
            ui_suppressed_targets: vec!["hickory_resolver::recursor".to_owned()],
            // `hickory_resolver` covers the recursor's per-lookup warnings
            // (97% of all WARN+ traffic on a live gateway).
            // `wardnetd::dns::pipeline` is the same failure re-reported by our
            // own resolver — a per-query event, not an operator signal. Both
            // stay in the log file; `sqlx` is deliberately *not* suppressed,
            // since slow-statement and slow-acquire warnings are exactly the
            // kind of thing worth seeing in `systemctl status`.
            journal_suppressed_targets: vec![
                "hickory_resolver".to_owned(),
                "wardnetd::dns::pipeline".to_owned(),
            ],
            // The daily database maintenance sequence (incremental vacuum, WAL
            // checkpoint, `PRAGMA optimize`) and the daily query-log retention
            // pass. Four INFO lines per day between them, and they are the only
            // record that the housekeeping which keeps the database from
            // growing without bound actually ran.
            journal_info_targets: vec![
                "wardnetd_services::db_maintenance_runner".to_owned(),
                "wardnetd_services::dns::query_log_runner".to_owned(),
            ],
        }
    }
}

/// Whether `target` starts with any entry in `prefixes`.
///
/// Empty entries are skipped: an empty prefix matches every target, so a
/// stray `""` in either list would turn a targeted rule into a blanket one —
/// never what a config typo means to say.
fn has_prefix(target: &str, prefixes: &[String]) -> bool {
    prefixes
        .iter()
        .any(|p| !p.is_empty() && target.starts_with(p.as_str()))
}

impl LoggingConfig {
    /// Whether an event at `level` from `target` belongs in the slice mirrored
    /// to stderr, and so into journald.
    ///
    /// Both lists hold target **prefixes**, so `hickory_resolver` also covers
    /// `hickory_resolver::recursor::handle`:
    ///
    /// * `suppressed` (normally [`Self::journal_suppressed_targets`]) drops
    ///   WARN events that would otherwise pass. ERROR always passes, whatever
    ///   the target.
    /// * `info_targets` (normally [`Self::journal_info_targets`]) raises INFO
    ///   events that would otherwise be dropped, for the handful of daily
    ///   housekeeping summaries whose *absence* is the thing worth noticing.
    ///
    /// `suppressed` wins when a target appears in both — a deny-list entry is
    /// the more specific statement of intent, and the alternative silently
    /// re-admits exactly what an operator asked to be rid of.
    ///
    /// Level alone is not the filter in either direction, because WARN is not a
    /// dependable "an operator should look at this" signal here — see
    /// [`Self::journal_suppressed_targets`] for the measured breakdown.
    ///
    /// Lives here rather than next to the subscriber setup in `wardnetd`: that
    /// crate is Linux-only (it links netlink), so a predicate defined there
    /// cannot be unit-tested on a developer machine.
    #[must_use]
    pub fn journal_allows(
        level: tracing::Level,
        target: &str,
        suppressed: &[String],
        info_targets: &[String],
    ) -> bool {
        // `tracing`'s `Level` ordering runs ERROR < WARN < INFO < DEBUG <
        // TRACE, so `<=` reads as "at least this severe".
        //
        // ERROR is unconditional. The suppression list targets crates that log
        // one WARN per failed DNS lookup; an ERROR from those same crates is a
        // different animal and rare enough to be worth seeing — across seven
        // days of production logs the daemon emitted no ERROR events at all,
        // so nothing here can become a flood.
        if level <= tracing::Level::ERROR {
            return true;
        }
        if level > tracing::Level::INFO {
            return false;
        }
        if has_prefix(target, suppressed) {
            return false;
        }
        level <= tracing::Level::WARN || has_prefix(target, info_targets)
    }

    /// Build an `EnvFilter`-compatible directive string from this config.
    #[must_use]
    pub fn to_filter_string(&self) -> String {
        use std::fmt::Write;

        // Two mechanisms, deliberately: an `EnvFilter` directive here gates the
        // *whole* subscriber (file and `OTel` included), while
        // `ui_suppressed_targets` hides a target from the admin UI only.
        //
        // Use this one when the events are worthless to everybody, and
        // `ui_suppressed_targets` when they are worth keeping on disk but would
        // drown the admin. `netlink_packet_route::link::buffer_tool` is the
        // former: it warns whenever the kernel returns more bytes for an
        // attribute than the crate version knows about (e.g. `IFLA_INET6_STATS`:
        // expecting 288, got 304), which newer kernels trip on every link poll.
        // Nobody — admin or developer — can act on it, and letting it reach the
        // file would churn the rotation window. A `[logging.filters]` entry can
        // still raise it again because it is appended after this directive.
        let mut directives = format!(
            "warn,wardnetd={level},wardnet_common={level},netlink_packet_route::link::buffer_tool=error",
            level = self.level,
        );

        for (crate_name, crate_level) in &self.filters {
            let _ = write!(directives, ",{crate_name}={crate_level}");
        }

        directives
    }
}

/// Network / LAN configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkConfig {
    pub lan_interface: String,
    pub default_policy: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            lan_interface: "eth0".to_owned(),
            default_policy: "direct".to_owned(),
        }
    }
}

/// Authentication settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuthConfig {
    pub session_expiry_hours: u64,
    /// Session lifetime when `remember_me = true` (default 30 days = 720 h).
    pub remember_me_expiry_hours: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            session_expiry_hours: 24,
            remember_me_expiry_hours: 720,
        }
    }
}

/// Initial admin account credentials.
///
/// Optional in the TOML file. When present, `bootstrap_admin` uses these
/// instead of generating random credentials.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminConfig {
    pub username: String,
    pub password: String,
}

// Redact `password` so a startup-time `?config` trace line can't leak
// the bootstrap admin password into the log file.
impl std::fmt::Debug for AdminConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdminConfig")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// `WireGuard` tunnel management settings.
///
/// Note: private-key storage is not configured here — it lives under the
/// top-level [`SecretStoreConfig`]. Tunnel creation refuses to operate
/// when no secret store is configured.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TunnelConfig {
    pub idle_timeout_secs: u64,
    pub health_check_interval_secs: u64,
    pub stats_interval_secs: u64,
    /// How often the per-tunnel ICMP latency probe runs. Defaults to 60s
    /// so each active tunnel produces one `tunnel.latency.rtt_ms` gauge
    /// sample per minute, lining up with the stats pipeline's minute
    /// bucket without spamming the upstream.
    pub latency_probe_interval_secs: u64,
    /// IP address the latency prober pings through each tunnel. Defaults
    /// to `1.1.1.1` (Cloudflare) — a stable, low-latency public anycast
    /// endpoint that responds to ICMP echo.
    pub latency_probe_target: String,
    /// URL probed by the tunnel test endpoint to determine the exit IP and
    /// country. The default is Cloudflare's `cdn-cgi/trace`, which returns a
    /// `key=value` document including `ip=` and `loc=`. Overriding this
    /// requires the same response shape.
    pub test_probe_url: String,
    /// URL the speed test downloads from to measure throughput. The default
    /// is Cloudflare's `__down` endpoint requesting a 50 MB payload. That
    /// endpoint only serves `bytes` values under 100 MB — at or above the cap
    /// it answers `403` with no body, which fails every stream — so the
    /// default deliberately sits well below it, leaving headroom in case the
    /// cap tightens. A stream that drains the payload before the measure
    /// window closes simply requests it again (see the throughput tester), so
    /// size only trades re-request overhead against the cap, never accuracy.
    /// The download runs twice per speed test — once unbound (direct/WAN) and
    /// once bound to the tunnel interface (`SO_BINDTODEVICE`) — so the
    /// endpoint must serve the requested byte count over plain HTTPS.
    pub speed_test_url: String,
    /// Number of ICMP echo samples taken per leg of a speed test to derive
    /// median latency and jitter. Defaults to 5.
    pub speed_test_latency_samples: u32,
    /// Number of concurrent download streams per throughput leg. A single
    /// TCP stream's throughput is capped by its bandwidth-delay product, so
    /// one flow understates available bandwidth on higher-RTT paths (e.g.
    /// through a tunnel) more than on a low-RTT direct path — running
    /// several streams in parallel avoids that single-flow ceiling. Defaults
    /// to 4.
    pub speed_test_parallel_streams: u32,
    /// Warm-up period (milliseconds) discarded from the start of each
    /// stream's download before bytes count toward the throughput
    /// calculation. Excludes connection setup and TCP slow-start from the
    /// measurement. Defaults to 1000 (1s).
    pub speed_test_warmup_ms: u64,
    /// Duration (milliseconds), after the warm-up period, over which bytes
    /// are counted toward the throughput calculation. Defaults to 4000
    /// (4s) — long enough to average out short ISP burst-allowance windows,
    /// short enough to keep total speed-test job time reasonable.
    pub speed_test_measure_ms: u64,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            idle_timeout_secs: 600,
            health_check_interval_secs: 10,
            stats_interval_secs: 5,
            latency_probe_interval_secs: 60,
            latency_probe_target: "1.1.1.1".to_owned(),
            test_probe_url: "https://1.1.1.1/cdn-cgi/trace".to_owned(),
            speed_test_url: "https://speed.cloudflare.com/__down?bytes=50000000".to_owned(),
            speed_test_latency_samples: 5,
            speed_test_parallel_streams: 4,
            speed_test_warmup_ms: 1000,
            speed_test_measure_ms: 4000,
        }
    }
}

/// Secret-store provider configuration.
///
/// The `provider` discriminator in TOML selects the storage backend;
/// each variant carries the fields specific to that backend. Today only
/// `file_system` is shipped — future variants (`hashicorp_vault`,
/// `azure_key_vault`, `aws_secrets_manager`, etc.) plug in behind the
/// same `SecretStore` trait without changing the wire format.
///
/// ```toml
/// [secret_store]
/// provider = "file_system"
/// path = "/var/lib/wardnet/secrets"
/// ```
// NOTE: no `deny_unknown_fields` here — serde rejects it on internally-tagged
// enums (`tag = "provider"`) at compile time. Unknown keys *inside* a variant
// (e.g. a stray field under `[secret_store]`) are therefore not caught; unknown
// top-level sections still are, via `ApplicationConfiguration`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum SecretStoreConfig {
    /// Local-filesystem-backed secret store. Each secret is written as a
    /// 0600-mode file rooted at `path`, namespaced by subdirectory
    /// (`wireguard/`, `backup/`, `destinations/`, etc.). The path must be
    /// writable by the `wardnet` user and should live on persistent
    /// (non-tmpfs) storage.
    FileSystem { path: PathBuf },
}

/// Device detection settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DetectionConfig {
    pub enabled: bool,
    pub departure_timeout_secs: u64,
    pub batch_flush_interval_secs: u64,
    pub departure_scan_interval_secs: u64,
    pub arp_scan_interval_secs: u64,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            departure_timeout_secs: 300,
            batch_flush_interval_secs: 30,
            departure_scan_interval_secs: 60,
            arp_scan_interval_secs: 60,
        }
    }
}

/// OpenTelemetry OTLP export configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OtelConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub service_name: String,
    pub interval_secs: u64,
    pub traces: OtelTracesConfig,
    pub logs: OtelLogsConfig,
    pub metrics: OtelMetricsConfig,
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://localhost:4317".to_owned(),
            service_name: "wardnetd".to_owned(),
            interval_secs: 10,
            traces: OtelTracesConfig::default(),
            logs: OtelLogsConfig::default(),
            metrics: OtelMetricsConfig::default(),
        }
    }
}

/// `OTel` trace export settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OtelTracesConfig {
    pub enabled: bool,
}

impl Default for OtelTracesConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// `OTel` log export settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OtelLogsConfig {
    pub enabled: bool,
}

impl Default for OtelLogsConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// VPN provider enable/disable overrides.
///
/// By default all registered providers are enabled. To disable a provider,
/// set its ID to `false`:
///
/// ```toml
/// [vpn_providers.enabled]
/// nordvpn = false
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VpnProvidersConfig {
    /// Map of provider ID to enabled flag. Providers not listed here are
    /// treated as enabled.
    pub enabled: std::collections::HashMap<String, bool>,
    /// Override for the `NordVPN` API base URL. Unset in production (the
    /// provider talks to `https://api.nordvpn.com`); the end-to-end test
    /// harness points this at the `nordvpn_mock` container so the daemon
    /// never reaches the real API. See issue #248 (E2E Stage 10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nordvpn_api_url: Option<String>,
}

/// Overrides for the wardnet-cloud `tenants`/`ddns` gateway URLs the DDNS
/// service's wardnet provider talks to (enroll, availability, network
/// registration, IP/ACME-challenge publishing, and the region health probe).
///
/// Unset in production (the daemon talks to the real `api.wardnet.network`
/// global gateway and the built-in region catalog); the end-to-end test
/// harness points these at the `wardnet_cloud_mock` container so
/// admin-app/user-app's premium entitlement gate can be exercised offline.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DdnsWardnetConfig {
    /// Override for the global gateway base URL (fronts `tenants`: enroll /
    /// token / availability / networks under prefix-free `/v1/…`, cloud
    /// ADR-0015).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_url: Option<String>,
    /// Override for the single built-in region's gateway base URL (fronts
    /// that region's `ddns` service, routing on `X-Mesh-Target` rather than a
    /// path prefix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_gateway_url: Option<String>,
    /// Override for the single built-in region's health-probe URL, checked by
    /// `register_network` before registering against it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_health_url: Option<String>,
}

/// OpenTelemetry metrics collection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OtelMetricsConfig {
    pub enabled: bool,
    pub enabled_metrics: EnabledMetrics,
}

impl Default for OtelMetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enabled_metrics: EnabledMetrics::default(),
        }
    }
}

/// Per-metric enable/disable toggles for the metrics collector.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EnabledMetrics {
    pub system_cpu_utilization: bool,
    pub system_memory_usage: bool,
    pub system_temperature: bool,
    pub system_network_io: bool,
    pub wardnet_device_count: bool,
    pub wardnet_tunnel_count: bool,
    pub wardnet_tunnel_active_count: bool,
    pub wardnet_uptime_seconds: bool,
    pub wardnet_db_size_bytes: bool,
    pub wardnet_disk_free_bytes: bool,
}

impl Default for EnabledMetrics {
    fn default() -> Self {
        Self {
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
        }
    }
}

/// Auto-update subsystem configuration.
///
/// Runtime behaviour (auto-update on/off, channel) lives in `system_config`
/// so admins can toggle it from the UI without editing the TOML. The values
/// here are the deploy-time knobs: where to fetch releases from, how often
/// to check, and the binary layout paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateConfig {
    /// HTTPS base URL for the release manifest server.
    ///
    /// The runner fetches `<manifest_base_url>/<channel>.json`. The default
    /// points at `wardnet.network`, which is the authenticity anchor: TLS
    /// protects the fetch, the embedded signing key protects the payload.
    pub manifest_base_url: String,
    /// Background check interval in seconds. Jittered by ±10% at runtime.
    pub check_interval_secs: u64,
    /// Absolute path to the currently-executing binary. Auto-detected from
    /// `/proc/self/exe` on startup when left at the default sentinel.
    pub live_binary_path: PathBuf,
    /// Directory used to stage downloads and extracted binaries. Must be
    /// writable by the daemon user and on the same filesystem as the live
    /// binary for atomic rename.
    pub staging_dir: PathBuf,
    /// Require a valid minisign signature before swapping the binary.
    /// Production builds must set this to `true`.
    pub require_signature: bool,
    /// HTTP request timeout for manifest/asset fetches, in seconds.
    pub http_timeout_secs: u64,
    /// Allow this box to follow the `edge` channel — unreviewed, ungated
    /// builds published straight from a branch (ADR-0023).
    ///
    /// Deliberately deploy-time, not an admin toggle: putting a box on edge
    /// should require root *on that box*, so an admin session (or a stolen
    /// one) cannot opt it into unvetted code. Default `false`. If this is
    /// turned off on a box already following edge, the daemon logs a warning
    /// at startup and falls back to `beta`.
    pub allow_edge_channel: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            manifest_base_url: "https://releases.wardnet.network".to_owned(),
            check_interval_secs: 6 * 60 * 60,
            live_binary_path: PathBuf::from("/usr/local/bin/wardnetd"),
            staging_dir: PathBuf::from("/var/lib/wardnet/updates"),
            require_signature: true,
            http_timeout_secs: 60,
            allow_edge_channel: false,
        }
    }
}

/// mDNS advertisement configuration.
///
/// On startup, the daemon advertises an `_http._tcp.local.` service
/// record so users can reach the setup wizard at `http://wardnet.local`
/// without knowing the LAN IP. Disable via `enabled = false` if the
/// LAN already has another mDNS responder owning the name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MdnsConfig {
    /// When `false`, the daemon does not start the mDNS advertiser.
    pub enabled: bool,
    /// Hostname to advertise (without the `.local.` suffix).
    ///
    /// `None` means use the built-in default (`"wardnet"`). On detected
    /// collision with another responder, the advertiser falls back to
    /// `<hostname>-2`, `<hostname>-3`, … in memory only — no persistence.
    pub hostname: Option<String>,
}

impl Default for MdnsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            hostname: None,
        }
    }
}

/// Health-monitor configuration (issue #214).
///
/// The `HealthMonitor` runs every registered `HealthCheck` on each refresh
/// tick, debounces failures, and produces an overall `HealthStatus` that the
/// health-gated soft watchdog consults before petting systemd's watchdog.
/// All three fields are `serde` defaults so an existing `wardnet.toml` needs
/// no `[health]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HealthConfig {
    /// How often the monitor re-runs every check, in seconds.
    pub refresh_interval_secs: u64,
    /// Consecutive failed checks required before a component flips to DOWN.
    /// Recovery is immediate (a single success clears the streak), so this
    /// only debounces *into* the DOWN state — it never delays recovery.
    pub failure_threshold: u32,
    /// Per-check timeout, in seconds. A `check()` that exceeds it is recorded
    /// as `Down { detail: "timeout" }` so a hung probe can never stall the
    /// whole refresh cycle.
    pub check_timeout_secs: u64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: 5,
            failure_threshold: 3,
            check_timeout_secs: 2,
        }
    }
}

/// Watchdog configuration (issue #214).
///
/// Covers both the hardware `/dev/watchdog` (ungated kernel-reboot backstop)
/// and the health-gated soft restart driven by `sd_notify(WATCHDOG=1)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WatchdogConfig {
    /// Master switch for the hardware `/dev/watchdog` runner. When `false`
    /// the daemon never opens the device (e.g. boards without a watchdog).
    pub enabled: bool,
    /// Path to the hardware watchdog character device.
    pub device_path: PathBuf,
    /// Hardware timeout programmed into the device, in seconds. The kernel
    /// reboots the host if the device isn't pet within this window.
    pub hardware_timeout_secs: u64,
    /// How often the hardware runner pets the device, in seconds. Must be
    /// comfortably below `hardware_timeout_secs`.
    pub pet_interval_secs: u64,
    /// When `true`, the soft watchdog sends `sd_notify(WATCHDOG=1)` only
    /// while overall health is UP and the snapshot is fresh. Independent of
    /// `enabled` — the soft path works even on boards without `/dev/watchdog`.
    pub soft_enabled: bool,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            device_path: PathBuf::from("/dev/watchdog"),
            hardware_timeout_secs: 15,
            pet_interval_secs: 5,
            soft_enabled: true,
        }
    }
}

/// Test-only backend overrides.
///
/// The daemon that the end-to-end suite runs is the real production binary,
/// but the compose stack has no live `WireGuard` tunnel and no guaranteed
/// internet egress, so the tunnel speed-test / tunnel-test measurement path
/// has nothing real to measure. Enabling this swaps the `WireGuard` interface,
/// throughput tester, latency prober and exit probe for deterministic stubs
/// that return fixed numbers with no kernel or network I/O.
///
/// **Never enable in production** — `install.sh` never writes this section,
/// and every field defaults to off, so a normal deployment behaves exactly
/// as before.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TestConfig {
    /// When `true`, wire deterministic stub tunnel backends instead of the
    /// real `WireGuard` / HTTP / ICMP implementations. The speed test and
    /// tunnel test then report fabricated results. Defaults to `false`.
    pub stub_tunnel_backends: bool,
}

/// Pyroscope continuous profiling agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PyroscopeConfig {
    pub enabled: bool,
    pub endpoint: String,
}

impl Default for PyroscopeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://localhost:4040".to_owned(),
        }
    }
}
