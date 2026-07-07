# Local DNS

Beyond ad blocking, Wardnet's resolver can be authoritative for your
own domains, hold custom DNS records, and send specific domains to a
chosen upstream server. Open **Local DNS** to manage all three.

![Local DNS page](/docs/local-dns/local-dns-page.png "wide")

## Authoritative zones

An authoritative zone is a domain name, like `home` or `lan`, that
Wardnet answers for directly instead of forwarding upstream. Enable a
zone and any name under it that doesn't match a record you've added
returns NXDOMAIN rather than being sent out to the internet, useful
once you're relying on your own naming for a domain and don't want
typos silently resolving to someone else's server.

Wardnet warns (without blocking you) if a zone name looks like a real
public top-level domain, `.com`, `.io`, and similar, since making
yourself authoritative for one of those means your devices will never
reach the real site under that name.

The built-in `lan` zone can't be deleted, everything DHCP auto-registers
lands here (see below).

## Custom records

Add A, AAAA, CNAME, TXT, MX, or SRV records under **Records**. Each
record has a domain, type, value, TTL, and an optional zone, unzoned
records still resolve, they just aren't covered by an authoritative
zone's catch-all behavior. Toggle a record off to disable it without
deleting it.

This table only shows records you've added by hand. DHCP-generated
`.lan` records are tracked separately, see the next section.

## DHCP-issued `.lan` records

Every device that gets a lease from Wardnet's [DHCP server](/docs/dhcp-server)
is automatically registered as `<hostname>.lan`, so you can reach your
devices by name without configuring anything. These auto-registered
records don't clutter the Records table, the card here just shows how
many exist and links through to the DHCP page where the underlying
leases live.

## Conditional forwarding

A conditional forwarding rule sends queries for one specific domain to
a chosen upstream server, instead of your default resolver. Add a
domain and an upstream IP, useful for split-horizon setups, for example
a work VPN's internal domain that only resolves against your
employer's DNS server.

## Resolution order

For every query, Wardnet checks, in order: is there an active
authoritative zone that matches, does a conditional forwarding rule
match, is the answer already cached, does a [DNS filter](/docs/dns-ad-blocking)
block or rewrite it, and only then does it forward to the matched
upstream (or your default resolver). Authoritative answers and
conditional-forwarding matches both bypass the filter and cache paths
they don't need.
