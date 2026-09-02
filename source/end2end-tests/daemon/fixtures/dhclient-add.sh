#!/bin/sh
# Custom dhclient script for the e2e test clients.
#
# Why this exists: the default /sbin/dhclient-script flushes the
# interface and applies the leased address as the only IPv4 on the
# NIC. Inside the e2e compose stack that breaks compose's service-name
# DNS, which resolves `test_debian` / `test_ubuntu` to the container's
# docker-IPAM-assigned address (10.91.0.2-.15). After a flush, that
# address is gone and the test runner can no longer reach the agent.
#
# This script ADDS the leased address as a secondary IPv4 (and removes
# only that secondary on release/expire), leaving the docker IPAM
# address untouched. /interfaces will then report both, and the spec
# can assert that an address in the daemon's DHCP pool is present.
#
# It also installs a host route to the lease's router, which the real
# dhclient-script does and which a /32 lease cannot work without. An
# isolate-members zone deliberately advertises a /32 so peers look
# off-link, so the gateway is NOT reachable via the address's own prefix:
# without an explicit route, traffic to the gateway falls through to the
# default route and leaves sourced from the docker address instead of the
# lease. The daemon then never observes the client at its leased address
# and never re-keys the device onto it.

mask_to_prefix() {
    m="$1"; p=0; IFS=.
    for o in $m; do
        case "$o" in
            255) p=$((p+8));;
            254) p=$((p+7));;
            252) p=$((p+6));;
            248) p=$((p+5));;
            240) p=$((p+4));;
            224) p=$((p+3));;
            192) p=$((p+2));;
            128) p=$((p+1));;
            0)   ;;
            *)   return 1;;
        esac
    done
    unset IFS
    echo "$p"
}

case "$reason" in
    BOUND|RENEW|REBIND|REBOOT)
        prefix=$(mask_to_prefix "$new_subnet_mask") || prefix=24
        # `add` errors when the address is already present; ignore so
        # repeated renews are idempotent. The address survives across
        # renews of the same lease, so this is the common path.
        ip addr add "$new_ip_address/$prefix" dev "$interface" 2>/dev/null || true
        # Reach the router from the leased address. `src` pins the source so
        # traffic to the gateway carries the lease, not the docker address —
        # the same preferred-source problem the daemon side of #1198 fixes.
        if [ -n "$new_routers" ]; then
            for r in $new_routers; do
                ip route replace "$r/32" dev "$interface" src "$new_ip_address" \
                    2>/dev/null || true
            done
        fi
        ;;
    EXPIRE|FAIL|RELEASE|STOP)
        prefix=$(mask_to_prefix "$old_subnet_mask") || prefix=24
        if [ -n "$old_routers" ]; then
            for r in $old_routers; do
                ip route del "$r/32" dev "$interface" 2>/dev/null || true
            done
        fi
        ip addr del "$old_ip_address/$prefix" dev "$interface" 2>/dev/null || true
        ;;
esac
exit 0
