//! Anomaly detection: the registry, the detectors, and the engine that drives
//! them.
//!
//! See [`wardnet_common::anomaly`] for the catalogue and the domain types.
//!
//! The subsystem has two entry points, and the difference between them is the
//! whole design:
//!
//! * **Preventive** — the [`engine`] asks each detector how often it wants to
//!   run and calls its `detect` on that cadence. Use it for conditions that
//!   are *state* you can go and inspect.
//! * **Reactive** — the [`listener`] turns error-flavoured domain events into
//!   reports and submits them. Use it for conditions that are *events*, with
//!   nothing left to inspect afterwards.
//!
//! Both funnel into the same [`AnomalyService::submit`], which deduplicates on
//! `(type, subject)` and notifies only on the open. That is what makes an
//! alert fire once for a condition rather than once per observation.

pub mod detector;
pub mod detectors;
pub mod engine;
pub mod listener;
pub mod registry;
pub mod service;

pub use detector::AnomalyDetector;
pub use engine::AnomaliesDetectionEngine;
pub use listener::AnomalyListener;
pub use registry::{AnomalyDetectorRegistry, DetectorDeps, EnabledDetectors};
pub use service::{AnomalyService, AnomalyServiceImpl};

#[cfg(test)]
mod tests;
