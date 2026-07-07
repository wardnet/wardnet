---
status: accepted
date: 2026-06-25
issue: "#214 (hardware watchdog) — scope deliberately expanded during design"
---

# ADR: Three-layer watchdog + health-monitor subsystem

---

## Context

`Restart=always` only catches a daemon that **exits**. A `wardnetd` that is
livelocked, deadlocked, or stuck in an uninterruptible syscall keeps systemd
happy and never recovers (NFR-009). Issue #214 asked for a **hardware
watchdog** so a hung daemon reboots the Pi within ~15 s.

A dumb timer that pets `/dev/watchdog` only catches a *total* runtime freeze.
It cannot distinguish "process alive but the DNS subsystem is deadlocked" from
"fully healthy", so it would either reboot too eagerly or not at all for the
common partial-failure case. We therefore expanded scope to add a
**Spring-Actuator-style health subsystem** and gate a *proportionate* recovery
on it.

## Decision

A **three-layer** model:

| Layer | Trigger | Mechanism | Recovery |
|---|---|---|---|
| **HealthMonitor** | components register `HealthCheck`s; *Y* consecutive failures ⇒ component DOWN ⇒ overall DOWN | new subsystem in `wardnetd-services/src/health/` (`ArcSwap` snapshot, concurrent refresh with per-check `tokio::time::timeout`) | reports status only |
| **Soft watchdog** | overall health DOWN **or** snapshot stale | withhold `sd_notify(WATCHDOG=1)` ⇒ systemd `WatchdogSec=15` | systemd **restarts the service** (~seconds; Pi stays up) |
| **Hard watchdog** | total runtime freeze (even the health loop can't run; D-state) | `/dev/watchdog`, pet **UNGATED** | kernel **reboots the host** (≤15 s) |

### Key invariant: the hardware watchdog is never health-gated

The `/dev/watchdog` pet runs on a fixed cadence and **never consults health**.
It is the backstop for the case where *nothing* — not even the health checker
or the soft sd_notify loop — is running. Gating it on health would defeat its
only purpose. Only the *soft* sd_notify ping is health-gated. This is the
single most important property of the design.

### Other decisions

- **Daemon owns `/dev/watchdog` directly** (rather than systemd's
  `RuntimeWatchdogSec=`/`WatchdogDevice=`). The appliance model keeps recovery
  self-contained and testable: a `WatchdogOps` trait with a Linux
  `/dev/watchdog` impl and a `NoopWatchdog` mock, wired onto `Backends` like
  `SystemPowerOps`/`GarpOps`. systemd's own hardware-watchdog support would
  have meant PID-1 owning the device and a coarser, health-blind policy.
- **`Type=notify` + `READY=1` after listeners bind.** The daemon only reports
  ready once `:7411`/`:443`/`:80` are bound, so systemd's `active(running)` is
  meaningful and the watchdog supervision arms at the right moment.
- **Unauthenticated `GET /health`** (200 = UP, 503 = DOWN). An explicit,
  documented exception to the require-auth rule — the same pattern as
  `GET /api/setup/status`. It carries no sensitive data and must be reachable
  by load balancers / uptime checks.
- **Concurrent checks, per-check timeout, no segregated runtime.** Checks run
  concurrently via `futures::future::join_all`; each is bounded by
  `tokio::time::timeout` so a hung probe becomes `Down { detail: "timeout" }`
  instead of stalling the cycle. Every check must be genuinely async; any
  unavoidable blocking work wraps itself in `spawn_blocking` (as the SQLite
  layer does). A dedicated health runtime is recorded here as a **future
  option**, to revisit only if a probe ever needs sustained heavy work —
  premature for four lightweight probes.
- **Magic-close disarm.** On clean shutdown the hardware runner writes `'V'`
  and closes the device first, so a graceful `systemctl stop` does **not**
  reboot.

### Initial probes

DB connectivity (`SELECT 1`), liveness (always UP — its presence on a fresh
snapshot proves the loop schedules), DNS, and DHCP. (The original sketch named
"tunnel" as the fourth probe, but tunnels are optional and expose no clean
readiness signal, whereas DHCP is a core LAN service with the same shape as
DNS — so DHCP replaced it.)

The DNS/DHCP probes are **desired-vs-actual**, not raw `is_running()`. Both
servers are started by their runners *only when the corresponding config flag
is enabled*, and stopped when toggled off — so `is_running()` is the admin
**enable-state**, not health. A naive `is_running()` probe would report DOWN
for a legitimately disabled service (e.g. DHCP off because the operator uses
their router's DHCP), the soft watchdog would withhold its ping, and systemd
would **restart-loop a healthy daemon**. Instead, each probe reads its
configured `enabled` flag through the auth-gated service under a nil-admin
context (exactly as the DNS/DHCP/heartbeat runners already do) and reports DOWN
**only** when `enabled && !is_running()` — i.e. the service actually crashed.
This was a bug in the original 4-probe sketch, caught and fixed during
implementation.

The mock daemon registers only liveness + DB, because its noop DNS/DHCP servers
never bind a socket.

## Consequences

- A wedged subsystem now triggers a **proportionate** systemd service restart,
  not a full host reboot; only a *total* freeze escalates to the kernel reboot.
- `deploy/wardnetd.service` changes from `Type=simple` to `Type=notify` with
  `NotifyAccess=main` and an active `WatchdogSec=15`. Repeated watchdog
  restarts count toward the existing `StartLimitBurst` budget, so a
  persistently-unhealthy daemon eventually trips `OnFailure=` rollback rather
  than restarting forever.
- New dependency: `sd-notify` (pure-Rust, no libsystemd C dependency; a
  graceful no-op when `NOTIFY_SOCKET` is unset, so dev/mock runs are
  unaffected). The `WDIOC_*` ioctls are defined locally against the existing
  `libc` dependency (no `nix`).
- **Testability.** Health aggregation/debounce/recovery, the soft-watchdog
  gating decision (healthy→ping, DOWN→withhold, stale→withhold), the `/health`
  200/503 mapping, and the hardware pet-cadence/disarm-on-shutdown are all unit
  tested. The freeze→restart path is covered end-to-end: the daemon-side
  `wardnet-test-agent` (same container, same PID namespace) exposes
  `POST /process/signal` to `SIGSTOP` the daemon, and `watchdog.spec.ts`
  asserts systemd restarts it under a new PID. Because a frozen process never
  *exits*, only the watchdog can recover it, so the restart proves the
  mechanism. The hardware `/dev/watchdog` → host-reboot path is **not**
  CI-testable (no device in the container; `softdog` would reboot the runner)
  and stays a **manual Pi acceptance test**: `systemctl kill -s STOP wardnetd`
  must reboot the Pi within ~15 s, and a clean `systemctl stop` must not.
