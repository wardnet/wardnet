---
status: accepted
supersedes: "ADR-0022 §2"
date: 2026-08-10
issue: "#1146 (epic — Application hosting)"
---

# ADR: A published app is a name, a reach ladder, and a policy — not a port forward

*Supersedes decision #2 of [0022-inbound-wireguard-and-published-access.md](0022-inbound-wireguard-and-published-access.md). Decision #1 of that ADR (inbound peers are `Device` rows) still stands. Companion to [0031-household-identity.md](0031-household-identity.md).*

---

## Context

ADR-0022 §2 modelled published access as two orthogonal axes: a **mechanism** (Address forward = raw L4 DNAT, App forward = L7 reverse proxy) and a **visibility** (Tunnel-only by default, Public by explicit opt-in). Issues #814–#817 were written against that model.

Designing the surface for epic #1146 broke it in four places.

**The admin never wanted to pick a mechanism.** "Address forward vs App forward" is a restatement of "is this HTTP or not", which the admin already knows implicitly from the app they are publishing. Exposing it as a choice moved a derivable fact onto the user.

**Two visibilities were the wrong number.** The most valuable rung was missing: a name and a real certificate for an app *at home*, with nothing leaving the house. Wardnet can already do it — it is the authoritative DNS server, it holds a wildcard certificate covering `*.<vanity>.my.wardnet.services`, and ADR-0029 already seeded a split-horizon `*.<fqdn> → LAN IP` record. Meanwhile "Tunnel-only" was a poor name in a codebase where *tunnel* already means an outbound VPN-provider tunnel and *Tunneller* means the cloud relay, and it named neither.

**The DNAT primitive was specified for cells that do not need it.** Wardnet *is* the router — an assumption Pangolin, Cloudflare Tunnel and Tailscale cannot make about their deployments. A LAN client is on the same L2 and already reaches `192.168.1.50:22`. A **Remote peer** is inside the WireGuard tunnel and the Pi already routes it onto the LAN. Neither needs a forward; both need a *name*.

**Public raw L4 is a different product from public HTTPS.** The cloud edge routes by TLS **SNI** (ADR-0017). HTTPS carries a hostname in the ClientHello, so publishing it publicly reuses machinery that exists. A raw TCP stream or UDP datagram carries nothing to demux on — wardnet-cloud hit this exact wall for inbound WireGuard and answered it in cloud ADR-0017 with a stable per-network edge **port**. Generalising that means a port-allocation service, a managed range per edge node, an abuse surface on uninspectable ports, relay bandwidth, and an address the admin does not get to choose.

## Decision

### 1. One unit: the **published app**, with transport, reach, and access policy as attributes

A **published app** is a named service on a LAN device. **Transport** (HTTPS/WebSocket or raw TCP/UDP) is derived from what is being published, not chosen from a menu of mechanisms. **Access policy** is defined in ADR-0031's terms.

`Address forward`, `App forward`, `visibility`, and `Tunnel-only` are retired as domain terms.

### 2. Reach is a three-rung ladder; **LAN is always on and is not a choice**

| Rung | Reachable by | Path in |
|---|---|---|
| **LAN** — implicit, always | anything on the home network | split-horizon DNS → Pi's LAN IP → daemon proxy |
| **Remote peer** — opt-in | authenticated **Remote peers** (ADR-0022 §1) | inbound WireGuard → Pi; the cloud edge is not involved |
| **Public** — opt-in, warned | the open internet | cloud edge SNI demux → **Tunneller** → Pi |

Each rung strictly widens the previous, so **Public subsumes Remote peer** — they are not orthogonal checkboxes and must not be presented as such. This mirrors the guarantee-ladder framing already used for Network Zones in ADR-0018.

Defaulting to LAN — rather than ADR-0022's Tunnel-only — means publishing is *useful* the moment it is done and still exposes nothing. Widening is always a deliberate, per-app act.

### 3. The Public rung is HTTPS/WebSockets only; public raw L4 is deferred

Public HTTPS rides the existing SNI demux, and the stream stays encrypted end to end: the edge routes on the still-encrypted ClientHello and the Pi terminates with its own certificate. **We never see plaintext** — the concrete distinction from Cloudflare Tunnel, which decrypts and inspects at its edge, and it is worth saying so in the product.

Public raw L4 needs the DNAT primitive **and** a cloud-side per-network port allocator; neither is useful alone, so they are one deferred unit (#1158). Tailscale Funnel stops at exactly this line, so the deferral does not put us behind the market.

### 4. No DNAT primitive in v1: raw-L4 publishing is a **DNS record plus a Network Zone exception**

On the LAN and Remote-peer rungs, publishing a raw-L4 service means maintaining an authoritative local record for its name (through `DnsLocalService`, never `dns_local_repo` directly) and — when the target device sits in an isolated zone — a narrow, per-port, admin-visible exception through `ZoneEnforcementService` (ADR-0018/0019/0021).

The zone exception is the interesting half and is genuinely differentiated: no competitor enforces zones, so none of them can offer "let the living-room TV reach the Jellyfin box on the IoT zone" as a bounded hole rather than a hole in everything. A published app must never silently widen a zone: the exception is listable and is revoked on unpublish.

### 5. The app catalog is compiled into the daemon

Publishing a known app pre-fills port, transport, WebSocket upgrade paths, reachability probe, OIDC wiring, and the recommended access policy. That recipe set ships **in the binary**, like the vendor catalog in ADR-0025 — not fetched.

A remotely-updatable catalog is a channel that can change **where an app's traffic is proxied** and **which OIDC issuer it trusts**; a spoofed or compromised feed is a nasty primitive to hand an attacker. In-binary, the catalog inherits the release signature and the update path we already vet, and it works with no internet, which the LAN rung requires. The churn cost is real but bounded: **custom publishing always exists**, so the catalog is convenience and never a gate.

## Consequences

- **A new Host-header reverse proxy is needed on `:443`.** `wardnetd/src/tls_server.rs` only ever serves the daemon's own Axum router today. It is built once (#1150) and reused by all three rungs. It must proxy **WebSocket upgrades** correctly — Vaultwarden's `/notifications/hub` is the canonical case, and failure there is silent (live sync stops) rather than loud.
- **The Public rung opens Tunneller `dest_port=443`**, recorded in `CONTEXT.md` as "reserved, closed until #816". This unblocks #920 (Private DNS DoH), whose dependency moves to #1151.
- **A working Public rung depends on #824** — the daemon does not yet know its real relay endpoint.
- **`AddressForward` is never built.** #814/#815 are closed rather than edited; the DNAT design moves whole into #1158.
- **Reversibility.** Rungs are per-app flags whose rules are added and removed live; the catalog is data; the zone exception is an existing enforcement primitive. What is *not* cheap to reverse is the retirement of the mechanism/visibility vocabulary — hence this ADR.
- **Honest limit.** On the LAN rung, publishing a raw-L4 app grants a *name*, not a tunnel: a client that could not already route to the device (other than by zone policy) still cannot. The UI must not imply forwarding that is not happening.
