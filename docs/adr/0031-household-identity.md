---
status: accepted
date: 2026-08-10
issue: "#1146 (epic — Application hosting); implemented across #1147–#1149"
---

# ADR: Household identity is box-local; the cloud may hint but never vouch, and device affinity never authenticates

*Companion to [0030-published-apps.md](0030-published-apps.md).*

---

## Context

Until now the box has had exactly two principals: an **admin** (username + Argon2id password, created once by the setup wizard, carried by a session cookie or API key) and a **device** (identified by source IP via `ConnectInfo<SocketAddr>`, used by `GET /api/devices/me` and the user PWA). There are no people in the data model — only an operator and a set of MAC addresses.

Publishing apps breaks that. "Who may reach Immich" is a question about *people*: my partner yes, the kids no, the houseguest no. So is "sign in to Vaultwarden with the same identity you use for Wardnet", which requires Wardnet to be an identity provider (#1149) — and an IdP with no user directory is a contradiction.

Two tempting shortcuts had to be examined and rejected, because both are one-way doors.

**Reusing wardnet-cloud identities.** The cloud already has human authentication: password + Google + GitHub, sessions, minted JWTs (cloud ADR-0009). But its model is `USER == tenant`, strictly 1:1, with multi-user explicitly deferred. A cloud tenant is the *subscriber* — the person who pays. Household members are a different population.

**Letting device affinity stand in for a login.** Wardnet knows which device a request came from. It is very tempting to say "this device belongs to Pedro, Pedro is an admin, let him in without a password."

## Decision

### 1. The household user directory lives on the box; the cloud is a hint, never a credential

The Pi owns a `users` table and its own credential rows — local password, **Google**, **GitHub**, and **passkey/WebAuthn** — with federated logins verified **by the box** against the provider directly. The cloud tenant stays exactly what it is: the owner/billing identity. At setup the owner's email may be **pre-filled** from the enrolled tenant as a convenience; the box still verifies the credential itself and stores its own row.

Three reasons, in ascending order of weight:

- **No cloud un-deferring.** Cloud ADR-0009's 1:1 stays intact; no cross-repo identity migration.
- **A household member is not a customer.** Reusing cloud identity would require a nine-year-old to create a Wardnet cloud account to watch Jellyfin.
- **The trust topology, which is the real argument.** Today the arrow points only one way: the daemon authenticates *to* the cloud with an Ed25519 PoP (ADR-0016), and the certificate private key never leaves the Pi (ADR-0008). **There is no path by which wardnet-cloud can grant anyone access to a home network.** Making the cloud able to vouch for a box login would create one — and with it, a compromise of the cloud identity plane, or a legal order served on us, becomes a way into people's homes. For a product whose pitch is that we cannot see your traffic, that property is load-bearing.

The cost is accepted knowingly: **account recovery is local only** — another admin, an API key, or physical access to the Pi. Nobody at Wardnet can let themselves in, including us. That sentence is worth more than a recovery flow.

**Passkeys are first-class**, not a later nicety. WebAuthn requires a stable secure origin with a publicly-trusted certificate, and the **canonical FQDN** already is one — so the RP ID is `<vanity>.my.wardnet.services`, covering published-app subdomains. Most home networks cannot offer passkeys at all.

### 2. Device affinity is attribution; authentication comes from a device-held session

Two facts that "this device is Pedro's" was being asked to carry are split apart:

- **Device affinity** (`devices.owner_user_id`) — an *attribution* fact, set by an admin. It decides whose filter profile applies, which published apps are listed, and how the query log attributes traffic. **It is never a credential.**
- **A device-held session** — an *authentication* fact: someone proved they were Pedro on that device (Google, passkey, or local password) and the box issued a persistent, revocable session scoped to it.

The desired UX is unchanged by this split — open admin-app on the phone you already signed in on and you are in, with no prompt — but what made it safe is the earlier sign-in, not the device knowing a name.

The alternative was rejected because device identity is **source-IP-derived**: spoofable by anything on the L2, reassigned by DHCP, inherited by whoever picks up an unlocked phone. Treating it as an authentication factor would collapse network admin to IP spoofing, and every guarantee in the Network Zone ladder (ADR-0018/0019/0021) sits downstream of admin being hard to obtain. Affinity remains perfectly adequate for the low-stakes decisions it drives, including the **Claimed devices** access policy — a *reach* decision, never an *admin* one.

### 3. `Admin` is a role on a user; the setup-wizard credential survives as break-glass

Two credentials, one authority — not a new tier above admin:

- **Local admin** — the setup-wizard username and password. The break-glass path: works with no internet, no federated provider, and no user table. Never removed, never the primary route.
- **Admin role** — held by a household user; signing in as them yields the same `AuthContext::Admin`.

`Admin` is the **only** role in v1. No groups: a household is 2–6 people, and a per-app allow-list is honest at that size. Pangolin ships full RBAC because it sells to teams; we do not.

The term **"master account" is deliberately not used** — it is vague about what it is master of, and in a product whose flagship catalog app is Vaultwarden it will be read as "master password".

## Consequences

- **`AuthContext` grows a `User` principal**, and `.agents/auth.md`'s HARD REQUIREMENT means every service method's guard must be audited against the new principal rather than defaulting through. This is a cross-cutting change, not an additive one — hence its own child epic (#1147) and a follow-on for the three surfaces (#1148).
- **Wardnet's own login is reworked before the IdP ships** (#1148 before #1149): an identity provider must issue tokens against a settled model.
- **The user PWA stays device-keyed with no login.** Invariant: affinity alone must never unlock anything a stranger on the LAN should not have; a surface that becomes sensitive asks for a real sign-in instead.
- **Being an IdP is worth it for a reason unrelated to protocol support.** Authelia and Pocket ID implement OIDC fine; what they cannot obtain on a home network is a stable public FQDN with a trusted certificate and knowledge of which device is asking. Wardnet has both already.
- **A forward-auth gate and a native mobile app remain mutually exclusive** — the Bitwarden client has no browser to complete an interactive login. App-native OIDC is *more* mobile-compatible than a gate, because the app opens its own SSO webview. This is why the IdP exists and why an ambient gate does not.
- **Reversibility.** Adding groups or roles later is additive. Adding a cloud-vouches-for-box path later would be easy mechanically and irreversible in trust terms — which is precisely why the refusal is recorded here rather than left as an unstated default.

---

## Implementation decisions (#1147)

The sections above settle the *model*. Building it settled a further set of
decisions that are not derivable from the model and are easy to get wrong, so
they are recorded here rather than left in commit messages.

### 4. One credential table, and `(kind, subject)` uniqueness is a security invariant

All four credential kinds live in one `user_credentials` table — `kind` ∈
`password | google | github | passkey`, plus a `subject` and an optional
`secret`. `subject` is the login identifier: the username for a backfilled local
admin, the email for a new local user, Google's `sub`, GitHub's **numeric id**
(never the login — renameable and reusable), or the base64url passkey credential
id.

`UNIQUE(kind, subject)` is not tidiness, it is the **anti-hijack invariant**: one
provider account links to at most one household user. Without it, two people
could link the same Google account and each become the other. The refusal
deliberately does not disclose *which* user holds an existing link, because that
would turn an authenticated link attempt into a directory-enumeration oracle.

`UNIQUE(user_id) WHERE kind='password'` makes "one password per user" a database
fact rather than a convention. `secret` — the Argon2id PHC string or the passkey
COSE public key — must never leave the repository layer, which is enforced by
type: listing methods return a `CredentialSummary` that structurally has no
`secret` field.

### 5. `AuthContext` collapses to `User { user_id, role }`, not a separate `Admin`

Decision 3 above says signing in as an `admin`-role user "yields the same
`AuthContext::Admin`". In implementation that is one variant too many: keeping an
`Admin` variant alongside a `User` variant means two ways to say the same thing
and a conversion between them. The enum is
`User { user_id, role } | Device | Anonymous`, and `require_admin()` is a single
honest predicate — `role == Admin` — at every call site.

The escalation this closes was live before the change: `resolve_auth_context`
promoted **any** valid session to admin, which was true by construction when a
session could only belong to the single admin and is a privilege escalation the
moment a second role exists. `validate_session` therefore returns a typed
`AuthenticatedUser` carrying the role read live from `users`, so a demotion takes
effect on the next request rather than the next login.

Three mechanisms keep it closed, because documentation is not enforcement:
`AuthenticatedUser`'s fields are private so a context cannot be built from a
`user_id` somebody had lying around; `build-support/check-auth-constructors.sh`
fails CI if the one constructor is called outside the code that has just verified
a credential; and a guard × principal truth table
(`wardnetd-services/src/tests/auth_context.rs`) asserts every cell, with a
companion test that the table is *complete* so adding a principal forces an edit
there.

### 6. Federated login verifies against **userinfo**, and each household registers its own app

The authorization code is exchanged for an access token and the access token is
spent on the provider's userinfo endpoint. Verifying an `id_token` instead would
mean a JOSE stack — JWKS fetching, key rotation, algorithm confusion — which this
repository deliberately does not have (push VAPID is hand-signed with `p256`).
Userinfo over TLS gets the same answer from the same authority with none of that
surface, and needs no new dependency.

Wardnet ships **no client credentials** and hosts **no callback**. The admin
registers an app with the provider and Wardnet shows the exact redirect URI to
paste. A Wardnet-hosted callback would put a third party on the critical path of
logging into your own house and route every household's sign-ins through
infrastructure we run — the same one-way-trust argument as decision 1.

An unknown provider subject is **refused**: Wardnet never auto-creates a
household user from a federated login, or anyone with a Google account could
create an account on somebody else's home network. An admin links first.

`state` and the PKCE verifier live in memory with a five-minute TTL and are never
persisted. A single-use nonce in a table is only single-use if the delete
succeeds; a row that fails to delete is a replay primitive. PKCE is used even
though the client secret is server-side, because it binds the code to *this*
ceremony. A link ceremony records **who started it** and refuses a mismatch —
otherwise an attacker could consent with their own account and have a signed-in
admin's browser redeem the result.

### 7. Passkeys are deferred, and the reason is a dependency, not a design change

The model above keeps passkeys first-class and that stands — `user_credentials`
already accepts `kind = 'passkey'`. The implementation is **not in the first
delivery**, because `webauthn-rs` 0.5 depends on OpenSSL unconditionally
(`webauthn-rs-core` declares `openssl` and `openssl-sys` as plain dependencies,
not behind a feature), and the daemon cross-compiles to
`aarch64-unknown-linux-gnu` with no target libssl. An earlier draft of this ADR
asserted the crate was pure Rust and coexisted with the existing
rustls/aws-lc-rs provider; **that was wrong**, and it is recorded here rather
than quietly deleted because it is the kind of premise worth checking before it
is built on. Tracked in #1194.

Two design consequences hold whenever it does land, and they are worth keeping
written down now:

The RP ID must be **pinned** at first registration and never silently changed,
because WebAuthn binds a credential to a domain: re-pinning breaks every
existing passkey in the household with no explanation. Divergence from the live
canonical FQDN has to fail loudly, with an explicit admin "reset passkeys" as
the recovery.

And passkeys **cannot** work on the plain-HTTP `:7411` surface or a bare LAN IP,
because WebAuthn requires a secure context and a real domain. That is not a gap
to close later — it is why the local password can never be removed. It is the
only credential that works on a box with no certificate and no public hostname,
which makes the WAN-down guarantee true by construction rather than by policy.

### 8. Enrolment sets a *first* credential and never replaces one

An admin issues a one-time, hashed, expiring token; the member redeems it and
sets their own password, so the admin never learns it. That property only holds
if redemption **refuses an account that already has a password** — otherwise an
admin could issue a second token against a member, redeem it themselves, and sign
in as them, which is exactly what decision 3's "one authority" is not supposed to
mean. The token is claimed only after every other check passes, so a refusal
leaves the invitation spendable rather than burning it.

Because the email *is* the password login identifier, changing a user's email
moves the credential's `subject` with it. Letting them diverge leaves a profile
showing an address that cannot sign in, and frees an address that becomes a
uniqueness landmine for the next user given it.

### 9. The `admins` → `users` backfill, and the one case it cannot satisfy

`admins` rows are backfilled into `users` **preserving their ids**, which turns
the `sessions` rebuild into a column rename and lets live sessions survive the
upgrade. SQLite cannot alter a foreign key in place, so the table is rebuilt; a
mistake here leaves a daemon that will not start and cannot roll back.

Subjects are lowercased unconditionally, because the login path lowercases what
the operator types and matches the column exactly — a subject preserved in its
original casing would be unreachable, locking that admin out with no recovery.
Two usernames differing only in case are therefore **one login** under the new
scheme. That is a genuine data conflict, so the oldest admin in a colliding group
keeps the credential and the others arrive credential-less, keeping their id and
their `admin` role, to be re-enrolled. Aborting instead would be unbootable;
inventing an unreachable row would be worse than either.

`push_subscriptions.owner_kind` moves `admin` → `user` in the same migration.
Because the ids were preserved, that is a lossless rename of the discriminator —
and leaving the old value would make every upgraded box's admin subscriptions
present but unreachable, since the live `AuthContext` no longer has an `Admin`
variant.

### 10. Login rate limiting is in memory, per identity **and** per source IP

Both counters are required: per-identity stops one known account being ground
from a botnet where every request has a different source, and per-IP stops one
host walking the directory with a couple of guesses per account so no identity
counter trips. State is process-local and lost on restart, deliberately —
persisting it would turn a lockout into something an attacker can induce and
leave behind against the household's only admin, which is a denial-of-service
primitive aimed at the break-glass credential.

### 11. One callback URL, dispatched on the ceremony — never on the request

Decision 6 fixes the redirect URI at
`https://<fqdn>/api/auth/oauth/<provider>/callback`, and that string is
registered by hand with Google or GitHub by every household. It is therefore the
single least reversible thing in this design: changing its shape invalidates
every existing registration, silently, at the moment somebody tries to sign in.

One URL has to serve both ceremonies the model allows — a **sign-in** and a
**link** — and the request arriving at it is a bare provider redirect carrying
only `state` and `code`. Nothing about that request says which ceremony it
belongs to. The dispatch therefore reads `started_by` from the stored ceremony:
`None` is a sign-in, `Some(user)` is a link, and a link additionally requires
that the caller *is* that user.

Deciding in the HTTP handler instead — "treat it as a link if the request has a
session" — was rejected for a reason worth recording, because it is the obvious
shortcut. `resolve_callback` consumes the `state` **before** it can discover it
guessed wrong (a single-use nonce that survived a failed read would be a replay
primitive, §6). A wrong guess is thus unrecoverable for the user: their
invitation to sign in is spent and the only remedy is to start again. So the
service exposes exactly one callback entry point returning a typed
`OauthOutcome`, and the two former entry points are gone rather than left
callable. Keeping a second door that skips the `started_by` check would be the
same mistake §5 rejected when it refused a separate `Admin` variant: two ways to
say one thing, one of which is wrong.

Two pieces of client intent cannot survive a provider redirect on their own,
because the callback has no request body: **where to return the browser** and
**whether the session is long-lived**. Both are parked on the ceremony at
`/start` — which is what OAuth's `state` is *for*, beyond CSRF — rather than
being re-asserted afterwards. `remember_me` in particular cannot be re-asserted:
it gates `refresh_session`, so an endpoint that raised it after the fact would
be an endpoint that upgrades any short session into a 90-day one.

The return target is an **enum**, not a path. A caller-supplied relative path is
the classic open-redirect trap (`//evil.com` and `/\evil.com` are read as
absolute by browsers), and validating it correctly is famously easy to get
wrong. With an enum, `Location` is always one of two compile-time constants and
the vulnerability cannot be written.

The alternative of dropping `remember_me` for federated users in favour of
**silent re-authentication** (`prompt=none`) — the idiomatic answer in most
federated apps, where the provider holds the durable session — is rejected for
the same reason the local password is unremovable (§7): it makes staying signed
in depend on a reachable WAN and a reachable provider. It would make federated
admins the only people logged out during an outage, on a box whose guarantee is
that it keeps working when the internet does not.
