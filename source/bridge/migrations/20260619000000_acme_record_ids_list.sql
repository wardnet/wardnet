-- Per-user wildcard certificate (#540): an install's ACME DNS-01 challenge now
-- carries TWO TXT values at the one `_acme-challenge.<name>` name — the apex and
-- the wildcard SAN authorize through the same name simultaneously. The bridge
-- therefore tracks a LIST of Cloudflare record IDs rather than a single one.
--
-- Native `TEXT[]` (not JSON): maps directly to Rust `Vec<String>` via sqlx, needs
-- no serde round-trip, and stays queryable. `NOT NULL DEFAULT '{}'` so "no live
-- challenge" is the empty array — no nullable distinction to handle.
--
-- Regional migration (alters `installs`, which lives in the regional Postgres) —
-- belongs here, NOT in `migrations-global/`.

ALTER TABLE installs DROP COLUMN cf_acme_record_id;
ALTER TABLE installs ADD COLUMN cf_acme_record_ids TEXT[] NOT NULL DEFAULT '{}';
