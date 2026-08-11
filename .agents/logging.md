# Logging Guidelines

When a log line includes structured fields, those key values **must** also appear in the message text. This ensures readability in both structured log aggregators (Loki, Grafana) and plain text output. Simple status messages without meaningful structured data (e.g. `"device detector shut down"`, `"using no-op network backends"`) are fine without structured fields.

## Pattern

```rust
// CORRECT — fields in both structured args AND message text (named params)
tracing::info!(mac = %obs.mac, ip = %obs.ip, "device detected: mac={mac}, ip={ip}", mac = obs.mac, ip = obs.ip);
tracing::warn!(error = %e, interface = %iface, "ARP scan failed on {iface}: {e}");
tracing::debug!(count, "flushed last_seen timestamps: count={count}");

// CORRECT — simple status message, no structured fields needed
tracing::info!("device detector shut down");

// WRONG — fields only in structured args (message is opaque in plain text)
tracing::info!(mac = %obs.mac, ip = %obs.ip, "device detected");

// WRONG — fields only in message text (not queryable in structured logs)
tracing::info!("device detected: mac={mac}, ip={ip}", mac = obs.mac, ip = obs.ip);
```

## Rules

1. Always use `tracing` macros (`tracing::info!`, `tracing::warn!`, etc.), never `log` or `println!`.
2. Structured fields go first: `field = %value` or `field = value` (for Display vs Debug).
3. The message string repeats key values using tracing's `{variable}` interpolation syntax (resolved at the macro level, zero-cost when level is disabled).
4. `error` level — always capture the error as a structured field (`error = %e`).
   Interpolating it into the message text as well (`"operation failed on {thing}: {e}"`)
   is optional, and worth doing for one-off or rare errors. The structured field is
   what Loki queries key off, so it is the part that must never be omitted.
5. `warn` level — include enough context to diagnose: what failed, which entity, the error.
6. `info` level — include the primary identifiers: MAC, IP, device_id, interface, etc.
7. `debug` level — include counts and operational details: `"flushed {count} timestamps"`.
8. `trace` level — rarely used, for packet-level details during development.

## Where a line ends up

Three sinks, three different filters — a line that is "logged" is not
necessarily a line anyone will see:

| Sink | Carries |
|---|---|
| Rotating log file + `OTel` | everything the subscriber's `EnvFilter` admits (INFO and above by default) |
| Admin UI live stream | the same, minus `logging.ui_suppressed_targets` |
| stderr → journald (`journalctl -u wardnetd`) | ERROR always; WARN minus `logging.journal_suppressed_targets`; INFO **only** from `logging.journal_info_targets` |

The journal is the narrow one, and it is what an operator reads on a box
they are debugging. If you are writing the line that answers *"is this
background job still running?"*, an `info!` is not enough on its own —
add the module's target prefix to `journal_info_targets`
(`wardnet-common/src/config.rs`).

Keep that list to targets emitting a **bounded, countable** number of
INFO events per day. Per-request or per-query lines belong in the log
file; a target added here at DEBUG-adjacent volume evicts other units'
logs from the journal.

## Report the boring outcome

A periodic job that only logs when something goes wrong is
indistinguishable from one that has stopped running. Log the successful
case too, with the numbers that let a reader tell "nothing to do" from
"could not do anything" — see `run_vacuum` in
`wardnetd-services/src/db_maintenance_runner.rs`, which reports pages
reclaimed, the freelist on both sides, and why the loop stopped, every
run, including the runs that reclaim nothing.

## Performance

Tracing macros are zero-cost when the level is filtered out. The level check happens first — if disabled, no arguments are evaluated, no strings are formatted.
