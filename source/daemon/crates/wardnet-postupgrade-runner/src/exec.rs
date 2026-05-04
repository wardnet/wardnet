//! `memfd_create` + `fexecve` exec strategy for the verified payload.
//!
//! Holding the bytes in an anonymous in-memory file means the wardnet
//! user cannot replace the payload between verify and exec, and there
//! is no executable file on disk for an attacker to swap. `MFD_CLOEXEC`
//! ensures the memfd is dropped if exec fails — no leak into a child
//! process.
//!
//! `fexecve` on Linux is implemented by glibc as `execveat(fd, "",
//! …, AT_EMPTY_PATH)` which does the right thing for memfds.

use std::convert::Infallible;
use std::ffi::CString;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd};

use anyhow::Context;

/// Write `payload` into a fresh memfd and `fexecve` it. On success this
/// function does not return — the current process is replaced by the
/// payload binary.
///
/// `name` is purely cosmetic (shows up in `/proc/<pid>/fd/`); the
/// kernel does not enforce uniqueness.
///
/// `args` becomes the new process's `argv`. Convention: include the
/// program name as `args[0]`.
pub fn exec_memfd(name: &str, payload: &[u8], args: &[CString]) -> anyhow::Result<Infallible> {
    let fd_name = CString::new(name).context("memfd name contained NUL")?;
    let raw_fd = unsafe { libc::memfd_create(fd_name.as_ptr(), libc::MFD_CLOEXEC) };
    if raw_fd < 0 {
        return Err(
            anyhow::Error::from(std::io::Error::last_os_error()).context("memfd_create failed")
        );
    }

    // SAFETY: memfd_create returned a valid fd we own.
    let mut memfile = unsafe { std::fs::File::from_raw_fd(raw_fd) };
    memfile
        .write_all(payload)
        .context("write payload into memfd failed")?;

    let envp: Vec<CString> = std::env::vars_os()
        .filter_map(|(k, v)| {
            let mut combined = k.into_encoded_bytes();
            combined.push(b'=');
            combined.extend_from_slice(v.as_encoded_bytes());
            CString::new(combined).ok()
        })
        .collect();

    let argv_ptrs: Vec<*const libc::c_char> = args
        .iter()
        .map(|s| s.as_ptr())
        .chain(std::iter::once(std::ptr::null()))
        .collect();
    let envp_ptrs: Vec<*const libc::c_char> = envp
        .iter()
        .map(|s| s.as_ptr())
        .chain(std::iter::once(std::ptr::null()))
        .collect();

    // SAFETY: argv/envp are valid until this call returns; the call
    // either replaces the process (no return) or sets errno and
    // returns -1.
    let rc = unsafe { libc::fexecve(memfile.as_raw_fd(), argv_ptrs.as_ptr(), envp_ptrs.as_ptr()) };
    debug_assert_eq!(rc, -1, "fexecve must not return on success");
    Err(anyhow::Error::from(std::io::Error::last_os_error()).context("fexecve failed"))
}
