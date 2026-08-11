# Licensing

Wardnet is split across two licenses. Which one applies depends on which
part of the tree you're looking at.

## The daemon — GPL-3.0-or-later

Everything under [`source/daemon/`](source/daemon/) — the `wardnetd`
binary and every crate in that Cargo workspace — is licensed
**GPL-3.0-or-later**. See [`LICENSE`](LICENSE) for the full text.

This is not a preference, it's an obligation. The daemon links
[`rustables`](https://crates.io/crates/rustables) (pure-Rust nftables over
netlink — see the firewall subsystem), which is GPL-3.0-or-later and has
never been published under any other license. Rust links statically, so
the compiled `wardnetd` binary is a combined work that GPL-3.0's terms
extend to. Distributing it under a permissive license would not be valid.

Practically, for anyone running Wardnet, this changes nothing: you may
use, modify, self-host, and redistribute it freely. It matters if you want
to ship a **modified** daemon binary — GPL-3.0 requires you release the
corresponding source under GPL-compatible terms, and (per GPLv3's
anti-tivoization terms) that users of a device you ship can install their
own modified builds on it.

One other non-permissive dependency is in the tree, and it does not change the
above: [`wireguard-control`](https://crates.io/crates/wireguard-control) is
LGPL-2.1-or-later, whose linking exception means it does not, on its own,
impose GPL terms.

## The SDK — MIT

[`source/sdk/wardnet-js/`](source/sdk/wardnet-js/) — the `@wardnet/js`
client library published to npm — stays **MIT**. See
[`source/sdk/wardnet-js/LICENSE`](source/sdk/wardnet-js/LICENSE).

The SDK is a client library with no Rust dependency and no `rustables` in
its dependency graph, so no copyleft obligation reaches it. Keeping it MIT
means you can import `@wardnet/js` into your own application without that
application inheriting GPL obligations.

## Everything else — MIT

The web UI, the admin and user PWAs, and the marketing site
(`source/web/`, `source/admin-site/`, `source/admin-app/`,
`source/user-app/`, `source/marketing-site/`) remain **MIT**, as does the
rest of the repository outside `source/daemon/` — including the Go SDK
(`source/sdk/wardnet-go/`) and the `wctl` CLI (`source/wctl/`).

`wctl` is MIT because it is a Go binary that speaks HTTP to the daemon. It
does not link `rustables` or any other GPL code, so no copyleft obligation
reaches it. (It was GPL while it was a Rust crate inside the daemon's Cargo
workspace; the Go rewrite moved it out of that workspace and out of that
obligation.)

MIT is GPL-compatible, so the web UI assets embedded into the `wardnetd`
binary at build time combine cleanly into the GPL-3.0 whole. The MIT
sources themselves stay MIT and can be reused independently under MIT
terms; only the *combined binary* carries GPL-3.0's terms.

## Summary

| Path | License |
| --- | --- |
| `source/daemon/**` | GPL-3.0-or-later |
| `source/sdk/wardnet-js/**` | MIT |
| `source/sdk/wardnet-go/**`, `source/wctl/**` | MIT |
| everything else | MIT |
| the distributed `wardnetd` binary | GPL-3.0-or-later (combined work) |
