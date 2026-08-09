# Wardnet Domain Glossary

## Surfaces

**Admin site** — The full desktop web admin. Served at `<vanity>.my.wardnet.services/admin/`. Not a PWA; intended for desktop use only. Source package: `source/admin-site`.

**User PWA** — Installable mobile app for non-admin household members. Served at `<vanity>.my.wardnet.services/app/` (the bare origin root permanently redirects there). Scope: self-service only (own device routing, own DNS stats, own connection status). Cannot manage other devices.

**Admin mobile PWA** — Installable mobile app for admins. Served at `<vanity>.my.wardnet.services/admin-app/`. Scope: daily operational tasks (device management, tunnel status, power actions). Not a replacement for the admin site; configuration work (DHCP, filter profiles, tunnel creation) stays on the desktop.

## Identity and access

**Device-keyed** — Identified by MAC address / LAN IP. Non-admin users have no credentials; their identity is their device on the network. Push subscriptions and self-service routing rules are device-keyed.

**Admin session** — Credential-based (username + password). Required for any admin surface. Push subscriptions on the admin mobile PWA are **admin-account-keyed** (to the admin account UUID, not the ephemeral session token, so they survive session rotation and logout).

**Admin lock** — Flag set by an admin on a device that prevents the device owner from changing their own routing rule. Read-only state visible in the user PWA.

## Features

**Setup wizard** — The first-run flow on the admin site. Ten linear steps — Admin, Network, DHCP, Router, DNS, Tunnel, Policy, HTTPS (remote access), Review, Done — rendered in a fixed-shape "guided rail" card (step rail + scrollable body + docked footer CTA). Progress is server-authoritative: the daemon persists `wizard_step` and every transition goes through `POST /api/setup/advance`. Each step commits its changes immediately; the Review step is a read-only summary, not a deferred apply.

**Wizard rewind** — Navigating the setup wizard backward to an already-visited step (rail row click, mobile Back link, or a Review "Edit" link). A rewind is a normal `advance` call to an earlier step; the daemon allows it down to the **rewind floor** and never out of the **terminal step**. Forward jumps of any distance remain allowed (e2e drains rely on this); the UI only ever offers backward jumps.

**Rewind floor** — The earliest step a wizard rewind may target: Network. The Admin step is unreachable backward because admin creation is one-shot (`POST /api/setup` returns 409 once an admin exists).

**Terminal step** — `wizard_step == completed`. Once reached, the wizard never reopens: rewinding out of it is rejected, and `SetupGuard` stops redirecting to `/setup`.

**Route verification** — User PWA feature. Makes a client-side request to an external IP geolocation API to show the device's current public IP and inferred country/location. Used to confirm that a VPN routing rule is working as intended. Client-side call is correct: the browser request travels through the Pi's per-device routing, so the result reflects the device's actual egress path.

**Device-keyed push subscription** — A Web Push subscription (VAPID) stored in the daemon's database keyed to a device record (MAC/IP). Allows the daemon to notify a specific device's browser even when the PWA is not open.

**Admin-account-keyed push subscription** — A Web Push subscription stored keyed to the admin account UUID. Admin-PWA notifications (tunnel offline, a device changing its own routing) fan out to every admin-account subscription. Keyed to the account rather than the session so a subscription outlives session rotation/logout; an explicit unsubscribe (or a 404/410 from the push service) is what removes it.

**Notification feed** — Daemon-persisted record of admin-audience push notifications (issue #482), shown on the admin-PWA System screen. Written before delivery fan-out — it records "what happened", not "what was delivered" — so entries exist even with zero subscriptions. Admin-audience only (device-keyed pushes are never persisted), count-capped (oldest pruned on insert), and shared across admin accounts: Clear deletes the feed for every admin. Each entry carries a `kind` tag and a kind-driven `subject_id` (device UUID for device kinds, tunnel UUID for tunnel kinds).

## Routing

**Routing target** — Where a device's traffic egresses: a specific **tunnel**, **direct** (bypass all tunnels, use the WAN), or **default** (explicitly defer to the gateway's default policy). A device's *current* routing target is its per-device rule if one exists.

**Routing rule** — A per-device binding of a device to a routing target, created by an admin or by the device owner (self-service). At most one rule exists per device.

**Default policy** — The gateway-wide fallback applied to a device that has **no** routing rule of its own. A device following the default policy is distinct from one whose rule's target is explicitly *default*: the former has no rule (its current routing target is absent/`null`), the latter has a rule that names *default* as the target. Both ultimately follow the gateway policy, but only the latter is a persisted choice.

**Routing profile** — A named bundle of **domain routing rules** (issue #241), the routing sibling of a **DNS filter profile**. Assigned to a device in **priority order** (many-to-many, with a device-local `position`); when several assigned profiles match a resolved domain, the one earliest in the device's order decides — specificity does *not* override order (the operator ranks profiles the way they'd rank an ordered firewall rule list). Within the winning profile the most-specific matching pattern wins. Orthogonal to the per-device **Routing rule**: a profile overrides the route for its *matched destinations* only, and a device with no profile keeps its plain routing. See [0024-domain-routing-profiles.md](docs/adr/0024-domain-routing-profiles.md).

**Domain routing rule** — One `pattern → target` entry in a **Routing profile**. `pattern` is glob/suffix (`*.netflix.com` covers the apex and any subdomain; a bare name is exact); `target` is a **tunnel** (route the domain through it) or **direct** (carve the domain out of the device's tunnel back to the WAN). Enforced by the DNS→routing hook: when the local resolver answers a matched domain for a device, `RoutingProfileService::note_resolution` queues the resolved IPs and `RoutingService::route_resolved_domain` installs `ip rule from <device_ip> to <resolved_ip>/32 lookup <table> priority 2000` — above **Switchback** (1000) yet below the kernel per-tunnel source rules, so the per-destination decision wins for that IP. Each rule is leased for the DNS record's TTL (clamped `[30s, 1h]`) and GC'd on expiry; IPv4-only in v1. Shared-CDN IPs may pull unrelated traffic (documented, accepted).

## Network Zones

**Network Zone** — A named policy bucket a device belongs to (**exactly one**) that gates the device's allowed **routing targets**, its reachability of the Pi's admin surfaces, and (Phase 2+) its network isolation. Deliberately **not** the DNS *authoritative local zone*: unrelated concept, hence the qualifier "network." Three are seeded by the daemon — **Trusted**, **IoT**, **Guest**. Zones *constrain* the routing choice (via `allowed_targets`, a coarse list of `direct` / `tunnel` kinds) but do not make it — a device in a tunnel-only zone is rejected (409) when someone tries to set its target to direct. See epic #244 and [0018-network-zone-isolation.md](docs/adr/0018-network-zone-isolation.md).

**Zone isolation stance** — The **cross-zone** rung of the guarantee ladder a zone sits on: **shared subnet** (nftables egress + admin-UI gating only; peer isolation delegated to the AP) or **isolate members** (per-device `/32` + proxy-ARP; requires Wardnet-owned DHCP). Only rungs with backing issues exist; **VLAN is a non-goal**, not a variant. Both rungs are *enforced* as of #737: shared-subnet by the #736 per-device egress/admin-UI gates, and — once a zone is given a **Zone subnet** in **DHCP-mode** — cross-subnet isolation by the L3 enforcer.

**Zone subnet** — The per-zone CIDR that gives a zone its own address space (CI-3 #737). `None` keeps the zone on the **base LAN subnet** (the Pi's own subnet, shared with every other `None` zone — the #736 behavior). `Some(cidr)` makes the Pi alias a gateway (`.1`) on the LAN interface, hand DHCP leases from the cidr, and default-deny traffic between that subnet and every other subnet. Admin-assigned and opt-in; seeds ship `None`, so upgrades change nothing until a subnet is set. Requires **DHCP-mode**. See [0021-network-zone-deep-isolation.md](docs/adr/0021-network-zone-deep-isolation.md).

**DHCP-mode** — The condition (`dhcp_enabled`) under which Wardnet is the network's DHCP server and therefore controls addressing. All **Zone subnet**–based isolation (per-zone subnets, gateway aliasing, cross-subnet deny, isolate-members proxy-ARP) is gated on it; when Wardnet is *not* the DHCP server the L3 enforcer no-ops and isolation degrades to the #736 shared-subnet gates. A subnet configured while DHCP is off is recorded but inactive until DHCP is enabled.

**Casting preset** — A built-in **Cross-zone exception** service set that opens the ports **receiver-pull** casting needs across a zone boundary: mDNS 5353/udp, SSDP/DLNA 1900/udp, Chromecast 8008-8009/9000/tcp, AirPlay 7000/7100/tcp, and the Google Home device-listing port 8443/tcp, bidirectionally. It deliberately does **not** open the sender's live-media ports — see **Mirroring preset** for why. The traffic model it targets is the receiver fetching media from the cloud (YouTube, Netflix, AirPlay-from-cloud, DLNA, Spotify Connect), where only the sender's control channel crosses the zone boundary, so a fixed short port list suffices. There is **no mDNS reflector**: all zones share one L2 segment (IP aliasing, not VLANs), so discovery multicast already crosses the subnet split; the exception's allow-rules carry the routed *unicast* stream that the cross-subnet deny would otherwise drop. Two further conditions must hold for that unicast to actually flow, and the preset's port list alone does **not** guarantee them: a **tunnel-bound** sender needs **Switchback** so its cast packets reach the forward chain instead of being swallowed up its tunnel, and the *reply* stream — which the stateless `dport` allow-rules cannot match (replies carry `sport`, not `dport`) — is carried by a `ct state established,related accept` at the top of the isolation chain.

**Mirroring preset** — A built-in **Cross-zone exception** service set for the **sender-push** traffic model: screen/desktop mirroring (Cast tab-mirror, AirPlay mirroring) and local-file casting (VLC), where the sender is the *live media source* rather than a remote control. Because these negotiate media ports dynamically (Cast UDP 32768-61000, AirPlay 49152-65535, VLC's own HTTP port, …) and community experience is that mirroring "only works with the firewall wide open", the preset opens **all ports, TCP and UDP (1-65535)** between the two endpoints — and is therefore **restricted to device-to-device exceptions** (both endpoints must be a specific **Device**, not a whole zone), so the wide surface is bounded to exactly the two devices that mirror. It also depends on the **Cross-zone NAT exemption**: the sender's real IP must be reachable by the receiver, so source-NAT between the pair is suppressed. **Miracast/Wi-Fi Direct is out of scope** — it forms its own Wi-Fi Direct radio link outside the routed LAN and cannot cross zones at all.

**Smart home preset** — A built-in **Cross-zone exception** service set for the third traffic model, distinct from the two above: **client-initiated unicast control of a LAN appliance**, with no media stream at all. A phone runs the vendor's app and speaks the vendor's local API straight at the device — Govee 4001/4002/4003 udp, Tuya/Smart Life CoAP 5683/udp and 6668/tcp, LIFX 56700/udp, ESPHome 9123/tcp, local MQTT/TLS 8883/tcp — plus the *unicast* legs of mDNS 5353/udp and SSDP 1900/udp for apps that query a known device IP directly. **Must be used bidirectionally**: the device→client leg (Govee's 4002) is a *fresh* flow, not a conntrack reply, because the multicast discovery it answers never transited the Pi, so no conntrack entry exists for it. It deliberately **omits TCP 80/443** even though several vendors (Shelly, Tasmota, ESPHome's web server) expose local control there: applied zone-to-zone, those two ports reach *every* HTTP listener in the peer zone — a NAS, a camera, someone's dev server — which is a general web hole wearing a smart-home label, so it stays a separate, deliberate choice (the admin UI's `web` bundle). Unlike the **Mirroring preset** it is **not** restricted to device-to-device: the port list is narrow, and zone-scoping is what lets it work for an admin who cannot identify which of their devices is the bulb. Like casting, discovery multicast is **not** carried by any rule it emits — see the **Casting preset** entry and ADR 0021 §5.

**Cross-zone NAT exemption** — The postrouting rule set that stops cross-zone **exception** traffic from being source-NAT'd to the gateway alias by the base `oifname <lan> masquerade` (which exists for WAN egress but incidentally catches inter-zone LAN traffic, since the WAN and the zones share one interface). A dedicated `zone_natexempt` chain, jumped from postrouting **before** the masquerade, carries an `accept` (terminal for the hook, so masquerade is skipped) for each exception's `from_cidr ↔ to_cidr` pair, both directions. It preserves the sender's real IP end-to-end — necessary for the **Mirroring preset**'s sender-push flows and honest in the logs for the rest. Scoped to exception pairs (like **Switchback**); non-exception cross-zone traffic is dropped by **Zone packet enforcement** regardless, so there is no security cost to un-NATing.

**Switchback** — The routing counterpart to a **Cross-zone exception**. A **tunnel-bound** device's source `ip rule` directs *all* its traffic to the tunnel's routing table, whose only route is `default dev wg_wardX`; its cross-zone LAN unicast therefore matches that default and is sent up the tunnel, never reaching the forward chain where the exception's allow-rules live. Switchback restores local delivery: for each peer the device's zone has an exception with, a higher-precedence source+destination `ip rule` — `from <device_ip> to <peer_cidr> lookup main`, at a fixed priority (1000) below the kernel-auto-assigned tunnel rules — returns just that traffic to the main table. **Scoped to exceptions** (routing encodes the same zone-pair policy the firewall does) and **subnet-granular** (the firewall still decides which *ports* pass). It is computed by the **Zone packet enforcement** layer from the same exceptions and pushed to the routing service, which materialises the `ip rule`s only while the device is tunnel-bound and tears them down on unbind, IP change, or removal. See [0026-switchback-and-cross-zone-return.md](docs/adr/0026-switchback-and-cross-zone-return.md).

**Zone packet enforcement** — The #736 layer that makes a zone bite on a flat shared subnet: a per-device **egress gate** (forward-chain drop of egress via a routing-target kind the zone forbids — `wg_ward*` for tunnel, the WAN interface for direct) and an **admin-UI gate** (input-chain TCP-reset of device→Pi :443/:7411 when `admin_ui_reachable = false`, leaving DNS/DHCP to pass). Rules are keyed by device IP via nftables comment UDATA so they survive restarts, and are live-reloaded on zone/device events. **Honest limit:** same-subnet peer↔peer traffic is *not* affected — the daemon never sees it on a flat L2 segment; that's the AP's job (or the isolate-members rung). See [0019-network-zone-enforcement.md](docs/adr/0019-network-zone-enforcement.md).

**Member isolation** — An **orthogonal** toggle (independent of the isolation stance): within a zone that has a **Zone subnet**, also isolate **same-zone peers** from each other. Enforced (CI-3 #737) by handing each member a `/32` lease and enabling proxy-ARP on the Pi, so peer↔peer traffic is forced through the gateway where the L3 enforcer drops it. **Cooperating-devices-only**: a device that self-assigns a wider mask can ARP a peer directly — breaking that would need ARP spoofing, an epic non-goal. No effect without a Zone subnet + DHCP-mode.

**Default zone** — The protected **anchor** ("home") zone — full trust, deletion-guarded. Exactly one. It is **Trusted**. Distinct from the *default zone for new devices*.

**Default zone for new devices** — Where a freshly-discovered device is assigned at discovery time. Exactly one. It is **Guest** — nothing is auto-trusted. Membership is **sticky**: set once at insert from this flag and never re-resolved, so re-pointing the flag later does not move existing devices. Both default flags move only by *promoting* another zone (you cannot clear a default, only relocate it).

**Cross-zone exception** — An admin-granted allowance for one endpoint to reach another across an otherwise-isolated zone boundary (e.g. a phone casting to a TV in the IoT zone). CI-3 #737. Each of `from`/`to` is a **device** (matched as its `/32`) or a whole **zone** (matched as its subnet); the service is a named preset (the **Casting preset**) or a custom port list; rules are stateful (conntrack allows the return path) with an explicit `bidirectional` flag. The L3 enforcer emits an exception's allow-rules **ahead of** the cross-subnet deny, so it re-opens exactly the named flow. Removing an exception revokes it live (conntrack flush).

## Device identification (issue #1099)

**Identification signal** — Any observed fact that helps name a device: its OUI, a DHCP option 12 hostname, an option 55 parameter-request list, an option 60 vendor class, an advertised mDNS service type, or a port that answered an admin-triggered probe. Deliberately distinct from the device's **name**, which is what an admin typed; a signal is something the network told us. Signals are multi-valued and append-only — a device can advertise several mDNS services, and each is independent evidence rather than a field that overwrites the last. Stored in `device_signals`, never on the device row.

**Randomized MAC** (privacy MAC) — A locally-administered address (bit 1 of the first byte set) that a device presents to avoid being tracked across networks, instead of its burnt-in address. Explicitly **not a manufacturer**: it says how the device chose to present itself, not who built it. The term exists to kill a conflation — the string `"Randomized MAC"` used to be written into the `manufacturer` column, making a privacy MAC indistinguishable from a real vendor name to every consumer of that column. It is now the boolean `devices.is_randomized`, and a randomized address has no manufacturer at all.

**Vendor catalog** — The curated, versioned data file (`wardnetd-data/data/vendors.toml`) mapping a vendor to the marks that identify it: OUI prefixes, TCP ports, mDNS service types and DHCP vendor-class strings. The single extension point for device identification — teaching Wardnet about a new manufacturer is an edit here and nothing else. Distinct from the IEEE OUI database, which is imported wholesale and never hand-edited. A catalog **OUI override** names a block whose IEEE listing is the placeholder `Private`; because that asserts something the registrant deliberately withheld, such a match is recorded as `manufacturer_source = 'catalog'` and always rendered as a hedge ("Likely Govee"), never as a registered fact. See [0025-device-identification.md](docs/adr/0025-device-identification.md).

**Derived MAC** (neighbour MAC) — One of the several addresses a single chipset derives from one base MAC. Espressif assigns Wi-Fi STA, Wi-Fi AP, Bluetooth and Ethernet as base+0/+1/+2/+3. This is why the MAC printed in a vendor's mobile app — often the **Bluetooth** one — is not the address the device associated with over Wi-Fi, and why an admin comparing the two is unknowingly comparing different identifiers. Wardnet's device search answers this by offering devices within ±4 of a missed exact MAC search as clearly-labelled *possible* matches.

## Remote access (inbound WireGuard + published access)

**Remote peer** — A `Device`, previously discovered on the LAN, that an admin has explicitly granted an inbound WireGuard credential (issue #810). Not a separate identity: the credential (`inbound_wg_peers.device_id`, one per device) attaches to the device's existing row, so it participates in **Routing rule**, **Network Zone** enforcement, and DNS capture exactly like any LAN device — there is deliberately no separate device concept for it. There is no way to grant remote access to a device that has never connected to the LAN; the MAC needed to link the credential can't be known in advance, since modern OSes randomize the MAC presented to a network the device hasn't associated with before. Distinguished from a device currently on the LAN only by its live `connection_mode` (`lan` | `remote`), which flips with whichever path — LAN or tunnel — most recently observed the device; it is not a permanent record of how the device was first discovered.

**Inbound WireGuard server** — The daemon-managed WireGuard listener (a persistent, multi-peer interface, distinct from the single-peer-per-interface outbound tunnels used to reach VPN providers) that accepts connections from **Remote peers**. Issue #266. WAN reachability is provided by the **Tunneller** relay, not a LAN port-forward — see [0022-inbound-wireguard-and-published-access.md](docs/adr/0022-inbound-wireguard-and-published-access.md) and wardnet-cloud ADR-0015.

**Published access** — The umbrella feature letting an admin make an internal LAN device's service reachable by something other than being physically on the LAN. Two mechanisms, chosen per published item:
- **Address forward** — raw L4 TCP/UDP forwarding to a device's `ip:port`.
- **App forward** — L7 HTTP(S) reverse-proxying to a device's `ip:port`, reachable via a subdomain of the gateway's DDNS domain (e.g. `bitwarden.home1.my.wardnet.services`), requiring a wildcard certificate on that domain.

Each published item also has a **visibility**, independent of its mechanism:
- **Tunnel-only** (default) — reachable only from an authenticated **Remote peer**; the daemon source-IP-gates the forward the same way the Network Zone **admin-UI gate** already TCP-resets disallowed traffic.
- **Public** — reachable from the open internet, no **Remote peer** required. An explicit, admin opt-in per item, mirroring how Tailscale separates private `Serve` from public `Funnel`.

See [0022-inbound-wireguard-and-published-access.md](docs/adr/0022-inbound-wireguard-and-published-access.md).

## Private DNS (issue #910)

**Private DNS** — The user-facing Premium **feature**: a phone resolves through the Pi everywhere — on the LAN and on cellular — over encrypted DNS, with no VPN. Android consumes it through its built-in **Private DNS** setting, iOS through an encrypted-DNS **configuration profile**. Deliberately *not* a synonym for the **Tunneller**, which is the relay infrastructure one of its two paths happens to ride on: Private DNS on the LAN never touches the cloud at all. **DoT-only in v1** (Android's setting speaks nothing else); DoH waits on the `:443` relay (#816, then #920). Queries carry the granted device's own **DNS filter profile** and are attributed to it in the query log with `protocol=dot` — there is no shared "roaming" bucket. See [0029-private-dns-dot.md](docs/adr/0029-private-dns-dot.md).

**Private-DNS grant** — The per-device authorization, minted by an admin (`private_dns_grants`, one row per device, `UNIQUE` on `device_id`). Granting mints the device's **device hostname** token; revoking deletes the row *and* publishes `PrivateDnsGrantRevoked`, which tears down that device's live `DoT` sessions — without it a revoked phone would keep resolving until it happened to reconnect, since the token is checked only at handshake. Like an **inbound WireGuard** credential (**Remote peer**) it is a grant *on* an already-discovered `Device`, not an identity of its own. Admin-grant-only in v1; a user request→approve flow is #919.

**Device hostname** — The secret name a granted device dials: `<token>.<canonical-FQDN>` (e.g. `k7m2q…4x.happy-einstein.my.wardnet.services`), where `token` is 80 bits of CSPRNG entropy in lowercase base-32 (16 label characters). It is covered by the existing **per-user wildcard certificate**'s `*.<vanity>.my.wardnet.services` SAN, so — unlike a per-device certificate — it is **never published to a CT log** and the token stays secret. It is simultaneously the credential and the routing key: see **SNI = auth + attribution**. On the LAN a **split-horizon** wildcard `*.<fqdn> → LAN IP` system record (seeded on enable) points it at the Pi directly; roaming, the public wildcard lands it on the regional edge.

**SNI = auth + attribution** — How the `DoT` `:853` listener identifies a connection: the TLS **SNI** *is* the credential. The listener resolves the SNI's token label to a live **Private-DNS grant** — that both authenticates the connection and names the device whose filter profiles and query-log attribution apply. It is the AdGuard/NextDNS/ControlD pattern, and it is why the resolver is **closed**: an unknown token, or the bare apex, closes the connection before any DNS is read. Apex-only serving would not do, because the apex slug *is* public via CT logs.

**Private-DNS profile** — The signed iOS `.mobileconfig` a granted device downloads (`GET /api/private-dns/me/profile`, device-keyed by source IP): a `com.apple.dnsSettings.managed` payload carrying `DNSProtocol=TLS` and the **device hostname** as `ServerName`. **CMS-signed with the box's live Let's Encrypt leaf**, so iOS renders it "Verified" rather than "Not Signed". Three deliberate omissions: no `ServerAddresses` (the hostname must be free to split-horizon differently on LAN vs cellular), no `OnDemandRules` (an always-on rule strands the phone behind hotel captive portals), and no MDM — payload identifiers are `UUIDv5`-derived from the hostname so a re-download *replaces* the profile instead of stacking a second copy.

**`private_dns_granted` notification** — The device-keyed push an admin fires from the granted-device modal ("Send to device", `POST /api/private-dns/grants/{device_id}/notify` → `{ delivered }`), deep-linking the user PWA to `/settings#private-dns`. Device-audience (`owner_kind = device`), so — like every device-keyed push — it is **not** written to the **Notification feed**. Purely an onboarding convenience, never the only path: the hostname and the per-platform steps are always shown in the admin granted-modal and in the user PWA. `delivered: false` is a `200`, not an error — it means the household member simply hasn't enabled notifications.

## Local DNS

**Authoritative local zone** — A named DNS domain (e.g. `lan`, `home`) the gateway answers for directly rather than forwarding upstream. Single-label names are valid. Zones group custom records; deleting a zone keeps its records but unlinks them.

**Custom DNS record** — A user-defined record (`A`, `AAAA`, `CNAME`, `TXT`, `MX`, `SRV`) mapping a domain to a value, answered locally. May belong to an authoritative local zone or stand alone (unzoned).

**Forwarding rule** — Also called *conditional forwarding*: a `domain → upstream` override that sends queries under a specific domain to a chosen upstream resolver instead of the default upstream pool (e.g. `corp.example.com → 10.0.0.53`). It is the per-domain form of the gateway-wide **Forwarding** resolution mode; the latter forwards *all* queries to the default upstreams.

**Private reverse DNS (PTR)** — The gateway answers `in-addr.arpa` PTR (reverse) lookups for private/internal IPv4 ranges — RFC 1918 (`10/8`, `172.16/12`, `192.168/16`), link-local (`169.254/16`), and RFC 6598 CGN (`100.64/10`) — locally instead of forwarding them upstream (RFC 6303). A known address (one named by a forward A record, including DHCP `.lan` records) resolves to that hostname; an unknown address in a private range gets an **authoritative** NXDOMAIN with a synthetic SOA so the negative is cacheable. Because these are local answers they bypass the per-client **rate limiter**, which guards only upstream-bound queries — so a device that floods private PTR lookups is always answered and never driven into a REFUSED retry loop. A **forwarding rule** on the reverse name still overrides this.

**Forwarder selection** — How the configured upstreams are used, pool-wide, in **Forwarding** mode. Three modes: **Failover** (the default) uses the servers in their listed order — send to the first, fall back to the next only on failure — so the list order is a priority and the resolver uses `UserProvidedOrder`; **Fastest** forwards to all upstreams and routes to the fastest-responding one by live round-trip time (`QueryStatistics`), ignoring order; **Single** forwards *exclusively* to one chosen upstream (identified by its address, stored as `single_upstream`), the others unused. In Single mode, removing the chosen server from the list is rejected until a different server is selected or the mode changes. Distinct from a **Forwarding rule**, which is per-domain — forwarder selection is pool-wide.

**Upstream latency probe** — A background measurement, independent of live query traffic, that periodically resolves a fixed benign name against each configured upstream individually and folds the round-trip time into a per-upstream rolling average. It exists because the shared resolver pool does not expose which upstream answered a real query, so per-server latency can't be attributed from the query log. Surfaced (per address, with a reachable flag) in the DNS status response for the admin UI; it feeds display only — **Auto** routing uses the resolver's own live timing, not this probe.

**Zone provenance** — Whether an **authoritative local zone** was created by an admin (`manual`) or seeded by the daemon (`system`). A **system zone** — currently only the seeded `.lan` zone — cannot be deleted: the admin API rejects the attempt and the UI hides the delete control. Manual zones are freely deletable. (Custom records carry an analogous provenance: `manual`, `dhcp`, or `system`.)

**System DNS record** — A **custom DNS record** the daemon maintains for itself (provenance `system`), as opposed to admin- or DHCP-created ones. Two exist: the **split-horizon** record (the canonical FQDN → the Pi's LAN IP) and the convenience `wardnet.lan` → Pi LAN IP. The daemon owns their lifecycle; a DHCP-sourced upsert can never overwrite them.

**Split-horizon resolution** — Answering the public **canonical FQDN** with the Pi's *LAN* IP for clients querying through the gateway, while the same name resolves to the **Public WAN IP** on the public internet. Lets a LAN device reach the Pi directly (and get the valid certificate for that name) instead of hair-pinning out through the WAN.

## Infrastructure

**DDNS service** — Wardnet-operated, **regional** service (see *Service decomposition*) that publishes DNS for an enrolled **network**: the A record for its **slug** under `<slug>.my.wardnet.services` and the `_acme-challenge` TXT records the Pi's own ACME client needs, so Let's Encrypt can issue a certificate via DNS-01 without the user needing a domain or DNS-provider credentials. The cert private key is generated on the Pi and never leaves it. Reached through the **per-service cloud client** with an **identity JWT** as bearer; it is a **premium-tier** capability whose access is gated by whether that JWT mints (see **Suspended**), not by a separate lease.

**Remote access (setup step)** — The setup wizard's HTTPS step (`wizard_step == remote_access`, between Policy and Review). The operator picks a **DnsProvider** — **wardnet** (default) or **BYOD-Cloudflare** (their own domain + API token). The wardnet path is a three-step enrollment: request an **enrollment code** by email → submit the code to *enroll* (bind the **daemon identity** to the **tenant**) → pick a **slug** with a live availability check and *register the network*. The daemon registers synchronously, then issues the certificate in the background (`POST /api/ddns/{enrollment-code,enroll,register}` / `/cloudflare` → `mark_provisioning_started` → detached `ensure_certificate`). Non-blocking: the step is skippable and completes even offline, with issuance retried later from Settings. Progress is the **TLS provisioning phase**.

**DnsProvider** — The daemon-side abstraction over the publish side of DDNS: a provider bound at construction to one target that can `upsert_a` (publish the A record), `set_txt` / `delete_txt` (publish one *or more* `_acme-challenge` TXT values at the one challenge name simultaneously, then remove all of them — multi-valued because a **per-user wildcard certificate** authorizes two SANs through the same name), and `teardown` (remove the published presence). Two implementations: the **wardnet** provider (default; talks to the regional **DDNS service** through the **per-service cloud client**, authenticated by the **daemon identity**'s **identity JWT** + Ed25519 **PoP**; `teardown` removes the network's upstream presence via the cloud) and the **Cloudflare** provider (Bring-Your-Own-Domain, talks to the user's Cloudflare zone directly). The cert/signing key never leaves the Pi under either.

**Region slug** — A short identifier for a wardnet region (e.g. `use1`). It selects which regional **DDNS service** endpoint the daemon talks to (resolved through the **region catalog**) and is returned at registration for display.

**Region catalog** — The built-in, daemon-shipped table mapping each **region slug** to its regional **cloud gateway** base URL (an `api.<region-slug>.wardnet.network` FQDN). The cloud cannot supply this (each regional gateway is region-specific), so the daemon must already know it. At registration the daemon probes every catalogued region's health endpoint and registers against the lowest-latency one. (The production FQDNs are a daemon constant pending infra confirmation.)

**Cloud gateway** — The per-scope north-south edge the daemon HTTPS-es into to reach wardnet-cloud (wardnet-cloud ADR-0014 / inforge ADR-0032): one **global** gateway fronting the **tenants service**, and one gateway per region fronting that region's **regional plane** (DDNS + Tunneller). The target service is the first path segment (`/tenants/…`, `/ddns/…`, `/tunneller/…`) and the gateway forwards the path unmodified, so the daemon's Ed25519 **PoP** signature is computed over the full prefixed path it puts on the wire. TLS is an ordinary public server certificate; the daemon presents no client certificate (the cloud's internal mesh mTLS is invisible to it).

**Vanity name** — A user's chosen slug (e.g. `alice`) forming the flat, region-free user host `<vanity>.my.wardnet.services`. Validated `[a-z0-9-]`, 3–32 chars. The region is deliberately *not* in the name (it lives in the record's value and in infra names only), so a user can be migrated between regions without changing their host, bookmarks, or certificate. Per-service hosts nest under it: `<service>.<vanity>.my.wardnet.services`. See [0005-two-domain-strategy.md](docs/adr/0005-two-domain-strategy.md).

**Global naming authority** — The strongly-consistent registry of **slugs**, owned by the global **tenants** service (a *separate global Postgres*, distinct from each region's operational DB) whose `UNIQUE` slug constraint *is* the cross-region allocation lock. Because slugs form one flat global namespace, a single authority must answer availability and guarantee one-winner allocation across regions. The daemon never touches it directly — it asks the **tenants** service over HTTP (`GET /api/ddns/check` → tenants availability). Availability is a read against this registry — *not* DNS and *not* a cache. DNS stays purely the resolution layer. Deliberately not Cloudflare KV (eventual consistency breaks atomic reserve) / D1 / DNS-as-registry. See [0004-global-naming-authority.md](docs/adr/0004-global-naming-authority.md).

**Network registration** — Claiming a **slug** for an enrolled **tenant** and binding it to a **network** on the chosen region. Runs against the **tenants** service after *enroll*: it atomically reserves the slug in the **global naming authority** (a conflict means taken) and provisions the regional **network** record; the wildcard `*.my.wardnet.services` is infra-provisioned and the per-user cert is daemon-issued. The daemon persists `tenant_id` / `network_id` / `slug` in `system_config` on success.

**Per-user wildcard certificate** — One certificate per **vanity name** carrying two SANs — the apex `<vanity>.my.wardnet.services` (serves the PWA + admin site) and the wildcard `*.<vanity>.my.wardnet.services` (per-service gateway hosts) — issued via ACME DNS-01. Both SANs authorize through the *same* `_acme-challenge.<vanity>.my.wardnet.services` TXT name, so their two challenge values are published *simultaneously*; the **DnsProvider** challenge path is therefore multi-valued. Stable across region migrations (no region in the SAN); new services need no new cert.

**Public WAN IP** — The home's internet-facing IPv4 address, discovered by the daemon via an external echo service over its default (WAN) route. This is what DDNS publishes — explicitly *not* a tunnel exit IP (a device's egress address when routed through a VPN tunnel), which the daemon measures separately for routing diagnostics.

**Resolution check** — A diagnostic that confirms the *public* internet resolves the **canonical FQDN** to the IP the daemon last published. The daemon queries a fixed pair of public resolvers (Cloudflare `1.1.1.1` + Google `8.8.8.8`) **by IP over DoH**, which deliberately bypasses the daemon's own **split-horizon** record (that record only answers LAN clients). It has three outcomes: **match** (public DNS agrees with the published IP — propagation complete), **mismatch** (resolves to a different IP — stale record or wrong config), and **pending** (no A record yet — the normal state in the propagation window right after registration). It compares against the *last published* IP, not the current WAN IP; detecting a WAN-IP change is the DDNS runner's job, not the check's. Read via `GET /api/ddns/resolution-check`.

**Path-based app routing** — All three surfaces are served from a single host (`<vanity>.my.wardnet.services`) at different paths (`/app/`, `/admin-app/`, `/admin/`; the bare root `/` permanently redirects to `/app/`). Each PWA has its own `manifest.json` with a distinct `id`, `scope`, and `start_url`. The scopes must be **siblings**, never nested: Chrome refuses to install a PWA whose page sits inside an already-installed app's scope (a distinct manifest `id` does not override the containment check, and Android ignores `id` entirely), which is why the user PWA lives at `/app/` rather than owning the origin root.

**Caddy** *(retired)* — Formerly the reverse proxy that terminated TLS in front of both surfaces. It is no longer used anywhere: the daemon does **Daemon-owned TLS termination** (DNS-01), and the cloud edge does L4 **edge SNI demux** in front of the regional services. Retained here only so older docs and issues that mention "Caddy" resolve to "the thing replaced with in-process termination / SNI routing."

**Daemon-owned TLS termination** — `wardnetd` terminating TLS itself on port 443, replacing Caddy. The daemon obtains a certificate via ACME DNS-01 (publishing `_acme-challenge` TXT through the **DnsProvider**), serves `:443` with it, hot-swaps it on renewal, and 308-redirects `:80`→`:443`. The leaf private key is generated on the Pi and never leaves the LAN; cert + key are stored only through the **SecretStore** abstraction.

**Placeholder cert** — A throwaway self-signed certificate generated at boot to seed the `:443` listener before a real certificate has been issued, so the port is always bound (TLS can't handshake without *a* cert). It is never trusted by clients: while it is in use the **TLS provisioning** gate is closed and every `:443` route returns `503`, pointing the operator at the plain-HTTP `:7411` fallback.

**TLS provisioning** — The boolean state of whether the daemon is serving a real (vs **placeholder**) certificate on `:443`. A shared `provisioned` flag gates a 503 guard on the `:443` app; it flips to `true` when the first real certificate is activated. Pre-provisioning, `:7411` plain HTTP is the honest admin surface.

**TLS renewal** — The background re-issuance of the certificate before expiry. `TlsService::ensure_certificate()` is a single idempotent operation — issue-if-missing or renew-if-within-30-days — driven on a 12-hour tick by `TlsRenewalRunner` and inert until DDNS (and therefore the public FQDN) is configured.

**TLS provisioning phase** — A coarse, persisted progress signal for certificate issuance — `idle` → `issuing` → `issued` / `failed` — surfaced to the **Remote access (setup step)** and the dashboard so an operator can watch the (otherwise opaque) ACME round-trip and see any failure. Distinct from **TLS provisioning** (the boolean serving-a-real-cert gate): the phase narrates the *process*, the gate names the *outcome*. A live cert reads as `issued` even with no marker; `failed` carries the last error. Read via `GET /api/tls/status`.

**Canonical FQDN** — The single public hostname the gateway is reached by and holds a valid certificate for: the **domain the active certificate was issued for** (`tls_cert_domain`), not merely the configured DDNS hostname. The two are normally identical and diverge only transiently (issuance lag, a domain change before re-issuance, an ACME failure); the cert domain is authoritative precisely because it is the name that currently works. It is the primary entry point (PWA `start_url`/`scope`, bookmark) and the target of the **short-name redirect** and the **split-horizon** record. Absent (no cert yet) ⟹ both are inert.

**Short-name redirect** — The `:80` listener's behaviour of 308-redirecting a request arriving under a short or LAN name (`wardnet`, `wardnet.lan`, the bare LAN IP) to `https://<canonical-FQDN>`, so the client lands on the name with a valid cert. When no canonical FQDN is provisioned, or the request already targets it, the redirect is a plain same-host HTTP→HTTPS upgrade.

**Serving identity** — The daemon's current `:443` serving state — *which domain's certificate is live* — exposed to the unauthenticated `:80`/`:443` listeners through methods (`is_provisioned` / `canonical_fqdn`) rather than a shared flag or an admin-gated call. It is the hot-path projection of the authoritative served domain (`tls_cert_domain`, read by the API via `TlsService`); a non-empty serving identity is equivalent to **TLS provisioning** being complete.

**Tunneller** — Wardnet-operated, **regional** relay service (see *Service decomposition*) that gives a box WAN reachability without a port-forward. The daemon holds **one persistent** outbound WebSocket to its region's PoP (the only long-lived socket in a daemon otherwise built from fixed-interval poll loops); the PoP frames inbound flows back down it (`FRAME_CONNECT` with a `dest_port`, then bytes) and the daemon relays each to the right loopback listener. It is **infrastructure carrying features, not a feature itself** — spelled with **two Ls** throughout (matching the `/tunneller/…` wire path); the `TunnelerConnector` / `TunnelerClient` Rust identifiers are a one-L straggler, not a second concept. It carries: inbound-WireGuard UDP (#266/#809), `dest_port=853` **Private DNS** `DoT` (#913), and `dest_port=443` HTTPS SNI passthrough (reserved, closed until #816). The runner dials while *either* feature is enabled and tears down only when *both* are off, gating each inbound flow per-frame on its own feature.

**Edge SNI demux** — The cloud edge routing inbound connections to the right regional service by the client's TLS **SNI** (server name), at L4 — so multiple logical services (the regional **DDNS service**, the **Tunneller** PoP) can share one ingress without path-based coupling, and the tenant's own traffic passes through still-encrypted to terminate on the Pi. The daemon's only obligation is to address each service by its correct FQDN (from the **region catalog**); the edge does the rest. See [0017-per-service-cloud-clients.md](docs/adr/0017-per-service-cloud-clients.md).

## Release channels

**Release channel** — Which stream of daemon builds a box follows. The daemon
stores its choice in `system_config`; the update runner fetches
`<manifest_base_url>/<channel>.json` and installs only what that manifest
names. Three channels exist, in ascending order of risk: **stable**, **beta**,
**edge**. A channel is a *promise about vetting*, not about recency.

**Stable channel** — Reviewed, released builds with no pre-release suffix. The
default for every install.

**Beta channel** — Released builds carrying a `-beta.N` suffix. Cut through the
full release ceremony (release-notes doc, version bump, PR, signed tag), so a
beta build is *vetted*; it is simply newer.

**Edge channel** — Builds published straight from a branch by an on-demand
workflow, with **no review, no release notes, no version bump, and no test
gates** — deliberately, because the point is to put a candidate on real
hardware in minutes rather than an hour. Edge builds are signed with the same
production key, so the *channel* is still authentic; what's absent is any
promise that the code is good. Versioned `<base-calver>-edge.<run-number>`,
which sorts above every `-beta.N` of the same base and below the final release.
Gated by the deploy-time `[update] allow_edge_channel` flag: a box cannot be
put on edge without root on that box, and a box already on edge falls back to
beta at startup if the flag is removed. Never a destination for a real user —
an operator's testing loop. See
[0023-edge-release-channel.md](docs/adr/0023-edge-release-channel.md).

## Applying an update

**Pending tarball** — The signed release archive the daemon has downloaded,
verified, and set aside for the next start, rather than applied itself. It
lives in the daemon's own writable staging area, which means the unprivileged
daemon user can put *anything* there; nothing about its presence is trusted.
It is a request to swap, not a swap.

**Privileged swap** — The step that actually replaces the running daemon
binary, performed at the next start by a separate root-owned component before
the daemon is allowed to start. It re-verifies the **pending tarball** from
scratch — the daemon's earlier check is not taken on faith — and preserves the
outgoing binary so a **rollback** remains possible. If verification fails the
swap does not happen *and* the daemon is not started, so a box never runs a
binary that failed the check.

**Trust anchor** — The signing key the **privileged swap** verifies against,
fixed when the component was built and therefore not replaceable by anything
running on the box. It is the whole basis of the arrangement: the daemon user
can stage any bytes it likes, and the trust anchor alone decides which of them
ever execute.

**Rollback** — Returning to the binary that was live before the last
**privileged swap**. Requested by the daemon and carried out by the same
privileged component on the next start, for the same reason the swap is: the
daemon cannot write to that location itself. Available only while the previous
binary is still preserved — one step back, not a history.

## Reliability and watchdog (issue #214)

**HealthMonitor** — The daemon-side aggregator (in `wardnetd-services/src/health/`) that holds the registered **HealthCheck**s, re-runs them all on a fixed tick, debounces failures, and publishes an immutable **HealthSnapshot** through an `ArcSwap` for lock-free reads. It only *reports* status; recovery policy lives in the watchdog layers. Checks run concurrently with a per-check `tokio::time::timeout`, so one hung probe can't stall the cycle.

**HealthCheck** — A pluggable async probe (`name()` + `check() -> CheckOutcome`) adapting one subsystem into a cheap readiness signal. The four initial probes are **database** (`SELECT 1`), **liveness** (always UP — proves the loop schedules), **dns** and **dhcp**. The DNS/DHCP probes are **desired-vs-actual**: each reads its configured `enabled` flag (under an admin context, like the runners) and reports DOWN *only* when the service is enabled yet not running (a crash) — never for a deliberately toggled-off service, which would otherwise restart-loop the daemon. Must be non-blocking and never panic.

**HealthStatus** — The debounced verdict, `UP` or `DOWN`, for a single component and for the daemon overall (overall is `DOWN` if *any* component is `DOWN`). A component only flips to `DOWN` after `failure_threshold` *consecutive* failed checks; it recovers on the first success.

**`GET /health`** — The unauthenticated liveness/readiness endpoint (Actuator/k8s convention): `200` when overall **HealthStatus** is `UP`, `503` when `DOWN`, with a per-component breakdown in the body. A deliberate, documented exception to the require-auth rule, like `GET /api/setup/status`.

**Soft watchdog** — The proportionate middle recovery layer: the daemon sends `sd_notify(WATCHDOG=1)` on a `WATCHDOG_USEC/2` cadence **only while** overall health is `UP` and the **HealthSnapshot** is fresh. If health goes `DOWN` — or the refresh loop stalls (stale snapshot) — the ping is withheld, systemd's `WatchdogSec=15` elapses, and systemd **restarts the service** (the host stays up). Health-gated, unlike the hard watchdog.

**Hard watchdog** — The last-resort backstop: the daemon pets `/dev/watchdog` on a fixed cadence **ungated** by health (a `WatchdogOps` trait with a Linux impl and a `NoopWatchdog` mock). If the entire runtime freezes — even the health loop and the soft sd_notify ping can no longer run — the pets stop and the kernel **reboots the host**. On clean shutdown it disarms (magic close) so a graceful `systemctl stop` does not reboot. **Invariant: this layer is never health-gated.** See [0014-watchdog-and-health.md](docs/adr/0014-watchdog-and-health.md).

## Uninstall and teardown (issue #864)

**Runtime state** — The state the daemon installs in the *kernel*, as distinct from the files the installer put on disk: the `inet wardnet` nftables table and the `wg_ward*` `WireGuard` interfaces (outbound tunnels plus the inbound remote-access server). It is removed by name, never by flushing the whole ruleset, so rules belonging to Docker or to the operator are untouched. A stopped daemon leaves none of it behind; the installed files are a separate concern, removed by **uninstall**.

**Shutdown cause** — Why the daemon is shutting down: a **signal** (SIGINT/SIGTERM from outside the process) or a **restart** (the daemon cancelling its own shutdown token to hand over to a replacement, as the auto-updater, the rollback path and the admin Restart button all do). **Runtime state** is torn down only on a signal, because a restart's replacement process is seconds away while tunnels are only rebuilt on the tunnel monitor's next tick. `systemctl restart` is indistinguishable from `systemctl stop` and therefore counts as a signal. See [0028-shutdown-teardown-and-uninstall.md](docs/adr/0028-shutdown-teardown-and-uninstall.md).

**Purge** — The uninstall tier that additionally destroys `/var/lib/wardnet`: the database, the `WireGuard` private keys, the backup passphrase and the DDNS credentials. The default tier keeps them and re-owns them to root. Purged data is unrecoverable, so it is the only operation that demands the word typed in full rather than a yes.

## Monetization and entitlement

**Free tier** — Self-host the daemon with your **own** domain (the **BYOD-Cloudflare DnsProvider**). Full features via the desktop admin website and `/api/*`, uncapped, forever-free; touches no wardnet-operated service beyond release downloads, so it costs Wardnet nothing. The growth surface. Does **not** include the mobile PWAs — see **Premium tier**.

**Premium tier** — Paid. Grants the wardnet-operated, cost-bearing capabilities — the **DDNS service** (a managed `<slug>.my.wardnet.services` via the **wardnet DnsProvider**) and the two features the **Tunneller** carries, the **Inbound WireGuard server** (#266, "Personal VPN" on the marketing site) and **Private DNS** (#910) — plus the mobile app surfaces (the user PWA and admin mobile app), which are Premium-only regardless of whether the box brings its own domain. Free/BYO-domain installs administer entirely through the desktop admin website and `/api/*`, which stay reachable on every tier.

> The **Tunneller** is *infrastructure*, not a user-facing feature: it is the relay that gives a box WAN reachability with no port-forward, and it carries whichever features need it. Naming it as the deliverable ("the Tunneller (private DNS while roaming)") conflated the two, which is what [ADR-0029](docs/adr/0029-private-dns-dot.md), the **Tunneller** entry under *Infrastructure*, and the *Private DNS* section exist to separate.

**Tenant** — The *durable* billing principal in the **tenants** service, keyed to an **email**. The **Stripe customer** is *referenced* (`stripe_customer_id`), never authoritative for identity — so the processor can be swapped without losing accounts. A tenant owns one or more **networks**. Survives daemon reinstalls.

**Network** — A registered presence on the wardnet cloud: a **slug** + region + assigned `<slug>.my.wardnet.services` FQDN, owned by a **tenant**. The unit the **DDNS service** publishes for. The daemon persists `network_id` / `slug` in `system_config` after **network registration**.

**Slug** — A tenant's chosen vanity label (e.g. `happy-einstein`) forming the flat, region-free host `<slug>.my.wardnet.services`. Validated `[a-z0-9-]`, 3–32 chars, no leading/trailing hyphen, unreserved. (Synonym for the older **vanity name**.)

**Enrollment code** — A one-time code emailed to a **tenant**'s account address that the daemon requests on the operator's behalf (`POST /api/ddns/enrollment-code` → tenants `POST /tenants/v1/verification-codes {email, purpose:"enrollment"}`). Submitting it to *enroll* binds the **daemon identity** to the tenant. Replaces the old reinstall **magic-link**: a fresh box wipes-and-re-enrolls (no migration).

**Daemon identity** — The per-box cloud identity: an **Ed25519** seed in the daemon `SecretStore` (`SECRET_DAEMON_KEY`, generated at *enroll*), an in-memory cached **identity JWT**, and the shared **entitlement** flag. Authenticates every cloud call as this box; see [0016-daemon-cloud-auth.md](docs/adr/0016-daemon-cloud-auth.md).

**Identity JWT / PoP** — The short-lived bearer the daemon mints from the **tenants** service to call any cloud service, opaque to the daemon (it reads only `exp` to schedule a refresh). The mint request is authenticated by **Ed25519 proof-of-possession** (PoP) — signed with the daemon key, so no long-lived secret crosses the wire. The JWT is tenant-scoped after *enroll* and re-minted to network-scoped after **network registration**.

**Entitlement** — The daemon's local view of whether it's entitled to the **premium app surfaces** (the user PWA and admin mobile app) right now: `premium` (is this box on the wardnet DDNS provider at all — set/cleared by DDNS provider changes and primed at startup) **and not** `suspended` (has the wardnet provider's last token mint been refused — derived **directly from the token-mint outcome**, no signed lease: a `403` ("subscription not active") on mint ⇒ suspended, the next successful mint ⇒ restored). Two independent process-wide lock-free flags — the DDNS service flips `premium` on provider changes, the cloud clients flip `suspended` on mint outcomes, and the serving + runner layers read the composed result.

**Suspended** — The daemon state while a previously-active wardnet subscription has lapsed (`premium = true`, `suspended = true`). One of two ways a box ends up **not entitled** — the other being **Free tier**, which never subscribed at all (`premium = false`) — and both produce the identical serving-layer gate: `403` for the **premium app surfaces** — the user PWA (`/app/`) and admin mobile app (`/admin-app/`) — while the admin **website** (`/admin/`) and the whole `/api/*` surface stay reachable on every listener (including the plain-HTTP `:7411` LAN admin fallback) so the operator can always (re)subscribe. Only Suspended implies a domain that *was* working and is now degrading: the **DDNS runner** stops publishing but keeps a cheap per-tick token-mint re-probe (so the box self-heals on resubscribe with no operator action); the **TLS renewal runner** goes fully inert (the cert ages out, after which `:7411` is the re-entry path). See [0016-daemon-cloud-auth.md](docs/adr/0016-daemon-cloud-auth.md) and [0010-premium-tier-and-entitlement.md](docs/adr/0010-premium-tier-and-entitlement.md).

## Service decomposition

**Tenants service** — The *global* service and database, reached through the global **cloud gateway** under `/tenants/…`. Owns **tenants**, billing/Stripe linkage, the **global naming authority** (slug allocation), enrollment (verification codes, identity binding), and **identity JWT** minting. Lowest-traffic, most security-sensitive — isolating it from the internet-facing planes is the primary reason to split, ahead of scaling.

**Per-service cloud client** — The daemon's `cloud/` module: a `TenantsClient` (bound to the global **cloud gateway**) and a `DdnsClient` (bound to a regional gateway from the **region catalog**), each addressing its own service by path prefix but sharing one **daemon identity**. See [0017-per-service-cloud-clients.md](docs/adr/0017-per-service-cloud-clients.md).

**Regional plane** — The **DDNS** and **Tunneller** services, deployed per region behind the **edge SNI demux**. The daemon pins a region at registration (lowest-latency probe) and stays. Each holds only regional/operational state and authorizes calls by the **identity JWT** bearer rather than the global DB. DDNS writes into the global Cloudflare zone; the Tunneller accepts the daemon's relay connection at its PoP.

> **Status:** the three-way split — **tenants** (global), **DDNS** (regional), **Tunneller** (regional) — is the target the daemon already speaks to via per-service cloud clients. Entitlement crosses no boundary as an artifact: it is the **token-mint outcome** (see [0016-daemon-cloud-auth.md](docs/adr/0016-daemon-cloud-auth.md)). There is **no** OAuth/IdP server: machine auth is the Ed25519 daemon key + PoP-minted JWT, human enrollment is the **enrollment code**, and future inter-service auth is **mTLS**.
