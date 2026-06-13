-- Lightweight "ask the admin" inbox: a household user (device, by IP) can
-- request that a domain be blocked or allowed. Admins view pending requests
-- and approve/reject them. Approval records the decision only — the admin
-- applies the actual DNS filter rule via the existing filter UI (auto-apply
-- is a deferred follow-up).
CREATE TABLE IF NOT EXISTS device_rule_requests (
    id          TEXT PRIMARY KEY,
    device_id   TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,                    -- 'block' | 'allow'
    domain      TEXT NOT NULL,
    reason      TEXT,
    status      TEXT NOT NULL DEFAULT 'pending',  -- 'pending' | 'approved' | 'rejected'
    created_at  TEXT NOT NULL,
    decided_at  TEXT,
    decided_by  TEXT
);

CREATE INDEX IF NOT EXISTS idx_rule_requests_status ON device_rule_requests(status);
CREATE INDEX IF NOT EXISTS idx_rule_requests_device ON device_rule_requests(device_id);
