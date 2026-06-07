-- Bridge service initial schema — PostgreSQL.
--
-- installs        — one row per Pi registration; tracks the subdomain name,
--                   Ed25519 public key, bearer-token hash, Cloudflare record IDs.
-- registration_log — rate-limit table: one row per registration attempt,
--                   keyed on the caller's remote IP for the 3-per-day guard.

CREATE TABLE installs (
    id                  VARCHAR(36) NOT NULL,
    name                VARCHAR(64) NOT NULL,
    public_key          VARCHAR(64) NOT NULL,
    token_hash          VARCHAR(64) NOT NULL,
    ip                  VARCHAR(45),
    cf_a_record_id      VARCHAR(64),
    cf_acme_record_id   VARCHAR(64),
    created_at          TIMESTAMPTZ NOT NULL,
    updated_at          TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT uq_installs_name       UNIQUE (name),
    CONSTRAINT uq_installs_token_hash UNIQUE (token_hash)
);

CREATE TABLE registration_log (
    id          BIGINT      GENERATED ALWAYS AS IDENTITY,
    remote_ip   VARCHAR(45) NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (id)
);

CREATE INDEX idx_registration_log_ip_time ON registration_log (remote_ip, created_at);
