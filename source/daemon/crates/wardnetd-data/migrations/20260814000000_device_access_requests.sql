-- Generalize the "ask the admin" inbox from rule requests to **access
-- requests** (issue #919).
--
-- `device_rule_requests` was built for one question — block or allow a domain.
-- Private DNS needs the same request→approve loop with no domain at all, so
-- rather than stand up a second parallel inbox the table becomes generic: a
-- `kind` discriminator, and a `domain` that is only meaningful for the kinds
-- that name one.
--
-- Approval is no longer uniform. It dispatches through a per-kind approver
-- registry in `wardnetd-services`: `private_dns` mints the grant, while
-- `allow` / `block` remain record-only (the admin still applies the DNS filter
-- rule by hand — auto-applying it is #1197, because the rule lands on
-- a *profile* and choosing that profile has household-wide blast radius).
--
-- SQLite cannot add a CHECK constraint or relax a NOT NULL in place, so the
-- table is rebuilt and the rows copied. Existing `kind` values ('block' /
-- 'allow') are already correct under the new vocabulary, so the copy is a
-- straight move — no value rewriting, and every past decision is preserved.

CREATE TABLE IF NOT EXISTS device_access_requests (
    id          TEXT PRIMARY KEY,
    device_id   TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,                    -- 'block' | 'allow' | 'private_dns'
    domain      TEXT,                             -- NULL exactly for kinds naming no domain
    reason      TEXT,
    status      TEXT NOT NULL DEFAULT 'pending',  -- 'pending' | 'approved' | 'rejected'
    created_at  TEXT NOT NULL,
    decided_at  TEXT,
    decided_by  TEXT,
    -- The payload pairing, as a database fact rather than a service
    -- convention: a domain-naming kind must carry one, and a kind that names
    -- no domain must not. Mirrors `AccessRequestKind::requires_domain`.
    CHECK ((kind IN ('block', 'allow')) = (domain IS NOT NULL))
);

INSERT INTO device_access_requests
    (id, device_id, kind, domain, reason, status, created_at, decided_at, decided_by)
SELECT id, device_id, kind, domain, reason, status, created_at, decided_at, decided_by
FROM device_rule_requests;

DROP TABLE IF EXISTS device_rule_requests;

CREATE INDEX IF NOT EXISTS idx_access_requests_status ON device_access_requests(status);
CREATE INDEX IF NOT EXISTS idx_access_requests_device ON device_access_requests(device_id);

-- At most one *open* Private-DNS request per device, enforced by the database
-- rather than by a service remembering it already asked (the same reasoning as
-- the anomalies partial unique index).
--
-- Deliberately scoped to `kind = 'private_dns'`: a Private-DNS request carries
-- no payload, so a second open one is pure duplicate nagging, whereas a device
-- legitimately has several `allow` requests pending at once — one per domain.
CREATE UNIQUE INDEX IF NOT EXISTS idx_access_requests_open_private_dns
    ON device_access_requests(device_id)
    WHERE status = 'pending' AND kind = 'private_dns';
