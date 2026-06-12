-- Bridge-owned TLS (drop Caddy): the bridge terminates TLS for its own FQDN and
-- issues/renews that certificate itself via ACME HTTP-01. The cert material and
-- the multi-host coordination state live in the regional Postgres so the cert
-- survives restarts and can be shared across hosts behind the same FQDN.

-- Sealed certificate + ACME account material for the bridge's own FQDN.
-- `sealed_blob` is AES-256-GCM of {account_credentials, chain_pem, key_pem} under
-- the per-region ENCRYPTION_KEY; `nonce` is its 96-bit GCM nonce. `not_after` is
-- stored in plaintext so the renewal loop can decide "renew?" without decrypting.
-- `version` is bumped on every successful (re)issue and drives cross-host reload:
-- a host hot-swaps its in-memory cert when the DB version exceeds what it serves.
CREATE TABLE bridge_tls (
    fqdn        TEXT PRIMARY KEY,
    sealed_blob BYTEA       NOT NULL,
    nonce       BYTEA       NOT NULL,
    not_after   TIMESTAMPTZ NOT NULL,
    version     BIGINT      NOT NULL DEFAULT 1,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Live ACME HTTP-01 challenge tokens. The issuing host writes the token before
-- marking the challenge ready; ANY host's :8080 responder serves it (LE's
-- validation may land on a different host than the issuer). Rows are reaped on a
-- TTL by the sweep so a failed order cannot strand a token.
CREATE TABLE acme_http_challenge (
    token             TEXT PRIMARY KEY,
    key_authorization TEXT        NOT NULL,
    expires_at        TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_acme_http_challenge_expires_at ON acme_http_challenge (expires_at);

-- Lease-based issuance lock: a host claims issuance by winning a conditional
-- UPDATE (locked_until in the past or NULL). A lease — not pg_advisory_lock —
-- because an advisory session lock would pin a Neon connection for the whole
-- multi-second ACME round-trip, fighting the min_connections=0 pool rule.
CREATE TABLE bridge_tls_lease (
    fqdn         TEXT PRIMARY KEY,
    holder       TEXT,
    locked_until TIMESTAMPTZ
);
