# Mobile apps

Wardnet ships two separate installable web apps, each scoped to its own
path so they share one host and port with the admin site. Neither
replaces the full [admin site](/docs/device-routing) for configuration
work, they cover different audiences and different day-to-day tasks.

- **User PWA** (`/`), for anyone on the network. No login, self-service
  only: see your own routing, your own DNS activity, and ask an admin
  for a rule change.
- **Admin mobile PWA** (`/admin-app/`), for admins on the go. Logged
  in, read-mostly with a few quick actions, plus push alerts for
  things that need attention.

Both install like any PWA: open the site on a phone and use the
browser's "Add to Home Screen" (or the in-app install prompt, below),
no app store involved.

## The User PWA

There's no login. Wardnet identifies you by your device's IP/MAC on
the LAN, whichever device you're browsing from is the one you're
looking at. If Wardnet doesn't recognize the device yet, every tab
shows a "Device not detected" empty state instead.

### Home

![User PWA home tab](/docs/mobile-apps/user-home.png "phone")

The **Internet route** card lets you switch your own device between
Direct and any tunnel, the same routing choice an admin can set from
the [Devices page](/docs/device-routing), just self-service. If an
admin has locked your routing, this card turns read-only with a lock
icon and a short explanation instead of a picker. Below it, **Network
zone** is always read-only, only an admin can move a device between
zones.

**Verify your route** answers "is this actually working?" without
trusting a status pill: it calls out to an external IP-geolocation
service and plots the result on a map, so you can see, in your own
browser, which country and ISP your traffic is actually leaving from.
A **Match** or **Mismatch** badge compares that against what your
current route should produce, useful for confirming a tunnel is really
carrying your traffic and not silently falling back to Direct.

### Stats

![User PWA stats tab](/docs/mobile-apps/user-stats.png "phone")

Your own [DNS ad blocking](/docs/dns-ad-blocking) activity, scoped to
your device only. Query volume over the last 7 days, a day picker,
headline counts (queries, blocked, allowed), and ranked lists of your
top blocked and most-queried domains. This reads from an on-device
store synced live from the gateway, not a server-side query, so it
keeps working offline for anything already synced.

Tap the request icon next to any domain to ask your administrator to
block or allow it:

![Ask your administrator sheet](/docs/mobile-apps/user-ask-admin.png "phone")

Pick **Block it** or **Allow it**, add an optional note, and send. The
admin decides whether to apply it, you're not changing the filter
yourself. Once you've sent at least one request, a **My requests** list
on the Settings tab shows its status.

### Settings

![User PWA settings tab](/docs/mobile-apps/user-settings.png "phone")

**DNS capture** turns the on-device stats sync off entirely, data
stays local and is never sent anywhere else regardless, this just
stops capturing it. The retention line underneath (event count and
day count) is informational, set by your administrator, not something
you can change here.

**Notifications** enables Web Push for this device, you'll get
notified if an admin locks or changes your routing, even with the app
fully closed. On iOS this requires installing to the Home Screen
first, Safari doesn't support push for an ordinary browser tab.

## The Admin mobile PWA

This one requires an admin login, session cookie plus, on supported
devices, a biometric unlock (Face ID/Touch ID/fingerprint) layered on
top so you're not retyping a password every time you glance at it.
Think of it as a monitoring and quick-response surface, not a
replacement for the desktop admin site when you need to actually
configure something in depth.

### Home

![Admin PWA dashboard](/docs/mobile-apps/admin-dashboard.png "phone")

A single glanceable summary: overall health banner and uptime, devices
online (with how many are on a tunnel), tunnels up (with the busiest
one and its live throughput), and DNS queries plus blocked percentage
for the last 24 hours. Each card links through to its full page
(Devices, Tunnels, DNS) for more detail.

### Devices, Tunnels, DNS

The three linked pages are read-first views of the same data you'd see
on the desktop admin site, sized for a phone. Devices has one quick
action worth calling out: tap a device to open a bottom sheet with its
routing target and zone, so you can re-route or re-zone a device from
the couch without opening the full [Devices page](/docs/device-routing):

![Device routing and zone quick-action sheet](/docs/mobile-apps/admin-device-routing-sheet.png "phone")

### System

![Admin PWA system tab](/docs/mobile-apps/admin-system.png "phone")

Daemon health (version, uptime, CPU, memory, disk) sits above a
**Notifications** section: a toggle for admin-account push alerts,
plus a persisted feed of exactly the kind of thing you'd want to know
about away from your desk, a new device joining, a device's routing
changing, a tunnel going offline. Push subscriptions here are keyed to
your admin account rather than the device, so they survive a session
rotation or a new login.

Below that, **Power** exposes a daemon restart and a full device
reboot, and **Account** has a straight link to the full desktop admin
site plus sign-out.
