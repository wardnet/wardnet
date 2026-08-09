# Private DNS

Private DNS puts your phone's DNS on your Wardnet **everywhere**: at
home on Wi-Fi, and out on cellular. Your ad blocking and that device's
own [filter profiles](/docs/dns-ad-blocking) follow the phone off the
LAN, and its queries still show up attributed to it in your query log.

What does **not** follow it is routing. A phone assigned to a
[VPN tunnel](/docs/device-routing) uses that tunnel for its DNS only
while it's on your LAN; off-LAN its lookups go to your gateway's default
upstream, and [domain routing](/docs/domain-routing) rules don't apply
to them. That's inherent to the feature rather than a limitation to work
around: Private DNS carries lookups, not traffic, so there is no egress
path to steer. If you want the phone's actual traffic on your network
when it's away, that's [Personal VPN](/docs/personal-vpn).

There is **no VPN involved**. Both Android and iOS have a built-in
encrypted-DNS setting, and Private DNS uses it directly, so nothing has
to stay connected and nothing drains the battery beyond the DNS lookups
the phone was making anyway. The two features run happily side by side
if you want both.

Private DNS is a Premium capability, and it needs a Wardnet hostname.
Set up [remote access](/docs/remote-access) first, then open **Remote
access** in the admin site to find the Private DNS card.

## Turning it on

The card lists what's needed before the switch will move:

- **Wardnet hostname** — a hostname from Wardnet, not your own domain.
  Private DNS relies on the wildcard certificate and the Wardnet edge
  that come with it.
- **HTTPS certificate** — issued and live. Phones will not accept an
  encrypted-DNS server whose certificate isn't valid, so this one is
  strict.
- **Premium subscription** — active.

It also shows the **reverse tunnel** status. That's informational: the
tunnel is what carries encrypted DNS to your gateway when the phone is
on cellular, so if it's down, home works and roaming doesn't.

Flip the switch and Wardnet starts listening for encrypted DNS on port
853. Enabling also adds a wildcard record to your
[local DNS](/docs/local-dns) pointing `*.yourname.my.wardnet.services` at
your gateway, so granted phones on the LAN reach it directly instead of
hairpinning out through your router. It's worth knowing that record
answers *any* unclaimed subdomain of your hostname for LAN clients while
Private DNS is on; turning the feature off removes it again.

## Granting a device

Private DNS is granted per device, and the device has to be one Wardnet
already **manages** — discovered on your LAN and given a name. Unnamed
devices don't appear in the picker, so name the device first if you
haven't. Then click **Grant access** and choose it from the list.

Wardnet mints that device a **private hostname** of its own, something
like `k7m2q4bvx6ncr3ea.yourname.my.wardnet.services`. That hostname is
the device's key: your gateway answers encrypted DNS only for hostnames
it has issued, and refuses everything else, so the name is worth keeping
to yourself. Each device gets a different one, which is how your gateway
knows whose queries it's answering.

Every device you grant appears in the list, and **Revoke** cuts one off
immediately, mid-connection, not whenever it next reconnects.

## Setting up the phone

**Do this part while the phone is on your home Wi-Fi.** The hostname
itself works anywhere once it's set, but the iOS profile is served only
to a device your gateway recognises by its address, so downloading it
from cellular just returns "not found".

The granted-device dialog shows the steps for both platforms. The same
steps also appear in the [user PWA](/docs/mobile-apps) on the phone
itself, under **Settings › Private DNS** — usually the easier route,
since the person holding the phone can follow along there. (The PWA
shows a direct link rather than a QR code, since a phone can't scan its
own screen.) From the admin site you can also click **Send to device**
to push a notification that opens straight to those steps, if that
household member has notifications turned on.

### Android

Android can't be configured by an app, so the hostname is pasted in by
hand:

1. Copy the device's hostname from the dialog.
2. On the phone, open **Settings › Network & internet › Private DNS**.
3. Choose **Private DNS provider hostname**.
4. Paste the hostname and save.

That's the whole setup, and it applies on Wi-Fi and on mobile data
both. Android is **fail-closed** here: if the hostname is wrong, or your
gateway's certificate has expired, the phone gets no DNS at all rather
than quietly falling back to the carrier's resolver. That's the correct
behaviour for a privacy setting, but it does mean a typo looks like "the
internet is broken" — re-check the hostname first.

### iPhone & iPad

iOS installs a small configuration profile instead:

1. With the iPhone on your home Wi-Fi, scan the QR code in the dialog
   with the camera, or open the **Download configuration profile** link
   on the phone itself.
2. Open **Settings** — a **Profile Downloaded** banner appears near the
   top.
3. Tap **Install** and confirm.

Once installed the profile works on cellular too; only the download step
needs the phone at home.

The profile is signed with your gateway's own certificate, so iOS shows
it as **Verified** rather than warning you about an unsigned profile.
Because it's a profile rather than a per-network setting, it covers
**cellular** as well as Wi-Fi. Downloading it again later replaces the
existing one rather than adding a second copy.

To remove it, go to **Settings › General › VPN & Device Management** and
delete the **wardnet Private DNS** profile.

## What it does and doesn't cover

- **Encrypted DNS only.** DNS-over-TLS today. Your web traffic still
  goes out over the phone's own connection; only the lookups come to
  your gateway — which is also why tunnel and domain routing don't
  apply off-LAN.
- **Both networks, one setting.** The same hostname resolves to your
  gateway directly on the LAN and to Wardnet's edge on cellular, so
  there's nothing to switch when you leave the house.
- **Your gateway sees the queries; Wardnet doesn't.** On cellular the
  connection is relayed through Wardnet's edge still encrypted, and
  terminates on your own gateway.
- **Per-device DNS only.** Private DNS is a phone setting, so it applies
  to the whole phone, but it doesn't route or filter anything for other
  devices on the network it happens to be joined to.
