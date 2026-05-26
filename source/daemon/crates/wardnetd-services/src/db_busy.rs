//! Shared helpers for handling `SQLite`'s `SQLITE_BUSY` / `SQLITE_LOCKED`
//! transient errors.
//!
//! Background writers (DNS query-log flush, stats flush, daily
//! maintenance) all sit on the same single-connection writer pool. The
//! pool's 30 s `busy_timeout` covers the common case, but when one
//! writer overruns — e.g. a long `incremental_vacuum` after a big
//! retention DELETE — another writer's next operation can still come
//! back with `database is locked`. These helpers wrap the offending
//! operation in a bounded retry loop so a transient lock window costs a
//! few hundred milliseconds rather than a dropped batch.

use std::time::Duration;

use tracing::warn;

/// Recognise a transient `SQLite` lock error.
///
/// Walks the `anyhow::Error` chain for a [`sqlx::Error::Database`] and
/// matches the `SQLite` primary result code: `5` (`SQLITE_BUSY`) or `6`
/// (`SQLITE_LOCKED`). Both indicate the operation can succeed if
/// retried after the conflicting writer releases its lock.
///
/// Falls back to a string match against the rendered error if the
/// underlying type can't be recovered (e.g. an error that was bounced
/// through `anyhow::anyhow!` and lost its source chain), so callers
/// retain coverage even when intermediate layers reshape the error.
#[must_use]
pub fn is_busy_error(err: &anyhow::Error) -> bool {
    if let Some(sqlx_err) = err.downcast_ref::<sqlx::Error>()
        && let sqlx::Error::Database(db_err) = sqlx_err
        && let Some(code) = db_err.code()
    {
        return code == "5" || code == "6";
    }
    let rendered = err.to_string();
    rendered.contains("database is locked") || rendered.contains("database table is locked")
}

/// Run `op` with bounded retry on `SQLITE_BUSY` / `SQLITE_LOCKED`.
///
/// Up to `retries` retries after the initial attempt, sleeping
/// `backoff` between each. Logs a `WARN` per retry with the
/// `operation` label so panels stay legible across runners. Any
/// non-busy error returns immediately.
pub async fn retry_on_busy<F, Fut, T>(
    operation: &'static str,
    retries: u32,
    backoff: Duration,
    mut op: F,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let mut attempt: u32 = 0;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if is_busy_error(&e) && attempt < retries => {
                attempt += 1;
                warn!(
                    operation,
                    attempt,
                    retries,
                    "SQLITE_BUSY; retrying after backoff: operation={operation}, attempt={attempt}/{retries}"
                );
                tokio::time::sleep(backoff).await;
            }
            Err(e) => return Err(e),
        }
    }
}
