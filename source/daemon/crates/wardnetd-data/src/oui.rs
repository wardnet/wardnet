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

/// Parse the first three bytes (the OUI) out of a normalised MAC.
///
/// MAC must be in normalised format "aa:bb:cc:dd:ee:ff" (issue #312).
/// Casing of the input is irrelevant — `from_str_radix` is case-insensitive.
fn oui_prefix(mac: &str) -> Option<[u8; 3]> {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let b0 = u8::from_str_radix(parts[0], 16).ok()?;
    let b1 = u8::from_str_radix(parts[1], 16).ok()?;
    let b2 = u8::from_str_radix(parts[2], 16).ok()?;
    Some([b0, b1, b2])
}

/// Whether the MAC has the locally-administered bit set (bit 1 of the first
/// byte) — i.e. it is a randomized/privacy MAC rather than a burnt-in address.
///
/// This is deliberately *not* a manufacturer (issue #1099). A privacy MAC tells
/// us how the device chose to present itself, not who built it, and the two
/// facts used to be conflated in a single `manufacturer` string. Callers store
/// this separately so the UI can badge the address without inventing a vendor.
#[must_use]
pub fn is_randomized_mac(mac: &str) -> bool {
    oui_prefix(mac).is_some_and(|[b0, _, _]| b0 & 0x02 != 0)
}

/// Look up the manufacturer for a MAC address by its OUI prefix (first 3 bytes).
///
/// Returns the manufacturer name if the OUI prefix is known. Returns `None` for
/// a randomized MAC (which has no meaningful OUI), for an unregistered prefix,
/// and for prefixes whose IEEE listing is a placeholder — see
/// `is_placeholder_org_name` in `build.rs`, which drops those rows so they are
/// absent from the table entirely.
pub fn lookup_manufacturer(mac: &str) -> Option<&'static str> {
    let prefix = oui_prefix(mac)?;

    // A locally-administered address is self-assigned, so its leading bytes are
    // not a registered OUI and any table hit would be coincidental.
    if prefix[0] & 0x02 != 0 {
        return None;
    }

    OUI_MAP.get(&prefix).copied()
}

/// Guess the device type based on the manufacturer name.
///
/// Uses simple substring matching to categorize devices.
#[must_use]
pub fn guess_device_type(manufacturer: &str) -> DeviceType {
    let lower = manufacturer.to_lowercase();

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
