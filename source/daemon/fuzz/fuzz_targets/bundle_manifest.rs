#![no_main]
//! Fuzz target: `serde_json::from_slice::<BundleManifest>` on
//! arbitrary bytes.
//!
//! Simplest and cheapest of the three targets — no async, no I/O, no
//! decryption. libfuzzer will saturate this at high throughput and
//! exercise the chrono `DateTime<Utc>` parser + serde's numeric-bound
//! checks on `schema_version` / `bundle_format_version` / `key_count`.

use libfuzzer_sys::fuzz_target;
use wardnet_common::backup::BundleManifest;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<BundleManifest>(data);
});
