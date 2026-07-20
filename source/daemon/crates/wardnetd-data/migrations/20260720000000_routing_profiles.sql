-- Routing profiles: per-domain routing rules grouped into named profiles,
-- assigned to devices in priority order. Mirrors the DNS filter profile model
-- (catalog + rules FK + ordered device assignment), but the target of a rule is
-- a routing decision (a tunnel or a direct carve-out) rather than a filter
-- action.

-- 1. Profile catalog. No builtins are seeded — routing targets reference a
--    user's own tunnels, so there is nothing sensible to ship by default.
CREATE TABLE IF NOT EXISTS routing_profiles (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- 2. Domain routing rules. `target_kind` is 'tunnel' (route matched traffic
--    through `tunnel_id`) or 'direct' (carve the domain out of the device's
--    tunnel back to the WAN — `tunnel_id` is then NULL). One pattern per
--    profile keeps a profile's rule set unambiguous.
CREATE TABLE IF NOT EXISTS routing_profile_rules (
    id          TEXT PRIMARY KEY NOT NULL,
    profile_id  TEXT NOT NULL REFERENCES routing_profiles(id) ON DELETE CASCADE,
    pattern     TEXT NOT NULL,
    target_kind TEXT NOT NULL,
    tunnel_id   TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (profile_id, pattern)
);
CREATE INDEX IF NOT EXISTS idx_routing_profile_rules_profile
    ON routing_profile_rules(profile_id);

-- 3. Ordered many-to-many device -> profile assignment. `position` is the
--    device-local priority: when several assigned profiles match a domain, the
--    lowest `position` wins. A device with no rows keeps its plain per-device
--    routing with no domain overrides.
CREATE TABLE IF NOT EXISTS routing_device_profile (
    device_id  TEXT NOT NULL REFERENCES devices(id)          ON DELETE CASCADE,
    profile_id TEXT NOT NULL REFERENCES routing_profiles(id) ON DELETE CASCADE,
    position   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (device_id, profile_id)
);
CREATE INDEX IF NOT EXISTS idx_routing_device_profile_device
    ON routing_device_profile(device_id);
CREATE INDEX IF NOT EXISTS idx_routing_device_profile_profile
    ON routing_device_profile(profile_id);
