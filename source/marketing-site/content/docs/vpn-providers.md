# VPN providers

Wardnet routes device traffic through WireGuard tunnels. You can add a
tunnel two ways: paste a config from any provider that supports
WireGuard, or use a built-in **provider integration** that fetches the
config for you from your account. Either way, the tunnel then appears on
the [Tunnels](/docs/wireguard-tunnels) page and you point devices at it
from [device routing](/docs/device-routing).

## Provider integration

For providers Wardnet integrates with directly, you never touch a
`.conf` file. Enter your credentials, pick a country, and Wardnet
generates and imports the WireGuard config for you. Endpoints that
rotate IPs are re-resolved at bring-up, so you do not need to re-import
when the provider changes servers.

**NordVPN** is the built-in integration today. More providers can be
added over time; anything else works through manual import (below).

### Adding a provider tunnel

1. Go to **Tunnels** and click **Add tunnel**, then open the
   **Provider** tab.
2. Pick your provider from the list.

![Add tunnel, Provider tab](/docs/vpn-providers/provider-tab.png "wide")

3. Enter your credentials. NordVPN uses an **access token** you generate
   from your NordVPN account's manual-configuration page; the field's
   help text links to the exact page. Other providers may use a username
   and password instead.
4. Click **Validate credentials**. Wardnet checks them against the
   provider before going any further, so a typo is caught here rather
   than at connect time.
5. Once validated, pick a **country**. Wardnet lists that country's
   servers with a live load indicator so you can pick a lightly-loaded
   one, or leave it on auto-select for the best available. Advanced
   users can pin a specific **hostname** for a dedicated IP.
6. Optionally give the tunnel a **label**, then click **Create tunnel**.

Wardnet fetches the WireGuard configuration, stores the private key in
its secret store, and adds the tunnel. Route a device at it from the
[device detail page](/docs/device-routing) and the tunnel comes up on
first use.

## Manual import

Any provider that hands you a WireGuard `.conf` works, even ones without
a built-in integration (Mullvad, IVPN, ProtonVPN, your own server). On
the **Add tunnel** panel, use the **Manual** tab, paste or upload the
config, give it a label and country code for the card layout, and save.
See [WireGuard tunnels](/docs/wireguard-tunnels) for the full manual
flow and the tunnel detail page.

## Credentials and security

Provider credentials are used once, to validate and to fetch the tunnel
config. The resulting WireGuard private key is held in Wardnet's
encrypted secret store and is included in an encrypted
[backup](/docs/backup-restore), so restoring on fresh hardware brings
every provider tunnel back without re-entering anything.
