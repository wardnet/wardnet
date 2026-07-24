//! Small formatting helpers shared by the command modules.
//!
//! Every command supports two renderings: a human-readable one and, under the
//! global `--json` flag, the response serialized verbatim so it can be piped
//! into `jq` and friends.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::CliError;

/// Print a value as pretty JSON on stdout.
///
/// # Errors
/// Returns [`CliError::Protocol`] if the value cannot be serialized, which for
/// the response types in this crate should never happen.
pub fn print_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|e| CliError::Protocol(format!("failed to render JSON: {e}")))?;
    println!("{rendered}");
    Ok(())
}

/// Render an optional timestamp as RFC 3339, or `-` when absent.
#[must_use]
pub fn timestamp(value: Option<DateTime<Utc>>) -> String {
    value.map_or_else(|| "-".to_owned(), |t| t.to_rfc3339())
}

/// Render an optional string, or `-` when absent.
#[must_use]
pub fn opt(value: Option<&str>) -> String {
    value.map_or_else(|| "-".to_owned(), str::to_owned)
}

/// Human-readable byte count (binary units).
#[must_use]
pub fn bytes(mut value: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if value < 1024 {
        return format!("{value} B");
    }
    let mut unit = 0;
    let mut rem = 0u64;
    while value >= 1024 && unit < UNITS.len() - 1 {
        rem = value % 1024;
        value /= 1024;
        unit += 1;
    }
    // One decimal place, derived from the remainder of the final division.
    let tenths = rem * 10 / 1024;
    format!("{value}.{tenths} {}", UNITS[unit])
}

/// Human-readable duration from a whole number of seconds (`1d 2h 3m 4s`).
#[must_use]
pub fn duration(mut secs: u64) -> String {
    if secs == 0 {
        return "0s".to_owned();
    }
    let days = secs / 86_400;
    secs %= 86_400;
    let hours = secs / 3_600;
    secs %= 3_600;
    let mins = secs / 60;
    secs %= 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if mins > 0 {
        parts.push(format!("{mins}m"));
    }
    if secs > 0 {
        parts.push(format!("{secs}s"));
    }
    parts.join(" ")
}
