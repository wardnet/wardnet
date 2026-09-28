//! Egress-path probe outcomes. The two stages are what distinguish a dead path
//! from one that establishes and then stalls, so the predicates that tell them
//! apart are pinned here.

use crate::egress_path::{DIRECT_PATH, PathProbeOutcome, ProbeStage, StageOutcome};

fn outcome(connect_ok: bool, transfer: Option<bool>) -> PathProbeOutcome {
    PathProbeOutcome {
        connect: if connect_ok {
            StageOutcome::succeeded(12)
        } else {
            StageOutcome::failed("connection refused")
        },
        transfer: transfer.map(|ok| {
            if ok {
                StageOutcome::succeeded(48)
            } else {
                StageOutcome::failed("timed out mid-handshake")
            }
        }),
    }
}

#[test]
fn stage_slugs_are_stable() {
    assert_eq!(ProbeStage::Connect.as_str(), "connect");
    assert_eq!(ProbeStage::Transfer.as_str(), "transfer");
}

#[test]
fn a_succeeded_stage_carries_its_latency_and_no_error() {
    let stage = StageOutcome::succeeded(144);

    assert!(stage.ok);
    assert_eq!(stage.latency_ms, Some(144));
    assert!(stage.error.is_none());
}

/// A failed stage has no latency to report; a missing measurement is
/// meaningfully different from a zero one.
#[test]
fn a_failed_stage_carries_its_error_and_no_latency() {
    let stage = StageOutcome::failed("no route to host");

    assert!(!stage.ok);
    assert!(stage.latency_ms.is_none());
    assert_eq!(stage.error.as_deref(), Some("no route to host"));
}

#[test]
fn a_path_that_cannot_connect_reads_as_unreachable_only() {
    let result = outcome(false, None);

    assert!(result.connect_failed());
    assert!(
        !result.degraded(),
        "a dead path has no transfer stage to judge, so one fault raises one anomaly"
    );
}

/// The signature the whole two-stage design exists for: the handshake completes
/// — so a connect-only probe calls this healthy — and the transfer does not.
#[test]
fn a_path_that_connects_then_stalls_reads_as_degraded_only() {
    let result = outcome(true, Some(false));

    assert!(result.degraded());
    assert!(!result.connect_failed());
}

#[test]
fn a_working_path_reads_as_neither() {
    let result = outcome(true, Some(true));

    assert!(!result.connect_failed());
    assert!(!result.degraded());
}

/// A path probed before its transfer stage ran is not yet degraded — absence of
/// a verdict is not a bad verdict.
#[test]
fn a_connect_only_result_is_not_degraded() {
    let result = outcome(true, None);

    assert!(!result.connect_failed());
    assert!(!result.degraded());
}

/// The WAN is not a row anywhere, so its subject id is a literal that has to
/// stay stable across restarts.
#[test]
fn the_direct_path_has_a_stable_identifier() {
    assert_eq!(DIRECT_PATH, "direct");
}
