---
status: accepted
date: 2026-08-08
issue: "#864 — Provide a proper uninstall path"
---

# ADR: Tear down runtime state on shutdown, but only when the daemon is stopping rather than restarting

---

## Context

Wardnet shipped `install.sh` with no uninstall path. Underneath that convenience
gap sat a real defect: **the daemon never removed the kernel state it created.**
The `inet wardnet` nftables table is created at startup and was never deleted,
and the `wg_ward*` `WireGuard` interfaces likewise survived a stop. After
`systemctl stop wardnetd`, every forward/input/NAT rule stayed live in the
kernel until the next reboot. A stopped daemon was still filtering the user's
traffic.

The capability was already there and unused: `FirewallManager` has had a
`destroy_wardnet_table()` since the netlink migration, documented as "cleanup on
shutdown", implemented idempotently, and called by nothing. The work was wiring,
not implementation.

Two facts about the codebase shape the decision:

1. **The daemon restarts itself far more often than it is stopped.** The
   auto-update runner, the rollback path, and the admin Restart button all
   cancel the shared shutdown token to hand over to a replacement process under
   `Restart=always`. On a live box the six-hourly update poll makes restart the
   overwhelmingly common shutdown.
2. **Tunnels have no boot reconcile of their own.** `routing.reconcile()` runs
   at startup and rebuilds the nftables table (it already begins with a
   `flush_wardnet_table()`), but there is no `services.tunnel.reconcile()` in
   `main.rs`. The only thing that recreates a `wg_ward*` interface is the
   on-demand bring-up inside `RoutingService::apply_rule`, and that fires only
   when the tunnel is *recorded* as `Down`. Which record the database holds is
   therefore load-bearing — see decision 2.

## Decision

### 1. Teardown is gated on the cause of the shutdown

`shutdown_signal` previously collapsed all three `select!` arms to `()`,
discarding which one fired. It now returns a `ShutdownCause`:

- **`Signal`** — SIGINT or SIGTERM arrived from outside the process.
- **`Restart`** — the daemon cancelled its own shutdown token.

Runtime state is torn down only for `Signal`.

The alternative — unconditional teardown — is simpler and was rejected. Even
with decision 2 making teardown recoverable, it would still tear down and
re-establish every tunnel on **every auto-update**, six-hourly, for no benefit:
the replacement process is seconds away and inherits a kernel state that was
already correct. Users would notice the churn far more than a leftover table.

`systemctl restart` typed by a human is indistinguishable from `systemctl stop`:
both arrive as a bare SIGTERM, and systemd offers the process nothing to tell
them apart. It therefore classifies as `Signal` and tears down. That costs one
bring-up cycle but is not otherwise harmful, given decision 2 below. We accepted
it rather than inventing an on-disk "expect a restart" marker, which would be a
new correctness hazard (a stale marker would suppress teardown on a real stop)
in exchange for tidying a rare case.

### 2. Tunnels are torn down *through the service*, not the interface

This is what makes signal teardown safe, and it is easy to get wrong.

Deleting a `wg_ward*` interface behind the database's back leaves the tunnel
recorded as `Up` with no interface, and **the daemon never recovers from that on
its own**. The monitor's `reconcile_iface_presence` flips it to `Down` and
publishes `TunnelDown`; `RoutingService::handle_tunnel_down` then *removes* the
routing for every device using that tunnel and drops its route table. Nothing
recreates the interface: the on-demand bring-up inside `apply_rule` only fires
for a tunnel already recorded as `Down`, and startup reconcile runs before the
monitor's first tick, when the record still says `Up`. The observable result
would be tunnel-routed devices silently falling back to direct WAN after any
`systemctl stop`, until an admin brought each tunnel up by hand — a regression
against the previous behaviour, where interfaces simply survived a restart.

So shutdown calls `TunnelService::tear_down_internal` per live tunnel, which
removes the interface *and* records `Down`. The next boot's
`routing.reconcile()` per-device `apply_rule` pass then sees `Down` and uses the
existing on-demand bring-up to recreate exactly the interfaces the configured
devices need. Uninstall keeps using the raw interface path, because there is no
database left to keep in step — and with `--purge` it is about to delete it.

Both halves ride the same gate even though the nftables half would be safe
unconditionally. One rule is easier to reason about than two, and the cost of
the extra caution is a table that startup reconcile flushes anyway.

### 3. Teardown runs after the hardware watchdog is disarmed

The shutdown sequence already disarmed the hardware watchdog first, so "a slow
shutdown can't trip a reboot". Netlink teardown is exactly that slow thing, so
it belongs after that point and before the graceful-shutdown marker is recorded.

Every step is best-effort and logged rather than propagated: failing loudly on
the shutdown path would turn an untidy exit into a failed unit.

### 4. `wardnetd uninstall` owns the uninstall implementation

This follows directly from `0013-nftables-pure-netlink.md`. That ADR removed the
`nft` shell-out in favour of `rustables` so that nothing C links at runtime, and
`install.sh` accordingly does **not** list `nft` as a dependency — it may not
exist on the host at all. A shell uninstaller therefore cannot be relied on to
delete `table inet wardnet`.

So the uninstaller calls the same abstraction that installed the state, the same
instinct as the `WatchdogOps` boundary. `install.sh` additionally generates a
thin `/usr/local/sbin/wardnet-uninstall` wrapper (k3s-style, written before the
units are enabled so a half-failed install is still removable, and on disk
because `curl | sudo bash` leaves no script to re-run). The wrapper execs the
subcommand when the binary works; when it does not, it falls back to file
removal only and **exits non-zero saying firewall state may remain**, rather
than reporting a clean uninstall it did not achieve.

The fallback is deliberately dumber than the subcommand rather than a mirror of
it. Two implementations of the same inventory would drift.

### 5. Two tiers, with retained data re-owned to root

The default run keeps `/var/lib/wardnet/wardnet.db*` and
`/var/lib/wardnet/secrets/` and prints where they are; `--purge` destroys them.
Anything retained is `chown -R root:root`, because the `wardnet` system user is
deleted in the same run and files owned by an orphaned UID can be silently
inherited by whatever service is assigned that ID next.

Confirmation is inverted relative to `install.sh`: the installer degrades to
non-interactive defaults with no tty because installing is safe, whereas
uninstall refuses without `--yes`. `--purge` requires the word `PURGE` in full.
No automatic backup is taken before `--purge` — it would need a passphrase we
cannot prompt for in a piped context, and a half-written backup is worse than
none.

## Consequences

- A stopped daemon no longer filters traffic. Uninstall becomes mostly file
  removal, because the clean stop has already done the kernel work; the
  subcommand repeats it as an idempotent sweep in case the daemon was killed
  ungracefully.
- Restart behaviour is unchanged, so auto-updates cost no tunnel downtime.
- The shutdown asymmetry is invisible from the code alone, which is the main
  reason this ADR exists: a reader will ask why `systemctl stop` deletes the
  table and an auto-update restart does not.
- `TunnelInterface::list()` enumerates *every* `WireGuard` device on the host,
  so teardown filters on the `wg_ward` prefix (now a shared
  `TUNNEL_INTERFACE_PREFIX` constant rather than three scattered literals).
  Without that filter we would delete tunnels the user created themselves.
- An ungraceful kill still leaves the hardware watchdog armed and reboots the
  host ~15s later. The uninstaller always stops the unit cleanly for this
  reason, and the behaviour is now documented rather than merely true.
- `install.sh` ends with `systemctl restart wardnetd` on every run, including
  `--upgrade-only`. That is a bare SIGTERM, so it classifies as `Signal` and
  does tear down. Decision 2 is what makes this benign: the tunnels come back
  during the next boot's routing reconcile rather than staying down.
- Tunnel bring-up at boot is **demand-driven, not blanket**. Only tunnels that
  a configured device actually routes through are recreated, because the
  bring-up rides the per-device `apply_rule` loop. A tunnel with no devices
  pointing at it stays `Down`, which is both correct and consistent with the
  idle-tunnel watcher's behaviour.

## Not decided here

Whether to add an explicit `services.tunnel.reconcile()` at startup instead of
relying on the routing reconcile's per-device bring-up. The current route reuses
a well-tested path and keeps bring-up demand-driven; a dedicated reconcile would
be more obvious to a reader but would need its own answer to "which tunnels
should be up with no devices on them?", which is really the idle-tunnel
watcher's question.

## References

- `0013-nftables-pure-netlink.md` — why `nft` is not an install dependency.
- `0014-watchdog-and-health.md` — the disarm-first shutdown contract.
