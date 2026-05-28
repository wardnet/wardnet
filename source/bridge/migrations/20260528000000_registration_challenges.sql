-- Single-use PoW challenges gating POST /v1/register — MySQL.
--
-- Each challenge has a 5-minute TTL. The bridge burns it atomically on first
-- use so two concurrent registration attempts with the same challenge_id both
-- fail (only one UPDATE WHERE used_at IS NULL wins).

CREATE TABLE registration_challenges (
    id          CHAR(36)     NOT NULL,
    nonce       VARCHAR(64)  NOT NULL,
    difficulty  INT UNSIGNED NOT NULL,
    remote_ip   VARCHAR(45)  NOT NULL,
    created_at  DATETIME(3)  NOT NULL,
    expires_at  DATETIME(3)  NOT NULL,
    used_at     DATETIME(3),
    PRIMARY KEY (id),
    INDEX idx_challenges_ip_time    (remote_ip, created_at),
    INDEX idx_challenges_expires_at (expires_at)
);
