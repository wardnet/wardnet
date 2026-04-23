/// Writes the current process PID to a file and removes it on drop.
///
/// A `PidfileGuard` is created by calling [`PidfileGuard::write`] and
/// held for the lifetime of the daemon process. Dropping the guard (on
/// clean shutdown) removes the file so stale PID files do not confuse
/// operators or monitoring tooling.
///
/// Errors from both write and remove are non-fatal: a failure to write
/// is returned to the caller (who should log it and continue); a failure
/// to remove on drop is printed to stderr (tracing may already be torn down).
pub struct PidfileGuard {
    path: std::path::PathBuf,
}

impl PidfileGuard {
    /// Write the current process PID to `path`.
    ///
    /// Creates or truncates the file. The file is written with a trailing
    /// newline to match conventional PID file format (`kill -TERM $(cat …)`).
    ///
    /// Returns an error if the file cannot be created or written. Callers
    /// should log the error and continue — a missing PID file is non-fatal.
    pub fn write(path: &std::path::Path) -> std::io::Result<Self> {
        let pid = std::process::id();
        std::fs::write(path, format!("{pid}\n"))?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for PidfileGuard {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            // NotFound is fine: the file was never written or already cleaned up.
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "wardnetd: failed to remove pidfile {}: {e}",
                    self.path.display()
                );
            }
        }
    }
}
