#!/bin/sh
# wg_gateway entrypoint — brings up the WireGuard exit interface and NATs
# tunnel-origin traffic out to the isolated wardnet_internet network so the
# daemon's tunnelled LAN clients can reach internet_target (issue #248).
#
# Requires the WireGuard kernel module on the Docker host (kernel/netlink
# backend, same as the daemon) plus NET_ADMIN. There is no userspace fallback.
set -eu

echo "wg_gateway: starting" >&2

# Enable forwarding between wg0 and the internet-facing NIC.
sysctl -w net.ipv4.ip_forward=1 >/dev/null 2>&1 \
  || echo "wg_gateway: WARNING could not set net.ipv4.ip_forward" >&2

# Bring up the tunnel. wg-quick assigns the [Interface] Address and adds the
# AllowedIPs route (10.5.0.0/24 dev wg0).
wg-quick up wg0

# Identify the NIC Docker gave the wardnet_internet (10.93.0.0/24) address.
# Docker does not guarantee a stable ethN ordering across multi-network
# containers, so discover it rather than hardcoding.
INET_IFACE=$(ip -o -4 addr show | awk '/inet 10\.93\./{print $2; exit}')
if [ -z "$INET_IFACE" ]; then
  echo "wg_gateway: FATAL no NIC on 10.93.0.0/24" >&2
  exit 1
fi

# Masquerade tunnel-origin traffic (10.5.0.0/24) onto the internet net so the
# target can reply back through the gateway, and permit the forwarding both
# ways.
iptables -t nat -A POSTROUTING -s 10.5.0.0/24 -o "$INET_IFACE" -j MASQUERADE
iptables -A FORWARD -i wg0 -o "$INET_IFACE" -j ACCEPT
iptables -A FORWARD -i "$INET_IFACE" -o wg0 -m state --state RELATED,ESTABLISHED -j ACCEPT

echo "wg_gateway: up — wg0 <-> $INET_IFACE masquerade active" >&2
wg show wg0 || true

# Keep PID 1 alive. `sleep infinity` is unreliable on busybox; this loop is
# portable.
while true; do
  sleep 3600 &
  wait "$!"
done
