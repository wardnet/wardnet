# Daemon E2E mock services

Container images for the daemon end-to-end topology that need more than a
static file server. Wired into [`../compose.yaml`](../compose.yaml).

## Stage 10 — VPN provider integration (issue #248)

Exercises the daemon's NordVPN provider all the way to a **live WireGuard
tunnel** and proves a LAN device's traffic egresses through it. Two images plus
a target host make up the topology:

| Service           | Image             | Where                              | Role                                                                 |
| ----------------- | ----------------- | ---------------------------------- | -------------------------------------------------------------------- |
| `nordvpn_mock`    | `nordvpn/`        | `wardnet_wan` `10.92.0.52`         | Node HTTP mock of `api.nordvpn.com`, backed by `../fixtures/nordvpn` |
| `wg_gateway`      | `wg-gateway/`     | `wardnet_wan` `.53` + `internet` `.1` | Real WireGuard exit peer; NAT-masquerades tunnel traffic          |
| `internet_target` | `busybox` (compose) | `wardnet_internet` `10.93.0.10`  | "Public internet" host, reachable **only** through the tunnel        |

### How the pieces connect

1. The daemon's config points its NordVPN provider at `nordvpn_mock`
   (`nordvpn_api_url`, injected in `compose.yaml`) instead of the real API.
2. `nordvpn_mock` serves fixtures whose single server advertises the
   `wg_gateway`'s WAN IP as its hostname and the gateway's **public** key, and
   whose `/credentials` returns the **client** private key.
3. The daemon builds a WireGuard config from those, brings the tunnel up (on
   demand, when a device is routed through it), and handshakes with
   `wg_gateway`.
4. `wg_gateway` masquerades tunnel-origin traffic onto `wardnet_internet`.
   Because neither the daemon nor the LAN clients attach to that network and
   Docker isolates bridges from each other, a LAN client reaches
   `internet_target` **iff** its traffic went through the tunnel. The spec
   asserts this with ICMP (`tests/nordvpn-provider.spec.ts`).

The throwaway keypairs and their four coupled locations are documented in
[`../fixtures/nordvpn/README.md`](../fixtures/nordvpn/README.md).

## Stage 9 — Tunnel import lifecycle (issue #247)

Exercises the daemon's **manual** tunnel path — import a WireGuard `.conf`,
bring it up on demand, read its counters, and fall back when it goes down —
independently of any provider. One image serves both peers:

| Service        | Image               | Where                    | Role                                                            |
| -------------- | ------------------- | ------------------------ | --------------------------------------------------------------- |
| `wg_gateway_1` | `wg-tunnel-gateway/`| `wardnet_wan` `10.92.0.54` | Real `wg` peer for `tunnel-a.conf`; answers on `10.9.1.1`       |
| `wg_gateway_2` | `wg-tunnel-gateway/`| `wardnet_wan` `10.92.0.55` | Real `wg` peer for `tunnel-b.conf`; answers on `10.9.2.1`       |

Unlike `wg_gateway` above, these do **no** NAT or forwarding and never touch
`wardnet_internet`: each just brings up `wg0` from its bind-mounted config and
answers ICMP on its own tunnel-inner address. That is enough for the specs to
observe a handshake, read rx/tx via the daemon's `wg show`, and prove
tunnel-only reachability comes and goes with the tunnel. The `tunnel-*.spec.ts`
files import the client side; the coupled keypairs live in
[`../fixtures/tunnels/README.md`](../fixtures/tunnels/README.md).

### Host requirement

`wg_gateway` and the daemon both use the **kernel** WireGuard backend
(netlink), so the Docker host must have the `wireguard` kernel module
available. There is no userspace fallback; without it the tunnel never comes up
and the routing assertion fails.
