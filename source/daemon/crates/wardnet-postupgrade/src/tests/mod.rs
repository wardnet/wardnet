//! Unit tests for the migration runner. Per project convention each
//! module owns a sibling test file; `lib.rs` declares this module
//! gated on `#[cfg(test)]`.

mod migrations;
mod runner;
mod state;
