#!/bin/sh
# wg_gateway_{1,2} entrypoint — brings up the WireGuard interface from the
# bind-mounted /etc/wireguard/wg0.conf and keeps PID 1 alive (issue #247, E2E
# Stage 9). No NAT or forwarding: the gateway only needs to complete the
# handshake and answer ICMP on its tunnel-inner address so the bring-up,
# tunnel-stats, and fallback specs can observe the peer + rx/tx counters via
# the daemon's `wg show`.
#
# Requires the WireGuard kernel module on the Docker host plus NET_ADMIN.
set -eu

echo "wg_tunnel_gateway: starting" >&2

if [ ! -f /etc/wireguard/wg0.conf ]; then
  echo "wg_tunnel_gateway: FATAL /etc/wireguard/wg0.conf not mounted" >&2
  exit 1
fi

# wg-quick assigns the [Interface] Address, sets ListenPort, and installs the
# [Peer] AllowedIPs as routes on wg0.
wg-quick up wg0

echo "wg_tunnel_gateway: up" >&2
wg show wg0 || true

# Keep PID 1 alive. `sleep infinity` is unreliable on busybox; this loop is
# portable.
while true; do
  sleep 3600 &
  wait "$!"
done
