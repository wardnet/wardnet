# Device routing

Every device on your LAN gets its own routing rule. Send one phone
through a VPN tunnel, keep the smart TV on the direct connection, and
route the work laptop through a different tunnel entirely. Wardnet
enforces each rule at the gateway, so nothing on the device needs a VPN
app installed.

## The Devices page

Open **Devices** to see every device Wardnet has seen on the network.
Each row shows the device name, type, current IP, and when it was last
seen. Devices are discovered automatically from DHCP and network
traffic, so the list fills in on its own as things connect.

![Devices page](/docs/device-routing/devices-list.png "wide")

The group tabs across the top split the list:

| Group | Shows |
| --- | --- |
| **All** | Every device, discovered or managed. |
| **Managed** | Devices you have named and taken control of. |
| **Unmanaged** | Discovered devices you have not named yet. |
| **Recently seen** | Anything active in the last hour. |

Search by MAC, hostname, or IP to jump straight to a device on a busy
network.

## Managing a device

Click any row to open its detail page. A device is **unmanaged** until
you give it a friendly name. Naming it promotes the device to
**managed**, at which point its routing and DNS-filtering rules take
effect.

Open the **Settings** card and click **Edit** to set:

- **Friendly name**, what the device is called across the UI.
- **Device type**, picks the icon and helps you scan the list.
- **Routing**, where this device's traffic goes.
- **Admin lock**, prevents household members from changing the routing
  from the User app.

![Device settings, editing routing](/docs/device-routing/routing-edit.png "wide")

## Routing targets

The **Routing** selector offers:

- **Direct (no VPN)**, traffic leaves through your normal internet
  connection. This is the default for every device.
- **Via tunnel**, traffic is sent through one of your WireGuard
  [tunnels](/docs/wireguard-tunnels). Pick the tunnel by name; its flag
  and label appear in the selector.

Save, and the change takes effect immediately. Wardnet brings the chosen
tunnel up if it is not already running and starts forwarding that
device's traffic through it. There is nothing to install or configure on
the device itself.

## Admin lock

Turn on **Admin lock** to freeze a device's routing so it can only be
changed from the admin site. This is useful for a child's device you
want pinned to a filtered, tunneled route, or an always-direct device
you never want accidentally sent through a VPN.

## How enforcement works

Routing rules are applied in the gateway's packet-forwarding path, not
on the device. When you point a device at a tunnel, Wardnet matches that
device's traffic by its network address and forwards it through the
tunnel interface. Switch the rule back to **Direct** and the next
packets take the normal path. Because the enforcement lives on the
gateway, the same rule covers every app on the device, including ones
that ignore system VPN settings.
