//! `wardnet-postupgrade-runner` entrypoint.
//!
//! Resolves to a process exit code and a single line of structured
//! tracing output per outcome. The systemd unit captures stdout/stderr
//! into journald with `SyslogIdentifier=wardnet-postupgrade`.

use std::ffi::CString;

use wardnet_postupgrade_runner::{RunOutcome, Runner};

/// Embedded at compile time via `build.rs`. The default points at
/// `deploy/keys/wardnet-release.pub`; the e2e test image overrides
/// `WARDNET_POSTUPGRADE_PUBKEY_PATH` at build time to a test pubkey.
const EMBEDDED_PUBLIC_KEY: &str = include_str!(env!("WARDNET_POSTUPGRADE_PUBKEY"));

fn main() {
    init_tracing();

    let runner = Runner::with_default_paths(EMBEDDED_PUBLIC_KEY);
    let argv = vec![CString::new("wardnet-postupgrade").expect("argv[0] has no NUL")];

    match runner.run(&argv) {
        RunOutcome::NoPayload => {
            tracing::info!(
                payload = %runner.payload_path.display(),
                "no postupgrade payload present at {payload}; nothing to do",
                payload = runner.payload_path.display(),
            );
            std::process::exit(0);
        }
        RunOutcome::VerifyFailed(e) => {
            tracing::error!(
                error = %e,
                payload = %runner.payload_path.display(),
                "postupgrade signature verification failed for {payload}: {e}",
                payload = runner.payload_path.display(),
            );
            std::process::exit(1);
        }
        RunOutcome::ExecFailed(e) => {
            tracing::error!(
                error = %e,
                "fexecve of postupgrade payload failed: {e}",
            );
            std::process::exit(2);
        }
    }
}

fn init_tracing() {
    // Compact format suits journald: one line, structured fields
    // visible in `journalctl -u wardnet-postupgrade`. No timestamp
    // (journald already prefixes one).
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_target(false)
        .without_time()
        .try_init();
}
