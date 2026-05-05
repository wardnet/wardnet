//! Unit tests for the runner crate. Per project convention each
//! module owns a sibling test file; `lib.rs` declares this module
//! gated on `#[cfg(test)]`.

mod exec;
mod runner;
mod state;
mod swap;
mod verify;
