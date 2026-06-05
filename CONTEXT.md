# Wardnet Domain Glossary

## Surfaces

**Admin site** — The full desktop web admin. Served at `<id>.wardnet.network/admin/`. Not a PWA; intended for desktop use only. Source package: `source/admin-site`.

**User PWA** — Installable mobile app for non-admin household members. Served at `<id>.wardnet.network/`. Scope: self-service only (own device routing, own DNS stats, own connection status). Cannot manage other devices.

**Admin mobile PWA** — Installable mobile app for admins. Served at `<id>.wardnet.network/admin-app/`. Scope: daily operational tasks (device management, tunnel status, power actions). Not a replacement for the admin site; configuration work (DHCP, filter profiles, tunnel creation) stays on the desktop.

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

## Infrastructure

**DDNS service** — Wardnet-operated service that assigns each installation a unique subdomain (`<install-id>.wardnet.network`) and manages DNS records for it. Also acts as an ACME bridge: handles `_acme-challenge` TXT records on behalf of the Pi so Let's Encrypt can issue a certificate via DNS-01 without the user needing a domain or DNS provider credentials. The cert private key is generated on the Pi and never leaves it.

**DnsProvider** — The daemon-side abstraction over the publish side of DDNS: a provider bound at construction to one target that can `upsert_a` (publish the A record) and `set_txt` / `delete_txt` (the ACME `_acme-challenge` record). Two implementations: the **bridge** provider (default, talks to a wardnet bridge, keyed by install id, every request Ed25519-signed) and the **Cloudflare** provider (Bring-Your-Own-Domain, talks to the user's Cloudflare zone directly). The cert/signing key never leaves the Pi under either.

**Region slug** — A short identifier for a wardnet bridge deployment (e.g. `use1`). It selects which **bridge endpoint** the daemon talks to; it is distinct from the region *label* the bridge embeds in an assigned FQDN (e.g. `…my.us…`), which the bridge owns and returns at registration.

**Region catalog** — The built-in, daemon-shipped table mapping each **region slug** to its **bridge endpoint** URL. The bridge cannot supply this (each bridge is region-specific), so the daemon must already know it. At registration the daemon probes every catalogued region's health endpoint and registers against the lowest-latency one.

**Bridge endpoint** — The base URL of a region's wardnet bridge (e.g. `https://bridge.use1.wardnet.network`), the value a **region slug** resolves to in the **region catalog**.

**Public WAN IP** — The home's internet-facing IPv4 address, discovered by the daemon via an external echo service over its default (WAN) route. This is what DDNS publishes — explicitly *not* a tunnel exit IP (a device's egress address when routed through a VPN tunnel), which the daemon measures separately for routing diagnostics.

**Path-based app routing** — All three surfaces are served from a single domain (`<id>.wardnet.network`) at different paths (`/`, `/admin-app/`, `/admin/`). Each PWA has its own `manifest.json` with a distinct `scope` and `start_url`, making them independently installable despite sharing an origin.

**Caddy** — Reverse proxy bundled in the wardnet release tarball alongside `wardnetd`. Runs as a companion systemd service. Handles TLS termination on port 443, certificate provisioning via Let's Encrypt DNS-01 (using the wardnet DDNS service as the ACME bridge), and forwards all traffic to the daemon on port 7411. The daemon manages the Caddyfile on startup and config changes.
