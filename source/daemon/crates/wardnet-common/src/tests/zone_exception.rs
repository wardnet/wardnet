use crate::zone_exception::*;

/// `(proto, port)` pairs for a preset, in declaration order — the shape the
/// assertions below read most clearly.
fn expanded(set: ServiceSet) -> Vec<(&'static str, u16)> {
    set.ports()
        .into_iter()
        .map(|p| {
            assert_eq!(p.from, p.to, "preset ports are single-value specs");
            (p.proto.as_str(), p.from)
        })
        .collect()
}

#[test]
fn smart_home_expands_to_the_vendor_lan_control_ports() {
    // Pinned literally, not against `ServiceSet::SmartHome.ports()` — a
    // self-referential comparison cannot catch an accidental port change.
    assert_eq!(
        expanded(ServiceSet::SmartHome),
        vec![
            ("udp", 4001),  // Govee discovery
            ("udp", 4002),  // Govee device -> client
            ("udp", 4003),  // Govee client -> device
            ("udp", 5353),  // mDNS, unicast leg
            ("udp", 1900),  // SSDP, unicast leg
            ("udp", 5683),  // CoAP / Tuya
            ("tcp", 6668),  // Tuya local control
            ("tcp", 8883),  // local MQTT/TLS
            ("udp", 56700), // LIFX
            ("tcp", 9123),  // ESPHome native API
        ]
    );
}

#[test]
fn smart_home_omits_local_http_ports() {
    // The 80/443 omission is a deliberate scope decision (issue #1098), not an
    // oversight: a zone-scoped hole there reaches every HTTP listener in the
    // peer zone. Adding them must be a conscious edit that breaks this test.
    let ports = expanded(ServiceSet::SmartHome);
    assert!(!ports.contains(&("tcp", 80)));
    assert!(!ports.contains(&("tcp", 443)));
}

#[test]
fn smart_home_is_narrow_enough_to_apply_zone_to_zone() {
    // The service-layer guard forces device-to-device endpoints for any spec
    // spanning the full port range. This preset must stay clear of it so it can
    // be applied zone-to-zone without the admin identifying a specific device.
    assert!(
        !ServiceSet::SmartHome
            .ports()
            .iter()
            .any(|p| p.from <= 1 && p.to == u16::MAX)
    );
}

#[test]
fn casting_expands_to_the_receiver_pull_control_ports() {
    assert_eq!(
        expanded(ServiceSet::Casting),
        vec![
            ("udp", 5353), // mDNS
            ("udp", 1900), // SSDP / DLNA
            ("tcp", 8008), // Chromecast / Cast
            ("tcp", 8009), // Chromecast / Cast
            ("tcp", 8443), // Google Home device listing (TLS)
            ("tcp", 9000), // Chromecast / Cast
            ("tcp", 7000), // AirPlay
            ("tcp", 7100), // AirPlay
        ]
    );
}

#[test]
fn mirroring_opens_the_full_range_on_both_protocols() {
    assert_eq!(
        ServiceSet::Mirroring.ports(),
        vec![
            PortSpec {
                proto: Proto::Tcp,
                from: 1,
                to: 65535
            },
            PortSpec {
                proto: Proto::Udp,
                from: 1,
                to: 65535
            },
        ]
    );
}

#[test]
fn service_set_wire_strings_are_snake_case() {
    // The DB stores this string verbatim in `zone_exceptions.service_preset`,
    // so renaming a variant silently orphans every existing row.
    for (set, wire) in [
        (ServiceSet::Casting, "\"casting\""),
        (ServiceSet::Mirroring, "\"mirroring\""),
        (ServiceSet::SmartHome, "\"smart_home\""),
    ] {
        assert_eq!(serde_json::to_string(&set).unwrap(), wire);
        assert_eq!(serde_json::from_str::<ServiceSet>(wire).unwrap(), set);
    }
}

#[test]
fn preset_spec_round_trips_through_the_tagged_wire_form() {
    let spec = ServiceSpec::Preset {
        set: ServiceSet::SmartHome,
    };
    let json = serde_json::to_string(&spec).unwrap();
    assert_eq!(json, r#"{"type":"preset","set":"smart_home"}"#);
    assert_eq!(serde_json::from_str::<ServiceSpec>(&json).unwrap(), spec);
}

#[test]
fn resolve_ports_funnels_presets_and_explicit_lists_alike() {
    assert_eq!(
        ServiceSpec::Preset {
            set: ServiceSet::SmartHome
        }
        .resolve_ports(),
        ServiceSet::SmartHome.ports()
    );

    let explicit = vec![PortSpec {
        proto: Proto::Tcp,
        from: 6668,
        to: 6668,
    }];
    assert_eq!(
        ServiceSpec::Ports {
            ports: explicit.clone()
        }
        .resolve_ports(),
        explicit
    );
}
