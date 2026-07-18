# Installation

Wardnet can be installed via Docker or directly on the host (bare-metal).
Docker is the simpler path, no dependency management, and auto-update +
crash-loop rollback work identically because systemd runs as PID 1 inside
the container.

## Run with Docker

A gateway runs DHCP, DNS, and HTTPS **directly on your LAN**, so how you
network the container matters more than which ports you publish. Pick one
of the three modes below.

### Host networking (recommended)

The daemon shares the host's network stack and binds the host's interfaces
directly: web UI on 7411, DNS on 53, DHCP on 67, automatic HTTPS on 80/443,
plus WireGuard. DHCP broadcasts reach real LAN clients, and there's nothing
to publish. This is the simplest setup and behaves just like a bare-metal
install.

```bash
# Enable IP forwarding on the host (persists across reboots):
echo 'net.ipv4.ip_forward=1' | sudo tee /etc/sysctl.d/99-wardnet.conf
sudo sysctl --system

docker run -d \
  --name wardnetd \
  --network host \
  --cap-add NET_ADMIN --cap-add NET_RAW \
  --device /dev/net/tun \
  --tmpfs /run --tmpfs /run/lock \
  -v wardnet-data:/var/lib/wardnet \
  ghcr.io/wardnet/wardnetd:latest
```

Open **http://localhost:7411** to complete the setup wizard.

| Flag | Why |
| --- | --- |
| `--network host` | Puts the daemon on the LAN directly, so DHCP broadcasts reach clients and DNS/HTTPS bind the real interfaces. |
| `--cap-add NET_ADMIN` | Create/configure WireGuard interfaces, manage nftables and `ip rule`. |
| `--cap-add NET_RAW` | Raw sockets for the packet-capture device detector. |
| `--device /dev/net/tun` | WireGuard tunnels use the tun device. |
| `--tmpfs /run --tmpfs /run/lock` | systemd (PID 1) needs a writable, non-persistent `/run`. |
| `-v wardnet-data:/var/lib/wardnet` | Persistent state: database, WireGuard keys, staged updates. |

`net.ipv4.ip_forward` is enabled on the **host** (above), not via
`docker run --sysctl`, because Docker rejects network sysctls when a
container shares the host network namespace. Host networking also needs
ports 53/67/80/443/7411 free on the host, so run it on a box dedicated to
Wardnet (for example, don't leave `systemd-resolved` bound to `:53`).

### Its own LAN IP with macvlan (advanced)

If Wardnet shares a host with other services, give the container its **own
MAC and IP on the LAN** with a macvlan network, so it doesn't collide with
the host's ports and clients point at a dedicated gateway address:

```bash
# Replace eth0 with your LAN NIC, and the subnet/gateway/IP with your LAN's.
docker network create -d macvlan \
  --subnet 192.168.1.0/24 --gateway 192.168.1.1 \
  -o parent=eth0 wardnet-lan

docker run -d \
  --name wardnetd \
  --network wardnet-lan --ip 192.168.1.2 \
  --cap-add NET_ADMIN --cap-add NET_RAW \
  --device /dev/net/tun \
  --tmpfs /run --tmpfs /run/lock \
  -v wardnet-data:/var/lib/wardnet \
  ghcr.io/wardnet/wardnetd:latest
```

macvlan caveats worth knowing before you choose it:

- **The Docker host can't reach the container** over the network by default
  (and vice versa); it's a macvlan limitation. If you need host-to-daemon
  access, add a macvlan "shim" interface on the host.
- The chosen IP (`192.168.1.2` above) must sit **outside** both your
  router's DHCP pool and the pool Wardnet hands out.
- It needs the NIC in promiscuous mode: fine on wired NICs, but **most WiFi
  access points reject macvlan**, and it doesn't work on Docker Desktop
  (macOS/Windows).
- Enable `ip_forward` on the host, same as the host-networking mode above.

### Bridge with published ports (web UI, DNS, and tunnels only)

To keep your **existing router's DHCP** and use Wardnet only for the
dashboard, DNS ad-blocking, and tunnel management, a normal bridge with
published ports is enough; point your clients' DNS at the Docker **host's**
IP. This mode **cannot run Wardnet's DHCP server**: DHCP relies on LAN
broadcast, which a NAT bridge doesn't carry.

```bash
docker run -d \
  --name wardnetd \
  --cap-add NET_ADMIN --cap-add NET_RAW \
  --device /dev/net/tun \
  --sysctl net.ipv4.ip_forward=1 \
  --tmpfs /run --tmpfs /run/lock \
  -p 7411:7411 \
  -p 53:53/tcp -p 53:53/udp \
  -v wardnet-data:/var/lib/wardnet \
  ghcr.io/wardnet/wardnetd:latest
```

Add `-p 80:80 -p 443:443` to serve automatic HTTPS for remote access here
rather than through Premium tunneling. In this mode `--sysctl
net.ipv4.ip_forward=1` works, because the container has its own bridged
network namespace.

A reference compose file covering all three modes is at
[`source/daemon/examples/docker-compose.yaml`](https://github.com/wardnet/wardnet/blob/main/source/daemon/examples/docker-compose.yaml).

### Auto-update in Docker

The daemon's built-in auto-update runner works inside the container:
systemd restarts `wardnetd` in place, and `wardnetd-rollback.service`
fires on crash-loop just as it does on bare metal. One caveat: recreating
the container (`docker rm` + `docker run`) resets to the image's baked-in
version. Use `docker restart` to preserve an auto-updated binary, or
re-pull a newer image tag.

## Bare-metal install

### Requirements

- A Raspberry Pi (aarch64) or x86_64 Linux host.
- A Debian/Ubuntu-based distribution (other distros work too, as long as
  the required tools are available, see below).
- Root access on the target machine.
- Outbound HTTPS to `wardnet.network` (release manifest + tarball download).

The installer requires these tools to be present:

| Tool | Used for |
| --- | --- |
| `curl` | Fetching the manifest and release artefacts |
| `tar` | Unpacking the release tarball |
| `sha256sum` | Verifying the tarball digest |
| `minisign` | Verifying the release signature |
| `jq` | Parsing the release manifest JSON |
| `systemctl`, `install`, `awk`, `uname` | Standard install plumbing |

On a fresh Debian/Ubuntu image:

```bash
sudo apt-get update
sudo apt-get install -y curl tar minisign jq
```

If any tool is missing, the installer fails early with a clear message
listing the missing packages, it never installs anything behind your
back.

### One-shot install

```bash
curl -sSL https://wardnet.network/install.sh | sudo bash
```

When a TTY is attached, the installer prompts for which network interface
to bind to. Set `LAN_INTERFACE=<iface>` to skip the prompt (required when
piping through `sudo bash`, otherwise the installer picks the first
plausible interface).

Verification flow the installer runs, in order:

1. Fetch `https://releases.wardnet.network/stable.json` (the release manifest).
2. Download `wardnetd-<version>-<arch>.tar.gz` plus its `.sha256` and
   `.minisig` sidecars.
3. Recompute the SHA-256 and compare against the sidecar.
4. Verify the `.minisig` signature against the public key that is
   **embedded in the installer itself**, this is the authenticity
   anchor. A compromised DNS record or CDN cannot forge a signed release.
5. Extract, install the binary owned by the `wardnet` user at
   `/usr/local/bin/wardnetd`, drop the systemd units, enable, and start.

### What the installer sets up

| Path | Purpose |
| --- | --- |
| `/usr/local/bin/wardnetd` | Daemon binary (owned by the `wardnet` user so auto-update can atomically rename it in place). |
| `/etc/wardnet/wardnet.toml` | Configuration. Only written if absent, so re-running the installer preserves tweaks. |
| `/etc/wardnet/keys/` | WireGuard private keys (mode `0700`). |
| `/var/lib/wardnet/` | SQLite database + auto-update staging area. |
| `/var/log/wardnet/` | Daemon log files. |
| `/etc/systemd/system/wardnetd.service` | Main service unit. |
| `/etc/systemd/system/wardnetd-rollback.service` | `OnFailure=` target that rolls back to `<binary>.old` after a crash-loop. |
| `/etc/systemd/system/wardnet-postupgrade.service` | Runs one-off migrations shipped with an upgrade, when a release includes them. |
| `/usr/local/libexec/wardnet/runner/wardnet-postupgrade-runner` | Root-owned post-upgrade migration runner. |
| `/var/lib/wardnet/postupgrade/`, `/var/lib/wardnet-postupgrade/` | Post-upgrade migration state. |
| `/etc/sysctl.d/99-wardnet.conf` | Enables `net.ipv4.ip_forward=1` so per-device VPN routing can forward LAN traffic through the tunnels. |

The `wardnet` system user owns all of the above (except the root-owned
post-upgrade runner, which needs elevated privileges to apply
migrations). The daemon itself never runs as root.

Two install-time flags worth knowing about:

- `--static-ip <address>` writes `/etc/dhcpcd.conf.d/wardnet.conf` to pin
  the host's own LAN IP, useful on a dedicated gateway where you don't
  want the address to drift.
- `--upgrade-only` re-runs the installer idempotently, skipping user and
  config creation, handy for scripted upgrade flows that just need the
  binary and units refreshed.

### Air-gapped install

No outbound network from the target machine? Download the release bundle
on a machine that does have internet, copy it across, and point the
installer at the directory:

```bash
sudo ./install.sh --from /path/to/release-bundle
```

The bundle directory must contain:

- `wardnetd-<version>-<arch>.tar.gz`
- `wardnetd-<version>-<arch>.tar.gz.sha256`
- `wardnetd-<version>-<arch>.tar.gz.minisig`
- `wardnetd.service`, `wardnetd-rollback.service`
- `wardnet-postupgrade.service` (only needed if the release includes a
  post-upgrade migration step, the installer skips it silently when
  absent from the bundle)

The installer still verifies SHA-256 and the minisign signature against
its embedded public key, air-gapped mode does not skip verification.

### Choosing a channel

By default the installer pulls from the `stable` channel. To install a
pre-release build, pass `--channel beta`:

```bash
sudo ./install.sh --channel beta
```

You can also switch channels at any time from the daemon's Settings page
(Auto-update card), the background runner will then track the chosen
channel for future updates.

### Verifying the service

After the installer finishes, it prints the web UI URL, for example:

```
=== Install complete ===
Web UI: http://192.168.1.20:7411
```

On first visit the web UI runs a one-time setup wizard to create the
admin account. From there, the daemon is managed entirely through the
web UI.

**Next step:** follow the [first-time setup](/docs/first-run) guide to
walk through the wizard. Once you've configured a few devices and
tunnels, head to [backup & restore](/docs/backup-restore) for a
one-click encrypted safety net before you start tinkering.

Useful follow-ups:

```bash
# Service status
sudo systemctl status wardnetd

# Live logs (JSON, pipe through jq to pretty-print)
sudo journalctl -u wardnetd -f
```

### Upgrades

You never need to re-run `install.sh` for upgrades, the daemon's
auto-update runner polls the release manifest every six hours and, when
enabled, installs new releases in place. You can also trigger a manual
install from the Settings page.

If an upgrade produces a crash-looping daemon, systemd automatically
fires the `wardnetd-rollback.service` unit after three failures within
120 seconds, which restores the previous binary (`/usr/local/bin/wardnetd.old`).

### Uninstall

```bash
sudo systemctl disable --now wardnetd
sudo systemctl disable --now wardnet-postupgrade.service
sudo rm -f /etc/systemd/system/wardnetd.service
sudo rm -f /etc/systemd/system/wardnetd-rollback.service
sudo rm -f /etc/systemd/system/wardnet-postupgrade.service
sudo rm -f /usr/local/bin/wardnetd /usr/local/bin/wardnetd.old
sudo rm -rf /etc/wardnet /var/lib/wardnet /var/log/wardnet
sudo rm -rf /usr/local/libexec/wardnet /var/lib/wardnet-postupgrade
sudo rm -f /etc/sysctl.d/99-wardnet.conf
sudo rm -f /etc/dhcpcd.conf.d/wardnet.conf
sudo userdel wardnet
sudo systemctl daemon-reload
```

This removes everything the installer created. Two notes:

- `/etc/dhcpcd.conf.d/wardnet.conf` only exists if you installed with
  `--static-ip`; the `rm -f` is a harmless no-op otherwise.
- Removing `/etc/sysctl.d/99-wardnet.conf` stops IP forwarding from
  persisting across reboots, but the running kernel keeps
  `net.ipv4.ip_forward=1` until the next reboot. Leave the setting alone
  if anything else on the host relies on forwarding.

**This deletes your configuration and data** (the SQLite database,
WireGuard keys, and secrets under `/var/lib/wardnet`). Take an
[encrypted backup](/docs/backup-restore) first if you might want it back.
