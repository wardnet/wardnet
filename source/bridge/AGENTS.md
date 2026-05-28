# wardnet-bridge agent guide

Conventions and invariants for agents working inside `source/bridge/`.

## Must-know invariants (never violate these)

1. **Bearer token never stored raw.** `register.rs` returns `hex(random_32_bytes)` to the caller once and stores only `hex(SHA-256(token))`. Never persist, log, or echo the raw token.

2. **DB token lookup is path-gated.** `auth_layer` only queries the DB when the request path starts with `/v1/installs/`. Adding a new public endpoint that starts with `/v1/installs/` would silently require auth — use a different path prefix.

3. **Uniqueness before challenge burn.** In `register.rs`, the `find_by_name` check always runs _before_ `challenges().consume()`. Reversing the order would consume the user's PoW proof on a name-conflict error, forcing them to solve another challenge.

4. **ReplayCache keyed on `{install_id}:{timestamp}:{body_hash}`.** Do not change this format without updating the replay window constant and tests. The window is ±120 s (double the timestamp window) to account for clock skew at the cache boundary.

5. **Body buffered before auth.** The 1 MiB body guard runs for _every_ request, including unauthenticated ones. It is the first thing `auth_layer` does — before any DB call.

6. **`pub_key_bytes` decoded once.** `InstallRow::into_install` decodes the base64 public key into `[u8; 32]` when the row is loaded from SQLite. Auth uses `install.pub_key_bytes` directly — never re-decode the base64 string on a hot path.

7. **Canonical payload includes `path_and_query`.** The Ed25519 signature covers `"METHOD\npath_and_query\ntimestamp\nhex-sha256(body)"`. Use `uri.path_and_query()`, not just `uri.path()`, so query parameters are authenticated.

8. **X-Forwarded-For only from loopback peers.** `client_ip()` in `challenge.rs` trusts the header only when `addr.ip().is_loopback()`. Never call `headers.get("X-Forwarded-For")` directly in a handler.

## Test placement

Tests **must not** be inline (`mod tests { ... }` inside the source file). They belong in:
- `src/<module>/tests.rs` — for unit tests of a single module
- `src/tests/<module>.rs` — for repository integration tests

Declare them with `#[cfg(test)] mod tests;` at the bottom of the source file.

## SQL conventions

- Query strings are `const &str` at module level — never inline in `sqlx::query(format!(...))`.
- SQLite stores `DateTime<Utc>` as ISO 8601 text via `to_rfc3339()` / `.parse::<DateTime<Utc>>()`.
- Mutations always use `self.pools.write`; reads always use `self.pools.read`.
- `difficulty` is `u32` in Rust and `INTEGER` (i64) in SQLite. Convert with `u32::try_from(row.difficulty)?` (not `as u32`).

## Adding a new authenticated endpoint

1. Place it under `/v1/installs/` — the auth middleware will enforce Ed25519 signing automatically.
2. Use the `AuthenticatedInstall` extractor to access the verified install:
   ```rust
   pub async fn my_handler(
       AuthenticatedInstall(install): AuthenticatedInstall,
       ...
   ) -> Result<..., ApiError> { ... }
   ```
3. Register the route in `api/mod.rs` via `utoipa_axum::routes!`.
4. Add `#[utoipa::path(...)]` annotation with at least `401` in the responses.

## Adding a new unauthenticated endpoint

- Use a path prefix **other than** `/v1/installs/`.
- Annotate the `#[utoipa::path]` with `security(())` to mark it public in the OpenAPI spec.

## Error handling

- Return `ApiError` from handlers — it maps to `(StatusCode, Json<ErrorBody>)` via `IntoResponse`.
- Wrap database errors with `map_err(ApiError::Internal)`.
- Use `ApiError::BadRequest`, `ApiError::Conflict`, `ApiError::TooManyRequests`, `ApiError::Unauthorized` for client errors.

## DNS provider

`DnsProvider` is a trait (`dns/mod.rs`). In production `CloudflareDnsProvider` is used. In tests, implement a `MockDnsProvider` or use the existing mock in `tests/api.rs`. Never call the Cloudflare REST API in unit tests.

## Validation

All name and public-key validation goes through `api/validation.rs`:
- `validate_name(&str) -> Result<(), ApiError>` — structured error messages for registration
- `is_valid_name(&str) -> bool` — availability endpoint (returns `false` for invalid names, no error)
- `validate_public_key(&str) -> Result<(), ApiError>` — verifies base64 + 32-byte length

`RESERVED_NAMES` is the single source of truth for reserved slugs.

## Running checks

```sh
# From repo root
make check-bridge   # cargo clippy -D warnings + cargo test

# Or directly
cargo test   --manifest-path source/bridge/Cargo.toml
cargo clippy --manifest-path source/bridge/Cargo.toml --all-targets -- -D warnings
```

The bridge has no Linux-specific dependencies and builds natively on macOS.

## Environment variables for local dev

```sh
DATABASE_URL=":memory:"
CLOUDFLARE_API_TOKEN=dummy
CLOUDFLARE_ZONE_ID=dummy
REGION=dev
SUBDOMAIN_PARENT=dev.wardnet.local
# LISTEN_ADDR defaults to 127.0.0.1:8080
```

Never commit real Cloudflare tokens. In production they are injected via the `BRIDGE_DEPLOY_HOST` / `BRIDGE_DEPLOY_KEY` GitHub secrets into the systemd environment file.
