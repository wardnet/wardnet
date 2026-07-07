# ADR: Push notifications — VAPID + Web Push delivery

**Status**: Accepted
**Date**: 2026-07-01
**Issue**: #440 (daemon VAPID + Web Push), under the #441 PWA-split umbrella

---

## Context

Both new PWAs (user #438, admin-mobile #439) expose push notifications. The
daemon must own a VAPID key pair, expose a subscription-management API reachable
by both admins and unauthenticated LAN devices, and deliver Web Push messages at
defined trigger points. Push is a **best-effort convenience channel**: in v1 a
notification received off-LAN is informational only (the user can't act until
they return to the LAN or connect via WireGuard).

Three cross-cutting choices had genuine alternatives and are hard to reverse
once real keys and subscriptions exist in the field.

## Decision

### 1. Delivery via `web-push-native` + the existing `reqwest`, not `web-push`

The mainstream `web-push` crate bundles its own **isahc** HTTP client and
requires **OpenSSL** to build — both clash with the daemon's rustls-only,
reqwest-based stack and would add a second TLS/HTTP toolchain to the Pi image.

We use **`web-push-native`** instead: HTTP-client-agnostic, pure RustCrypto
(`p256` / `aes-gcm` / `hkdf` / `sha2`), re-exporting `jwt-simple` for VAPID
ES256. It returns request headers + an encrypted body; we POST it with the
workspace `reqwest` client. A `WebPushSender` trait isolates all crypto + HTTP
so the audience/mapping logic is unit-testable with a recording mock and the
mock daemon no-ops delivery.

Trade-off: `web-push-native` was last released Dec 2023. The Web Push wire
format (RFC 8291/8188/8292) is stable, and we verified it builds cleanly against
the workspace's crate versions; the fallback, if it ever bit-rots, is composing
the same RustCrypto primitives directly.

### 2. Subscriptions in one table, keyed by an `(owner_kind, owner_key)` pair

A single `push_subscriptions` table serves both auth models:

- `owner_kind = 'device'` → `owner_key` = device **MAC** (the stable device key,
  per the glossary; survives DHCP-lease changes).
- `owner_kind = 'admin'` → `owner_key` = admin **account UUID** — **not** the
  ephemeral session token. Admin-PWA notifications fan out to every `'admin'`
  row. Keying to the account means a subscription survives session
  rotation/logout; the only things that remove it are an explicit unsubscribe or
  a 404/410 prune. (This sharpens the glossary's original "admin-session-keyed"
  wording to "admin-account-keyed".)

`endpoint` is UNIQUE, so a browser re-subscribing upserts its owner + keys
rather than duplicating — and can move between owners (anonymous device →
admin after login) on the same endpoint.

### 3. Best-effort delivery with 404/410 pruning

Each send is classified `Delivered | Gone | TransientFailure`. On **Gone**
(404/410) the stale subscription is pruned. Transient failures (network, 5xx)
are logged and dropped — never retried. A malformed stored subscription that
can't be built is treated as Gone so it is pruned rather than retried forever.

### VAPID key lifecycle

The key pair is generated **once, lazily, on first use** and **never rotated**
(rotation invalidates every existing subscription). The private key lives in the
`SecretStore` (`push/vapid/private_key`); the browser-facing public key is
cached (non-secret) in `system_config` (`push_vapid_public_key`) and served
unauthenticated from `GET /api/push/vapid-public-key`.

### Trigger sourcing

Event-driven: a thin `PushNotificationListener` forwards every `WardnetEvent` to
`PushService::handle_event` under an admin context; the service maps events to
audience + content. Two new events were added at their service-layer sources:
`DeviceAdminLocked` (`DeviceService::update_admin_locked`) and
`TunnelStartFailed` (`TunnelService::bring_up_core` error path). "Tunnel went
offline" fires on `TunnelReconnecting` and on `TunnelDown` **only when
`reason == "interface absent"`** — deliberate tear-downs are not notified.

## Consequences

- The security-sensitive surface is confined to `push/sender.rs` (VAPID signing,
  RFC 8291 encryption) plus the migration — the focus of the mandated security
  review.
- Only two new dependencies (`web-push-native`, `http`); no OpenSSL, no second
  HTTP client.
- Because keys are never rotated, a compromised VAPID private key would require a
  deliberate reset and full re-subscription — an accepted operational trade-off
  matching the Web Push spec's own guidance.
