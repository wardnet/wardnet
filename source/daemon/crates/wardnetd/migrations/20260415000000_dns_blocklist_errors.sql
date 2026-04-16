-- Milestone 1g Stage 2: surface blocklist refresh/parse failures in the UI.
-- Adds two columns to dns_blocklists for the most recent error message and
-- timestamp. Cleared on successful refresh.

ALTER TABLE dns_blocklists ADD COLUMN last_error    TEXT;
ALTER TABLE dns_blocklists ADD COLUMN last_error_at TEXT;
