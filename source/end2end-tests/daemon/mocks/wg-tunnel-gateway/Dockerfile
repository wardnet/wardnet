# Generic WireGuard peer image for the E2E tunnel-lifecycle topology
# (issue #247, Stage 9). Unlike mocks/wg-gateway — the NAT exit gateway for the
# NordVPN routing test — this image does no forwarding or masquerade: it just
# brings up a single wg0 interface from a bind-mounted config and answers on its
# tunnel-inner address, so a routed device can handshake and ping through the
# tunnel. Two instances (wg_gateway_1 / wg_gateway_2) run from it, each supplied
# its own keypair + inner subnet via the mounted wg0.conf.
#
# Build context is this directory.
FROM alpine:3.20@sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc

# wireguard-tools brings wg + wg-quick. The data path is the host kernel module
# (netlink), same as the daemon-under-test — there is no userspace fallback.
RUN apk add --no-cache wireguard-tools

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# wg0.conf is bind-mounted at runtime (per-instance keypair), not baked in.
ENTRYPOINT ["/entrypoint.sh"]
