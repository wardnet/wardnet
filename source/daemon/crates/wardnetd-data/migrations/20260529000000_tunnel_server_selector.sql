ALTER TABLE tunnels ADD COLUMN server_selector_country TEXT NULL DEFAULT NULL;
ALTER TABLE tunnels ADD COLUMN resolved_server_name    TEXT NULL DEFAULT NULL;
ALTER TABLE tunnels ADD COLUMN endpoint_resolved_at    TEXT NULL DEFAULT NULL;
