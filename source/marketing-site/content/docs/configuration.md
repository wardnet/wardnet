# Configuration

Wardnet reads its configuration from a single TOML file, by default
`/etc/wardnet/wardnet.toml`. The installer writes a minimal starter file
on first run; everything else is optional and falls back to sensible
defaults.

This page documents every supported section. Any section you leave out of
the file keeps its defaults.

```toml
# /etc/wardnet/wardnet.toml, minimal file written by the installer
[database]
connection_string = "/var/lib/wardnet/wardnet.db"

[logging]
path = "/var/log/wardnet/wardnetd.log"
level = "info"

[network]
lan_interface = "eth0"

[secret_store]
provider = "file_system"
path = "/var/lib/wardnet/secrets"
```

Reload the daemon after editing:

```bash
sudo systemctl restart wardnetd
```

## `[server]`

HTTP API + embedded web UI bind settings.

| Key | Default | Notes |
| --- | --- | --- |
| `host` | `"0.0.0.0"` | Loopback-only binding? Set `"127.0.0.1"`. |
| `port` | `7411` | Port for the HTTP API and web UI. |
| `https_port` | `443` | Port for the daemon-terminated TLS listener. |
| `http_redirect_port` | `80` | Port that redirects plain HTTP to `https_port`. |

## `[database]`

SQLite is the only supported provider today. The file path must be
writable by the `wardnet` user.

| Key | Default | Notes |
| --- | --- | --- |
| `provider` | `"sqlite"` | Only `sqlite` is supported. |
| `connection_string` | `"/var/lib/wardnet/wardnet.db"` | Absolute path. Relative paths are resolved against the daemon's working directory. |

## `[logging]`

Structured logs are written in JSON to the rolling appender and streamed
live over the `/api/system/logs/stream` WebSocket.

| Key | Default | Notes |
| --- | --- | --- |
| `format` | `"console"` | `console` or `json`. Affects stderr only, file output is always JSON. |
| `level` | `"info"` | `trace`, `debug`, `info`, `warn`, or `error`. Overridden by `RUST_LOG` env var. |
| `filters` | `{}` | Per-crate level overrides: `{ sqlx = "warn" }`. |
| `path` | `"/var/log/wardnet/wardnetd.log"` | File appender destination. |
| `rotation` | `"daily"` | `hourly`, `daily`, or `never`. |
| `max_log_files` | `7` | Retention count for rotated files. |
| `max_recent_errors` | `15` | Ring buffer size for `/api/system/errors`. |
| `broadcast_capacity` | `256` | Buffer size for the live log WebSocket. |
| `ui_suppressed_targets` | `["hickory_resolver::recursor"]` | Tracing targets hidden from the admin UI (live log stream and `/api/system/errors`), matched as a target prefix. These events are still written to the log file — this only keeps per-query dependency noise from drowning the events an admin can act on. Set to `[]` to show everything. |

## `[network]`

| Key | Default | Notes |
| --- | --- | --- |
| `lan_interface` | `"eth0"` | The physical interface Wardnet binds to for DHCP, ARP scanning, and routing. Set by the installer based on the interface you pick. |
| `default_policy` | `"direct"` | Default routing for newly-discovered devices: `direct` (bypass Wardnet tunnels) or a tunnel label. |

## `[auth]`

| Key | Default | Notes |
| --- | --- | --- |
| `session_expiry_hours` | `24` | Admin session cookie lifetime. |
| `remember_me_expiry_hours` | `720` | Session lifetime when "remember me" is checked at login. |

## `[admin]` (optional)

Omit this section in production, the first-run setup wizard creates the
admin account interactively. Present only in the mock / dev environment
where the wizard is bypassed.

```toml
[admin]
username = "admin"
password = "…"
```

## `[secret_store]`

Where Wardnet keeps secret material, WireGuard private keys today,
backup passphrases and destination credentials in upcoming releases.
Anything that must never appear in the database, the API, or the logs
lives here.

The section is **optional**. Omit it entirely to run without a secret
store: the daemon still starts and serves DHCP, DNS, and device
detection, but tunnel creation and backup features refuse with
`"no secret store configured"` until you add a provider.

| Key | Default | Notes |
| --- | --- | --- |
| `provider` | _(required when section is present)_ | Storage backend. Only `file_system` is supported today. Future: `hashicorp_vault`, `azure_key_vault`, `aws_secrets_manager`. |

### `provider = "file_system"`

| Key | Default | Notes |
| --- | --- | --- |
| `path` | _(required)_ | Directory that holds secret files (mode 0700, owned by `wardnet`). Files inside are 0600. Must be writable by the daemon and on persistent (non-tmpfs) storage. |

```toml
[secret_store]
provider = "file_system"
path = "/var/lib/wardnet/secrets"
```

## `[tunnel]`

| Key | Default | Notes |
| --- | --- | --- |
| `idle_timeout_secs` | `600` | Tear down tunnels idle for this long. |
| `health_check_interval_secs` | `10` | How often to poll each tunnel for liveness. |
| `stats_interval_secs` | `5` | How often to pull bytes-tx/rx counters. |
| `latency_probe_interval_secs` | `60` | How often to re-measure tunnel latency for the latency chart. |
| `latency_probe_target` | `"1.1.1.1"` | Host pinged to measure tunnel latency. |
| `test_probe_url` | `"https://1.1.1.1/cdn-cgi/trace"` | URL used for the tunnel connectivity test probe. |
| `speed_test_url` | `"https://speed.cloudflare.com/__down?bytes=500000000"` | Download URL used by the tunnel speed test. Sized generously since each stream stops reading once `speed_test_measure_ms` elapses, not once the payload finishes. |
| `speed_test_latency_samples` | `5` | Number of samples averaged for the speed test's latency reading. |
| `speed_test_parallel_streams` | `4` | Concurrent download streams per throughput leg — avoids the single-TCP-flow bandwidth ceiling that understates tunnel throughput at higher RTT. |
| `speed_test_warmup_ms` | `1000` | Warm-up period (ms) discarded from each stream before bytes count toward the measurement, excluding connection setup and TCP slow-start. |
| `speed_test_measure_ms` | `4000` | Measurement window (ms), after warm-up, over which bytes are counted. Keep `speed_test_warmup_ms + speed_test_measure_ms` well under 15s — the daemon adds a fixed 15s connect/scheduling grace on top as a safety-net timeout, so a much larger window will make every stream time out. |

Tunnel private keys are stored via `[secret_store]` (above), they are
not configured here.

## `[detection]`

Passive + active device discovery settings.

| Key | Default | Notes |
| --- | --- | --- |
| `enabled` | `true` | Set `false` to disable passive packet capture + ARP scans. |
| `departure_timeout_secs` | `300` | Mark a device gone if not seen for this long. |
| `batch_flush_interval_secs` | `30` | How often to flush observation batches to disk. |
| `departure_scan_interval_secs` | `60` | How often to sweep for stale devices. |
| `arp_scan_interval_secs` | `60` | How often to broadcast an ARP discovery scan. |

## `[update]`

Auto-update subsystem. Runtime state (auto-update on/off, active channel)
lives in the database so admins can toggle it from the web UI without
editing the TOML. These are the deploy-time knobs only.

| Key | Default | Notes |
| --- | --- | --- |
| `manifest_base_url` | `"https://releases.wardnet.network"` | Server that hosts `<channel>.json`. Point at a mirror for air-gapped networks. |
| `check_interval_secs` | `21600` | Background poll cadence (±10% jitter). |
| `live_binary_path` | `"/usr/local/bin/wardnetd"` | Where the running daemon binary lives. Must be writable by the `wardnet` user. |
| `staging_dir` | `"/var/lib/wardnet/updates"` | Temporary directory for download + extraction. Must share a filesystem with `live_binary_path` for the swap to be atomic. |
| `require_signature` | `true` | Refuse to install a tarball without a valid minisign signature. Never set `false` in production. |
| `http_timeout_secs` | `60` | Per-request timeout for manifest + asset fetches. |
| `allow_edge_channel` | `false` | Permit this box to follow the **edge** channel. See below. |

### The edge channel

Edge builds are published straight from a branch by an on-demand workflow,
with no review, no release notes, and no test gates. They are signed with the
same production key — so the channel is authentic — but nothing promises the
code is good. Edge is an operator's testing loop, never a destination for a
real user.

Because of that, edge cannot be selected from the web UI alone. Set the flag,
restart, and the channel selector will offer it:

```toml
[update]
allow_edge_channel = true
```

Setting it requires root on the box, which is the point: an admin session — or
a stolen one — is not enough to opt a box into unvetted code.

### Getting a box off edge

Remove the flag and restart. The daemon logs a warning, falls back to `beta`,
and writes that back, so the stored channel can't contradict the config:

```toml
[update]
allow_edge_channel = false   # or delete the line
```

Do this **first**. Reinstalling alone is not enough: `install.sh` only chooses
which tarball to download, it does not change the channel the daemon has
stored, so a box still set to `edge` with the flag still on will simply
auto-update back to the newest edge build on its next check.

The updater also never downgrades, so dropping the flag does not by itself move
the box off the edge *binary*: `2026.07.00-edge.147` outranks
`2026.07.00-beta.6`, and the box will sit on it until a newer base version
ships. To go back immediately, re-run `install.sh` with `CHANNEL=beta` **after**
clearing the flag — the installer performs no version comparison, so it
installs whatever the manifest names, older or not.

## `[otel]`

OpenTelemetry export. Disabled by default.

| Key | Default | Notes |
| --- | --- | --- |
| `enabled` | `false` | Master switch for all OTel export. |
| `endpoint` | `"http://localhost:4317"` | OTLP gRPC endpoint. |
| `service_name` | `"wardnetd"` | Populated into the OTel resource. |
| `interval_secs` | `10` | Metric export cadence. |
| `traces.enabled` | `true` | Export tracing spans. |
| `logs.enabled` | `true` | Export structured logs. |
| `metrics.enabled` | `true` | Export metrics. |
| `metrics.enabled_metrics.*` | `true` | Per-metric toggles (see below). |

Per-metric toggles under `[otel.metrics.enabled_metrics]`:
`system_cpu_utilization`, `system_memory_usage`, `system_temperature`,
`system_network_io`, `wardnet_device_count`, `wardnet_tunnel_count`,
`wardnet_tunnel_active_count`, `wardnet_uptime_seconds`,
`wardnet_db_size_bytes`, `wardnet_disk_free_bytes`.

## `[vpn_providers]`

| Key | Default | Notes |
| --- | --- | --- |
| `enabled` | `{}` | Map of provider ID → bool. Providers not listed are enabled. Set `nordvpn = false` to disable one. |

## `[pyroscope]`

Continuous profiling agent. Disabled by default.

| Key | Default | Notes |
| --- | --- | --- |
| `enabled` | `false` | Master switch. |
| `endpoint` | `"http://localhost:4040"` | Pyroscope server URL. |

## `[mdns]`

| Key | Default | Notes |
| --- | --- | --- |
| `enabled` | `true` | Advertise the daemon over mDNS so `wardnet.local` resolves on the LAN. |
| `hostname` | _(none)_ | Override the advertised hostname. Defaults to the system hostname when unset. |

## `[health]`

Tuning for the internal `HealthMonitor` that backs the health-gated soft
watchdog restart.

| Key | Default | Notes |
| --- | --- | --- |
| `refresh_interval_secs` | `5` | How often each health check runs. |
| `failure_threshold` | `3` | Consecutive failures required before a check is considered unhealthy. |
| `check_timeout_secs` | `2` | Per-check timeout. |

## `[watchdog]`

Hardware watchdog integration. The soft (systemd) watchdog can be
disabled independently of the hard (kernel) watchdog; the hard watchdog
pet is never health-gated.

| Key | Default | Notes |
| --- | --- | --- |
| `enabled` | `true` | Master switch for hardware watchdog petting. |
| `device_path` | `"/dev/watchdog"` | Watchdog character device. |
| `hardware_timeout_secs` | `15` | Timeout configured on the hardware watchdog. |
| `pet_interval_secs` | `5` | How often the daemon pets the watchdog. |
| `soft_enabled` | `true` | Whether the pet is gated on `HealthMonitor` status. Set `false` to pet unconditionally. |

## Top-level keys

| Key | Default | Notes |
| --- | --- | --- |
| `pidfile_path` | `"/run/wardnetd/wardnetd.pid"` | Where the daemon writes its PID file. Not part of any `[section]`. |

## Environment variable overrides

Two runtime overrides are honoured independent of the TOML:

- `RUST_LOG`, directly sets the tracing filter; wins over
  `logging.level` and `logging.filters`.
- `WARDNET_VERSION_OVERRIDE`, overrides the git-derived compile-time
  version string. Only useful for local testing of the auto-update flow
  (see the dev notes in the repository).
