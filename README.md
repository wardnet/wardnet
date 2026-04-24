<div align="center">

<img src="artwork/logo-256x256.png" alt="Wardnet logo" width="160" />

# Wardnet

**Your network. Your rules.**

</div>

[![CI](https://github.com/wardnet/wardnet/actions/workflows/ci.yml/badge.svg)](https://github.com/wardnet/wardnet/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/wardnet/wardnet/branch/main/graph/badge.svg)](https://codecov.io/gh/wardnet/wardnet)
[![Rust](https://img.shields.io/badge/rust-1.95-orange.svg)](https://www.rust-lang.org)
[![Rust Report Card](https://rust-reportcard.xuri.me/badge/github.com/wardnet/wardnet)](https://rust-reportcard.xuri.me/report/github.com/wardnet/wardnet)
[![Security Audit](https://github.com/wardnet/wardnet/actions/workflows/security.yml/badge.svg)](https://github.com/wardnet/wardnet/actions/workflows/security.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/wardnet/wardnet/badge)](https://securityscorecards.dev/viewer/?uri=github.com/wardnet/wardnet)
[![Dependabot](https://badgen.net/github/dependabot/wardnet/wardnet)](https://github.com/wardnet/wardnet/pulls?q=is%3Apr+author%3Aapp%2Fdependabot)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Wardnet is a self-hosted network privacy gateway you run on your own hardware. It sits alongside your existing home or small-office router and acts as the warden of every device's connection to the internet — encrypting traffic through per-device VPN tunnels, blocking ads and trackers at the DNS level, and giving you full control from a local web dashboard.

**Think of it as a Pi-hole replacement with per-device VPN routing built in.** Network-wide ad and tracker blocking (you can bring your existing Pi-hole blocklists) plus WireGuard tunnels you can assign to individual devices — in one signed binary, one dashboard, no cloud.

Devices that can't run VPN software themselves — smart TVs, consoles, IoT — get the same protection at the gateway level. One host, one binary, no cloud account, no third-party dashboard.

Learn more at [**wardnet.network**](https://wardnet.network).

## What Wardnet does

- **Per-device VPN routing.** Send the kids' TV through one tunnel, your laptop through another, and the printer direct — or through the default. Policies apply instantly via `ip rule` + nftables.
- **Network-wide ad and tracker blocking.** DNS-level filtering with cron-refreshed blocklists (StevenBlack, OISD, AdGuard, or bring your own), allowlists for exceptions, and custom filter rules. Applies to every device on the LAN regardless of routing.
- **Built-in DHCP server.** Lease management, static MAC-to-IP reservations, conflict detection, audit trail. Disable your existing DHCP source when you're ready — not before.
- **Automatic device discovery.** ARP scanning plus IEEE OUI vendor lookup (~39k entries embedded in the binary) identifies new devices as they join. Randomised-MAC detection flags modern phones.
- **WireGuard tunnels on demand.** Add tunnels from a `.conf` file or provision through a provider (NordVPN integration ships today — more to follow). Interfaces come up when needed and tear down after an idle timeout.
- **Admin + self-service model.** Admins manage shared devices and set locks; end-users change their own routing policy from an unauthenticated self-service page identified by source IP.
- **Local web dashboard.** Manage everything from one UI. No cloud account, no relay, nothing leaves the LAN.
- **Single signed binary.** The web UI is embedded into `wardnetd`. Every release is signed with [minisign](https://jedisct1.github.io/minisign/) so you can verify what's running on your gateway.

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

---

Full walkthrough, configuration reference, and guides in the [**user documentation**](https://wardnet.network/docs). See the [latest release](https://github.com/wardnet/wardnet/releases/latest) for signed artefacts and verification instructions.

## Documentation

- [**User documentation**](https://wardnet.network/docs) — installation, configuration, setup walkthrough, guides
- [**Development guide**](docs/DEVELOPMENT.md) — build, run locally, deploy, contribute
- [**Security policy & release signing**](SECURITY.md) — reporting vulnerabilities, verifying releases
- [**Release notes**](docs/releases/) — per-version changelogs
- [**Marketing site**](https://wardnet.network) — setup walkthrough, screenshots, docs

## Project status

Wardnet is in active development. It's daily-driven on a single Pi at home, but expect rough edges — read the [development guide](docs/DEVELOPMENT.md#project-status) for a full picture of what works today, what's missing, and known caveats.

## Contributing

Contributions welcome. Start with the [development guide](docs/DEVELOPMENT.md) and the [agent/contributor conventions](AGENTS.md). For security issues, please use [GitHub's private vulnerability reporting](https://github.com/wardnet/wardnet/security/advisories/new) — see [SECURITY.md](SECURITY.md) for details.

## License

[MIT](LICENSE)
