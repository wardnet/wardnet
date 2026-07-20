# Domain routing

Some traffic should follow the destination, not the device. You might want
every request to `netflix.com` to leave through a UK tunnel, no matter which
TV, phone, or laptop made it, while everything else keeps taking its normal
path. Domain routing does exactly that: you group domain rules into a
**routing profile** and assign the profile to the devices that should honour
it. Wardnet enforces the rule at the gateway, so nothing on the device needs a
VPN app installed.

This is separate from [device routing](/docs/device-routing). A device rule
decides where *all* of a device's traffic goes; a domain rule overrides that
for *matched destinations only*. The two work together — a laptop can sit on
the direct connection yet still send a handful of streaming domains through a
tunnel.

## Routing profiles

Open **Routing** and create a **profile** — a named set of domain rules, like
"Streaming UK" or "Work". Each rule pairs a **domain pattern** with a
**target**:

- **Via tunnel**, matched traffic is sent through one of your WireGuard
  [tunnels](/docs/wireguard-tunnels). Pick the tunnel by name; its flag and
  exit country appear in the selector.
- **Direct**, matched traffic is pulled *out* of whatever tunnel the device is
  otherwise using and sent through your normal internet connection. This is the
  useful inverse — keep your banking or a work domain on the local connection
  even while the rest of the device is tunnelled.

A pattern is either an exact name or a wildcard:

| Pattern | Matches |
| --- | --- |
| `netflix.com` | Only `netflix.com`. |
| `*.netflix.com` | `netflix.com` and every subdomain (`www.netflix.com`, `api.netflix.com`, …). |

A single `*.netflix.com` rule therefore covers a whole service.

## Assigning profiles to devices

A profile does nothing until you assign it. On a device, add one or more
routing profiles in **priority order**. The order is the tie-breaker: if two
assigned profiles both match a domain, the one **higher in the list wins**, so
you decide precedence by dragging profiles up or down rather than fighting over
which rule is "more specific". A device with no profile just follows its normal
device routing.

## How enforcement works

Wardnet is your network's DNS resolver, so it sees every lookup. When a device
resolves a domain that one of its profiles matches, Wardnet notes the answer's
IP addresses and installs a routing rule that sends traffic to those addresses
through the chosen tunnel — for that device. The rule is tied to the DNS
record's lifetime and is cleaned up automatically when it expires, so the set
of pinned addresses stays current as a service moves around.

Because the decision is made from the DNS answer and enforced at the gateway,
the same rule covers every app on the device, and it works even for devices —
smart TVs, consoles — that can't run VPN software themselves.

## Good to know

- **Shared CDNs.** Big services often sit behind shared content-delivery
  networks (CloudFront, Akamai, Fastly), where one IP address can serve many
  unrelated sites. Routing a domain that resolves to a shared address can pull
  some of that other traffic through the same tunnel. It's usually harmless,
  but worth knowing if a rule seems to catch more than you expected.
- **Apps that ignore your DNS.** A device or app that hardcodes its own DNS
  resolver bypasses Wardnet, so domain routing can't see those lookups. Point
  your devices at Wardnet for DNS to get the most out of it.
