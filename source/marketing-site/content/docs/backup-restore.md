# Backup & restore

Wardnet ships with a first-class backup system. One click produces a
single encrypted archive of the database, operator config, and
`WireGuard` private keys. Another click restores it. Use it when you're
migrating to new hardware, recovering from an SD-card failure, or just
giving yourself a safety net before a risky configuration change.

## What a backup contains

Every bundle is a single `.wardnet.age` file holding:

- **Database snapshot** — a point-in-time copy of everything the
  daemon persists: devices, tunnels, DHCP leases, DNS blocklists,
  admin accounts, session tokens, and system configuration. Captured
  in a way that's safe to take while the daemon is serving traffic
  — no lock, no downtime. The database provider ships its own
  snapshot routine behind a shared trait; the SQLite provider (what
  every installation uses today) is covered in
  [database backup — SQLite](/docs/database-backup-sqlite) if
  you're curious about the mechanics.
- **`wardnet.toml`** — the operator configuration file.
- **`secrets/…`** — everything the configured secret store wants
  to include. For the default `file_system` provider this is every
  WireGuard private key under `/var/lib/wardnet/secrets/wireguard/`.
  External providers (HashiCorp Vault, 1Password) may contribute
  nothing because their secrets live in the external service.
- **`manifest.json`** — bundle metadata: daemon version, schema
  version, host identifier, creation timestamp, secret count, and the
  bundle format version. The importer reads this first and refuses
  bundles that are newer than the running daemon can handle.

Logs, blocklist caches, the update staging directory, and the daemon
binary itself are **not** included — they're either regenerated or
unrelated to daemon state.

## Encryption

Bundles are always encrypted. Wardnet uses
[age](https://age-encryption.org) in passphrase mode:

- Passphrases are stretched through scrypt.
- The archive is encrypted with ChaCha20-Poly1305.
- The passphrase is supplied by the admin at export time and required
  again on restore.

Wardnet **never stores or logs the passphrase** anywhere. If you lose
it the bundle is unrecoverable. Write it down in a password manager
before you close the dialog.

The minimum passphrase length is **12 characters**, enforced on both
the export and restore sides.

## Exporting a backup

From the web UI:

1. Sign in as an admin and navigate to **Settings**.
2. Scroll to the **Backup & restore** card.
3. Click **Download backup**.
4. In the dialog, choose a strong passphrase and confirm it.
5. Click **Download**. The browser saves a file named
   `wardnet-<timestamp>.wardnet.age`.

![Settings page with the Backup and restore card visible](/docs/backup-restore/settings-backup-card.png "wide")

![Download backup dialog asking for a passphrase and confirmation](/docs/backup-restore/export-dialog.png)

Exports are safe to trigger during normal operation — device
discovery, DNS resolution, and tunnels are not interrupted. The
database provider guarantees a consistent snapshot without taking an
exclusive lock; see the
[SQLite-specific guide](/docs/database-backup-sqlite) for how that
works under the hood.

## Restoring a backup

Restore is a two-step wizard: you preview first, then confirm. The
daemon never touches disk until you explicitly click the apply button.

### Step 1 — Preview

1. Sign in as an admin on the **target** daemon (fresh install or an
   existing one you want to overwrite) and navigate to **Settings →
   Backup & restore**.
2. Click **Restore from backup**. The dialog lets you pick the
   `.wardnet.age` file and enter its passphrase:

![Restore dialog with a file picker and passphrase input](/docs/backup-restore/restore-upload-dialog.png)

3. Click **Preview**. The daemon decrypts the bundle in memory,
   validates it against the running version, and returns a summary:

![Restore preview dialog listing manifest details and files to replace](/docs/backup-restore/restore-preview-dialog.png)

The preview shows:

- **From version** — the daemon version that produced the bundle.
- **Host ID** — source machine identifier (usually the hostname).
- **Created** — when the bundle was exported.
- **Schema version** — the database migration level.
- **WireGuard keys** — number of secrets the bundle carries.
- **Will replace** — the paths on disk that will be renamed to
  `.bak-<timestamp>` siblings and overwritten.

If the bundle is incompatible — the format version is newer than the
running daemon can handle, or the schema version is ahead of what the
daemon has applied — the dialog surfaces a red "Bundle incompatible"
banner with the specific reason and the apply button is disabled.
Upgrade the daemon to a matching version and try again.

### Step 2 — Apply

1. Review the preview carefully.
2. Click **Apply restore**.

The daemon:

1. Renames the live database, `wardnet.toml`, and the current
   secret store contents to `.bak-<timestamp>` siblings in the same
   directory.
2. Writes the bundle's database, config, and secrets into place.
3. Sets a `backup_restart_pending` marker in the database.
4. Returns success.

A progress dialog opens automatically and walks through the restart
cycle. The daemon exits, systemd brings it back with the restored
state, and the UI moves through the phases live:

![Restart progress dialog showing a spinner while the daemon restarts](/docs/backup-restore/restart-progress-dialog.png)

Once the daemon is answering again and your session is still valid,
the dialog flips to a green confirmation and you can dismiss it:

![Restart progress dialog showing the daemon is back online](/docs/backup-restore/restart-ready-dialog.png)

If the restore invalidated your session cookie (for example, the
bundle came from a different daemon with a different admin account),
the same dialog switches to a "Sign in again" state instead.

## Snapshot retention

Every restore leaves `.bak-<timestamp>` siblings next to the files it
replaced. They live for **24 hours** before a background cleanup task
deletes them. This is your manual-recovery path if a restore turned
out to be the wrong bundle:

```bash
# Stop the daemon
sudo systemctl stop wardnetd

# Inspect what was kept
ls -lh /var/lib/wardnet/wardnet.db.bak-* 2>/dev/null
ls -lh /etc/wardnet/wardnet.toml.bak-*
ls -lh /var/lib/wardnet/secrets.bak-*.json

# Roll back by renaming (example for the default SQLite provider)
sudo mv /var/lib/wardnet/wardnet.db.bak-20260421T143022Z \
        /var/lib/wardnet/wardnet.db

# Restart
sudo systemctl start wardnetd
```

The secrets snapshot is a base64-encoded JSON dump; restoring it
requires re-inserting the entries via the API or a future
`wctl vault` subcommand.

## Bundle format compatibility

Bundles carry a `bundle_format_version` (currently `1`). The import
rules are:

- `bundle_format_version` **equal to** the running daemon's supported
  version → applied directly.
- `bundle_format_version` **higher than** supported → rejected. Upgrade
  the daemon first.
- `bundle_format_version` **lower than** supported → applied; the
  daemon runs any pending migrations against the restored database
  so it catches up to the current schema.

Schema versions follow the same rule. This means a bundle produced by
an old daemon can always be restored onto a newer one, but not the
other way around. Upgrade before you restore.

## Scheduling backups

Scheduled backups are planned for a follow-up release. The current
release supports **manual export** only. When scheduled backups land
they'll reuse the same bundle format, so any bundles you produce
today will still be restorable once scheduling is available.

## Threat model

A backup bundle is designed to be safe to store in any location you
trust with an encrypted blob — an external drive, cloud object
storage, an email attachment — because:

- The ChaCha20-Poly1305 AEAD prevents silent tampering.
- Decryption is gated by the scrypt-hardened passphrase.
- There are no plaintext secrets, session tokens, or private keys
  anywhere in the archive.

The attacker who matters is the one who obtains **both** your bundle
and your passphrase. Pick a passphrase that survives an offline
brute-force attempt, store it separately from the bundle, and you're
good.

## See also

- [Installation](/docs/installation) — get Wardnet running on a Pi.
- [Configuration](/docs/configuration) — `wardnet.toml` reference,
  including the `[secret_store]` section the backup flow relies on.
- [Database backup — SQLite](/docs/database-backup-sqlite) — how the
  SQLite provider captures a consistent snapshot and restores it in
  place.
