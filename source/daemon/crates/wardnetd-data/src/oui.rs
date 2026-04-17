use std::collections::HashMap;
use std::sync::LazyLock;

use wardnet_common::device::DeviceType;

// Include the generated OUI data from the build script.
// This contains ~39,000 entries from the IEEE MA-L database.
include!(concat!(env!("OUT_DIR"), "/oui_data.rs"));

/// Static map from OUI prefix (3 bytes) to manufacturer name.
pub(crate) static OUI_MAP: LazyLock<HashMap<[u8; 3], &'static str>> = LazyLock::new(|| {
    let mut map = HashMap::with_capacity(OUI_ENTRIES.len());
    for &(prefix, name) in OUI_ENTRIES {
        map.insert(prefix, name);
    }
    map
});

/// Look up the manufacturer for a MAC address by its OUI prefix (first 3 bytes).
///
/// MAC must be in normalized format "AA:BB:CC:DD:EE:FF".
/// Returns the manufacturer name if the OUI prefix is known.
///
/// If the MAC has the locally-administered bit set (bit 1 of the first byte),
/// it is a randomized/private MAC and `"Randomized MAC"` is returned.
pub fn lookup_manufacturer(mac: &str) -> Option<&'static str> {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let b0 = u8::from_str_radix(parts[0], 16).ok()?;
    let b1 = u8::from_str_radix(parts[1], 16).ok()?;
    let b2 = u8::from_str_radix(parts[2], 16).ok()?;

    // Locally administered bit (bit 1 of first byte) indicates a randomized MAC.
    if b0 & 0x02 != 0 {
        return Some("Randomized MAC");
    }

    let prefix = [b0, b1, b2];
    OUI_MAP.get(&prefix).copied()
}

/// Guess the device type based on the manufacturer name.
///
/// Uses simple substring matching to categorize devices.
#[must_use]
pub fn guess_device_type(manufacturer: &str) -> DeviceType {
    let lower = manufacturer.to_lowercase();

    // Randomized MACs are typically phones with MAC randomization enabled.
    if lower.contains("randomized") {
        return DeviceType::Phone;
    }

    // Game consoles (check before generic brand matches).
    if lower.contains("nintendo") {
        return DeviceType::GameConsole;
    }
    if lower.contains("sony interactive") {
        return DeviceType::GameConsole;
    }

    // Phones.
    if lower.contains("apple")
        || lower.contains("samsung")
        || lower.contains("google")
        || lower.contains("huawei")
        || lower.contains("xiaomi")
        || lower.contains("oneplus")
        || lower.contains("motorola")
    {
        return DeviceType::Phone;
    }

    // TVs.
    if lower.contains("lg electronics")
        || lower.contains("sony")
        || lower.contains("vizio")
        || lower.contains("tcl")
        || lower.contains("hisense")
    {
        return DeviceType::Tv;
    }

    // Laptops / desktops.
    if lower.contains("intel")
        || lower.contains("dell")
        || lower.contains("hp")
        || lower.contains("lenovo")
        || lower.contains("asus")
        || lower.contains("microsoft")
    {
        return DeviceType::Laptop;
    }

    // IoT / networking devices.
    if lower.contains("amazon")
        || lower.contains("espressif")
        || lower.contains("tuya")
        || lower.contains("shenzhen")
        || lower.contains("raspberry")
        || lower.contains("tp-link")
    {
        return DeviceType::Iot;
    }

    DeviceType::Unknown
}
