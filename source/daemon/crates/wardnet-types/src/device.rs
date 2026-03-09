use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The type/category of a network device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Tv,
    Phone,
    Laptop,
    Tablet,
    GameConsole,
    SettopBox,
    Iot,
    Unknown,
}

/// A discovered network device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: Uuid,
    pub mac: String,
    pub name: Option<String>,
    pub hostname: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: DeviceType,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub last_ip: String,
    pub admin_locked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_type_round_trip() {
        for variant in [
            DeviceType::Tv,
            DeviceType::Phone,
            DeviceType::Laptop,
            DeviceType::Tablet,
            DeviceType::GameConsole,
            DeviceType::SettopBox,
            DeviceType::Iot,
            DeviceType::Unknown,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let back: DeviceType = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back);
        }
    }

    #[test]
    fn device_type_rename_snake_case() {
        assert_eq!(
            serde_json::to_string(&DeviceType::GameConsole).unwrap(),
            "\"game_console\""
        );
    }

    #[test]
    fn device_round_trip() {
        let device = Device {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            mac: "AA:BB:CC:DD:EE:01".to_owned(),
            name: Some("My Phone".to_owned()),
            hostname: None,
            manufacturer: Some("Apple".to_owned()),
            device_type: DeviceType::Phone,
            first_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
            last_seen: "2026-03-07T01:00:00Z".parse().unwrap(),
            last_ip: "192.168.1.10".to_owned(),
            admin_locked: false,
        };
        let json = serde_json::to_string(&device).unwrap();
        let back: Device = serde_json::from_str(&json).unwrap();
        assert_eq!(device, back);
    }
}
