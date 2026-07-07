# DHCP server

Wardnet includes a built-in DHCP server so it can hand out addresses
on your LAN directly, rather than relying on your router. Open **DHCP**
to configure it and manage leases.

![DHCP page](/docs/dhcp-server/dhcp-page.png "wide")

## Enabling and configuring

The status card shows whether the server is actually **Running** and
lets you flip the **Enable DHCP** switch, the desired on/off state.
Below it, the configuration card covers:

- **Pool start / end**, the IP range Wardnet hands out.
- **Subnet mask**.
- **Lease duration**.
- **Fallback router**, your real router's IP, handed out as a secondary
  gateway so devices keep a path to the internet if Wardnet is ever
  down.
- **Upstream DNS**, the DNS servers clients are told to use. If Wardnet's
  own [DNS resolver](/docs/dns-ad-blocking) is enabled, this is fixed to
  point at Wardnet itself so ad blocking applies automatically.

Changing the pool range previews which active leases would fall
outside the new range and confirms before revoking them.

## Leases and reservations

Every active lease and static reservation shows up in one table, with
a group filter for **All**, **Reservations**, or **Leases**, plus
search by MAC, hostname, or IP.

![DHCP leases table](/docs/dhcp-server/leases-table.png "wide")

A lease shows an **Active** or **Expired** badge. Click **Make static**
on a lease to turn it into a permanent reservation, pinning that
device to its current IP forever, the form pre-fills from the lease so
you don't have to re-enter the MAC address. **Revoke** ends a lease
immediately. Reservations show a **Static** pill and can be deleted the
same way.

## How this ties into device discovery

DHCP and device discovery are independent, a device doesn't need a
Wardnet lease to show up on the [Devices page](/docs/device-routing),
network traffic alone is enough. But when a device does get a lease,
its hostname is automatically registered as a [`.lan` DNS record](/docs/local-dns#dhcp-issued-lan-records),
and rows in the leases table link straight through to that device's
detail page when the MAC matches a known device.
