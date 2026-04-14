# Wardnet Follow-Ups

Tracked issues and feature ideas surfaced during day-1 production usage that
are not blocking but should be addressed. Each item below has a corresponding
GitHub issue for public tracking.

---

## Bugs

### B-1 — pedromvgomes/wardnet#77: Phone TCP sockets stay stale ~30–60s after switching off a tunnel

**Symptom.** When a device's routing target changes from a VPN tunnel back to
direct (or to a different tunnel), browsing on the device appears broken for
~30–60 seconds before recovering on its own. Restarting Wi-Fi on the device
fixes it instantly. Does **not** happen in the opposite direction
(direct → VPN is instant).

**Root cause (most likely).** Pi-side state is correct the moment the switch
applies — the conntrack flush + route-cache flush in
`RoutingService::apply_rule` clear NAT/route state cleanly (verified in
production logs: `flushed conntrack entries for source IP src_ip=… deleted=N`
fires every switch). The residual outage is caused by the **device's own TCP
sockets**: established sockets keep retransmitting on the previous flow's
sequence numbers, but the source IP seen by the destination server has now
changed (NordVPN exit → home ISP). The destination server treats the packets
as belonging to a new 4-tuple, finds no matching TCP state, and drops them
silently. The device's TCP retry/teardown timer (Android default ~30–60s)
eventually closes the socket; new connections then work.

**Why it's asymmetric.** Direct → VPN works instantly because the device's
old TCP sockets were going *via* the home ISP's NAT, which had already been
torn down by the Pi's switch — the device gets RST quickly. VPN → Direct is
slower because the NordVPN server doesn't observe the IP change (it's still
holding the original session), so no RST is generated to wake the device up.

**Confirming evidence (gathered 2026-04-13).**

- Daemon logs at switch time:
  - `flushed conntrack entries for source IP src_ip=10.232.1.203 deleted=29`
  - `applied direct routing rule device_ip=10.232.1.203`
- `conntrack -L -s 10.232.1.203` shortly after switch shows a mix:
  - ~70% `[ASSURED]` flows working immediately (fresh TCP/UDP/QUIC)
  - ~30% `[UNREPLIED]` flows on specific destinations (Google
    Cloud, AWS, AdGuard DoT) — these are the device's stale persistent
    sockets failing
- Phone is reachable for LAN traffic (Pi UI on `10.232.1.10:7411`) the
  whole time, confirming routing is correct
- Manual `conntrack -D -s 10.232.1.203` does not change the symptom
- Cycling Wi-Fi on the device fixes it instantly

**Possible fixes to investigate.**

1. Inject TCP RST packets toward the device for every tracked TCP flow at
   switch time. Tells the device's TCP stack each connection is dead so apps
   close and reopen immediately. Implementation options:
   - Shell out to `tcpkill` (`dsniff` package) — adds a runtime dep
   - Send raw RSTs in Rust using `socket2` / a netlink-aware crate
   - nftables ephemeral `reject with tcp reset` rule for the device's source
     IP, applied for ~100ms then removed
2. Document the workaround in the UI (e.g. "if browsing seems stuck after
   switching, toggle Wi-Fi on the device").

**Files involved.** `source/daemon/crates/wardnetd/src/service/routing.rs`
(`apply_rule`, `handle_tunnel_down`).

---

### B-2 — pedromvgomes/wardnet#78: Duplicate `ip rule` entries for the same device

**Symptom.** `ip rule list` on the Pi shows multiple identical rules for one
source IP, e.g.

```
32756: from 10.232.1.50 lookup 100
32757: from 10.232.1.50 lookup 100
32758: from 10.232.1.50 lookup 100
32759: from 10.232.1.50 lookup 100
32760: from 10.232.1.50 lookup 100
```

Functionally innocuous (they all point to the same table) but indicates
state drift between the daemon's in-memory `applied` map and the kernel.
Will eventually cause confusion or hit a kernel rule-count limit.

**Root cause.** `RoutingServiceImpl::remove_device_kernel_state` looks up the
old rule from `state.applied` to know what to delete. If the kernel has
duplicates that the in-memory state never recorded (e.g. from a daemon
restart that re-applied without cleaning, or a race), they are never
removed, and subsequent applies pile on more.

**Possible fixes.**

- In `apply_rule`, after `remove_device_kernel_state`, also call
  `remove_ip_rule` repeatedly for the device IP until it returns "no such
  rule" (idempotent best-effort cleanup)
- In `reconcile`, detect duplicates against the database and prune all but
  one matching rule per device

**Files involved.** `source/daemon/crates/wardnetd/src/service/routing.rs`
(`remove_device_kernel_state`, `reconcile`).

---

### B-3 — pedromvgomes/wardnet#79: Tunnel marked `up` even when WireGuard never handshakes

**Symptom.** Switching a device to the US tunnel (`wg_ward2`,
`us8395.nordvpn.com:51820`) routes traffic correctly into the tunnel, but
no replies come back. `wg show wg_ward2` shows zero received bytes and no
"latest handshake" timestamp. The PT tunnels work fine; only the US one is
broken.

```
interface: wg_ward2
  endpoint: 192.145.116.106:51820
  transfer: 0 B received, 43.79 KiB sent
  persistent keepalive: every 25 seconds
```

Despite never handshaking, the daemon's tunnel `status` is `up` (because we
mark it up after configuring the interface, not after observing the first
handshake). The UI therefore claims the tunnel is healthy.

**Probable cause.** Either (a) NordVPN's `us8395` server is offline /
geo-blocked from this network, or (b) the peer public key cached in the
tunnel config is stale. Closely related to F-1 (best-server resolution).

**Possible fixes.**

- Tunnel-monitor: only mark a tunnel `up` once the first handshake is
  observed; mark `connecting` until then and `down` if no handshake within N
  seconds of bring-up
- UI: surface "no handshake yet" state distinctly from "up"
- Per-tunnel re-resolution of the peer endpoint at bring-up time (see F-1)

**Files involved.** `source/daemon/crates/wardnetd/src/service/tunnel.rs`,
`source/daemon/crates/wardnetd/src/tunnel_monitor.rs`,
`source/web-ui/src/components/compound/TunnelCard.tsx`.

---

## Features

### F-1 — pedromvgomes/wardnet#80: Re-resolve "best NordVPN server" at tunnel bring-up

**Idea.** When a user creates a tunnel by selecting a NordVPN
"best/automatic" option (e.g. *Best US server*, *Best low-latency in
Europe*) instead of a specific named server, Wardnet currently pins the
resolved endpoint into the tunnel config at creation time. The hostname
(e.g. `us8395.nordvpn.com`) is then used for every bring-up and never
re-evaluated.

**Why it matters.** The "best" server at config time may not be the best —
or even reachable — at bring-up time. We saw exactly this with the US
tunnel (B-3): the pinned host doesn't handshake at all, so the tunnel is
permanently dead even though NordVPN has plenty of working US servers.

**Desired behaviour.** When a tunnel was created via a "best" selector,
re-query the NordVPN provider for the current best matching host on each
bring-up and use that endpoint for the WireGuard config. Re-resolution
must happen only at bring-up (not on every packet), so the WireGuard
session itself stays stable for its lifetime.

**Implementation sketch.**

- Persist the user's *original selector* on the tunnel record (e.g.
  `selector_kind = best_country`, `selector_args = { country = "US" }`),
  alongside the currently-resolved endpoint
- In `TunnelService::bring_up`, branch on `selector_kind`:
  - `specific` → use the stored endpoint (current behaviour)
  - `best_*` → call the provider to re-resolve, persist the new endpoint
    onto the tunnel record, then bring up with the new endpoint
- UI: show both the selector ("Best US") and the currently-resolved server
  ("us8395.nordvpn.com — last resolved 2 minutes ago"), so users can tell
  what they're actually connected through

**Files involved.** `source/daemon/crates/wardnetd/src/service/tunnel.rs`,
`source/daemon/crates/wardnetd/src/vpn_provider_nordvpn.rs`,
`source/daemon/crates/wardnet-types/src/tunnel.rs`,
`source/web-ui/src/components/features/CreateTunnelSheet.tsx` and
`TunnelCard.tsx`.
