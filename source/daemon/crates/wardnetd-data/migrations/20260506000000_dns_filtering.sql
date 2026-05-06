-- Stage 7: per-device DNS filtering with profiles.
-- Renames the dns_* filter tables to dns_filter_*, introduces DNS Filter
-- Profiles, and rewires the per-device toggle on top of the new model.

-- 1. Profile catalog. The three builtin profiles are seeded here so the
--    rebuilt filter-source tables can FK against the Ad Blocking row.
CREATE TABLE IF NOT EXISTS dns_filter_profiles (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL UNIQUE,
    builtin    INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT INTO dns_filter_profiles (id, name, builtin) VALUES
    ('00000000-0000-0000-0000-000000000100', 'Ad Blocking',         1),
    ('00000000-0000-0000-0000-000000000101', 'Parental Controls',   1),
    ('00000000-0000-0000-0000-000000000102', 'Malware & Phishing',  1);

-- 2. Rename the four filter-source tables. SQLite auto-rewrites FKs that
--    reference a renamed table (dns_blocked_domains' FK to dns_blocklists
--    becomes a FK to dns_filter_blocklists).
ALTER TABLE dns_blocklists       RENAME TO dns_filter_blocklists;
ALTER TABLE dns_blocked_domains  RENAME TO dns_filter_blocked_domains;
ALTER TABLE dns_allowlist        RENAME TO dns_filter_allowlist;
ALTER TABLE dns_custom_rules     RENAME TO dns_filter_custom_rules;

DROP INDEX IF EXISTS idx_dns_blocked_domains_domain;
CREATE INDEX IF NOT EXISTS idx_dns_filter_blocked_domains_domain
    ON dns_filter_blocked_domains(domain);

-- 3. Rebuild each filter-source table to add a `profile_id` column that
--    FKs onto `dns_filter_profiles`. SQLite forbids
--    `ALTER TABLE ... ADD COLUMN ... REFERENCES`, so we drop+recreate.
--    Existing rows get backfilled to the Ad Blocking profile so current
--    behaviour is preserved.

-- 3a. Blocklists (was UNIQUE(url); now UNIQUE(profile_id, url)).
CREATE TABLE dns_filter_blocklists_new (
    id            TEXT PRIMARY KEY NOT NULL,
    profile_id    TEXT NOT NULL REFERENCES dns_filter_profiles(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    url           TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    entry_count   INTEGER NOT NULL DEFAULT 0,
    last_updated  TEXT,
    last_error    TEXT,
    last_error_at TEXT,
    cron_schedule TEXT NOT NULL DEFAULT '0 3 * * *',
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (profile_id, url)
);
INSERT INTO dns_filter_blocklists_new
    (id, profile_id, name, url, enabled, entry_count, last_updated,
     last_error, last_error_at, cron_schedule, created_at, updated_at)
    SELECT id,
           '00000000-0000-0000-0000-000000000100',
           name, url, enabled, entry_count, last_updated,
           last_error, last_error_at, cron_schedule, created_at, updated_at
      FROM dns_filter_blocklists;
DROP TABLE dns_filter_blocklists;
ALTER TABLE dns_filter_blocklists_new RENAME TO dns_filter_blocklists;

-- 3b. Allowlist (was UNIQUE(domain); now UNIQUE(profile_id, domain)).
CREATE TABLE dns_filter_allowlist_new (
    id         TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES dns_filter_profiles(id) ON DELETE CASCADE,
    domain     TEXT NOT NULL,
    reason     TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (profile_id, domain)
);
INSERT INTO dns_filter_allowlist_new (id, profile_id, domain, reason, created_at)
    SELECT id,
           '00000000-0000-0000-0000-000000000100',
           domain, reason, created_at
      FROM dns_filter_allowlist;
DROP TABLE dns_filter_allowlist;
ALTER TABLE dns_filter_allowlist_new RENAME TO dns_filter_allowlist;

-- 3c. Custom rules.
CREATE TABLE dns_filter_custom_rules_new (
    id         TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES dns_filter_profiles(id) ON DELETE CASCADE,
    rule_text  TEXT NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1,
    comment    TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
INSERT INTO dns_filter_custom_rules_new
    (id, profile_id, rule_text, enabled, comment, created_at, updated_at)
    SELECT id,
           '00000000-0000-0000-0000-000000000100',
           rule_text, enabled, comment, created_at, updated_at
      FROM dns_filter_custom_rules;
DROP TABLE dns_filter_custom_rules;
ALTER TABLE dns_filter_custom_rules_new RENAME TO dns_filter_custom_rules;

-- 4. Per-device kill switch. Absence of a row means enabled = true.
CREATE TABLE IF NOT EXISTS dns_filter_device_settings (
    device_id  TEXT PRIMARY KEY NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    enabled    INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- 5. Many-to-many device → profile assignment. Empty set → fall through
--    to dns_default_filter_profile_id.
CREATE TABLE IF NOT EXISTS dns_filter_device_profile (
    device_id  TEXT NOT NULL REFERENCES devices(id)             ON DELETE CASCADE,
    profile_id TEXT NOT NULL REFERENCES dns_filter_profiles(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (device_id, profile_id)
);
CREATE INDEX IF NOT EXISTS idx_dns_filter_device_profile_device
    ON dns_filter_device_profile(device_id);
CREATE INDEX IF NOT EXISTS idx_dns_filter_device_profile_profile
    ON dns_filter_device_profile(profile_id);

-- 6. Drop the legacy unused per-device adblock table — replaced by
--    dns_filter_device_settings + dns_filter_device_profile.
DROP TABLE IF EXISTS dns_device_adblock;

-- 7. Rename the global emergency-stop key and seed the default profile.
--    DNS settings live in system_config KV (no `dns_config` table).
UPDATE system_config
   SET key = 'dns_filtering_enabled'
 WHERE key = 'dns_ad_blocking_enabled';

INSERT OR IGNORE INTO system_config (key, value) VALUES
    ('dns_default_filter_profile_id', '00000000-0000-0000-0000-000000000100');

-- 8. Seed curated, disabled blocklist URLs into the two new builtin profiles.
INSERT OR IGNORE INTO dns_filter_blocklists
    (id, profile_id, name, url, enabled, cron_schedule)
VALUES
    ('00000000-0000-0000-0000-000000000110',
     '00000000-0000-0000-0000-000000000101',
     'StevenBlack Porn',
     'https://raw.githubusercontent.com/StevenBlack/hosts/master/alternates/porn-only/hosts',
     0, '0 3 * * *'),
    ('00000000-0000-0000-0000-000000000111',
     '00000000-0000-0000-0000-000000000101',
     'OISD NSFW',
     'https://nsfw.oisd.nl/domainswild',
     0, '0 3 * * *'),
    ('00000000-0000-0000-0000-000000000120',
     '00000000-0000-0000-0000-000000000102',
     'HaGeZi Threat Intelligence Feeds',
     'https://raw.githubusercontent.com/hagezi/dns-blocklists/main/wildcard/tif.txt',
     0, '0 3 * * *'),
    ('00000000-0000-0000-0000-000000000121',
     '00000000-0000-0000-0000-000000000102',
     'URLhaus by abuse.ch',
     'https://urlhaus.abuse.ch/downloads/hostfile/',
     0, '0 3 * * *');
