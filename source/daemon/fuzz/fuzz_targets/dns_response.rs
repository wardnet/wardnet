#![no_main]
//! Fuzz target: feed arbitrary bytes to the DNS upstream-response
//! parse+classify core and watch for panics.
//!
//! Calls `parse_and_classify_response_for_fuzz` directly — the
//! feature-gated sync core the forwarders run on every datagram they
//! receive from an upstream resolver (`forward_via_tunnel` and the
//! conditional forwarder). It is the trust boundary the README calls
//! out: bytes an upstream — or an off-path attacker who beats it back
//! to our socket — can shape, decoded by `Message::from_bytes` and
//! then walked by our own `classify_response` (answer/authority
//! sections + SOA-TTL arithmetic for negative caching).
//!
//! Most random inputs fail the wire-format decode (cheap) — that
//! exercises the relay-raw-and-skip-caching branch. Inputs the
//! mutator drives into a valid-enough message reach the classifier,
//! which is where a panic in our TTL/section handling would surface.
//!
//! No async, no I/O: the socket recv and cache insert are stripped
//! away so libfuzzer saturates the parser at high throughput.

use libfuzzer_sys::fuzz_target;
use wardnetd_services::dns::response::parse_and_classify_response_for_fuzz;

fuzz_target!(|data: &[u8]| {
    let _ = parse_and_classify_response_for_fuzz(data);
});
