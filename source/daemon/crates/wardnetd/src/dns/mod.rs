pub mod dot;
pub mod pipeline;
pub mod reply_capture;
pub mod server;

mod rate_limit;

// The bench seam (issue: DNS pipeline criterion benches). Compiled only under
// `cfg(test)` or the `bench-internals` feature — never in a production build.
// It re-exposes the crate-private query-path construction helpers and a set of
// in-process stubs so `benches/dns_resolution.rs` can drive `QueryPipeline`
// directly. Adds visibility only; production paths are untouched when the
// feature is off.
#[cfg(any(test, feature = "bench-internals"))]
pub mod bench_support;

#[cfg(test)]
mod tests;
