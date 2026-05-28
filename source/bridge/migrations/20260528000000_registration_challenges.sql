-- Single-use PoW challenges gating POST /v1/register.
--
-- Each challenge has a 5-minute TTL. The bridge burns it atomically on first
-- use so two concurrent registration attempts with the same challenge_id both
-- fail (only one UPDATE WHERE used_at IS NULL wins).

CREATE TABLE registration_challenges (
    id          TEXT    PRIMARY KEY NOT NULL,   -- UUIDv4, returned to client
    nonce       TEXT    NOT NULL,               -- 32 random bytes as hex
    difficulty  INTEGER NOT NULL,               -- required leading zero bits in SHA256
    remote_ip   TEXT    NOT NULL,               -- for the per-IP challenge rate limit
    created_at  TEXT    NOT NULL,               -- ISO 8601 UTC
    expires_at  TEXT    NOT NULL,               -- ISO 8601 UTC (created_at + 5 min)
    used_at     TEXT                            -- NULL until consumed
);

-- Used by the rate-limit query:
--   SELECT COUNT(*) FROM registration_challenges
--   WHERE remote_ip = ? AND created_at > datetime('now', '-1 hour')
CREATE INDEX idx_challenges_ip_time
    ON registration_challenges (remote_ip, created_at);

-- Used by the expiry cleanup (future maintenance job):
CREATE INDEX idx_challenges_expires_at
    ON registration_challenges (expires_at);
