// Tests for the wardnetd DNS server were rewritten as part of issue #221
// when the hot path moved to `Arc<dyn DnsFilterService>`. New e2e coverage
// lives under `source/end2end-tests/daemon/tests/`.
