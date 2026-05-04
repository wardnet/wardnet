//! Tests for `exec::exec_memfd`. The function only returns on
//! failure (success replaces the current process), so the only
//! reachable test path is "fexecve fails because the bytes are not
//! a valid executable image". The test still exercises the full
//! `memfd_create` + `write` + `fexecve` syscall chain.

use std::ffi::CString;

use crate::exec::exec_memfd;

#[test]
fn returns_err_when_payload_is_not_a_valid_executable() {
    // A handful of bytes that match no executable format (no #!,
    // no ELF magic, no PE magic). fexecve will fall through every
    // binfmt handler the kernel knows and return ENOEXEC.
    let bogus_payload = b"this-is-not-an-executable-image";
    let argv = [CString::new("test-payload").unwrap()];

    let err = exec_memfd("test-payload", bogus_payload, &argv)
        .expect_err("fexecve must fail on a non-executable payload");
    let chain = format!("{err:#}");
    assert!(
        chain.contains("fexecve failed"),
        "expected fexecve-failure message, got: {chain}"
    );
}

#[test]
fn returns_err_when_name_contains_nul() {
    let argv = [CString::new("p").unwrap()];
    let err = exec_memfd("bad\0name", b"_", &argv)
        .expect_err("memfd name with NUL must be rejected before any syscall");
    let chain = format!("{err:#}");
    assert!(
        chain.contains("NUL"),
        "expected NUL-validation error, got: {chain}"
    );
}
