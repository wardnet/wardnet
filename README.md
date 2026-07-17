<div align="center" style="margin-bottom:25px">
<img src="logo.png" alt="Wardnet logo" />
</div>

[![CI](https://github.com/wardnet/wardnet/actions/workflows/ci.yml/badge.svg)](https://github.com/wardnet/wardnet/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/wardnet/wardnet/branch/main/graph/badge.svg)](https://codecov.io/gh/wardnet/wardnet)
[![Rust](https://img.shields.io/badge/rust-1.96-orange.svg)](https://www.rust-lang.org)
[![Rust Report Card](https://rust-reportcard.xuri.me/badge/github.com/wardnet/wardnet)](https://rust-reportcard.xuri.me/report/github.com/wardnet/wardnet)
[![Security Audit](https://github.com/wardnet/wardnet/actions/workflows/security.yml/badge.svg)](https://github.com/wardnet/wardnet/actions/workflows/security.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/wardnet/wardnet/badge)](https://securityscorecards.dev/viewer/?uri=github.com/wardnet/wardnet)
[![Dependabot](https://badgen.net/github/dependabot/wardnet/wardnet)](https://github.com/wardnet/wardnet/pulls?q=is%3Apr+author%3Aapp%2Fdependabot)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

**Your network. Your rules.**

Wardnet is a self-hosted network privacy gateway you run on your own hardware — a Raspberry Pi, a mini-PC, or any Linux host. It sits alongside your existing home or small-office router and acts as the warden of every device's connection to the internet: routing traffic through per-device VPN tunnels, blocking ads and trackers at the DNS level, running your own local DNS, and giving you full control from a dashboard — on your desktop or right from your phone.

**Think of it as a Pi-hole replacement with per-device VPN routing, network segmentation, and mobile apps built in.** Network-wide ad and tracker blocking (bring your existing Pi-hole blocklists), WireGuard tunnels you can assign to individual devices, and locked-down zones for IoT and guest devices — in one signed binary, one dashboard, no cloud account required.

Devices that can't run VPN software themselves — smart TVs, consoles, IoT — get the same protection at the gateway level automatically. One host, one binary, no third-party dashboard.

Learn more at [**wardnet.network**](https://wardnet.network).

## What Wardnet does

### Network protection

- **Per-device VPN routing.** Send the kids' TV through one tunnel, your laptop through another, and the printer direct — or through the default. Policies apply instantly via `ip rule` + nftables.
- **Network-wide ad and tracker blocking.** DNS-level filtering with cron-refreshed blocklists (StevenBlack, OISD, AdGuard, or bring your own), allowlists for exceptions, and custom filter rules — plus per-device filter profiles (Ad Blocking, Parental Controls, Malware & Phishing) so different family members or devices can run different rules. A per-device kill switch still logs what it *would* have blocked.
- **Your own local DNS.** Answer custom domains on your LAN (`nas.home`, `printer.lan`) with your own A/AAAA/CNAME/TXT/MX/SRV records, and forward specific domains to specific upstream resolvers with automatic failover or fastest-server selection.
- **Network Zones.** Put IoT, guest, and kids' devices into locked-down zones enforced at the firewall — no admin-UI access, no bypassing a mandated tunnel — with exceptions for things like casting to a TV in another zone.
- **Built-in DHCP server.** Lease management, static MAC-to-IP reservations, conflict detection, audit trail. Disable your existing DHCP source when you're ready — not before.
- **Automatic device discovery.** ARP scanning plus IEEE OUI vendor lookup (~39k entries embedded in the binary) identifies new devices as they join. Randomised-MAC detection flags modern phones.

### Manage it from anywhere

- **Mobile apps.** Installable User and Admin apps (PWAs) — check status, flip your own routing policy, or manage the whole gateway from your phone. A Premium capability; everything they do is also available free from the desktop admin site.
- **Push notifications.** Get alerted the moment a tunnel drops or a device changes its own routing — no need to keep the dashboard open. Delivered to the mobile apps, and Premium along with them.
- **Automatic HTTPS.** The daemon terminates TLS itself and renews certificates automatically, so you can reach your gateway securely from outside your LAN. Bring your own domain for free, or let Wardnet manage a hostname and certificate for you.
- **WireGuard tunnels on demand.** Add tunnels from a `.conf` file or provision through a provider (NordVPN integration ships today — more to follow). Interfaces come up when needed and tear down after an idle timeout.

### See what's happening

- **Live stats and DNS query logs.** Time-series charts, top-domain and top-client breakdowns, per-tunnel throughput and latency — right on the dashboard, with a live-tailing query log.
- **One-click tunnel verification.** Confirm a VPN routing rule is actually doing what you configured — exit IP, country, and latency, at a glance.

### It just keeps running

- **Self-healing.** A layered watchdog restarts a frozen service — or reboots the host if it has to — and tunnels reconnect automatically if their interface disappears.
- **Signed, safe auto-updates.** Updates apply themselves and roll back automatically on a crash loop, with an opt-in beta channel for early access.
- **Encrypted backup and restore.** Export your entire configuration — devices, policies, tunnels, DNS records, secrets — to one encrypted file, and restore it on new hardware in minutes.

### Built for trust

- **Admin + self-service model.** Admins manage shared devices and set locks; end-users change their own routing policy from an unauthenticated self-service page identified by source IP.
- **Local web dashboard.** Manage everything from one UI. No cloud account, no relay, nothing leaves the LAN unless you ask it to.
- **Single signed binary.** The web UI is embedded into `wardnetd`. Every release is signed with [minisign](https://jedisct1.github.io/minisign/) so you can verify what's running on your gateway.

## Free vs. Premium

The gateway itself is free and fully self-hostable, forever: per-device VPN
routing and WireGuard tunnels, DNS ad and tracker blocking, local DNS,
Network Zones, DHCP, device discovery, encrypted backups, self-healing, and
the full desktop admin site — with automatic HTTPS when you bring your own
domain. No account required, and none of it is time-limited or feature-gated.

**Premium** is an optional, paid add-on covering the parts that cost us real
money to run on your behalf, plus the mobile layer:

- **Dynamic DNS** — reach your gateway with no domain of your own.
- **Secure remote tunneling** with automatic HTTPS — encrypted inbound access
  from anywhere.
- **Roaming private DNS** — keep your filtering with you off the LAN.
- **Personal VPN** — your own inbound WireGuard server, so your phone and
  laptop reach your home network from anywhere.
- **The mobile apps** — the User PWA and the Admin mobile PWA, and the push
  notifications they deliver.

The mobile apps are a convenience layer, not a functionality gate: every
action they offer can be performed for free from the desktop admin site. The
one thing that genuinely needs them is push — alerts are delivered to the
installed apps, so they arrive with them.

See [Premium](https://wardnet.network/docs/premium) for the full breakdown.

## Install

### Run with Docker

```sh
docker run -d \
  --name wardnetd \
  --cap-add NET_ADMIN --cap-add NET_RAW \
  --device /dev/net/tun \
  --sysctl net.ipv4.ip_forward=1 \
  --tmpfs /run --tmpfs /run/lock \
  -p 7411:7411 \
  -v wardnet-data:/var/lib/wardnet \
  ghcr.io/wardnet/wardnetd:latest
```

Open **http://localhost:7411** to complete the setup wizard. Auto-update and crash-loop rollback work inside the container because systemd runs as PID 1, but recreating the container resets to the image's baked-in version — only `docker restart` preserves an auto-updated binary. See [`source/daemon/examples/docker-compose.yaml`](source/daemon/examples/docker-compose.yaml) for a reference compose file with all networking options documented.

### Bare-metal install

For setups where you prefer to run the daemon directly on the host:

```sh
curl -sSL https://wardnet.network/install.sh | sudo bash
```

Supported targets: `aarch64-unknown-linux-gnu` (Raspberry Pi, aarch64 SBCs) and `x86_64-unknown-linux-gnu` (mini-PCs, x86_64 servers).

Can't find your gateway's IP? On first run it's reachable at **http://wardnet.local:7411** from any device on the LAN.

---

Full walkthrough, configuration reference, and guides in the [**user documentation**](https://wardnet.network/docs). See the [latest release](https://github.com/wardnet/wardnet/releases/latest) for signed artefacts and verification instructions.

## Documentation

- [**User documentation**](https://wardnet.network/docs) — installation, configuration, setup walkthrough, guides
- [**Development guide**](docs/DEVELOPMENT.md) — build, run locally, deploy, contribute
- [**Security policy & release signing**](SECURITY.md) — reporting vulnerabilities, verifying releases
- [**AI declaration**](ai-declaration.md) — where and how much AI is used in developing Wardnet
- [**Release notes**](docs/releases/) — per-version changelogs
- [**Marketing site**](https://wardnet.network) — setup walkthrough, screenshots, docs

## Project status

Wardnet is in active development. It's daily-driven on a single Pi at home, but expect rough edges — read the [development guide](docs/DEVELOPMENT.md#project-status) for a full picture of what works today, what's missing, and known caveats. Roadmap and known work-in-flight live in [GitHub issues](https://github.com/wardnet/wardnet/issues), grouped by [milestones](https://github.com/wardnet/wardnet/milestones).

![Repobeats analytics image](https://repobeats.axiom.co/api/embed/542b52b1295b4c6d2f98aee099989eded7862c46.svg "Repobeats analytics image")

## Contributing

Contributions welcome. Start with the [development guide](docs/DEVELOPMENT.md) and the [agent/contributor conventions](AGENTS.md). For security issues, please use [GitHub's private vulnerability reporting](https://github.com/wardnet/wardnet/security/advisories/new) — see [SECURITY.md](SECURITY.md) for details.

## License

The daemon (`source/daemon/`) is [GPL-3.0-or-later](LICENSE) — it links
[`rustables`](https://crates.io/crates/rustables), which is GPL-3.0-or-later,
and Rust links statically, so the compiled binary is a combined work.

The `@wardnet/js` SDK and the web/app frontends stay
[MIT](source/sdk/wardnet-js/LICENSE) — you can build against the SDK without
inheriting GPL obligations.

See [LICENSING.md](LICENSING.md) for the full breakdown.
