use crate::output::{bytes, duration, opt, timestamp};

#[test]
fn bytes_scales_units() {
    assert_eq!(bytes(0), "0 B");
    assert_eq!(bytes(512), "512 B");
    assert_eq!(bytes(1024), "1.0 KiB");
    assert_eq!(bytes(1536), "1.5 KiB");
    assert_eq!(bytes(1_048_576), "1.0 MiB");
}

#[test]
fn duration_composes_units() {
    assert_eq!(duration(0), "0s");
    assert_eq!(duration(59), "59s");
    assert_eq!(duration(60), "1m");
    assert_eq!(duration(3_661), "1h 1m 1s");
    assert_eq!(duration(90_061), "1d 1h 1m 1s");
}

#[test]
fn opt_and_timestamp_render_dash_when_absent() {
    assert_eq!(opt(None), "-");
    assert_eq!(opt(Some("x")), "x");
    assert_eq!(timestamp(None), "-");
}
