#!/bin/sh
# LAN-client entrypoint for test_debian / test_ubuntu.
#
# Docker attaches the container to wardnet_lan with an IPAM-assigned
# address in 10.91.0.2/28. We flush that and ask the daemon's DHCP
# server (10.91.0.1, listening on the same bridge) for a lease in its
# .100-.150 dynamic pool. Any post-DHCP address >= .100 unambiguously
# came from the daemon -- Docker's IPAM cannot reach there.
#
# After a lease is acquired, exec the test-agent's HTTP server so the
# Vitest runner can probe interface state, routes, DNS, etc. without
# mounting the container-runtime socket.
set -eu

IFACE="${WARDNET_TEST_IFACE:-eth0}"
PORT="${WARDNET_TEST_AGENT_PORT:-3001}"

# Drop the docker-assigned address. dhclient won't replace an existing
# lease on its own; flushing is the cheapest way to force a fresh
# DISCOVER. Errors here are fatal: a misconfigured CAP_NET_ADMIN drop
# would otherwise produce a confusing "no lease" failure later.
ip addr flush dev "$IFACE"

# `-1` exits non-zero on lease-acquisition failure (vs the default of
# backgrounding and retrying forever), so a broken DHCP server fails
# the container start instead of silently leaving the spec to time
# out. dhclient writes its pidfile under /var/lib/dhcp; the directory
# exists by default in the isc-dhcp-client package.
dhclient -1 -v "$IFACE"

# Hand off to the test-agent. exec replaces shell PID 1 so SIGTERM
# from `compose down` reaches the agent directly.
exec /usr/local/bin/wardnet-test-agent client serve --port "$PORT"
