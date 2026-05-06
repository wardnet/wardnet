use uuid::Uuid;

use crate::dns_filter::{DeviceDnsFilterSettings, DnsFilterConfig, DnsFilterProfile};

#[test]
fn dns_filter_profile_round_trip() {
    let profile = DnsFilterProfile {
        id: Uuid::new_v4(),
        name: "Ad Blocking".to_owned(),
        builtin: true,
        created_at: "2026-05-06T00:00:00Z".parse().unwrap(),
        updated_at: "2026-05-06T00:00:00Z".parse().unwrap(),
    };
    let json = serde_json::to_string(&profile).unwrap();
    let back: DnsFilterProfile = serde_json::from_str(&json).unwrap();
    assert_eq!(profile, back);
}

#[test]
fn device_dns_filter_settings_default_for() {
    let device_id = Uuid::new_v4();
    let settings = DeviceDnsFilterSettings::default_for(device_id);
    assert_eq!(settings.device_id, device_id);
    assert!(settings.enabled);
    assert!(settings.profile_ids.is_empty());
}

#[test]
fn dns_filter_config_default() {
    let cfg = DnsFilterConfig::default();
    assert!(cfg.enabled);
    assert!(cfg.default_profile_id.is_none());
}

#[test]
fn dns_filter_config_round_trip() {
    let cfg = DnsFilterConfig {
        enabled: false,
        default_profile_id: Some(Uuid::new_v4()),
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let back: DnsFilterConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg, back);
}
