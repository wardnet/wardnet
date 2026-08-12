-- Anomalies: typed admin-facing conditions with an open/resolved lifecycle.
--
-- This replaces the in-memory ring buffer that backed the dashboard's
-- "recent errors" panel. That buffer could only ever answer "what went wrong
-- recently", which is the wrong question for anything that alerts. A blocklist
-- that has been failing to refresh for three days is one condition, not one
-- event per retry, and an admin should be told once — not every six hours,
-- and not again after a daemon restart.
--
-- The lifecycle is what makes that work. An anomaly opens when a condition is
-- first observed, is *touched* (last_seen_at, occurrences) on every subsequent
-- observation, and resolves when its detector says the condition no longer
-- holds. Notification happens on the open and on the resolve, never in
-- between.
--
-- The partial unique index below is the mechanism, not an optimisation: it
-- permits at most one OPEN anomaly per (type, subject), so a second attempt to
-- open one collides at the database rather than relying on the service to
-- remember it already alerted. That is what survives a restart, a lost write,
-- or two detectors racing. Resolved rows are deliberately outside the index so
-- the same condition can recur later and alert again.
--
-- `subject_id` is the entity the anomaly is about (a tunnel id, a blocklist
-- id) or NULL for box-wide conditions. It is also how per-entity error
-- surfacing works: an open anomaly whose subject is a tunnel IS that tunnel's
-- current error, which is why no entity carries its own `last_error` column.
-- COALESCE in the index is required because NULLs do not compare equal in a
-- unique index, so box-wide anomalies would otherwise never deduplicate.
--
-- `notified_at` gates the recovery notice: "X is working again" must not fire
-- unless "X is broken" fired first, which is possible whenever the threshold
-- changes or an anomaly opens while notification is failing.
CREATE TABLE IF NOT EXISTS anomalies (
    id            TEXT PRIMARY KEY NOT NULL,
    anomaly_type  TEXT NOT NULL,
    subject_id    TEXT,
    message       TEXT NOT NULL,
    details       TEXT,
    opened_at     TEXT NOT NULL,
    last_seen_at  TEXT NOT NULL,
    occurrences   INTEGER NOT NULL DEFAULT 1,
    resolved_at   TEXT,
    notified_at   TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_anomalies_open_key
    ON anomalies (anomaly_type, COALESCE(subject_id, ''))
    WHERE resolved_at IS NULL;

-- Queried on every tunnel card render ("what is wrong with this tunnel").
CREATE INDEX IF NOT EXISTS idx_anomalies_subject ON anomalies (subject_id);

-- The dashboard lists newest-first; retention prunes oldest-resolved-first.
CREATE INDEX IF NOT EXISTS idx_anomalies_opened_at ON anomalies (opened_at);
CREATE INDEX IF NOT EXISTS idx_anomalies_resolved_at ON anomalies (resolved_at);
