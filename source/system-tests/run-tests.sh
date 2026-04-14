#!/usr/bin/env bash
# Wardnet system test lifecycle manager.
#
# Runs on the Pi. Manages the daemon, test containers, mock WireGuard peers,
# and the test agent. The actual tests run remotely (TypeScript + Vitest on
# the dev machine).
#
# Usage:
#   sudo ./run-tests.sh setup      # Start everything
#   sudo ./run-tests.sh teardown   # Stop everything
#   sudo ./run-tests.sh            # Setup, wait for signal, teardown

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Load and export environment (export needed for podman-compose substitution).
set -a
# shellcheck source=wardnet-test.env
source "${SCRIPT_DIR}/wardnet-test.env"
set +a

# -- Configuration -----------------------------------------------------------

WARDNETD_BIN="${WARDNETD_BIN:-/usr/local/bin/wardnetd}"
TEST_AGENT_BIN="${TEST_AGENT_BIN:-/usr/local/bin/wardnet-test-agent}"
WORK_DIR="/tmp/wardnet-system-test"

# Auto-detect container runtime: prefer podman, fall back to docker.
if command -v podman > /dev/null 2>&1; then
    CT_ENGINE="podman"
    if command -v podman-compose > /dev/null 2>&1; then
        CT_COMPOSE="podman-compose"
    else
        CT_COMPOSE="podman compose"
    fi
elif command -v docker > /dev/null 2>&1; then
    CT_ENGINE="docker"
    CT_COMPOSE="docker compose"
else
    log_error "No container engine found. Install podman (recommended) or docker."
    exit 1
fi
DB_PATH="${WORK_DIR}/wardnet-test.db"
LOG_PATH="${WORK_DIR}/wardnetd.log"
CONFIG_PATH="${WORK_DIR}/wardnet-test.toml"
KEYS_DIR="${WORK_DIR}/keys"
FIXTURES_DIR="${SCRIPT_DIR}/fixtures/generated"

# Colors.
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
RESET='\033[0m'

log_info()  { echo -e "${BLUE}[INFO]${RESET}  $*"; }
log_ok()    { echo -e "${GREEN}[OK]${RESET}    $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
log_error() { echo -e "${RED}[ERROR]${RESET} $*"; }

# -- Preflight checks -------------------------------------------------------

preflight() {
    log_info "Running preflight checks..."

    if [[ $EUID -ne 0 ]]; then
        log_error "System tests must run as root (needed for kernel operations)"
        exit 1
    fi

    for tool in ip nft wg curl; do
        if ! command -v "$tool" > /dev/null 2>&1; then
            log_error "Required tool not found: ${tool}"
            exit 1
        fi
    done

    log_info "Container engine: ${CT_ENGINE} (compose: ${CT_COMPOSE})"

    if [[ ! -x "$WARDNETD_BIN" ]]; then
        log_error "wardnetd binary not found at ${WARDNETD_BIN}"
        exit 1
    fi

    if [[ ! -x "$TEST_AGENT_BIN" ]]; then
        log_error "wardnet-test-agent binary not found at ${TEST_AGENT_BIN}"
        exit 1
    fi

    log_ok "Preflight checks passed"
}

# -- Key generation ----------------------------------------------------------

generate_keys() {
    mkdir -p "$FIXTURES_DIR"

    if [[ -f "${FIXTURES_DIR}/peer1-wg0.conf" && -f "${FIXTURES_DIR}/peer2-wg0.conf" \
       && -f "${FIXTURES_DIR}/tunnel-1.conf" && -f "${FIXTURES_DIR}/tunnel-2.conf" ]]; then
        log_info "WireGuard keys already generated, skipping"
        return
    fi

    log_info "Generating WireGuard keypairs..."

    local peer1_privkey peer1_pubkey
    peer1_privkey=$(wg genkey)
    peer1_pubkey=$(echo "$peer1_privkey" | wg pubkey)

    local peer2_privkey peer2_pubkey
    peer2_privkey=$(wg genkey)
    peer2_pubkey=$(echo "$peer2_privkey" | wg pubkey)

    local daemon1_privkey daemon1_pubkey
    daemon1_privkey=$(wg genkey)
    daemon1_pubkey=$(echo "$daemon1_privkey" | wg pubkey)

    local daemon2_privkey daemon2_pubkey
    daemon2_privkey=$(wg genkey)
    daemon2_pubkey=$(echo "$daemon2_privkey" | wg pubkey)

    # Peer configs (used by mock peer containers).
    cat > "${FIXTURES_DIR}/peer1-wg0.conf" <<EOF
[Interface]
ListenPort = ${MOCK_PEER_1_PORT}
PrivateKey = ${peer1_privkey}

[Peer]
PublicKey = ${daemon1_pubkey}
AllowedIPs = 0.0.0.0/0
EOF

    cat > "${FIXTURES_DIR}/peer2-wg0.conf" <<EOF
[Interface]
ListenPort = ${MOCK_PEER_2_PORT}
PrivateKey = ${peer2_privkey}

[Peer]
PublicKey = ${daemon2_pubkey}
AllowedIPs = 0.0.0.0/0
EOF

    # Tunnel configs (imported into wardnetd via API).
    cat > "${FIXTURES_DIR}/tunnel-1.conf" <<EOF
[Interface]
PrivateKey = ${daemon1_privkey}
Address = 10.99.1.2/24
DNS = 1.1.1.1

[Peer]
PublicKey = ${peer1_pubkey}
Endpoint = ${MOCK_PEER_1_IP}:${MOCK_PEER_1_PORT}
AllowedIPs = 0.0.0.0/0, ::/0
PersistentKeepalive = 25
EOF

    cat > "${FIXTURES_DIR}/tunnel-2.conf" <<EOF
[Interface]
PrivateKey = ${daemon2_privkey}
Address = 10.99.2.2/24
DNS = 1.1.1.1

[Peer]
PublicKey = ${peer2_pubkey}
Endpoint = ${MOCK_PEER_2_IP}:${MOCK_PEER_2_PORT}
AllowedIPs = 0.0.0.0/0, ::/0
PersistentKeepalive = 25
EOF

    echo "$peer1_pubkey" > "${FIXTURES_DIR}/peer1.pub"
    echo "$peer2_pubkey" > "${FIXTURES_DIR}/peer2.pub"

    log_ok "WireGuard keys generated"
}

# -- Daemon configuration ---------------------------------------------------

detect_bridge_interface() {
    # Find the bridge interface created by podman for the wardnet_test network.
    local iface
    iface=$($CT_ENGINE network inspect wardnet_test 2>/dev/null \
        | grep -o '"bridge": "[^"]*"' | head -1 | sed 's/"bridge": "//;s/"//')

    if [[ -z "$iface" ]]; then
        # Fallback: look for common podman bridge names.
        for candidate in podman1 cni-podman1 br-wardnet; do
            if ip link show "$candidate" > /dev/null 2>&1; then
                iface="$candidate"
                break
            fi
        done
    fi

    if [[ -z "$iface" ]]; then
        log_warn "Could not detect bridge interface, defaulting to eth0"
        iface="eth0"
    fi

    echo "$iface"
}

write_daemon_config() {
    local bridge_iface="$1"
    mkdir -p "$WORK_DIR" "$KEYS_DIR"

    cat > "$CONFIG_PATH" <<EOF
[server]
host = "0.0.0.0"
port = ${WARDNET_API_PORT}

[database]
path = "${DB_PATH}"

[logging]
level = "debug"
path = "${LOG_PATH}"
format = "console"
rotation = "never"
max_log_files = 1

[logging.filters]

[auth]
session_expiry_hours = 24

[network]
lan_interface = "${bridge_iface}"
default_policy = "direct"

[detection]
enabled = true

[tunnel]
keys_dir = "${KEYS_DIR}"
stats_interval_secs = 30
health_check_interval_secs = 60
idle_timeout_secs = ${WARDNET_IDLE_TIMEOUT}

[otel]
enabled = false
endpoint = ""
service_name = "wardnetd-test"

[providers.enabled]
nordvpn = false
EOF
}

# -- Setup -------------------------------------------------------------------

setup() {
    log_info "Setting up test environment..."

    # Clean previous state.
    rm -rf "$WORK_DIR"
    mkdir -p "$WORK_DIR"

    # Generate WireGuard keys.
    generate_keys

    # Start test containers.
    log_info "Starting podman compose..."
    cd "$SCRIPT_DIR"
    $CT_COMPOSE up -d 2>&1 | while read -r line; do
        log_info "  compose: $line"
    done

    # Detect the bridge interface.
    local bridge_iface
    bridge_iface=$(detect_bridge_interface)
    log_info "Detected bridge interface: ${bridge_iface}"

    # Write daemon config.
    write_daemon_config "$bridge_iface"

    # Start wardnetd.
    log_info "Starting wardnetd..."
    "$WARDNETD_BIN" --config "$CONFIG_PATH" > "${LOG_PATH}" 2>&1 &
    local wardnetd_pid=$!
    echo "$wardnetd_pid" > "${WORK_DIR}/wardnetd.pid"
    log_info "wardnetd started (PID ${wardnetd_pid})"

    # Wait for API.
    log_info "Waiting for wardnetd API..."
    local attempts=0
    while ! curl -sf "http://localhost:${WARDNET_API_PORT}/api/info" > /dev/null 2>&1; do
        attempts=$((attempts + 1))
        if [[ $attempts -ge 30 ]]; then
            log_error "wardnetd failed to start. Log tail:"
            tail -20 "$LOG_PATH" 2>/dev/null || true
            exit 1
        fi
        sleep 0.5
    done
    log_ok "wardnetd API is ready"

    # Start test agent.
    log_info "Starting wardnet-test-agent..."
    "$TEST_AGENT_BIN" --port 3001 --fixtures-dir "${FIXTURES_DIR}" > "${WORK_DIR}/test-agent.log" 2>&1 &
    local agent_pid=$!
    echo "$agent_pid" > "${WORK_DIR}/test-agent.pid"
    log_info "test-agent started (PID ${agent_pid})"

    # Wait for test agent.
    attempts=0
    while ! curl -sf "http://localhost:3001/health" > /dev/null 2>&1; do
        attempts=$((attempts + 1))
        if [[ $attempts -ge 20 ]]; then
            log_error "test-agent failed to start. Log tail:"
            tail -10 "${WORK_DIR}/test-agent.log" 2>/dev/null || true
            exit 1
        fi
        sleep 0.5
    done
    log_ok "test-agent is ready"

    # Wait for containers.
    log_info "Waiting for containers..."
    for container in wardnet_test_alpine wardnet_test_ubuntu; do
        local c=0
        while ! $CT_ENGINE exec "$container" true 2>/dev/null; do
            c=$((c + 1))
            if [[ $c -ge 60 ]]; then
                log_error "Container ${container} not ready after 30s"
                exit 1
            fi
            sleep 0.5
        done
    done
    for container in wardnet_mock_wg_peer_1 wardnet_mock_wg_peer_2; do
        local c=0
        while ! $CT_ENGINE exec "$container" wg show wg0 2>/dev/null; do
            c=$((c + 1))
            if [[ $c -ge 60 ]]; then
                log_warn "Mock peer ${container} WireGuard not ready after 30s"
                break
            fi
            sleep 0.5
        done
    done

    log_ok "Test environment is ready"
    echo ""
    echo -e "${BOLD}Run tests from your dev machine:${RESET}"
    echo -e "  WARDNET_PI_IP=$(hostname -I | awk '{print $1}') yarn test"
    echo ""
}

# -- Teardown ----------------------------------------------------------------

teardown() {
    log_info "Tearing down test environment..."

    # Stop test agent.
    if [[ -f "${WORK_DIR}/test-agent.pid" ]]; then
        local pid
        pid=$(cat "${WORK_DIR}/test-agent.pid")
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    fi

    # Stop wardnetd.
    if [[ -f "${WORK_DIR}/wardnetd.pid" ]]; then
        local pid
        pid=$(cat "${WORK_DIR}/wardnetd.pid")
        if kill -0 "$pid" 2>/dev/null; then
            log_info "Stopping wardnetd (PID ${pid})..."
            kill "$pid" 2>/dev/null || true
            for _ in $(seq 1 50); do
                if ! kill -0 "$pid" 2>/dev/null; then
                    break
                fi
                sleep 0.1
            done
            if kill -0 "$pid" 2>/dev/null; then
                log_warn "wardnetd did not stop gracefully, sending SIGKILL"
                kill -9 "$pid" 2>/dev/null || true
            fi
        fi
    fi

    # Stop containers.
    cd "$SCRIPT_DIR"
    $CT_COMPOSE down 2>/dev/null || true

    # Clean up leftover WireGuard interfaces.
    for iface in $(ip link show 2>/dev/null | grep -o 'wg_ward[0-9]*' || true); do
        log_info "Cleaning up leftover interface: ${iface}"
        ip link del "$iface" 2>/dev/null || true
    done

    log_ok "Teardown complete"
}

# -- Main --------------------------------------------------------------------

case "${1:-}" in
    setup)
        preflight
        setup
        ;;
    teardown)
        teardown
        ;;
    "")
        # Interactive mode: setup, wait, teardown.
        preflight
        trap teardown EXIT
        setup
        log_info "Environment is up. Press Ctrl+C to teardown."
        # Wait indefinitely.
        while true; do sleep 60; done
        ;;
    *)
        echo "Usage: $0 [setup|teardown]"
        exit 1
        ;;
esac
