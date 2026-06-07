-- Global naming authority — PostgreSQL (separate global DB, distinct from each
-- bridge's per-region install DB).
--
-- names — one row per vanity slug in the flat global namespace. The `slug`
--         PRIMARY KEY is the cross-region allocation lock: an INSERT that hits
--         it (SQLSTATE 23505) means the name is taken. Registration is
--         two-phase — a row is created `reserved` with a TTL (`expires_at`),
--         then `confirm`ed to `active` (expiry cleared). A region-scoped sweep
--         reaps expired `reserved` rows so a crashed registration never leaks a
--         name. `install_id` links back to the install row in the regional DB
--         (cleaned alongside the names row on release/sweep — the saga spans
--         both databases). DNS resolution and TLS are handled elsewhere (infra
--         wildcard + daemon-issued cert); this table is allocation only.

CREATE TABLE names (
    slug        VARCHAR(64)  PRIMARY KEY,
    install_id  VARCHAR(36)  NOT NULL,
    region      VARCHAR(32)  NOT NULL,
    status      VARCHAR(16)  NOT NULL CHECK (status IN ('reserved', 'active')),
    expires_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ  NOT NULL
);

-- Supports the region-scoped sweep: WHERE status='reserved' AND region=$ AND expires_at<$.
CREATE INDEX idx_names_sweep ON names (status, region, expires_at);
