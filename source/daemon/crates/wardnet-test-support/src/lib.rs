//! Shared helpers for daemon unit tests.
//!
//! Kept in one crate so tests across the workspace don't each reinvent the
//! same plumbing. Consumed as a `[dev-dependencies]` entry.

use std::cell::RefCell;
use std::io;
use std::sync::{Arc, Mutex, Once};

use tracing_subscriber::fmt::MakeWriter;

pub mod principal;

/// Where the current thread's capture, if any, wants its log lines.
struct Sink {
    buf: Arc<Mutex<Vec<u8>>>,
    max_level: tracing::Level,
}

thread_local! {
    /// The capture installed on this thread, set by [`capture_logs`] and
    /// cleared when the returned [`LogCapture`] drops.
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
}

/// The writer end of a capture: either this thread's buffer, or nowhere.
enum RoutedWriter {
    Buf(Arc<Mutex<Vec<u8>>>),
    Discard,
}

impl io::Write for RoutedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Self::Buf(sink) = self {
            sink.lock().unwrap().extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Routes each formatted log line to the capture installed on the thread that
/// emitted it, discarding lines from threads that aren't capturing.
struct ThreadRouter;

impl ThreadRouter {
    /// `None` means "no capture on this thread", so the line is dropped.
    fn writer_for(level: Option<&tracing::Level>) -> RoutedWriter {
        SINK.with(|sink| {
            // `try_borrow` because a log emitted while `capture_logs` or the
            // drop glue holds the mutable borrow must not panic.
            match sink.try_borrow().as_deref() {
                Ok(Some(sink)) if level.is_none_or(|level| *level <= sink.max_level) => {
                    RoutedWriter::Buf(Arc::clone(&sink.buf))
                }
                _ => RoutedWriter::Discard,
            }
        })
    }
}

impl<'a> MakeWriter<'a> for ThreadRouter {
    type Writer = RoutedWriter;

    fn make_writer(&'a self) -> Self::Writer {
        Self::writer_for(None)
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        Self::writer_for(Some(meta.level()))
    }
}

/// A live in-memory `tracing` capture. Keep it in scope for the duration of
/// the logging under test — the thread's capture is uninstalled when it drops
/// — then read the accumulated output with [`LogCapture::contents`].
///
/// Only events emitted on the thread that called [`capture_logs`] are
/// recorded, so this does not compose with
/// `#[tokio::test(flavor = "multi_thread")]`.
pub struct LogCapture {
    buf: Arc<Mutex<Vec<u8>>>,
    prev: Option<Sink>,
}

impl LogCapture {
    /// The formatted log lines emitted since the capture was installed.
    #[must_use]
    pub fn contents(&self) -> String {
        String::from_utf8(self.buf.lock().unwrap().clone()).unwrap()
    }
}

impl Drop for LogCapture {
    fn drop(&mut self) {
        SINK.with(|sink| {
            *sink.borrow_mut() = self.prev.take();
        });
    }
}

/// Install the one process-wide subscriber that every capture routes through.
///
/// This has to be a *global* subscriber, and it has to stay installed for the
/// life of the process. `tracing` caches each callsite's `Interest`
/// process-wide, computed by folding over the dispatchers registered at the
/// moment the callsite is first reached, and that fold bottoms out at
/// `Interest::never()` when no dispatcher is registered. A thread-local
/// subscriber (`tracing::subscriber::set_default`) is only registered while its
/// guard is alive, so under `cargo test` — many threads, one capture — whichever
/// test reaches a callsite first while no capture is running pins it to `never`
/// for the whole process, and the capturing test then sees an empty buffer.
/// Re-running `rebuild_interest_cache()` afterwards does not reliably undo it:
/// registration happens later, at event time, on whichever thread gets there
/// first.
///
/// With a permanently-registered global subscriber the fold can never be empty,
/// so no callsite is ever pinned to `never`, and per-thread routing is left to
/// [`ThreadRouter`] instead of to the dispatcher.
fn install_global_subscriber() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            // Permissive here; the per-capture level is applied in
            // `ThreadRouter::writer_for`.
            .with_max_level(tracing::Level::TRACE)
            .without_time()
            .with_writer(ThreadRouter)
            .finish();
        // A binary under test may legitimately have installed its own global
        // subscriber already; nothing to do if so.
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

/// Install an in-memory log capture on the current thread up to `max_level`.
///
/// Lets a test assert on the *rendered message text* a log line produces — the
/// part that must carry interpolated field values per `.agents/logging.md`,
/// not just the structured span fields.
///
/// Captures nest: dropping the returned guard restores whatever capture was
/// installed on this thread before it.
#[must_use]
pub fn capture_logs(max_level: tracing::Level) -> LogCapture {
    install_global_subscriber();
    let buf = Arc::new(Mutex::new(Vec::new()));
    let prev = SINK.with(|sink| {
        sink.borrow_mut().replace(Sink {
            buf: Arc::clone(&buf),
            max_level,
        })
    });
    LogCapture { buf, prev }
}
