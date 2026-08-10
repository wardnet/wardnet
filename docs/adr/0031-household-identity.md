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
