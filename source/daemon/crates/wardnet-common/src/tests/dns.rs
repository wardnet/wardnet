use crate::api::{
    CreateAllowlistRequest, CreateAllowlistResponse, CreateBlocklistRequest,
    CreateBlocklistResponse, CreateFilterRuleRequest, CreateFilterRuleResponse,
    DeleteAllowlistResponse, DeleteBlocklistResponse, DeleteFilterRuleResponse, DnsConfigResponse,
    ListAllowlistResponse, ListBlocklistsResponse, ListFilterRulesResponse,
    UpdateBlocklistNowResponse, UpdateBlocklistRequest, UpdateBlocklistResponse,
    UpdateDnsConfigRequest, UpdateFilterRuleRequest, UpdateFilterRuleResponse, UpstreamDnsRequest,
};
use crate::dns::{
    AllowlistEntry, Blocklist, ConditionalForwardingRule, CustomDnsRecord, CustomFilterRule,
    DnsConfig, DnsProtocol, DnsQueryLogEntry, DnsQueryResult, DnsRecordType, DnsResolutionMode,
    DnsStats, DnsZone, FilterAction, UpstreamDns,
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
    for result in [
        DnsQueryResult::Forwarded,
        DnsQueryResult::Cached,
        DnsQueryResult::Blocked,
        DnsQueryResult::Local,
        DnsQueryResult::Recursive,
        DnsQueryResult::Error,
    ] {
        let json = serde_json::to_string(&result).unwrap();
        let back: DnsQueryResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, back);
    }
}

#[test]
fn dns_config_default_values() {
    let config = DnsConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.resolution_mode, DnsResolutionMode::Forwarding);
    assert_eq!(config.upstream_servers.len(), 2);
    assert_eq!(config.cache_size, 10_000);
    assert!(config.ad_blocking_enabled);
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
        "ad_blocking_enabled": false,
        "query_log_enabled": false,
        "query_log_retention_days": 14
    }"#;
    let req: UpdateDnsConfigRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.resolution_mode.as_deref(), Some("recursive"));
    assert_eq!(req.cache_size, Some(20_000));
    assert_eq!(req.dnssec_enabled, Some(true));
    assert_eq!(req.rate_limit_per_second, Some(100));
    assert!(req.upstream_servers.is_some());
    assert_eq!(req.upstream_servers.as_ref().unwrap().len(), 1);
}

#[test]
fn upstream_dns_round_trip() {
    let upstream = UpstreamDns {
        address: "1.1.1.1".to_owned(),
        name: "Cloudflare".to_owned(),
        protocol: DnsProtocol::Tls,
        port: Some(853),
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
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let json = serde_json::to_string(&record).unwrap();
    let back: CustomDnsRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(record.domain, back.domain);
    assert_eq!(record.record_type, back.record_type);
}

#[test]
fn dns_zone_round_trip() {
    let zone = DnsZone {
        id: Uuid::new_v4(),
        name: "lab".to_owned(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let json = serde_json::to_string(&zone).unwrap();
    let back: DnsZone = serde_json::from_str(&json).unwrap();
    assert_eq!(zone.name, back.name);
    assert_eq!(zone.enabled, back.enabled);
}

#[test]
fn blocklist_round_trip() {
    let blocklist = Blocklist {
        id: Uuid::new_v4(),
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
fn update_blocklist_now_response_round_trip() {
    let resp = UpdateBlocklistNowResponse {
        blocklist: Blocklist {
            id: Uuid::new_v4(),
            name: "Test".to_owned(),
            url: "https://example.com".to_owned(),
            enabled: true,
            entry_count: 1234,
            last_updated: Some(Utc::now()),
            cron_schedule: "0 3 * * *".to_owned(),
            last_error: None,
            last_error_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        entry_count: 1234,
        message: "Refreshed".to_owned(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: UpdateBlocklistNowResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.entry_count, 1234);
}

#[test]
fn list_allowlist_response_round_trip() {
    let resp = ListAllowlistResponse {
        entries: vec![AllowlistEntry {
            id: Uuid::new_v4(),
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
