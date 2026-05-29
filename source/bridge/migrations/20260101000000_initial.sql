-- Bridge service initial schema — MySQL.
--
-- installs        — one row per Pi registration; tracks the subdomain name,
--                   Ed25519 public key, bearer-token hash, Cloudflare record IDs.
-- registration_log — rate-limit table: one row per registration attempt,
--                   keyed on the caller's remote IP for the 3-per-day guard.

CREATE TABLE installs (
    id                  CHAR(36)    NOT NULL,
    name                VARCHAR(64) NOT NULL,
    public_key          VARCHAR(64) NOT NULL,
    token_hash          VARCHAR(64) NOT NULL,
    ip                  VARCHAR(45),
    cf_a_record_id      VARCHAR(64),
    cf_acme_record_id   VARCHAR(64),
    created_at          DATETIME(3) NOT NULL,
    updated_at          DATETIME(3) NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uq_installs_name       (name),
    UNIQUE KEY uq_installs_token_hash (token_hash)
);

CREATE TABLE registration_log (
    id          BIGINT      NOT NULL AUTO_INCREMENT,
    remote_ip   VARCHAR(45) NOT NULL,
    created_at  DATETIME(3) NOT NULL,
    PRIMARY KEY (id),
    INDEX idx_registration_log_ip_time (remote_ip, created_at)
);
