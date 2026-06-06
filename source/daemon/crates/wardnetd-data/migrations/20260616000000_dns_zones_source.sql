-- Add a provenance discriminator to authoritative local DNS zones so the
-- daemon-seeded `.lan` zone is distinguishable from admin-created ones and can
-- be protected from deletion. Existing rows are admin-created, hence the
-- 'manual' default; the seeded `.lan` zone is then promoted to 'system'.
ALTER TABLE dns_zones
    ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';

UPDATE dns_zones
    SET source = 'system'
    WHERE id = '00000000-0000-0000-0000-000000000010';
