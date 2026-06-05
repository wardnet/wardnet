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
  ├── ddns_update_runner{}                      # background task (keeps the public A record current)
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

## SQLite performance notes

### Index every column used in WHERE on high-volume tables

`dns_query_log` is written on every DNS query and can hold millions of rows.
Any column used in a `WHERE` clause — including filter columns added later to
`QueryLogFilter` — must have an explicit index in the migration that introduces
the column. Omitting the index causes full-table scans on reads **and** inflates
WAL file size, which makes writes progressively slower on Raspberry Pi SD cards.

**Checklist for a new column on `dns_query_log` (or any append-only log table):**

1. Add `CREATE INDEX IF NOT EXISTS idx_<table>_<col> ON <table>(<col>)` in the
   same migration that adds the column.
2. If the column participates in FK-like joins (even without a `REFERENCES`
   clause), add the index. SQLite enforces `foreign_keys=ON` constraints by
   scanning the referenced table; an unindexed child column forces a full scan
   on every insert.
3. Verify with `EXPLAIN QUERY PLAN` that the insert does not trigger a table
   scan (`SCAN <table>` in the plan output is a red flag on a table > 1k rows).

### WAL checkpoint contention on Raspberry Pi

WAL mode (`synchronous=NORMAL`) is the correct configuration. However, if a
read transaction holds an old WAL snapshot — e.g., a long-running stats
aggregation query — SQLite cannot reclaim WAL frames and the WAL file grows
unbounded. Once the WAL exceeds a few hundred MB, the auto-checkpoint that runs
after each commit dominates write latency, producing `slow statement` warnings
(`elapsed > 1s`) on otherwise trivial INSERTs.

**Regression signal:** `slow statement` warnings on `INSERT INTO dns_query_log`
with `elapsed > 1s` on a production Raspberry Pi. Root cause is almost always
either a missing index causing FK scan amplification or a stale WAL from a
long-running reader.

## Versioning

- Version is derived from git tags at compile time via `build.rs` → `WARDNET_VERSION` env var.
- Shared version-parsing logic lives in `source/daemon/build-support/version.rs` (included by both `wardnetd/build.rs` and `wctl/build.rs` via `include!()`).
- Release: `v0.1.0` tag → `0.1.0`. Dev: N commits after tag → `0.1.1-dev.N+gabc1234`.
