---
status: accepted
date: 2026-07-20
issue: "#241"
---

# ADR: Domain-based routing via routing profiles

---

## Context

Per-device routing (issue-era #735) binds a device to a single routing target
and installs `ip rule from <device_ip> lookup <tunnelTable>`. `#241` asks for an
orthogonal axis: route traffic **to a domain** — "everything to `*.netflix.com`
exits via the UK tunnel" — while the rest of each device's traffic follows its
normal rule.

The incumbents (pfSense/OPNsense alias-based PBR, UniFi's domain policy routes)
all express this as a policy rule scoped by **source (device/network) +
destination (domain)**, and all require clients to resolve through the gateway.
A global "all devices" design would be simpler but strictly below that bar, so
the decision is to match it: per-device scoping, expressed the way Wardnet
already expresses per-device DNS filtering.

Two mechanisms already in the tree make this cheap:

- **Switchback** (ADR-0026) already installs *destination-scoped* `ip rule`s
  (`from <ip> to <cidr> lookup …`) with reconcile/prune. Domain routing is the
  same primitive with the table and destination varied.
- **DNS filter profiles** already model "a named bundle of rules, assigned to
  one or more devices, compiled into a per-device hot-path context." Domain
  routing is the routing sibling of exactly that shape.

## Decision

Ship **Routing Profiles**: a named set of `domain → target` rules, assigned to a
device in **priority order**. The local DNS server, after resolving a matched
domain, pins the resolved IPs to the chosen table for the querying device.

### Data & service — mirror DNS filter profiles

`routing_profiles` + `routing_profile_rules` (FK to a profile) +
`routing_device_profile` (many-to-many). Unlike the filter join, the assignment
carries a `position`: routing has a genuine conflict (two profiles, two tunnels)
that boolean filtering does not, so the assignment is **ordered**. A
`RoutingProfileService` owns CRUD and compiles a per-device routing view; it is
the parallel of `DnsFilterService`.

### Target kinds

A rule targets a **tunnel** (route the domain through it) or **direct** (carve
the domain out of the device's tunnel back to the WAN — `lookup main`, the
useful inverse of a per-device tunnel binding). `Default` has no meaning for a
per-domain override and is not representable.

### Conflict resolution — profile order, not specificity

A device's profiles are evaluated in **assignment order**; the first profile
with an enabled matching rule wins. Specificity does *not* override order — a
broad `*.example.com` high in the order intentionally shadows an exact rule
below it, so the operator controls precedence by ordering profiles (the same
mental model as an ordered firewall rule list). Within the winning profile the
most-specific pattern is used. Matching is glob/suffix: `*.example.com` covers
the apex and any subdomain; a bare name is exact.

### Enforcement — Switchback's primitive at a new priority band

The DNS pipeline, after an upstream answer, hands the querying device's resolved
A/AAAA IPs to `RoutingProfileService::note_resolution` (a cheap in-memory lookup
+ non-blocking channel send — no work on the DNS hot path). A background runner
drains the channel and calls `RoutingService::route_resolved_domain`, which
installs `ip rule from <device_ip> to <resolved_ip>/32 lookup <table> priority
2000`. Priority **2000** sits above Switchback's 1000 (so a cross-zone carve-out
still wins for its narrow pair) yet far below the kernel's per-tunnel source
rules (~32764), so the per-destination decision beats the device's own source
rule for that one IP. Each rule is **leased for the DNS record's TTL** (clamped
to `[30s, 1h]`); a periodic GC expires leases and prunes orphaned rules, and a
full `reconcile()` clears them (they re-install as devices re-resolve).

## Considered options

- **Global (all-device) domain rules.** Simpler (one rule per resolved IP, no
  device attribution) and enough for the issue's literal "from any device", but
  below the competition and a dead end for the common "route X only for the
  kids' TV" case. Rejected; per-device scoping reuses the very same
  `from…to…lookup` rule Switchback already installs, so the simplicity saving was
  marginal.
- **Pre-resolve FQDNs into an IP-set alias on a timer** (the pfSense model)
  instead of hooking live DNS answers. A poll can miss IPs a CDN hands a client
  between refreshes; hooking our own resolver captures exactly the IPs the client
  will use, at resolve time, with the real TTL. Rejected in favour of the live
  hook (we *are* the resolver, so the "clients must use the gateway" precondition
  is free).
- **Specificity-wins conflict resolution.** More "intuitive" in isolation but
  gives no operator control when two profiles genuinely disagree; profile order
  is explicit and matches how admins already reason about rule lists. (Chosen
  with the requester.)
- **Regex patterns.** Rejected for v1: ReDoS risk on the DNS hot path and a
  harder UI, for little gain over glob/suffix.

## Consequences

- Enforcement is IPv4-only in v1 (the `ip rule` primitive is v4).
- **Shared-IP CDNs** (CloudFront/Akamai/Fastly): a rule's resolved IP may also
  serve other domains, whose traffic then takes the tunnel too. Documented and
  accepted; a per-domain "strict" flag is deferred.
- **SNI-only / hardcoded-DNS services** can't be captured from DNS answers; DPI
  is out of scope.
- **Established flows past TTL** keep their route (conntrack pins them); GC
  affects new flows only — the same behaviour pfSense/UniFi exhibit.
- **No global default profile** in v1: a device with no assigned profile behaves
  exactly as before. Source-scoping and a strict flag are additive follow-ups;
  the schema (`position`, per-`(device,IP)` rules) already accommodates them.
