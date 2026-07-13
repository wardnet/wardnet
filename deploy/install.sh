#!/usr/bin/env bash
set -euo pipefail

# Wardnet installer.
#
# Default (online) flow — downloads the latest signed release, verifies the
# tarball, creates the daemon user + directory layout, installs the systemd
# units, and starts the service:
#
#   sudo ./install.sh
#
# Offline / air-gapped flow — point at a directory that already holds the
# release artefacts (`wardnetd-<version>-<arch>.tar.gz`, its `.sha256` and
# `.minisig`, and the two `.service` units):
#
#   sudo ./install.sh --from /path/to/release-bundle
#
# Non-interactive overrides (CI, scripted re-runs):
#   sudo LAN_INTERFACE=eth0 ./install.sh
#   sudo CHANNEL=beta ./install.sh

CHANNEL="${CHANNEL:-stable}"
MANIFEST_URL="${MANIFEST_URL:-https://releases.wardnet.network/${CHANNEL}.json}"
LAN_INTERFACE="${LAN_INTERFACE:-}"
STATIC_IP="${STATIC_IP:-}"
OFFLINE_DIR=""
CONTAINER_MODE=""
UPGRADE_ONLY=""

# Embedded release-signing public key. Baking it into the installer is the
# authenticity anchor: even if DNS or Cloudflare is hijacked, an attacker
# can't produce a signed tarball without the matching private counterpart.
# Rotating the key means cutting a new install.sh — intentionally loud.
MINISIGN_PUBLIC_KEY='untrusted comment: minisign public key 020D42D570096F5E
RWRebwlw1UINAqv5q0FJpaQq509v9rZ3ZHvvKi6hgZ/7vd8eoB/QGnQt'

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

print_usage() {
    cat <<EOF
Usage: sudo ./install.sh [OPTIONS]

Options:
  --from <dir>          Install from an already-downloaded release bundle;
                        skips the network download and signature fetch.
  --channel <name>      Release channel to install from (default: stable).
                        Ignored when --from is given.
  --lan-interface <if>  Bind the daemon to this LAN interface. If omitted,
                        the script prompts (tty) or picks the first candidate.
  --static-ip <cidr>    Configure a static IPv4 address on the LAN interface
                        (e.g. 192.168.1.2/24). Writes
                        /etc/dhcpcd.conf.d/wardnet.conf with a routers/dns
                        block derived from the current default gateway. If
                        omitted on a tty install the script prompts; on
                        non-interactive installs the LAN interface keeps
                        whatever address it has (typically DHCP-leased).
                        The setup wizard's network step shows a remediation
                        panel when the LAN IP is still DHCP-derived, so
                        skipping this is safe for users who want to handle
                        addressing later.
  --container-mode      Skip systemctl daemon-reload, start, and restart.
                        Use when running inside a Docker image build: systemd
                        is not running yet, but the enable symlink is still
                        created so systemd starts the service at boot.
  --upgrade-only        Skip interactive prompts and the LAN-interface picker;
                        do not create the wardnet user or rewrite
                        /etc/wardnet/wardnet.toml. Re-installs the binary,
                        systemd units, and the post-upgrade migration framework
                        only. Use this on existing Pis to bootstrap the
                        post-upgrade framework without disturbing operator
                        config. Idempotent.
  -h, --help            Show this help text.

Environment overrides:
  CHANNEL=<name>        Same as --channel.
  LAN_INTERFACE=<if>    Same as --lan-interface.
  STATIC_IP=<cidr>      Same as --static-ip.
  MANIFEST_URL=<url>    Override the release manifest URL (advanced).
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --from)           OFFLINE_DIR="$2";       shift 2 ;;
        --channel)        CHANNEL="$2";           shift 2 ;;
        --lan-interface)  LAN_INTERFACE="$2";     shift 2 ;;
        --static-ip)      STATIC_IP="$2";         shift 2 ;;
        --container-mode) CONTAINER_MODE=true;    shift   ;;
        --upgrade-only)   UPGRADE_ONLY=true;      shift   ;;
        -h|--help)        print_usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; print_usage >&2; exit 1 ;;
    esac
done

if [[ -n "$OFFLINE_DIR" && ! -d "$OFFLINE_DIR" ]]; then
    echo "Error: --from directory '$OFFLINE_DIR' does not exist" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Pre-flight
# ---------------------------------------------------------------------------

if [[ $EUID -ne 0 ]]; then
    echo "Error: install.sh must be run as root (try: sudo $0 $*)" >&2
    exit 1
fi

# Hard-fail on missing deps with a clear remediation. We explicitly do NOT
# auto-install packages: not every distro has apt, and silently pulling in
# packages behind the user's back is exactly the kind of footgun this
# script should avoid.
missing=()
require_cmd() {
    command -v "$1" >/dev/null 2>&1 || missing+=("$1")
}

# Always required (online + offline):
require_cmd tar
require_cmd awk
require_cmd sha256sum
require_cmd uname
require_cmd install
require_cmd systemctl
require_cmd minisign

# Only online mode needs curl + jq (the manifest is JSON).
if [[ -z "$OFFLINE_DIR" ]]; then
    require_cmd curl
    require_cmd jq
fi

if [[ ${#missing[@]} -gt 0 ]]; then
    echo "Error: required commands not installed: ${missing[*]}" >&2
    echo "" >&2
    echo "On Debian/Ubuntu:" >&2
    echo "  sudo apt-get update && sudo apt-get install -y ${missing[*]}" >&2
    echo "" >&2
    if [[ -z "$OFFLINE_DIR" ]]; then
        echo "Alternatively, download the release bundle on another machine and" >&2
        echo "re-run with: sudo ./install.sh --from /path/to/bundle" >&2
    fi
    exit 1
fi

# ---------------------------------------------------------------------------
# Detect arch
# ---------------------------------------------------------------------------

case "$(uname -m)" in
    aarch64|arm64) ARCH="aarch64" ;;
    x86_64|amd64)  ARCH="x86_64"  ;;
    *)
        echo "Error: unsupported CPU architecture '$(uname -m)' (expected aarch64 or x86_64)" >&2
        exit 1
        ;;
esac

# ---------------------------------------------------------------------------
# Detect hardware watchdog (issue #214)
# ---------------------------------------------------------------------------
# The daemon opens /dev/watchdog for the ungated hardware-reboot backstop. The
# device is created by a platform watchdog driver; on the Raspberry Pi that's
# `bcm2835_wdt`, which isn't always autoloaded. Best-effort: if the device is
# absent, try to load the Pi module and re-check. This is purely informational
# — the daemon runs fine without a watchdog (it logs "watchdog unavailable,
# skipping" and skips the hardware layer), so a failure here never aborts the
# install.
detect_watchdog() {
    if [ -c /dev/watchdog ]; then
        echo "Hardware watchdog: /dev/watchdog present."
        return
    fi
    echo "Hardware watchdog: /dev/watchdog not present; attempting to load bcm2835_wdt..."
    if command -v modprobe >/dev/null 2>&1 && modprobe bcm2835_wdt 2>/dev/null && [ -c /dev/watchdog ]; then
        echo "Hardware watchdog: loaded bcm2835_wdt; /dev/watchdog now present."
        # Persist the module load across reboots (best-effort).
        if [ -d /etc/modules-load.d ]; then
            echo "bcm2835_wdt" > /etc/modules-load.d/wardnet-watchdog.conf 2>/dev/null \
                && echo "Hardware watchdog: persisted module via /etc/modules-load.d/wardnet-watchdog.conf." \
                || echo "Hardware watchdog: could not persist module load (non-fatal)."
        fi
    else
        echo "Hardware watchdog: unavailable on this host — the daemon will run" \
             "without the hardware reboot backstop (soft sd_notify watchdog still active)."
    fi
}
detect_watchdog

# ---------------------------------------------------------------------------
# Pick LAN interface
# ---------------------------------------------------------------------------

pick_lan_interface() {
    # iproute2 is standard on modern Linux. Filter loopback + obvious virtual
    # devices so the prompt only offers real LAN candidates.
    mapfile -t candidates < <(
        ip -o link show \
            | awk -F': ' '{print $2}' \
            | awk '{print $1}' \
            | grep -Ev '^(lo|docker|br-|veth|tun|tap|wg|virbr|cni|flannel|cali|kube-|podman|dummy)' \
            | sort -u
    )
    if [[ ${#candidates[@]} -eq 0 ]]; then
        echo "Error: no network interfaces detected. Set LAN_INTERFACE=<iface> and re-run." >&2
        exit 1
    fi

    if [[ -n "$LAN_INTERFACE" ]]; then
        echo "Using LAN interface: $LAN_INTERFACE (from env/flag)"
        return
    fi

    if [[ ! -t 0 ]]; then
        # Piped via `curl | sudo bash` — no tty. Fall back to the first
        # candidate so the install still succeeds unattended, and print the
        # choice so the operator can correct it if it's wrong.
        LAN_INTERFACE="${candidates[0]}"
        echo "Non-interactive install — defaulting LAN interface to: $LAN_INTERFACE"
        echo "Override with LAN_INTERFACE=<iface> if this is wrong, or edit /etc/wardnet/wardnet.toml."
        return
    fi

    echo ""
    echo "Available network interfaces:"
    local i=1
    for iface in "${candidates[@]}"; do
        printf "  %d) %s\n" "$i" "$iface"
        i=$((i + 1))
    done
    printf "Pick the LAN interface [1]: "
    read -r choice
    choice="${choice:-1}"
    if ! [[ "$choice" =~ ^[0-9]+$ ]] || (( choice < 1 || choice > ${#candidates[@]} )); then
        echo "Error: invalid selection '$choice'" >&2
        exit 1
    fi
    LAN_INTERFACE="${candidates[$((choice - 1))]}"
    echo "Using LAN interface: $LAN_INTERFACE"
}

# --upgrade-only skips the picker because the new run reuses the
# LAN interface already recorded in /etc/wardnet/wardnet.toml from
# the original install — no config rewrite, no new prompt.
if [[ -z "$UPGRADE_ONLY" ]]; then
    pick_lan_interface
fi

# ---------------------------------------------------------------------------
# Static IP — operator-selected
# ---------------------------------------------------------------------------
#
# Daemon runs unprivileged with CAP_NET_ADMIN/CAP_NET_RAW only; it cannot
# rewrite /etc/dhcpcd.conf at runtime. To deliver "no terminal needed
# post-install" while keeping the daemon non-root, the install script
# captures the desired static IP up-front and writes the dhcpcd drop-in
# here. The setup wizard's network step is read-only confirmation of the
# resulting state (or a remediation panel if the operator skipped this).

validate_cidr() {
    # Coarse CIDR validation: four octets + /prefix. dhcpcd tolerates a
    # number of edge cases (e.g. /32) so we don't need to be strict; we
    # just want to catch typos before writing the config.
    [[ "$1" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}/[0-9]{1,2}$ ]]
}

write_dhcpcd_dropin() {
    # Resolve the current default gateway so we can carry it forward into
    # the static config. Without an explicit `routers` line, dhcpcd would
    # leave the host with no default route the moment DHCP stops.
    local gateway
    gateway="$(ip -4 route show default 2>/dev/null \
        | awk '/^default/ {print $3; exit}')"
    if [[ -z "$gateway" ]]; then
        echo "Warning: no default gateway detected — the resulting static" >&2
        echo "config will likely break upstream connectivity. Set up the" >&2
        echo "gateway manually and re-run, or skip --static-ip." >&2
    fi

    install -d -m 0755 /etc/dhcpcd.conf.d
    cat > /etc/dhcpcd.conf.d/wardnet.conf <<EOF
# Managed by Wardnet install.sh. Edit this file to change the LAN IP;
# the daemon does not rewrite it at runtime.
interface ${LAN_INTERFACE}
static ip_address=${STATIC_IP}
${gateway:+static routers=${gateway}}
static domain_name_servers=${gateway:-1.1.1.1} 9.9.9.9
EOF
    chmod 0644 /etc/dhcpcd.conf.d/wardnet.conf
    echo "Wrote /etc/dhcpcd.conf.d/wardnet.conf (interface=${LAN_INTERFACE}, ip=${STATIC_IP})"
}

pick_static_ip() {
    if [[ -n "$STATIC_IP" ]]; then
        if ! validate_cidr "$STATIC_IP"; then
            echo "Error: --static-ip value '$STATIC_IP' is not in IPv4 CIDR form (e.g. 192.168.1.2/24)" >&2
            exit 1
        fi
        echo "Using static IP: $STATIC_IP (from env/flag)"
        return
    fi

    if [[ ! -t 0 ]]; then
        echo "Non-interactive install — leaving LAN interface on its current address."
        echo "If the wizard's network step flags it as DHCP-derived, re-run with --static-ip <cidr>."
        return
    fi

    echo ""
    echo "Wardnet should ideally have a stable LAN IP so opted-in devices keep"
    echo "pointing at it across reboots. You can:"
    echo "  - enter a CIDR now (e.g. 192.168.1.2/24) — written to /etc/dhcpcd.conf.d/wardnet.conf"
    echo "  - press Enter to skip and let DHCP assign one (the wizard will warn you)"
    printf "Static IP for %s [skip]: " "$LAN_INTERFACE"
    read -r entered
    entered="${entered:-}"
    if [[ -z "$entered" ]]; then
        echo "Skipping static-IP configuration."
        return
    fi
    if ! validate_cidr "$entered"; then
        echo "Error: '$entered' is not in IPv4 CIDR form (e.g. 192.168.1.2/24)" >&2
        exit 1
    fi
    STATIC_IP="$entered"
}

if [[ -z "$UPGRADE_ONLY" ]]; then
    pick_static_ip
fi

# ---------------------------------------------------------------------------
# Stage release artefacts (online download OR offline pre-unpacked dir)
# ---------------------------------------------------------------------------

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

if [[ -n "$OFFLINE_DIR" ]]; then
    echo "Installing from local bundle: $OFFLINE_DIR"
    # Pick the tarball matching this host's arch. This lets you stage a
    # bundle that carries multiple architectures without the operator
    # needing to name the exact file.
    TARBALL_PATH="$(find "$OFFLINE_DIR" -maxdepth 1 -name "wardnetd-*-${ARCH}.tar.gz" | head -n1)"
    if [[ -z "$TARBALL_PATH" ]]; then
        echo "Error: no 'wardnetd-*-${ARCH}.tar.gz' tarball in $OFFLINE_DIR" >&2
        exit 1
    fi
    TARBALL_NAME="$(basename "$TARBALL_PATH")"
    for ext in sha256 minisig; do
        if [[ ! -f "$OFFLINE_DIR/${TARBALL_NAME}.${ext}" ]]; then
            echo "Error: missing $OFFLINE_DIR/${TARBALL_NAME}.${ext}" >&2
            exit 1
        fi
    done
    cp "$TARBALL_PATH"                              "$WORKDIR/$TARBALL_NAME"
    cp "$OFFLINE_DIR/${TARBALL_NAME}.sha256"        "$WORKDIR/${TARBALL_NAME}.sha256"
    cp "$OFFLINE_DIR/${TARBALL_NAME}.minisig"       "$WORKDIR/${TARBALL_NAME}.minisig"
    # Extract version from filename (wardnetd-<version>-<arch>.tar.gz).
    VERSION="${TARBALL_NAME#wardnetd-}"
    VERSION="${VERSION%-${ARCH}.tar.gz}"
else
    echo "Fetching release manifest from $MANIFEST_URL..."
    curl -fsSL --connect-timeout 15 --max-time 60 --retry 3 --retry-delay 5 "$MANIFEST_URL" -o "$WORKDIR/manifest.json"

    VERSION="$(jq -r '.version'        "$WORKDIR/manifest.json")"
    ASSET_BASE="$(jq -r '.asset_base_url' "$WORKDIR/manifest.json")"
    if [[ -z "$VERSION" || "$VERSION" == "null" ]]; then
        echo "Error: manifest at $MANIFEST_URL has no version (channel '$CHANNEL' has no release yet)" >&2
        exit 1
    fi

    TARBALL_NAME="wardnetd-${VERSION}-${ARCH}.tar.gz"
    TARBALL_URL="${ASSET_BASE%/}/${TARBALL_NAME}"

    echo "Downloading v$VERSION ($ARCH)..."
    curl -fsSL --connect-timeout 15 --max-time 120 --retry 3 --retry-delay 5 "$TARBALL_URL"           -o "$WORKDIR/$TARBALL_NAME"
    curl -fsSL --connect-timeout 15 --max-time 120 --retry 3 --retry-delay 5 "${TARBALL_URL}.sha256"  -o "$WORKDIR/${TARBALL_NAME}.sha256"
    curl -fsSL --connect-timeout 15 --max-time 120 --retry 3 --retry-delay 5 "${TARBALL_URL}.minisig" -o "$WORKDIR/${TARBALL_NAME}.minisig"
fi

# ---------------------------------------------------------------------------
# Verify + extract
# ---------------------------------------------------------------------------

echo "Verifying SHA-256..."
EXPECTED_SHA="$(awk '{print $1}' "$WORKDIR/${TARBALL_NAME}.sha256")"
ACTUAL_SHA="$(sha256sum "$WORKDIR/$TARBALL_NAME" | awk '{print $1}')"
if [[ "$EXPECTED_SHA" != "$ACTUAL_SHA" ]]; then
    echo "Error: SHA-256 mismatch (expected $EXPECTED_SHA, got $ACTUAL_SHA)" >&2
    exit 1
fi

echo "Verifying minisign signature..."
echo "$MINISIGN_PUBLIC_KEY" > "$WORKDIR/wardnet-release.pub"
minisign -V -p "$WORKDIR/wardnet-release.pub" \
    -m "$WORKDIR/$TARBALL_NAME" \
    -x "$WORKDIR/${TARBALL_NAME}.minisig" >/dev/null

echo "Extracting..."
tar -C "$WORKDIR" -xzf "$WORKDIR/$TARBALL_NAME"
if [[ ! -x "$WORKDIR/wardnetd" ]]; then
    echo "Error: tarball did not contain a 'wardnetd' executable" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Install
# ---------------------------------------------------------------------------

if [[ -n "$UPGRADE_ONLY" ]]; then
    echo "=== Wardnet upgrade-only run (v$VERSION) ==="
else
    echo "=== Installing Wardnet v$VERSION ==="
    echo "LAN interface: $LAN_INTERFACE"
fi

# 1. System user. Locked-down account: no shell, no home dir.
if ! id wardnet &>/dev/null; then
    useradd --system --no-create-home --shell /usr/sbin/nologin wardnet
fi

# 2. Directory structure. `/var/lib/wardnet/updates` is wardnet-writable
#    (the daemon stages the next release tarball here for the runner to
#    pick up) and must share a filesystem with `/usr/local/bin/wardnetd`
#    so the runner's final rename is atomic. `/var/lib` qualifies on a
#    typical Debian/Ubuntu install. The secret store lives under
#    `/var/lib/wardnet/secrets` (not `/etc`) because it holds runtime
#    state — generated WireGuard keys, backup passphrases, destination
#    credentials — not static operator configuration.
install -d -o wardnet -g wardnet -m 750 /etc/wardnet
install -d -o wardnet -g wardnet -m 750 /var/lib/wardnet
install -d -o wardnet -g wardnet -m 700 /var/lib/wardnet/secrets
install -d -o wardnet -g wardnet -m 750 /var/lib/wardnet/updates
install -d -o wardnet -g wardnet -m 750 /var/log/wardnet

# 3. Default config — written only when none exists, so re-running (e.g.
#    upgrade that bundles new units) preserves operator tweaks. Skipped
#    explicitly under --upgrade-only so a stray missing config never gets
#    rebuilt with whatever LAN_INTERFACE the picker chose.
if [[ -z "$UPGRADE_ONLY" && ! -f /etc/wardnet/wardnet.toml ]]; then
    cat > /etc/wardnet/wardnet.toml <<EOF
[database]
provider = "sqlite"
connection_string = "/var/lib/wardnet/wardnet.db"

[logging]
path = "/var/log/wardnet/wardnetd.log"
level = "info"

[network]
lan_interface = "$LAN_INTERFACE"

[secret_store]
provider = "file_system"
path = "/var/lib/wardnet/secrets"
EOF
    chown wardnet:wardnet /etc/wardnet/wardnet.toml
    chmod 640 /etc/wardnet/wardnet.toml
fi

# 4. Binary. Owned by wardnet so the daemon process can read+exec it,
#    but the parent /usr/local/bin/ stays root-owned. The atomic swap
#    on auto-update is performed by `wardnet-postupgrade-runner` (root)
#    not the daemon — see step 5 and source/daemon/crates/wardnet-
#    postupgrade-runner/src/swap.rs for the trust anchor.
install -o wardnet -g wardnet -m 0755 "$WORKDIR/wardnetd" /usr/local/bin/wardnetd

# 5. Privileged post-upgrade migration framework. The runner is the trust
#    anchor: root-owned, NOT in wardnetd.service's ReadWritePaths, so the
#    unprivileged wardnet user can't replace it via auto-update. The
#    signed payload + sig live in /var/lib/wardnet/postupgrade/ — wardnet-
#    writable so auto-update can refresh them. The state file lives in
#    /var/lib/wardnet-postupgrade/ (root:root, NOT under the wardnet tree)
#    so the wardnet user can't mark a Required failure as `applied` and
#    bypass the daemon-startup gate. Each install run is idempotent and
#    safe to re-run (used by --upgrade-only on existing Pis).
install -d -o root    -g root    -m 0755 /usr/local/libexec/wardnet
install -d -o root    -g root    -m 0755 /usr/local/libexec/wardnet/runner
install -d -o wardnet -g wardnet -m 0755 /var/lib/wardnet/postupgrade
install -d -o root    -g root    -m 0755 /var/lib/wardnet-postupgrade

if [[ -f "$WORKDIR/wardnet-postupgrade-runner" ]]; then
    install -o root -g root -m 0755 \
        "$WORKDIR/wardnet-postupgrade-runner" \
        /usr/local/libexec/wardnet/runner/wardnet-postupgrade-runner
fi
if [[ -f "$WORKDIR/wardnet-postupgrade.bin" ]]; then
    install -o wardnet -g wardnet -m 0644 \
        "$WORKDIR/wardnet-postupgrade.bin" \
        /var/lib/wardnet/postupgrade/wardnet-postupgrade.bin
fi
if [[ -f "$WORKDIR/wardnet-postupgrade.minisig" ]]; then
    install -o wardnet -g wardnet -m 0644 \
        "$WORKDIR/wardnet-postupgrade.minisig" \
        /var/lib/wardnet/postupgrade/wardnet-postupgrade.minisig
fi

# 5b. Static IP — write the dhcpcd drop-in if the operator chose one.
#     Done before systemd unit install so the file is in place before the
#     daemon starts. We deliberately do NOT restart dhcpcd here: changing
#     the LAN IP mid-install would drop the operator's SSH session. The
#     new address takes effect at the next reboot, which is fine because
#     the wizard's network step will surface the current state and prompt
#     for a reboot when the running IP doesn't match the configured one.
if [[ -z "$UPGRADE_ONLY" && -n "$STATIC_IP" ]]; then
    write_dhcpcd_dropin
fi

# 6. systemd units. The rollback unit is the `OnFailure=` target of the main
#    unit (see wardnetd.service) — both must land together. The post-upgrade
#    unit declares `RequiredBy=wardnetd.service` in its [Install] section,
#    so `systemctl enable wardnet-postupgrade.service` materialises a
#    requires/ symlink that gates wardnetd startup on the runner. Source
#    units from the offline bundle, the script's sibling dir, or (as a
#    last resort) GitHub raw for `curl | sudo bash` runs.
SCRIPT_DIR="$(cd "$(dirname "$0")" 2>/dev/null && pwd)" || SCRIPT_DIR=""
UNIT_BASE="https://raw.githubusercontent.com/wardnet/wardnet/main/deploy"
for unit in wardnetd.service wardnetd-rollback.service wardnet-postupgrade.service; do
    src=""
    if [[ -n "$OFFLINE_DIR" && -f "$OFFLINE_DIR/$unit" ]]; then
        src="$OFFLINE_DIR/$unit"
    elif [[ -n "$SCRIPT_DIR" && -f "$SCRIPT_DIR/$unit" ]]; then
        src="$SCRIPT_DIR/$unit"
    fi
    if [[ -n "$src" ]]; then
        install -m 0644 "$src" "/etc/systemd/system/$unit"
    else
        curl -fsSL --connect-timeout 15 --max-time 60 --retry 3 "$UNIT_BASE/$unit" -o "/etc/systemd/system/$unit"
        chmod 0644 "/etc/systemd/system/$unit"
    fi
done
if [[ -z "$CONTAINER_MODE" ]]; then
    systemctl daemon-reload
fi

# 7. Enable (always — creates the WantedBy symlink so the service starts at
#    boot, plus the wardnetd.service.requires/ symlink for the post-upgrade
#    runner). In container mode systemd is not running yet during the image
#    build, so we skip daemon-reload, the immediate start, and the port wait;
#    systemd will start wardnetd when it initialises as PID 1 at runtime.
if [[ -n "$CONTAINER_MODE" ]]; then
    systemctl enable wardnet-postupgrade.service
    systemctl enable wardnetd
    echo ""
    echo "=== Image build complete ==="
    echo "wardnetd will start when the container initialises (systemd as PID 1)."
else
    systemctl enable wardnet-postupgrade.service
    systemctl enable --now wardnetd
    systemctl restart wardnetd

    # Wait briefly for the daemon to bind its HTTP port so the URL we print is
    # already reachable when the user opens it.
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        if ss -lnt 'sport = :7411' 2>/dev/null | grep -q ':7411'; then
            break
        fi
        sleep 1
    done

    IP=$(hostname -I 2>/dev/null | awk '{print $1}')
    echo ""
    if [[ -n "$UPGRADE_ONLY" ]]; then
        echo "=== Upgrade complete ==="
    else
        echo "=== Install complete ==="
        # The admin site (and with it the first-run setup wizard) is mounted at
        # /admin/ — `/` serves the user PWA, which is not where the operator
        # needs to land on a fresh install.
        echo "Setup wizard: http://${IP:-<host>}:7411/admin/"
    fi
fi
