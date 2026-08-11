-- Household identity (#1147, ADR-0031).
--
-- Replaces the single-`admins` model with a household user directory:
-- `users` + uniform `user_credentials` rows (password / passkey / google /
-- github) + admin-issued one-time `user_enrolment_tokens`.
--
-- Ordering matters and a mistake here is unbootable, so the steps below run
-- strictly in order. The dangerous one is step 3: SQLite cannot alter a
-- foreign key in place, so `sessions` must be rebuilt. Step 2 preserves each
-- admin's `id` when backfilling it into `users`, which turns that rebuild into
-- a straight `admin_id` -> `user_id` column rename and lets **live sessions
-- survive the upgrade** rather than logging the whole household out.

-- ---------------------------------------------------------------------------
-- 1. The new tables.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS users (
    id           TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    email        TEXT,
    role         TEXT NOT NULL CHECK (role IN ('admin', 'member')),
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- SQLite's TEXT UNIQUE is case-sensitive, so a plain UNIQUE on `email` would
-- happily admit both `Ann@example.com` and `ann@example.com` as two people.
-- Partial, because several users may legitimately have no email at all.
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email_lower
    ON users(lower(email)) WHERE email IS NOT NULL;

CREATE TABLE IF NOT EXISTS user_credentials (
    id           TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL CHECK (kind IN ('password', 'google', 'github', 'passkey')),
    -- The login identifier the flow presents: username for a backfilled local
    -- admin, email for a new local user, Google `sub`, GitHub **numeric id**
    -- (never the login — GitHub logins are renameable *and* reusable, so a
    -- login-keyed credential is an account-takeover primitive), or the
    -- base64url passkey credential id.
    subject      TEXT NOT NULL,
    -- Argon2id PHC string, or the passkey COSE public key. Must never leave
    -- the repository layer: listing methods return a `CredentialSummary` row
    -- type that has no such field.
    secret       TEXT,
    label        TEXT,
    metadata     TEXT NOT NULL DEFAULT '{}',
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_used_at TEXT,
    -- Simultaneously the login lookup key and the anti-hijack invariant: two
    -- users must never be able to link the same Google/GitHub account, or a
    -- login resolves to whichever row the query happened to return. Enforced
    -- here rather than in a service so no code path can bypass it.
    UNIQUE (kind, subject)
);

CREATE INDEX IF NOT EXISTS idx_user_credentials_user_id ON user_credentials(user_id);

-- One password per user, as a database fact. A second password row is not a
-- lesser credential, it is a parallel one.
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_credentials_one_password
    ON user_credentials(user_id) WHERE kind = 'password';

CREATE TABLE IF NOT EXISTS user_enrolment_tokens (
    id         TEXT PRIMARY KEY NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Hashed at rest: a readable enrolment token in a database backup is a
    -- standing invitation to become a household member.
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    expires_at TEXT NOT NULL,
    used_at    TEXT
);

CREATE INDEX IF NOT EXISTS idx_user_enrolment_tokens_user_id    ON user_enrolment_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_user_enrolment_tokens_expires_at ON user_enrolment_tokens(expires_at);

-- ---------------------------------------------------------------------------
-- 2. Backfill `admins` into `users`, preserving `id`.
--
--    The nil UUID is excluded explicitly. It is the reserved system actor
--    (`AuthContext::system()`), and a real `users` row bearing it would make
--    background work indistinguishable from a person in the audit log — the
--    one thing the nil-UUID convention exists to prevent.
-- ---------------------------------------------------------------------------

INSERT INTO users (id, display_name, email, role, enabled, created_at, updated_at)
SELECT id, username, NULL, 'admin', 1, created_at, created_at
FROM admins
WHERE id != '00000000-0000-0000-0000-000000000000';

-- The existing Argon2id hash becomes a `password` credential.
--
-- `subject` is normally the **lowercased** username, so the break-glass login
-- works case-insensitively from here on. But `admins.username` was UNIQUE
-- *case-sensitively*, so a box holding both `Ann` and `ann` would collapse two
-- distinct rows onto one `(kind, subject)` and abort the migration — and an
-- aborted migration here means a daemon that will not start and cannot roll
-- back. Rare (the wizard creates one admin) is not the same as impossible, and
-- the cost of being wrong is unbootable.
--
-- Every subject is therefore lowercased unconditionally, because the login path
-- lowercases what the operator types and `find_for_login` matches the column
-- exactly. A subject preserved in its original casing would be **unreachable**:
-- no input could ever match it, and that admin would be locked out with no
-- recovery. (An earlier draft of this migration did exactly that, on the theory
-- that colliding admins would "keep logging in case-sensitively" — they cannot;
-- the case-sensitivity lived in the old `find_by_username`, which is gone.)
--
-- Two usernames differing only in case are, under the new scheme, one login.
-- That is a genuine data conflict, so within a colliding group only the
-- **oldest** admin gets the password credential. The others keep their `users`
-- row, their id and their `admin` role — nothing is deleted, and a household is
-- never left with fewer admins than it had — but they arrive with no password
-- and are re-enrolled by an admin (`issue_enrolment` accepts exactly the
-- credential-less case). That beats an abort, which would be unbootable, and it
-- beats a row nobody can ever authenticate against.
INSERT INTO user_credentials (id, user_id, kind, subject, secret, label, metadata, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4'
      || substr(lower(hex(randomblob(2))), 2) || '-a'
      || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6))),
    a.id,
    'password',
    lower(a.username),
    a.password_hash,
    'Local admin password',
    '{}',
    a.created_at
FROM admins a
WHERE a.id != '00000000-0000-0000-0000-000000000000'
  -- The canonical member of this username's case-insensitive group. Ordered by
  -- `created_at` then `id` so the choice is deterministic when timestamps tie.
  -- For the overwhelmingly common case — one admin, no collision — this
  -- subquery selects that admin and the clause is a no-op.
  AND a.id = (
      SELECT b.id
      FROM admins b
      WHERE lower(b.username) = lower(a.username)
        AND b.id != '00000000-0000-0000-0000-000000000000'
      ORDER BY b.created_at, b.id
      LIMIT 1
  );

-- ---------------------------------------------------------------------------
-- 3. Rebuild `sessions` so it references `users` instead of `admins`.
--
--    `defer_foreign_keys` (not `foreign_keys`) is the correct pragma here:
--    sqlx runs each migration inside a transaction, where `PRAGMA
--    foreign_keys` is silently a no-op, while `defer_foreign_keys` is honoured
--    and resets itself at COMMIT. In practice every copied row already
--    satisfies the new FK — step 2 preserved the ids — so this only guards the
--    window between DROP and RENAME.
-- ---------------------------------------------------------------------------

PRAGMA defer_foreign_keys = ON;

CREATE TABLE sessions_new (
    id                  TEXT PRIMARY KEY NOT NULL,
    user_id             TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash          TEXT NOT NULL UNIQUE,
    created_at          TEXT NOT NULL,
    expires_at          TEXT NOT NULL,
    -- Carried over from 20260613000000_session_remember_me.
    remember_me         INTEGER NOT NULL DEFAULT 0,
    -- Which device the session was issued to, for per-session revocation in
    -- the UI. Nullable: an API-key or `wctl` session has no device.
    device_id           TEXT REFERENCES devices(id) ON DELETE SET NULL,
    user_agent          TEXT,
    -- The hard ceiling a refresh can never push past. Replaces re-deriving
    -- MAX_SESSION_DAYS from `created_at` on every single refresh.
    absolute_expires_at TEXT NOT NULL
);

INSERT INTO sessions_new
    (id, user_id, token_hash, created_at, expires_at, remember_me,
     device_id, user_agent, absolute_expires_at)
SELECT
    s.id,
    s.admin_id,
    s.token_hash,
    s.created_at,
    s.expires_at,
    s.remember_me,
    NULL,
    NULL,
    -- The ceiling is `created_at + 90 days`, which is **exactly** the rule these
    -- sessions were already living under: the pre-migration `refresh_session`
    -- recomputed `created_at + MAX_SESSION_DAYS` on every call. So this
    -- reproduces the old policy rather than inventing a longer life.
    --
    -- Using `s.expires_at` here instead — an earlier draft did — silently breaks
    -- the sliding window for every migrated session: `refresh_session` takes
    -- `min(slid_expiry, absolute_expiry)`, so a ceiling equal to the current
    -- expiry pins the expiry where it is and refresh can never move it again.
    -- The session then dies at its original expiry with no warning.
    --
    -- `strftime` with an explicit `+00:00`, not `datetime()`: every other
    -- timestamp in this schema is RFC 3339 and the comparisons against them are
    -- lexicographic string compares. `datetime()` returns
    -- `YYYY-MM-DD HH:MM:SS` (space, no offset), which would sort wrongly against
    -- `chrono`'s `to_rfc3339()` output for the rest of the table.
    --
    -- `max(...)` so a session somehow already past that ceiling is never
    -- *shortened* by the upgrade — nobody gets logged out mid-migration.
    max(
        s.expires_at,
        strftime('%Y-%m-%dT%H:%M:%S+00:00', s.created_at, '+90 days')
    )
FROM sessions s
-- Drop any session whose admin was the excluded nil UUID; it has no `users`
-- row to point at, and honouring the FK matters more than one impossible row.
WHERE s.admin_id IN (SELECT id FROM users);

DROP TABLE sessions;
ALTER TABLE sessions_new RENAME TO sessions;

CREATE INDEX IF NOT EXISTS idx_sessions_token_hash ON sessions(token_hash);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_sessions_user_id    ON sessions(user_id);

-- ---------------------------------------------------------------------------
-- 4. Device affinity. Legal as a plain ALTER because it is nullable.
--
--    ON DELETE SET NULL, not CASCADE: deleting a person must not delete the
--    household's hardware. This column is attribution and never a credential
--    (ADR-0031 §4) — it has no path into an `AuthContext`.
-- ---------------------------------------------------------------------------

ALTER TABLE devices ADD COLUMN owner_user_id TEXT REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_devices_owner_user_id ON devices(owner_user_id);

-- ---------------------------------------------------------------------------
-- 5. Push subscriptions follow the principal rename.
--
--    `push_subscriptions.owner_kind` knew only 'device' | 'admin', and for the
--    'admin' rows `owner_key` held the admin account UUID. Step 2 preserved
--    those ids into `users`, so this is a lossless rename of the discriminator
--    and NOT a data migration: every 'admin' row keeps pointing at the same
--    person.
--
--    Doing this here rather than leaving 'admin' as a legacy value is
--    deliberate. `caller_owner()` derives the pair from the live `AuthContext`,
--    which no longer has an `Admin` variant; if existing rows kept the old
--    discriminator, every upgraded box would silently lose its admin push
--    subscriptions — the rows would be present but unreachable, which is worse
--    than either keeping or deleting them.
--
--    There is no CHECK constraint on the column, so a plain UPDATE is enough.
-- ---------------------------------------------------------------------------

UPDATE push_subscriptions SET owner_kind = 'user' WHERE owner_kind = 'admin';

-- ---------------------------------------------------------------------------
-- 6. `admins` is now fully represented in `users` and nothing references it.
-- ---------------------------------------------------------------------------

DROP TABLE admins;
