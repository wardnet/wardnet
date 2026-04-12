#!/usr/bin/env bash
set -euo pipefail

# Wardnet production install script.
# Run on the target device (Raspberry Pi or similar Linux host).
#
# Usage:
#   sudo ./install.sh [--lan-interface eth1]
#
# What it does:
#   1. Creates the wardnet system user (if not exists)
#   2. Creates directory structure with correct permissions
#   3. Generates a default config file (if not exists)
#   4. Installs the systemd unit
#   5. Enables the service (but does not start it)
#
# The binary must be copied separately (make deploy-pi handles this).

LAN_INTERFACE="eth0"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --lan-interface) LAN_INTERFACE="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "=== Wardnet Production Install ==="
echo "LAN interface: $LAN_INTERFACE"

# 1. Create system user
if ! id wardnet &>/dev/null; then
    echo "Creating wardnet system user..."
    useradd --system --no-create-home --shell /usr/sbin/nologin wardnet
else
    echo "User wardnet already exists"
fi

# 2. Directory structure
echo "Creating directories..."
install -d -o wardnet -g wardnet -m 750 /etc/wardnet
install -d -o wardnet -g wardnet -m 700 /etc/wardnet/keys
install -d -o wardnet -g wardnet -m 750 /var/lib/wardnet
install -d -o wardnet -g wardnet -m 750 /var/log/wardnet

# 3. Default config (don't overwrite existing)
if [ ! -f /etc/wardnet/wardnet.toml ]; then
    echo "Creating default config..."
    cat > /etc/wardnet/wardnet.toml << EOF
[database]
path = "/var/lib/wardnet/wardnet.db"

[logging]
path = "/var/log/wardnet/wardnetd.log"
level = "info"

[network]
lan_interface = "$LAN_INTERFACE"

[tunnel]
keys_dir = "/etc/wardnet/keys"
EOF
    chown wardnet:wardnet /etc/wardnet/wardnet.toml
    chmod 640 /etc/wardnet/wardnet.toml
else
    echo "Config already exists at /etc/wardnet/wardnet.toml"
fi

# 4. Install systemd unit
echo "Installing systemd unit..."
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cp "$SCRIPT_DIR/wardnetd.service" /etc/systemd/system/wardnetd.service
systemctl daemon-reload

# 5. Enable (but don't start)
systemctl enable wardnetd
echo ""
echo "=== Install complete ==="
echo ""
echo "Next steps:"
echo "  1. Copy the wardnetd binary to /usr/local/bin/wardnetd"
echo "  2. Review config: /etc/wardnet/wardnet.toml"
echo "  3. Start the service: sudo systemctl start wardnetd"
echo "  4. Check status: sudo systemctl status wardnetd"
echo "  5. View logs: sudo journalctl -u wardnetd -f"
echo ""
echo "Web UI will be available at http://$(hostname -I | awk '{print $1}'):7411"
