# Backup subsystem

Backup/restore is composed of three primitive traits plus a service
that orchestrates them. Nothing in the subsystem encodes the
database provider directly — swap SQLite for rqlite or Postgres and
only one file (`database_dumper.rs`) needs to change.

## Primitives

- **`BackupArchiver`** (`wardnetd-services/src/backup/archiver.rs`) —
  turns bundle inputs (`BundleContents`) into an encrypted
  `.wardnet.age` byte stream and back. Reference impl: `AgeArchiver`
  (tar + gzip + [age](https://age-encryption.org) passphrase
  encryption). Stateless; safe to share via `Arc`.
- **`DatabaseDumper`** (`wardnetd-data/src/database_dumper.rs`) —
  captures a consistent database snapshot and restores one in place.
  Lives in `wardnetd-data` alongside the repositories because
  snapshot/restore is a responsibility of the database provider, not
  the backup service. The SQLite impl (`SqliteDumper`) uses `VACUUM
  INTO` for capture and atomic `rename(2)` for restore; see the
  [public docs](../source/site/content/docs/database-backup-sqlite.md)
  for the user-facing mechanics.
- **`SecretStore::backup_contents` / `restore_from_backup`**
  (`wardnetd-data/src/secret_store.rs`) — each provider decides what
  travels with a bundle. `FileSecretStore` ships every entry under
  the store root; external providers (HashiCorp Vault, 1Password,
  AWS Secrets Manager) can return empty lists because their secrets
  live in the external service, not the bundle.

## Service

`BackupService` (`wardnetd-services/src/backup/service.rs`) composes
the three primitives via:

- `RepositoryFactory::dumper()` for the database portion
- the shared `SecretStore` trait object from `Backends`
- a stateless `AgeArchiver`

### Two-phase import

Restore is **preview → apply**. Preview decrypts the bundle in
memory, validates compatibility (`bundle_format_version`,
`schema_version`), and caches the unpacked contents under a
short-lived `preview_token` (5 min TTL). Apply consumes the token
and commits. Nothing on disk changes between preview and apply.

### Apply swap

On apply, the service:

1. Renames the live database / config / secret-store state to
   `.bak-<timestamp>` siblings in the same directory (so operators
   have a manual recovery path).
2. Writes the bundle's contents into the live paths via the
   dumper's `restore` and the secret store's `restore_from_backup`.
3. Sets `backup_restart_pending = "true"` in `system_config`.
4. Returns the list of snapshots created.

### Deploy-time-only config keys (issue #1112)

Step 2 does **not** write the bundle's `wardnet.toml` verbatim. The
systemd unit grants `ReadWritePaths=/etc/wardnet` so restore can write
the config at all, which puts that file within reach of an admin
session: export a bundle, edit the TOML (the passphrase is the admin's
own), repack, restore, restart. `[update] allow_edge_channel` is
exactly what must stay out of that reach — ADR-0023 makes putting a box
on the edge channel take root *on that box*.

`wardnet_common::config_restore::preserve_deploy_time_keys` therefore
merges the bundle's config over the live one, taking every key in
`DEPLOY_TIME_ONLY_KEYS` from the live machine: the update path, the
filesystem locations the sandbox pins, this box's hardware identity, and
the test-harness overrides that are never set in production. The merge is
untyped (`toml::Table`, never `ApplicationConfiguration`) so the restored
file keeps the shape the operator wrote instead of becoming a dump of every
defaulted field, and a bundle that changes none of those keys is passed
through byte-for-byte so its comments survive.

Untyped is not unchecked. `ApplicationConfiguration` is
`deny_unknown_fields` and a restore is followed by a mandatory restart, so a
bundle carrying an unknown section or a wrongly-typed value would write
cleanly and then leave the daemon failing to start with no UI left to fix it
from. `check_bundle_config` runs as part of `check_compat`, so the *preview*
reports it — the operator finds out before committing, not after — and the
merged result is re-checked before it is written.

Adding a config field without deciding which side of that line it falls on
fails `wardnet-common`'s
`tests::config_restore::every_config_key_is_classified`. That guard reads
serde's own field lists rather than a serialised sample, because `toml`
omits an `Option` that is `None` and would hide such a field from it.

The running SQLite pool still points at the (now renamed) old file
via its open file descriptor, so writes from the old pool would land
in the unlinked inode and vanish. That's why the response tells the
client to restart the daemon — the new pool connects to the restored
file. The web UI surfaces this automatically through the
`RestartProgressDialog` flow (see `source/web-ui/src/hooks/useRestart.ts`).

### Background cleanup

`BackupCleanupRunner` (`wardnetd-services/src/backup/runner.rs`)
runs on a 1-hour interval (± jitter) under the root
`wardnetd{version=...}` tracing span. It deletes `.bak-<timestamp>`
siblings older than **24 hours**.

## API surface

All endpoints are admin-guarded and registered via
`utoipa_axum::OpenApiRouter`:

- `GET /api/backup/status` — current subsystem phase
- `POST /api/backup/export` — JSON body with passphrase; returns
  `application/octet-stream` with a `Content-Disposition` attachment
- `POST /api/backup/import/preview` — `multipart/form-data` with
  `bundle` (binary) + `passphrase` (text), returns a preview token
- `POST /api/backup/import/apply` — JSON body with the preview token
- `GET /api/backup/snapshots` — list of retained `.bak-*` siblings
