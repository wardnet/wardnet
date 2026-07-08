---
status: accepted
date: 2026-07-07
issue: "#266 (epic — Remote access: inbound WireGuard + published access)"
---

# ADR: Inbound WireGuard peers are Devices; published access defaults to tunnel-only, public is an explicit opt-in

## Context

Issue #266 asks for two related but distinct capabilities: (1) an inbound WireGuard server so a remote peer (the admin's phone on hotel WiFi, a household member away from home) gets the same DNS filtering, ad-blocking, and outbound-tunnel routing as a LAN device, and (2) a way for an admin to publish an internal LAN service to the outside world — a raw address/port (e.g. SSH to a NAS) or a self-hosted web app on its own subdomain (e.g. `https://bitwarden.home1.my.wardnet.services`).

Both are greenfield: no inbound-WireGuard code, no DNAT/reverse-proxy primitive, and no multi-hostname TLS support exist anywhere in `source/daemon` today. Two decisions here are hard to reverse and not obvious from the code, so they are recorded here.

## Decision

### 1. Inbound WireGuard peers are modeled as `Device` rows, not a parallel concept

A peer gets a synthetic MAC derived from its WireGuard public key and a `last_ip` from the inbound WireGuard subnet. This makes it participate in `RoutingRule`, Network Zone enforcement, and DNS capture through the exact same pipeline every LAN device already uses (`DeviceDiscovered`/`DeviceIpChanged`/`DeviceGone` events, `ZoneEnforcementListener`, `DnsFilterService`) — no parallel enforcement path to build or keep in sync.

The alternative — a standalone `InboundWgPeer` concept with its own routing and zone-gate wiring — was rejected: the product's entire mental model is device-centric (routing rules, zones, DNS capture are all keyed by device), and duplicating that machinery for a second "device-like" entity buys no functional benefit, only drift risk between the two enforcement paths.

### 2. Published access: mechanism and visibility are separate, orthogonal choices; visibility defaults to tunnel-only

An admin publishes an internal device's service via one of two mechanisms — **Address forward** (raw L4 TCP/UDP to `ip:port`) or **App forward** (L7 HTTP(S) reverse-proxy to `ip:port`, reachable via a subdomain of the gateway's DDNS domain). Independently, each published item has a **visibility**: **Tunnel-only** (default — reachable only from an authenticated inbound WireGuard peer, source-IP-gated the same way the Network Zone admin-UI gate already TCP-resets disallowed traffic) or **Public** (reachable from the open internet, no peer required).

This mirrors how Tailscale splits private `Serve` from public `Funnel` (an explicit, separately-confirmed per-service opt-in), and how Cloudflare Tunnel separates "reachable" from an identity-aware access policy — rather than the traditional router-style port-forward model (UniFi/pfSense/Firewalla), where a forward is unconditionally public with no built-in gating. Defaulting every new published item to tunnel-only means the gateway never exposes an internal service to the internet by accident; going public is a deliberate, visible choice per item.

## Consequences

- **Public App forward requires a wildcard certificate.** `TlsServiceImpl::ensure_certificate` (`wardnetd-services/src/tls/mod.rs`) issues one certificate for one FQDN today via ACME DNS-01. Wildcard SAN support is additive — it reuses the same DNS-01 TXT-record mechanism DDNS already drives — but is new scope for the TLS service.
- **A new reverse-proxy layer is needed.** `:443` today only ever serves the daemon's own Axum router (`wardnetd/src/tls_server.rs`); App forward needs Host-header-based routing to internal `ip:port` targets, which doesn't exist anywhere in the daemon.
- **A new DNAT-equivalent firewall primitive is needed** for Address forward; `FirewallManager` (`wardnetd-services/src/routing/firewall.rs`) today only has masquerade (SNAT) and zone accept/drop rules.
- **Reversibility:** both decisions are additive on top of the existing device/routing/zone and TLS/DDNS subsystems — nothing existing is restructured, and turning a published item back to tunnel-only or deleting it removes the corresponding firewall/proxy rule live.
- **WAN reachability for the inbound WireGuard server goes through the Tunneller, not a LAN port-forward.** The Tunneller has no UDP/WireGuard support as of #809's planning (it's a TCP-stream multiplexer routed by TLS SNI, which has no analogue for WireGuard's SNI-less UDP handshake), so a new cloud-side UDP relay is part of #809's scope — see wardnet-cloud ADR-0015 for the design (stable per-network UDP port, reused frame protocol).
