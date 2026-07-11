-- Link inbound WireGuard peers to an already-managed Device (issue #810).
--
-- Revises #809's original standalone-peer design: a remote-access grant is
-- now a property of a specific Device the admin already manages (discovered
-- on the LAN at least once), not a freestanding named credential that births
-- its own identity on first handshake. One credential per device (UNIQUE).
--
-- `device_id` is nullable at the schema level only because SQLite can't add
-- a NOT NULL column without a constant default and there is no sensible
-- default device; the application layer always sets it at peer-creation time
-- (see `InboundWgService::add_peer`) and there are no pre-existing rows to
-- backfill (the foundation table is unreleased).
--
-- SQLite's `ALTER TABLE ... ADD COLUMN` cannot add a `UNIQUE` (or otherwise
-- indexed) column inline — doing so fails with "Cannot add a UNIQUE column".
-- The one-credential-per-device guarantee is therefore expressed as a separate
-- `UNIQUE INDEX`. Because SQLite treats `NULL`s as distinct in a unique index,
-- this still permits any number of unlinked rows while forbidding two peers
-- from sharing the same non-NULL `device_id`.
ALTER TABLE inbound_wg_peers ADD COLUMN device_id TEXT
    REFERENCES devices(id) ON DELETE CASCADE;

CREATE UNIQUE INDEX IF NOT EXISTS idx_inbound_wg_peers_device_id
    ON inbound_wg_peers (device_id);
