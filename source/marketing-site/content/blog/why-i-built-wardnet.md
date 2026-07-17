# My TV can't run a VPN. So I gave it one anyway.

It started with a television.

I wanted my TV's traffic to leave the house through a VPN tunnel. Not
the laptop, not the phones, just the TV. This turns out to be
surprisingly hard, because a TV cannot run a VPN client. Neither can a
games console, a set-top box, a robot vacuum, or the cheap IP camera in
the hallway. The devices you'd most like to keep an eye on are exactly
the ones that give you no way to do it.

The advice you'll find is: put the VPN on your router. So I did, and it
does work, in the sense that a light switch works. It's one switch for
the entire house. Turn it on and every device goes through the tunnel,
which breaks streaming, adds latency to traffic that never needed it,
and means the smart doorbell is now apparently in Amsterdam. Turn it off
and nothing is protected. There's no middle setting, and the middle
setting was the entire thing I wanted.

I already ran Pi-hole, which I liked, and which was never going to help
here. Pi-hole answers DNS. It can refuse to tell a device where an ad
server lives. It cannot decide which way that device's packets leave
your house.

What I actually wanted was per-device: the TV out through a tunnel, the
work laptop through a different one, the printer straight out, and the
NAS left alone. One network, different rules per device, decided in one
place.

So I built it.

## What it looks like

This is the whole idea, in one control. Open a device, choose where its
traffic goes:

![Choosing a device's routing target](/docs/device-routing/routing-edit.png "wide")

Direct, or any tunnel you've configured. That's it. The device isn't
consulted and doesn't need to be. Nothing is installed on the TV,
nothing is configured on the console, and neither of them can opt out or
even tell. The rule lives on the gateway, so it covers every app on the
device, including the ones that ignore system VPN settings entirely.

Change the dropdown and the change is live. No reboot, no re-pairing, no
fighting a router's web UI from 2011.

## Then it grew

Once every device on the network is identified and every packet is
already passing through one box, a lot of things you'd otherwise install
separately become obvious.

**Ad and tracker blocking**, network-wide, for devices that will never
have an ad blocker. It reads the same blocklists Pi-hole does, so if
you're already running one you can bring your lists across. What I
didn't expect is how much I'd use the stats: which client is noisiest,
what's getting blocked, what a device does when nobody's home.

![DNS stats: queries over time, top blocked domains, top clients](/docs/dns-ad-blocking/dns-stats.png "wide")

**Zones**, which is the part I now rely on most. Every device sits in a
zone, and the zone decides what that device is allowed to do: which
tunnels it may use, whether it can reach the gateway's admin pages at
all. The IoT gear goes in a zone that can't touch anything interesting.
Guests go in a zone of their own. It's enforced at the firewall, not by
asking nicely.

![Network zones](/docs/network-zones/zones-page.png "wide")

There's more underneath (it runs your DHCP, answers your local names,
keeps a live query log) but those are the three that changed how I run
my own network.

## What it is now

Wardnet is one signed binary you run on your own hardware. A Raspberry
Pi, a mini-PC, any Linux box. It sits next to your router and takes over
the parts your router is bad at. There's no cloud account, nothing
leaves your network unless you ask it to, and it's GPL-3.0.

Since you'd find out anyway: there's one optional paid add-on. If you
don't want to bring your own domain, Wardnet can manage a hostname and
HTTPS certificate for you; it can carry your DNS filtering with you when
you leave the house, run an inbound WireGuard server so you can reach
home from anywhere, and it's what the mobile apps come with. Those cost
me real money to run, so they cost money to use. Everything above is
free and always will be, and anything the mobile apps do, the desktop
admin site does for nothing.

## The honest part

I run this on my own home LAN. A Raspberry Pi CM5 in my house is the
gateway every device here goes through, on the same signed release you'd
install. That's a real deployment doing a real job, and it's also a
sample size of one.

It's a young project. If you try it, I would genuinely rather hear where
it broke than not hear from you at all.

[github.com/wardnet/wardnet](https://github.com/wardnet/wardnet)
