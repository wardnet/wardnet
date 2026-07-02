#!/bin/sh
# LAN-client entrypoint for test_debian / test_ubuntu.
#
# Just brings up the test-agent's HTTP server. Lease acquisition is
# spec-driven (the daemon's DHCP service is disabled by default; specs
# enable it in beforeAll then trigger dhclient via /dhcp/renew on this
# agent). Trying to acquire a lease here would race the spec's enable
# call and fail container start.
#
# Compose's service-name DNS resolves test_debian/test_ubuntu to the
# docker-IPAM-assigned IPv4 (10.91.0.2-.15). The custom dhclient
# script (see dhclient-add.sh) preserves that address when applying
# the daemon-issued lease, so the runner can keep reaching the agent
# by name across renews.
set -eu

PORT="${WARDNET_TEST_AGENT_PORT:-3001}"

# Static route for the VPN routing test (issue #248): send traffic for the
# isolated wardnet_internet segment (10.93.0.0/24) to the daemon's LAN address
# (10.91.0.1) so the daemon can steer it through an active tunnel. The daemon
# owns 10.91.0.1 on this connected LAN, so the route installs regardless of
# whether the daemon is answering yet. Harmless to every other spec — nothing
# else routes to 10.93.0.0/24 (without a tunnel the daemon has no path there,
# and Docker's inter-network isolation blocks the host-bridged route).
ip route replace 10.93.0.0/24 via 10.91.0.1 2>/dev/null \
  || echo "client-entrypoint: WARNING could not add route to 10.93.0.0/24" >&2

# exec replaces shell PID 1 so SIGTERM from `compose down` reaches
# the agent directly.
exec /usr/local/bin/wardnet-test-agent client serve --port "$PORT"
