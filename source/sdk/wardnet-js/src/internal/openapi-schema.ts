// AUTO-GENERATED — DO NOT EDIT.
//
// Internal type picture of the daemon's REST contract, generated from
// docs/openapi.json by `yarn generate` (scripts/generate-openapi.mjs).
//
// This module is private to @wardnet/js: nothing here is re-exported from
// the package entrypoint. The hand-authored services import these types to
// pin their request/response mapping to the wire, so a daemon-side field
// change breaks the build here instead of drifting silently.

export interface paths {
    "/api/auth/login": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Log in with username and password. On success, sets a `wardnet_session` cookie the browser replays on subsequent admin-gated requests. The JSON body also carries the raw `token` and `expires_in_seconds` so non-browser clients (scripts, the `wctl` CLI, third-party integrations) can replay the token via `Authorization: Bearer <token>` without a cookie jar. */
        post: operations["post_api_auth_login"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/auth/logout": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description End the current session. Deletes the session server-side so the token can never authenticate again, and clears the `wardnet_session` cookie in the response. Requires a valid session cookie or a `Authorization: Bearer <session token>` header; only the session that authenticated this request is affected. Callers authenticated with an API key get a 401 — API keys are not sessions and cannot be logged out here. */
        post: operations["post_api_auth_logout"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/auth/refresh": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Slide the current session's expiry forward by 30 days. Called by the admin-app on every open to implement the 'login once' sliding-window behaviour. Requires a valid session cookie or `Authorization: Bearer <token>` header. */
        post: operations["post_api_auth_refresh"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/backup/export": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Produce an encrypted bundle and stream it back as `application/octet-stream`. The passphrase is required again on restore; the daemon cannot recover a forgotten one. */
        post: operations["post_api_backup_export"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/backup/import/apply": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Commit a previously-previewed import. Renames live files to `.bak-<timestamp>` siblings, writes the bundle contents into place, and sets `backup_restart_pending` in `system_config`. The daemon requires a restart afterwards for the live database pool to pick up the new file. */
        post: operations["post_api_backup_import_apply"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/backup/import/preview": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Accept a multipart upload with `bundle` (binary file) + `passphrase` (text), decrypt it server-side, validate compatibility against the running daemon, and return a short-lived preview token the caller can pass to `/api/backup/import/apply`. Nothing on disk changes yet. */
        post: operations["post_api_backup_import_preview"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/backup/snapshots": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Enumerate `.bak-<timestamp>` siblings retained from prior restores. Snapshots are trimmed by a background runner after 24 hours. */
        get: operations["get_api_backup_snapshots"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/backup/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Return the current backup subsystem phase (idle, exporting, importing with a nested phase, or failed with a reason). */
        get: operations["get_api_backup_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/ddns": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** @description Disable remote access: remove this daemon from its network (wardnet) or delete the Cloudflare record (BYOD), drop the stored certificate, and revert :443 to the unprovisioned placeholder. Idempotent. Admin only. */
        delete: operations["delete_api_ddns"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/ddns/check": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Check whether a vanity slug is available. Returns `available: false` for malformed or reserved slugs too (no error). Requires a prior enroll. Admin only. */
        get: operations["get_api_ddns_check"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/ddns/cloudflare": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Configure the BYOD-Cloudflare provider for a domain the operator controls (token validated against the zone), then kick off certificate issuance in the background. Poll `GET /api/tls/status` for progress. Admin only. */
        post: operations["post_api_ddns_cloudflare"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/ddns/enroll": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Enroll this daemon against the one-time code: generate its cloud identity and bind it to the tenant. Step 2 of the wardnet flow; afterwards check slug availability and register a network. Admin only. */
        post: operations["post_api_ddns_enroll"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/ddns/enrollment-code": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Request a one-time enrollment code be emailed to the wardnet account. Step 1 of the wardnet remote-access flow; the operator then submits the emailed code to `POST /api/ddns/enroll`. Admin only. */
        post: operations["post_api_ddns_enrollment_code"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/ddns/register": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Register a network under the given vanity slug on the lowest-latency region, then kick off certificate issuance in the background. Requires a prior enroll. The response returns as soon as the identity is persisted; poll `GET /api/tls/status` for issuance progress. Admin only. */
        post: operations["post_api_ddns_register"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/ddns/resolution-check": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Check whether the public internet resolves the active FQDN to the IP the daemon last published. Queries fixed external resolvers (1.1.1.1 / 8.8.8.8) by IP, deliberately bypassing the local split-horizon override. `pending` means no public A record yet (normal right after registration). Admin only. */
        get: operations["get_api_ddns_resolution_check"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/ddns/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Report the current DDNS configuration (provider, active hostname, last-published IP). Admin only. */
        get: operations["get_api_ddns_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/devices": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every device the daemon has seen on the LAN, enriched with its current DHCP status (reservation, active lease, or external/unknown) and current routing target (a specific tunnel, direct, or null when the device follows the gateway default policy). Admin only. */
        get: operations["get_api_devices"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/devices/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Fetch a single device by ID, including its current routing rule and DHCP status. Admin only. */
        get: operations["get_api_devices_id"];
        /** @description Update mutable device properties: display name, device type, routing target, and the admin-locked flag. Each field is optional; only fields present in the request body are changed. Partial updates are best-effort — the mutations are not transactional across services, but they are ordered most-likely-to-fail first so a rejected routing rule leaves the name and admin-locked flag unchanged. Admin only. */
        put: operations["put_api_devices_id"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/devices/{id}/dns-capture": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Return current DNS capture settings and storage stats for a device. */
        get: operations["get_api_devices_id_dns_capture"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /** @description Update DNS capture settings for a device. Omitted fields are left unchanged. */
        patch: operations["patch_api_devices_id_dns_capture"];
        trace?: never;
    };
    "/api/devices/{id}/zone": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** @description Reassign a device to a different Network Zone (epic #244, issue #735). Membership is otherwise sticky (set once at discovery). Returns the updated device detail. Admin only. */
        put: operations["put_api_devices_id_zone"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/devices/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Identify the caller by source IP and return their device info, current routing rule, and the list of available tunnels they can route through. Used by the self-service "My Device" page, so no authentication is required — the daemon trusts the source IP of the incoming TCP connection. */
        get: operations["get_api_devices_me"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/devices/me/dns-capture": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /** @description Let the caller enable or disable DNS capture for their own device. The device is identified by source IP — no authentication is required (self-service by IP). Only the `enabled` flag is changed; retention caps (cap_count / cap_days) are admin-only and left untouched. */
        patch: operations["patch_api_devices_me_dns_capture"];
        trace?: never;
    };
    "/api/devices/me/dns-events/ack": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Acknowledge receipt of DNS events up to and including `up_to_id`. The daemon deletes all events with id ≤ up_to_id for this device. */
        post: operations["post_api_devices_me_dns_events_ack"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/devices/me/dns-events/stream": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description SSE stream of captured DNS events for this device. Reconnect with `Last-Event-ID` to resume from the last seen row; omit the header to replay all pending events. */
        get: operations["get_api_devices_me_dns_events_stream"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/devices/me/rule": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** @description Let the caller change their own routing rule (direct vs. a specific tunnel). The target device is identified by source IP. Admin-locked devices are rejected with 403 — an admin must lift the lock first. No authentication is required (self-service by IP). */
        put: operations["put_api_devices_me_rule"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/devices/me/rule-requests": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List the rule requests made by this device (by source IP), newest first. */
        get: operations["get_api_devices_me_rule_requests"];
        put?: never;
        /** @description Submit a request to the admin to block or allow a domain. The device is identified by source IP — no authentication required. */
        post: operations["post_api_devices_me_rule_requests"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dhcp/config": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Return the current DHCP pool configuration (enabled flag, range, lease time, default options). Admin only. */
        get: operations["get_api_dhcp_config"];
        /** @description Update the DHCP pool configuration (range, lease time, default options). Does not toggle the enabled flag — use the /api/dhcp/config/toggle endpoint for that. Admin only. */
        put: operations["put_api_dhcp_config"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dhcp/config/preview": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Dry-run a pool-range change: report the active leases that would be revoked because their IP would fall outside the proposed pool (and are not pinned by a reservation). Mutates nothing — used to warn the admin before saving. Admin only. */
        post: operations["post_api_dhcp_config_preview"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dhcp/config/toggle": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Enable or disable the DHCP server. In addition to persisting the enabled flag, this starts or stops the live DHCP server process so the change takes effect immediately. Admin only. */
        post: operations["post_api_dhcp_config_toggle"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dhcp/leases": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every active DHCP lease currently tracked by the daemon, including MAC, IP, hostname, and expiry. Admin only. */
        get: operations["get_api_dhcp_leases"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dhcp/leases/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** @description Revoke an active DHCP lease by ID. The IP returns to the pool and the client will re-negotiate on its next renewal cycle. Admin only. */
        delete: operations["delete_api_dhcp_leases_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dhcp/reservations": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every configured static MAC-to-IP DHCP reservation. Reservations take precedence over dynamic leases. Admin only. */
        get: operations["get_api_dhcp_reservations"];
        put?: never;
        /** @description Create a new static MAC-to-IP DHCP reservation so a given device always receives the same IP. Admin only. */
        post: operations["post_api_dhcp_reservations"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dhcp/reservations/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** @description Delete a static DHCP reservation by ID. Any existing lease for the MAC is preserved until its natural expiry. Admin only. */
        delete: operations["delete_api_dhcp_reservations_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dhcp/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Get live DHCP server status and pool usage (running flag, total/used addresses, lease counts). Admin only. */
        get: operations["get_api_dhcp_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/cache/flush": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Flush the DNS resolver cache. Admin only. */
        post: operations["post_api_dns_cache_flush"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/config": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Return the current DNS server configuration. Admin only. */
        get: operations["get_api_dns_config"];
        /** @description Partially update DNS server configuration. Admin only. */
        put: operations["put_api_dns_config"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/config/toggle": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Enable or disable the DNS server. Admin only. */
        post: operations["post_api_dns_config_toggle"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/config": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Read the global DNS filter config (emergency stop + default profile pointer). Admin only. */
        get: operations["get_api_dns_filter_config"];
        /** @description Update the global DNS filter config. Admin only. */
        put: operations["put_api_dns_filter_config"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/devices": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List devices with explicit DNS filter settings or profile assignments. Use `?enabled=false` to filter to devices where the per-device kill switch is off. Admin only. */
        get: operations["get_api_dns_filter_devices"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/devices/{device_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Get a device's DNS filter settings. Returns the default (enabled, no explicit profiles) for devices that have never been configured. Admin only. */
        get: operations["get_api_dns_filter_devices_device_id"];
        /** @description Update a device's DNS filter settings (kill switch + profile assignments). Admin only. */
        put: operations["put_api_dns_filter_devices_device_id"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/profiles": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every DNS filter profile. Admin only. */
        get: operations["get_api_dns_filter_profiles"];
        put?: never;
        /** @description Create a new DNS filter profile. Admin only. */
        post: operations["post_api_dns_filter_profiles"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/profiles/{profile_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Fetch a single DNS filter profile. Admin only. */
        get: operations["get_api_dns_filter_profiles_profile_id"];
        /** @description Rename a DNS filter profile. Admin only. */
        put: operations["put_api_dns_filter_profiles_profile_id"];
        post?: never;
        /** @description Delete a non-builtin DNS filter profile. Returns 409 for builtin profiles (Ad Blocking, Parental Controls, Malware & Phishing). Admin only. */
        delete: operations["delete_api_dns_filter_profiles_profile_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/profiles/{profile_id}/allowlist": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List allowlist entries scoped to a profile. Admin only. */
        get: operations["get_api_dns_filter_profiles_profile_id_allowlist"];
        put?: never;
        /** @description Add a domain to the profile's allowlist. Admin only. */
        post: operations["post_api_dns_filter_profiles_profile_id_allowlist"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/profiles/{profile_id}/allowlist/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** @description Remove a domain from a profile's allowlist. Admin only. */
        delete: operations["delete_api_dns_filter_profiles_profile_id_allowlist_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/profiles/{profile_id}/blocklists": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every blocklist scoped to a profile. Admin only. */
        get: operations["get_api_dns_filter_profiles_profile_id_blocklists"];
        put?: never;
        /** @description Create a blocklist under a profile. Admin only. */
        post: operations["post_api_dns_filter_profiles_profile_id_blocklists"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/profiles/{profile_id}/blocklists/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** @description Update a blocklist within a profile. Admin only. */
        put: operations["put_api_dns_filter_profiles_profile_id_blocklists_id"];
        post?: never;
        /** @description Delete a blocklist from a profile. Admin only. */
        delete: operations["delete_api_dns_filter_profiles_profile_id_blocklists_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/profiles/{profile_id}/blocklists/{id}/refresh": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Trigger an immediate blocklist refresh job. Admin only. */
        post: operations["post_api_dns_filter_profiles_profile_id_blocklists_id_refresh"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/profiles/{profile_id}/rules": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List a profile's custom filter rules. Admin only. */
        get: operations["get_api_dns_filter_profiles_profile_id_rules"];
        put?: never;
        /** @description Add a custom filter rule to a profile. Admin only. */
        post: operations["post_api_dns_filter_profiles_profile_id_rules"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/filter/profiles/{profile_id}/rules/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** @description Update a custom filter rule. Admin only. */
        put: operations["put_api_dns_filter_profiles_profile_id_rules_id"];
        post?: never;
        /** @description Delete a custom filter rule from a profile. Admin only. */
        delete: operations["delete_api_dns_filter_profiles_profile_id_rules_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/local/forwarding": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every conditional-forwarding rule (domain → upstream). Admin only. */
        get: operations["get_api_dns_local_forwarding"];
        put?: never;
        /** @description Create a conditional-forwarding rule. Admin only. */
        post: operations["post_api_dns_local_forwarding"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/local/forwarding/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Fetch a single conditional-forwarding rule. Admin only. */
        get: operations["get_api_dns_local_forwarding_id"];
        /** @description Partially update a conditional-forwarding rule. Admin only. */
        put: operations["put_api_dns_local_forwarding_id"];
        post?: never;
        /** @description Delete a conditional-forwarding rule. Admin only. */
        delete: operations["delete_api_dns_local_forwarding_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/local/records": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every custom local DNS record. Admin only. */
        get: operations["get_api_dns_local_records"];
        put?: never;
        /** @description Create a custom local DNS record. Admin only. */
        post: operations["post_api_dns_local_records"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/local/records/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Fetch a single custom local DNS record. Admin only. */
        get: operations["get_api_dns_local_records_id"];
        /** @description Partially update a custom local DNS record. Admin only. */
        put: operations["put_api_dns_local_records_id"];
        post?: never;
        /** @description Delete a custom local DNS record. Admin only. */
        delete: operations["delete_api_dns_local_records_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/local/zones": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every authoritative local DNS zone. Admin only. */
        get: operations["get_api_dns_local_zones"];
        put?: never;
        /** @description Create an authoritative local DNS zone. Admin only. */
        post: operations["post_api_dns_local_zones"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/local/zones/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Fetch a single local DNS zone. Admin only. */
        get: operations["get_api_dns_local_zones_id"];
        /** @description Partially update a local DNS zone. Admin only. */
        put: operations["put_api_dns_local_zones_id"];
        post?: never;
        /** @description Delete a local DNS zone. Records keep their data but lose the zone link (set to NULL). Admin only. */
        delete: operations["delete_api_dns_local_zones_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/local/zones/{id}/records": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List the custom records assigned to a zone. Admin only. */
        get: operations["get_api_dns_local_zones_id_records"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/log": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Paginated DNS query log. Admin only. */
        get: operations["get_api_dns_log"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/dns/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Return live DNS server status and cache statistics. Admin only. */
        get: operations["get_api_dns_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/inbound-wg/config": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Read the current inbound WireGuard server config (enabled, listen port, public key) without mutating anything. Admin only. */
        get: operations["get_api_inbound_wg_config"];
        /** @description Enable or disable the inbound (multi-peer) WireGuard server and set its UDP listen port. On enable the daemon generates the server keypair (once), stands up the `wg_wardin0` interface, installs the NAT masquerade and listen-port accept firewall rules, and re-admits every enabled peer. On disable it removes those rules and tears the interface down; peer definitions are preserved. Admin only. */
        put: operations["put_api_inbound_wg_config"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/inbound-wg/peers": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every admitted inbound WireGuard peer with its name, public key, allocated address, and enabled state. Private keys are never returned. Admin only. */
        get: operations["get_api_inbound_wg_peers"];
        put?: never;
        /** @description Grant remote access to an already-managed device via inbound WireGuard. The daemon generates a fresh keypair, allocates the next free address on the inbound subnet, stores the peer (public key + device link), and admits it onto the server interface. The peer's name is taken from the device. The response carries the peer's **private key exactly once** — it is never persisted, so it must be copied now. Returns 409 if the server is disabled or the device already has a credential, 404 if the device does not exist. Admin only. */
        post: operations["post_api_inbound_wg_peers"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/inbound-wg/peers/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** @description Remove an inbound WireGuard peer by id: drop it from the live server interface and delete its stored definition. Admin only. */
        delete: operations["delete_api_inbound_wg_peers_id"];
        options?: never;
        head?: never;
        /** @description Pause or resume an inbound WireGuard peer without deleting its credential: re-admits it onto the live interface (enable) or best-effort removes it (disable), then persists the flag. Distinct from DELETE, which revokes the credential permanently — a paused peer can be resumed without a fresh keypair or QR scan. Admin only. */
        patch: operations["patch_api_inbound_wg_peers_id"];
        trace?: never;
    };
    "/api/info": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Return the daemon version string, uptime in seconds, and premium entitlement status. Used by the web UI connection-status widget to detect that the daemon is reachable, to display which build is running, and to self-gate the mobile PWAs when not entitled. No authentication required. */
        get: operations["get_api_info"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/jobs/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Poll the status of a background job previously dispatched by another endpoint (e.g. a blocklist refresh). Returns 404 when the job id is unknown — either never dispatched or garbage-collected after its retention TTL. Admin only. */
        get: operations["get_api_jobs_id"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/network/dhcp-self-probe": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Broadcast a DHCPDISCOVER from a synthetic MAC and report whether Wardnet (and any foreign DHCP server) responded with an OFFER. The wizard's primary-mode step 3 calls this after the operator says they've disabled DHCP on their router; a foreign-only response tells the UI to re-show the disable-DHCP guide. Admin only. */
        post: operations["post_api_network_dhcp_self_probe"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/network/discover-gateway-mac": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Discover (or accept) the upstream router MAC address. With an empty body, the daemon ARP-probes the LAN gateway from `GET /api/network/status` and returns the responder's MAC. Pass `mac` to skip the probe and record an operator-supplied value (validated). Pass `target_ip` to override the probe target. The result is persisted in `system_config` so subsequent calls are idempotent. Admin only. */
        post: operations["post_api_network_discover_gateway_mac"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/network/quarantine-new-devices": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Read the new-device quarantine toggle (issue #738). When on, a freshly-discovered device lands in the default-for-new zone (as always) and admins get a push to approve it. Off by default. Admin only. */
        get: operations["get_api_network_quarantine_new_devices"];
        /** @description Enable or disable new-device quarantine (issue #738). Placement is unchanged either way (new devices always land in the default-for-new zone); the toggle only controls whether admins are notified to approve. Admin only. */
        put: operations["put_api_network_quarantine_new_devices"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/network/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Read the LAN interface's current address + default gateway and classify whether the IP came from DHCP or a Wardnet-managed static config (install.sh --static-ip writes /etc/dhcpcd.conf.d/wardnet.conf, which flips dhcp_source to "static"). Powers the wizard's network step and the Settings page. Admin only. */
        get: operations["get_api_network_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/network/zones": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every Network Zone with its current member count. Admin only. */
        get: operations["get_api_network_zones"];
        put?: never;
        /** @description Create a Network Zone. `provenance` is forced to manual and default flags are never set on create (use PUT to promote). Admin only. */
        post: operations["post_api_network_zones"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/network/zones/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Fetch a single Network Zone with its member count. Admin only. */
        get: operations["get_api_network_zones_id"];
        /** @description Partially update a Network Zone. Setting `is_default` or `is_default_for_new` to true promotes this zone; false is rejected (flags only move by promoting another zone). System zones are editable but not deletable. Admin only. */
        put: operations["put_api_network_zones_id"];
        post?: never;
        /** @description Delete a Network Zone. Rejected for system zones, the anchor (default) zone, or zones that still have member devices. Admin only. */
        delete: operations["delete_api_network_zones_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/network/zones/exceptions": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every cross-zone exception. Admin only. */
        get: operations["get_api_network_zones_exceptions"];
        put?: never;
        /** @description Create a cross-zone exception. Both endpoints must exist and differ; a custom port list must be non-empty with valid ranges. Admin only. */
        post: operations["post_api_network_zones_exceptions"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/network/zones/exceptions/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Fetch a single cross-zone exception. Admin only. */
        get: operations["get_api_network_zones_exceptions_id"];
        /** @description Partially update a cross-zone exception. Every field is optional; the resolved exception is re-validated. Admin only. */
        put: operations["put_api_network_zones_exceptions_id"];
        post?: never;
        /** @description Delete a cross-zone exception. Admin only. */
        delete: operations["delete_api_network_zones_exceptions_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/private-dns": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** @description Enable or disable Private DNS. Enabling is Premium-gated and requires the hard prerequisites — the wardnet DNS provider, an issued TLS certificate, and an active entitlement — returning 409 when any is missing (reverse-tunnel readiness is informational and never blocks). On enable the daemon seeds the split-horizon wildcard record and starts the `:853` listener; on disable it removes the record and stops it. Returns the full status. Admin only. */
        put: operations["put_api_private_dns"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/private-dns/grants": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Grant a managed device encrypted-DNS access: mint its secret hostname (a high-entropy label under the wardnet wildcard, never in CT logs). Requires the feature to be enabled; returns 409 if the device already holds a grant or the feature is disabled, and 404 if the device is unknown. The response carries the full minted hostname. Admin only. */
        post: operations["post_api_private_dns_grants"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/private-dns/grants/{device_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** @description Revoke a device's encrypted-DNS grant by device id: delete its hostname and immediately cut off any established `DoT` session (the token is otherwise only checked at handshake). Returns 404 if the device has no grant. Admin only. */
        delete: operations["delete_api_private_dns_grants_device_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/private-dns/grants/{device_id}/notify": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Send a device-keyed push nudging the granted device's household member to set up Private DNS. The notification deep-links the user PWA to `/settings#private-dns`, where the Private DNS card carries the setup steps. Returns `delivered: true` when at least one push subscription for the device was targeted, and `delivered: false` (a 200, not an error) when the device holds a grant but has no subscription — the member simply hasn't enabled notifications yet. Returns 404 when the device has no grant. Admin only. */
        post: operations["post_api_private_dns_grants_device_id_notify"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/private-dns/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description The caller device's own Private DNS state, identified by source IP (like `GET /api/devices/me`): whether the feature is enabled, whether this device is granted, and its full hostname when granted. Backs the user PWA. No authentication required — the daemon trusts the source IP of the incoming TCP connection. */
        get: operations["get_api_private_dns_me"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/private-dns/me/profile": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description The signed iOS configuration profile (`.mobileconfig`) for the caller device, identified by source IP. Served as `application/x-apple-aspen-config` at a plain, navigable URL so Safari's install flow triggers on a real navigation. Returns 404 when this device isn't granted or the feature is disabled. No authentication required. */
        get: operations["get_api_private_dns_me_profile"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/private-dns/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Report Private DNS state for the admin UI: whether it is enabled, the wardnet domain grants live under, the enable prerequisites (wardnet provider, issued certificate, active entitlement, plus the informational reverse-tunnel readiness), and every per-device grant with its full minted hostname. Admin only. */
        get: operations["get_api_private_dns_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/providers": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every VPN provider the daemon knows how to integrate with (e.g. NordVPN), along with the capabilities each provider exposes. Admin only. */
        get: operations["get_api_providers"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/providers/{id}/countries": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List the countries in which a given VPN provider offers servers. Used by the setup wizard's country picker. Admin only. */
        get: operations["get_api_providers_id_countries"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/providers/{id}/servers": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description List the available VPN servers for a provider, optionally filtered by country. Credentials are supplied in the request body because some providers gate their server catalogue behind authentication. Admin only. */
        post: operations["post_api_providers_id_servers"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/providers/{id}/setup": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Run the full guided tunnel setup for a VPN provider end to end: validate credentials, generate a WireGuard key pair via the provider's API, materialise the resulting tunnel, and persist it. Admin only. */
        post: operations["post_api_providers_id_setup"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/providers/{id}/validate": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Validate a set of provider credentials by attempting a live auth call against the provider's API. Used by the guided setup wizard before persisting credentials. Admin only. */
        post: operations["post_api_providers_id_validate"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/push/notifications": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description The admin notification feed: recently pushed admin-audience notifications, newest first. Recorded regardless of whether any push subscription existed at the time. */
        get: operations["get_api_push_notifications"];
        put?: never;
        post?: never;
        /** @description Clear the admin notification feed. The feed is shared across admin accounts, so this clears it for every admin. */
        delete: operations["delete_api_push_notifications"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/push/subscriptions": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Register a Web Push subscription for the calling context. Admin callers (session cookie present) are keyed to their account; LAN devices are keyed to their MAC. Accessible to both — the caller must be a known device or an admin. */
        post: operations["post_api_push_subscriptions"];
        /** @description Remove the caller's push subscription(s). Used when the user revokes notification permission in the app. */
        delete: operations["delete_api_push_subscriptions"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/push/vapid-public-key": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description The VAPID application server public key (base64url). Unauthenticated — the browser needs it to build a push subscription before any identity exists. */
        get: operations["get_api_push_vapid_public_key"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/routing/devices/{device_id}/profiles": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List a device's assigned routing profiles, in priority order. Admin only. */
        get: operations["get_api_routing_devices_device_id_profiles"];
        /** @description Set a device's routing profiles in priority order (first = highest priority). Replaces the whole assignment. Admin only. */
        put: operations["put_api_routing_devices_device_id_profiles"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/routing/profiles": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every routing profile. Admin only. */
        get: operations["get_api_routing_profiles"];
        put?: never;
        /** @description Create a routing profile. Admin only. */
        post: operations["post_api_routing_profiles"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/routing/profiles/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Get a single routing profile. Admin only. */
        get: operations["get_api_routing_profiles_id"];
        /** @description Rename a routing profile. Admin only. */
        put: operations["put_api_routing_profiles_id"];
        post?: never;
        /** @description Delete a routing profile and its rules and assignments. Admin only. */
        delete: operations["delete_api_routing_profiles_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/routing/profiles/{id}/devices": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List the devices a routing profile is assigned to. Admin only. */
        get: operations["get_api_routing_profiles_id_devices"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/routing/profiles/{id}/rules": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List the domain rules in a routing profile. Admin only. */
        get: operations["get_api_routing_profiles_id_rules"];
        put?: never;
        /** @description Add a domain rule to a routing profile. Admin only. */
        post: operations["post_api_routing_profiles_id_rules"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/routing/rules/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** @description Update a domain routing rule. Admin only. */
        put: operations["put_api_routing_rules_id"];
        post?: never;
        /** @description Delete a domain routing rule. Admin only. */
        delete: operations["delete_api_routing_rules_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/rule-requests": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List all device rule requests, optionally filtered by status. Admin only. */
        get: operations["get_api_rule_requests"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/rule-requests/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /** @description Approve or reject a rule request. Recording a decision does not apply the DNS rule — the admin applies it via the DNS filter UI. Admin only. */
        patch: operations["patch_api_rule_requests_id"];
        trace?: never;
    };
    "/api/setup": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Create the first admin account as part of the initial setup wizard. Password is hashed with Argon2id before persistence. No authentication required (because by definition no admin exists yet), but returns 409 Conflict if setup has already been completed to prevent this endpoint from being used as an admin-creation backdoor. */
        post: operations["post_api_setup"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/setup/advance": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Advance the setup wizard to a new step. Admin-authenticated — the web UI auto-logs the operator in immediately after `POST /api/setup` and uses normal admin auth from then on. Forward transitions are always allowed; rewinds are allowed down to `network` (never back to `admin`), and `completed` is terminal. `wizard_mode` is recorded when the operator picks the DHCP onboarding branch at step 3. */
        post: operations["post_api_setup_advance"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/setup/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Report the current setup-wizard state. `setup_completed` is derived from `wizard_step == "completed"`. No authentication required — the web UI calls this before rendering to decide whether to redirect to the wizard. */
        get: operations["get_api_setup_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/stats": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Query a time-series metric. Pass either `metric=<name>` for a single series, or repeat `metrics=<name>` to fetch multiple series in one round-trip — both share the same time range, bucket, and label filter. Admin only. */
        get: operations["get_api_stats"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/stats/top": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Return the top-N label values for a metric. Admin only. */
        get: operations["get_api_stats_top"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/system/default-policy": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Read the global default routing policy. Returns either the literal string "direct" or a tunnel UUID. Used by the setup wizard's policy step and the post-install settings page so the operator can see the active default. Admin only. */
        get: operations["get_api_system_default_policy"];
        /** @description Set the global default routing policy. Body `policy` must be either the literal string "direct" or a tunnel UUID. The change is persisted in `system_config` and applied in-memory immediately — devices whose stored rule is `Default` pick up the new policy on their next apply or reconcile. Admin only. */
        put: operations["put_api_system_default_policy"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/system/errors": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Return the most recent admin-facing diagnostics: typed error conditions raised by daemon components, each with a message and a remediation hint. Fed by the domain event bus rather than by scraping log lines. Powers the dashboard's "recent errors" panel. Admin only. */
        get: operations["get_api_system_errors"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/system/logs/download": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Download the full daemon log file as a plain-text attachment. Served with Content-Disposition: attachment so browsers prompt to save. Admin only. */
        get: operations["get_api_system_logs_download"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/system/reboot": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Reboot the host (Pi-level), not just the daemon.
         * @description Reboot the host. Returns immediately; the reboot itself is queued via systemd-logind and runs once the response has flushed. The daemon will be unreachable for as long as the host takes to come back up — typically 30–60 s on a Raspberry Pi.
         */
        post: operations["post_api_system_reboot"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/system/restart": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Ask the daemon to exit so the supervisor restarts it.
         * @description Ask the daemon to exit cleanly so the supervisor (systemd on a Pi install) restarts it. The response returns before the process exits; the daemon will be unreachable for a few seconds while it comes back up.
         */
        post: operations["post_api_system_restart"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/system/shutdown": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Power off the host. Internet is unavailable until the operator
         *     physically powers the Pi back on.
         * @description Power the host off. Returns immediately; the poweroff itself is queued via systemd-logind and runs once the response has flushed. The daemon will not come back up until the operator turns the Pi on again.
         */
        post: operations["post_api_system_shutdown"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/system/shutdown/acknowledge": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Dismiss the dashboard's unclean-shutdown banner.
         * @description Acknowledge the most recent unclean-shutdown event so the dashboard banner is dismissed. Idempotent — repeated calls just refresh the timestamp. The banner reappears automatically on the next unclean shutdown. Admin only.
         */
        post: operations["post_api_system_shutdown_acknowledge"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/system/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Return an aggregate snapshot of system state: daemon version, uptime, host CPU and memory usage, and counts of tracked resources (devices, tunnels, leases). Admin only. */
        get: operations["get_api_system_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/tls/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Report the certificate's coarse provisioning phase (idle/issuing/issued/failed) with the active domain, expiry, and last error. Admin only. */
        get: operations["get_api_tls_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/tunnels": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List every configured VPN tunnel with its label, country, peer endpoint summary, and current runtime status. Admin only. */
        get: operations["get_api_tunnels"];
        put?: never;
        /** @description Import a tunnel from a WireGuard `.conf` file. The daemon parses the config, persists the tunnel definition, and stores the private key in the key store. Admin only. */
        post: operations["post_api_tunnels"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/tunnels/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Fetch a single tunnel by ID with its label, country, peer endpoint summary, runtime status, and live byte counters. Admin only. */
        get: operations["get_api_tunnels_id"];
        put?: never;
        post?: never;
        /** @description Delete a tunnel by ID: tear down the live WireGuard interface if it is up, remove the persisted config, and delete the associated private key from the key store. Admin only. */
        delete: operations["delete_api_tunnels_id"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/tunnels/{id}/devices": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List the devices currently routed through this tunnel — i.e. those whose routing rule has `target = Tunnel { tunnel_id: <id> }`. Admin only. */
        get: operations["get_api_tunnels_id_devices"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/tunnels/{id}/dns-override": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** @description Toggle the per-tunnel `override_default_dns` flag. When true, tunneled-device DNS queries are filtered by wardnet's ad-blocking pipeline and forwarded via a SO_BINDTODEVICE-bound socket on the tunnel interface. When false, those queries fall back to the system-wide upstream pool. Replaces the legacy nftables prerouting DNAT bypass — see issue #342. Admin only. */
        put: operations["put_api_tunnels_id_dns_override"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/tunnels/{id}/rebuild": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Tear down the WireGuard interface and re-apply it from the persisted config. Use when a tunnel is stale or devices cannot route through it. Equivalent to the internal watchdog recovery path. Admin only. */
        post: operations["post_api_tunnels_id_rebuild"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/tunnels/{id}/speed-test": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Start a speed test for this tunnel. Returns immediately with a job id (202); the background job measures throughput, latency and jitter twice — once over the direct (WAN) path and once through the tunnel — so the result shows how much of the line the VPN preserves. If the tunnel is down it is brought up for the run and torn back down after. Poll `GET /api/jobs/{job_id}` for progress. A concurrent run (or an overlapping test) on the same tunnel returns 409. Admin only. */
        post: operations["post_api_tunnels_id_speed_test"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/tunnels/{id}/speed-test/results": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List recent speed test results for this tunnel, newest first (last few runs). Each result carries both the direct (WAN) and tunnel measurements for throughput, latency and jitter. Admin only. */
        get: operations["get_api_tunnels_id_speed_test_results"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/tunnels/{id}/test": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Run an exit-IP / country probe through this tunnel. The daemon brings the tunnel up if needed, waits for a fresh handshake, sends one HTTP probe through the tunnel interface, and reports the exit IP, ISO-3166 alpha-2 country code, and probe latency. Restores the prior up/down state when finished. Admin only. */
        post: operations["post_api_tunnels_id_test"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/update/check": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Force an immediate refresh of the update manifest from the release server, bypassing the normal polling interval, and return whether a newer version is available on the configured channel. Admin only. */
        post: operations["post_api_update_check"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/update/config": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** @description Update auto-update settings: enable or disable automatic installs and switch between release channels (e.g. stable vs. beta). Admin only. */
        put: operations["put_api_update_config"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/update/history": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description List recent install and rollback attempts, including timestamp, source and target versions, outcome, and any error message. The `limit` query parameter caps the number of entries returned (default 20). Admin only. */
        get: operations["get_api_update_history"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/update/install": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Kick off an install of an available release. If the request body is omitted, the latest release on the configured channel is installed. The install runs asynchronously; poll /api/update/status for progress. Admin only. */
        post: operations["post_api_update_install"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/update/rollback": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** @description Roll back to the previous binary that was swapped aside as `<live>.old` during the last successful install. Used to recover from a bad release without manual SSH intervention. Admin only. */
        post: operations["post_api_update_rollback"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/update/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Return the current auto-update state: currently running version, latest known release from the manifest, configured channel, and whether an install is in progress. Admin only. */
        get: operations["get_api_update_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/users/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Return the authenticated admin's identity. Used by the web UI (e.g. the setup wizard's review step) to display the account name without a separate credential store. */
        get: operations["get_api_users_me"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/health": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Liveness/readiness probe. Returns 200 with status `UP` when every registered health check passes (after debounce), or 503 with status `DOWN` when any component is down. Unauthenticated by design — reachable by load balancers and uptime monitors without a session. See issue #214. */
        get: operations["get_health"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
}
export type webhooks = Record<string, never>;
export interface components {
    schemas: {
        /** @description Request body for `POST /api/inbound-wg/peers`.
         *
         *     A remote-access grant targets an already-managed device (issue #810): the
         *     peer's user-facing name is taken from that device, not supplied here. */
        AddInboundWgPeerRequest: {
            /**
             * Format: uuid
             * @description The device to grant remote access to. Must already exist (discovered on
             *     the LAN at least once) and must not already have a credential.
             */
            device_id: string;
        };
        /** @description Response for `POST /api/inbound-wg/peers`.
         *
         *     Carries the complete, ready-to-import `WireGuard` client configuration —
         *     assembled server-side and containing the freshly generated **private key**
         *     exactly once. The daemon never persists the private key (it lives only in
         *     this response); the admin must copy/scan it now. This is the ONLY endpoint
         *     that ever exposes private key material, and it is admin-gated. */
        AddInboundWgPeerResponse: {
            /** @description The peer's allocated `/32` inside the inbound tunnel subnet. */
            allowed_ip: string;
            /** @description The full `WireGuard` client config (`.conf`) — `[Interface]`/`[Peer]`
             *     stanzas including the private key and endpoint — ready to render as a QR
             *     code or download. `None` when no reachable endpoint is known yet (remote
             *     access / DDNS not configured); the credential is created but not usable
             *     until an endpoint exists. */
            client_config?: string | null;
            /** Format: uuid */
            id: string;
            name: string;
            /** @description Base64 `WireGuard` public key (stored server-side). */
            public_key: string;
        };
        /** @description Request body for POST /api/setup/advance. */
        AdvanceWizardRequest: {
            to_step: components["schemas"]["WizardStep"];
            wizard_mode?: null | components["schemas"]["WizardMode"];
        };
        /** @description Response for POST /api/setup/advance. */
        AdvanceWizardResponse: {
            wizard_mode?: null | components["schemas"]["WizardMode"];
            wizard_step: components["schemas"]["WizardStep"];
        };
        /**
         * @description A permitted routing-target *kind* a zone allows. Coarse: any tunnel, or direct.
         * @enum {string}
         */
        AllowedTargetKind: "direct" | "tunnel";
        /** @description An allowlist entry, scoped to a DNS filter profile, that overrides
         *     blocklist matches. */
        AllowlistEntry: {
            /** Format: date-time */
            created_at: string;
            domain: string;
            /** Format: uuid */
            id: string;
            /** Format: uuid */
            profile_id: string;
            reason?: string | null;
        };
        /** @description API-layer mirror of [`wardnetd_services::diagnostics::Diagnostic`] that
         *     carries an `OpenAPI` schema. The service-layer type lives in a crate that
         *     does not depend on `utoipa`; mirroring it here keeps the schema boundary
         *     aligned with the HTTP API without leaking the docs dependency into the
         *     services crate. The `code` and `severity` enums are flattened to their
         *     stable string forms. */
        ApiDiagnostic: {
            /** @description Stable machine-readable class identifier, e.g. `tunnel_start_failed`. */
            code: string;
            /** @description The subsystem that raised it. */
            component: string;
            /** @description What the admin can do about it. */
            hint: string;
            /** @description Plain-language description of what happened. */
            message: string;
            /** @description One of `error`, `warning`, `info`. */
            severity: string;
            /**
             * Format: date-time
             * @description When the underlying condition occurred.
             */
            timestamp: string;
        };
        /** @description Standard API error response. */
        ApiError: {
            detail?: string | null;
            error: string;
            /** @description Request ID for correlation with server logs. */
            request_id?: string | null;
        };
        /** @description Request body for `POST /api/backup/import/apply`.
         *
         *     Consumes the `preview_token` returned by `/preview`, ensuring the
         *     apply step always has a prior preview (no silent blind restores). */
        ApplyImportRequest: {
            preview_token: string;
        };
        /** @description Response for `POST /api/backup/import/apply`.
         *
         *     Returned once the swap completes. The daemon restarts subsystems
         *     (DHCP/DNS/update runners) after the swap; subsequent API calls see
         *     the restored state. */
        ApplyImportResponse: {
            /** @description Final manifest for the applied bundle — identical to the one
             *     surfaced in the preview but repeated here so the UI can confirm
             *     without re-fetching. */
            manifest: components["schemas"]["BundleManifest"];
            /** @description `.bak-<timestamp>` snapshots of the files that were replaced.
             *     Retained by the background cleanup task for 24 h. */
            snapshots: components["schemas"]["LocalSnapshot"][];
        };
        /** @description Request body for PUT /api/devices/{id}/zone. */
        AssignDeviceZoneRequest: {
            /** Format: uuid */
            zone_id: string;
        };
        /** @description Coarse subsystem status surfaced by `GET /api/backup/status` and the web UI. */
        BackupStatus: {
            /** @enum {string} */
            state: "idle";
        } | {
            /** @enum {string} */
            state: "exporting";
        } | {
            phase: components["schemas"]["RestorePhase"];
            /** @enum {string} */
            state: "importing";
        } | {
            reason: string;
            /** @enum {string} */
            state: "failed";
        };
        /** @description Response for `GET /api/backup/status`. */
        BackupStatusResponse: {
            status: components["schemas"]["BackupStatus"];
        };
        /** @description Selector persisted when a tunnel was created via country-scoped auto-select.
         *     Present only on "best server" tunnels; `None` for specific-server or manual. */
        BestServerSelector: {
            country: string;
        };
        /** @description A URL-sourced domain blocklist scoped to a DNS filter profile. */
        Blocklist: {
            /** Format: date-time */
            created_at: string;
            /** @description Cron expression for update schedule (e.g. "0 3 * * *"). */
            cron_schedule: string;
            enabled: boolean;
            /** Format: int64 */
            entry_count: number;
            /** Format: uuid */
            id: string;
            /** @description Error message from the most recent failed download/parse, if any.
             *     Cleared on a successful refresh. Surfaced in the Ad Blocking UI so
             *     users can diagnose blocklist issues without inspecting daemon logs. */
            last_error?: string | null;
            /**
             * Format: date-time
             * @description Timestamp of the most recent failed download/parse.
             */
            last_error_at?: string | null;
            /** Format: date-time */
            last_updated?: string | null;
            name: string;
            /** Format: uuid */
            profile_id: string;
            /** Format: date-time */
            updated_at: string;
            url: string;
        };
        /** @description Metadata describing a bundle. Serialised as `manifest.json` inside the tar.
         *
         *     The manifest is the first entry extracted during validation so we can
         *     reject incompatible bundles before touching any daemon state. */
        BundleManifest: {
            /**
             * Format: int32
             * @description Bundle layout version. See `CURRENT_BUNDLE_FORMAT_VERSION`.
             */
            bundle_format_version: number;
            /**
             * Format: date-time
             * @description UTC timestamp the bundle was created.
             */
            created_at: string;
            /** @description Opaque identifier for the source host (hostname or a stable system id).
             *     Surfaced in the restore preview so operators can confirm they're not
             *     restoring a bundle from the wrong machine. */
            host_id: string;
            /**
             * Format: int32
             * @description Number of `WireGuard` key files included in the bundle. Surfaced in the
             *     preview UI so the operator knows exactly what will be overwritten.
             */
            key_count: number;
            /**
             * Format: int64
             * @description Highest applied sqlx migration version at export time.
             */
            schema_version: number;
            /** @description Daemon version (from `WARDNET_VERSION`) that produced the bundle, e.g.
             *     `0.2.0` or `0.2.1-dev.7+gabc1234`. Informational — not used for compat. */
            wardnet_version: string;
        };
        /** @description A conditional forwarding rule (domain → specific upstream). */
        ConditionalForwardingRule: {
            /** Format: date-time */
            created_at: string;
            domain: string;
            enabled: boolean;
            /** Format: uuid */
            id: string;
            upstream: string;
        };
        /** @description Request body for `POST /api/ddns/cloudflare` (BYOD provider). */
        ConfigureCloudflareRequest: {
            /** @description The fully-qualified domain the operator controls, e.g. `home.example.com`. */
            domain: string;
            /** @description A Cloudflare API token scoped to DNS:Edit on the domain's zone. */
            token: string;
        };
        /** @description A country available from a VPN provider. */
        CountryInfo: {
            /** @description ISO 3166-1 alpha-2 country code (e.g. "US"). */
            code: string;
            /** @description Human-readable country name (e.g. "United States"). */
            name: string;
        };
        /** @description Request body for POST /api/dns/allowlist. */
        CreateAllowlistRequest: {
            domain: string;
            reason?: string | null;
        };
        /** @description Response for POST /api/dns/allowlist. */
        CreateAllowlistResponse: {
            entry: components["schemas"]["AllowlistEntry"];
            message: string;
        };
        /** @description Request body for POST /api/dns/blocklists. */
        CreateBlocklistRequest: {
            cron_schedule: string;
            enabled: boolean;
            name: string;
            url: string;
        };
        /** @description Response for POST /api/dns/blocklists. */
        CreateBlocklistResponse: {
            blocklist: components["schemas"]["Blocklist"];
            message: string;
        };
        /** @description Request body for POST /api/dhcp/reservations. */
        CreateDhcpReservationRequest: {
            description?: string | null;
            hostname?: string | null;
            ip_address: string;
            mac_address: string;
        };
        /** @description Response for POST /api/dhcp/reservations. */
        CreateDhcpReservationResponse: {
            message: string;
            reservation: components["schemas"]["DhcpReservation"];
        };
        /** @description Request body for POST /api/routing/profiles/{id}/rules. */
        CreateDomainRoutingRuleRequest: {
            enabled: boolean;
            pattern: string;
            target: components["schemas"]["DomainRoutingTarget"];
        };
        /** @description Response for POST /api/routing/profiles/{id}/rules. */
        CreateDomainRoutingRuleResponse: {
            message: string;
            rule: components["schemas"]["DomainRoutingRule"];
        };
        /** @description Request body for POST /api/dns/rules. */
        CreateFilterRuleRequest: {
            comment?: string | null;
            enabled: boolean;
            rule_text: string;
        };
        /** @description Response for POST /api/dns/rules. */
        CreateFilterRuleResponse: {
            message: string;
            rule: components["schemas"]["CustomFilterRule"];
        };
        /** @description Request body for POST /api/dns/local/forwarding. */
        CreateForwardingRuleRequest: {
            domain: string;
            enabled: boolean;
            upstream: string;
        };
        /** @description Response for POST /api/dns/local/forwarding. */
        CreateForwardingRuleResponse: {
            message: string;
            rule: components["schemas"]["ConditionalForwardingRule"];
        };
        /** @description Request body for POST /api/network/zones. `provenance` is forced to `Manual`
         *     server-side; default flags are never set on create (use PUT to promote). */
        CreateNetworkZoneRequest: {
            admin_ui_reachable: boolean;
            allowed_targets: components["schemas"]["AllowedTargetKind"][];
            isolation_stance: components["schemas"]["ZoneStance"];
            member_isolation: boolean;
            name: string;
            subnet?: null | components["schemas"]["ZoneSubnet"];
        };
        /** @description Response for POST /api/network/zones. */
        CreateNetworkZoneResponse: {
            zone: components["schemas"]["NetworkZone"];
        };
        /** @description Request body for `POST /api/private-dns/grants`. */
        CreatePrivateDnsGrantRequest: {
            /**
             * Format: uuid
             * @description The device to grant encrypted-DNS access. Must already exist and must
             *     not already hold a grant.
             */
            device_id: string;
        };
        /** @description Request body for POST /api/dns/filter/profiles. */
        CreateProfileRequest: {
            /** @description Optional free-text annotation (max 200 characters). */
            description?: string | null;
            name: string;
        };
        /** @description Response for POST /api/dns/filter/profiles. */
        CreateProfileResponse: {
            message: string;
            profile: components["schemas"]["DnsFilterProfile"];
        };
        /** @description Request body for POST /api/dns/local/records. */
        CreateRecordRequest: {
            domain: string;
            enabled: boolean;
            record_type: components["schemas"]["DnsRecordType"];
            /** Format: int32 */
            ttl: number;
            value: string;
            /**
             * Format: uuid
             * @description Optional owning zone. `null`/omitted creates an unzoned record.
             */
            zone_id?: string | null;
        };
        /** @description Response for POST /api/dns/local/records. */
        CreateRecordResponse: {
            message: string;
            record: components["schemas"]["CustomDnsRecord"];
        };
        /** @description Request body for POST /api/routing/profiles. */
        CreateRoutingProfileRequest: {
            name: string;
        };
        /** @description Response for POST /api/routing/profiles. */
        CreateRoutingProfileResponse: {
            message: string;
            profile: components["schemas"]["RoutingProfile"];
        };
        /** @description Request body for `POST /api/devices/me/rule-requests`. */
        CreateRuleRequestRequest: {
            domain: string;
            kind: components["schemas"]["RuleRequestKind"];
            reason?: string | null;
        };
        /** @description Request body for POST /api/tunnels (import .conf file). */
        CreateTunnelRequest: {
            config: string;
            country_code: string;
            label: string;
            provider?: string | null;
            /** @description Human-readable name of the server resolved at creation time. */
            resolved_server_name?: string | null;
            server_selector?: null | components["schemas"]["BestServerSelector"];
        };
        /** @description Response for POST /api/tunnels. */
        CreateTunnelResponse: {
            message: string;
            tunnel: components["schemas"]["Tunnel"];
        };
        /** @description Request body for POST /api/network/zones/exceptions. */
        CreateZoneExceptionRequest: {
            bidirectional: boolean;
            from: components["schemas"]["ExceptionEndpoint"];
            service: components["schemas"]["ServiceSpec"];
            to: components["schemas"]["ExceptionEndpoint"];
        };
        /** @description Response for POST /api/network/zones/exceptions. */
        CreateZoneExceptionResponse: {
            exception: components["schemas"]["ZoneException"];
        };
        /** @description Request body for POST /api/dns/local/zones. */
        CreateZoneRequest: {
            enabled: boolean;
            name: string;
        };
        /** @description Response for POST /api/dns/local/zones. */
        CreateZoneResponse: {
            message: string;
            zone: components["schemas"]["DnsZone"];
        };
        /** @description A user-defined local DNS record. */
        CustomDnsRecord: {
            /** Format: date-time */
            created_at: string;
            domain: string;
            enabled: boolean;
            /** Format: uuid */
            id: string;
            record_type: components["schemas"]["DnsRecordType"];
            source: components["schemas"]["DnsRecordSource"];
            /** Format: int32 */
            ttl: number;
            /** Format: date-time */
            updated_at: string;
            value: string;
            /** Format: uuid */
            zone_id?: string | null;
        };
        /** @description A user-created AdGuard-syntax filter rule, scoped to a DNS filter profile. */
        CustomFilterRule: {
            comment?: string | null;
            /** Format: date-time */
            created_at: string;
            enabled: boolean;
            /** Format: uuid */
            id: string;
            /** Format: uuid */
            profile_id: string;
            rule_text: string;
            /** Format: date-time */
            updated_at: string;
        };
        /** @description Response for `GET /api/ddns/check`. */
        DdnsCheckResponse: {
            /** @description `true` when the slug is well-formed and unclaimed. */
            available: boolean;
        };
        /** @description Request body for `POST /api/ddns/enrollment-code` (wardnet provider, step 1). */
        DdnsEnrollmentCodeRequest: {
            /** @description The wardnet account email a one-time enrollment code is emailed to. */
            email: string;
        };
        /** @description Request body for `POST /api/ddns/enroll` (wardnet provider, step 2). */
        DdnsEnrollRequest: {
            /** @description The one-time code emailed to the account, as entered by the operator. */
            code: string;
        };
        /** @description Request body for `POST /api/ddns/register` (wardnet provider, final step). */
        DdnsRegisterRequest: {
            /** @description Optional human-facing network name; defaults to the slug when omitted. */
            display_name?: string | null;
            /** @description The vanity slug to claim, e.g. `happy-einstein`, forming
             *     `<slug>.my.wardnet.services`. The cloud validates it (3–32 chars,
             *     `[a-z0-9-]`, no leading/trailing hyphen, not reserved). */
            slug: string;
        };
        /** @description Response for `POST /api/ddns/register` and `POST /api/ddns/cloudflare`. */
        DdnsRegisterResponse: {
            /** @description `true` when the daemon JOINED an already-existing network of this
             *     account (same slug, re-enrollment) instead of creating a fresh one —
             *     the network's region and name won over the request (display only). */
            adopted?: boolean;
            /** @description The public hostname now assigned to this network. */
            fqdn: string;
            /** @description The region slug (display only); `None` for BYOD-Cloudflare. */
            region?: string | null;
        };
        /** @description Response for `GET /api/ddns/resolution-check`. */
        DdnsResolutionCheckResponse: {
            /** @description The IP the daemon last published (the value public DNS is expected to
             *     return), if any. */
            expected_ip?: string | null;
            /** @description The canonical FQDN that was queried, if DDNS is configured. */
            fqdn?: string | null;
            /** @description The A records the external resolver actually returned for `fqdn`. */
            resolved_ips: string[];
            /** @description Overall verdict. */
            verdict: components["schemas"]["DdnsResolutionVerdict"];
        };
        /**
         * @description Verdict of the external [resolution check](DdnsResolutionCheckResponse):
         *     whether the *public* internet resolves the canonical FQDN to the IP the
         *     daemon last published. Deliberately bypasses the local split-horizon override.
         * @enum {string}
         */
        DdnsResolutionVerdict: "not_configured" | "match" | "mismatch" | "pending";
        /** @description Response for `GET /api/ddns/status`. */
        DdnsStatusResponse: {
            /** @description The active public hostname (wardnet subdomain or BYOD domain), if any. */
            fqdn?: string | null;
            /** @description The IP last published by the daemon, if any. */
            last_public_ip?: string | null;
            /** @description `None` when DDNS is not configured; otherwise `"wardnet"` or `"cloudflare"`. */
            provider?: string | null;
            /** @description `true` when the wardnet subscription is suspended — the premium app
             *     surfaces (user PWA + admin mobile app) are disabled until it is restored.
             *     Always `false` for BYOD-Cloudflare. */
            suspended?: boolean;
        };
        /** @description Request body for `PATCH /api/rule-requests/{id}` (admin decision). Only
         *     `approved` / `rejected` are accepted — a request cannot be set back to
         *     `pending`. */
        DecideRuleRequestRequest: {
            status: components["schemas"]["RuleRequestStatus"];
        };
        /** @description Response for DELETE /api/dns/allowlist/{id}. */
        DeleteAllowlistResponse: {
            message: string;
        };
        /** @description Response for DELETE /api/dns/blocklists/{id}. */
        DeleteBlocklistResponse: {
            message: string;
        };
        /** @description Response for DELETE /api/dhcp/reservations/:id. */
        DeleteDhcpReservationResponse: {
            message: string;
        };
        /** @description Response for DELETE /api/routing/rules/{id}. */
        DeleteDomainRoutingRuleResponse: {
            message: string;
        };
        /** @description Response for DELETE /api/dns/rules/{id}. */
        DeleteFilterRuleResponse: {
            message: string;
        };
        /** @description Response for DELETE /api/dns/local/forwarding/{id}. */
        DeleteForwardingRuleResponse: {
            message: string;
        };
        /** @description Response for DELETE /api/network/zones/{id}. */
        DeleteNetworkZoneResponse: {
            deleted: boolean;
        };
        /** @description Response for DELETE /api/dns/filter/profiles/{id}. */
        DeleteProfileResponse: {
            message: string;
        };
        /** @description Response for DELETE /api/dns/local/records/{id}. */
        DeleteRecordResponse: {
            message: string;
        };
        /** @description Response for DELETE /api/routing/profiles/{id}. */
        DeleteRoutingProfileResponse: {
            message: string;
        };
        /** @description Response for DELETE /api/tunnels/:id. */
        DeleteTunnelResponse: {
            message: string;
        };
        /** @description Response for DELETE /api/network/zones/exceptions/{id}. */
        DeleteZoneExceptionResponse: {
            deleted: boolean;
        };
        /** @description Response for DELETE /api/dns/local/zones/{id}. */
        DeleteZoneResponse: {
            message: string;
        };
        /** @description A discovered network device. */
        Device: {
            admin_locked: boolean;
            /** @description How the device is currently reachable (LAN vs. inbound `WireGuard`).
             *     Live status, last-observation-wins — see [`DeviceConnectionMode`]. */
            connection_mode: components["schemas"]["DeviceConnectionMode"];
            device_type: components["schemas"]["DeviceType"];
            /** Format: int64 */
            dns_capture_cap_count: number;
            /** Format: int64 */
            dns_capture_cap_days: number;
            dns_capture_enabled: boolean;
            /** Format: date-time */
            first_seen: string;
            hostname?: string | null;
            /** Format: uuid */
            id: string;
            last_ip: string;
            /** Format: date-time */
            last_seen: string;
            mac: string;
            manufacturer?: string | null;
            name?: string | null;
            /**
             * Format: uuid
             * @description The Network Zone this device belongs to (exactly one). Sticky: set from
             *     the default-for-new zone at discovery-insert time; never resolved at read
             *     time. See [`crate::network_zone`] and epic #244.
             */
            zone_id: string;
        };
        /** @description Request body for `PATCH /api/devices/me/dns-capture` — the self-service
         *     capture toggle. Flips only the `enabled` flag; retention caps stay
         *     admin-only (set via `DnsCaptureSettingsRequest` on the admin endpoint). */
        DeviceCaptureToggleRequest: {
            enabled: boolean;
        };
        /**
         * @description How a device is currently reachable on the network (issue #810).
         *
         *     A **live status**, not a lineage tag: a device flips between the two as it
         *     connects from different paths over time (last-observation-wins), exactly
         *     like `last_ip` already does across DHCP renewals. Set by whichever path
         *     most recently observed the device — LAN ARP/DHCP discovery sets [`Lan`], an
         *     inbound `WireGuard` handshake sets [`Remote`].
         *
         *     [`Lan`]: DeviceConnectionMode::Lan
         *     [`Remote`]: DeviceConnectionMode::Remote
         * @enum {string}
         */
        DeviceConnectionMode: "lan" | "remote";
        /** @description Response for GET /api/devices/:id (admin). */
        DeviceDetailResponse: {
            current_rule?: null | components["schemas"]["RoutingTarget"];
            device: components["schemas"]["DeviceWithStatus"];
        };
        /** @description Per-device DNS filtering settings.
         *
         *     `enabled = false` is the kill switch; the device's queries skip filtering
         *     entirely. When `enabled = true` and `profile_ids` is empty, the device
         *     inherits the global default profiles from [`DnsFilterConfig`]. */
        DeviceDnsFilterSettings: {
            /** Format: uuid */
            device_id: string;
            enabled: boolean;
            /** @description Explicit profile assignments. Empty means "follow the default profiles". */
            profile_ids: string[];
            /** Format: date-time */
            updated_at: string;
        };
        /** @description Response for GET /api/devices/me. */
        DeviceMeResponse: {
            admin_locked: boolean;
            /** @description Available tunnels for self-service routing selection. */
            available_tunnels: components["schemas"]["TunnelSummary"][];
            current_rule?: null | components["schemas"]["RoutingTarget"];
            device?: null | components["schemas"]["Device"];
            /** @description Routing profiles assigned to the caller device, in priority order.
             *     Read-only — a device-keyed caller cannot manage profiles. Empty when
             *     none are assigned. */
            routing_profiles?: components["schemas"]["RoutingProfileSummary"][];
            zone?: null | components["schemas"]["ZoneSummary"];
        };
        /** @description A single rule request row. */
        DeviceRuleRequest: {
            created_at: string;
            decided_at?: string | null;
            decided_by?: string | null;
            device_id: string;
            domain: string;
            id: string;
            kind: components["schemas"]["RuleRequestKind"];
            reason?: string | null;
            status: components["schemas"]["RuleRequestStatus"];
        };
        /**
         * @description The type/category of a network device.
         * @enum {string}
         */
        DeviceType: "tv" | "phone" | "laptop" | "tablet" | "game_console" | "settop_box" | "iot" | "router" | "managed_switch" | "server" | "unknown";
        /** @description A device enriched with its DHCP status and current routing target for API
         *     responses.
         *
         *     Uses `#[serde(flatten)]` so the JSON output includes all `Device` fields
         *     at the top level alongside `dhcp_status` and `current_rule`, keeping the
         *     response backwards-compatible for consumers that ignore unknown fields.
         *
         *     `current_rule` mirrors [`DeviceDetailResponse::current_rule`]: `None` means
         *     the device has no rule of its own and follows the gateway default policy;
         *     `Some(RoutingTarget::Default)` is an explicit persisted default choice. */
        DeviceWithStatus: components["schemas"]["Device"] & {
            current_rule?: null | components["schemas"]["RoutingTarget"];
            dhcp_status: components["schemas"]["DhcpStatus"];
        };
        /** @description DHCP pool configuration.
         *
         *     Wardnet IS the gateway and DNS server — no need for users to configure those.
         *     The daemon auto-detects its own LAN IP and advertises itself as gateway (option 3)
         *     and DNS server (option 6) to clients. Option 3 includes the router IP as secondary
         *     entry for automatic failover on supported clients. */
        DhcpConfig: {
            enabled: boolean;
            /** @description Wardnet's own LAN IP — auto-detected from the LAN interface.
             *     Advertised as gateway (option 3) and DNS server (option 6). */
            gateway_ip: string;
            /** Format: int32 */
            lease_duration_secs: number;
            pool_end: string;
            pool_start: string;
            /** @description Fallback router — the real router's IP, included as secondary in option 3. */
            router_ip?: string | null;
            subnet_mask: string;
            upstream_dns: string[];
        };
        /** @description Response for GET /api/dhcp/config. */
        DhcpConfigResponse: {
            config: components["schemas"]["DhcpConfig"];
        };
        /** @description An active or historical DHCP lease. */
        DhcpLease: {
            /** Format: date-time */
            created_at: string;
            /** Format: uuid */
            device_id?: string | null;
            hostname?: string | null;
            /** Format: uuid */
            id: string;
            ip_address: string;
            /** Format: date-time */
            lease_end: string;
            /** Format: date-time */
            lease_start: string;
            mac_address: string;
            status: components["schemas"]["DhcpLeaseStatus"];
            /** Format: date-time */
            updated_at: string;
        };
        /**
         * @description The current status of a DHCP lease.
         * @enum {string}
         */
        DhcpLeaseStatus: "active" | "expired" | "released";
        /** @description A static MAC-to-IP reservation. */
        DhcpReservation: {
            /** Format: date-time */
            created_at: string;
            description?: string | null;
            hostname?: string | null;
            /** Format: uuid */
            id: string;
            ip_address: string;
            mac_address: string;
            /** Format: date-time */
            updated_at: string;
        };
        /** @description Response for POST /api/network/dhcp-self-probe.
         *
         *     The wizard's primary-mode step 3 uses this to verify Wardnet now
         *     owns LAN DHCP after the operator disabled it on their router:
         *
         *     - `wardnet_responded == true && foreign_responded == false` → ready
         *       to advance.
         *     - `foreign_responded == true` → re-show the disable-DHCP guide;
         *       `foreign_server_ip` lets the wizard call out which device is
         *       still answering. */
        DhcpSelfProbeResponse: {
            foreign_responded: boolean;
            foreign_server_ip: string | null;
            wardnet_responded: boolean;
        };
        /**
         * @description How the LAN interface acquired its current IP address.
         *
         *     Returned by `GET /api/network/status` so the wizard's network step
         *     can show a remediation panel when the host is still relying on
         *     DHCP — install.sh writes `/etc/dhcpcd.conf.d/wardnet.conf` when the
         *     operator passes `--static-ip`, which flips this to `Static`.
         * @enum {string}
         */
        DhcpSource: "static" | "dhcp" | "unknown";
        /**
         * @description Whether a device's IP is managed by the wardnet DHCP server.
         * @enum {string}
         */
        DhcpStatus: "lease" | "reservation" | "external";
        /** @description Response for GET /api/dhcp/status. */
        DhcpStatusResponse: {
            /** Format: int64 */
            active_lease_count: number;
            enabled: boolean;
            /** Format: int64 */
            pool_total: number;
            /** Format: int64 */
            pool_used: number;
            running: boolean;
        };
        /** @description Request body for POST /api/network/discover-gateway-mac.
         *
         *     All fields optional. If `mac` is provided the daemon skips the ARP
         *     probe and just persists the value (validated). Otherwise it probes
         *     the gateway IP from `GET /api/network/status`; `target_ip` lets a
         *     caller override that for testing or when the gateway differs from
         *     the kernel's default route. */
        DiscoverGatewayMacRequest: {
            mac?: string | null;
            target_ip?: string | null;
        };
        /** @description Response for POST /api/network/discover-gateway-mac. */
        DiscoverGatewayMacResponse: {
            mac: string;
            source: components["schemas"]["RouterMacSource"];
        };
        /** @description Response for POST /api/dns/cache/flush. */
        DnsCacheFlushResponse: {
            /** Format: int64 */
            entries_cleared: number;
            message: string;
        };
        /** @description Request body for `PATCH /api/devices/{id}/dns-capture` — the admin capture
         *     controls. Omitted fields are left unchanged (merged via SQL `COALESCE`). */
        DnsCaptureSettingsRequest: {
            /** Format: int64 */
            cap_count?: number | null;
            /** Format: int64 */
            cap_days?: number | null;
            enabled?: boolean | null;
        };
        /** @description Response for `GET`/`PATCH /api/devices/{id}/dns-capture` — current capture
         *     settings alongside storage stats for the device. */
        DnsCaptureSettingsResponse: {
            /** Format: int64 */
            cap_count: number;
            /** Format: int64 */
            cap_days: number;
            enabled: boolean;
            /** Format: int64 */
            row_count: number;
            /** Format: int64 */
            size_bytes: number;
        };
        /** @description Top-level DNS server configuration.
         *
         *     Persisted as individual keys in the `system_config` KV table,
         *     following the same pattern as [`crate::dhcp::DhcpConfig`]. */
        DnsConfig: {
            /** Format: int32 */
            cache_size: number;
            /** Format: int32 */
            cache_ttl_max_secs: number;
            /** Format: int32 */
            cache_ttl_min_secs: number;
            /** @description Global DNS filtering emergency stop. When `false`, every query
             *     short-circuits to `Pass` regardless of per-device or per-profile
             *     state — the kill switch above the kill switch. */
            dns_filtering_enabled: boolean;
            dnssec_enabled: boolean;
            enabled: boolean;
            /** @description How the configured upstreams are used on the forwarding path. */
            forwarder_selection_mode: components["schemas"]["ForwarderSelectionMode"];
            query_log_enabled: boolean;
            /** Format: int32 */
            query_log_retention_days: number;
            /** Format: int32 */
            rate_limit_per_second: number;
            rebinding_protection: boolean;
            resolution_mode: components["schemas"]["DnsResolutionMode"];
            /** @description The `address` of the single chosen upstream. `Some` iff
             *     `forwarder_selection_mode` is [`ForwarderSelectionMode::Single`], and
             *     always one of the `upstream_servers` addresses. */
            single_upstream?: string | null;
            upstream_servers: components["schemas"]["UpstreamDns"][];
        };
        /** @description Response for GET /api/dns/config. */
        DnsConfigResponse: {
            config: components["schemas"]["DnsConfig"];
        };
        /** @description Request body for `POST /api/devices/me/dns-events/ack` — acknowledges every
         *     captured event up to and including `up_to_id` so the daemon can delete them. */
        DnsEventsAckRequest: {
            /** Format: int64 */
            up_to_id: number;
        };
        /** @description Global DNS filtering configuration. */
        DnsFilterConfig: {
            /** @description Profiles applied to devices with no explicit assignment. Empty means
             *     unassigned devices skip filtering. Multiple profiles stack — a domain
             *     blocked in any of them is blocked. Treat as a set: the order across
             *     the get/set roundtrip is not preserved. */
            default_profile_ids: string[];
            /** @description Global emergency stop. When `false`, every query short-circuits to
             *     `Pass` regardless of profile state. */
            enabled: boolean;
        };
        /** @description Response for GET /api/dns/filter/config. */
        DnsFilterConfigResponse: {
            config: components["schemas"]["DnsFilterConfig"];
            /** @description Whether the filters are actually being enforced right now.
             *
             *     The DNS server answers queries as soon as it binds, but the compiled
             *     filters are built in the background — and after an upgrade that resets
             *     the blocklist cache, the lists have to be re-downloaded before they can
             *     block anything. During either window DNS resolves normally and
             *     blocklists are **not** enforced, which is a fail-open the admin has to
             *     be able to see. `false` means exactly that. */
            filters_ready: boolean;
            /**
             * Format: int32
             * @description How many enabled blocklists have never been imported, i.e. are empty and
             *     enforcing nothing until their first download completes.
             */
            pending_blocklists: number;
        };
        /** @description A named bundle of DNS filter sources (blocklists, allowlist, custom rules).
         *
         *     Builtin profiles cannot be deleted or modified — the API responds with
         *     `409 Conflict` when an admin attempts either operation. */
        DnsFilterProfile: {
            builtin: boolean;
            /** Format: date-time */
            created_at: string;
            /** @description Optional operator-supplied annotation. Builtin profiles carry a
             *     seeded description; custom profiles default to `None`. */
            description?: string | null;
            /** Format: uuid */
            id: string;
            name: string;
            /** Format: date-time */
            updated_at: string;
        };
        /**
         * @description Transport protocol for upstream DNS communication.
         * @enum {string}
         */
        DnsProtocol: "udp" | "tcp" | "tls" | "https";
        /** @description A single entry in the DNS query log. */
        DnsQueryLogEntry: {
            client_ip: string;
            /** Format: uuid */
            device_id?: string | null;
            domain: string;
            /** Format: int64 */
            id: number;
            /** Format: double */
            latency_ms: number;
            query_type: string;
            result: components["schemas"]["DnsQueryResult"];
            /** Format: date-time */
            timestamp: string;
            upstream?: string | null;
        };
        /**
         * @description Result classification for a DNS query.
         *
         *     Each variant's `snake_case` serialisation exactly matches the string the DNS
         *     resolver writes to the `dns_query_log.result` column, so no translation is
         *     needed between the DB and the API.
         * @enum {string}
         */
        DnsQueryResult: "forwarded" | "cache_hit" | "blocked" | "blocked_skipped" | "rewritten" | "recursive" | "upstream_error" | "negative" | "authoritative" | "authoritative_nodata" | "authoritative_nxdomain" | "rate_limited" | "rebinding_blocked" | "recursor_failed" | "error";
        /**
         * @description Provenance of a custom local DNS record.
         *
         *     `Manual` records are admin-created via the API. `Dhcp` records are
         *     auto-registered by [`crate::event::WardnetEvent::DhcpLeaseAssigned`] /
         *     `DhcpLeaseRenewed` (the `{hostname}.lan` integration). `System` is
         *     reserved for daemon-seeded records that must not be hand-edited.
         * @enum {string}
         */
        DnsRecordSource: "manual" | "dhcp" | "system";
        /**
         * @description DNS record type for custom local records.
         * @enum {string}
         */
        DnsRecordType: "A" | "AAAA" | "CNAME" | "TXT" | "MX" | "SRV";
        /**
         * @description DNS resolution strategy.
         * @enum {string}
         */
        DnsResolutionMode: "forwarding" | "recursive";
        /** @description Response for GET /api/dns/status. */
        DnsStatusResponse: {
            /** Format: int32 */
            cache_capacity: number;
            /** Format: double */
            cache_hit_rate: number;
            /** Format: int64 */
            cache_size: number;
            enabled: boolean;
            running: boolean;
            /** @description Rolling-average latency per configured upstream, from the background
             *     prober. One entry per `upstream_servers` address; empty until the
             *     prober has run at least once. */
            upstream_latencies: components["schemas"]["UpstreamLatency"][];
        };
        /** @description An authoritative local DNS zone (e.g. "lab", "home", "local"). */
        DnsZone: {
            /** Format: date-time */
            created_at: string;
            enabled: boolean;
            /** Format: uuid */
            id: string;
            name: string;
            source: components["schemas"]["DnsZoneSource"];
            /** Format: date-time */
            updated_at: string;
        };
        /**
         * @description Provenance of an authoritative local DNS zone.
         *
         *     `Manual` zones are admin-created via the API and freely deletable. `System`
         *     zones are daemon-seeded (currently only the `.lan` zone) and **cannot be
         *     deleted** — `DnsLocalService::delete_zone` rejects them. Zones are never
         *     DHCP-sourced, so (unlike [`DnsRecordSource`]) there is no `Dhcp` variant.
         * @enum {string}
         */
        DnsZoneSource: "manual" | "system";
        /** @description A single domain routing rule within a profile. */
        DomainRoutingRule: {
            /** Format: date-time */
            created_at: string;
            enabled: boolean;
            /** Format: uuid */
            id: string;
            /** @description Domain pattern. Either an exact name (`netflix.com`) or a wildcard
             *     (`*.netflix.com`) that also covers the apex — see [`domain_matches`]. */
            pattern: string;
            /** Format: uuid */
            profile_id: string;
            target: components["schemas"]["DomainRoutingTarget"];
            /** Format: date-time */
            updated_at: string;
        };
        /** @description Where a matched domain's traffic should go.
         *
         *     Deliberately narrower than [`crate::routing::RoutingTarget`]: a domain rule
         *     can only send traffic to a tunnel or force it direct. `Default` has no
         *     meaning for a per-domain override, so it is not representable here. */
        DomainRoutingTarget: {
            /** Format: uuid */
            tunnel_id: string;
            /** @enum {string} */
            type: "tunnel";
        } | {
            /** @enum {string} */
            type: "direct";
        };
        /** @description One side of a cross-zone exception: a `kind`-tagged reference to a device or
         *     a zone. */
        ExceptionEndpoint: {
            /** Format: uuid */
            id: string;
            kind: components["schemas"]["ExceptionEndpointKind"];
        };
        /**
         * @description The kind of endpoint a cross-zone exception references. An endpoint is either
         *     a single device or a whole zone; the `id` is interpreted against the matching
         *     catalog (`devices` or `network_zones`). FKs are soft/kind-tagged and are
         *     validated in the service, not the schema.
         * @enum {string}
         */
        ExceptionEndpointKind: "device" | "zone";
        /** @description Request body for `POST /api/backup/export`.
         *
         *     The passphrase is used to derive the age encryption key via scrypt.
         *     Server-side we enforce a minimum length of
         *     `crate::backup::MIN_PASSPHRASE_LEN`; clients should surface the same
         *     minimum in UI copy so failures happen before the request. */
        ExportBackupRequest: {
            /** @description Passphrase chosen by the admin. Never logged, never persisted —
             *     lives only in the memory of the request that produced the bundle. */
            passphrase: string;
        };
        /**
         * @description How the configured upstreams are used on the forwarding path
         *     (only meaningful in [`DnsResolutionMode::Forwarding`]).
         * @enum {string}
         */
        ForwarderSelectionMode: "failover" | "fastest" | "single";
        /** @description Response for GET /api/dns/filter/devices/{id}. */
        GetDeviceFilterSettingsResponse: {
            settings: components["schemas"]["DeviceDnsFilterSettings"];
        };
        /** @description Response for GET /`api/routing/devices/{device_id}/profiles`. */
        GetDeviceRoutingProfilesResponse: {
            /** @description Assigned profile ids, in priority order (first = highest priority). */
            profile_ids: string[];
        };
        /** @description Response for GET /api/dns/local/forwarding/{id}. */
        GetForwardingRuleResponse: {
            rule: components["schemas"]["ConditionalForwardingRule"];
        };
        /** @description Response for GET /api/network/zones/{id}. */
        GetNetworkZoneResponse: {
            zone: components["schemas"]["NetworkZoneView"];
        };
        /** @description Response for GET /api/dns/filter/profiles/{id}. */
        GetProfileResponse: {
            profile: components["schemas"]["DnsFilterProfile"];
        };
        /** @description Response for GET /api/dns/local/records/{id}. */
        GetRecordResponse: {
            record: components["schemas"]["CustomDnsRecord"];
        };
        /** @description Response for GET /api/routing/profiles/{id}. */
        GetRoutingProfileResponse: {
            profile: components["schemas"]["RoutingProfile"];
        };
        /** @description Response for GET /api/network/zones/exceptions/{id}. */
        GetZoneExceptionResponse: {
            exception: components["schemas"]["ZoneException"];
        };
        /** @description Response for GET /api/dns/local/zones/{id}. */
        GetZoneResponse: {
            zone: components["schemas"]["DnsZone"];
        };
        /** @description One component's debounced status within a [`HealthResponse`]. */
        HealthComponentDto: {
            /** @description Failure detail, present only while the component is `DOWN`. */
            detail?: string | null;
            /** @description Stable check name (e.g. `"database"`, `"dns"`, `"dhcp"`, `"liveness"`). */
            name: string;
            /** @description Debounced status of this component. */
            status: components["schemas"]["HealthStatusDto"];
        };
        /** @description Response body for `GET /health` (issue #214).
         *
         *     Unauthenticated, Actuator/k8s-style. The HTTP status code carries the
         *     machine-readable verdict (200 UP / 503 DOWN); the body adds the per-
         *     component breakdown for humans and dashboards. */
        HealthResponse: {
            /** @description Per-component statuses, in registration order. */
            components: components["schemas"]["HealthComponentDto"][];
            /** @description Overall verdict — `DOWN` if any component is down. */
            status: components["schemas"]["HealthStatusDto"];
        };
        /**
         * @description Overall health verdict surfaced by `GET /health` (issue #214).
         *
         *     Maps to the HTTP status: `Up` → 200, `Down` → 503. Mirrors the
         *     `HealthStatus` produced by the daemon's `HealthMonitor`.
         * @enum {string}
         */
        HealthStatusDto: "UP" | "DOWN";
        /** @description Request body for `PUT /api/inbound-wg/config`. */
        InboundWgConfigRequest: {
            /** @description Whether the inbound `WireGuard` server should be running. */
            enabled: boolean;
            /**
             * Format: int32
             * @description UDP port the server listens on for inbound peer handshakes.
             */
            listen_port: number;
        };
        /** @description Response for `PUT /api/inbound-wg/config`. */
        InboundWgConfigResponse: {
            /** @description The applied enabled state. */
            enabled: boolean;
            /**
             * Format: int32
             * @description The applied listen port.
             */
            listen_port: number;
            /** @description The server's public key once a keypair exists (generated on first
             *     enable). `None` until the server has been enabled at least once. */
            server_public_key?: string | null;
        };
        /** @description A single inbound-`WireGuard` peer, without any private key material. */
        InboundWgPeerSummary: {
            allowed_ip: string;
            /** Format: date-time */
            created_at: string;
            /**
             * Format: uuid
             * @description The `Device` this credential grants remote access to. `None` only for
             *     pre-#810 rows written before the device link existed; every row
             *     written since always carries it.
             */
            device_id?: string | null;
            enabled: boolean;
            /** Format: uuid */
            id: string;
            name: string;
            public_key: string;
        };
        /** @description Response from the public info endpoint.
         *
         *     Returns basic server information without requiring authentication.
         *     Used by the web UI's connection status widget. */
        InfoResponse: {
            /** @description Whether this box is currently entitled to the premium app surfaces
             *     (the user PWA and admin mobile app) — on the wardnet DDNS provider and
             *     not suspended. The web UI uses this to self-gate an already-installed
             *     PWA served from its service-worker precache, which never re-hits the
             *     server-side serving gate. */
            entitled: boolean;
            /** @description Public-facing `CalVer` (`YYYY.MM.DD`) read from the workspace-
             *     root `CALVER` file at compile time. Stable across dev rebuilds
             *     and is the string the web UI displays, the auto-update runner
             *     compares against the published manifest, and the `OpenAPI` spec
             *     declares as `info.version`. */
            release_version: string;
            /** Format: int64 */
            uptime_seconds: number;
            /** @description Diagnostic version string — git-derived
             *     `MAJOR.MINOR.PATCH[-dev.N+gHASH]`. Carries dev-suffix on
             *     non-tag builds so logs and `--version` output identify the
             *     exact commit. Use `release_version` for anything user-facing. */
            version: string;
        };
        /** @description Handle returned when an install is kicked off. */
        InstallHandle: {
            /** Format: uuid */
            install_id: string;
            target_version: string;
        };
        /** @description Phase of an in-flight update install. */
        InstallPhase: {
            /** @enum {string} */
            phase: "idle";
        } | {
            /** @enum {string} */
            phase: "checking";
        } | {
            /** Format: int64 */
            bytes: number;
            /** @enum {string} */
            phase: "downloading";
            /** Format: int64 */
            total?: number | null;
        } | {
            /** @enum {string} */
            phase: "verifying";
        } | {
            /** @enum {string} */
            phase: "staging";
        } | {
            /** @enum {string} */
            phase: "swapping";
        } | {
            /** @enum {string} */
            phase: "restart_pending";
        } | {
            /** @enum {string} */
            phase: "applied";
        } | {
            /** @enum {string} */
            phase: "failed";
            reason: string;
        };
        /** @description Request body for POST /api/update/install.
         *
         *     If `version` is omitted, installs the latest known version for the current
         *     channel. The operation is idempotent — calling twice while one is in
         *     flight returns the same handle. */
        InstallUpdateRequest: {
            version?: string | null;
        };
        /** @description Response for POST /api/update/install. */
        InstallUpdateResponse: {
            handle: components["schemas"]["InstallHandle"];
            message: string;
        };
        /** @description Snapshot of a job, returned by `GET /api/jobs/:id`. */
        Job: {
            /** Format: date-time */
            created_at: string;
            /** @description Populated when `status == Failed`. */
            error?: string | null;
            /** Format: uuid */
            id: string;
            kind: components["schemas"]["JobKind"];
            /**
             * Format: int32
             * @description Completion percentage, 0..=100. Jumps to 100 when the job terminates.
             */
            percentage_done: number;
            status: components["schemas"]["JobStatus"];
            /** Format: date-time */
            updated_at: string;
        };
        /** @description Response body for a handler that dispatches a background job instead of
         *     doing the work inline. The HTTP status code is `202 Accepted`. */
        JobDispatchedResponse: {
            /** Format: uuid */
            job_id: string;
        };
        /**
         * @description Discriminator for what kind of work a job represents. The kind is fixed
         *     at job creation time; new kinds get new variants here.
         * @enum {string}
         */
        JobKind: "blocklist_refresh" | "speed_test";
        /**
         * @description Lifecycle state of a [`Job`]. Jobs start as `Pending`, move to `Running`
         *     when execution begins, and end in one of the two terminal states.
         * @enum {string}
         */
        JobStatus: "PENDING" | "RUNNING" | "SUCCEED" | "TERMINATED_WITH_ERRORS";
        /**
         * @description How the previous daemon shutdown ended.
         * @enum {string}
         */
        LastShutdownState: "unknown" | "graceful" | "unclean";
        /** @description Classification of the previous daemon shutdown.
         *
         *     `at` is the timestamp the previous run last touched the database
         *     (graceful exit time, or last heartbeat for unclean events). It is
         *     `None` only for `unknown` (first-ever boot). `acknowledged_at` is
         *     set by `POST /api/system/shutdown/acknowledge`; the banner is
         *     considered dismissed iff `acknowledged_at >= at`. */
        LastShutdownStatus: {
            /** Format: date-time */
            acknowledged_at?: string | null;
            /** Format: date-time */
            at?: string | null;
            state: components["schemas"]["LastShutdownState"];
        };
        /** @description Response for GET /api/dns/allowlist. */
        ListAllowlistResponse: {
            entries: components["schemas"]["AllowlistEntry"][];
        };
        /** @description Response for GET /api/dns/blocklists. */
        ListBlocklistsResponse: {
            blocklists: components["schemas"]["Blocklist"][];
        };
        /** @description Response for GET /api/providers/:id/countries. */
        ListCountriesResponse: {
            /** @description Available countries for this provider. */
            countries: components["schemas"]["CountryInfo"][];
        };
        /** @description Response for GET /api/dns/filter/devices. */
        ListDeviceFilterSettingsResponse: {
            devices: components["schemas"]["DeviceDnsFilterSettings"][];
        };
        /** @description Response for GET /api/devices (admin). */
        ListDevicesResponse: {
            devices: components["schemas"]["DeviceWithStatus"][];
        };
        /** @description Response for GET /api/dhcp/leases. */
        ListDhcpLeasesResponse: {
            leases: components["schemas"]["DhcpLease"][];
        };
        /** @description Response for GET /api/dhcp/reservations. */
        ListDhcpReservationsResponse: {
            reservations: components["schemas"]["DhcpReservation"][];
        };
        /** @description Response for GET /api/routing/profiles/{id}/rules. */
        ListDomainRoutingRulesResponse: {
            rules: components["schemas"]["DomainRoutingRule"][];
        };
        /** @description Response for GET /api/dns/rules. */
        ListFilterRulesResponse: {
            rules: components["schemas"]["CustomFilterRule"][];
        };
        /** @description Response for GET /api/dns/local/forwarding. */
        ListForwardingRulesResponse: {
            rules: components["schemas"]["ConditionalForwardingRule"][];
        };
        /** @description Response for `GET /api/inbound-wg/peers`. */
        ListInboundWgPeersResponse: {
            peers: components["schemas"]["InboundWgPeerSummary"][];
        };
        /** @description Response for GET /api/network/zones. */
        ListNetworkZonesResponse: {
            zones: components["schemas"]["NetworkZoneView"][];
        };
        /** @description Response for GET /`api/routing/profiles/{id}/devices`. */
        ListProfileDevicesResponse: {
            /** @description Device ids the profile is currently assigned to. */
            device_ids: string[];
        };
        /** @description Response for GET /api/dns/filter/profiles. */
        ListProfilesResponse: {
            profiles: components["schemas"]["DnsFilterProfile"][];
        };
        /** @description Response for GET /api/providers. */
        ListProvidersResponse: {
            /** @description List of available VPN providers. */
            providers: components["schemas"]["ProviderInfo"][];
        };
        /** @description Response for `GET /api/dns/log`. */
        ListQueryLogResponse: {
            entries: components["schemas"]["DnsQueryLogEntry"][];
            /** Format: int64 */
            total: number;
        };
        /** @description Response for GET /api/dns/local/records and
         *     GET /api/dns/local/zones/{id}/records. */
        ListRecordsResponse: {
            records: components["schemas"]["CustomDnsRecord"][];
        };
        /** @description Response for GET /api/routing/profiles. */
        ListRoutingProfilesResponse: {
            profiles: components["schemas"]["RoutingProfile"][];
        };
        /** @description Request body for POST /api/providers/:id/servers. */
        ListServersRequest: {
            /** @description Credentials for authenticating with the provider. */
            credentials: components["schemas"]["ProviderCredentials"];
            /** @description Optional filters for the server list. */
            filter?: components["schemas"]["ServerFilter"];
        };
        /** @description Response for GET/POST /api/providers/:id/servers. */
        ListServersResponse: {
            /** @description List of available servers from the provider. */
            servers: components["schemas"]["ServerInfo"][];
        };
        /** @description Response for `GET /api/backup/snapshots`. */
        ListSnapshotsResponse: {
            snapshots: components["schemas"]["LocalSnapshot"][];
        };
        /** @description Response for GET /api/tunnels. */
        ListTunnelsResponse: {
            tunnels: components["schemas"]["Tunnel"][];
        };
        /** @description Response for GET /api/network/zones/exceptions. */
        ListZoneExceptionsResponse: {
            exceptions: components["schemas"]["ZoneException"][];
        };
        /** @description Response for GET /api/dns/local/zones. */
        ListZonesResponse: {
            zones: components["schemas"]["DnsZone"][];
        };
        /** @description Summary of a local `.bak-*` directory left behind by a previous import.
         *
         *     Retained for 24h by the background cleanup task so operators can manually
         *     recover from a bad restore. Surfaced by `GET /api/backup/snapshots`. */
        LocalSnapshot: {
            /**
             * Format: date-time
             * @description When the snapshot was created.
             */
            created_at: string;
            /** @description Which file the snapshot was taken of (`"database"`, `"config"`,
             *     `"keys"`). Lets the UI group snapshots produced by the same import. */
            kind: components["schemas"]["SnapshotKind"];
            /** @description Fully-qualified path on disk, e.g.
             *     `/var/lib/wardnet/wardnet.db.bak-20260421T143022Z`. */
            path: string;
            /**
             * Format: int64
             * @description Total size on disk in bytes (directories are summed recursively).
             */
            size_bytes: number;
        };
        /** @description Login request body. */
        LoginRequest: {
            password: string;
            /**
             * @description When `true`, the session cookie is set with a 30-day `Max-Age` instead
             *     of the default 24-hour expiry. Admin-app always sends `true`; admin-site
             *     exposes this as a "Remember me" checkbox.
             * @default false
             */
            remember_me: boolean;
            username: string;
        };
        /** @description Login response body.
         *
         *     `token` is the same opaque value written into the `wardnet_session` cookie;
         *     non-browser clients (e.g. scripts that don't maintain a cookie jar) can
         *     replay it on admin-gated requests via the `Authorization: Bearer <token>`
         *     header. `expires_in_seconds` is the token's remaining lifetime measured
         *     from the moment this response was generated. */
        LoginResponse: {
            /** Format: int64 */
            expires_in_seconds: number;
            message: string;
            token: string;
        };
        /** @description Response for GET /api/users/me. */
        MeResponse: {
            /** @description Username of the authenticated admin. */
            username: string;
        };
        /** @description Response for GET /api/network/status. */
        NetworkStatusResponse: {
            dhcp_source: components["schemas"]["DhcpSource"];
            gateway: string | null;
            interface: string;
            ip: string;
            /** @description Upstream router MAC persisted by the wizard's discover step, if
             *     any — lets the review step display it without re-probing. */
            router_mac?: string | null;
        };
        /** @description A Network Zone: a named policy bucket a device belongs to (exactly one) that
         *     gates the device's allowed routing targets, its reachability of the Pi's
         *     admin surfaces, and (Phase 2+) its network isolation. See epic #244 and the
         *     `adr-network-zone-isolation` ADR. */
        NetworkZone: {
            /** @description May devices in this zone reach the Pi's admin surfaces? (intent-only in Phase 1) */
            admin_ui_reachable: boolean;
            /** @description Routing-target kinds a device in this zone may pick. Empty is rejected on write. */
            allowed_targets: components["schemas"]["AllowedTargetKind"][];
            /** Format: date-time */
            created_at: string;
            /** Format: uuid */
            id: string;
            /** @description The protected "home"/anchor zone. Exactly one true. Deletion-guarded. */
            is_default: boolean;
            /** @description Where freshly-discovered devices are assigned. Exactly one true. */
            is_default_for_new: boolean;
            isolation_stance: components["schemas"]["ZoneStance"];
            /** @description Within an isolate-members zone, also isolate same-zone peers. (CI-3 #737) */
            member_isolation: boolean;
            name: string;
            provenance: components["schemas"]["ZoneProvenance"];
            subnet?: null | components["schemas"]["ZoneSubnet"];
            /** Format: date-time */
            updated_at: string;
        };
        /** @description A Network Zone plus its current member count. Enriched view returned by the
         *     zone list/detail endpoints. */
        NetworkZoneView: components["schemas"]["NetworkZone"] & {
            /**
             * Format: int64
             * @description Number of devices currently assigned to this zone.
             */
            member_count: number;
        };
        /** @description One entry of the admin notification feed (issue #482): a persisted record
         *     of an admin-audience push notification. */
        NotificationItem: {
            body: string;
            /** @description RFC 3339 creation timestamp. */
            created_at: string;
            id: string;
            /** @description Stable machine tag, e.g. `new_device_quarantined`, `tunnel_offline`,
             *     `routing_changed`. */
            kind: string;
            /** @description Identifier of the subject entity; what it identifies is driven by
             *     `kind` (device UUID for device kinds, tunnel UUID for tunnel kinds). */
            subject_id?: string | null;
            title: string;
            /** @description App-relative deep link (no PWA base path), e.g. `/devices`. */
            url?: string | null;
        };
        /** @description Response of `GET /api/push/notifications`. */
        NotificationsResponse: {
            /** @description Newest first. */
            notifications: components["schemas"]["NotificationItem"][];
        };
        /** @description An inclusive port range for a single protocol. A single port is expressed as
         *     `from == to`. */
        PortSpec: {
            /** Format: int32 */
            from: number;
            proto: components["schemas"]["Proto"];
            /** Format: int32 */
            to: number;
        };
        /** @description Request body for POST /api/dhcp/config/preview.
         *
         *     A dry-run that reports which active leases would be revoked if the pool
         *     were changed to `pool_start`–`pool_end`, so the admin can be warned before
         *     saving. Mutates nothing. */
        PreviewDhcpConfigRequest: {
            pool_end: string;
            pool_start: string;
        };
        /** @description Response for POST /api/dhcp/config/preview. */
        PreviewDhcpConfigResponse: {
            /** @description Active leases whose IP would fall outside the proposed pool and are not
             *     pinned by a reservation. These devices would be forced to re-acquire an
             *     in-range lease on their next renewal. */
            affected: components["schemas"]["DhcpLease"][];
        };
        /** @description One per-device grant, carrying the full minted hostname the phone dials. */
        PrivateDnsGrantSummary: {
            /** @description RFC 3339 creation timestamp. */
            created_at: string;
            /** Format: uuid */
            device_id: string;
            /** @description The device's secret hostname (`<token>.<domain>`) — entered into
             *     Android Private DNS and carried as the `ServerName` in the iOS profile. */
            hostname: string;
            /** Format: uuid */
            id: string;
        };
        /** @description Response for `GET /api/private-dns/me` — the device-keyed self-service view
         *     (identified by source IP, like `GET /api/devices/me`). */
        PrivateDnsMeResponse: {
            /** @description Whether Private DNS is enabled on the box at all. */
            enabled: boolean;
            /** @description Whether *this* device holds a grant. */
            granted: boolean;
            /** @description This device's full hostname, present only when granted and the wardnet
             *     domain is known; `null` otherwise. */
            hostname?: string | null;
        };
        /** @description Enable prerequisites for Private DNS, surfaced to the admin UI so it can
         *     explain *why* the toggle is blocked.
         *
         *     The first three are **hard gates** — [`PrivateDnsStatusResponse::enabled`]
         *     cannot be turned on unless all are `true`. `relay_connected` is
         *     **informational**: the LAN (split-horizon) path works without the reverse
         *     tunnel; only roaming needs it, so a disconnected relay never blocks enable. */
        PrivateDnsPrerequisites: {
            /** @description An issued TLS certificate is live (the `DoT` listener derives its config
             *     from it). */
            certificate: boolean;
            /** @description The box currently holds an active Premium entitlement. */
            entitled: boolean;
            /** @description The reverse tunnel to the regional edge is up. Informational only —
             *     roaming queries need it, LAN queries do not. */
            relay_connected: boolean;
            /** @description The box is on the wardnet DDNS provider (so grants get a wildcard-SAN
             *     hostname). */
            wardnet_provider: boolean;
        };
        /** @description Response for `GET /api/private-dns/status` and `PUT /api/private-dns`. */
        PrivateDnsStatusResponse: {
            /** @description The wardnet FQDN grants live under (`<token>.<domain>`), when the
             *     provider is configured; `null` otherwise. */
            domain?: string | null;
            /** @description Whether the feature (and therefore the `:853` listener) is enabled. */
            enabled: boolean;
            /** @description Every grant, oldest first. */
            grants: components["schemas"]["PrivateDnsGrantSummary"][];
            prerequisites: components["schemas"]["PrivateDnsPrerequisites"];
        };
        /**
         * @description Transport protocol a [`PortSpec`] applies to.
         * @enum {string}
         */
        Proto: "tcp" | "udp";
        /**
         * @description Supported authentication methods for a VPN provider.
         * @enum {string}
         */
        ProviderAuthMethod: "credentials" | "token";
        /** @description Credentials submitted by the admin for provider operations. */
        ProviderCredentials: {
            /** @description Service password. */
            password: string;
            /** @enum {string} */
            type: "credentials";
            /** @description Service username. */
            username: string;
        } | {
            /** @description The access token value. */
            token: string;
            /** @enum {string} */
            type: "token";
        };
        /** @description Metadata about a registered VPN provider. */
        ProviderInfo: {
            /** @description Authentication methods this provider supports. */
            auth_methods: components["schemas"]["ProviderAuthMethod"][];
            /** @description Hint text explaining where to find credentials (shown in the UI). */
            credentials_hint?: string | null;
            /** @description URL to the provider's icon/logo for UI display. */
            icon_url?: string | null;
            /** @description Unique machine identifier (e.g. "nordvpn"). */
            id: string;
            /** @description Human-readable display name (e.g. `NordVPN`). */
            name: string;
            /** @description URL to the provider's website. */
            website_url?: string | null;
        };
        /** @description Response of the push subscription mutation endpoints. */
        PushSubscriptionResponse: {
            message: string;
        };
        /** @description Response for GET /api/network/quarantine-new-devices (issue #738). */
        QuarantineNewDevicesResponse: {
            /** @description Whether new-device quarantine is on. Off by default. */
            enabled: boolean;
        };
        /** @description Response for POST /api/tunnels/:id/rebuild. */
        RebuildTunnelResponse: {
            /** @description Always `true` on a 200 response. Errors surface as non-200 status codes. */
            ok: boolean;
        };
        /** @description Response for GET /api/system/errors. */
        RecentErrorsResponse: {
            errors: components["schemas"]["ApiDiagnostic"][];
        };
        /** @description Phase of an in-flight restore. Emitted as progress events so the UI can
         *     show a spinner with meaningful status text.
         *
         *     The restore pipeline is strictly sequential — no phase is ever skipped on
         *     the happy path, and a failure preserves the phase it occurred in so the
         *     operator can tell *where* the restore broke. */
        RestorePhase: "idle" | "validating" | "stopping_runners" | "backing_up" | "extracting" | "migrating" | "restarting_runners" | "applied" | {
            /** @description Something went wrong — the `.bak-*` siblings are still on disk for
             *     manual recovery. `reason` holds the operator-facing error message. */
            failed: {
                reason: string;
            };
        };
        /** @description Schema-only stand-in so the `OpenAPI` spec can describe the
         *     multipart fields `preview_import` accepts. The handler itself
         *     pulls fields out of [`axum::extract::Multipart`] directly — this
         *     type is never materialised at runtime. */
        RestorePreviewRequest: {
            /** @description The encrypted `.wardnet.age` bundle bytes. */
            bundle: number[];
            /** @description Passphrase that was used to encrypt the bundle on export. */
            passphrase: string;
        };
        /** @description Response for `POST /api/backup/import/preview`.
         *
         *     Returned before any daemon state is touched. The UI uses this to show
         *     the admin what will change — which database, config, and key files get
         *     swapped, and which bundle they came from — so the actual apply step
         *     is a conscious confirmation. */
        RestorePreviewResponse: {
            /** @description True when the bundle's `bundle_format_version` and
             *     `schema_version` are both compatible with the running daemon. A
             *     `false` here means `apply_import` will refuse; the UI should
             *     surface `incompatibility_reason` and hide the confirm button. */
            compatible: boolean;
            /** @description Files the apply step will rename to `.bak-<timestamp>` siblings
             *     and then overwrite. Surfaced verbatim in the UI so operators
             *     understand the blast radius before confirming. */
            files_to_replace: string[];
            /** @description Human-readable reason the bundle is incompatible, when
             *     `compatible` is `false`. `None` on the happy path. */
            incompatibility_reason?: string | null;
            /** @description Bundle manifest, fully decrypted and validated. */
            manifest: components["schemas"]["BundleManifest"];
            /** @description Opaque token the caller passes back to `apply_import` to prove
             *     they just saw this preview. Scoped to a single bundle + session. */
            preview_token: string;
        };
        /** @description Response for DELETE /api/dhcp/leases/:id. */
        RevokeDhcpLeaseResponse: {
            message: string;
        };
        /** @description Response for POST /api/update/rollback. */
        RollbackResponse: {
            message: string;
        };
        /**
         * @description How the upstream router MAC was obtained.
         * @enum {string}
         */
        RouterMacSource: "arp" | "manual";
        /** @description A named bundle of domain routing rules. */
        RoutingProfile: {
            /** Format: date-time */
            created_at: string;
            /** Format: uuid */
            id: string;
            name: string;
            /**
             * Format: int64
             * @description Number of domain rules in this profile. Populated on read so the
             *     profile list can show a per-profile rule count without a follow-up
             *     query per row; `0` on a freshly created profile.
             */
            rule_count: number;
            /** Format: date-time */
            updated_at: string;
        };
        /** @description Minimal routing-profile info exposed to a self-service caller for the
         *     read-only "profiles applied to your device" display in the user PWA.
         *
         *     Like [`ZoneSummary`], the caller is device-keyed and cannot call the
         *     admin-gated `GET /api/routing/profiles`; `GET /api/devices/me` resolves the
         *     caller's assigned profiles (via an internal admin context) and hands back
         *     just id + name — enough to show "Netflix → UK is routing some of your
         *     traffic" without exposing the rules or letting the user edit them. */
        RoutingProfileSummary: {
            id: string;
            /** @description Human-readable profile name (e.g. "Streaming (UK)"). */
            name: string;
        };
        /** @description Where traffic for a device should be routed. */
        RoutingTarget: {
            /** Format: uuid */
            tunnel_id: string;
            /** @enum {string} */
            type: "tunnel";
        } | {
            /** @enum {string} */
            type: "direct";
        } | {
            /** @enum {string} */
            type: "default";
        };
        /**
         * @description What the user is asking for.
         * @enum {string}
         */
        RuleRequestKind: "block" | "allow";
        /**
         * @description Lifecycle state of a request.
         * @enum {string}
         */
        RuleRequestStatus: "pending" | "approved" | "rejected";
        /** @description Response for `POST /api/private-dns/grants/{device_id}/notify` — the result
         *     of nudging a granted device's household member to set up Private DNS. */
        SendPrivateDnsNotificationResponse: {
            /** @description Whether at least one push subscription for the device was targeted.
             *     `false` (not an error) when the device holds a grant but the household
             *     member hasn't enabled notifications, so no subscription exists. */
            delivered: boolean;
        };
        /** @description Filters for server listing. */
        ServerFilter: {
            /** @description ISO 3166-1 alpha-2 country code. */
            country?: string | null;
            /**
             * Format: int32
             * @description Maximum server load percentage (0-100).
             */
            max_load?: number | null;
        };
        /** @description Information about a single VPN server. */
        ServerInfo: {
            /** @description City name if available. */
            city?: string | null;
            /** @description ISO 3166-1 alpha-2 country code. */
            country_code: string;
            /** @description Server hostname (e.g. "se142.nordvpn.com"). */
            hostname: string;
            /** @description Provider-specific server identifier. */
            id: string;
            /**
             * Format: int32
             * @description Current load percentage (0-100).
             */
            load: number;
            /** @description Human-readable server name (e.g. "Sweden #142"). */
            name: string;
        };
        /**
         * @description A named, curated bundle of ports for a common cross-zone use case. Presets
         *     keep the wire format compact and let the daemon evolve the underlying ports
         *     without a data migration.
         * @enum {string}
         */
        ServiceSet: "casting" | "mirroring";
        /** @description How the allowed ports of an exception are specified: either a curated preset
         *     or an explicit list. Internally tagged on the wire so the two shapes are
         *     distinguishable without a discriminant field colliding with payload keys. */
        ServiceSpec: {
            set: components["schemas"]["ServiceSet"];
            /** @enum {string} */
            type: "preset";
        } | {
            ports: components["schemas"]["PortSpec"][];
            /** @enum {string} */
            type: "ports";
        };
        /** @description Request body for PUT /api/system/default-policy.
         *
         *     `policy` is either the literal string `"direct"` or a tunnel UUID.
         *     The service layer validates the format and rejects anything else. */
        SetDefaultPolicyRequest: {
            policy: string;
        };
        /** @description Response for PUT /api/system/default-policy. */
        SetDefaultPolicyResponse: {
            policy: string;
        };
        /** @description Request body for PUT /`api/routing/devices/{device_id}/profiles`. */
        SetDeviceRoutingProfilesRequest: {
            /** @description Profile ids in priority order (first = highest priority). */
            profile_ids: string[];
        };
        /** @description Response for PUT /`api/routing/devices/{device_id}/profiles`. */
        SetDeviceRoutingProfilesResponse: {
            message: string;
        };
        /** @description Request body for `PATCH /api/inbound-wg/peers/{id}`.
         *
         *     Pauses or resumes a peer without deleting its credential — distinct from
         *     `DELETE`, which revokes it permanently and requires a fresh keypair (and
         *     QR scan) to re-grant. */
        SetInboundWgPeerEnabledRequest: {
            enabled: boolean;
        };
        /** @description Request body for PUT /api/devices/me/rule. */
        SetMyRuleRequest: {
            target: components["schemas"]["RoutingTarget"];
        };
        /** @description Response body for PUT /api/devices/me/rule. */
        SetMyRuleResponse: {
            message: string;
            target: components["schemas"]["RoutingTarget"];
        };
        /** @description Request body for `PUT /api/private-dns`. */
        SetPrivateDnsEnabledRequest: {
            enabled: boolean;
        };
        /** @description Request body for PUT /api/network/quarantine-new-devices (issue #738). */
        SetQuarantineNewDevicesRequest: {
            enabled: boolean;
        };
        /** @description Request body for POST /api/providers/:id/setup. */
        SetupProviderRequest: {
            /** @description Country code for server selection (optional). */
            country?: string | null;
            /** @description Credentials for authenticating with the provider. */
            credentials: components["schemas"]["ProviderCredentials"];
            /** @description Direct server hostname for dedicated IP or manual server selection.
             *     Bypasses server listing -- resolves directly by hostname.
             *     Accepts short form (`pt131`) or full (`pt131.nordvpn.com`).
             *     Takes precedence over `server_id` when both are set. */
            hostname?: string | null;
            /** @description Optional label override; defaults to provider-generated label. */
            label?: string | null;
            /** @description If set, use this specific server ID instead of auto-selecting. */
            server_id?: string | null;
        };
        /** @description Response for POST /api/providers/:id/setup. */
        SetupProviderResponse: {
            /** @description Human-readable result message. */
            message: string;
            /** @description The selected server. */
            server: components["schemas"]["ServerInfo"];
            /** @description The created tunnel. */
            tunnel: components["schemas"]["Tunnel"];
        };
        /** @description Request body for POST /api/setup. */
        SetupRequest: {
            password: string;
            username: string;
        };
        /** @description Response for POST /api/setup. */
        SetupResponse: {
            message: string;
        };
        /** @description Response for GET /api/setup/status. */
        SetupStatusResponse: {
            /** @description Derived: `wizard_step == Completed`. Kept for `SetupGuard`
             *     backwards-compat — clients that haven't been updated still get a
             *     boolean they understand. */
            setup_completed: boolean;
            wizard_mode?: null | components["schemas"]["WizardMode"];
            wizard_step: components["schemas"]["WizardStep"];
        };
        /**
         * @description What a [`LocalSnapshot`] is a snapshot of.
         * @enum {string}
         */
        SnapshotKind: "database" | "config" | "keys";
        /**
         * @description Time-series bucket granularity for stats queries.
         * @enum {string}
         */
        StatsBucket: "minute" | "hour" | "day";
        /** @description Response for a time-series stats query.
         *
         *     The shape of the response mirrors the request: a single-metric
         *     query returns `metric` + `series`; a multi-metric query returns
         *     `results` (metric name → series). Exactly one variant is populated. */
        StatsQueryResponse: {
            metric?: string | null;
            results?: {
                [key: string]: components["schemas"]["StatsSeriesPoint"][];
            } | null;
            series?: components["schemas"]["StatsSeriesPoint"][] | null;
        };
        /** @description A single point in a stats time series. */
        StatsSeriesPoint: {
            /** @description Sorted JSON labels object for this data point. */
            labels: string;
            /** Format: date-time */
            ts: string;
            /** Format: double */
            value: number;
        };
        /** @description A single entry in a top-N result. */
        StatsTopEntry: {
            /** @description Sorted JSON labels object for this entry. */
            labels: string;
            /** Format: double */
            total: number;
        };
        /** @description Response for a top-N stats query. */
        StatsTopResponse: {
            entries: components["schemas"]["StatsTopEntry"][];
            metric: string;
        };
        /** @description Response for GET /api/system/status. */
        SystemStatusResponse: {
            /** Format: float */
            cpu_usage_percent: number;
            /** Format: int64 */
            db_size_bytes: number;
            /** Format: int64 */
            device_count: number;
            /** Format: int64 */
            disk_free_bytes: number;
            /** Format: int64 */
            disk_total_bytes: number;
            /** @description Classification of the previous daemon shutdown plus its
             *     acknowledgement timestamp. The web UI uses this to surface a
             *     persistent banner after an unclean shutdown until an admin
             *     dismisses it. */
            last_shutdown: components["schemas"]["LastShutdownStatus"];
            /** Format: int64 */
            memory_total_bytes: number;
            /** Format: int64 */
            memory_used_bytes: number;
            /** @description Public-facing `CalVer`. See `InfoResponse.release_version`. */
            release_version: string;
            /** Format: int64 */
            tunnel_active_count: number;
            /** Format: int64 */
            tunnel_count: number;
            /** Format: int64 */
            uptime_seconds: number;
            /** @description Diagnostic git-derived version. See `InfoResponse.version`. */
            version: string;
        };
        /**
         * @description Coarse stage of the daemon's TLS-certificate provisioning, surfaced so the
         *     setup wizard and dashboard can show live progress without instrumenting the
         *     ACME round-trip. Persisted in `system_config`, so it survives a restart.
         * @enum {string}
         */
        TlsProvisioningPhase: "idle" | "issuing" | "issued" | "failed";
        /** @description Response for `GET /api/tls/status`. */
        TlsStatusResponse: {
            /** @description The domain being (or already) provisioned, if any. */
            domain?: string | null;
            /** @description Human-readable error from the last failed attempt, when `phase` is
             *     [`TlsProvisioningPhase::Failed`]. */
            error?: string | null;
            /** @description RFC 3339 expiry of the stored certificate, when one has been issued. */
            not_after?: string | null;
            /** @description Current coarse provisioning phase. */
            phase: components["schemas"]["TlsProvisioningPhase"];
        };
        /** @description Request body for POST /api/dhcp/config/toggle. */
        ToggleDhcpRequest: {
            enabled: boolean;
        };
        /** @description Request body for POST /api/dns/config/toggle. */
        ToggleDnsRequest: {
            enabled: boolean;
        };
        /** @description A `WireGuard` tunnel configuration and its live state. */
        Tunnel: {
            /** Format: int64 */
            bytes_rx: number;
            /** Format: int64 */
            bytes_tx: number;
            country_code: string;
            /** Format: date-time */
            created_at: string;
            endpoint: string;
            /**
             * Format: date-time
             * @description ISO 8601 timestamp of the last endpoint re-resolution.
             */
            endpoint_resolved_at?: string | null;
            /** Format: uuid */
            id: string;
            interface_name: string;
            label: string;
            /** Format: date-time */
            last_handshake?: string | null;
            /** @description When `true`, devices routed through this tunnel resolve DNS via
             *     wardnet's DNS server (so the ad-blocking filter still runs) and
             *     wardnet forwards those queries to the tunnel's DNS server with
             *     `SO_BINDTODEVICE`. When `false`, the per-tunnel DNS server (if
             *     any) is ignored and the system-wide upstream pool is used. */
            override_default_dns: boolean;
            provider?: string | null;
            /** @description Human-readable name of the last resolved server (e.g. "United States #8395"). */
            resolved_server_name?: string | null;
            server_selector?: null | components["schemas"]["BestServerSelector"];
            status: components["schemas"]["TunnelStatus"];
        };
        /** @description Response for GET /api/tunnels/{id}. */
        TunnelDetailResponse: {
            tunnel: components["schemas"]["Tunnel"];
        };
        /** @description Response for `GET /api/tunnels/{id}/devices`.
         *
         *     The devices currently routed through the given tunnel — i.e. those
         *     whose user-set or admin-set routing rule has
         *     `target = Tunnel { tunnel_id: id }`. */
        TunnelDevicesResponse: {
            devices: components["schemas"]["Device"][];
        };
        /** @description History of recent speed tests for one tunnel, newest first. */
        TunnelSpeedTestHistoryResponse: {
            results: components["schemas"]["TunnelSpeedTestResult"][];
        };
        /** @description One completed speed test: the direct (WAN) baseline and the tunnel result,
         *     measured back-to-back in a single run. */
        TunnelSpeedTestResult: {
            /**
             * Format: double
             * @description Latency jitter (sample standard deviation) over the direct path, in ms.
             */
            direct_jitter_ms: number;
            /**
             * Format: double
             * @description Median ICMP round-trip latency over the direct path, in milliseconds.
             */
            direct_latency_ms: number;
            /**
             * Format: double
             * @description Download throughput over the direct (unbound/WAN) path, in megabits/s.
             */
            direct_throughput_mbps: number;
            /** Format: uuid */
            id: string;
            /** Format: date-time */
            tested_at: string;
            /** Format: uuid */
            tunnel_id: string;
            /**
             * Format: double
             * @description Latency jitter (sample standard deviation) through the tunnel, in ms.
             */
            tunnel_jitter_ms: number;
            /**
             * Format: double
             * @description Median ICMP round-trip latency through the tunnel, in milliseconds.
             */
            tunnel_latency_ms: number;
            /**
             * Format: double
             * @description Download throughput through the tunnel interface, in megabits/s.
             */
            tunnel_throughput_mbps: number;
        };
        /**
         * @description The current status of a `WireGuard` tunnel.
         *
         *     State machine driven by user/admin actions and the tunnel monitor's
         *     health-check loop:
         *
         *     - `Down` — kernel interface not configured (initial state, after
         *       tear-down, or after delete).
         *     - `Connecting` — `bring_up` succeeded; kernel interface is configured;
         *       no handshake observed yet.
         *     - `Up` — recent (≤ 3 min) handshake observed.
         *     - `Reconnecting` — was `Up`, last handshake stale (> 3 min) or absent;
         *       the iface is still configured and `WireGuard` keepalive is retrying.
         * @enum {string}
         */
        TunnelStatus: "up" | "down" | "connecting" | "reconnecting";
        /** @description Minimal tunnel info exposed to self-service users for routing selection. */
        TunnelSummary: {
            country_code: string;
            id: string;
            label: string;
            /** Format: date-time */
            last_handshake?: string | null;
            status: components["schemas"]["TunnelStatus"];
        };
        /** @description Response envelope for `POST /api/tunnels/{id}/test`. */
        TunnelTestResponse: {
            result: components["schemas"]["TunnelTestResult"];
        };
        /** @description Result payload of `POST /api/tunnels/{id}/test`.
         *
         *     The daemon brings the tunnel up if needed, sends a single HTTP probe
         *     through the tunnel interface, and reports the exit IP, ISO-3166 alpha-2
         *     country code, and round-trip latency in milliseconds. */
        TunnelTestResult: {
            /** @description ISO-3166 alpha-2 country code reported by the probe service. */
            country_code: string;
            /** @description Public IP observed at the tunnel exit. */
            exit_ip: string;
            /**
             * Format: int64
             * @description Round-trip latency of the probe call, in milliseconds.
             */
            latency_ms: number;
            /** Format: uuid */
            tunnel_id: string;
        };
        /** @description Request body for PUT /api/dns/blocklists/{id} (partial update). */
        UpdateBlocklistRequest: {
            cron_schedule?: string | null;
            enabled?: boolean | null;
            name?: string | null;
            url?: string | null;
        };
        /** @description Response for PUT /api/dns/blocklists/{id}. */
        UpdateBlocklistResponse: {
            blocklist: components["schemas"]["Blocklist"];
            message: string;
        };
        /**
         * @description Update release channel, in ascending order of risk.
         *
         *     `Stable` tracks only full releases (e.g. `2026.07.00`). `Beta` also
         *     considers `-beta.N` pre-releases, which are still cut through the full
         *     release ceremony. `Edge` follows `-edge.N` builds published straight from
         *     a branch with no review and no test gates — an operator's testing loop,
         *     never a destination for a real user. Edge is gated by the deploy-time
         *     `[update] allow_edge_channel` flag; the service layer rejects it unless
         *     that flag is set. See `docs/adr/0023-edge-release-channel.md`.
         * @enum {string}
         */
        UpdateChannel: "stable" | "beta" | "edge";
        /** @description Response for POST /api/update/check. */
        UpdateCheckResponse: {
            status: components["schemas"]["UpdateStatus"];
        };
        /** @description Request body for PUT /api/update/config. */
        UpdateConfigRequest: {
            auto_update_enabled?: boolean | null;
            channel?: null | components["schemas"]["UpdateChannel"];
        };
        /** @description Response for PUT /api/update/config. */
        UpdateConfigResponse: {
            status: components["schemas"]["UpdateStatus"];
        };
        /** @description Request body for PUT /api/dns/filter/devices/{id}. */
        UpdateDeviceFilterSettingsRequest: {
            enabled: boolean;
            profile_ids: string[];
        };
        /** @description Response for PUT /api/dns/filter/devices/{id}. */
        UpdateDeviceFilterSettingsResponse: {
            message: string;
            settings: components["schemas"]["DeviceDnsFilterSettings"];
        };
        /** @description Request body for PUT /api/devices/:id (admin). */
        UpdateDeviceRequest: {
            /** @description Whether to lock routing changes for this device. */
            admin_locked?: boolean | null;
            device_type?: null | components["schemas"]["DeviceType"];
            name?: string | null;
            routing_target?: null | components["schemas"]["RoutingTarget"];
        };
        /** @description Request body for PUT /api/dhcp/config. */
        UpdateDhcpConfigRequest: {
            /** Format: int32 */
            lease_duration_secs: number;
            pool_end: string;
            pool_start: string;
            router_ip?: string | null;
            subnet_mask: string;
            upstream_dns: string[];
        };
        /** @description Request body for PUT /api/dns/config. */
        UpdateDnsConfigRequest: {
            /** Format: int32 */
            cache_size?: number | null;
            /** Format: int32 */
            cache_ttl_max_secs?: number | null;
            /** Format: int32 */
            cache_ttl_min_secs?: number | null;
            dns_filtering_enabled?: boolean | null;
            dnssec_enabled?: boolean | null;
            forwarder_selection_mode?: null | components["schemas"]["ForwarderSelectionMode"];
            query_log_enabled?: boolean | null;
            /** Format: int32 */
            query_log_retention_days?: number | null;
            /** Format: int32 */
            rate_limit_per_second?: number | null;
            rebinding_protection?: boolean | null;
            resolution_mode?: null | components["schemas"]["DnsResolutionMode"];
            /** @description The chosen single upstream's `address`. Only consulted when the
             *     resulting mode is `single`; ignored (and forced clear) in the other
             *     modes. Omit to leave the current selection unchanged. */
            single_upstream?: string | null;
            upstream_servers?: components["schemas"]["UpstreamDnsRequest"][] | null;
        };
        /** @description Request body for PUT /api/dns/filter/config.
         *
         *     Both fields are partial-update: omitted from JSON = no change. For
         *     `default_profile_ids`, an empty list clears the default (unassigned
         *     devices stop being filtered) and a non-empty list replaces the entire
         *     set. */
        UpdateDnsFilterConfigRequest: {
            default_profile_ids?: string[] | null;
            enabled?: boolean | null;
        };
        /** @description Request body for PUT /api/routing/rules/{id} (partial update). */
        UpdateDomainRoutingRuleRequest: {
            enabled?: boolean | null;
            pattern?: string | null;
            target?: null | components["schemas"]["DomainRoutingTarget"];
        };
        /** @description Response for PUT /api/routing/rules/{id}. */
        UpdateDomainRoutingRuleResponse: {
            message: string;
            rule: components["schemas"]["DomainRoutingRule"];
        };
        /** @description Request body for PUT /api/dns/rules/{id} (partial update). */
        UpdateFilterRuleRequest: {
            comment?: string | null;
            enabled?: boolean | null;
            rule_text?: string | null;
        };
        /** @description Response for PUT /api/dns/rules/{id}. */
        UpdateFilterRuleResponse: {
            message: string;
            rule: components["schemas"]["CustomFilterRule"];
        };
        /** @description Request body for PUT /api/dns/local/forwarding/{id} (partial update). */
        UpdateForwardingRuleRequest: {
            domain?: string | null;
            enabled?: boolean | null;
            upstream?: string | null;
        };
        /** @description Response for PUT /api/dns/local/forwarding/{id}. */
        UpdateForwardingRuleResponse: {
            message: string;
            rule: components["schemas"]["ConditionalForwardingRule"];
        };
        /** @description A single entry in the persistent update history. */
        UpdateHistoryEntry: {
            error?: string | null;
            /** Format: date-time */
            finished_at?: string | null;
            from_version: string;
            /** Format: int64 */
            id: number;
            phase: string;
            /** Format: date-time */
            started_at: string;
            status: components["schemas"]["UpdateHistoryStatus"];
            to_version: string;
        };
        /** @description Response for GET /api/update/history. */
        UpdateHistoryResponse: {
            entries: components["schemas"]["UpdateHistoryEntry"][];
        };
        /**
         * @description Final status of a historical update attempt.
         * @enum {string}
         */
        UpdateHistoryStatus: "started" | "succeeded" | "failed" | "rolled_back";
        /** @description Request body for PUT /api/network/zones/{id} (partial update). Every field
         *     is optional. `is_default`/`is_default_for_new` set to `Some(true)` promote
         *     this zone to the anchor / default-for-new; `Some(false)` is rejected (flags
         *     only move by promoting another zone). `subnet: Some(None)` clears the subnet. */
        UpdateNetworkZoneRequest: {
            admin_ui_reachable?: boolean | null;
            allowed_targets?: components["schemas"]["AllowedTargetKind"][] | null;
            is_default?: boolean | null;
            is_default_for_new?: boolean | null;
            isolation_stance?: null | components["schemas"]["ZoneStance"];
            member_isolation?: boolean | null;
            name?: string | null;
            subnet?: null | components["schemas"]["ZoneSubnet"];
        };
        /** @description Response for PUT /api/network/zones/{id}. */
        UpdateNetworkZoneResponse: {
            zone: components["schemas"]["NetworkZone"];
        };
        /** @description Request body for PUT /api/dns/filter/profiles/{id}.
         *
         *     All fields are partial-update: omitting them from JSON leaves the stored
         *     value unchanged. Pass `description: null` to clear an existing description. */
        UpdateProfileRequest: {
            /** @description `undefined` / omitted = leave unchanged. `null` = clear. `string` = set.
             *
             *     Standard serde cannot distinguish "field absent" from "field: null" for
             *     `Option<T>`. The custom deserializer below wraps the inner deserialize so
             *     that `null` → `Some(None)` and absent → `None`, giving us three states. */
            description?: string | null;
            name?: string | null;
        };
        /** @description Response for PUT /api/dns/filter/profiles/{id}. */
        UpdateProfileResponse: {
            message: string;
            profile: components["schemas"]["DnsFilterProfile"];
        };
        /** @description Request body for PUT /api/dns/local/records/{id} (partial update). */
        UpdateRecordRequest: {
            domain?: string | null;
            enabled?: boolean | null;
            record_type?: null | components["schemas"]["DnsRecordType"];
            /** Format: int32 */
            ttl?: number | null;
            value?: string | null;
            /**
             * Format: uuid
             * @description Three-state: omitted = leave zone link unchanged, `null` = clear to
             *     NULL (unzoned), value = reassign. See [`crate::serde_util::nullable_field`].
             */
            zone_id?: string | null;
        };
        /** @description Response for PUT /api/dns/local/records/{id}. */
        UpdateRecordResponse: {
            message: string;
            record: components["schemas"]["CustomDnsRecord"];
        };
        /** @description Request body for PUT /api/routing/profiles/{id}. */
        UpdateRoutingProfileRequest: {
            name?: string | null;
        };
        /** @description Response for PUT /api/routing/profiles/{id}. */
        UpdateRoutingProfileResponse: {
            message: string;
            profile: components["schemas"]["RoutingProfile"];
        };
        /** @description Snapshot of the update subsystem's current state. */
        UpdateStatus: {
            /**
             * Format: date-time
             * @description When `applied_version` was observed running. Clients dedupe their
             *     "updated to vX" notification on this timestamp.
             */
            applied_at?: string | null;
            /** @description Version the daemon most recently *finished* applying across a restart.
             *
             *     Set by the startup reconcile when `pending_version` matched the running
             *     binary. The UI polls status and has no event channel, so this (paired
             *     with `applied_at`) is how a browser learns an update landed. */
            applied_version?: string | null;
            /** @description Whether auto-update is enabled. */
            auto_update_enabled: boolean;
            /** @description Active release channel. */
            channel: components["schemas"]["UpdateChannel"];
            /** @description Currently running daemon version. */
            current_version: string;
            /** @description Whether this box may follow the `edge` channel — i.e. whether
             *     `[update] allow_edge_channel` is set in `/etc/wardnet/wardnet.toml`.
             *
             *     Setting it requires root *on the box*, so the UI cannot enable edge
             *     itself; it only renders the option when the daemon says it exists. */
            edge_available: boolean;
            /** @description Current install phase. */
            install_phase: components["schemas"]["InstallPhase"];
            /**
             * Format: date-time
             * @description ISO-8601 UTC timestamp of the last successful check, if any.
             */
            last_check_at?: string | null;
            /**
             * Format: date-time
             * @description ISO-8601 UTC timestamp of the most recent install attempt.
             */
            last_install_at?: string | null;
            /** @description Latest version known for the active channel (if any check has run). */
            latest_version?: string | null;
            /** @description Version the in-flight install is targeting (if any).
             *
             *     Cleared by the startup reconcile once the daemon comes back up on the
             *     new binary — a pending version that outlives the restart would other-
             *     wise be reported forever. */
            pending_version?: string | null;
            /** @description Whether a `.old` binary is present that could be rolled back to. */
            rollback_available: boolean;
            /** @description Whether a newer version than the running one is available. */
            update_available: boolean;
        };
        /** @description Response for GET /api/update/status. */
        UpdateStatusResponse: {
            status: components["schemas"]["UpdateStatus"];
        };
        /** @description Request body for `PUT /api/tunnels/{id}/dns-override`. */
        UpdateTunnelDnsOverrideRequest: {
            /** @description `true` routes tunneled-device DNS through wardnet (so the
             *     ad-blocking filter still runs) using the tunnel's DNS server as
             *     upstream with `SO_BINDTODEVICE`. `false` falls back to the
             *     system-wide upstream pool. See issue #342. */
            override_default_dns: boolean;
        };
        /** @description Response for `PUT /api/tunnels/{id}/dns-override`. */
        UpdateTunnelDnsOverrideResponse: {
            tunnel: components["schemas"]["Tunnel"];
        };
        /** @description Request body for PUT /api/network/zones/exceptions/{id} (partial update).
         *     Every field is optional; absent fields are left unchanged. */
        UpdateZoneExceptionRequest: {
            bidirectional?: boolean | null;
            from?: null | components["schemas"]["ExceptionEndpoint"];
            service?: null | components["schemas"]["ServiceSpec"];
            to?: null | components["schemas"]["ExceptionEndpoint"];
        };
        /** @description Response for PUT /api/network/zones/exceptions/{id}. */
        UpdateZoneExceptionResponse: {
            exception: components["schemas"]["ZoneException"];
        };
        /** @description Request body for PUT /api/dns/local/zones/{id} (partial update). */
        UpdateZoneRequest: {
            enabled?: boolean | null;
            name?: string | null;
        };
        /** @description Response for PUT /api/dns/local/zones/{id}. */
        UpdateZoneResponse: {
            message: string;
            zone: components["schemas"]["DnsZone"];
        };
        /** @description A configured upstream DNS server. */
        UpstreamDns: {
            /** @description IP address or hostname (e.g. "1.1.1.1", "dns.google"). */
            address: string;
            /** @description Human-readable label (e.g. "Cloudflare", "Google"). */
            name: string;
            /**
             * Format: int32
             * @description Port (default: 53 for UDP/TCP, 853 for TLS, 443 for HTTPS).
             */
            port?: number | null;
            /** @description Transport protocol. */
            protocol: components["schemas"]["DnsProtocol"];
            /** @description TLS server name (SNI) for DoT/DoH. Required when `protocol` is
             *     `Tls`/`Https`, ignored otherwise: `address` is the IP to dial, and
             *     this is the hostname the upstream's certificate must match (e.g.
             *     address `"1.1.1.1"`, `tls_server_name` `"cloudflare-dns.com"`). */
            tls_server_name?: string | null;
        };
        /** @description Upstream DNS server in API requests. */
        UpstreamDnsRequest: {
            address: string;
            name: string;
            /** Format: int32 */
            port?: number | null;
            protocol: components["schemas"]["DnsProtocol"];
            /** @description TLS server name (SNI) — required when protocol is tls/https. */
            tls_server_name?: string | null;
        };
        /** @description Rolling-average latency telemetry for one configured upstream, produced by
         *     the background latency prober and surfaced in the DNS status response so the
         *     UI can show per-server performance. Not persisted. */
        UpstreamLatency: {
            /** @description The upstream's `address`, matching [`UpstreamDns::address`]. */
            address: string;
            /**
             * Format: double
             * @description Rolling-average round-trip time in milliseconds, or `None` until the
             *     first successful probe.
             */
            avg_latency_ms?: number | null;
            /** @description Whether the most recent probe reached the upstream. */
            reachable: boolean;
        };
        /** @description Request body for POST /api/providers/:id/validate. */
        ValidateCredentialsRequest: {
            /** @description Credentials to validate against the provider. */
            credentials: components["schemas"]["ProviderCredentials"];
        };
        /** @description Response for POST /api/providers/:id/validate. */
        ValidateCredentialsResponse: {
            /** @description Human-readable validation result message. */
            message: string;
            /** @description Whether the credentials are valid. */
            valid: boolean;
        };
        /** @description Response of `GET /api/push/vapid-public-key`. */
        VapidPublicKeyResponse: {
            /** @description Base64url (unpadded) uncompressed P-256 application server key, passed
             *     to `PushManager.subscribe({ applicationServerKey })` in the browser. */
            key: string;
        };
        /** @description The subscriber's encryption keys, carried inside [`WebPushSubscription`]. */
        WebPushKeys: {
            /** @description Base64url (unpadded) shared authentication secret (`auth`). */
            auth: string;
            /** @description Base64url (unpadded) P-256 ECDH public key of the subscriber (`p256dh`). */
            p256dh: string;
        };
        /** @description Standard Web Push subscription object as produced in-browser by
         *     `PushManager.subscribe()`. Body of `POST /api/push/subscriptions`. */
        WebPushSubscription: {
            /** @description Push service endpoint the daemon POSTs encrypted payloads to. */
            endpoint: string;
            keys: components["schemas"]["WebPushKeys"];
        };
        /**
         * @description Branch of the DHCP onboarding flow chosen at step 3.
         *
         *     `Primary` runs Wardnet's built-in DHCP server. `LockedRouter` keeps the
         *     upstream ISP router as the LAN's DHCP server and configures opted-in
         *     devices statically with Wardnet as their gateway/DNS.
         * @enum {string}
         */
        WizardMode: "primary" | "locked_router";
        /**
         * @description Linear stage in the first-run setup wizard.
         *
         *     The wizard advances `Admin → Network → Dhcp → RouterMac → Dns → Tunnel →
         *     Policy → RemoteAccess → Review → Completed`. `setup_completed` (in
         *     [`SetupStatusResponse`] and the `SetupGuard` redirect logic) is derived
         *     from `wizard_step == Completed`, so existing installs that already
         *     finished setup are not re-routed through the new wizard after an upgrade.
         *
         *     The wizard may also rewind: any step down to [`Self::Network`] can be
         *     revisited (never [`Self::Admin`] — admin creation is one-shot), except
         *     once [`Self::Completed`] is reached, which is terminal.
         *
         *     Steps are persisted by their stable storage string (not their ordinal), so
         *     inserting [`Self::Dns`] and [`Self::Review`] mid-sequence needs no
         *     migration: finished installs (`"completed"`) and in-progress ones keep
         *     resolving to the same variant.
         * @enum {string}
         */
        WizardStep: "admin" | "network" | "dhcp" | "router_mac" | "dns" | "tunnel" | "policy" | "remote_access" | "review" | "completed";
        /** @description A cross-zone exception: an admin-granted allowance for one endpoint to reach
         *     another across an otherwise-isolated zone boundary (e.g. a phone casting to a
         *     TV). This is data-model only in issue #737 commit 1; the zone enforcer
         *     consumes it in a later commit. */
        ZoneException: {
            /** @description Whether the allowance applies in both directions (`from ↔ to`) or only
             *     `from → to`. */
            bidirectional: boolean;
            /** Format: date-time */
            created_at: string;
            from: components["schemas"]["ExceptionEndpoint"];
            /** Format: uuid */
            id: string;
            service: components["schemas"]["ServiceSpec"];
            to: components["schemas"]["ExceptionEndpoint"];
            /** Format: date-time */
            updated_at: string;
        };
        /**
         * @description Whether a zone was seeded by the daemon or created by an admin.
         *     Mirrors the existing DNS "Zone provenance" (system | manual).
         * @enum {string}
         */
        ZoneProvenance: "system" | "manual";
        /**
         * @description Cross-zone isolation rung of the guarantee ladder (epic #244). Only the rungs
         *     we have issues for exist; VLAN is a documented non-goal (ADR), not a variant.
         * @enum {string}
         */
        ZoneStance: "shared_subnet" | "isolate_members";
        /** @description A per-zone subnet (Phase 2 / DHCP-mode). Recorded-only in #735; always None on seeds. */
        ZoneSubnet: {
            /** @description CIDR, e.g. "10.44.0.0/24". Validated when set; unused by any Phase-1 code path. */
            cidr: string;
        };
        /** @description Minimal Network Zone info exposed to a self-service caller for the
         *     read-only zone display in the user PWA.
         *
         *     The caller is device-keyed and has no admin session, so it cannot call the
         *     admin-gated `GET /api/network/zones`. `GET /api/devices/me` resolves the
         *     caller's own zone (via an internal admin context) and hands back just the
         *     name plus whether it is the anchor "home" zone — enough for
         *     "You're on: Guest — isolated from the home network" without leaking the
         *     full zone policy. */
        ZoneSummary: {
            /** @description Whether this is the protected anchor "home" zone. When `false` the
             *     device sits in a non-home zone that is isolated from it. */
            is_default: boolean;
            /** @description Human-readable zone name (e.g. "Guest"). */
            name: string;
        };
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
    post_api_auth_login: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["LoginRequest"];
            };
        };
        responses: {
            /** @description Login successful; session cookie is set */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LoginResponse"];
                };
            };
            /** @description Malformed request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Invalid credentials */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    post_api_auth_logout: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Session invalidated; session cookie cleared */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description No valid session */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Caller lacks an admin auth context */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    post_api_auth_refresh: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Session expiry extended; fresh Set-Cookie issued */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description No valid session */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Session was not created with remember_me */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    post_api_backup_export: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ExportBackupRequest"];
            };
        };
        responses: {
            /** @description Encrypted .wardnet.age bundle */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/octet-stream": number[];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_backup_import_apply: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ApplyImportRequest"];
            };
        };
        responses: {
            /** @description Restore applied, restart required */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApplyImportResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_backup_import_preview: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Bundle file (`bundle`) and passphrase (`passphrase`) as multipart fields */
        requestBody: {
            content: {
                "multipart/form-data": components["schemas"]["RestorePreviewRequest"];
            };
        };
        responses: {
            /** @description Preview token + bundle manifest */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["RestorePreviewResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_backup_snapshots: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Retained .bak-<ts> snapshots */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListSnapshotsResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_backup_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current backup subsystem status */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["BackupStatusResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_ddns: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Remote access torn down (or already absent) */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_ddns_check: {
        parameters: {
            query: {
                /** @description The vanity slug to check, e.g. happy-einstein */
                slug: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Availability result */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DdnsCheckResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_ddns_cloudflare: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ConfigureCloudflareRequest"];
            };
        };
        responses: {
            /** @description Configured; provisioning started */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DdnsRegisterResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_ddns_enroll: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DdnsEnrollRequest"];
            };
        };
        responses: {
            /** @description Enrolled */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_ddns_enrollment_code: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DdnsEnrollmentCodeRequest"];
            };
        };
        responses: {
            /** @description Code requested (emailed if the account exists) */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_ddns_register: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DdnsRegisterRequest"];
            };
        };
        responses: {
            /** @description Registered; provisioning started */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DdnsRegisterResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_ddns_resolution_check: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Resolution-check result */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DdnsResolutionCheckResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_ddns_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description DDNS status */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DdnsStatusResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_devices: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description List of devices with DHCP status */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListDevicesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_devices_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Device ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Device detail */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeviceDetailResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_devices_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Device ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateDeviceRequest"];
            };
        };
        responses: {
            /** @description Updated device detail */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeviceDetailResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_devices_id_dns_capture: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Device UUID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current capture settings and stats */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DnsCaptureSettingsResponse"];
                };
            };
            /** @description Authentication required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Device not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    patch_api_devices_id_dns_capture: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Device UUID */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DnsCaptureSettingsRequest"];
            };
        };
        responses: {
            /** @description Updated capture settings and stats */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DnsCaptureSettingsResponse"];
                };
            };
            /** @description Invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Authentication required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Device not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    put_api_devices_id_zone: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Device ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["AssignDeviceZoneRequest"];
            };
        };
        responses: {
            /** @description Updated device detail */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeviceDetailResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_devices_me: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Caller device info and available tunnels */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeviceMeResponse"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    patch_api_devices_me_dns_capture: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DeviceCaptureToggleRequest"];
            };
        };
        responses: {
            /** @description Updated capture settings and stats */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DnsCaptureSettingsResponse"];
                };
            };
            /** @description Malformed request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Device not found for this IP */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    post_api_devices_me_dns_events_ack: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DnsEventsAckRequest"];
            };
        };
        responses: {
            /** @description Events acknowledged and deleted */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Device not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Invalid request body */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    get_api_devices_me_dns_events_stream: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description SSE stream of DnsEventItem (text/event-stream) */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Device not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    put_api_devices_me_rule: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SetMyRuleRequest"];
            };
        };
        responses: {
            /** @description Updated caller routing rule */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SetMyRuleResponse"];
                };
            };
            /** @description Malformed request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Device is admin-locked */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    get_api_devices_me_rule_requests: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description The caller's rule requests */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeviceRuleRequest"][];
                };
            };
            /** @description Device not found for this IP */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    post_api_devices_me_rule_requests: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateRuleRequestRequest"];
            };
        };
        responses: {
            /** @description Request created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeviceRuleRequest"];
                };
            };
            /** @description Malformed request */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Device not found for this IP */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    get_api_dhcp_config: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current DHCP configuration */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DhcpConfigResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_dhcp_config: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateDhcpConfigRequest"];
            };
        };
        responses: {
            /** @description Updated DHCP configuration */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DhcpConfigResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dhcp_config_preview: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PreviewDhcpConfigRequest"];
            };
        };
        responses: {
            /** @description Leases that would be revoked */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PreviewDhcpConfigResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dhcp_config_toggle: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ToggleDhcpRequest"];
            };
        };
        responses: {
            /** @description Updated DHCP configuration */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DhcpConfigResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dhcp_leases: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Active DHCP leases */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListDhcpLeasesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_dhcp_leases_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Lease ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Lease revoked */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["RevokeDhcpLeaseResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dhcp_reservations: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Static DHCP reservations */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListDhcpReservationsResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dhcp_reservations: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateDhcpReservationRequest"];
            };
        };
        responses: {
            /** @description Reservation created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateDhcpReservationResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_dhcp_reservations_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Reservation ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Reservation deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteDhcpReservationResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dhcp_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description DHCP server status and pool usage */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DhcpStatusResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dns_cache_flush: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Cache flushed */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DnsCacheFlushResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_config: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current DNS configuration */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DnsConfigResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_dns_config: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateDnsConfigRequest"];
            };
        };
        responses: {
            /** @description Updated DNS configuration */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DnsConfigResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dns_config_toggle: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ToggleDnsRequest"];
            };
        };
        responses: {
            /** @description Updated DNS configuration */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DnsConfigResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_filter_config: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Filter config */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DnsFilterConfigResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_dns_filter_config: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateDnsFilterConfigRequest"];
            };
        };
        responses: {
            /** @description Updated config */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DnsFilterConfigResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_filter_devices: {
        parameters: {
            query?: {
                /** @description When `Some(false)`, restrict to devices where the kill switch is off.
                 *     When `None` (default), return every device with explicit settings or
                 *     profile assignments. */
                enabled?: boolean | null;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Device settings */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListDeviceFilterSettingsResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_filter_devices_device_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Device id */
                device_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Device settings */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetDeviceFilterSettingsResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_dns_filter_devices_device_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Device id */
                device_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateDeviceFilterSettingsRequest"];
            };
        };
        responses: {
            /** @description Updated settings */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateDeviceFilterSettingsResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_filter_profiles: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Profiles list */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListProfilesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dns_filter_profiles: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateProfileRequest"];
            };
        };
        responses: {
            /** @description Profile created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateProfileResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_filter_profiles_profile_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Profile */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetProfileResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_dns_filter_profiles_profile_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateProfileRequest"];
            };
        };
        responses: {
            /** @description Updated profile */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateProfileResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_dns_filter_profiles_profile_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Profile deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteProfileResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Profile is builtin and cannot be deleted */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_filter_profiles_profile_id_allowlist: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Allowlist */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListAllowlistResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dns_filter_profiles_profile_id_allowlist: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateAllowlistRequest"];
            };
        };
        responses: {
            /** @description Allowlist entry */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateAllowlistResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_dns_filter_profiles_profile_id_allowlist_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Allowlist entry id */
                id: string;
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Allowlist entry deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteAllowlistResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_filter_profiles_profile_id_blocklists: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Blocklists */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListBlocklistsResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dns_filter_profiles_profile_id_blocklists: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateBlocklistRequest"];
            };
        };
        responses: {
            /** @description Blocklist created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateBlocklistResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_dns_filter_profiles_profile_id_blocklists_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Blocklist id */
                id: string;
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateBlocklistRequest"];
            };
        };
        responses: {
            /** @description Updated blocklist */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateBlocklistResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_dns_filter_profiles_profile_id_blocklists_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Blocklist id */
                id: string;
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Blocklist deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteBlocklistResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dns_filter_profiles_profile_id_blocklists_id_refresh: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Blocklist id */
                id: string;
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Refresh job dispatched */
            202: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["JobDispatchedResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_filter_profiles_profile_id_rules: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Filter rules */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListFilterRulesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dns_filter_profiles_profile_id_rules: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateFilterRuleRequest"];
            };
        };
        responses: {
            /** @description Filter rule created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateFilterRuleResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_dns_filter_profiles_profile_id_rules_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Rule id */
                id: string;
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateFilterRuleRequest"];
            };
        };
        responses: {
            /** @description Updated rule */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateFilterRuleResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_dns_filter_profiles_profile_id_rules_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Rule id */
                id: string;
                /** @description Profile id */
                profile_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Rule deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteFilterRuleResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_local_forwarding: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Forwarding rules list */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListForwardingRulesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dns_local_forwarding: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateForwardingRuleRequest"];
            };
        };
        responses: {
            /** @description Forwarding rule created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateForwardingRuleResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_local_forwarding_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Forwarding rule id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Forwarding rule */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetForwardingRuleResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_dns_local_forwarding_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Forwarding rule id */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateForwardingRuleRequest"];
            };
        };
        responses: {
            /** @description Updated forwarding rule */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateForwardingRuleResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_dns_local_forwarding_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Forwarding rule id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Forwarding rule deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteForwardingRuleResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_local_records: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Records list */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListRecordsResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dns_local_records: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateRecordRequest"];
            };
        };
        responses: {
            /** @description Record created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateRecordResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_local_records_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Record id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Record */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetRecordResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_dns_local_records_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Record id */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateRecordRequest"];
            };
        };
        responses: {
            /** @description Updated record */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateRecordResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_dns_local_records_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Record id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Record deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteRecordResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_local_zones: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Zones list */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListZonesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_dns_local_zones: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateZoneRequest"];
            };
        };
        responses: {
            /** @description Zone created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateZoneResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_local_zones_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Zone id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Zone */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetZoneResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_dns_local_zones_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Zone id */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateZoneRequest"];
            };
        };
        responses: {
            /** @description Updated zone */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateZoneResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_dns_local_zones_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Zone id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Zone deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteZoneResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_local_zones_id_records: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Zone id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Records list */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListRecordsResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_log: {
        parameters: {
            query?: {
                client_ip?: string | null;
                /** @description Filter by the device attributed at query time. Stable across DHCP
                 *     reassignment; rows recorded before device attribution existed (or
                 *     from unknown sources) have no device and only match `client_ip`. */
                device_id?: string | null;
                domain?: string | null;
                limit?: number | null;
                offset?: number | null;
                result?: null | components["schemas"]["DnsQueryResult"];
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Page of query log entries */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListQueryLogResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_dns_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description DNS server status and cache stats */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DnsStatusResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_inbound_wg_config: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current inbound WireGuard config */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["InboundWgConfigResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_inbound_wg_config: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["InboundWgConfigRequest"];
            };
        };
        responses: {
            /** @description Inbound WireGuard config applied */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["InboundWgConfigResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_inbound_wg_peers: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Configured inbound peers */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListInboundWgPeersResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_inbound_wg_peers: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["AddInboundWgPeerRequest"];
            };
        };
        responses: {
            /** @description Peer admitted */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AddInboundWgPeerResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Conflicting concurrent operation */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_inbound_wg_peers_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Peer ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Peer removed */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    patch_api_inbound_wg_peers_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Peer ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SetInboundWgPeerEnabledRequest"];
            };
        };
        responses: {
            /** @description Peer state applied */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["InboundWgPeerSummary"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_info: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Daemon version, uptime, and entitlement */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["InfoResponse"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    get_api_jobs_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Job ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current job state */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Job"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_network_dhcp_self_probe: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Probe complete */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DhcpSelfProbeResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_network_discover_gateway_mac: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DiscoverGatewayMacRequest"];
            };
        };
        responses: {
            /** @description MAC discovered or accepted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DiscoverGatewayMacResponse"];
                };
            };
            /** @description Invalid MAC or no gateway to probe */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description ARP probe got no reply */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_network_quarantine_new_devices: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current toggle state */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["QuarantineNewDevicesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_network_quarantine_new_devices: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SetQuarantineNewDevicesRequest"];
            };
        };
        responses: {
            /** @description Updated toggle state */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["QuarantineNewDevicesResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_network_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current LAN interface state */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["NetworkStatusResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_network_zones: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Zones list */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListNetworkZonesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_network_zones: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateNetworkZoneRequest"];
            };
        };
        responses: {
            /** @description Zone created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateNetworkZoneResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Name already in use */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_network_zones_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Zone id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Zone */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetNetworkZoneResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_network_zones_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Zone id */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateNetworkZoneRequest"];
            };
        };
        responses: {
            /** @description Updated zone */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateNetworkZoneResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Name already in use */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_network_zones_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Zone id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Zone deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteNetworkZoneResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description System / default / non-empty zone */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_network_zones_exceptions: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Exceptions list */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListZoneExceptionsResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_network_zones_exceptions: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateZoneExceptionRequest"];
            };
        };
        responses: {
            /** @description Exception created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateZoneExceptionResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_network_zones_exceptions_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Exception id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Exception */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetZoneExceptionResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_network_zones_exceptions_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Exception id */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateZoneExceptionRequest"];
            };
        };
        responses: {
            /** @description Updated exception */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateZoneExceptionResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_network_zones_exceptions_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Exception id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Exception deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteZoneExceptionResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_private_dns: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SetPrivateDnsEnabledRequest"];
            };
        };
        responses: {
            /** @description Private DNS state applied */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PrivateDnsStatusResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Conflicting concurrent operation */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_private_dns_grants: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreatePrivateDnsGrantRequest"];
            };
        };
        responses: {
            /** @description Device granted */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PrivateDnsGrantSummary"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Conflicting concurrent operation */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_private_dns_grants_device_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Device ID */
                device_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Grant revoked */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_private_dns_grants_device_id_notify: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Device ID */
                device_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Notification dispatch result */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SendPrivateDnsNotificationResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_private_dns_me: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description This device's Private DNS state */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PrivateDnsMeResponse"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    get_api_private_dns_me_profile: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Signed configuration profile */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/x-apple-aspen-config": number[];
                };
            };
            /** @description No profile for this device */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    get_api_private_dns_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current Private DNS status */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PrivateDnsStatusResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_providers: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Registered VPN providers */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListProvidersResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_providers_id_countries: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Provider ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Countries with available servers */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListCountriesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_providers_id_servers: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Provider ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ListServersRequest"];
            };
        };
        responses: {
            /** @description Available servers */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListServersResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_providers_id_setup: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Provider ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SetupProviderRequest"];
            };
        };
        responses: {
            /** @description Guided tunnel setup result */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SetupProviderResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_providers_id_validate: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Provider ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ValidateCredentialsRequest"];
            };
        };
        responses: {
            /** @description Validation result */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ValidateCredentialsResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_push_notifications: {
        parameters: {
            query?: {
                /** @description Maximum number of entries to return (newest first). Clamped to 1..=100;
                 *     defaults to 50. */
                limit?: number | null;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Recent notifications */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["NotificationsResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    delete_api_push_notifications: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Feed cleared */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PushSubscriptionResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    post_api_push_subscriptions: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["WebPushSubscription"];
            };
        };
        responses: {
            /** @description Subscription stored */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PushSubscriptionResponse"];
                };
            };
            /** @description Malformed subscription */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Caller could not be identified */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    delete_api_push_subscriptions: {
        parameters: {
            query?: {
                /** @description When set, only the subscription with this endpoint is removed; otherwise
                 *     every subscription owned by the caller is removed. */
                endpoint?: string | null;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Subscription(s) removed */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PushSubscriptionResponse"];
                };
            };
            /** @description Caller could not be identified */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    get_api_push_vapid_public_key: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description The VAPID public key */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["VapidPublicKeyResponse"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    get_api_routing_devices_device_id_profiles: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Device id */
                device_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Assigned profiles */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetDeviceRoutingProfilesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_routing_devices_device_id_profiles: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Device id */
                device_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SetDeviceRoutingProfilesRequest"];
            };
        };
        responses: {
            /** @description Assignment updated */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SetDeviceRoutingProfilesResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_routing_profiles: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Routing profiles */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListRoutingProfilesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_routing_profiles: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateRoutingProfileRequest"];
            };
        };
        responses: {
            /** @description Profile created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateRoutingProfileResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Conflicting concurrent operation */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_routing_profiles_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Routing profile */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetRoutingProfileResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_routing_profiles_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateRoutingProfileRequest"];
            };
        };
        responses: {
            /** @description Profile updated */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateRoutingProfileResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Conflicting concurrent operation */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_routing_profiles_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Profile deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteRoutingProfileResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_routing_profiles_id_devices: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Assigned devices */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListProfileDevicesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_routing_profiles_id_rules: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Domain routing rules */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListDomainRoutingRulesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_routing_profiles_id_rules: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Profile id */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateDomainRoutingRuleRequest"];
            };
        };
        responses: {
            /** @description Rule created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateDomainRoutingRuleResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Conflicting concurrent operation */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_routing_rules_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Rule id */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateDomainRoutingRuleRequest"];
            };
        };
        responses: {
            /** @description Rule updated */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateDomainRoutingRuleResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Conflicting concurrent operation */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_routing_rules_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Rule id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Rule deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteDomainRoutingRuleResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_rule_requests: {
        parameters: {
            query?: {
                /** @description Optional status filter (`pending` | `approved` | `rejected`). */
                status?: null | components["schemas"]["RuleRequestStatus"];
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Rule requests */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeviceRuleRequest"][];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    patch_api_rule_requests_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Rule request id */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DecideRuleRequestRequest"];
            };
        };
        responses: {
            /** @description Updated request */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeviceRuleRequest"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_setup: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SetupRequest"];
            };
        };
        responses: {
            /** @description Admin account created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SetupResponse"];
                };
            };
            /** @description Malformed request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Setup already completed */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    post_api_setup_advance: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["AdvanceWizardRequest"];
            };
        };
        responses: {
            /** @description Wizard state after advance */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AdvanceWizardResponse"];
                };
            };
            /** @description Invalid transition */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_setup_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current wizard state */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SetupStatusResponse"];
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    get_api_stats: {
        parameters: {
            query: {
                bucket: components["schemas"]["StatsBucket"];
                from: string;
                /** @description Exact labels JSON string to filter by. `None` returns all label combinations. */
                label_filter?: string;
                /** @description Single metric name. Mutually exclusive with `metrics`. */
                metric?: string;
                /** @description Multiple metric names to fan out over in a single call. Mutually
                 *     exclusive with `metric`. */
                metrics?: string[];
                to: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Stats query result */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["StatsQueryResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_stats_top: {
        parameters: {
            query: {
                /** @description Storage tier to rank over. Defaults to `Minute` (intraday) when
                 *     omitted, preserving the original short-window behaviour; pass `Hour`
                 *     or `Day` to rank over the longer hourly/daily tiers so top-N covers
                 *     the same window as a matching time-series query. */
                bucket?: components["schemas"]["StatsBucket"];
                /** @description Optional label dimension to group by when `label_key` is absent
                 *     from an entry's labels (e.g. rank by `"device_id"` falling back to
                 *     `"client"` for queries from sources with no known device). */
                fallback_label_key?: string;
                from: string;
                /** @description Label dimension to rank by (e.g. `"domain"`, `"client"`). */
                label_key: string;
                limit: number;
                metric: string;
                to: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Top-N query result */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["StatsTopResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_system_default_policy: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current default policy */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SetDefaultPolicyResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_system_default_policy: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SetDefaultPolicyRequest"];
            };
        };
        responses: {
            /** @description Default policy updated */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SetDefaultPolicyResponse"];
                };
            };
            /** @description Invalid policy */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_system_errors: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Recent diagnostics */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["RecentErrorsResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_system_logs_download: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Log file stream */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/octet-stream": number[];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_system_reboot: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Reboot scheduled */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_system_restart: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Restart scheduled */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_system_shutdown: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Shutdown scheduled */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_system_shutdown_acknowledge: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Acknowledgement recorded */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_system_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description System status (version, uptime, counts) */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SystemStatusResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_tls_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description TLS provisioning status */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TlsStatusResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_tunnels: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Configured tunnels */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ListTunnelsResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_tunnels: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateTunnelRequest"];
            };
        };
        responses: {
            /** @description Tunnel imported */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateTunnelResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_tunnels_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Tunnel ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Tunnel detail */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TunnelDetailResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    delete_api_tunnels_id: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Tunnel ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Tunnel deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DeleteTunnelResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_tunnels_id_devices: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Tunnel ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Devices using the tunnel */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TunnelDevicesResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_tunnels_id_dns_override: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Tunnel ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateTunnelDnsOverrideRequest"];
            };
        };
        responses: {
            /** @description DNS override updated */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateTunnelDnsOverrideResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_tunnels_id_rebuild: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Tunnel ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Tunnel rebuild initiated */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["RebuildTunnelResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_tunnels_id_speed_test: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Tunnel ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Speed test job dispatched */
            202: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["JobDispatchedResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Conflicting concurrent operation */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_tunnels_id_speed_test_results: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Tunnel ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Speed test history */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TunnelSpeedTestHistoryResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_tunnels_id_test: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Tunnel ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Tunnel test result */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TunnelTestResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Resource not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Conflicting concurrent operation */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Upstream service unavailable or unreachable */
            502: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_update_check: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Manifest refresh result */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateCheckResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    put_api_update_config: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateConfigRequest"];
            };
        };
        responses: {
            /** @description Updated auto-update configuration */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateConfigResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_update_history: {
        parameters: {
            query?: {
                /** @description Max entries to return (default 20, capped at 100). */
                limit?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Update history */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateHistoryResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_update_install: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Install options; if omitted, installs the latest available release */
        requestBody: {
            content: {
                "application/json": components["schemas"]["InstallUpdateRequest"];
            };
        };
        responses: {
            /** @description Install initiated */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["InstallUpdateResponse"];
                };
            };
            /** @description Malformed or invalid request body */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    post_api_update_rollback: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Rollback initiated */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["RollbackResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_update_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current auto-update status */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UpdateStatusResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
        };
    };
    get_api_users_me: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Authenticated admin identity */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["MeResponse"];
                };
            };
            /** @description Unauthenticated - session cookie or API key missing/invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Forbidden - caller is not an admin */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": {
                        detail?: string | null;
                        error: string;
                        /** @description Request ID for correlation with server logs. */
                        request_id?: string | null;
                    };
                };
            };
            /** @description Internal server error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    get_health: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Daemon healthy */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["HealthResponse"];
                };
            };
            /** @description Daemon unhealthy */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["HealthResponse"];
                };
            };
        };
    };
}
