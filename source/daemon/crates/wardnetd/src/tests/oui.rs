use crate::oui::{guess_device_type, lookup_manufacturer};
use wardnet_types::device::DeviceType;

#[test]
fn known_apple_prefix() {
    let result = lookup_manufacturer("AC:DE:48:00:11:22");
    assert_eq!(result, Some("Apple"));
}

#[test]
fn known_samsung_prefix() {
    let result = lookup_manufacturer("00:00:F0:AA:BB:CC");
    assert_eq!(result, Some("Samsung"));
}

#[test]
fn known_espressif_prefix() {
    let result = lookup_manufacturer("24:0A:C4:12:34:56");
    assert_eq!(result, Some("Espressif"));
}

#[test]
fn unknown_prefix_returns_none() {
    let result = lookup_manufacturer("FF:FF:FF:00:00:00");
    assert_eq!(result, None);
}

#[test]
fn invalid_mac_returns_none() {
    assert_eq!(lookup_manufacturer("not-a-mac"), None);
    assert_eq!(lookup_manufacturer(""), None);
    assert_eq!(lookup_manufacturer("AA:BB"), None);
}

#[test]
fn guess_apple_is_phone() {
    assert_eq!(guess_device_type("Apple"), DeviceType::Phone);
}

#[test]
fn guess_lg_electronics_is_tv() {
    assert_eq!(guess_device_type("LG Electronics"), DeviceType::Tv);
}

#[test]
fn guess_espressif_is_iot() {
    assert_eq!(guess_device_type("Espressif"), DeviceType::Iot);
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
