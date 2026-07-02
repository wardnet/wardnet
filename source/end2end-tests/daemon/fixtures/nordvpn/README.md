# NordVPN mock fixtures (E2E Stage 10, issue #248)

Static fixture data served by the `nordvpn_mock` container. The mock
simulates the subset of the NordVPN API the daemon's VPN provider calls.

- `countries.json` — response for `GET /v1/servers/countries`.
- `servers.json` — response for `GET /v1/servers/recommendations` (and the
  hostname-filtered `GET /v1/servers`). The single server's `hostname` and
  `station` point at the `wg_gateway` container's WAN IP (`10.92.0.53`), and
  `technologies[].metadata` carries the gateway's **public** key so the
  daemon-generated WireGuard config peers with the gateway.
- `credentials.json` — response for `GET /v1/users/services/credentials`;
  `nordlynx_private_key` is the **client** private key the daemon uses as its
  WireGuard interface key.

## Test-only keypairs

These keys are throwaway, generated for this harness — never real credentials.
The pairing must stay consistent across three places:

| Key            | base64                                         | Lives in                                              |
| -------------- | ---------------------------------------------- | ----------------------------------------------------- |
| client private | `ILw1NtV6zCrHMY4O07K5ALelqpFQKfjYnvwPEW3t5X8=` | `credentials.json` (served to the daemon)             |
| client public  | `/Wr553eF5OZqIJz06u6e8Kje0DNJJcarUDnwISJAzQw=` | `mocks/wg-gateway/wg0.conf` `[Peer] PublicKey`        |
| gateway private| `EFPLVL5vbhOhMkV3MTgAptPzdrwqvaxprfOqLYLMDms=` | `mocks/wg-gateway/wg0.conf` `[Interface] PrivateKey`  |
| gateway public | `ess2cB3fECqk9i1BfHjS9myRnokuZwWS1RjT5vmmsgU=` | `servers.json` `technologies[].metadata.public_key`   |

If you regenerate them, update all four locations together.
