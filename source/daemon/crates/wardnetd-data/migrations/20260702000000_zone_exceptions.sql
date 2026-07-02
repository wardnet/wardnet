-- Cross-zone exceptions (epic #244, issue #737).
--
-- An exception is an admin-granted allowance for one endpoint to reach another
-- across an otherwise-isolated zone boundary (e.g. a phone casting to a TV).
-- The endpoint references are soft/kind-tagged: `from_id` / `to_id` may point at
-- either a device or a network zone depending on the accompanying `*_kind`
-- column, so no SQL foreign key is declared — existence is validated in the
-- service layer. This migration is data-model only; the zone enforcer consumes
-- the table in a later commit.

CREATE TABLE IF NOT EXISTS zone_exceptions (
    id             TEXT PRIMARY KEY NOT NULL,
    from_kind      TEXT NOT NULL,   -- 'device' | 'zone'
    from_id        TEXT NOT NULL,
    to_kind        TEXT NOT NULL,
    to_id          TEXT NOT NULL,
    service_preset TEXT,            -- e.g. 'casting'; NULL when custom ports
    ports_json     TEXT,            -- JSON [{proto,from,to}]; NULL when preset
    bidirectional  INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
CREATE INDEX IF NOT EXISTS idx_zone_exceptions_from ON zone_exceptions(from_id);
CREATE INDEX IF NOT EXISTS idx_zone_exceptions_to   ON zone_exceptions(to_id);
