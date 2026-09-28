//! Device events: the observational record behind the connectivity timeline.
//!
//! Every other per-device module answers how a device is *configured*. This one
//! answers what happened to it, by watching the event bus and appending to
//! `device_events`.

pub mod listener;
pub mod service;

pub use listener::{DeviceEventListener, intent_from_event};
pub use service::{
    DeviceEventIntent, DeviceEventService, DeviceEventServiceImpl, DeviceRef, IntentKind,
};

#[cfg(test)]
mod tests;
