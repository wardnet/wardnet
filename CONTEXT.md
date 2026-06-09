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

**DDNS service** — Wardnet-operated service that assigns each installation a unique subdomain (`<install-id>.wardnet.network`) and manages DNS records for it. Also acts as an ACME bridge: handles `_acme-challenge` TXT records on behalf of the Pi so Let's Encrypt can issue a certificate via DNS-01 without the user needing a domain or DNS provider credentials. The cert private key is generated on the Pi and never leaves it.

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

**Caddy** — Reverse proxy bundled in the wardnet release tarball alongside `wardnetd`. Runs as a companion systemd service. Handles TLS termination on port 443, certificate provisioning via Let's Encrypt DNS-01 (using the wardnet DDNS service as the ACME bridge), and forwards all traffic to the daemon on port 7411. The daemon manages the Caddyfile on startup and config changes.

**Daemon-owned TLS termination** — `wardnetd` terminating TLS itself on port 443, replacing Caddy. The daemon obtains a certificate via ACME DNS-01 (publishing `_acme-challenge` TXT through the **DnsProvider**), serves `:443` with it, hot-swaps it on renewal, and 308-redirects `:80`→`:443`. The leaf private key is generated on the Pi and never leaves the LAN; cert + key are stored only through the **SecretStore** abstraction.

**Placeholder cert** — A throwaway self-signed certificate generated at boot to seed the `:443` listener before a real certificate has been issued, so the port is always bound (TLS can't handshake without *a* cert). It is never trusted by clients: while it is in use the **TLS provisioning** gate is closed and every `:443` route returns `503`, pointing the operator at the plain-HTTP `:7411` fallback.

**TLS provisioning** — The boolean state of whether the daemon is serving a real (vs **placeholder**) certificate on `:443`. A shared `provisioned` flag gates a 503 guard on the `:443` app; it flips to `true` when the first real certificate is activated. Pre-provisioning, `:7411` plain HTTP is the honest admin surface.

**TLS renewal** — The background re-issuance of the certificate before expiry. `TlsService::ensure_certificate()` is a single idempotent operation — issue-if-missing or renew-if-within-30-days — driven on a 12-hour tick by `TlsRenewalRunner` and inert until DDNS (and therefore the public FQDN) is configured.

**TLS provisioning phase** — A coarse, persisted progress signal for certificate issuance — `idle` → `issuing` → `issued` / `failed` — surfaced to the **Remote access (setup step)** and the dashboard so an operator can watch the (otherwise opaque) ACME round-trip and see any failure. Distinct from **TLS provisioning** (the boolean serving-a-real-cert gate): the phase narrates the *process*, the gate names the *outcome*. A live cert reads as `issued` even with no marker; `failed` carries the last error. Read via `GET /api/tls/status`.

**Canonical FQDN** — The single public hostname the gateway is reached by and holds a valid certificate for: the **domain the active certificate was issued for** (`tls_cert_domain`), not merely the configured DDNS hostname. The two are normally identical and diverge only transiently (issuance lag, a domain change before re-issuance, an ACME failure); the cert domain is authoritative precisely because it is the name that currently works. It is the primary entry point (PWA `start_url`/`scope`, bookmark) and the target of the **short-name redirect** and the **split-horizon** record. Absent (no cert yet) ⟹ both are inert.

**Short-name redirect** — The `:80` listener's behaviour of 308-redirecting a request arriving under a short or LAN name (`wardnet`, `wardnet.lan`, the bare LAN IP) to `https://<canonical-FQDN>`, so the client lands on the name with a valid cert. When no canonical FQDN is provisioned, or the request already targets it, the redirect is a plain same-host HTTP→HTTPS upgrade.

**Serving identity** — The daemon's current `:443` serving state — *which domain's certificate is live* — exposed to the unauthenticated `:80`/`:443` listeners through methods (`is_provisioned` / `canonical_fqdn`) rather than a shared flag or an admin-gated call. It is the hot-path projection of the authoritative served domain (`tls_cert_domain`, read by the API via `TlsService`); a non-empty serving identity is equivalent to **TLS provisioning** being complete.
