//! Key storage abstraction (re-exported from `wardnetd-data`).
//!
//! Re-exported here so crates that depend on `wardnetd-services` do not
//! need a direct dependency on `wardnetd-data`.
pub use wardnetd_data::keys::{FileKeyStore, KeyStore};
