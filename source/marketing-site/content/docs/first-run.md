# First-time setup

The first time you open Wardnet's web UI, a ten-step wizard walks you
through creating an admin account and configuring the essentials.
Nothing is guessed silently, every step either confirms what Wardnet
auto-detected or asks you to make a choice, and you can jump back to
any completed step from the sidebar before finishing.

## 1. Create admin account

![Create admin account step](/docs/first-run/01-admin.png "wide")

Pick a username and password. These are the credentials you'll use to
sign in to Wardnet going forward, there's no separate recovery flow
yet, so use a password manager.

## 2. Confirm network

![Confirm network step](/docs/first-run/02-network.png "wide")

Wardnet shows the LAN interface, IP, and gateway it detected (or that
`install.sh` set if you passed `--static-ip`). Confirm it's the right
interface, this is the one Wardnet will bind DHCP and DNS to.

## 3. DHCP onboarding

![DHCP onboarding step](/docs/first-run/03-dhcp.png "wide")

Choose how Wardnet handles DHCP on your LAN:

- **Primary (recommended)**, Wardnet runs DHCP. Pick your router model
  from the list and follow the on-screen steps to disable its built-in
  DHCP server first, otherwise you'll have two DHCP servers racing on
  the same LAN.
- **Locked router**, for ISP routers that won't let you disable DHCP.
  You'll manually point each opted-in device at Wardnet as its gateway
  and DNS server instead.

Click **Run a clean probe** once you've disabled the router's DHCP
server. Wardnet listens briefly for other DHCP offers on the LAN and
won't let you continue until it sees only itself responding.

![Clean probe result](/docs/first-run/03b-dhcp-probe.png "wide")

## 4. Router MAC

![Router MAC step](/docs/first-run/04-router.png "wide")

Wardnet ARP-probes your gateway to learn its MAC address, used later
for diagnostics and to keep packet-capture device detection from
misidentifying the router itself as a LAN device. This is automatic,
there's nothing to fill in, if the probe can't complete you can
**Skip** and it'll retry in the background.

## 5. DNS filtering

![DNS filtering step](/docs/first-run/05-dns.png "wide")

Pick a baseline filtering profile applied to every device: ad and
tracker blocking, malware and phishing blocklists, and/or parental
controls. All three are independent toggles, and every one of them can
be fine-tuned or overridden per-device later from the
[DNS ad blocking](/docs/dns-ad-blocking) page.

## 6. First VPN tunnel

![First VPN tunnel step](/docs/first-run/06-tunnel.png "wide")

This step shows any WireGuard tunnels already imported, the same data
the [Tunnels page](/docs/wireguard-tunnels) shows, so it's usually
empty on a brand new install. **Add tunnel** opens the same `.conf`
import flow you'd use from the Tunnels page later, without leaving the
wizard. Both are optional, skip for now and add tunnels any time.

## 7. Default routing policy

![Default routing policy step](/docs/first-run/07-policy.png "wide")

Pick how newly-discovered devices route by default, direct (bypassing
Wardnet's tunnels entirely) or through one of the tunnels you just
configured. With no tunnels yet, this defaults to direct and there's
nothing else to pick, you can override it per-device later from the
[device routing](/docs/device-routing) page.

## 8. Remote access (HTTPS)

![Remote access step](/docs/first-run/08-https.png "wide")

Optional, and a [Premium](/docs/premium) capability. Give Wardnet a
public hostname and a real HTTPS certificate here, or **Skip for now**
and set it up later from Settings, see [remote access](/docs/remote-access)
for the full walkthrough.

## 9. Review

![Review step](/docs/first-run/09-review.png "wide")

Everything you've chosen is already saved, this step is just a
summary. Jump back to any row's **Edit** button to change something
before finishing.

## 10. Done

![Setup complete step](/docs/first-run/10-done.png "wide")

Setup is complete, **Go to dashboard** drops you into the main admin
site. From here, see [backup & restore](/docs/backup-restore) for a
one-click encrypted safety net before you start adding devices and
tunnels.
