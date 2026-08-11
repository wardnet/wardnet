-- Managed devices + retention (issue #1181).
--
-- "Managed" was previously *inferred* from `name IS NOT NULL`, which is wrong
-- in both directions: a device granted Private DNS, a DHCP reservation, or a
-- Remote peer credential rendered as "Discovered", while a bare name alone made
-- a device "Managed". Retention (deleting long-absent devices) cannot be built
-- on that inference — pruning on `name IS NULL` would silently revoke remote
-- access and orphan admin-authored MAC-keyed rows that have no FK to clean them
-- up.
--
-- So `managed` becomes an explicit, latching column, promoted by any admin
-- configuration act and cleared only by an explicit release. That gives the
-- invariant the prune depends on:
--
--     managed = 0  =>  no admin artefacts exist for this device
--
-- which is why deleting an unmanaged row can never orphan anything.
ALTER TABLE devices ADD COLUMN managed INTEGER NOT NULL DEFAULT 0;

-- Backfill: any ADMIN-created artefact means the admin decided to control this
-- device. Self-service artefacts (user-created routing rules, device rule
-- requests, push subscriptions) deliberately do NOT promote — those are the
-- device asking, not the admin deciding, and promoting on them would make
-- every guest phone permanently unprunable.
--
-- Deliberate gap: a past zone reassignment is NOT backfilled. Zone membership
-- is sticky (CONTEXT.md), so `zone_id != <default-for-new>` is
-- indistinguishable from "the default-for-new flag was later re-pointed at a
-- different zone". Guessing inclusively would mark essentially every existing
-- device managed, silently disabling retention on exactly the boxes that need
-- it. Going forward `assign_device` promotes explicitly. See
-- docs/adr/0032-managed-devices-and-retention.md.
UPDATE devices SET managed = 1 WHERE
       name IS NOT NULL
    OR admin_locked = 1
    OR dns_capture_enabled = 1
    OR EXISTS (SELECT 1 FROM routing_rules r
               WHERE r.device_id = devices.id AND r.created_by = 'admin')
    OR EXISTS (SELECT 1 FROM routing_device_profile     t WHERE t.device_id = devices.id)
    OR EXISTS (SELECT 1 FROM dns_filter_device_settings t WHERE t.device_id = devices.id)
    OR EXISTS (SELECT 1 FROM dns_filter_device_profile  t WHERE t.device_id = devices.id)
    -- No `dns_device_adblock` clause: that table was dropped by
    -- 20260506000000_dns_filtering.sql and superseded by
    -- `dns_filter_device_settings` above.
    OR EXISTS (SELECT 1 FROM private_dns_grants         t WHERE t.device_id = devices.id)
    OR EXISTS (SELECT 1 FROM inbound_wg_peers           t WHERE t.device_id = devices.id)
    OR EXISTS (SELECT 1 FROM dhcp_reservations t WHERE t.mac_address = devices.mac)
    OR EXISTS (SELECT 1 FROM zone_exceptions t
               WHERE (t.from_kind = 'device' AND t.from_id = devices.id)
                  OR (t.to_kind   = 'device' AND t.to_id   = devices.id));

-- The retention prune scans by `last_seen`.
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen);
