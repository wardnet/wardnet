-- Private-DNS per-device grants (issue #912).
--
-- One row per device the admin has granted encrypted-DNS access to. The
-- `token` is the secret hostname label the device dials via DoT
-- (`<token>.<install>.my.<domain>`): high-entropy base-32, never logged by CT
-- (covered by the existing wildcard SAN), and doubling as the authentication
-- credential — the DoT listener resolves SNI token → device and closes the
-- connection on any unknown label (closed resolver, see #910).
--
-- Mirrors `inbound_wg_peers`: a grant is a property of an already-managed
-- device (`device_id` FK, one grant per device), created and revoked only
-- through the service layer. Deleting the device cascades the grant away —
-- a departed device must not keep a live resolver credential.
CREATE TABLE IF NOT EXISTS private_dns_grants (
    id          TEXT PRIMARY KEY,            -- UUID
    device_id   TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    token       TEXT NOT NULL,               -- base-32 secret hostname label
    created_at  TEXT NOT NULL                -- RFC 3339 (lexicographically sortable)
);

-- One grant per device; named so the unique-violation error is attributable
-- (same pattern as idx_inbound_wg_peers_device_id).
CREATE UNIQUE INDEX IF NOT EXISTS idx_private_dns_grants_device_id
    ON private_dns_grants (device_id);

-- The DoT listener resolves every accepted connection's SNI token to a device;
-- that lookup must never scan.
CREATE UNIQUE INDEX IF NOT EXISTS idx_private_dns_grants_token
    ON private_dns_grants (token);
