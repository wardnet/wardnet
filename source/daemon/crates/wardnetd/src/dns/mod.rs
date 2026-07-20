pub mod dot;
pub mod pipeline;
pub mod reply_capture;
pub mod server;

mod rate_limit;

// The bench seam (issue: DNS pipeline criterion benches). Compiled only under
// the `bench-internals` feature — never in a production build, and NOT under a
// plain `cfg(test)` either: nothing in the test suite uses it (only the bench
// target does), so gating it on the feature alone keeps its bench-only stubs
// out of the coverage build (which runs with `cfg(test)` but without the
// feature) rather than counting as ~190 permanently-uncovered lines.
// It re-exposes the crate-private query-path construction helpers and a set of
// in-process stubs so `benches/dns_resolution.rs` can drive `QueryPipeline`
// directly. Adds visibility only; production paths are untouched when the
// feature is off.
#[cfg(feature = "bench-internals")]
pub mod bench_support;

#[cfg(test)]
mod tests;
