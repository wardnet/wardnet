-- Device identification signals (issue #1099).
--
-- Splits three facts that used to be crammed into the single `manufacturer`
-- column, and gives observed identification signals a home of their own.

-- 1. Whether the device presents a locally-administered (privacy/randomized)
--    MAC. This is how the device chose to present itself, NOT who built it.
--    It previously lived in `manufacturer` as the sentinel string
--    'Randomized MAC', which meant a privacy MAC and a real vendor name were
--    indistinguishable to every consumer of that column.
ALTER TABLE devices ADD COLUMN is_randomized INTEGER NOT NULL DEFAULT 0;

-- 2. Where the manufacturer name came from, which is what licenses the UI to
--    hedge. `ieee` is the registrant on record and can be stated as fact;
--    `catalog` is our own curated mapping for an OUI whose IEEE listing is
--    deliberately private, so it renders as "likely <vendor>"; `signal` was
--    inferred from something the device announced or answered. NULL means we
--    genuinely do not know.
ALTER TABLE devices ADD COLUMN manufacturer_source TEXT;

-- Backfill. Derive `is_randomized` from the address itself rather than from
-- the old 'Randomized MAC' sentinel: the sentinel is what the bug wrote, so
-- trusting it would inherit any row where it was absent or later overwritten.
-- The locally-administered bit is bit 1 of the first octet, i.e. the second hex
-- character of the stored lowercase `aa:bb:cc:...` form.
UPDATE devices
SET is_randomized = 1
WHERE substr(mac, 2, 1) IN ('2', '3', '6', '7', 'a', 'b', 'e', 'f');

-- Placeholder strings that identify nothing. 'Private' is the IEEE token for a
-- registrant who paid to hide their name; these rows are now dropped from the
-- generated OUI table entirely, so a rediscovery would yield NULL anyway — this
-- brings already-discovered devices in line without waiting for them to churn.
UPDATE devices
SET manufacturer = NULL
WHERE manufacturer IN ('Randomized MAC', 'Private', 'IEEE Registration Authority', '');

-- Whatever name survived came from the IEEE table, by definition.
UPDATE devices SET manufacturer_source = 'ieee' WHERE manufacturer IS NOT NULL;

-- 3. Observed identification signals. Deliberately multi-valued: a device can
--    announce several mDNS services, and each signal kind is an independent
--    piece of evidence rather than a field to be overwritten.
--
--    `kind` is one of: dhcp_hostname | dhcp_param_list | dhcp_vendor_class
--                    | mdns_service  | probed_port
--    `confidence` is 'observed' (the device told us or answered us) or
--    'inferred' (we matched it against the curated vendor catalog).
CREATE TABLE IF NOT EXISTS device_signals (
    device_id   TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,
    value       TEXT NOT NULL,
    confidence  TEXT NOT NULL DEFAULT 'observed',
    observed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (device_id, kind, value)
);
CREATE INDEX IF NOT EXISTS idx_device_signals_device_id ON device_signals(device_id);
