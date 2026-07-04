-- Admin notification feed (issue #482). One row per admin-audience push
-- notification, written by the push service before fan-out — the feed records
-- "what happened", not "what was delivered", so rows exist even when no admin
-- subscription does.
--
-- Device-keyed pushes (routing locked/unlocked, admin-changed-your-routing)
-- are deliberately NOT persisted: they are personal to one device's browser
-- and no device-facing feed exists (user-app notifications are issue #594).
--
-- `subject_id` identifies the entity the notification is about; what it
-- identifies is driven by `kind` (device UUID for device kinds, tunnel UUID
-- for tunnel kinds). No FK for the same reason as push_subscriptions: the
-- subject may be forgotten/re-discovered and the feed row should survive.
--
-- Retention is a count cap enforced on insert (see NotificationRepository),
-- so the table needs no background cleanup job.
CREATE TABLE IF NOT EXISTS notifications (
    id          TEXT PRIMARY KEY,            -- UUID
    kind        TEXT NOT NULL,               -- 'new_device_quarantined' | 'tunnel_offline' | ...
    title       TEXT NOT NULL,
    body        TEXT NOT NULL,
    url         TEXT,                        -- app-relative deep link, e.g. '/devices'
    subject_id  TEXT,                        -- kind-driven subject entity id
    created_at  TEXT NOT NULL                -- RFC 3339 (lexicographically sortable)
);

CREATE INDEX IF NOT EXISTS idx_notifications_created_at
    ON notifications (created_at);
