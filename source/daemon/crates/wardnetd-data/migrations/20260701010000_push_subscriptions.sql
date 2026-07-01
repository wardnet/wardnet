-- Web Push subscriptions (issue #440). One row per browser subscription. The
-- daemon POSTs encrypted notifications to `endpoint` using the stored keys.
--
-- Ownership is a (kind, key) pair rather than a foreign key so both auth models
-- share one table:
--   owner_kind = 'device' -> owner_key = device MAC (the stable device key)
--   owner_kind = 'admin'  -> owner_key = admin account UUID (survives session
--                            rotation; admin-PWA notifications fan out to every
--                            'admin' row)
--
-- `endpoint` is UNIQUE so a browser re-subscribing (same push endpoint) upserts
-- its owner + keys instead of creating a duplicate. There is no FK on device
-- MAC: a forgotten-then-rediscovered device keeps the same MAC and its
-- subscription stays valid; stale rows are pruned on 404/410 from the push
-- service, not by cascade.
CREATE TABLE IF NOT EXISTS push_subscriptions (
    id          TEXT PRIMARY KEY,
    owner_kind  TEXT NOT NULL,               -- 'device' | 'admin'
    owner_key   TEXT NOT NULL,               -- device MAC | admin account UUID
    endpoint    TEXT NOT NULL UNIQUE,        -- push service URL
    p256dh      TEXT NOT NULL,               -- base64url subscriber ECDH pubkey
    auth        TEXT NOT NULL,               -- base64url auth secret
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_push_subscriptions_owner
    ON push_subscriptions (owner_kind, owner_key);
