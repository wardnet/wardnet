-- Stage 8: support setting multiple DNS filter profiles as default.
-- The per-device side already accepts N profiles via dns_filter_device_profile;
-- this widens the global default fallback to match. The single-value
-- system_config row `dns_default_filter_profile_id` is replaced by a join
-- table mirroring dns_filter_device_profile.

CREATE TABLE IF NOT EXISTS dns_filter_default_profile (
    profile_id TEXT PRIMARY KEY NOT NULL
        REFERENCES dns_filter_profiles(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Carry the existing single default forward, if it points at a real profile.
-- The JOIN against dns_filter_profiles is what filters out empty/invalid
-- values — a malformed `sc.value` (including '') won't match any profile id.
INSERT OR IGNORE INTO dns_filter_default_profile (profile_id)
    SELECT sc.value
      FROM system_config sc
      JOIN dns_filter_profiles p ON p.id = sc.value
     WHERE sc.key = 'dns_default_filter_profile_id';

DELETE FROM system_config WHERE key = 'dns_default_filter_profile_id';
