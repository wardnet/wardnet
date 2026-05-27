# Wardnet Domain Glossary

## Surfaces

**Admin site** — The full desktop web admin. Served at `<id>.wardnet.network/admin/`. Not a PWA; intended for desktop use only. Previously named `admin-app` in source; renamed to `admin-site`.

**User PWA** — Installable mobile app for non-admin household members. Served at `<id>.wardnet.network/`. Scope: self-service only (own device routing, own DNS stats, own connection status). Cannot manage other devices.

**Admin mobile PWA** — Installable mobile app for admins. Served at `<id>.wardnet.network/admin-app/`. Scope: daily operational tasks (device management, tunnel status, power actions). Not a replacement for the admin site; configuration work (DHCP, filter profiles, tunnel creation) stays on the desktop.

## Identity and access

**Device-keyed** — Identified by MAC address / LAN IP. Non-admin users have no credentials; their identity is their device on the network. Push subscriptions and self-service routing rules are device-keyed.

**Admin session** — Credential-based (username + password). Required for any admin surface. Push subscriptions on the admin mobile PWA are admin-session-keyed.

**Admin lock** — Flag set by an admin on a device that prevents the device owner from changing their own routing rule. Read-only state visible in the user PWA.

## Features

**Route verification** — User PWA feature. Makes a client-side request to an external IP geolocation API to show the device's current public IP and inferred country/location. Used to confirm that a VPN routing rule is working as intended. Client-side call is correct: the browser request travels through the Pi's per-device routing, so the result reflects the device's actual egress path.

**Device-keyed push subscription** — A Web Push subscription (VAPID) stored in the daemon's database keyed to a device record (MAC/IP). Allows the daemon to notify a specific device's browser even when the PWA is not open.

## Infrastructure

**DDNS service** — Wardnet-operated service that assigns each installation a unique subdomain (`<install-id>.wardnet.network`) and manages DNS records for it. Also acts as an ACME bridge: handles `_acme-challenge` TXT records on behalf of the Pi so Let's Encrypt can issue a certificate via DNS-01 without the user needing a domain or DNS provider credentials. The cert private key is generated on the Pi and never leaves it.

**Path-based app routing** — All three surfaces are served from a single domain (`<id>.wardnet.network`) at different paths (`/`, `/admin-app/`, `/admin/`). Each PWA has its own `manifest.json` with a distinct `scope` and `start_url`, making them independently installable despite sharing an origin.

**Caddy** — Reverse proxy bundled in the wardnet release tarball alongside `wardnetd`. Runs as a companion systemd service. Handles TLS termination on port 443, certificate provisioning via Let's Encrypt DNS-01 (using the wardnet DDNS service as the ACME bridge), and forwards all traffic to the daemon on port 7411. The daemon manages the Caddyfile on startup and config changes.
