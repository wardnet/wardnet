# Personal VPN

Turn your Wardnet into your own private VPN. Enable the inbound WireGuard
server and your phone, laptop, or tablet can connect back into your home
network from anywhere, with all the ad blocking, DNS filtering, and
[per-device routing](/docs/device-routing) they get on the LAN following
them wherever they are. No third-party VPN service, no traffic leaving
your own gateway.

This is the inbound counterpart to your outbound
[WireGuard tunnels](/docs/wireguard-tunnels): tunnels send your LAN
devices out through a VPN provider, while the Personal VPN lets your own
devices dial back in. Personal VPN is a Premium capability.

Open **VPN** in the admin site to set it up.

## Enabling the server

![The VPN page: inbound server and granted device peers](/docs/personal-vpn/server-peers.png "wide")

The **Server** card has a single switch. Turn it on and Wardnet stands up
its inbound WireGuard interface and starts listening for your devices.
The listen port defaults to `51821`; leave it unless you have a reason to
change it.

Each device you grant access to shows up under **Peers** with the tunnel
IP it was allocated and whether it is currently connected.

## Granting a device

Only [managed devices](/docs/device-routing) can be granted remote
access, so give a device a name first if you have not already. Then click
**Grant access**, pick the device, and Wardnet generates a fresh
credential for it.

![The generated QR code and config download](/docs/personal-vpn/grant-qr.png "wide")

The credential is shown exactly once, as a QR code and a downloadable
`.conf` file:

- **Scan the QR code** from inside the WireGuard app (choose Add, then
  Scan from QR code). The phone camera on its own just sees text, because
  a WireGuard config is not a link.
- Or **download the `.conf`** and import it into WireGuard on a laptop.

The private key lives only in that one response. Wardnet never stores it,
so this is the only chance to save it. If you lose it, revoke the peer
and grant again for a fresh one.

## Granting from your phone

The whole flow works from the [Admin mobile app](/docs/mobile-apps) too,
which is the natural way to onboard someone standing next to you. Open a
device, and grant remote access right from its sheet.

![Granting remote access from the admin mobile app](/docs/personal-vpn/admin-grant-sheet.png "phone")

The QR code appears on your phone, ready for a family member to scan
straight off your screen into their own WireGuard app.

![The one-time QR code on the admin mobile app](/docs/personal-vpn/admin-grant-qr.png "phone")

## Managing peers

A granted device carries a **Remote** badge in the device list. From
there you can:

- **Pause** a peer to temporarily block its access without deleting the
  credential. Resuming later needs no new QR scan.
- **Revoke** a peer to delete its credential permanently. Re-granting
  afterwards issues a fresh keypair and a fresh QR code.

## Personal VPN vs. remote access

These are two different Premium features that are easy to confuse:

- **[Remote access](/docs/remote-access)** gives *the gateway itself* a
  public hostname and HTTPS certificate so you can reach your admin site
  and mobile apps from outside the LAN.
- **Personal VPN** gives *your devices* an encrypted path back into your
  home network, so their traffic is filtered and routed by Wardnet even
  when they are away.

Many setups use both.
