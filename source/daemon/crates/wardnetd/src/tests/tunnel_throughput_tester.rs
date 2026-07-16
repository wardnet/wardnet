use std::time::Duration;

use wardnetd_services::tunnel::throughput_tester::ThroughputError;

use crate::tunnel_throughput_tester::{StreamOutcome, aggregate_throughput};

fn outcome(bytes_in_window: u64, failed: bool) -> StreamOutcome {
    StreamOutcome {
        bytes_in_window,
        failed,
    }
}

#[test]
fn all_streams_succeed_sums_bytes() {
    let outcomes = vec![outcome(1_000_000, false), outcome(2_000_000, false)];
    let result = aggregate_throughput(&outcomes, Duration::from_secs(1)).unwrap();
    // (3_000_000 bytes * 8 bits) / 1e6 / 1s = 24 Mbps.
    assert!((result.mbps - 24.0).abs() < 1e-9);
}

#[test]
fn partial_failure_sums_only_successful_streams() {
    let outcomes = vec![outcome(1_000_000, false), outcome(999_999_999, true)];
    let result = aggregate_throughput(&outcomes, Duration::from_secs(1)).unwrap();
    // The failed stream's bytes must not count toward the total.
    assert!((result.mbps - 8.0).abs() < 1e-9);
}

#[test]
fn all_streams_failed_errors() {
    let outcomes = vec![outcome(0, true), outcome(0, true)];
    let err = aggregate_throughput(&outcomes, Duration::from_secs(1)).unwrap_err();
    assert!(matches!(err, ThroughputError::Download(_)));
}

#[test]
fn zero_bytes_in_window_errors_rather_than_reporting_zero_mbps() {
    let outcomes = vec![outcome(0, false)];
    let err = aggregate_throughput(&outcomes, Duration::from_secs(4)).unwrap_err();
    assert!(matches!(err, ThroughputError::Download(_)));
}

#[test]
fn zero_measure_window_errors_instead_of_dividing_by_zero() {
    let outcomes = vec![outcome(1_000_000, false)];
    let err = aggregate_throughput(&outcomes, Duration::from_secs(0)).unwrap_err();
    assert!(matches!(err, ThroughputError::Download(_)));
}

#[test]
fn empty_outcomes_errors_via_all_failed_vacuous_truth() {
    // `outcomes.iter().all(...)` on an empty slice is vacuously true, so
    // this still errors rather than silently reporting a measurement — but
    // `HttpThroughputTester::download` rejects `parallel_streams == 0`
    // before ever reaching this function, so callers see a distinct,
    // actionable message instead of this generic one.
    let outcomes: Vec<StreamOutcome> = vec![];
    let err = aggregate_throughput(&outcomes, Duration::from_secs(4)).unwrap_err();
    assert!(matches!(err, ThroughputError::Download(_)));
}

#[test]
fn longer_measure_window_yields_lower_mbps_for_same_bytes() {
    let outcomes = vec![outcome(1_000_000, false)];
    let short = aggregate_throughput(&outcomes, Duration::from_secs(1))
        .unwrap()
        .mbps;
    let long = aggregate_throughput(&outcomes, Duration::from_secs(4))
        .unwrap()
        .mbps;
    assert!(long < short);
}
