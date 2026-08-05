use crate::oui::{guess_device_type, is_randomized_mac, lookup_manufacturer};
use wardnet_common::device::DeviceType;

#[test]
fn known_apple_prefix() {
    // F0:EE:7A is registered to "Apple, Inc." in the IEEE MA-L database.
    let result = lookup_manufacturer("F0:EE:7A:00:11:22");
    assert_eq!(result, Some("Apple, Inc."));
}

#[test]
fn known_samsung_prefix() {
    // 00:00:F0 is registered to "Samsung Electronics Co.,Ltd" in the IEEE MA-L database.
    let result = lookup_manufacturer("00:00:F0:AA:BB:CC");
    assert_eq!(result, Some("Samsung Electronics Co.,Ltd"));
}

#[test]
fn known_espressif_prefix() {
    // 24:0A:C4 is registered to "Espressif Inc." in the IEEE MA-L database.
    let result = lookup_manufacturer("24:0A:C4:12:34:56");
    assert_eq!(result, Some("Espressif Inc."));
}

#[test]
fn unknown_prefix_returns_none() {
    // Use a prefix with the locally-administered bit cleared that is unassigned
    // in the current IEEE database. 01:00:00 is a multicast prefix, not in MA-L.
    let result = lookup_manufacturer("01:00:00:00:00:00");
    assert_eq!(result, None);
}

#[test]
fn randomized_mac_detected() {
    // Bit 1 (0x02) of the first byte set means locally administered / randomized.
    // 0x02 has the bit set; 0xFE does too.
    assert!(is_randomized_mac("02:11:22:33:44:55"));
    assert!(is_randomized_mac("FE:FF:FF:00:00:00"));

    // A burnt-in address does not.
    assert!(!is_randomized_mac("F0:EE:7A:00:11:22"));

    // Garbage is not a randomized MAC — it is not a MAC at all.
    assert!(!is_randomized_mac("not-a-mac"));
}

#[test]
fn randomized_mac_has_no_manufacturer() {
    // A privacy MAC is self-assigned, so it has no registered OUI. It must not
    // yield a manufacturer string — that conflated "how it presents itself"
    // with "who built it" (issue #1099).
    assert_eq!(lookup_manufacturer("02:11:22:33:44:55"), None);
    assert_eq!(lookup_manufacturer("FE:FF:FF:00:00:00"), None);
}

#[test]
fn private_ieee_listing_returns_none() {
    // 5C-E7-53 is listed in the IEEE database as the literal token "Private" —
    // the registrant paid to hide their name. Surfacing "Private" as the
    // manufacturer looked like real data while identifying nothing, which is
    // the case reported in issue #1099.
    assert_eq!(lookup_manufacturer("5c:e7:53:4e:ec:db"), None);
}

#[test]
fn registration_authority_filler_returns_none() {
    // B8-4C-87 is an MA-M/MA-S parent block listed to "IEEE Registration
    // Authority". The real assignee sits behind a 28- or 36-bit prefix that a
    // 24-bit OUI lookup cannot resolve, so we must not name the registry.
    assert_eq!(lookup_manufacturer("b8:4c:87:00:11:22"), None);
}

#[test]
fn invalid_mac_returns_none() {
    assert_eq!(lookup_manufacturer("not-a-mac"), None);
    assert_eq!(lookup_manufacturer(""), None);
    assert_eq!(lookup_manufacturer("AA:BB"), None);
}

#[test]
fn guess_apple_is_phone() {
    assert_eq!(guess_device_type("Apple, Inc."), DeviceType::Phone);
}

#[test]
fn guess_samsung_is_phone() {
    assert_eq!(
        guess_device_type("Samsung Electronics Co.,Ltd"),
        DeviceType::Phone
    );
}

#[test]
fn guess_lg_electronics_is_tv() {
    assert_eq!(guess_device_type("LG Electronics"), DeviceType::Tv);
}

#[test]
fn guess_espressif_is_iot() {
    assert_eq!(guess_device_type("Espressif Inc."), DeviceType::Iot);
}

#[test]
fn guess_tp_link_is_iot() {
    assert_eq!(
        guess_device_type("TP-LINK TECHNOLOGIES CO.,LTD."),
        DeviceType::Iot
    );
}

#[test]
fn guess_microsoft_is_laptop() {
    assert_eq!(guess_device_type("Microsoft"), DeviceType::Laptop);
}

#[test]
fn guess_nintendo_is_game_console() {
    assert_eq!(guess_device_type("Nintendo"), DeviceType::GameConsole);
}

#[test]
fn guess_sony_interactive_is_game_console() {
    assert_eq!(
        guess_device_type("Sony Interactive Entertainment"),
        DeviceType::GameConsole
    );
}

#[test]
fn guess_unknown_manufacturer() {
    assert_eq!(guess_device_type(""), DeviceType::Unknown);
    assert_eq!(guess_device_type("SomeRandomBrand"), DeviceType::Unknown);
}

#[test]
fn oui_database_has_many_entries() {
    // Verify the full IEEE database was loaded -- it should have at least
    // 30,000 entries (the CSV has ~39,000 rows).
    use crate::oui::OUI_MAP;
    assert!(
        OUI_MAP.len() > 30_000,
        "Expected >30k OUI entries, got {}",
        OUI_MAP.len()
    );
}
