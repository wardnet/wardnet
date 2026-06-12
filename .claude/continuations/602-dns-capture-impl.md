# Continuation: Fix service layer violations + implement issue #602

## Context

This session has two sequential tasks. **Complete Phase 1 fully before starting Phase 2.**

---

## Phase 1 (prerequisite): Fix cross-service repository injection violations

### The rule

Cross-service access is always **service-to-service**. If service A needs data from domain B,
it holds and calls `Arc<dyn BService>`. Holding `Arc<dyn BRepository>` from a sibling domain
is forbidden — it bypasses business rules and leads to duplicated logic.

See `.agents/architecture.md` for the full rule.

### Known violations to fix

| Service | Illegal field | Replace with |
|---------|--------------|--------------|
| `DnsFilterServiceImpl` (`dns_filter/service.rs`) | `device_repo: Arc<dyn DeviceRepository>` | `Arc<dyn DeviceService>` |
| `RoutingServiceImpl` (`routing/service.rs`) | `devices: Arc<dyn DeviceRepository>` | `Arc<dyn DeviceService>` |
| `RoutingServiceImpl` (`routing/service.rs`) | `tunnel_repo: Arc<dyn TunnelRepository>` | `Arc<dyn TunnelService>` |
| `TunnelServiceImpl` (`tunnel/service.rs`) | `devices: Arc<dyn DeviceRepository>` | `Arc<dyn DeviceService>` |
| `SystemServiceImpl` (`system/service.rs`) | `tunnel_repo: Arc<dyn TunnelRepository>` | `Arc<dyn TunnelService>` |
| `DeviceDiscoveryServiceImpl` (`device/discovery.rs`) | `dhcp: Arc<dyn DhcpRepository>` | `Arc<dyn DhcpService>` |

### How to fix each violation

For each violation:

1. **Identify what the service uses from the sibling repo** — read every call site of the
   illegal field in that service. Note the exact operations.

2. **Add those operations to the owning service trait** — if the method is missing on
   `Arc<dyn BService>`, add it to the `BService` trait, implement it in `BServiceImpl`,
   and add a no-op or delegating impl to any mock in tests.

3. **Swap the field** — replace `Arc<dyn BRepository>` with `Arc<dyn BService>` in the
   struct and constructor. Update call sites to go through the service method.

4. **Update `init_services_with_factory`** (`wardnetd-services/src/lib.rs`) — rewire the
   constructor to pass the service instead of the repo.

5. **Update tests** — mock structs that implement the service trait will need the new
   methods added (return sensible defaults).

### Verification after Phase 1

```sh
make check-daemon   # must exit 0 before proceeding to Phase 2
```

---

## Phase 2: Implement issue #602 — Per-device DNS event capture state

### Read the approved plan first

```
/Users/pedrogomes/.claude-personal/plans/adaptive-purring-ripple.md
```

All decisions in the plan are final — do not re-litigate them.

### Start in a new worktree

```sh
gt wt add feature/dns-capture-602
cd feature/dns-capture-602
```

Never edit files in the repo root or `.bare/`.

### What the plan covers

1. **Migration** — 3 new columns on `devices`, new `dns_events` table with `ON DELETE CASCADE`
2. **Data layer** — new `DnsEventsRepository` (5 methods); `DeviceRepository` gains 2 methods; `DeviceRow` / `SELECT_COLS` / `into_device()` extended; `RepositoryFactory` gains `dns_events()`
3. **Common types** — `Device` gains 3 capture fields; new `DnsCaptureSettingsRequest` / `DnsCaptureSettingsResponse` DTOs; new `WardnetEvent::DeviceCaptureSettingsChanged { device_id, enabled, timestamp }` variant
4. **Services layer** — `DnsLogSink` gets a `capture_tx` channel (same `Mutex<Option<Receiver>>` pattern as `dns_log_persist_rx`); new `DnsCaptureRunner` (insert loop + event-cache loop + 1 h prune loop); `DeviceService` gains `update_dns_capture_settings`
5. **API** — new `GET /api/devices/{id}/dns-capture` + `PATCH /api/devices/{id}/dns-capture`
6. **`main.rs`** — wires `DnsCaptureRunner` alongside `DnsQueryLogRunner`
7. **Web UI** — SDK types + 2 methods, 2 hooks, new `DeviceDnsCaptureCard`, added to `DeviceDetail.tsx`

### Key decisions (do not re-litigate)

- `dns_events.status` = raw `QueryLogRow.result` value, no mapping — front end decides semantics
- Ring enforcement is in the **hourly prune loop**, never on the insert hot path
- Hot-path cache = `HashSet<DeviceId>` (presence = enabled); populated on startup from DB; updated via `DeviceCaptureSettingsChanged` event — zero per-request DB reads
- Prune loop reads caps from `devices` table each run; also deletes all rows for devices where capture is now off
- Admin sees toggle + cap inputs + storage indicator (`row_count` / `size_bytes`) — never the domain list
- `sync_state TEXT NOT NULL DEFAULT 'pending'` included now; update logic is deferred to the SSE issue
- `DeviceDnsCaptureCard` uses a unified save/cancel form for all three fields

### Hard rules (apply to both phases)

**1. Layered architecture — service-to-service only**

A service holds only its own repository. Any cross-domain access goes through `Arc<dyn BService>`.
Never inject a sibling `Arc<dyn BRepository>` into a service. See `.agents/architecture.md`.

**2. Tests in separate files — never inline**

Tests live under a `tests/` subdirectory. Never put `#[test]` blocks inside source files.

Service tests (`DnsCaptureRunner`):
```
src/dns/capture_runner.rs           ← source
src/dns/tests/capture_runner.rs     ← tests (mock repos)
src/dns/tests/mod.rs                ← add: mod capture_runner;
src/dns/mod.rs                      ← already has: #[cfg(test)] mod tests;
```

Repository tests (`DnsEventsRepository`):
```
src/repository/dns_events.rs              ← source
src/repository/tests/dns_events.rs        ← tests (use test_pool() from mod.rs)
src/repository/tests/mod.rs              ← add: mod dns_events;
```

Service tests use manually-defined mock structs implementing repository or service traits.
Repository tests use `test_pool()` (in-memory SQLite with all migrations applied).
See `.agents/testing.md`.

### Verification after Phase 2

```sh
make check-daemon   # compilation + clippy + all tests
make openapi        # regenerate docs/openapi.json
make run-dev        # verify card appears, save/cancel work in browser
```
