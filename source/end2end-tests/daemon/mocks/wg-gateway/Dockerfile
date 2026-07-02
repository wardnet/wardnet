# WireGuard exit-gateway image for the E2E VPN routing topology (issue #248).
# Build context is this directory.
FROM alpine:3.20

# wireguard-tools brings wg + wg-quick; iptables provides the NAT/forward
# rules the entrypoint installs. The WireGuard data path is the host kernel
# module (netlink), so no userspace implementation is bundled.
RUN apk add --no-cache wireguard-tools iptables

COPY wg0.conf /etc/wireguard/wg0.conf
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
