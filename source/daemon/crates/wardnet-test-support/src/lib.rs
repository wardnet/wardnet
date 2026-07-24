//! Shared helpers for daemon unit tests.
//!
//! Kept in one crate so tests across the workspace don't each reinvent the
//! same plumbing. Consumed as a `[dev-dependencies]` entry.

use std::io;
use std::sync::{Arc, Mutex};

use tracing::subscriber::DefaultGuard;
use tracing_subscriber::fmt::MakeWriter;

/// A `MakeWriter` that appends everything written to a shared byte buffer.
#[derive(Clone)]
struct BufWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for BufWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for BufWriter {
    type Writer = BufWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// A live in-memory `tracing` capture. Keep it in scope for the duration of
/// the logging under test — the subscriber is uninstalled when it drops — then
/// read the accumulated output with [`LogCapture::contents`].
pub struct LogCapture {
    buf: Arc<Mutex<Vec<u8>>>,
    _guard: DefaultGuard,
}

impl LogCapture {
    /// The formatted log lines emitted since the capture was installed.
    #[must_use]
    pub fn contents(&self) -> String {
        String::from_utf8(self.buf.lock().unwrap().clone()).unwrap()
    }
}

/// Install an in-memory log capture on the current thread up to `max_level`.
///
/// Lets a test assert on the *rendered message text* a log line produces — the
/// part that must carry interpolated field values per `.agents/logging.md`,
/// not just the structured span fields.
///
/// `#[tokio::test]` runs on a current-thread runtime, so the returned capture's
/// guard stays in effect across `.await` points within the same test body.
#[must_use]
pub fn capture_logs(max_level: tracing::Level) -> LogCapture {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(max_level)
        .without_time()
        .with_writer(BufWriter(buf.clone()))
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    LogCapture { buf, _guard: guard }
}
