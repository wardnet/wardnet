CREATE INDEX IF NOT EXISTS idx_dns_events_device_sync_id
    ON dns_events(device_id, sync_state, id);
