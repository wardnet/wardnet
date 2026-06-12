-- Add a provenance discriminator to custom DNS records so DHCP-created
-- A records (`{hostname}.lan`) are distinguishable from admin-created
-- ones. Existing rows are admin-created, hence the 'manual' default.
ALTER TABLE dns_custom_records
    ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';
