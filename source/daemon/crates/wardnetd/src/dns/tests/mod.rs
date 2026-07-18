// Tests for the wardnetd DNS server were rewritten as part of issue #221
// when the hot path moved to `Arc<dyn DnsFilterService>`. End-to-end
// coverage of the resolver path lives under
// `source/end2end-tests/daemon/tests/`; the unit-level tests below
// exercise the lifecycle and the `record_query` / `duration_to_ms` /
// `upstream_label` helpers that don't need a fake `DnsSocket` plumbed.

mod rate_limit;
mod server;
