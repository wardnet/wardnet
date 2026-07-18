use crate::api::{
    CreateAllowlistRequest, CreateAllowlistResponse, CreateBlocklistRequest,
    CreateBlocklistResponse, CreateFilterRuleRequest, CreateFilterRuleResponse,
    DeleteAllowlistResponse, DeleteBlocklistResponse, DeleteFilterRuleResponse, DnsConfigResponse,
    ListAllowlistResponse, ListBlocklistsResponse, ListFilterRulesResponse, UpdateBlocklistRequest,
    UpdateBlocklistResponse, UpdateDnsConfigRequest, UpdateFilterRuleRequest,
    UpdateFilterRuleResponse, UpstreamDnsRequest,
};
use crate::dns::{
    AllowlistEntry, Blocklist, ConditionalForwardingRule, CustomDnsRecord, CustomFilterRule,
    DnsConfig, DnsProtocol, DnsQueryLogEntry, DnsQueryResult, DnsRecordSource, DnsRecordType,
    DnsResolutionMode, DnsStats, DnsZone, DnsZoneSource, FilterAction, ForwarderSelectionMode,
    UpstreamDns,
};
use chrono::Utc;
use std::net::{IpAddr, Ipv4Addr};
use uuid::Uuid;

#[test]
fn upstream_dns_request_converts_to_upstream_dns() {
    let req = UpstreamDnsRequest {
        address: "1.1.1.1".to_owned(),
        name: "Cloudflare".to_owned(),
        protocol: DnsProtocol::Udp,
        port: Some(5353),
        tls_server_name: None,
    };
    let upstream: UpstreamDns = req.into();
    assert_eq!(upstream.address, "1.1.1.1");
    assert_eq!(upstream.name, "Cloudflare");
    assert_eq!(upstream.protocol, DnsProtocol::Udp);
    assert_eq!(upstream.port, Some(5353));
}

#[test]
fn upstream_dns_request_with_no_port() {
    let req = UpstreamDnsRequest {
        address: "8.8.8.8".to_owned(),
        name: "Google".to_owned(),
        protocol: DnsProtocol::Tcp,
        port: None,
        tls_server_name: None,
    };
    let upstream: UpstreamDns = req.into();
    assert!(upstream.port.is_none());
    assert_eq!(upstream.protocol, DnsProtocol::Tcp);
}

#[test]
fn dns_protocol_round_trip() {
    for protocol in [
        DnsProtocol::Udp,
        DnsProtocol::Tcp,
        DnsProtocol::Tls,
        DnsProtocol::Https,
    ] {
        let json = serde_json::to_string(&protocol).unwrap();
        let back: DnsProtocol = serde_json::from_str(&json).unwrap();
        assert_eq!(protocol, back);
    }
}

#[test]
fn dns_protocol_snake_case_rename() {
    assert_eq!(serde_json::to_string(&DnsProtocol::Udp).unwrap(), "\"udp\"");
    assert_eq!(serde_json::to_string(&DnsProtocol::Tcp).unwrap(), "\"tcp\"");
    assert_eq!(serde_json::to_string(&DnsProtocol::Tls).unwrap(), "\"tls\"");
    assert_eq!(
        serde_json::to_string(&DnsProtocol::Https).unwrap(),
        "\"https\""
    );
}

#[test]
fn dns_resolution_mode_round_trip() {
    for mode in [DnsResolutionMode::Forwarding, DnsResolutionMode::Recursive] {
        let json = serde_json::to_string(&mode).unwrap();
        let back: DnsResolutionMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, back);
    }
}

#[test]
fn dns_resolution_mode_snake_case_rename() {
    assert_eq!(
        serde_json::to_string(&DnsResolutionMode::Forwarding).unwrap(),
        "\"forwarding\""
    );
    assert_eq!(
        serde_json::to_string(&DnsResolutionMode::Recursive).unwrap(),
        "\"recursive\""
    );
}

#[test]
fn dns_record_type_round_trip() {
    for rtype in [
        DnsRecordType::A,
        DnsRecordType::Aaaa,
        DnsRecordType::Cname,
        DnsRecordType::Txt,
        DnsRecordType::Mx,
        DnsRecordType::Srv,
    ] {
        let json = serde_json::to_string(&rtype).unwrap();
        let back: DnsRecordType = serde_json::from_str(&json).unwrap();
        assert_eq!(rtype, back);
    }
}

#[test]
fn dns_record_type_screaming_snake_rename() {
    assert_eq!(serde_json::to_string(&DnsRecordType::A).unwrap(), "\"A\"");
    assert_eq!(
        serde_json::to_string(&DnsRecordType::Aaaa).unwrap(),
        "\"AAAA\""
    );
    assert_eq!(
        serde_json::to_string(&DnsRecordType::Cname).unwrap(),
        "\"CNAME\""
    );
}

#[test]
fn dns_query_result_round_trip() {
    // Iterate `ALL` rather than a hand-written list: a list that has to be
    // updated by hand silently stops covering new variants (this one had
    // already drifted — it was missing `Authoritative`).
    for result in DnsQueryResult::ALL {
        let json = serde_json::to_string(&result).unwrap();
        let back: DnsQueryResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, back);
    }
}

/// The `serde` rename and the DB string must agree: the API serialises the
/// enum with `rename_all = "snake_case"`, while the resolver writes
/// [`DnsQueryResult::as_str`] into `dns_query_log.result`. If the two ever
/// diverge, a row written by the resolver would deserialise into a different
/// variant over the wire than it parses to in the daemon.
#[test]
fn serde_repr_matches_db_string() {
    for result in DnsQueryResult::ALL {
        let json = serde_json::to_string(&result).unwrap();
        assert_eq!(json, format!("\"{}\"", result.as_str()));
    }
}

#[test]
fn dns_config_default_values() {
    let config = DnsConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.resolution_mode, DnsResolutionMode::Forwarding);
    // Cloudflare + Google + Quad9 (Quad9 added in #636).
    assert_eq!(config.upstream_servers.len(), 3);
    assert_eq!(
        config.forwarder_selection_mode,
        ForwarderSelectionMode::Failover
    );
    assert_eq!(config.single_upstream, None);
    assert_eq!(config.cache_size, 10_000);
    assert!(config.dns_filtering_enabled);
    assert!(config.rebinding_protection);
    assert!(!config.dnssec_enabled);
    assert_eq!(config.query_log_retention_days, 7);
}

#[test]
fn dns_config_round_trip() {
    let config = DnsConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let back: DnsConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config.enabled, back.enabled);
    assert_eq!(config.resolution_mode, back.resolution_mode);
    assert_eq!(config.cache_size, back.cache_size);
}

#[test]
fn dns_config_response_round_trip() {
    let resp = DnsConfigResponse {
        config: DnsConfig::default(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: DnsConfigResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.config.cache_size, back.config.cache_size);
}

#[test]
fn update_dns_config_request_partial_deserialization() {
    // Only some fields set — rest should be None.
    let json = r#"{"cache_size": 5000}"#;
    let req: UpdateDnsConfigRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.cache_size, Some(5000));
    assert!(req.upstream_servers.is_none());
    assert!(req.dnssec_enabled.is_none());
}

#[test]
fn update_dns_config_request_full_deserialization() {
    let json = r#"{
        "resolution_mode": "recursive",
        "upstream_servers": [{"address":"9.9.9.9","name":"Quad9","protocol":"udp"}],
        "cache_size": 20000,
        "cache_ttl_min_secs": 60,
        "cache_ttl_max_secs": 3600,
        "dnssec_enabled": true,
        "rebinding_protection": false,
        "rate_limit_per_second": 100,
        "dns_filtering_enabled": false,
        "query_log_enabled": false,
        "query_log_retention_days": 14
    }"#;
    let req: UpdateDnsConfigRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.resolution_mode, Some(DnsResolutionMode::Recursive));
    assert_eq!(req.cache_size, Some(20_000));
    assert_eq!(req.dnssec_enabled, Some(true));
    assert_eq!(req.rate_limit_per_second, Some(100));
    assert!(req.upstream_servers.is_some());
    assert_eq!(req.upstream_servers.as_ref().unwrap().len(), 1);
}

#[test]
fn update_dns_config_request_rejects_invalid_resolution_mode() {
    // Now that the field is typed as the enum, an unknown mode is a hard
    // deserialization error (HTTP 422) instead of silently defaulting to
    // forwarding.
    let json = r#"{ "resolution_mode": "sideways" }"#;
    assert!(serde_json::from_str::<UpdateDnsConfigRequest>(json).is_err());
}

#[test]
fn dns_resolution_mode_as_str_matches_serde() {
    for mode in [DnsResolutionMode::Forwarding, DnsResolutionMode::Recursive] {
        let serde_repr = serde_json::to_string(&mode).unwrap();
        assert_eq!(serde_repr, format!("\"{}\"", mode.as_str()));
    }
}

#[test]
fn upstream_dns_round_trip() {
    let upstream = UpstreamDns {
        address: "1.1.1.1".to_owned(),
        name: "Cloudflare".to_owned(),
        protocol: DnsProtocol::Tls,
        port: Some(853),
        tls_server_name: None,
    };
    let json = serde_json::to_string(&upstream).unwrap();
    let back: UpstreamDns = serde_json::from_str(&json).unwrap();
    assert_eq!(upstream, back);
}

#[test]
fn upstream_dns_no_port_omitted_from_serialization() {
    let upstream = UpstreamDns {
        address: "1.1.1.1".to_owned(),
        name: "Cloudflare".to_owned(),
        protocol: DnsProtocol::Udp,
        port: None,
        tls_server_name: None,
    };
    let json = serde_json::to_string(&upstream).unwrap();
    // skip_serializing_if on port=None means it's omitted.
    assert!(!json.contains("port"));
}

#[test]
fn custom_dns_record_round_trip() {
    let record = CustomDnsRecord {
        id: Uuid::new_v4(),
        zone_id: None,
        domain: "test.lan".to_owned(),
        record_type: DnsRecordType::A,
        value: "192.168.1.50".to_owned(),
        ttl: 300,
        enabled: true,
        source: DnsRecordSource::Manual,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let json = serde_json::to_string(&record).unwrap();
    let back: CustomDnsRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(record.domain, back.domain);
    assert_eq!(record.record_type, back.record_type);
    assert_eq!(record.source, back.source);
}

#[test]
fn dns_zone_round_trip() {
    let zone = DnsZone {
        id: Uuid::new_v4(),
        name: "lab".to_owned(),
        enabled: true,
        source: DnsZoneSource::System,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let json = serde_json::to_string(&zone).unwrap();
    let back: DnsZone = serde_json::from_str(&json).unwrap();
    assert_eq!(zone.name, back.name);
    assert_eq!(zone.enabled, back.enabled);
    assert_eq!(zone.source, back.source);
}

#[test]
fn blocklist_round_trip() {
    let blocklist = Blocklist {
        id: Uuid::new_v4(),
        profile_id: Uuid::nil(),
        name: "Steven Black".to_owned(),
        url: "https://example.com/hosts".to_owned(),
        enabled: true,
        entry_count: 100_000,
        last_updated: Some(Utc::now()),
        cron_schedule: "0 3 * * *".to_owned(),
        last_error: None,
        last_error_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let json = serde_json::to_string(&blocklist).unwrap();
    let back: Blocklist = serde_json::from_str(&json).unwrap();
    assert_eq!(blocklist.name, back.name);
    assert_eq!(blocklist.entry_count, back.entry_count);
}

#[test]
fn allowlist_entry_round_trip() {
    let entry = AllowlistEntry {
        id: Uuid::new_v4(),
        profile_id: Uuid::nil(),
        domain: "safe.example.com".to_owned(),
        reason: Some("Work-related".to_owned()),
        created_at: Utc::now(),
    };
    let json = serde_json::to_string(&entry).unwrap();
    let back: AllowlistEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(entry.domain, back.domain);
    assert_eq!(entry.reason, back.reason);
}

#[test]
fn custom_filter_rule_round_trip() {
    let rule = CustomFilterRule {
        id: Uuid::new_v4(),
        profile_id: Uuid::nil(),
        rule_text: "||ads.example.com^".to_owned(),
        enabled: true,
        comment: Some("Block example ads".to_owned()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let json = serde_json::to_string(&rule).unwrap();
    let back: CustomFilterRule = serde_json::from_str(&json).unwrap();
    assert_eq!(rule.rule_text, back.rule_text);
}

#[test]
fn conditional_forwarding_rule_round_trip() {
    let rule = ConditionalForwardingRule {
        id: Uuid::new_v4(),
        domain: "corp.example.com".to_owned(),
        upstream: "10.0.0.53".to_owned(),
        enabled: true,
        created_at: Utc::now(),
    };
    let json = serde_json::to_string(&rule).unwrap();
    let back: ConditionalForwardingRule = serde_json::from_str(&json).unwrap();
    assert_eq!(rule.domain, back.domain);
    assert_eq!(rule.upstream, back.upstream);
}

#[test]
fn dns_query_log_entry_round_trip() {
    let entry = DnsQueryLogEntry {
        id: 1,
        timestamp: Utc::now(),
        client_ip: "192.168.1.100".to_owned(),
        domain: "example.com".to_owned(),
        query_type: "A".to_owned(),
        result: DnsQueryResult::Forwarded,
        upstream: Some("1.1.1.1".to_owned()),
        latency_ms: 12.5,
        device_id: Some(Uuid::new_v4()),
    };
    let json = serde_json::to_string(&entry).unwrap();
    let back: DnsQueryLogEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(entry.domain, back.domain);
    assert_eq!(entry.result, back.result);
}

#[test]
fn filter_action_round_trip_pass() {
    let action = FilterAction::Pass;
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("\"action\":\"pass\""));
    let back: FilterAction = serde_json::from_str(&json).unwrap();
    assert_eq!(action, back);
}

#[test]
fn filter_action_round_trip_block() {
    let action = FilterAction::Block;
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("\"action\":\"block\""));
    let back: FilterAction = serde_json::from_str(&json).unwrap();
    assert_eq!(action, back);
}

#[test]
fn filter_action_round_trip_rewrite() {
    let action = FilterAction::Rewrite {
        ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)),
    };
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("\"action\":\"rewrite\""));
    let back: FilterAction = serde_json::from_str(&json).unwrap();
    assert_eq!(action, back);
}

#[test]
fn list_blocklists_response_round_trip() {
    let resp = ListBlocklistsResponse {
        blocklists: vec![Blocklist {
            id: Uuid::new_v4(),
            profile_id: Uuid::nil(),
            name: "Test".to_owned(),
            url: "https://example.com/list".to_owned(),
            enabled: true,
            entry_count: 42,
            last_updated: None,
            cron_schedule: "0 3 * * *".to_owned(),
            last_error: None,
            last_error_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: ListBlocklistsResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.blocklists.len(), 1);
    assert_eq!(back.blocklists[0].name, "Test");
}

#[test]
fn create_blocklist_request_round_trip() {
    let req = CreateBlocklistRequest {
        name: "OISD".to_owned(),
        url: "https://small.oisd.nl/domainswild".to_owned(),
        cron_schedule: "0 3 * * *".to_owned(),
        enabled: false,
    };
    let json = serde_json::to_string(&req).unwrap();
    let back: CreateBlocklistRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req.name, back.name);
    assert_eq!(req.url, back.url);
    assert_eq!(req.enabled, back.enabled);
}

#[test]
fn create_blocklist_response_round_trip() {
    let resp = CreateBlocklistResponse {
        blocklist: Blocklist {
            id: Uuid::new_v4(),
            profile_id: Uuid::nil(),
            name: "Test".to_owned(),
            url: "https://example.com".to_owned(),
            enabled: true,
            entry_count: 0,
            last_updated: None,
            cron_schedule: "0 3 * * *".to_owned(),
            last_error: None,
            last_error_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        message: "Created".to_owned(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: CreateBlocklistResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.message, "Created");
}

#[test]
fn update_blocklist_request_partial_deserialization() {
    let json = r#"{"enabled": true}"#;
    let req: UpdateBlocklistRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.enabled, Some(true));
    assert!(req.name.is_none());
}

#[test]
fn update_blocklist_request_full_deserialization() {
    let json = r#"{"name":"X","url":"https://x","cron_schedule":"* * * * *","enabled":false}"#;
    let req: UpdateBlocklistRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.name.as_deref(), Some("X"));
    assert_eq!(req.url.as_deref(), Some("https://x"));
    assert_eq!(req.cron_schedule.as_deref(), Some("* * * * *"));
    assert_eq!(req.enabled, Some(false));
}

#[test]
fn update_blocklist_response_round_trip() {
    let resp = UpdateBlocklistResponse {
        blocklist: Blocklist {
            id: Uuid::new_v4(),
            profile_id: Uuid::nil(),
            name: "Test".to_owned(),
            url: "https://example.com".to_owned(),
            enabled: false,
            entry_count: 0,
            last_updated: None,
            cron_schedule: "0 3 * * *".to_owned(),
            last_error: None,
            last_error_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        message: "Updated".to_owned(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: UpdateBlocklistResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.message, "Updated");
}

#[test]
fn delete_blocklist_response_round_trip() {
    let resp = DeleteBlocklistResponse {
        message: "Deleted".to_owned(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: DeleteBlocklistResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.message, "Deleted");
}

#[test]
fn list_allowlist_response_round_trip() {
    let resp = ListAllowlistResponse {
        entries: vec![AllowlistEntry {
            id: Uuid::new_v4(),
            profile_id: Uuid::nil(),
            domain: "safe.example.com".to_owned(),
            reason: None,
            created_at: Utc::now(),
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: ListAllowlistResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.entries.len(), 1);
}

#[test]
fn create_allowlist_request_round_trip() {
    let req = CreateAllowlistRequest {
        domain: "safe.example.com".to_owned(),
        reason: Some("Work".to_owned()),
    };
    let json = serde_json::to_string(&req).unwrap();
    let back: CreateAllowlistRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req.domain, back.domain);
    assert_eq!(req.reason, back.reason);
}

#[test]
fn create_allowlist_request_no_reason_omits_field() {
    let req = CreateAllowlistRequest {
        domain: "x.com".to_owned(),
        reason: None,
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(!json.contains("reason"));
}

#[test]
fn create_allowlist_response_round_trip() {
    let resp = CreateAllowlistResponse {
        entry: AllowlistEntry {
            id: Uuid::new_v4(),
            profile_id: Uuid::nil(),
            domain: "x.com".to_owned(),
            reason: None,
            created_at: Utc::now(),
        },
        message: "Added".to_owned(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: CreateAllowlistResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.message, "Added");
}

#[test]
fn delete_allowlist_response_round_trip() {
    let resp = DeleteAllowlistResponse {
        message: "Removed".to_owned(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: DeleteAllowlistResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.message, "Removed");
}

#[test]
fn list_filter_rules_response_round_trip() {
    let resp = ListFilterRulesResponse {
        rules: vec![CustomFilterRule {
            id: Uuid::new_v4(),
            profile_id: Uuid::nil(),
            rule_text: "||ads.example.com^".to_owned(),
            enabled: true,
            comment: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: ListFilterRulesResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.rules.len(), 1);
}

#[test]
fn create_filter_rule_request_round_trip() {
    let req = CreateFilterRuleRequest {
        rule_text: "||ads.example.com^".to_owned(),
        comment: Some("block ads".to_owned()),
        enabled: true,
    };
    let json = serde_json::to_string(&req).unwrap();
    let back: CreateFilterRuleRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req.rule_text, back.rule_text);
    assert_eq!(req.comment, back.comment);
}

#[test]
fn create_filter_rule_response_round_trip() {
    let resp = CreateFilterRuleResponse {
        rule: CustomFilterRule {
            id: Uuid::new_v4(),
            profile_id: Uuid::nil(),
            rule_text: "||ads.com^".to_owned(),
            enabled: true,
            comment: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        message: "Created".to_owned(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: CreateFilterRuleResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.message, "Created");
}

#[test]
fn update_filter_rule_request_partial() {
    let json = r#"{"enabled":false}"#;
    let req: UpdateFilterRuleRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.enabled, Some(false));
    assert!(req.rule_text.is_none());
    assert!(req.comment.is_none());
}

#[test]
fn update_filter_rule_response_round_trip() {
    let resp = UpdateFilterRuleResponse {
        rule: CustomFilterRule {
            id: Uuid::new_v4(),
            profile_id: Uuid::nil(),
            rule_text: "||x.com^".to_owned(),
            enabled: false,
            comment: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        message: "Updated".to_owned(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: UpdateFilterRuleResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.message, "Updated");
}

#[test]
fn delete_filter_rule_response_round_trip() {
    let resp = DeleteFilterRuleResponse {
        message: "Deleted".to_owned(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: DeleteFilterRuleResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.message, "Deleted");
}

#[test]
fn dns_stats_round_trip() {
    let stats = DnsStats {
        total_queries: 1000,
        blocked_queries: 150,
        cached_queries: 400,
        blocked_percent: 15.0,
        top_domains: vec![("example.com".to_owned(), 50)],
        top_blocked: vec![("ads.example.com".to_owned(), 30)],
        top_clients: vec![("192.168.1.100".to_owned(), 500)],
        queries_over_time: vec![("14:00".to_owned(), 100)],
    };
    let json = serde_json::to_string(&stats).unwrap();
    let back: DnsStats = serde_json::from_str(&json).unwrap();
    assert_eq!(stats.total_queries, back.total_queries);
    assert_eq!(stats.top_domains, back.top_domains);
}

#[test]
fn default_upstreams_include_quad9() {
    let cfg = DnsConfig::default();
    let addrs: Vec<&str> = cfg
        .upstream_servers
        .iter()
        .map(|u| u.address.as_str())
        .collect();
    assert!(addrs.contains(&"1.1.1.1"), "Cloudflare present");
    assert!(addrs.contains(&"8.8.8.8"), "Google present");
    assert!(
        addrs.contains(&"9.9.9.9"),
        "Quad9 present on fresh installs"
    );
    let quad9 = cfg
        .upstream_servers
        .iter()
        .find(|u| u.address == "9.9.9.9")
        .expect("quad9");
    assert_eq!(quad9.name, "Quad9");
    assert_eq!(quad9.protocol, DnsProtocol::Udp);
}

#[test]
fn default_forwarder_selection_is_failover() {
    let cfg = DnsConfig::default();
    assert_eq!(
        cfg.forwarder_selection_mode,
        ForwarderSelectionMode::Failover
    );
    assert_eq!(cfg.single_upstream, None);
}

#[test]
fn forwarder_selection_mode_str_matches_serde() {
    // as_str() must match the snake_case wire form used for KV persistence.
    for m in [
        ForwarderSelectionMode::Failover,
        ForwarderSelectionMode::Fastest,
        ForwarderSelectionMode::Single,
    ] {
        assert_eq!(
            serde_json::to_string(&m).unwrap(),
            format!("\"{}\"", m.as_str())
        );
        // from_wire round-trips through as_str.
        assert_eq!(ForwarderSelectionMode::from_wire(m.as_str()), m);
    }
    // Unknown/legacy values fall back to the default.
    assert_eq!(
        ForwarderSelectionMode::from_wire("auto"),
        ForwarderSelectionMode::Failover
    );
}

#[test]
fn parse_all_resolver_strings() {
    assert_eq!(
        DnsQueryResult::parse("forwarded"),
        DnsQueryResult::Forwarded
    );
    assert_eq!(DnsQueryResult::parse("cache_hit"), DnsQueryResult::CacheHit);
    assert_eq!(DnsQueryResult::parse("blocked"), DnsQueryResult::Blocked);
    assert_eq!(
        DnsQueryResult::parse("blocked_skipped"),
        DnsQueryResult::BlockedSkipped
    );
    assert_eq!(
        DnsQueryResult::parse("rewritten"),
        DnsQueryResult::Rewritten
    );
    assert_eq!(
        DnsQueryResult::parse("recursive"),
        DnsQueryResult::Recursive
    );
    assert_eq!(
        DnsQueryResult::parse("upstream_error"),
        DnsQueryResult::UpstreamError
    );
    assert_eq!(
        DnsQueryResult::parse("authoritative"),
        DnsQueryResult::Authoritative
    );
}

#[test]
fn parse_unknown_falls_back_to_error() {
    assert_eq!(DnsQueryResult::parse("gibberish"), DnsQueryResult::Error);
    assert_eq!(DnsQueryResult::parse(""), DnsQueryResult::Error);
    // Old aliases no longer accepted — the migration removes those rows.
    assert_eq!(DnsQueryResult::parse("cached"), DnsQueryResult::Error);
    assert_eq!(DnsQueryResult::parse("local"), DnsQueryResult::Error);
}

#[test]
fn as_str_round_trips_through_parse() {
    for v in DnsQueryResult::ALL {
        assert_eq!(DnsQueryResult::parse(v.as_str()), v);
    }
}

/// `Error` round-trips through a real match arm, not the unknown-string
/// fallback.
///
/// Without this, `as_str_round_trips_through_parse` passes vacuously for
/// `Error`: the fallback also returns `Error`, so a missing `"error"` arm
/// satisfies the assertion while silently routing every `"error"` row
/// through the "unknown DNS result string" path.
#[test]
fn every_variant_parses_without_hitting_the_fallback() {
    for v in DnsQueryResult::ALL {
        assert_eq!(
            DnsQueryResult::from_db_str(v.as_str()),
            Some(v),
            "`{}` has no match arm — it would only survive the round-trip \
             via the unknown-string fallback",
            v.as_str()
        );
    }
}

#[test]
fn from_db_str_rejects_unknown_strings() {
    assert_eq!(DnsQueryResult::from_db_str("gibberish"), None);
    assert_eq!(DnsQueryResult::from_db_str(""), None);
}

/// `ALL` really does hold every variant, exactly once.
///
/// `slot()` is an exhaustive match, so a new variant cannot compile without
/// being given a slot; this asserts that slot lines up with a distinct
/// entry in `ALL`. Together they make `ALL` a guarantee rather than a
/// promise — every test that iterates it covers the new variant on day one.
#[test]
fn all_is_complete() {
    for (i, v) in DnsQueryResult::ALL.into_iter().enumerate() {
        assert_eq!(v.slot(), i, "ALL[{i}] = {v:?} is in the wrong slot");
    }
}
