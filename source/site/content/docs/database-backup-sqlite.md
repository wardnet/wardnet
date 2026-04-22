# Database backup — SQLite

This page covers how the SQLite database backend handles the
database portion of a Wardnet backup. It's a companion to the
[backup & restore guide](/docs/backup-restore) — the mechanics
below are specific to SQLite, but the surrounding bundle format,
encryption, preview/apply wizard, and retention rules are the same
regardless of which database backend is configured.

## Why it's safe to back up while running

Wardnet runs SQLite with WAL mode, foreign-key enforcement, and
`NORMAL` synchronous level. The daemon is writing to the database
continuously — device observations, DNS query log, tunnel stats,
lease events — so a naive `cp` (or a filesystem-level snapshot)
during normal operation can capture torn pages or miss WAL frames
that haven't checkpointed yet.

Wardnet uses [`VACUUM INTO`](https://www.sqlite.org/lang_vacuum.html#vacuuminto)
instead:

```sql
VACUUM INTO '/var/lib/wardnet/.wardnet-dump-<uuid>.db';
```

What that gives you:

- **Consistency.** `VACUUM INTO` takes an implicit read lock for the
  duration of the copy. The output file is byte-identical to what a
  `SELECT *` would see at the moment the statement began — no torn
  pages, no half-applied transactions, no unflushed WAL frames.
- **No downtime.** Other connections keep reading and writing during
  the operation. SQLite serves those writes against the pre-vacuum
  snapshot while the copy progresses.
- **Self-contained output.** The file is a fully-valid SQLite
  database a fresh daemon can open without any recovery step — not
  a raw `.db` + `.db-wal` + `.db-shm` trio.

Once the dump completes, the file is read into memory, deleted, and
the bytes are handed off to be packed into the encrypted bundle.

## Restoring in place

Restore is the inverse — but it has to contend with the fact that
the running daemon has an open connection pool pointed at the live
`wardnet.db` file. Swapping the file out from under an active pool
is safe on Linux (the pool holds an open file descriptor to the
inode, which becomes unlinked-but-alive), but any subsequent writes
from that pool would land in the now-deleted inode and vanish.

The restore sequence:

1. **Stage** — the bundle's database bytes are written to a sibling
   path: `.wardnet-restore-<uuid>.db`.
2. **Atomic swap** — `rename(2)` moves the staging file over the
   live path. Same-filesystem renames are atomic on Linux, so the
   daemon never observes a half-written or missing database.
3. **Schema probe** — a throwaway read-only connection opens the
   restored file and reads `MAX(version)` from `_sqlx_migrations`.
   The value is logged alongside the restore so operators have a
   paper trail.
4. **Mark restart required** — a `backup_restart_pending` flag is
   written into the restored database's `system_config` table. The
   web UI surfaces a restart confirmation; after the supervisor
   brings the daemon back, the new pool connects to the restored
   file.

The step-3 probe is why the restore is safe to hand off to a normal
migrations pass on startup — if the bundle came from an older
daemon, any missing migrations run cleanly against the freshly
restored file before it starts serving traffic.

## Manual recovery from `.bak-*` siblings

Every restore leaves a `wardnet.db.bak-<timestamp>` sibling next to
the live file. The [retention section of the main
guide](/docs/backup-restore#snapshot-retention) covers the general
recovery flow; for SQLite specifically:

```bash
sudo systemctl stop wardnetd

# The .bak-* file is a fully-valid SQLite database produced by
# `rename(2)`, not a copy. Opening it read-only is completely safe:
sqlite3 /var/lib/wardnet/wardnet.db.bak-20260421T143022Z \
  "SELECT COUNT(*) FROM devices;"

# Roll back:
sudo mv /var/lib/wardnet/wardnet.db.bak-20260421T143022Z \
        /var/lib/wardnet/wardnet.db
sudo systemctl start wardnetd
```

Any stale `.db-wal` / `.db-shm` siblings from the previous pool are
harmless — SQLite will ignore them when it opens the rolled-back
file because they belong to a different inode.

## What's not in the dump

A few pieces of state live alongside the database but outside it:

- **`/var/lib/wardnet/secrets/`** — WireGuard private keys. Travels
  in the bundle via the secret store, not the dump.
- **`/var/lib/wardnet/updates/`** — staged auto-update artefacts.
  Regenerated next time the update runner polls.
- **Rolling DNS blocklists** — the blocklist URLs, schedules, and
  per-list state live in the database, so they survive a restore.
  The downloaded domain files are re-fetched on the next refresh
  cycle.

Because the database carries every piece of operator intent —
devices, tunnels, routing rules, DHCP reservations, DNS config,
admin accounts, session tokens — restoring the database dump is
enough to re-constitute a fully-functioning daemon on a fresh host.
The rest re-populates itself from the network.
