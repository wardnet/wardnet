# wardnet-bridge agent guide

Conventions and invariants for agents working inside `source/bridge/`.

> **Status:** invariants tagged `[#444]`/`[#445]` describe the agreed target architecture (SNI/tunnel
> data plane, PostgreSQL/Neon, runtime `SecretsProvider`, multi-node `TunnelRouter`) and land with those
> issues. Everything untagged is live on `main` today.

## Must-know invariants (never violate these)

1. **Bearer token never stored raw.** `register.rs` returns `hex(random_32_bytes)` to the caller once and stores only `hex(SHA-256(token))`. Never persist, log, or echo the raw token.

2. **DB token lookup is path-gated.** `auth_layer` only queries the DB when the request path starts with `/v1/installs/`. Adding a new public endpoint under that prefix would silently require auth — use a different path prefix.

3. **Uniqueness before challenge burn.** In `register.rs`, the `find_by_name` check always runs _before_ `challenges().consume()`. Reversing the order would consume the user's PoW proof on a name-conflict error.

4. **ReplayCache keyed on `{install_id}:{timestamp}:{body_hash}`.** Do not change this format without updating the replay window constant and tests. The window is ±120 s (double the timestamp window) for clock-skew at the cache boundary.

5. **Body buffered before auth.** The 1 MiB body guard runs for _every_ request, including unauthenticated ones. It is the first thing `auth_layer` does — before any DB call.

6. **`pub_key_bytes` decoded once.** The install row decodes the base64 public key into `[u8; 32]` when loaded from the DB. Auth uses `install.pub_key_bytes` directly — never re-decode the base64 string on a hot path.

7. **Canonical payload includes `path_and_query`.** The Ed25519 signature covers `"METHOD\npath_and_query\ntimestamp\nhex-sha256(body)"`. Use `uri.path_and_query()`, not just `uri.path()`, so query parameters are authenticated.

8. **X-Forwarded-For only from loopback peers.** `client_ip()` in `challenge.rs` trusts the header only when `addr.ip().is_loopback()`. Never call `headers.get("X-Forwarded-For")` directly in a handler.

9. **Secrets come from `SecretsProvider`, never the environment.** `[#445]` In production, `DATABASE_URL`, the Cloudflare token, etc. are fetched at runtime into memory via the `SecretsProvider` trait. Never read prod secrets from env, never write them to disk, never log them. The bootstrap session token lives on tmpfs only. `FileSecrets`/`EnvSecrets` are for dev/test with dummy values only.

10. **Tunnel upgrade requires Ed25519 challenge-response.** `[#445]` `GET /v1/installs/:id/tunnel` must verify a server-nonce challenge signed by the install's registered key before binding the tunnel — the bearer token alone is insufficient (an unauthenticated tunnel claim hijacks all of that install's traffic).

11. **Route inbound streams only through `TunnelRouter`.** `[#444/#445]` The SNI demuxer hands streams to the `TunnelRouter` trait — never look up the in-memory `TunnelRegistry` `DashMap` directly outside `LocalRouter`. Cross-node ownership lives in the `tunnel_routes` table; a node writes its ownership on tunnel connect and deletes it on disconnect.

12. **The tunnel registry is in-memory and per-node.** `[#444]` It is not persisted; after a node restart all Pis reconnect. The inter-node forward listener is **private-network-only and authenticated** (it bypasses SNI, so it must be). Treat `conn_id` as wrapping (`u32`).

## Test placement

Tests **must not** be inline (`mod tests { ... }` inside the source file). They belong in:
- `src/<module>/tests.rs` — unit tests of a single module
- `src/tests/<module>.rs` — repository integration tests

Declare them with `#[cfg(test)] mod tests;` at the bottom of the source file. Repository/integration tests run against a live Postgres started via `docker compose up -d` before running the tests.

## SQL conventions

- Query strings are `const &str` at module level — never inline in `sqlx::query(format!(...))`.
- **PostgreSQL** `[#444/#445]` stores `DateTime<Utc>` natively as `TIMESTAMPTZ` via sqlx's `chrono` feature — no `to_rfc3339()` / `.parse()` round-tripping.
- Mutations always use `self.pools.write`; reads always use `self.pools.read`.
- Postgres has no unsigned integers: store counters like `difficulty` as `INTEGER`/`BIGINT` and convert explicitly at the boundary (never `as`).
- Keep the Neon serverless pool rules in mind: `min_connections = 0`, graceful reconnect; do not hold an idle connection that would prevent autosuspend.

## Adding a new authenticated endpoint

1. Place it under `/v1/installs/` — the auth middleware enforces Ed25519 signing automatically.
2. Use the `AuthenticatedInstall` extractor to access the verified install:
   ```rust
   pub async fn my_handler(
       AuthenticatedInstall(install): AuthenticatedInstall,
       ...
   ) -> Result<..., ApiError> { ... }
   ```
3. Register the route in `api/mod.rs` via `utoipa_axum::routes!`.
4. Add `#[utoipa::path(...)]` with at least `401` in the responses.

## Adding a new unauthenticated endpoint

- Use a path prefix **other than** `/v1/installs/`.
- Annotate `#[utoipa::path]` with `security(())` to mark it public in the OpenAPI spec.

## Error handling

- Return `ApiError` from handlers — it maps to `(StatusCode, Json<ErrorBody>)` via `IntoResponse`.
- Wrap database errors with `map_err(ApiError::Internal)`.
- Use `ApiError::BadRequest`, `ApiError::Conflict`, `ApiError::TooManyRequests`, `ApiError::Unauthorized` for client errors.

## DNS provider

`DnsProvider` is a trait (`dns/mod.rs`). Production uses `CloudflareDnsProvider`. In tests, implement a `MockDnsProvider` or use the existing mock in `tests/api.rs`. Never call the Cloudflare REST API in unit tests.

## Validation

All name and public-key validation goes through `api/validation.rs`:
- `validate_name(&str) -> Result<(), ApiError>` — structured error messages for registration
- `is_valid_name(&str) -> bool` — availability endpoint (returns `false` for invalid names, no error)
- `validate_public_key(&str) -> Result<(), ApiError>` — verifies base64 + 32-byte length

`RESERVED_NAMES` is the single source of truth for reserved slugs.

## Running checks

```sh
# From repo root
make check-bridge   # cargo clippy -D warnings + cargo test  (Docker needed for DB tests)

# Or directly
cargo test   --manifest-path source/bridge/Cargo.toml
cargo clippy --manifest-path source/bridge/Cargo.toml --all-targets -- -D warnings
```

The bridge has no Linux-specific dependencies and builds natively on macOS.

## Local dev

Point `DATABASE_URL` at a local/Neon dev Postgres and use `FileSecrets`/env for the Cloudflare values
(dummy in tests). Never commit real Cloudflare tokens; in production secrets are resolved at runtime via
the `SecretsProvider` (Infisical) — see the infrastructure repo for provisioning.
