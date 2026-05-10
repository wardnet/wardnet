-- Canonicalise MAC addresses to lowercase across every table that stores
-- one (issue #312). Two casings co-existed historically: packet-capture
-- emitted uppercase (`AA:BB:…`), DHCP service emitted lowercase
-- (`aa:bb:…`), and equality lookups (`*_by_mac`) are case-sensitive in
-- SQLite, so cross-source joins silently failed. Repository writers and
-- readers now lowercase defensively; this migration brings existing rows
-- into the same form.
--
-- Also lowers the two MAC keys in `system_config` (`router_mac`,
-- `garp_router_mac`) for consistency. Both feed log lines, not equality
-- joins, so the only practical effect is uniform display.

UPDATE devices            SET mac         = LOWER(mac);
UPDATE dhcp_leases        SET mac_address = LOWER(mac_address);
UPDATE dhcp_reservations  SET mac_address = LOWER(mac_address);

UPDATE system_config
   SET value = LOWER(value)
 WHERE key IN ('router_mac', 'garp_router_mac')
   AND value <> '';

-- Denormalise MAC onto the lease audit log so the trail stays queryable
-- after the parent lease row is rotated (released, expired, replaced by
-- a re-DISCOVER with the same MAC, etc.). Pre-existing rows backfill
-- from `dhcp_leases` where the lease still exists; orphaned rows fall
-- back to '' so the NOT NULL constraint holds. New writes always carry
-- the MAC explicitly (see `DhcpServiceImpl::*_lease`).
ALTER TABLE dhcp_lease_log ADD COLUMN mac_address TEXT NOT NULL DEFAULT '';

UPDATE dhcp_lease_log
   SET mac_address = COALESCE(
       (SELECT LOWER(l.mac_address) FROM dhcp_leases l WHERE l.id = dhcp_lease_log.lease_id),
       ''
   )
 WHERE mac_address = '';

CREATE INDEX IF NOT EXISTS idx_dhcp_lease_log_mac
    ON dhcp_lease_log(mac_address);
