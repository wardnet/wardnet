# WireGuard tunnels

Wardnet routes any device on your LAN through a WireGuard tunnel of your
choice. Tunnels are managed entirely from the web UI: import a `.conf`
file from your provider, click into the tunnel to inspect its health,
then point one or more devices at it from the Devices page.

There are two ways to add a tunnel:

- **Manual import**, paste or upload a `.conf` from any provider that
  supports WireGuard (Mullvad, IVPN, your own server). Wardnet parses the
  interface and peer config, persists the tunnel, and stores the private
  key in the secret store.
- **Provider integration**, for providers Wardnet has a built-in
  integration with (NordVPN today), enter your credentials, pick a
  country and server, and Wardnet generates and imports the config for
  you. See [VPN providers](/docs/vpn-providers).

## The tunnel detail page

Click any tunnel card on the Tunnels page to open its detail page at
`/tunnels/<id>`. Everything you need to know about the tunnel lives
here:

![Tunnel detail page](/docs/wireguard-tunnels/detail-overview.png "wide")

The header shows the tunnel's status pill (`Active`, `Connecting`,
`Reconnecting`, `Down`) and the time since the last WireGuard handshake.
Below it, the **Configuration** card surfaces provider, country,
endpoint, and the local interface name in one glance.

### Throughput chart

The throughput chart plots upload (tx) and download (rx) rates
side-by-side. Wardnet samples WireGuard's cumulative byte counters every
five minutes, stores the deltas, and converts them to bytes/sec for
display.

![Throughput chart](/docs/wireguard-tunnels/throughput-chart.png "wide")

You can switch the visible window with the range toggle:

| Range | Sampled from | Sample interval |
| --- | --- | --- |
| `1h` / `6h` / `24h` / `48h` | Intraday table | 5 min |
| `12mo` | Daily rollup table | 1 day |

Above the chart, the **Window total** callout shows the cumulative
upload and download bytes for the visible window. Drag the brush handles
below the chart to zoom into a sub-window, the totals update to match
the selection.

If a tunnel has just been imported, the chart will show an empty state
until the first 5-minute sample lands. Counter resets (e.g. after
`wg-quick down && wg-quick up`) are detected automatically: the next
sample's delta is the new counter value, never a negative spike.

### Retention

Wardnet keeps:

- **48 hours** of intraday samples (~576 rows per active tunnel).
- **12 months** of daily rollups.

A background runner trims past-retention rows once an hour. When you
delete a tunnel, its history is removed too, the foreign key on the
metrics tables uses `ON DELETE CASCADE`.

### Devices using this tunnel

Below the chart, the **Devices using this tunnel** table lists every
device whose routing rule points at this tunnel. To re-route a device,
open the Devices page and edit the device, the table updates within
30 seconds.

![Devices table](/docs/wireguard-tunnels/devices-table.png "wide")

## Importing a tunnel

1. Go to **Tunnels → Add tunnel** and pick **Manual**.
2. Paste the contents of your `.conf` file or upload it.
3. Give the tunnel a label and pick its country code, these are used
   for the card layout on the list page (flag + label).

Wardnet generates a fresh interface name (`wg_ward0`, `wg_ward1`, …) on
import; you don't need to set one yourself. The peer's endpoint can be
either an IP:port or a hostname, Wardnet resolves hostnames at
bring-up time, so providers that rotate IPs (NordVPN, ProtonVPN) work
without re-importing the config.

## Bringing a tunnel up and down

Tunnels start in the `down` state. Set a device's routing target to
this tunnel (Devices page) and Wardnet brings the interface up
automatically when the first packet would route through it. The
status transitions:

```
down → connecting → up
                  ↓
                  reconnecting (handshake gone stale)
                  ↓
                  up (handshake observed again)
```

`connecting` means the kernel interface is configured but the peer
hasn't replied yet. `reconnecting` means a previously-established
handshake has aged past the 3-minute stale threshold; the interface
stays configured and recovers automatically when the peer becomes
reachable again. Both states are visible on the detail page header.

## Deleting a tunnel

Click **Delete tunnel** at the bottom of the detail page. Wardnet:

1. Reroutes any devices currently using the tunnel back to **Direct**
   so they don't lose connectivity.
2. Tears down the WireGuard interface in the kernel.
3. Removes the persisted config and private key.
4. Cascades and removes the tunnel's metrics history.

The action is destructive, the tunnel can be re-imported but the
history is gone.

## Backup and restore

Tunnel configs, private keys, and the full metrics history are all
captured by **Backup & restore** (see
[Backup & restore](/docs/backup-restore)). Restoring a backup on a
fresh install brings every tunnel back exactly as it was, including the
chart history.
