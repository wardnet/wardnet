-- Network Zones foundation (epic #244, issue #735).
--
-- Introduces the `network_zones` policy-bucket table, seeds the three system
-- zones (Trusted / IoT / Guest), and gives every device a NOT NULL `zone_id`
-- foreign key. SQLite forbids `ALTER TABLE ADD COLUMN` of a NOT NULL column
-- that carries a `REFERENCES` clause, so `devices` is rebuilt using the
-- established table-rebuild pattern (see 20260506000000_dns_filtering.sql).
--
-- This migration is DESTRUCTIVE: existing device rows are NOT copied. Every
-- device (and its cascaded routing rules, DNS capture, filter assignments, and
-- rule requests) is cleared. On next discovery every device re-enters under the
-- default-for-new zone (Guest). This supersedes epic #244 locked-decision #7
-- ("backfill all devices to Trusted, zero behavior change") — see the ADR
-- `adr-network-zone-isolation.md`.

-- 1. network_zones catalog.
CREATE TABLE IF NOT EXISTS network_zones (
    id                  TEXT PRIMARY KEY NOT NULL,
    name                TEXT NOT NULL UNIQUE,
    provenance          TEXT NOT NULL DEFAULT 'manual',        -- 'system' | 'manual'
    isolation_stance    TEXT NOT NULL DEFAULT 'shared_subnet', -- ladder rung
    allowed_targets     TEXT NOT NULL DEFAULT '["direct","tunnel"]', -- JSON array of kinds
    member_isolation    INTEGER NOT NULL DEFAULT 0,
    subnet_cidr         TEXT,                                   -- NULL until CI-3 #737
    admin_ui_reachable  INTEGER NOT NULL DEFAULT 1,
    is_default          INTEGER NOT NULL DEFAULT 0,
    is_default_for_new  INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- At most one anchor / one default-for-new zone (partial unique indexes).
CREATE UNIQUE INDEX IF NOT EXISTS idx_network_zones_one_default
    ON network_zones(is_default) WHERE is_default = 1;
CREATE UNIQUE INDEX IF NOT EXISTS idx_network_zones_one_default_for_new
    ON network_zones(is_default_for_new) WHERE is_default_for_new = 1;

-- 2. Seed the three system zones (fixed UUIDs in the 00000000-…-0000000002xx block).
--    Trusted is the anchor (is_default); Guest catches freshly-discovered
--    devices (is_default_for_new).
INSERT INTO network_zones
  (id, name, provenance, isolation_stance, allowed_targets, member_isolation,
   admin_ui_reachable, is_default, is_default_for_new) VALUES
  ('00000000-0000-0000-0000-000000000201', 'Trusted', 'system', 'shared_subnet',
     '["direct","tunnel"]', 0, 1, 1, 0),
  ('00000000-0000-0000-0000-000000000202', 'IoT', 'system', 'shared_subnet',
     '["direct","tunnel"]', 0, 0, 0, 0),
  ('00000000-0000-0000-0000-000000000203', 'Guest', 'system', 'shared_subnet',
     '["direct","tunnel"]', 0, 1, 0, 1);

-- 3. Rebuild `devices` with a NOT NULL zone_id FK. Old rows are NOT copied
--    (fresh start). Because we do not copy rows, the per-device child tables
--    are cleared explicitly: with foreign_keys OFF the DROP would otherwise
--    orphan their rows (the CASCADE / SET NULL actions do not fire while FK
--    enforcement is disabled).
PRAGMA foreign_keys = OFF;

-- 3a. Clear CASCADE children (NOT NULL device_id) so no rows dangle.
DELETE FROM routing_rules;
DELETE FROM dns_events;
DELETE FROM device_rule_requests;
DELETE FROM dns_filter_device_settings;
DELETE FROM dns_filter_device_profile;

-- 3b. Null out the SET NULL children (nullable device_id) so they self-heal.
UPDATE dhcp_leases  SET device_id = NULL WHERE device_id IS NOT NULL;
UPDATE dns_query_log SET device_id = NULL WHERE device_id IS NOT NULL;

-- 3c. Rebuild the devices table itself.
CREATE TABLE devices_new (
    id           TEXT PRIMARY KEY NOT NULL,
    mac          TEXT NOT NULL UNIQUE,
    name         TEXT,
    hostname     TEXT,
    manufacturer TEXT,
    device_type  TEXT NOT NULL DEFAULT 'unknown',
    first_seen   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_seen    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_ip      TEXT NOT NULL,
    admin_locked INTEGER NOT NULL DEFAULT 0,
    zone_id      TEXT NOT NULL REFERENCES network_zones(id) ON DELETE RESTRICT,
    dns_capture_enabled   INTEGER NOT NULL DEFAULT 0,
    dns_capture_cap_count INTEGER NOT NULL DEFAULT 1000,
    dns_capture_cap_days  INTEGER NOT NULL DEFAULT 7
);
-- intentionally NO "INSERT INTO devices_new SELECT … FROM devices" — devices are dropped.
DROP TABLE devices;
ALTER TABLE devices_new RENAME TO devices;
CREATE INDEX IF NOT EXISTS idx_devices_mac     ON devices(mac);
CREATE INDEX IF NOT EXISTS idx_devices_last_ip ON devices(last_ip);
CREATE INDEX IF NOT EXISTS idx_devices_zone_id ON devices(zone_id);

PRAGMA foreign_keys = ON;
