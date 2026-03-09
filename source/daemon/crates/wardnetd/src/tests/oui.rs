use crate::oui::{guess_device_type, lookup_manufacturer};
use wardnet_types::device::DeviceType;

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
    // 0x02 has the bit set.
    let result = lookup_manufacturer("02:11:22:33:44:55");
    assert_eq!(result, Some("Randomized MAC"));

    // 0xFE also has bit 1 set.
    let result = lookup_manufacturer("FE:FF:FF:00:00:00");
    assert_eq!(result, Some("Randomized MAC"));
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
fn guess_randomized_mac_is_phone() {
    assert_eq!(guess_device_type("Randomized MAC"), DeviceType::Phone);
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
