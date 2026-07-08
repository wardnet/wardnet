-- Device connection mode (issue #810).
--
-- Live status of how a device is currently reachable: `lan` (ARP/DHCP,
-- default) or `remote` (via the inbound WireGuard server, #809/#810).
-- Not a lineage/provenance tag — a device flips between the two as it
-- connects from different paths over time, last-observation-wins, exactly
-- like `last_ip` already does across DHCP renewals.
ALTER TABLE devices ADD COLUMN connection_mode TEXT NOT NULL DEFAULT 'lan';
