# Wardnet System Tests

End-to-end tests that verify real traffic routing, WireGuard tunnels, ip rules,
and nftables on a Raspberry Pi.

## Architecture

```
Dev machine (TypeScript + Vitest)
  │
  ├── wardnetd API (@wardnet/js SDK) ──► Pi :7411  (wardnetd)
  │
  └── test agent API (fetch) ──────────► Pi :3001  (wardnet-test-agent)
        /ip-rules, /nft-rules, /wg/:iface
        /link/:iface, /container/exec, /fixtures/:name
```

Tests run on the dev machine. The Pi runs wardnetd, the test agent, and podman
containers (test clients + mock WireGuard peers). All communication is over HTTP.

## Prerequisites

### Dev machine

1. **SSH key authentication to the Pi** (one-time setup):

   ```bash
   ssh-copy-id <user>@<pi-ip>
   ```

   The Makefile uses SSH to deploy binaries and manage the test environment.
   Password-based auth is not supported — key auth must be configured first.

2. **Cross-compilation toolchain** (one-time setup):

   ```bash
   make init
   ```

### Raspberry Pi

The Pi needs a container engine, WireGuard tools, and nftables. All are
installed once and persist across test runs.

**Container engine:** The test runner auto-detects podman or docker. Both work.
We recommend **podman** for the Pi for the following reasons:

- **Daemonless** — no background service consuming memory when containers aren't
  running. On a resource-constrained Pi this matters.
- **Rootless-capable** — runs containers without requiring a daemon running as root.
- **OCI-compatible** — uses the same image format as Docker. Existing compose
  files work without modification.
- **No daemon socket** — no Docker socket to secure, no systemd service to manage.

**Option A: Podman (recommended)**

```bash
sudo apt update
sudo apt install -y podman wireguard-tools nftables curl

# podman-compose: install via pip (the Debian apt package is too old).
sudo pip3 install --break-system-packages podman-compose
```

**Option B: Docker**

```bash
# Follow the official Docker install for your platform, then:
sudo apt install -y wireguard-tools nftables curl
```

Verify:

```bash
# Container engine (one of these)
podman --version          # 4.x+
podman-compose --version  # 1.x+ (only if using podman)
docker --version          # 24.x+ (alternative)

# Required tools
wg --version              # wireguard-tools v1.x
sudo nft --version        # nftables v1.x (nft is in /usr/sbin, needs root)
ip -V                     # iproute2 (pre-installed on Debian/Raspbian)
curl --version            # curl (pre-installed on most distros)
```

**Note:** `nft` lives in `/usr/sbin/` which is not in the normal user PATH.
This is fine — the test runner executes as root via `sudo`.

## Usage

### Full workflow (build, deploy, test, teardown)

```bash
make system-test PI_HOST=10.232.1.10 PI_USER=pgomes
```

### Step-by-step (useful for debugging)

```bash
# Deploy and start the test environment (leave running)
make system-test-setup PI_HOST=10.232.1.10 PI_USER=pgomes

# Run tests from dev machine (re-run as needed while iterating)
cd source/system-tests
WARDNET_PI_IP=10.232.1.10 yarn test

# Tear down when done
make system-test-teardown PI_HOST=10.232.1.10 PI_USER=pgomes
```

## Test suite

| # | File | What it tests |
|---|------|---------------|
| 01 | `01-health.test.ts` | API health, setup wizard, admin login |
| 02 | `02-tunnel-import.test.ts` | Import WireGuard tunnels via API |
| 03 | `03-device-detection.test.ts` | Traffic-based device detection |
| 04 | `04-device-routing.test.ts` | ip rules, nftables masquerade, WireGuard interface |
| 05 | `05-traffic-routing.test.ts` | Ping through tunnel to mock peer |
| 06 | `06-multi-tunnel.test.ts` | Two devices, two tunnels, isolation |
| 07 | `07-idle-teardown.test.ts` | Idle timeout tears down tunnels |

Tests run sequentially — each depends on state from prior tests.

## Project structure

```
source/system-tests/
├── run-tests.sh           # Pi-side lifecycle (setup/teardown)
├── compose.yaml           # Podman: test clients + mock WireGuard peers
├── wardnet-test.env       # Bridge network IPs, ports, credentials
├── vitest.config.ts
├── src/
│   ├── helpers/
│   │   ├── agent.ts       # Test agent client (kernel/container ops)
│   │   ├── client.ts      # SDK client with cookie handling for Node.js
│   │   ├── env.ts         # Environment configuration
│   │   ├── setup.ts       # Shared service instances
│   │   └── state.ts       # Shared mutable state between test files
│   └── tests/
│       └── NN-*.test.ts   # Test files
└── fixtures/generated/    # WireGuard configs (generated on Pi, gitignored)

source/daemon/crates/wardnet-test-agent/
├── src/
│   ├── main.rs            # CLI + axum server
│   ├── kernel/            # ip rule, nft, wg, ip link handlers
│   ├── container.rs       # podman exec handler
│   ├── fixtures.rs        # Fixture file serving
│   └── models.rs          # Request/response types
```
