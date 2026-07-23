# Tunnel-lifecycle fixtures (E2E Stage 9, issue #247)

WireGuard configs for the manual tunnel-import specs
(`tunnel-import`, `tunnel-bringup`, `tunnel-stats`, `tunnel-fallback`).
They exercise the daemon's tunnel lifecycle — import from a `.conf`,
on-demand bring-up when a device is routed through it, live rx/tx
counters, and fallback-to-direct when the tunnel goes away — against two
real `wg` peers on `wardnet_wan`.

This is distinct from the NordVPN routing topology (`../nordvpn`,
`../../mocks/wg-gateway`), which drives the *provider* path to a single
NAT exit gateway. Here nothing is masqueraded: each gateway only
terminates a tunnel and answers ICMP on its own inner address.

## The two tunnels

| Tunnel | Imported by daemon | Gateway       | Gateway WAN IP  | Inner subnet   |
| ------ | ------------------ | ------------- | --------------- | -------------- |
| A      | `tunnel-a.conf`    | `wg_gateway_1`| `10.92.0.54`    | `10.9.1.0/24`  |
| B      | `tunnel-b.conf`    | `wg_gateway_2`| `10.92.0.55`    | `10.9.2.0/24`  |

`gateway-a.conf` / `gateway-b.conf` are the gateways' own `wg0.conf`,
bind-mounted into the containers (see `../../compose.yaml`).

## Keys

Throwaway Curve25519 keypairs, generated once and committed so the specs
are deterministic. They never leave the compose stack. Each gateway
trusts exactly one client public key, and each client dials exactly one
gateway public key — so a successful handshake is itself proof the
daemon imported and brought up the right config.

### Tunnel A

| Role          | Where                                    | Public key                                     |
| ------------- | ---------------------------------------- | ---------------------------------------------- |
| daemon/client | `tunnel-a.conf` `[Interface] PrivateKey` | `7ky5poA0VOVN5qjc9p4ASesMdUW9QGsv9p6wLFyodUk=` |
| gateway       | `gateway-a.conf` `[Interface] PrivateKey`| `GbvcoQUxUYi2H7mKfqtiXhpbiBREvCuEL7rLRcpdUxk=` |

`tunnel-a.conf`'s `[Peer]` is the gateway public key; `gateway-a.conf`'s
`[Peer]` is the client public key.

### Tunnel B

| Role          | Where                                    | Public key                                     |
| ------------- | ---------------------------------------- | ---------------------------------------------- |
| daemon/client | `tunnel-b.conf` `[Interface] PrivateKey` | `eirSvsY/rGOfyAW+0uJpydsfAjocGZf86oOe2jwyRV0=` |
| gateway       | `gateway-b.conf` `[Interface] PrivateKey`| `V4Q3sEsll64SYhbHcwccVNfiEmywa7RALSKY54XBAno=` |

## Host requirement

Both the daemon and the gateways use the **kernel** WireGuard backend
(netlink), so the Docker host must have the `wireguard` module
available — the same requirement the NordVPN routing test carries. There
is no userspace fallback; without it the tunnel never handshakes and the
bring-up / stats / fallback assertions fail.
