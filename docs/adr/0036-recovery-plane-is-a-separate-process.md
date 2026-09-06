---
status: accepted
date: 2026-09-06
issue: "#1202 (Agent-ops 1 — the recovery plane is a separate process the daemon provisions)"
---

# ADR: The recovery plane is a separate process the daemon provisions

> **Numbering.** The #1201 epic and its children call this ADR-0033 and its sibling
> ADR-0034. Both numbers were taken by ADRs authored between the epic's design
> interview (2026-08-15) and this change; per the README's rule the set is scanned
> and incremented, so this is **0036** and the MCP authorization-server ADR (#1203)
> is **0037**. The epic's text is not rewritten — this note is the redirect.

## Context

Three incidents were debugged by SSH-ing into the Pi. Two of them exposed the
shape of the problem rather than the individual bug: the network-zones routing
fault required buying a USB ethernet adapter and physically cabling a Mac to the
LAN, and the missing-static-IP fault needed OS-level changes the daemon cannot
make. Both share one property — **the moment the tooling is most needed is the
moment the LAN is down.** A phone on cellular has internet, the daemon is
running, and there is no path between them.

The **Tunneller** already provides exactly that path: one persistent outbound
WebSocket from the box to its region's PoP, framing inbound flows back down by
`dest_port` ([ADR-0029](0029-private-dns-dot.md) §2, cloud ADR-0015). It carries
inbound-WireGuard UDP and `dest_port=853` Private DNS today, and `dest_port=443`
is reserved and closed.
The problem is where the client for it lives: **inside `wardnetd`**
(`TunnelerRunner`, spawned from `wardnetd/src/main.rs`). The recovery path is a
component of the thing it exists to recover. `systemctl restart wardnetd` severs
it; a daemon wedged badly enough to need diagnosing is a daemon that has already
taken the diagnostic channel down with it; and the routing-zones bug was
precisely a case where broken policy routing could plausibly take the tunnel out
before anyone could use it.

This ADR records the process split that fixes that, and the constraints that
made every simpler arrangement impossible. It decides *where the transport
lives*; ADR-0037 decides how the control plane on top of it authenticates.

## Decision

### 1. Three processes, and the one that must never die is the simplest

| Process | Role | `Restart=` |
|---|---|---|
| `wardnetd` | features; provisions the tunneller at setup and on remote-access change | `always` + `WatchdogSec=15` |
| `wardnet-tunneller` | dumb frame router: owns the single WebSocket, dispatches by `dest_port` / SNI | `always`, no watchdog |
| `wardnet-mcp` | control plane; unprivileged local listener behind OAuth 2.1 (ADR-0037) | `always` |

The split is not "extract the recovery feature". It is "extract the *transport*,
and nothing else". `wardnet-tunneller` owns the dial (Ed25519 PoP + WebSocket
upgrade), the reconnect/backoff lifecycle, the frame protocol, and a
`dest_port` → local-port table. It holds no database handle, no repository, no
event bus, and no knowledge of what any relayed byte means.

### 2. The tunneller is extracted, not the MCP server

The obvious alternative — leave the tunnel where it is and let `wardnet-mcp`
dial its own — fails on a hard cloud constraint and would be wrong even if it
did not.

**A second tunnel is impossible without cloud surgery.**
`TunnelRegistry::register(&slug)` is keyed on **slug** and aborts the incumbent,
so a second connection from the same box *displaces* the first rather than
joining it. `max_daemons` defaults to `1`, and `bind_daemon`'s adopt path evicts
the network's oldest `daemons` row. There is no scope or capability axis to
narrow a second grant with: `aud` is the only one, and its values are mesh
service names. One box gets one tunnel, and something has to own it.

Given that, the question is only *which* process. The answer follows from what
the tunnel actually relays. Both current payloads — `DoT :853` and inbound-WG
UDP — **terminate inside `wardnetd`**. A daemon restart kills those features
whether or not the socket survives, so keeping the socket in the daemon buys
them nothing. What must survive a daemon restart is the **transport**, because
the transport is what a diagnosis arrives over. Meanwhile an MCP surface is the
opposite of a thing that should be in the critical path: it is feature-rich, it
will change on nearly every release of the diagnostic tool set, and its blast
radius on a bad deploy is the recovery channel itself.

So the always-up process is a frame router with a fixed protocol and almost no
reason to change, and everything that changes sits behind it. `wardnet-mcp`
restarting, crashing, or being upgraded costs a reconnect of one local socket,
not the tunnel.

### 3. The daemon hands over the Ed25519 seed, not tokens

`wardnet-tunneller` needs cloud credentials. The daemon writes it a mode-0600
handoff file at provisioning time — setup, and again whenever remote access
changes — carrying the 32-byte Ed25519 **seed**, the region gateway URL, the
`dest_port` → local-port map, and the feature gates.

Handing over *tokens* instead was rejected on a number: `IDENTITY_JWT_TTL_SECS =
3600`. Identity JWTs are minted from the key and live an hour. A tunneller
holding only tokens is a tunneller that goes dark 60 minutes into a daemon
outage — which is not an edge case, it is the **entire scenario this epic
exists for**. Nor can the daemon refresh them on its behalf: the daemon is the
thing that is down. A process that must outlive the daemon indefinitely must be
able to mint its own credentials, and that means holding the key.

**Accepted cost: the seed exists in two places on disk.** Today it lives once,
under the SecretStore at `ddns/daemon/signing_key`, in a mode-0600 file readable
only by the `wardnet` user. After this change a second copy sits in
`wardnet-tunneller`'s handoff file. The blast radius, stated plainly:

- The seed is the box's **cloud** identity. Whoever holds it can mint identity
  JWTs for this network and therefore impersonate the box to wardnet-cloud —
  connect as its tunneller (displacing the real one, per the registry semantics
  above), publish DDNS records for its slug, and answer `_acme-challenge`, which
  is enough to obtain a certificate for the canonical FQDN.
- It grants **nothing inside the house**.
  [ADR-0031](0031-household-identity.md)'s rule holds unchanged: nothing in
  wardnet-cloud can vouch for a box login, so a stolen seed does not become an
  admin session. The compromise is of the box's cloud presence, not of the LAN.
- It is a **second file at the same trust level, not a lower one** — same
  mode-0600, same owner class, on the same disk. An attacker who can read one
  can read the other; there is no new privilege boundary being crossed, only a
  second copy behind the same one.
- **Rotation gets a second writer.** Any future seed rotation must rewrite both
  copies and signal the tunneller to re-read, or the box splits into two
  identities racing for the same slug-keyed registration.

The alternative that avoids the second copy — invert the ownership so the
tunneller holds the seed and the daemon asks *it* for tokens — was rejected as
worse: it makes the daemon's DDNS, ACME and entitlement paths depend on the
recovery process being up, which is the coupling this ADR is removing, pointed
the other way.

### 4. Egress independence is already true; DNS is the part that is not

A recovery channel that rides the box's own policy routing is not a recovery
channel — a routing bug takes it out along with everything else. It does not,
and this is by construction rather than by new work:

- Nothing anywhere in the daemon sets `SO_MARK` or `fwmark`, and no
  ordinary socket sets `SO_BINDTODEVICE`. The only callers that bind a device
  are the diagnostics that must — `TunnelExitProbe` and the tunnel
  latency/throughput testers — and they opt in *precisely because* default
  egress is not through a tunnel.
- The nftables ruleset has no `output` hook. Its base chains are `prerouting`,
  `postrouting`, `forward` and `input` only.
- Routing is source-based `ip rule` keyed on **device** IPs
  (`PolicyRouter::add_ip_rule(src_ip, table)`). The daemon's own sockets bind the
  Pi's host IP, match no per-device rule, and fall through to the main table and
  the WAN default.

That rationale is already written down, at `ddns/public_ip.rs:1-11`, where it
justifies DDNS publishing the WAN IP rather than a tunnel exit IP. The same
three facts are what make the tunneller's egress independent, and
`wardnet-tunneller` inherits them by being an ordinary unprivileged process
opening an ordinary socket. **No new mechanism is needed to keep the recovery
channel off Wardnet's own policy routing — only the discipline not to add one.**

**DNS is the exception, and it is a real one.** The PoP is a hostname, resolved
through `getaddrinfo` → `/etc/resolv.conf`, which on a Wardnet box can point at
the Pi's own resolver. A dead or misconfigured local resolver therefore takes
the recovery channel down through a path that has nothing to do with routing.
Closing that is child #1206 (pin the tunneller's resolver, cache the
last-known-good PoP address); this ADR records that the gap is DNS-shaped and
nothing else, so nobody re-audits egress looking for it.

### 5. Recovery-channel health never feeds `HealthMonitor` or `/dev/watchdog`

This is the invariant, and it is the one a future contributor is most likely to
break by accident, because adding a `HealthCheck` is the obvious thing to do
with a new connectivity signal.

`HealthMonitor` drives the **soft watchdog**: `wardnetd` sends
`sd_notify(WATCHDOG=1)` only while overall health is UP, and withholding the
ping lets systemd's `WatchdogSec=15` restart the service. So a tunnel
`HealthCheck` would mean: **wardnet-cloud has an outage ⇒ the box restarts
`wardnetd` every 15 seconds.** A cloud-side fault would become a local
availability fault, during the exact window in which someone is trying to reach
the box to find out what is wrong. The hardware `/dev/watchdog` is worse still —
it is ungated and reboots the host.

Tunnel connectivity is therefore an **API status field plus an `AnomalyType`**,
edge-triggered by the anomaly subsystem's existing partial unique index on
`(anomaly_type, COALESCE(subject_id, '')) WHERE resolved_at IS NULL`. It is
reported, it alerts once, and it restarts nothing. This is the same rule
[ADR-0030](0030-published-apps.md) states for published-app reachability probes,
for the same reason: **a signal about something outside the box must never drive
a control loop that restarts the box.** Child #1208 implements it; the rule is
stated here because it constrains anything that later wants to observe the
recovery plane.

The corollary runs the other way too: `wardnet-tunneller` gets `Restart=always`
and **no** `WatchdogSec`. It is a frame router with no health to gate on, and a
watchdog on it would only add a way for it to die.

### 6. The name is `wardnet-tunneller`, with two Ls

`CONTEXT.md` defines **tunnel** as an outbound VPN tunnel — `wg_ward*`, a
routing target, the thing a device's traffic egresses through. A binary named
`wardnet-tunnel` would therefore read as "the VPN thing" to every reader who
already knows the glossary, and it is the one process in the system that has
nothing to do with VPN tunnels.

**Tunneller**, two Ls, is already the established spelling for the relay: the
`/tunneller/v1/tunnel` wire path, the regional cloud service, and the
*Tunneller* entry under *Infrastructure* in `CONTEXT.md`. The new process is a
client of exactly that service, so it takes that name. The one-L
`TunnelerConnector` / `TunnelerClient` Rust identifiers stay as they are — a
cosmetic straggler [ADR-0029](0029-private-dns-dot.md) §Consequences already
declined to churn, not a second concept.

## Consequences

- **The daemon stops owning a long-lived socket.** `TunnelerRunner` leaves
  `wardnetd/src/main.rs`, and the daemon returns to being built entirely from
  fixed-interval poll loops. Anything reading tunnel state moves to the status
  field from #1208.
- **A new systemd unit cannot arrive via the auto-updater**, which ships
  binaries only. `wardnet-tunneller` needs a `deploy/wardnet-tunneller.service`
  file, an entry in `install.sh`'s `UNITS` array, and a **new** append-only
  migration id in `wardnet-postupgrade`. The same applies to `wardnet-mcp`.
- **`dest_port=443` is claimed** by the tunneller's SNI demux (#1207), so #816 /
  #1151 — the HTTPS reverse proxy and the **Public** rung of **Reach** — must
  route through that demux rather than binding the frame port themselves.
- **Two processes now hold the cloud identity**, with the blast radius above.
  Any future seed rotation is a two-writer operation.
- **The relayed features still die with the daemon.** Extracting the transport
  does not make `DoT :853` or inbound WireGuard survive a `wardnetd` restart —
  they terminate in the daemon and always will. What survives is the channel,
  which is what a diagnosis needs and what those features reconnect over.
- **The `dest_port` → local-port map makes an existing mismatch explicit.**
  wardnet-cloud's `udp_relay.rs` carries a live `TODO(wardnet#809)` that the
  WireGuard relay port is hardcoded `51820` while the daemon defaults to
  `51821`. The handoff file is where the Pi side of that becomes a written-down
  table rather than an assumption.
- **Reversibility is good, and the seam is testable.** Everything is additive: a
  new binary built from code that already exists, a new unit, and a handoff file.
  The frame protocol does not change, so the cloud is untouched. The failure mode
  of a half-finished migration is the status quo — the daemon dialling its own
  tunnel — rather than a broken box.
