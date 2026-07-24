# DNS ad blocking

Wardnet is the DNS resolver for your whole network, so it can block ads,
trackers, and unwanted domains for every device at once, no per-device
app, no browser extension. A request for a known ad domain never
resolves, so the ad never loads and the tracker never phones home.

Filtering is organised into **profiles**. A profile bundles blocklists,
an allowlist, and custom rules, and you assign profiles to devices. One
profile can cover the whole house while a stricter one covers the kids'
devices.

## The DNS Filtering page

Open **DNS Filtering** to manage the master switch and your profiles.

![DNS Filtering page](/docs/dns-ad-blocking/filtering-page.png "wide")

The settings card at the top is the network-wide switch. When filtering
is off, every profile is bypassed and queries are answered without
blocking, useful for a quick diagnosis of "is Wardnet blocking this?"
without editing any lists.

Below it, the profile table lists each profile with a count of its
blocked, allowed, and custom entries. The **Default** toggle on a row
marks that profile as part of the default set applied to any device you
have not given an explicit profile. Wardnet ships with a builtin
**Ad Blocking** profile so filtering works the moment you turn it on.

## Inside a profile

Click a profile to open it. Each profile has three sections.

![Profile detail](/docs/dns-ad-blocking/profile-detail.png "wide")

### Blocklists

Blocklists are the bulk of the blocking. Each one is a URL to a hosts
file or ad-block-style list (Steven Black, AdGuard, and similar formats
all work). Wardnet downloads the list, compiles the domains, and
refreshes on a schedule you set per list. Toggle a list off to disable
it without deleting it.

### Allowlist

The allowlist is your escape hatch. Add a domain here and it is never
blocked under this profile, even if a blocklist contains it. Use it when
an over-eager list breaks a site you need, for example a work login that
shares a domain with an ad network.

### Custom rules

Custom rules are individual AdGuard / ABP syntax rules, for example
`||tracker.example.net^` to block one domain and its subdomains. Use
these for targeted blocks or allows that do not warrant a whole list.

## Seeing what was blocked

The **DNS query log** shows every query passing through Wardnet, live or
historical, with the result of each: forwarded, cache hit, blocked, or
rewritten.

![DNS query log](/docs/dns-ad-blocking/query-log.png "wide")

Filter by device, by result (show only **blocked**), or by domain to see
exactly what a given device is reaching for. The log is where you
confirm a blocklist is doing its job, or spot a domain to add to the
allowlist. When the network-wide switch is off, queries that a profile
would have blocked are marked **blocked (skipped)** so you can still see
what filtering would catch.

## Stats at a glance

The main **DNS** page (the resolver overview, separate from DNS
Filtering) rolls query volume and blocking into a set of stat cards and
a chart, so you don't have to read the raw log to see how filtering is
performing.

![DNS stats](/docs/dns-ad-blocking/dns-stats.png "wide")

At the top, four cards cover the selected window: total **queries**,
the **blocked** percentage, the single most-blocked domain, and the
count of **active clients**. The **queries over time** chart plots
total queries against blocked queries side by side, drag the range
toggle between `1h` and `12mo` to zoom out and spot trends, like a new
device flooding the network with tracker requests. Below the chart,
**top blocked domains** and **top clients** rank the busiest offenders
and the chattiest devices for the same window, a quick way to find
which device to investigate or which domain is worth adding a custom
rule for.

Underneath, a second row goes deeper. A **compared to previous period**
panel puts this window's query and blocked totals next to the window
before it, so a week-over-week jump in either is obvious at a glance.
**Top trackers blocked** takes the domains that got blocked and groups
them by the company behind them, matched against a curated tracker
list, so instead of a wall of opaque hostnames you see which
organisations your network is turning away most. And a **per-device
queries** chart plots one device's query volume over time, pick a
device from the dropdown to see when it's busy and whether that lines
up with when someone's actually using it.

## Per-device profiles

Assign a profile to a specific device from its
[device detail page](/docs/device-routing). A device with no explicit
profile falls back to the default set. This is how one network runs a
relaxed profile on the adults' devices and a strict one on a child's
tablet, all from the same gateway.
