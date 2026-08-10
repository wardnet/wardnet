---
status: accepted
date: 2026-08-09
issue: "#910 (epic — Private DNS: encrypted DNS at home and while roaming)"
---

# ADR: Private DNS is a closed DoT resolver keyed by a per-device secret hostname

## Context

CONTEXT.md, the README and the marketing site have promised "private DNS while roaming" as a Premium capability since the tier was defined. Nothing implemented it, and the promise was recorded against the wrong noun: the Premium-tier glossary entry named *the **Tunneller*** as the deliverable ("the Tunneller (private DNS while roaming)"). The Tunneller is the relay service that gives a box WAN reachability with no port-forward; it carries inbound-WireGuard UDP today and will carry `:443` after #816. It is infrastructure. **Private DNS** is the user-facing feature, and on the LAN it does not involve the Tunneller at all. This ADR separates them and records the design settled in the #910 plan interview.

The ask is narrow and concrete: a household member's phone should resolve through the Pi — with that phone's own filter profiles — at home *and* on cellular, with **no VPN**. Both mobile platforms already have a first-class way to do this, and both constrain the design hard:

- **Android**'s Private DNS setting takes a **hostname** and speaks **DoT** only. It sends **no ALPN**, requires a publicly-trusted and fully time-valid chain (no system-CA install, no user-CA acceptance), is **fail-closed** (a bad hostname means no DNS at all, not a silent fallback), and **cannot be configured programmatically** by any app.
- **iOS** has no programmatic encrypted-DNS API either, but honours a `com.apple.dnsSettings.managed` **configuration profile**, which — unlike a Wi-Fi-scoped setting — applies on **cellular** too.

So the phone side is fixed: one hostname, entered by hand on Android and shipped in a profile on iOS, that must work identically on both network paths. Everything below follows from making that one string do all the work.

The pieces the daemon already had: a resolution pipeline behind the UDP `:53` listener (extracted to `QueryPipeline` + `ReplyCapture` in #911), **daemon-owned TLS termination** with a **per-user wildcard certificate** whose SANs are the apex `<vanity>.my.wardnet.services` *and* `*.<vanity>.my.wardnet.services`, **split-horizon resolution**, and the **Tunneller** reverse tunnel with its `FRAME_CONNECT`/`dest_port` framing.

## Decision

### 1. Each granted device gets a secret hostname under the existing wildcard; the SNI is both authentication and attribution

Granting a device mints a **token** — 80 bits from the CSPRNG, lowercase RFC 4648 base-32, exactly 16 label characters — and the device's **device hostname** is `<token>.<canonical-FQDN>`. The `DoT` `:853` listener resolves the TLS **SNI**'s token label back to a live grant before reading a single byte of DNS. That one lookup does two jobs: it **authenticates** the connection (the token is the credential) and it **attributes** the queries (the resolved grant names the device whose DNS filter profiles apply and whose row the query log gets). This is the pattern AdGuard DNS, NextDNS and ControlD all converged on, for the same reason: DoT gives the server no other per-client channel.

The resolver is therefore **closed**. An unknown token — or the bare apex — closes the connection. Serving the apex was considered and rejected: the apex slug is **public via CT logs** (every issued certificate is logged), so an apex-serving resolver is an open resolver with the box's address printed in a public ledger. The secret label survives that exposure precisely because it is covered by the **wildcard SAN** the box already holds, so no per-device certificate is issued and **no token ever reaches a CT log**.

Alternatives rejected:
- **A per-device certificate per grant.** Puts every device hostname in a CT log, which destroys the only property making the hostname a credential, and multiplies ACME issuance by the household size.
- **Client-certificate (mTLS) authentication.** Android's Private DNS setting has no field for a client certificate; the whole feature would be iOS-only.
- **Source-IP allow-listing.** The roaming path's peer is the relay's loopback, and a phone's cellular IP changes constantly. It authenticates nothing.

Because the SNI is only checked at handshake, revocation cannot wait for the device to reconnect: `revoke_grant` publishes `PrivateDnsGrantRevoked`, and the listener terminates that device's live connections at once.

### 2. One hostname, two paths — split-horizon on the LAN, the existing Tunneller relay while roaming

The same string must resolve differently depending on where the phone is, and it does so at the DNS layer rather than in any client configuration:

- **On the LAN:** enabling Private DNS seeds a **split-horizon** wildcard `*.<fqdn> → LAN IP` `System` record, so the phone resolves its hostname straight to the Pi and connects to the local `:853`. No cloud round-trip; the feature works on a box whose Tunneller is down.
- **Roaming:** the public wildcard lands on the regional edge, whose `:853` **SNI demux** relays the **still-encrypted** stream down the box's existing Tunneller connection as `FRAME_CONNECT dest_port=853`; the daemon TCP-relays it to its own loopback `:853`. The cloud sees SNI and ciphertext, never a query.

Reusing the Tunneller — rather than standing up a second relay for DNS — is the decision here. It is one persistent connection per box either way, it inherits the enrollment identity, region pinning, backoff and keepalive already built for inbound WireGuard, and it keeps "the box is reachable from the WAN" as a single mechanism with a single failure mode. The cost is coupling: the runner now gates on `inbound_wg_enabled() || private_dns_enabled()`, dialling while either is on and tearing down only when both are off, with each inbound flow additionally gated per-frame on the feature it belongs to.

`dest_port=443` is **reserved and closed** until #816 lands the reverse web proxy. That is also the gate on DoH: `/dns-query` needs the `:443` relay, so **v1 is DoT-only** (#920 follows #816). Android forces DoT regardless, so DoT-only costs nothing on the platform that constrains us most.

### 3. Premium-only, and an entitlement loss persists a disable

Enabling requires all three of: the **wardnet DnsProvider** (the wildcard SAN and the regional edge are wardnet-operated — a BYO-domain box has neither; LAN-only BYO-domain DoT is #921), an **issued certificate**, and an **active entitlement**. **Disabling is always allowed**, so a lapsed or reconfigured box can still be switched off cleanly, and `reconcile` at startup **persists a disable** when the entitlement lapsed while the daemon was down.

Persisting the disable rather than merely gating the listener is deliberate, and mirrors inbound WireGuard: a Premium feature must not silently resurrect itself when a lapsed box restarts, and an operator looking at the admin UI must see the state the box is actually in. The visible consequence is that re-subscribing does **not** auto-re-enable Private DNS — the admin re-enables it. Grants survive; only the feature flag flips.

### 4. iOS gets a CMS-signed profile with no `ServerAddresses`, no `OnDemandRules`, and no MDM

`GET /api/private-dns/me/profile` is **device-keyed by source IP** (like `GET /api/devices/me`) and returns a `.mobileconfig` **CMS-signed with the box's live Let's Encrypt leaf**, intermediate embedded so the phone builds a path to the ISRG root it already trusts. Unsigned would show iOS's red "Not Signed" banner on a profile that reconfigures the user's DNS — for a self-hosted privacy product that is the wrong first impression, and the box already holds exactly the right key.

Three omissions are decisions, not gaps:
- **No `ServerAddresses`.** The hostname *must* be free to resolve to the Pi on the LAN and to the edge on cellular; pinning an address breaks whichever path it doesn't name.
- **No `OnDemandRules`.** iOS already auto-exempts captive-portal probing for managed DNS, and an always-on rule strands the phone behind hotel logins.
- **No MDM enrollment.** Manual install covers cellular, which is the whole requirement; MDM would demand a supervised device and an enrollment server.

Payload identifiers are `UUIDv5` derived from the hostname, so a re-download **replaces** the profile in place rather than stacking duplicates in Settings.

### 5. Admin-grant only; the push notification is a convenience, never the onboarding path

v1 has no user-initiated request flow (that is #919): an admin grants a device, and the admin-site granted-modal shows the hostname, a copy button, the per-platform steps, and a QR to the profile. The user PWA shows the same steps on the device itself.

#1041 added a **device-keyed `private_dns_granted` push** — "Send to device" (`POST /api/private-dns/grants/{device_id}/notify` → `{ delivered }`) — deep-linking the user PWA to `/settings#private-dns`. It is **device-audience** (`owner_kind = device`), so like every device-keyed push it is **not** written to the admin **Notification feed**. It is an admin-triggered convenience for the common case of granting a device whose owner is not standing next to you; the modal's hostname and instructions remain the always-present fallback, and `delivered: false` (a `200`, not an error) simply means that member hasn't enabled notifications.

## Consequences

- **Android's constraints are load-bearing on the listener.** The derived `:853` `rustls::ServerConfig` **clears the ALPN list** — Android sends no ALPN, and a server advertising any protocol list fails those handshakes with `no_application_protocol`. The listener owns no certificate of its own: it derives its config per connection from the live `:443` config, keyed on that config's `Arc`, so a **TLS renewal** rotation is picked up without a restart. A cert that lapses takes Private DNS down **fail-closed** — a phone with a bad Private DNS hostname has no DNS at all, so certificate health is now a household-visible dependency, not just an admin-site one.
- **Rate limiting is per-token, not per-IP.** The roaming transport peer is the relay's loopback, so per-IP limiting would pool every roaming device into one bucket. A breach is answered `REFUSED` rather than a connection reset: tearing the stream down would make a fail-closed client reconnect-storm.
- **Upstream selection is per-device, not per-IP, for the same reason (#923).** The per-device DNS routing snapshot is keyed by LAN source IP and reflects *applied* kernel state, both of which a roaming device lacks — v1 therefore sent roaming queries to the default upstream. #923 adds a second, `device_id`-keyed snapshot built by the routing service from the **persisted** routing rules (with `Default` resolved through the global policy, gated on `override_default_dns` and the tunnel not being `Down`), and the pipeline resolves a device-authenticated client's upstream through it: token → device → tunnel binding → `forward_via_tunnel`, so a device routed through the UK tunnel gets UK DNS answers on the go too; on a device-map miss it falls back to the IP-keyed map, so an on-LAN `DoT` client never loses the binding the kernel is enforcing. A `Down` tunnel drops the device to the default upstream — the same soft fallback the LAN path takes — while a transiently unbuildable forwarder fails **closed** (`SERVFAIL`) and a failed rebuild lookup **retains** the prior entries, so a transient error can neither leak a binding to the default upstream nor silently wipe it. Because the map is persisted-rule-driven and the Network-Zone enforcer clamps forbidden `Default` bindings in applied state only (never rewriting the rule row), the rebuild **mirrors that clamp itself**: a `Default` rule whose resolved policy the device's zone forbids (while permitting direct) is excluded. The map is kept current by a dedicated bus listener (`DnsDeviceSnapshotListener`) that coalesces event bursts into single serialized rebuilds — every input of the map is persisted before its event is published, which is what makes the bus a race-free choke point. When per-device kill-switch mode (#235) lands, its `block` mode belongs in the snapshot's down-tunnel gate: a block-mode device must keep failing rather than fall back.
- **The wildcard record is a real DNS record with real blast radius.** `*.<fqdn> → LAN IP` answers *every* unclaimed subdomain of the box's FQDN for LAN clients. It is a `System`-source record the daemon owns and removes on disable. Seeding it is **non-fatal** — a local-DNS failure logs and continues, since the grant is already persisted and roaming never consults the LAN view; LAN clients then hairpin via the WAN.
- **Nothing pre-registers.** Like a **Remote peer**, a device must already have been discovered on the LAN to be granted — the grant hangs off an existing `Device` row, which is what makes filter profiles, Network Zone enforcement and query attribution apply with no parallel path.
- **The naming fix is now load-bearing on docs.** "Tunneller" (two Ls, matching the `/tunneller/…` wire path) is the relay; "Private DNS" is the feature. The `TunnelerConnector` / `TunnelerClient` Rust identifiers remain one-L — a cosmetic straggler deliberately left alone rather than churn a rename through the cloud clients for a docs change.
- **Reversibility is good.** Everything is additive: a new listener, a new grants table, a second `dest_port` on an existing relay, and one new system DNS record. Disabling removes the record and stops the listener; the Tunneller keeps running if inbound WireGuard is on and stops if it isn't.
