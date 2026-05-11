# Configuration

Wardnet reads its configuration from a single TOML file, by default
`/etc/wardnet/wardnet.toml`. The installer writes a minimal starter file
on first run; everything else is optional and falls back to sensible
defaults.

This page documents every supported section. Any section you leave out of
the file keeps its defaults.

```toml
# /etc/wardnet/wardnet.toml — minimal file written by the installer
[database]
path = "/var/lib/wardnet/wardnet.db"

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

## `[database]`

SQLite is the only supported provider today. The file path must be
writable by the `wardnet` user.

| Key | Default | Notes |
| --- | --- | --- |
| `provider` | `"sqlite"` | Only `sqlite` is supported. |
| `connection_string` | `"./wardnet.db"` | Installer overrides this to `/var/lib/wardnet/wardnet.db`. |

## `[logging]`

Structured logs are written in JSON to the rolling appender and streamed
live over the `/api/system/logs/stream` WebSocket.

| Key | Default | Notes |
| --- | --- | --- |
| `format` | `"console"` | `console` or `json`. Affects stderr only — file output is always JSON. |
| `level` | `"info"` | `trace`, `debug`, `info`, `warn`, or `error`. Overridden by `RUST_LOG` env var. |
| `filters` | `{}` | Per-crate level overrides: `{ sqlx = "warn" }`. |
| `path` | `"/var/log/wardnet/wardnetd.log"` | File appender destination. |
| `rotation` | `"daily"` | `hourly`, `daily`, or `never`. |
| `max_log_files` | `7` | Retention count for rotated files. |
| `max_recent_errors` | `15` | Ring buffer size for `/api/system/errors`. |
| `broadcast_capacity` | `256` | Buffer size for the live log WebSocket. |

## `[network]`

| Key | Default | Notes |
| --- | --- | --- |
| `lan_interface` | `"eth0"` | The physical interface Wardnet binds to for DHCP, ARP scanning, and routing. Set by the installer based on the interface you pick. |
| `default_policy` | `"direct"` | Default routing for newly-discovered devices: `direct` (bypass Wardnet tunnels) or a tunnel label. |

## `[auth]`

| Key | Default | Notes |
| --- | --- | --- |
| `session_expiry_hours` | `24` | Admin session cookie lifetime. |

## `[admin]` (optional)

Omit this section in production — the first-run setup wizard creates the
admin account interactively. Present only in the mock / dev environment
where the wizard is bypassed.

```toml
[admin]
username = "admin"
password = "…"
```

## `[secret_store]`

Where Wardnet keeps secret material — WireGuard private keys today,
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

Tunnel private keys are stored via `[secret_store]` (above) — they are
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
`wardnet_db_size_bytes`.

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

## Environment variable overrides

Two runtime overrides are honoured independent of the TOML:

- `RUST_LOG` — directly sets the tracing filter; wins over
  `logging.level` and `logging.filters`.
- `WARDNET_VERSION_OVERRIDE` — overrides the git-derived compile-time
  version string. Only useful for local testing of the auto-update flow
  (see the dev notes in the repository).
