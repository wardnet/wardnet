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

# exec replaces shell PID 1 so SIGTERM from `compose down` reaches
# the agent directly.
exec /usr/local/bin/wardnet-test-agent client serve --port "$PORT"
