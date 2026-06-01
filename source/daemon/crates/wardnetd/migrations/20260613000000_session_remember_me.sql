-- Add a flag so refresh_session can only extend sessions that were
-- explicitly created as long-lived (remember_me = true).
ALTER TABLE sessions ADD COLUMN remember_me BOOLEAN NOT NULL DEFAULT 0;
