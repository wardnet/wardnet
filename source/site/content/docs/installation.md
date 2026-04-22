# Installation

Wardnet ships as a single, signed Linux binary. The installer handles
everything: downloading the latest release, verifying its signature,
creating a locked-down system user, writing a default configuration, and
starting the systemd service.

## Requirements

- A Raspberry Pi (aarch64) or x86_64 Linux host.
- A Debian/Ubuntu-based distribution (other distros work too, as long as
  the required tools are available — see below).
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
listing the missing packages — it never installs anything behind your
back.

## One-shot install

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
   **embedded in the installer itself** — this is the authenticity
   anchor. A compromised DNS record or CDN cannot forge a signed release.
5. Extract, install the binary owned by the `wardnet` user at
   `/usr/local/bin/wardnetd`, drop the systemd units, enable, and start.

## What the installer sets up

| Path | Purpose |
| --- | --- |
| `/usr/local/bin/wardnetd` | Daemon binary (owned by the `wardnet` user so auto-update can atomically rename it in place). |
| `/etc/wardnet/wardnet.toml` | Configuration. Only written if absent, so re-running the installer preserves tweaks. |
| `/etc/wardnet/keys/` | WireGuard private keys (mode `0700`). |
| `/var/lib/wardnet/` | SQLite database + auto-update staging area. |
| `/var/log/wardnet/` | Daemon log files. |
| `/etc/systemd/system/wardnetd.service` | Main service unit. |
| `/etc/systemd/system/wardnetd-rollback.service` | `OnFailure=` target that rolls back to `<binary>.old` after a crash-loop. |

The `wardnet` system user owns all of the above. The daemon never runs
as root.

## Air-gapped install

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

The installer still verifies SHA-256 and the minisign signature against
its embedded public key — air-gapped mode does not skip verification.

## Choosing a channel

By default the installer pulls from the `stable` channel. To install a
pre-release build, pass `--channel beta`:

```bash
sudo ./install.sh --channel beta
```

You can also switch channels at any time from the daemon's Settings page
(Auto-update card) — the background runner will then track the chosen
channel for future updates.

## Verifying the service

After the installer finishes, it prints the web UI URL, for example:

```
=== Install complete ===
Web UI: http://192.168.1.20:7411
```

On first visit the web UI runs a one-time setup wizard to create the
admin account. From there, the daemon is managed entirely through the
web UI or via `wctl` on the host.

**Next step:** follow the [first-time setup](/docs/first-run) guide to
walk through the wizard. Once you've configured a few devices and
tunnels, head to [backup & restore](/docs/backup-restore) for a
one-click encrypted safety net before you start tinkering.

Useful follow-ups:

```bash
# Service status
sudo systemctl status wardnetd

# Live logs (JSON — pipe through jq to pretty-print)
sudo journalctl -u wardnetd -f

# Quick status from the CLI
sudo -u wardnet wctl status
```

## Upgrades

You never need to re-run `install.sh` for upgrades — the daemon's
auto-update runner polls the release manifest every six hours and, when
enabled, installs new releases in place. You can also trigger a manual
install from the Settings page, or via `wctl update install`.

If an upgrade produces a crash-looping daemon, systemd automatically
fires the `wardnetd-rollback.service` unit after three failures within
120 seconds, which restores the previous binary (`/usr/local/bin/wardnetd.old`).

## Uninstall

```bash
sudo systemctl disable --now wardnetd
sudo rm -f /etc/systemd/system/wardnetd.service
sudo rm -f /etc/systemd/system/wardnetd-rollback.service
sudo rm -f /usr/local/bin/wardnetd /usr/local/bin/wardnetd.old
sudo rm -rf /etc/wardnet /var/lib/wardnet /var/log/wardnet
sudo userdel wardnet
sudo systemctl daemon-reload
```
