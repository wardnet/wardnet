-- Single-use PoW challenges gating POST /v1/register — PostgreSQL.
--
-- Each challenge has a 5-minute TTL. The bridge burns it atomically on first
-- use so two concurrent registration attempts with the same challenge_id both
-- fail (only one UPDATE WHERE used_at IS NULL wins).

CREATE TABLE registration_challenges (
    id          VARCHAR(36)  NOT NULL,
    nonce       VARCHAR(64)  NOT NULL,
    difficulty  INTEGER      NOT NULL,
    remote_ip   VARCHAR(45)  NOT NULL,
    created_at  TIMESTAMPTZ  NOT NULL,
    expires_at  TIMESTAMPTZ  NOT NULL,
    used_at     TIMESTAMPTZ,
    PRIMARY KEY (id)
);

CREATE INDEX idx_challenges_ip_time    ON registration_challenges (remote_ip, created_at);
CREATE INDEX idx_challenges_expires_at ON registration_challenges (expires_at);
