# WireGuard exit-gateway image for the E2E VPN routing topology (issue #248).
# Build context is this directory.
FROM alpine:3.24@sha256:294b683cb724975bec92580e1e685676bd4b50bda910ddb8c51d4cabeaec77e6

# wireguard-tools brings wg + wg-quick; iptables provides the NAT/forward
# rules the entrypoint installs. The WireGuard data path is the host kernel
# module (netlink), so no userspace implementation is bundled.
RUN apk add --no-cache wireguard-tools iptables

COPY wg0.conf /etc/wireguard/wg0.conf
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
