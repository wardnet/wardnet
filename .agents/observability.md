# Observability

## Tracing spans

Every log entry includes the daemon version via a hierarchical span tree. This is a **hard requirement** for all new components.

### Span hierarchy

```
wardnetd{version=0.1.1-dev.5+gabc1234}       # root span in main.rs
  ├── tunnel_monitor{}                         # background task
  ├── idle_watcher{}                           # background task
  ├── device_detector{}                        # background task
  ├── routing_listener{}                       # background task (event→routing dispatcher)
  ├── dhcp_server{}                            # background task (if DHCP enabled)
  ├── update_runner{}                          # background task (auto-update poll)
  ├── backup_cleanup_runner{}                  # background task (.bak-* sweep)
  ├── stats_flush_runner{}                     # background task (10s buffer flush + 1h rollup/trim)
  ├── mdns{}                                   # background task (advertises wardnet.local)
  └── api_server{}                             # axum serve
        └── http_request{method=GET, path=/api/devices}  # per-request (tower-http TraceLayer)
```

### Rules for new components

1. Every background component's `start()` method accepts a `parent: &tracing::Span` parameter.
2. Inside `start()`, create a child span: `let span = tracing::info_span!(parent: parent, "component_name");`.
3. Every `tokio::spawn(future)` must be `tokio::spawn(future.instrument(span.clone()))` — spawned tasks do NOT inherit parent spans.
4. For inner spawns (e.g. hostname resolution inside device_detector), capture `tracing::Span::current()` and instrument the spawned future.
5. `main.rs` captures `root_span = tracing::Span::current()` (which is the `wardnetd{version=...}` span) and passes it to all component `start()` calls.

## OUI database

- Full IEEE MA-L database (~39K entries) in `crates/wardnetd/data/oui.csv`.
- Parsed at build time by `crates/wardnetd/build.rs` → generates `oui_data.rs` in `OUT_DIR`.
- Locally administered MACs (bit 1 of first byte set) detected as "Randomized MAC" (typically phones using MAC randomization).
- `cargo::rerun-if-changed=data/oui.csv` — only regenerates when CSV changes.

## Versioning

- Version is derived from git tags at compile time via `build.rs` → `WARDNET_VERSION` env var.
- Shared version-parsing logic lives in `source/daemon/build-support/version.rs` (included by both `wardnetd/build.rs` and `wctl/build.rs` via `include!()`).
- Release: `v0.1.0` tag → `0.1.0`. Dev: N commits after tag → `0.1.1-dev.N+gabc1234`.
