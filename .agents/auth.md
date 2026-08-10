# Auth model

The authoritative design rationale is
[ADR-0031](../docs/adr/0031-household-identity.md). This file is the
working reference: what the principals are, and what you must do in
every service method.

## Principals

There are exactly three, and `AuthContext` (`wardnet-common/src/auth.rs`)
names all of them:

| Variant | Who | How it is established |
|---|---|---|
| `User { user_id, role }` | A **household user** — a person holding a credential | A session, from any credential kind (password, passkey, Google, GitHub), or an API key |
| `Device { mac }` | A **device** on the LAN | Source IP → `devices` row, no credential involved |
| `Anonymous` | Nobody identified | Everything else |

`UserRole` is `Admin` or `Member`. A `role = Admin` household user is
**exactly equal** to the legacy local admin — there is no deny-list and
no second tier. Background/system work uses
`User { user_id: Uuid::nil(), role: Admin }` so audit logs keep
distinguishing the system from a real person.

### Two rules about `AuthContext` that the compiler enforces

1. **Never a wildcard arm.** Every `match` over an `AuthContext` lists
   every variant explicitly. `_ =>` means the next principal someone adds
   lands in whichever branch happens to be last, silently. If you find a
   `_ =>` in a match over `AuthContext`, that is a defect — fix it, don't
   copy it.
2. **Never `unreachable!()` on an authz path.** `Anonymous` arms that
   "cannot happen" because a guard ran first still return
   `AppError::Forbidden`. An `unreachable!()` there converts an
   authorization bug into a remotely-triggerable panic.

`require_authenticated()` is written as a **positive** match —
`matches!(ctx, User { .. } | Device { .. })` — and not as
`!matches!(ctx, Anonymous)`, for the same reason. Several of its callers
then branch on `Device` and let everything else fall through to the
*admin* path, so a negative guard hands every new principal an admin-path
bypass with no compile error.

### Device affinity is not a credential

`devices.owner_user_id` says which household user a device belongs to. It
is attribution and grants nothing.

`AuthContext::User` wraps an `AuthenticatedUser` whose fields are
**private**, so a `user_id` you happen to be holding cannot be turned
into a principal with a struct literal. The constructor
`AuthenticatedUser::from_validated_session` has to be `pub` though —
credential verification lives in a different crate from the type, and
Rust cannot say "only this one function may call you". So the rule is
enforced by `build-support/check-auth-constructors.sh`, which fails CI if
that constructor appears outside the sanctioned files, plus a regression
test that a device owned by an `admin`-role user still resolves to
`Device`.

If you are about to add a file to that allow-list, the question to answer
in the PR is: *what credential did this code just verify?* If there isn't
one, the answer is no.

## Setup flow

- On first run, no user exists. `GET /api/setup/status` returns
  `setup_completed: false`. Web UI redirects to the setup page.
- `POST /api/setup` creates the first household user with `role = admin`
  and a `password` credential (Argon2id). Returns 409 if already
  completed.
- Subsequent users are added by an admin issuing a **one-time enrolment
  token**; the member redeems it and sets their own credential. The admin
  never learns a member's password.

## Unauthenticated endpoints

This list is **illustrative, not exhaustive** — it drifts. For the current
set, grep `security(())` in `wardnetd-api/src/api`.

- `GET /api/info` — version + uptime
- `GET /api/setup/status`, `POST /api/setup`
- `GET /api/health` — unauthenticated by design; the watchdog depends on it
- `GET /api/devices/me`, `PUT /api/devices/me/rule` — self-service,
  identifies the caller by source IP via `ConnectInfo<SocketAddr>`
- `wardnetd-api/src/api/auth.rs` — `POST /api/auth/login`, `logout`,
  `refresh`. These *establish* identity and so cannot require it.

The household-identity credential paths — `available_methods`, the OAuth
start/callback pair, the passkey ceremonies, and enrolment redemption — are
implemented on `UserService` and carry the same
category-(b) exception, documented per method. They have **no HTTP surface
yet**: the API layer for them is step 10 of #1147 and is not in the tree.
When it lands it will live in `wardnetd-api/src/api/user_auth.rs` and each
route needs `security(())`, because the document-level default would
otherwise mark them authenticated.

## Admin endpoints

Everything else. Requires a `User { role: Admin }` identity resolved from
either a session cookie (set by `POST /api/auth/login`) or an API key
(`Authorization: Bearer <key>`).

`resolve_auth_context` tries the **session before** the device-by-IP
lookup: a signed-in person sitting at a known device is a person, not a
device.

## Authentication context in services (HARD REQUIREMENT)

Every service method **must** validate the authentication context as its
first operation using `auth_context::require_admin()?;` or
`auth_context::require_authenticated()?;`. Services never trust their
caller — they always check. This is defense in depth: even if a handler
bug exposes a service method, the guard inside the service rejects the
call.

### Guard × principal truth table

This is the contract. It is also asserted as a test at
`wardnetd-services/src/tests/auth_context.rs` — **widening any cell means
editing that test**, which is exactly the point: the change shows up in
the diff where a reviewer will see it.

| `AuthContext` | `require_admin()` | `require_authenticated()` |
|---|---|---|
| `User { role: Admin }` | ✅ allow | ✅ allow |
| `User { role: Member }` | ❌ `Forbidden` | ✅ allow |
| `Device { .. }` | ❌ `Forbidden` | ✅ allow |
| `Anonymous` | ❌ `Forbidden` | ❌ `Forbidden` |
| no context set (background task without `with_context`) | ❌ `Forbidden` | ❌ `Forbidden` |

A method that needs something finer than these two — "this member may
edit their own profile but not someone else's" — does the ownership check
itself, **after** the guard, and never in place of it.

### HTTP request path (automatic)

The `AuthContextLayer` middleware resolves the caller identity (from
session cookie or API key) and sets a task-local `AuthContext` before the
request reaches handlers. Service methods read it via
`auth_context::require_admin()`.

### Background tasks calling services

Background processes (e.g. `IdleTunnelWatcher` tearing down idle tunnels,
DHCP lease expiry, backup cleanup) run outside the HTTP middleware, so no
`AuthContext` is set by default. They **must** wrap service calls in
`auth_context::with_context()`:

```rust
use wardnet_common::auth::AuthContext;

// Background task calling a service method:
let system_ctx = AuthContext::system();
auth_context::with_context(system_ctx, tunnel_service.tear_down(id, "idle timeout")).await?;
```

`AuthContext::system()` is `User { user_id: Uuid::nil(), role: Admin }`.
The nil UUID is what distinguishes background/system actions from a real
person's actions in audit logs, so use the constructor rather than
hand-rolling the variant.

### Tests

Same pattern:

```rust
use wardnet_common::auth::{AuthContext, AuthenticatedUser, UserRole};

let ctx = AuthContext::user(AuthenticatedUser::from_validated_session(
    Uuid::new_v4(),
    UserRole::Admin,
));
let result = auth_context::with_context(ctx, svc.get_config()).await;
```

Tests are exempt from `check-auth-constructors.sh` — building an
arbitrary principal is the whole point of a truth-table test. Use
`AuthContext::system()` when the test is standing in for a background
task rather than a person.

### Rules

1. Every service trait method implementation must call
   `auth_context::require_admin()?;` or
   `auth_context::require_authenticated()?;` as its first line.
2. There are exactly three sanctioned exception categories. Each one
   **must** carry a comment explaining why the guard is skipped; an
   undocumented exception is a violation, not a precedent:
   - **(a) Startup / restore** — methods that run before the system is
     ready (e.g. `restore_tunnels`, `sync_premium`).
   - **(b) Auth bootstrap** — methods that establish identity in the first
     place, and therefore cannot require it (`login`, `setup_admin`,
     `validate_session`, `validate_api_key`, `wizard_state`, the OAuth and
     passkey ceremony methods, and enrolment-token redemption).
   - **(c) Self-service by IP/device** — methods backing the
     unauthenticated endpoints above, which implement their own
     `AuthContext`-variant checks instead of the blunt `require_admin` /
     `require_authenticated` pair. These are *not* unguarded — they are
     guarded differently, and the check must still be the first thing the
     method does.

   Anything outside these three categories with no guard is a defect.
3. Background tasks wrap service calls in
   `auth_context::with_context(AuthContext::system(), ...)`.
4. Tests wrap service calls in `auth_context::with_context(ctx, ...)` to
   simulate the caller identity.
5. Anonymous callers get `Err(AppError::Forbidden)` — never silently
   succeed.

## Credentials

All four kinds are rows in `user_credentials` with the same shape; see
ADR-0031 §1 for why `UNIQUE(kind, subject)` and the partial
`UNIQUE(user_id) WHERE kind='password'` are security invariants rather
than tidiness.

- **Local password** — Argon2id. The floor: no network, no certificate,
  no provider. Never removable. Unknown identities still pay the
  `DECOY_PASSWORD_HASH` constant-work cost (`auth/password.rs`) so timing
  does not disclose whether a user exists.
- **Passkey** — `webauthn-rs`, RP ID **pinned** in `system_config` at
  first registration. Returns `412` unless a pinned RP ID exists and the
  request `Host` matches it, so passkeys do not work over `:7411` or a
  bare LAN IP. Divergence from the live canonical FQDN fails loudly and
  requires an explicit admin "reset passkeys".
- **Google / GitHub** — each household registers its own OAuth app;
  verification is against the provider's **userinfo** endpoint (no JOSE
  stack). Optional and hidden until configured — `GET /api/auth/methods`
  is the contract a sign-in surface uses to decide which buttons to
  render. An unknown subject is **401, never auto-create**.

OAuth `state`/PKCE verifiers and WebAuthn challenges are **in-memory with
a 5-minute TTL**, never persisted: a persisted single-use nonce that fails
to delete is a replay primitive.

Login is rate-limited **per identity and per IP**. Per-IP alone falls to a
botnet; per-identity alone lets an attacker lock a household out.
