//! Three-layer watchdog (issue #214).
//!
//! | Layer | Trigger | Mechanism | Recovery |
//! |---|---|---|---|
//! | `HealthMonitor` | *Y* consecutive check failures | [`wardnetd_services::HealthMonitor`] | reports status |
//! | **soft** | overall health DOWN **or** snapshot stale | withhold `sd_notify(WATCHDOG=1)` ⇒ `WatchdogSec=15` | systemd restarts the *service* |
//! | **hard** | total runtime freeze (health loop itself can't run) | `/dev/watchdog`, pet **ungated** | kernel reboots the *host* |
//!
//! The soft layer ([`soft`]) is health-gated and proportionate; the hard layer
//! ([`hard`]) is the never-gated backstop. See the module docs of each and
//! `docs/adr/0001-watchdog-and-health.md`.

pub mod hard;
pub mod soft;

pub use hard::HardwareWatchdogRunner;
pub use soft::{Notifier, SdNotifier, SoftWatchdogRunner};
