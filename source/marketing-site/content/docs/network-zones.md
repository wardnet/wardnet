# Network zones

Zones group devices into policy buckets that gate routing and isolate
them from the rest of your network. Instead of managing rules
device-by-device, put a device in a zone and it inherits that zone's
routing and access rules automatically.

## The Zones page

Open **Zones** to see every zone and manage them.

![Zones page](/docs/network-zones/zones-page.png "wide")

Wardnet ships with three zones out of the box:

- **Trusted**, the **home** zone (full trust, admin surfaces reachable).
  Every newly-discovered device starts here until you set up quarantine
  (below).
- **IoT**, for smart devices you don't want reaching the admin site.
- **Guest**, for visitors.

Each zone defines:

- **Isolation stance**, shared subnet or isolate members from each
  other.
- **Allowed routing**, which targets this zone's devices may use:
  direct, tunnel, or both. At least one must stay on.
- **Zone subnet**, an optional CIDR for devices in this zone. Requires
  Wardnet to be your DHCP server; recorded but inactive otherwise.
- **Admin UI reachable**, whether devices in this zone can reach
  Wardnet's admin surfaces at all.

Exactly one zone is the **home** zone (full trust, can't be deleted)
and one is **default for new devices**, where freshly-discovered
devices land. Use the row actions to promote a zone to either role, or
to delete a zone that has no members.

## Assigning a device to a zone

Open a device's [detail page](/docs/device-routing) and edit its
**Zone** card.

![Editing a device's zone](/docs/network-zones/device-zone-edit.png "wide")

The disclaimer under the picker is the one thing to keep in mind:
zones are isolated from each other, but devices *within* the same zone
are only isolated from one another if your access point provides
client isolation, or you turn on **member isolation** for the zone.
Wardnet enforces zone boundaries at the gateway; it can't isolate
peers that talk directly over the same Wi-Fi network.

## Cross-zone exceptions

Sometimes you want a narrow hole punched through an isolation
boundary, for example a phone in Trusted casting to a TV in IoT.
The **Cross-zone exceptions** table on the Zones page handles this: pick
a From and To (a zone or a specific device), a service (a preset
bundle like casting, or a custom protocol and port range), and Wardnet
opens exactly that path, in both directions.

## New-device quarantine

Turn on **Notify admins about new devices** to get a push notification
whenever an unrecognized device joins the network. The **default zone
for new devices** setting decides where those devices land, point it
at a locked-down zone (like Guest) if you want new devices quarantined
until you've had a chance to look at them.

The **Awaiting review** list shows every device currently sitting in
the default-for-new zone. Pick a target zone and click **Approve** to
move it out.

## How enforcement works

Every zone rule is enforced at the gateway, not on the device. Two
independent checks run per device IP:

- An **egress gate** blocks traffic to VPN tunnels or the direct
  internet path, whichever the zone doesn't allow.
- An **admin-UI gate** resets connections from non-admin-reachable
  zones to Wardnet's admin ports, while leaving DNS and DHCP untouched
  so the device still gets connectivity.

Both reload live when you change a zone or move a device, no restart
required.
