# Wardnet Domain Glossary

## Surfaces

**Admin site** — The full desktop web admin. Served at `<vanity>.my.wardnet.services/admin/`. Not a PWA; intended for desktop use only. Source package: `source/admin-site`.

**User PWA** — Installable mobile app for non-admin household members. Served at `<vanity>.my.wardnet.services/`. Scope: self-service only (own device routing, own DNS stats, own connection status). Cannot manage other devices.

**Admin mobile PWA** — Installable mobile app for admins. Served at `<vanity>.my.wardnet.services/admin-app/`. Scope: daily operational tasks (device management, tunnel status, power actions). Not a replacement for the admin site; configuration work (DHCP, filter profiles, tunnel creation) stays on the desktop.

## Identity and access

**Device-keyed** — Identified by MAC address / LAN IP. Non-admin users have no credentials; their identity is their device on the network. Push subscriptions and self-service routing rules are device-keyed.

**Admin session** — Credential-based (username + password). Required for any admin surface. Push subscriptions on the admin mobile PWA are admin-session-keyed.

**Admin lock** — Flag set by an admin on a device that prevents the device owner from changing their own routing rule. Read-only state visible in the user PWA.

## Features

**Route verification** — User PWA feature. Makes a client-side request to an external IP geolocation API to show the device's current public IP and inferred country/location. Used to confirm that a VPN routing rule is working as intended. Client-side call is correct: the browser request travels through the Pi's per-device routing, so the result reflects the device's actual egress path.

**Device-keyed push subscription** — A Web Push subscription (VAPID) stored in the daemon's database keyed to a device record (MAC/IP). Allows the daemon to notify a specific device's browser even when the PWA is not open.

## Routing

**Routing target** — Where a device's traffic egresses: a specific **tunnel**, **direct** (bypass all tunnels, use the WAN), or **default** (explicitly defer to the gateway's default policy). A device's *current* routing target is its per-device rule if one exists.

**Routing rule** — A per-device binding of a device to a routing target, created by an admin or by the device owner (self-service). At most one rule exists per device.

**Default policy** — The gateway-wide fallback applied to a device that has **no** routing rule of its own. A device following the default policy is distinct from one whose rule's target is explicitly *default*: the former has no rule (its current routing target is absent/`null`), the latter has a rule that names *default* as the target. Both ultimately follow the gateway policy, but only the latter is a persisted choice.

## Local DNS

**Authoritative local zone** — A named DNS domain (e.g. `lan`, `home`) the gateway answers for directly rather than forwarding upstream. Single-label names are valid. Zones group custom records; deleting a zone keeps its records but unlinks them.

**Custom DNS record** — A user-defined record (`A`, `AAAA`, `CNAME`, `TXT`, `MX`, `SRV`) mapping a domain to a value, answered locally. May belong to an authoritative local zone or stand alone (unzoned).

**Forwarding rule** — Also called *conditional forwarding*: a `domain → upstream` override that sends queries under a specific domain to a chosen upstream resolver instead of the default upstream pool (e.g. `corp.example.com → 10.0.0.53`). It is the per-domain form of the gateway-wide **Forwarding** resolution mode; the latter forwards *all* queries to the default upstreams.

**Zone provenance** — Whether an **authoritative local zone** was created by an admin (`manual`) or seeded by the daemon (`system`). A **system zone** — currently only the seeded `.lan` zone — cannot be deleted: the admin API rejects the attempt and the UI hides the delete control. Manual zones are freely deletable. (Custom records carry an analogous provenance: `manual`, `dhcp`, or `system`.)

**System DNS record** — A **custom DNS record** the daemon maintains for itself (provenance `system`), as opposed to admin- or DHCP-created ones. Two exist: the **split-horizon** record (the canonical FQDN → the Pi's LAN IP) and the convenience `wardnet.lan` → Pi LAN IP. The daemon owns their lifecycle; a DHCP-sourced upsert can never overwrite them.

**Split-horizon resolution** — Answering the public **canonical FQDN** with the Pi's *LAN* IP for clients querying through the gateway, while the same name resolves to the **Public WAN IP** on the public internet. Lets a LAN device reach the Pi directly (and get the valid certificate for that name) instead of hair-pinning out through the WAN.

## Infrastructure

**DDNS service** — Wardnet-operated service that assigns each installation a **vanity name** under `<vanity>.my.wardnet.services` and manages DNS records for it. Also acts as an ACME bridge: publishes the `_acme-challenge` TXT records the Pi's own ACME client needs, so Let's Encrypt can issue a certificate via DNS-01 without the user needing a domain or DNS provider credentials. The cert private key is generated on the Pi and never leaves it. A **regional** deployment (see *Service decomposition*) that writes records into the global Cloudflare `wardnet.services` zone; it is a **premium-tier** capability, gated by the **entitlement lease**.

**Remote access (setup step)** — The setup wizard's HTTPS step (`wizard_step == remote_access`, between Policy and Completed). The operator picks a **DnsProvider** — the wardnet **bridge** (default; suggests an editable two-word hostname with a live availability check) or **BYOD-Cloudflare** (their own domain + API token) — and the daemon registers it synchronously, then issues the certificate in the background (`POST /api/ddns/register` / `/cloudflare` → `mark_provisioning_started` → detached `ensure_certificate`). Non-blocking: the step is skippable and completes even offline, with issuance retried later from Settings. Progress is the **TLS provisioning phase**.

**DnsProvider** — The daemon-side abstraction over the publish side of DDNS: a provider bound at construction to one target that can `upsert_a` (publish the A record), `set_txt` / `delete_txt` (publish one *or more* `_acme-challenge` TXT values at the one challenge name simultaneously, then remove all of them — multi-valued because a **per-user wildcard certificate** authorizes two SANs through the same name), and `teardown` (remove the published presence — the bridge provider calls `DELETE /v1/installs/{id}` with its bearer token, which drops the upstream A + ACME records and the install row; the Cloudflare provider deletes its A record). Two implementations: the **bridge** provider (default, talks to a wardnet bridge, keyed by install id, every publish request Ed25519-signed) and the **Cloudflare** provider (Bring-Your-Own-Domain, talks to the user's Cloudflare zone directly). The cert/signing key never leaves the Pi under either.

**Region slug** — A short identifier for a wardnet bridge deployment (e.g. `use1`). It selects which **bridge endpoint** the daemon talks to; it is distinct from the region *label* the bridge embeds in an assigned FQDN (e.g. `…my.us…`), which the bridge owns and returns at registration.

**Region catalog** — The built-in, daemon-shipped table mapping each **region slug** to its **bridge endpoint** URL. The bridge cannot supply this (each bridge is region-specific), so the daemon must already know it. At registration the daemon probes every catalogued region's health endpoint and registers against the lowest-latency one.

**Bridge endpoint** — The base URL of a region's wardnet bridge (e.g. `https://bridge.prod.use1.wardnet.network`), the value a **region slug** resolves to in the **region catalog**.

**Vanity name** — A user's chosen slug (e.g. `alice`) forming the flat, region-free user host `<vanity>.my.wardnet.services`. Validated `[a-z0-9-]`, 3–32 chars. The region is deliberately *not* in the name (it lives in the record's value and in infra names only), so a user can be migrated between regions without changing their host, bookmarks, or certificate. Per-service hosts nest under it: `<service>.<vanity>.my.wardnet.services`. See [adr-two-domain-strategy.md](docs/adr-two-domain-strategy.md).

**Global naming authority** — The strongly-consistent registry of **vanity names**, a *separate global Postgres* (distinct from each bridge's per-region install DB) holding one `names` table whose `UNIQUE` slug constraint *is* the cross-region allocation lock. Because vanity names form one flat global namespace, a single authority must answer availability and guarantee one-winner allocation across regions. Each bridge connects to both the global (names) and its regional (installs) DB; the daemon never touches it (it calls the bridge over HTTP). Availability is a read against this registry — *not* DNS and *not* a cache. DNS stays purely the resolution layer. Deliberately not Cloudflare KV (eventual consistency breaks atomic reserve) / D1 / DNS-as-registry. See [adr-global-naming-authority.md](docs/adr-global-naming-authority.md).

**Name reservation** — The two-phase registration protocol against the **global naming authority**: (1) atomically *reserve* the slug (`INSERT … status='reserved'` with a TTL; a unique violation means taken); (2) *provision* the regional install row (the bridge creates no DNS record — it is pure SNI passthrough; the wildcard `*.my.wardnet.services` is infra-provisioned and the per-user cert is daemon-issued); (3) *confirm* (`status='active'`). Because the `names` row is global and the install row is regional this is a two-database saga: on failure the reservation is *released* (both rows deleted), and a region-scoped scheduled sweep reaps expired `reserved` rows and their install orphans so a crashed registration never leaks a name.

**Per-user wildcard certificate** — One certificate per **vanity name** carrying two SANs — the apex `<vanity>.my.wardnet.services` (serves the PWA + admin site) and the wildcard `*.<vanity>.my.wardnet.services` (per-service gateway hosts) — issued via ACME DNS-01. Both SANs authorize through the *same* `_acme-challenge.<vanity>.my.wardnet.services` TXT name, so their two challenge values are published *simultaneously*; the **DnsProvider** challenge path is therefore multi-valued. Stable across region migrations (no region in the SAN); new services need no new cert.

**Public WAN IP** — The home's internet-facing IPv4 address, discovered by the daemon via an external echo service over its default (WAN) route. This is what DDNS publishes — explicitly *not* a tunnel exit IP (a device's egress address when routed through a VPN tunnel), which the daemon measures separately for routing diagnostics.

**Resolution check** — A diagnostic that confirms the *public* internet resolves the **canonical FQDN** to the IP the daemon last published. The daemon queries a fixed pair of public resolvers (Cloudflare `1.1.1.1` + Google `8.8.8.8`) **by IP over DoH**, which deliberately bypasses the daemon's own **split-horizon** record (that record only answers LAN clients). It has three outcomes: **match** (public DNS agrees with the published IP — propagation complete), **mismatch** (resolves to a different IP — stale record or wrong config), and **pending** (no A record yet — the normal state in the propagation window right after registration). It compares against the *last published* IP, not the current WAN IP; detecting a WAN-IP change is the DDNS runner's job, not the check's. Read via `GET /api/ddns/resolution-check`.

**Path-based app routing** — All three surfaces are served from a single host (`<vanity>.my.wardnet.services`) at different paths (`/`, `/admin-app/`, `/admin/`). Each PWA has its own `manifest.json` with a distinct `scope` and `start_url`, making them independently installable despite sharing an origin.

**Caddy** *(retired)* — Formerly the reverse proxy that terminated TLS in front of both surfaces. It is no longer used anywhere: the daemon does **Daemon-owned TLS termination** (DNS-01), and the bridge does **Bridge self-terminated TLS** (HTTP-01) behind a transparent L4 proxy. Retained here only so older docs and issues that mention "Caddy" resolve to "the thing both components replaced with in-process termination."

**Daemon-owned TLS termination** — `wardnetd` terminating TLS itself on port 443, replacing Caddy. The daemon obtains a certificate via ACME DNS-01 (publishing `_acme-challenge` TXT through the **DnsProvider**), serves `:443` with it, hot-swaps it on renewal, and 308-redirects `:80`→`:443`. The leaf private key is generated on the Pi and never leaves the LAN; cert + key are stored only through the **SecretStore** abstraction.

**Placeholder cert** — A throwaway self-signed certificate generated at boot to seed the `:443` listener before a real certificate has been issued, so the port is always bound (TLS can't handshake without *a* cert). It is never trusted by clients: while it is in use the **TLS provisioning** gate is closed and every `:443` route returns `503`, pointing the operator at the plain-HTTP `:7411` fallback.

**TLS provisioning** — The boolean state of whether the daemon is serving a real (vs **placeholder**) certificate on `:443`. A shared `provisioned` flag gates a 503 guard on the `:443` app; it flips to `true` when the first real certificate is activated. Pre-provisioning, `:7411` plain HTTP is the honest admin surface.

**TLS renewal** — The background re-issuance of the certificate before expiry. `TlsService::ensure_certificate()` is a single idempotent operation — issue-if-missing or renew-if-within-30-days — driven on a 12-hour tick by `TlsRenewalRunner` and inert until DDNS (and therefore the public FQDN) is configured.

**TLS provisioning phase** — A coarse, persisted progress signal for certificate issuance — `idle` → `issuing` → `issued` / `failed` — surfaced to the **Remote access (setup step)** and the dashboard so an operator can watch the (otherwise opaque) ACME round-trip and see any failure. Distinct from **TLS provisioning** (the boolean serving-a-real-cert gate): the phase narrates the *process*, the gate names the *outcome*. A live cert reads as `issued` even with no marker; `failed` carries the last error. Read via `GET /api/tls/status`.

**Canonical FQDN** — The single public hostname the gateway is reached by and holds a valid certificate for: the **domain the active certificate was issued for** (`tls_cert_domain`), not merely the configured DDNS hostname. The two are normally identical and diverge only transiently (issuance lag, a domain change before re-issuance, an ACME failure); the cert domain is authoritative precisely because it is the name that currently works. It is the primary entry point (PWA `start_url`/`scope`, bookmark) and the target of the **short-name redirect** and the **split-horizon** record. Absent (no cert yet) ⟹ both are inert.

**Short-name redirect** — The `:80` listener's behaviour of 308-redirecting a request arriving under a short or LAN name (`wardnet`, `wardnet.lan`, the bare LAN IP) to `https://<canonical-FQDN>`, so the client lands on the name with a valid cert. When no canonical FQDN is provisioned, or the request already targets it, the redirect is a plain same-host HTTP→HTTPS upgrade.

**Serving identity** — The daemon's current `:443` serving state — *which domain's certificate is live* — exposed to the unauthenticated `:80`/`:443` listeners through methods (`is_provisioned` / `canonical_fqdn`) rather than a shared flag or an admin-gated call. It is the hot-path projection of the authoritative served domain (`tls_cert_domain`, read by the API via `TlsService`); a non-empty serving identity is equivalent to **TLS provisioning** being complete.

**Bridge self-terminated TLS** — The bridge terminating TLS for its **own** FQDN (`INFORGE_DEPLOYMENT_FQDN`, under the infra `wardnet.network` domain) in-process, having dropped **Caddy**. It issues that certificate via **ACME HTTP-01** (the bridge is publicly reachable on `:80`, unlike the NAT'd daemon which must use DNS-01) and serves the control-plane API over the terminated connection. On its TLS listener it peeks the **SNI**: a match for its own FQDN terminates locally; every other SNI is passed through still-encrypted to the tenant's reverse tunnel (TLS terminates on the Pi). The bridge sits behind a **transparent L4 proxy** (nginx + PROXY protocol v1) that maps the public privileged ports to the bridge's localhost ports and carries the real client IP. Cert + ACME account material are sealed (AES-256-GCM, per-region key) in the regional Postgres so they survive restarts and are shared across hosts. Has its own **Placeholder cert** seeding the listener before first issuance.

**Issuance lease** — The coordination primitive that lets multiple bridge hosts behind one FQDN renew safely: a host claims issuance by winning a conditional row `UPDATE` (a lease, not a held DB lock), so only one host runs the ACME round-trip and concurrent hosts never race-burn the Let's Encrypt rate limit. A non-issuing host instead reloads (hot-swaps) its in-memory cert when the stored cert `version` overtakes what it serves.

**Shared challenge token** — The bridge's HTTP-01 challenge token, written to a shared Postgres table rather than held in one host's memory, so Let's Encrypt's `:80` validation is answered correctly no matter which host it reaches. Reaped on a TTL by the bridge sweep, mirroring the daemon's "always clear the challenge" discipline.

## Reliability and watchdog (issue #214)

**HealthMonitor** — The daemon-side aggregator (in `wardnetd-services/src/health/`) that holds the registered **HealthCheck**s, re-runs them all on a fixed tick, debounces failures, and publishes an immutable **HealthSnapshot** through an `ArcSwap` for lock-free reads. It only *reports* status; recovery policy lives in the watchdog layers. Checks run concurrently with a per-check `tokio::time::timeout`, so one hung probe can't stall the cycle.

**HealthCheck** — A pluggable async probe (`name()` + `check() -> CheckOutcome`) adapting one subsystem into a cheap readiness signal. The four initial probes are **database** (`SELECT 1`), **liveness** (always UP — proves the loop schedules), **dns** and **dhcp**. The DNS/DHCP probes are **desired-vs-actual**: each reads its configured `enabled` flag (under an admin context, like the runners) and reports DOWN *only* when the service is enabled yet not running (a crash) — never for a deliberately toggled-off service, which would otherwise restart-loop the daemon. Must be non-blocking and never panic.

**HealthStatus** — The debounced verdict, `UP` or `DOWN`, for a single component and for the daemon overall (overall is `DOWN` if *any* component is `DOWN`). A component only flips to `DOWN` after `failure_threshold` *consecutive* failed checks; it recovers on the first success.

**`GET /health`** — The unauthenticated liveness/readiness endpoint (Actuator/k8s convention): `200` when overall **HealthStatus** is `UP`, `503` when `DOWN`, with a per-component breakdown in the body. A deliberate, documented exception to the require-auth rule, like `GET /api/setup/status`.

**Soft watchdog** — The proportionate middle recovery layer: the daemon sends `sd_notify(WATCHDOG=1)` on a `WATCHDOG_USEC/2` cadence **only while** overall health is `UP` and the **HealthSnapshot** is fresh. If health goes `DOWN` — or the refresh loop stalls (stale snapshot) — the ping is withheld, systemd's `WatchdogSec=15` elapses, and systemd **restarts the service** (the host stays up). Health-gated, unlike the hard watchdog.

**Hard watchdog** — The last-resort backstop: the daemon pets `/dev/watchdog` on a fixed cadence **ungated** by health (a `WatchdogOps` trait with a Linux impl and a `NoopWatchdog` mock). If the entire runtime freezes — even the health loop and the soft sd_notify ping can no longer run — the pets stop and the kernel **reboots the host**. On clean shutdown it disarms (magic close) so a graceful `systemctl stop` does not reboot. **Invariant: this layer is never health-gated.** See [adr-watchdog-and-health.md](docs/adr-watchdog-and-health.md).

## Monetization and entitlement

**Free tier** — Self-host the daemon with your **own** domain (the **BYOD-Cloudflare DnsProvider**). Full features, uncapped, forever-free; touches no wardnet-operated service beyond release downloads, so it costs Wardnet nothing. The growth surface.

**Premium tier** — Paid. Grants the two wardnet-operated, cost-bearing capabilities: the **DDNS service** (a managed `<vanity>.my.wardnet.services` via the **bridge DnsProvider**) and the **Tunneler** (private DNS while roaming). A free user who wants the mobile PWAs but no premium can still get them by bringing their own domain — premium buys *not needing a domain*, plus the tunnel.

**Premium account** — The *durable* billing principal, keyed to an **email**. Its master record lives in **tenant management** (alongside the **global naming authority**); the **Stripe customer** is *referenced* (`stripe_customer_id`), never authoritative for identity — so the processor can be swapped without losing accounts. Survives daemon reinstalls.

**Install binding** — The *ephemeral* link from a **premium account** to a running install's Ed25519 key. One **active** binding per subscription; reinstall re-binds the single slot (no re-payment) by proving account ownership via an email **magic-link**. A second simultaneous install is a second subscription; moving premium between Pis is a serial rebind.

**Entitlement** — An account's current right to wardnet services, derived from Stripe via webhooks (plus a nightly reconciliation). **Active through Stripe `past_due`** (a failed card enters dunning grace, not an instant cutoff) and **revoked on `canceled`**. Held on the tenant-management account record.

**Entitlement lease** — A short-lived signed token `{install_id, entitled, exp}` issued by **tenant management** (global) and verified *locally* — by the regional **DDNS** and **Tunneler** services and by the daemon — against tenant's public key. It is the **only** thing crossing the global↔regional boundary for entitlement, so no regional service ever queries the global DB on the hot path. TTL ~7 days; the daemon refreshes daily, so a transient outage cannot break a paying customer for up to the TTL. Distinct from the **Issuance lease** (TLS-renewal coordination) — same word, unrelated mechanism.

**Suspended** — The daemon state once its **entitlement lease** goes invalid: the user and admin PWAs return `403`, the **Tunneler** drops, and ACME renewal stops (the cert ages out within ≤90 days). Enforcement is belt-and-suspenders — regional services refuse at the boundary *and* the daemon self-degrades. Distinct from **Free tier**, which is fully functional via its own domain; a Suspended install has no working domain until it either resubscribes or adds its own. Re-entry is always reachable: the desktop **admin site** during the cert window, plus a LAN-local HTTP admin fallback after expiry; resubscribing refreshes the lease and restores service.

## Service decomposition

**Tenant management** — The *global* service and the single global database. Owns **premium accounts**, **entitlement**, **install bindings**, the **global naming authority** (vanity-name allocation), Stripe linkage, and **entitlement-lease** signing. Lowest-traffic, most security-sensitive — isolating it from the internet-facing planes is the primary reason to split, ahead of scaling.

**Regional plane** — The **DDNS** and **Tunneler** services, deployed per region. The daemon pins a region at install (lowest-latency probe) and stays. Each holds only regional/operational state and trusts the **entitlement lease** rather than the global DB. DDNS writes into the global Cloudflare zone; the Tunneler accepts the daemon's relay connection at its PoP.

> **Status:** today these are one **bridge** deployment (per-region install DB + the global names DB). The three-way split — **tenant management** (global), **DDNS** (regional), **Tunneler** (regional) — is the agreed target, with the **entitlement lease** as the boundary primitive. There is **no** OAuth/IdP server: machine auth is the Ed25519 install key, human recovery is the **magic-link**, and future inter-service auth is **mTLS**.
