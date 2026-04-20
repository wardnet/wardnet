//! Cron expression parsing for blocklist schedules.
//!
//! The Rust `cron` crate requires 6-field expressions (seconds included),
//! but the web UI and most operator-facing tooling use 5-field POSIX cron
//! (`min hour dom mon dow`). This helper accepts either form and always
//! returns a [`cron::Schedule`] — 5-field input gets an implicit `0` for
//! the seconds position.

use std::str::FromStr;

/// Parse a cron expression in either 5-field POSIX or 6-field extended form.
///
/// 5-field input is normalized by prepending a `0` seconds field. Any other
/// field count is passed through unchanged so the underlying parser emits
/// its usual error.
pub fn parse_schedule(expr: &str) -> Result<cron::Schedule, cron::error::Error> {
    let trimmed = expr.trim();
    let field_count = trimmed.split_whitespace().count();
    if field_count == 5 {
        let extended = format!("0 {trimmed}");
        cron::Schedule::from_str(&extended)
    } else {
        cron::Schedule::from_str(trimmed)
    }
}
