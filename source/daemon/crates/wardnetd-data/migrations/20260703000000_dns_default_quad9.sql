-- Issue #636: add Quad9 (9.9.9.9) to the default upstream DNS forwarders.
--
-- The base dns migration (20260414000000_dns.sql) seeds the two-server default
-- [Cloudflare, Google]. This migration appends Quad9 to that default. It runs
-- once on every database:
--   * Fresh installs — the base seed inserts the two-server list, then this
--     upgrades it to include Quad9, so new installs ship with all three.
--   * Existing installs — upgraded to include Quad9 ONLY IF the stored list is
--     still exactly the untouched two-server default. The `WHERE value = ...`
--     guard leaves any customised upstream list untouched, so a user who edited
--     their forwarders never has this migration clobber their choice.
--
-- The compared strings must match the base migration's serialisation byte-for-
-- byte (serde emits fields in struct order: address, name, protocol; port and
-- tls_server_name are omitted when absent), which is also what the API writes
-- back when the default list is re-saved unchanged.
UPDATE system_config
SET value = '[{"address":"1.1.1.1","name":"Cloudflare","protocol":"udp"},{"address":"8.8.8.8","name":"Google","protocol":"udp"},{"address":"9.9.9.9","name":"Quad9","protocol":"udp"}]'
WHERE key = 'dns_upstream_servers'
  AND value = '[{"address":"1.1.1.1","name":"Cloudflare","protocol":"udp"},{"address":"8.8.8.8","name":"Google","protocol":"udp"}]';
