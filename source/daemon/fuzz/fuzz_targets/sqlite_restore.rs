#![no_main]
//! Fuzz target: feed arbitrary bytes to `SqliteDumper::restore` and
//! watch for panics.
//!
//! `restore` writes the bytes to disk, reopens the file as SQLite, and
//! reads the `_sqlx_migrations` table. Invalid SQLite files fail fast
//! (magic-byte check); valid-looking-but-malformed ones drive deeper
//! into sqlx / rusqlite parsing.
//!
//! Two perf-relevant choices:
//! - The placeholder `SqlitePool` is hoisted into a `Lazy` static.
//!   `SqliteDumper::new` takes a pool but `restore` reconnects
//!   internally to the target path, so the same in-memory pool is
//!   reusable across iterations. Recreating it per iteration paid
//!   ~10ms of connection setup + a slow async drop that leaks file
//!   descriptors under libfuzzer's loop.
//! - A single per-process target path (overwritten each call) keeps
//!   disk footprint constant.

use libfuzzer_sys::fuzz_target;
use once_cell::sync::Lazy;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use tokio::runtime::Runtime;
use wardnetd_data::database_dumper::{DatabaseDumper, SqliteDumper};

static RUNTIME: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("runtime init"));

static POOL: Lazy<SqlitePool> = Lazy::new(|| {
    RUNTIME.block_on(async {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("placeholder pool")
    })
});

fn fuzz_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("wardnet-fuzz-restore-{}.db", std::process::id()))
}

fuzz_target!(|data: &[u8]| {
    RUNTIME.block_on(async {
        // `SqlitePool` is `Arc`-backed internally, so `clone()` is cheap
        // and the real pool underneath is shared across every fuzz
        // iteration in this process.
        let dumper = SqliteDumper::new(POOL.clone(), fuzz_db_path());
        let _ = dumper.restore(data).await;
    });
});
