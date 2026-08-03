-- Consecutive failed refresh attempts for a blocklist, for exponential
-- backoff in the cron runner.
--
-- A failed download never advances `last_updated`, so the every-minute cron
-- tick keeps seeing the blocklist as due and retries it forever: a feed
-- returning errors was re-downloaded every 60s indefinitely. `last_error_at`
-- alone gives the time of the last failure but not how many have piled up,
-- which is what the backoff interval has to scale on.
--
-- Reset to 0 by any successful refresh (both `replace_blocklist_domains` and
-- `set_blocklist_error(None)`), incremented by `set_blocklist_error(Some)`.
ALTER TABLE dns_filter_blocklists
    ADD COLUMN consecutive_failures INTEGER NOT NULL DEFAULT 0;
