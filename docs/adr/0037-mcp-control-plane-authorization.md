---
status: accepted
date: 2026-09-06
issue: "#1203 (Agent-ops 2 — the MCP control plane is its own OAuth 2.1 authorization server)"
---

# ADR: The MCP control plane is its own OAuth 2.1 authorization server

> **Numbering.** The #1201 epic calls this ADR-0034 and its predecessor ADR-0033.
> Both numbers were taken before the epic landed, so this is **0037** and the
> recovery-plane ADR is
> [0036](0036-recovery-plane-is-a-separate-process.md). See that ADR's note.

## Context

[ADR-0036](0036-recovery-plane-is-a-separate-process.md) established the
transport: `wardnet-tunneller` keeps one WebSocket up independently of
`wardnetd`, and `dest_port=443` demuxes by SNI. `wardnet-mcp` is what rides it —
the typed, enumerated tool surface that replaces SSH-ing into the Pi to find out
why the LAN is broken.

Two facts make its authorization a decision rather than a detail.

**It is reachable from the open internet by anyone who knows the slug.** No cloud
work is needed to get there: wardnet-cloud ADR-0018 publishes a wildcard
`*.<slug>.my.wardnet.services`, and the edge's `extract_slug` routes on the slug
alone, deliberately ignoring leading labels — so `mcp.<slug>…:443` already
arrives at the box, and the **per-user wildcard certificate**'s
`*.<vanity>.my.wardnet.services` SAN already covers it. Convenient, and it means
the front door is on the internet from day one.

**It must work when nothing else does.** The whole epic exists because the
moment you need the tooling is the moment the LAN is down and `wardnetd` may be
the thing that is broken. An authorization design that depends on the daemon, or
on the LAN, fails exactly when it is needed.

This ADR records how `wardnet-mcp` authenticates, and — more importantly — the
two plausible alternatives it rejects, both of which fail on one of those two
facts.

## Decision

### 1. Standard MCP auth, because standard auth means standard clients

`wardnet-mcp` implements the MCP authorization spec as written, with no local
dialect:

- an OAuth 2.0 **Resource Server**, publishing RFC 9728 **Protected Resource
  Metadata** at `/.well-known/oauth-protected-resource`;
- `401` with a `WWW-Authenticate` header naming the resource metadata, so an
  unauthenticated client discovers where to authenticate rather than guessing;
- **authorization-server discovery** via RFC 8414;
- **PKCE** on every authorization-code exchange;
- **Dynamic Client Registration** (RFC 7591), so a client the box has never seen
  can register itself;
- **Resource Indicators** (RFC 8707), so a token minted for this box is bound to
  this box and is not replayable against another resource.

The argument is not protocol elegance. It is that every MCP client — Claude
Code, the desktop and mobile apps, anything else that speaks MCP — already
implements this ceremony. A bespoke scheme, however simple, means writing and
maintaining a shim per agent host, and the whole point of the control plane is
that an operator can point *whatever they already have* at their box during an
outage. A credential that only works with software we also ship is a credential
that fails when you are on someone else's laptop.

### 2. `wardnet-mcp` is its own Authorization Server — not just a Resource Server

The spec lets an MCP server delegate to an external AS. Both candidates fail.

**The daemon as AS is circular.** `wardnetd` already holds the household
identity directory ([ADR-0031](0031-household-identity.md)) — users,
credentials, sessions, and soon an OIDC issuer for published apps. Pointing
`wardnet-mcp` at it is the obvious reuse, and it is wrong for one reason:
**daemon down ⇒ no token ⇒ cannot diagnose the daemon.** It is not merely a
degraded path; it fails in *precisely and only* the scenario the epic exists
for, which is the same shape as the flaw
[ADR-0036](0036-recovery-plane-is-a-separate-process.md) removed by taking the
tunnel out of the daemon. Having just extracted the transport so a daemon
restart cannot sever the channel, re-attaching the channel's *authorization* to
the daemon would give the single point of failure back under another name.

**wardnet-cloud as AS would work, and is refused anyway.** It genuinely survives
a daemon outage, it is already the enrollment authority, and cloud ADR-0009's
federated login is sitting there. It is rejected because it violates
[ADR-0031](0031-household-identity.md) decision 1's invariant, stated there as a
property worth more than a recovery flow: **there is no path by which
wardnet-cloud can grant anyone access to a home network.** Making the cloud the
AS for the control plane would not merely create such a path — it would create
the *most powerful* one, because the control plane is by design the surface that
can change the box. A compromise of our identity plane, or a legal order served
on us, would then reach into every customer's LAN with root-adjacent verbs. For
a product whose pitch is that we cannot see your traffic, that is not a trade to
make for the convenience of not writing an AS.

So `wardnet-mcp` issues its own tokens, against its own local user records, on
the box. The trust arrow keeps pointing one way.

### 3. Being an *issuer* avoids the surface ADR-0031 declined to take on

A reader who knows [ADR-0031](0031-household-identity.md) §6 will object: that
ADR refused to verify federated `id_token`s specifically to avoid a JOSE stack —
JWKS fetching, key rotation, algorithm confusion — which the Rust daemon
deliberately does not have. How does the same project now run an OAuth server?

Two things changed, and both are real rather than a change of heart:

- **`wardnet-mcp` is Go, not Rust.** `wctl` is already Go + cobra
  (`source/wctl/go.mod`), the Go MCP SDK reuses the extracted command handlers
  directly, and the binary rides the existing `build-go.yml` cross-compile
  matrix. Mature, maintained JOSE and OAuth-server libraries are ordinary
  dependencies there.
- **An issuer verifies its own signatures, not a stranger's.** ADR-0031's
  hazards are all consequences of trusting *someone else's* keys over a
  discovery document: fetching JWKS from a third party, following its rotations,
  and accepting whatever `alg` it claims. An AS that mints tokens against a
  locally-generated key it also verifies has none of that. The token format is
  an internal contract, so it may be opaque and validated by lookup rather than
  by signature at all.

### 4. Break-glass is a refresh token handed out *in advance*

The lesson of [ADR-0031](0031-household-identity.md)'s **Local admin** is that a
break-glass credential must be obtainable *before* the emergency: an escape
hatch you can only fetch by reaching the thing that is broken is not an escape
hatch. Local admin works with no internet, no provider, and no user table
precisely because it was created at setup.

The control plane's version is a **long-lived refresh token, pre-provisioned to
the operator's phone and Mac** during setup or a later explicit act. It is
minted while everything is healthy and stored on the devices that will be used
during an outage. The interactive DCR-plus-PKCE flow of decision 1 is the
*normal* path — new clients, new devices, day-to-day use; the pre-provisioned
token is the path that still works when the operator is on a hotspot at
midnight with a box that is refusing connections.

The corollary is that these tokens are inventory: they are enumerable and
individually revocable through the control plane itself, because a credential
issued once and never listed again is one nobody can retire.

### 5. The hostname is not a secret, so OAuth carries the whole load

[ADR-0029](0029-private-dns-dot.md) built Private DNS on a hostname that *is* a
credential — `<token>.<fqdn>`, 80 bits of entropy, never in a CT log because it
rides the wildcard SAN. It is tempting to reach for the same trick here. **It
does not apply.** `mcp.<slug>.my.wardnet.services` has a fixed leading label and
a slug that is public: the apex `<slug>.my.wardnet.services` appears in
Certificate Transparency logs by construction (ADR-0029 §1 turns on exactly this
fact), so anyone enumerating CT can derive the control-plane hostname of every
Wardnet box in existence. Obscurity contributes nothing and must not be counted
on.

**OAuth is therefore the only barrier**, and the obligations that follow are part
of this decision rather than implementation detail:

- **Rate limiting is per-identity *and* per source IP**, the pair
  [ADR-0031](0031-household-identity.md) §10 already argues for: per-identity
  alone loses to a botnet where every request has a fresh source, per-IP alone
  loses to one host walking the directory a couple of guesses at a time. This
  surface needs it more than the LAN login does, because its front door is on
  the open internet.
- **Lockout state stays in memory and is lost on restart**, for §10's reason,
  which is sharper here: a persisted lockout an attacker can induce against the
  operator's only credential is a denial-of-service primitive aimed at the
  break-glass path — during an outage, which is when it is being used.
- **Dynamic Client Registration is constrained.** RFC 7591 registration is
  unauthenticated by default, which on a Raspberry Pi is an unauthenticated
  write endpoint on the public internet. Registration must be rate-limited,
  capped, and expiring for clients that never complete an authorization; a
  registered client that has never been authorized grants nothing, but it must
  not be able to accumulate.
- **Failures are uniform.** No response distinguishes "no such user" from "wrong
  password", and none confirms that a given slug is a live box.

### 6. The network-exposed surface never runs as root; the root surface is never network-exposed

Least privilege is the *point* of replacing SSH, not a side effect. SSH grants
an arbitrary shell to whoever holds the key; a fixed tool surface grants exactly
its enumerated operations, and the difference is the entire security argument for
doing this at all. It would be self-defeating to then run that surface as root.

So the split is structural:

- **`wardnet-mcp`** runs as its own unprivileged user, in the `systemd-journal`
  group so it can read the journal, and holds the network listener.
- **`wardnet-mcp-helper`** is a root-owned libexec binary with a fixed verb list
  and no shell. It takes the shape already proven by
  `wardnet-postupgrade-runner`: installed `root:root` under
  `/usr/local/libexec/wardnet/`, **outside** `wardnetd.service`'s
  `ReadWritePaths`, so the unprivileged users cannot replace the privileged
  binary they invoke.

The invariant is worth stating as a sentence someone can check a diff against:
**the network-exposed surface never runs as root, and the root surface is never
network-exposed.** A verb is added to the helper's list by name, or it does not
exist.

### 7. A mutation disarms on a positive re-probe, and rolls back through the database

A network-affecting change applied over the control plane can cut the channel it
arrived on. Every such mutation is therefore **armed**: it reverts unless
confirmed, on a dead-man's switch. Two rules make that actually work.

**Disarm requires an independent positive re-probe of the changed path — never
channel liveness.** The recovery channel is deliberately independent of the
box's own routing ([ADR-0036](0036-recovery-plane-is-a-separate-process.md) §4),
which is exactly what makes it a useless witness here: it will report itself
healthy across a change that severed every LAN device from the gateway. "The
tunnel is still up" answers a question nobody asked. What must be re-probed is
the thing that was changed, from a vantage point that would notice it breaking.

**Rollback happens at the database layer, not the kernel's.** nftables rules and
`ip rule` entries are reconciled *from* the database, so restoring kernel state
directly would be undone by the next reconcile — the same trap
[ADR-0028](0028-shutdown-teardown-and-uninstall.md) §2 documents for tunnel
teardown, where deleting an interface behind the database's back leaves a record
saying `Up` that nothing recovers from. The revert therefore writes the previous
*intent* and lets reconciliation re-derive kernel state, which is the only form
of rollback that survives the next tick. State the daemon does not own (OS-level
addressing, reached through the helper) is out of that scope and needs its own
revert, which is why #1227's `network static-ip` is a separate issue rather than
another verb.

## Consequences

- **The box ends up running two authorization systems, deliberately.** The
  household IdP ([ADR-0031](0031-household-identity.md)) authenticates people to
  published apps and the admin surfaces; `wardnet-mcp`'s AS authenticates
  operators to the control plane. They share no state and no lifecycle, and
  merging them is the rejected daemon-as-AS design under a friendlier name. The
  cost is a second credential to manage; the benefit is that one being down, or
  compromised, is not the other.
- **The `mcp.` label needs no cloud change and no new certificate.** It rides
  the existing wildcard SAN and the existing `extract_slug` behaviour. Nothing
  in wardnet-cloud is touched by this ADR.
- **`wardnet-mcp` needs a systemd unit, and units do not ship via the
  auto-updater.** Like `wardnet-tunneller`: a `deploy/*.service` file, an entry
  in `install.sh`'s `UNITS` array, a new unprivileged user, and a **new**
  append-only migration id in `wardnet-postupgrade`.
- **An unauthenticated public write endpoint now exists on every box** — the DCR
  endpoint — with the constraints of decision 5 as the mitigation. This is the
  single largest new attack surface in the epic and should be reviewed as such.
- **Go gains a real dependency footprint** (MCP SDK, JOSE/OAuth libraries) in a
  module that until now was a thin cobra CLI over the generated SDK. Per
  `.agents/workflow.md` those additions are an *ask first*, not a default.
- **The skill's MCP-driven path (#1218) can now be written**, because the
  credential story it has to describe to an operator — pre-provision this token
  now, before you need it — is settled here rather than left to the
  implementation.
- **Reversibility is asymmetric, which is why the refusal is recorded and not
  merely implied.** Tightening later is easy; adding a cloud-vouches-for-box
  path later would be mechanically trivial and irreversible in trust terms.
  [ADR-0031](0031-household-identity.md) made that point about logins. It is
  more true of a surface that can reconfigure the network.
