# Blocking ads is easy. Knowing what you blocked is the fun part.

I said this in passing when I first wrote about Wardnet: the part I
didn't expect to enjoy was the stats. Turning on a blocklist is a
one-time thing you forget about by dinner. Watching what it catches is
the thing you keep coming back to. It's why people who run Pi-hole and
NextDNS leave the dashboard open on a spare monitor. The blocking is the
job; the numbers are the reason it's satisfying.

So this release is about the numbers.

## Who, not just what

The DNS page already showed you the busy stuff, queries over time, top
blocked domains, the chattiest clients. That answers *what* got blocked.
It doesn't answer *who's behind it*, and "who" is usually the more
interesting question.

![DNS stats, deeper: top trackers, week-over-week, per-device](/docs/dns-ad-blocking/dns-stats.png "wide")

**Top trackers blocked** takes the domains that got turned away and
groups them by the company that runs them, matched against a curated
tracker list. Instead of scrolling a wall of hostnames that all sort of
look the same, you get a short list of names you recognise, and a count
next to each. It reframes the whole thing: not "something called
`pagead2.googlesyndication.com` got blocked 4,000 times" but "this many
requests went to these companies, and none of them made it out."

## Is this normal for a Tuesday?

A single number is hard to read. Forty thousand queries, is that a lot?
Compared to what?

Compared to last week, it turns out. Every window now sits next to the
one before it, so a **week-over-week** jump in queries or blocks is
obvious the moment you glance at it. A quiet baseline that suddenly
doubles is exactly the kind of thing worth noticing, a new device, an
app gone chatty, something phoning home more than it used to.

## One device at a time

The other thing I kept wanting was to follow a single device. Not the
whole network, just the one I'm suspicious of. **Per-device query
charts** do that: pick a device from the dropdown and watch its query
volume over time. The interesting reads are the boring-sounding ones,
a TV that keeps talking at 3am, a "smart" plug that never stops. It
keys on the device itself, not its IP, so the line doesn't break in half
just because DHCP handed it a new address overnight.

## Where to find it

It's all on the **DNS** page, under the same range toggle as the rest of
the stats, `1h` through `12mo`. Nothing to enable, nothing to configure,
it draws on the query log Wardnet already keeps. If you're on the latest
release, it's already there waiting for you.

As always: it's a young project, and I'd rather hear what's missing from
these than not hear from you.

[github.com/wardnet/wardnet](https://github.com/wardnet/wardnet)
