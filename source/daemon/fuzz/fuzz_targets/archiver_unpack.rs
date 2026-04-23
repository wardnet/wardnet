#![no_main]
//! Fuzz target: feed arbitrary bytes to the synchronous bundle
//! unpacker and watch for panics.
//!
//! Calls `unpack_sync_for_fuzz` directly — the feature-gated sync
//! core of `AgeArchiver::unpack`. Going through the async public API
//! would cost a tokio runtime hop, a `spawn_blocking` call, and a
//! per-iteration `bytes.to_vec()` on libfuzzer's hottest path.
//!
//! Most random inputs fail at the age header check (very cheap) —
//! that exercises the decryption error path. Occasionally the mutator
//! stumbles into something that decrypts far enough to reach the gzip
//! / tar / manifest layers, which is where the interesting bugs
//! would be.

use libfuzzer_sys::fuzz_target;
use wardnetd_services::backup::archiver::unpack_sync_for_fuzz;

fuzz_target!(|data: &[u8]| {
    // A fixed passphrase keeps mutation focused on the byte stream.
    // Fuzzing the passphrase separately has low yield — age's scrypt
    // KDF dominates the error path regardless.
    let _ = unpack_sync_for_fuzz("fuzz-passphrase-placeholder-1234", data);
});
