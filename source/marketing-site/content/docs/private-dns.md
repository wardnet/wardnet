# Private DNS

Private DNS puts your phone's DNS on your Wardnet **everywhere**: at
home on Wi-Fi, and out on cellular. Your ad blocking, your
[filter profiles](/docs/dns-ad-blocking), and your device's own
[routing](/docs/device-routing) follow the phone off the LAN, and its
queries still show up attributed to it in your query log.

There is **no VPN involved**. Both Android and iOS have a built-in
encrypted-DNS setting, and Private DNS uses it directly, so nothing has
to stay connected and nothing drains the battery beyond the DNS
lookups the phone was making anyway. If you also want the phone
*inside* your home network — reaching your NAS, your printer — that's
[Personal VPN](/docs/personal-vpn), a separate feature you can run
alongside this one.

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
853.

## Granting a device

Private DNS is granted per device, and a device has to have been seen
on your LAN at least once before it can be granted. Pick the device and
click **Grant access**.

Wardnet mints that device a **private hostname** of its own, something
like `k7m2q4bvx8ncr3ea.yourname.my.wardnet.services`. That hostname is
the device's key: your gateway answers encrypted DNS only for hostnames
it has issued, and refuses everything else, so the name is worth keeping
to yourself. Each device gets a different one, which is how your gateway
knows whose queries it's answering.

Every device you grant appears in the list, and **Revoke** cuts one off
immediately, mid-connection, not whenever it next reconnects.

## Setting up the phone

The granted-device dialog shows the steps for both platforms, and the
same steps appear in the [user PWA](/docs/mobile-apps) on the phone
itself under **Settings › Private DNS** — which is usually the easier
route, since the person holding the phone can follow along there. From
the admin site you can also click **Send to device** to push a
notification to the phone that opens straight to those steps, if that
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

1. Scan the QR code in the dialog with the iPhone camera, or open the
   **Download configuration profile** link on the phone itself.
2. Open **Settings** — a **Profile Downloaded** banner appears near the
   top.
3. Tap **Install** and confirm.

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
  your gateway.
- **Both networks, one setting.** The same hostname resolves to your
  gateway directly on the LAN and to Wardnet's edge on cellular, so
  there's nothing to switch when you leave the house.
- **Your gateway sees the queries; Wardnet doesn't.** On cellular the
  connection is relayed through Wardnet's edge still encrypted, and
  terminates on your own gateway.
- **Per-device DNS only.** Private DNS is a phone setting, so it applies
  to the whole phone, but it doesn't route or filter anything for other
  devices on the network it happens to be joined to.
