mod db_maintenance_runner;
mod entitlement;
mod error;
mod event;
mod health;
mod health_checks;
mod init;
mod jobs;
mod request_context;
mod subnet;
mod version;

use uuid::Uuid;

/// A MAC address derived from `id`, for test devices.
///
/// `devices.mac` is UNIQUE, so every device a test inserts needs a distinct
/// MAC. Deriving one from a single byte of the UUID leaves only 256 possible
/// addresses — two devices in one test then collide about 1 run in 256, which
/// is exactly the flake this replaced. A MAC is six bytes, so use six.
pub(crate) fn mac_from(id: Uuid) -> String {
    let b = id.as_bytes();
    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        b[10], b[11], b[12], b[13], b[14], b[15]
    )
}
