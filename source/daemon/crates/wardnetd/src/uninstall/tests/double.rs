//! A recording [`UninstallOps`] so the orchestration can be driven without a
//! host to destroy.
//!
//! Mirrors the pattern in `shutdown/tests/doubles.rs`: every call is appended
//! to an ordered log, and each operation can be told to fail so the
//! decision branches are reachable.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::uninstall::ops::{SystemctlError, UninstallOps};

pub type CallLog = Arc<Mutex<Vec<String>>>;

/// How the fake host is configured.
#[allow(clippy::struct_excessive_bools)] // test toggles, not a domain state machine
pub struct FakeHost {
    pub log: CallLog,
    pub is_root: bool,
    /// Answer handed back from the confirmation prompt.
    pub prompt_answer: String,
    /// When false, `prompt` errors (no terminal).
    pub prompt_available: bool,
    /// Outcome of `systemctl stop`.
    pub stop_result: Option<SystemctlError>,
    /// Outcome of `systemctl is-active` — `Ok(())` means still running.
    pub is_active_result: Option<SystemctlError>,
    /// Paths that exist (as `symlink_metadata` would see them).
    pub present: Vec<String>,
    /// Paths that resolve (as `metadata` would see them).
    pub reachable: Vec<String>,
    /// `canonicalize` answer for the data directory.
    pub canonical: Option<PathBuf>,
    pub chown_fails: bool,
    pub delete_user_fails: bool,
    pub remove_path_fails: Vec<String>,
    /// Failures returned by the typed kernel teardown.
    pub teardown_failures: Vec<String>,
    /// Links reported before, and then after, the `ip link` sweep.
    pub links_before: Vec<String>,
    pub links_after: Vec<String>,
    pub links_error: bool,
}

impl Default for FakeHost {
    fn default() -> Self {
        Self {
            log: Arc::new(Mutex::new(Vec::new())),
            is_root: true,
            prompt_answer: "yes".to_owned(),
            prompt_available: true,
            // Default: the stop succeeds and the unit is then inactive (3).
            stop_result: None,
            is_active_result: Some(inactive()),
            present: Vec::new(),
            reachable: Vec::new(),
            canonical: None,
            chown_fails: false,
            delete_user_fails: false,
            remove_path_fails: Vec::new(),
            teardown_failures: Vec::new(),
            links_before: Vec::new(),
            links_after: Vec::new(),
            links_error: false,
        }
    }
}

/// `systemctl` exit 3 — unit inactive.
pub fn inactive() -> SystemctlError {
    SystemctlError::Status {
        args: "is-active --quiet wardnetd.service".to_owned(),
        code: Some(3),
        stderr: String::new(),
    }
}

/// `systemctl` exit 5 — unit not loaded.
pub fn not_loaded() -> SystemctlError {
    SystemctlError::Status {
        args: "stop wardnetd.service".to_owned(),
        code: Some(5),
        stderr: String::new(),
    }
}

/// A `systemctl` failure that says nothing about liveness.
pub fn indeterminate() -> SystemctlError {
    SystemctlError::Status {
        args: "is-active --quiet wardnetd.service".to_owned(),
        code: Some(1),
        stderr: "Failed to get properties: Connection timed out".to_owned(),
    }
}

/// systemd is not PID 1 here.
pub fn not_booted() -> SystemctlError {
    SystemctlError::Status {
        args: "is-active --quiet wardnetd.service".to_owned(),
        code: Some(1),
        stderr: "System has not been booted with systemd as init system".to_owned(),
    }
}

impl FakeHost {
    pub fn calls(&self) -> Vec<String> {
        self.log.lock().expect("poisoned").clone()
    }

    fn record(&self, call: impl Into<String>) {
        self.log.lock().expect("poisoned").push(call.into());
    }

    fn clone_err(err: &SystemctlError) -> SystemctlError {
        match err {
            SystemctlError::Spawn(e) => SystemctlError::Spawn(std::io::Error::new(e.kind(), "")),
            SystemctlError::Status { args, code, stderr } => SystemctlError::Status {
                args: args.clone(),
                code: *code,
                stderr: stderr.clone(),
            },
        }
    }
}

#[async_trait]
impl UninstallOps for FakeHost {
    fn canonicalize(&self, _path: &Path) -> Option<PathBuf> {
        self.canonical.clone()
    }

    fn path_present(&self, path: &Path) -> bool {
        self.present.iter().any(|p| Path::new(p) == path)
    }

    fn is_reachable(&self, path: &Path) -> bool {
        self.reachable.iter().any(|p| Path::new(p) == path)
    }

    fn remove_path(&self, path: &Path) -> anyhow::Result<()> {
        let shown = path.display().to_string();
        self.record(format!("remove_path:{shown}"));
        if self.remove_path_fails.contains(&shown) {
            anyhow::bail!("permission denied");
        }
        Ok(())
    }

    fn chown_root_recursive(&self, path: &Path) -> anyhow::Result<()> {
        self.record(format!("chown:{}", path.display()));
        if self.chown_fails {
            anyhow::bail!("chown exited with 1");
        }
        Ok(())
    }

    async fn stop_unit(&self, unit: &str) -> Result<(), SystemctlError> {
        self.record(format!("stop:{unit}"));
        match &self.stop_result {
            None => Ok(()),
            Some(e) => Err(Self::clone_err(e)),
        }
    }

    async fn unit_is_active(&self, unit: &str) -> Result<(), SystemctlError> {
        self.record(format!("is_active:{unit}"));
        match &self.is_active_result {
            None => Ok(()),
            Some(e) => Err(Self::clone_err(e)),
        }
    }

    async fn disable_unit(&self, unit: &str) {
        self.record(format!("disable:{unit}"));
    }

    async fn daemon_reload(&self) {
        self.record("daemon_reload");
    }

    async fn delete_user(&self, user: &str) -> anyhow::Result<()> {
        self.record(format!("delete_user:{user}"));
        if self.delete_user_fails {
            anyhow::bail!("userdel exited with 1");
        }
        Ok(())
    }

    async fn teardown_runtime_state(&self) -> Vec<String> {
        self.record("teardown_runtime_state");
        self.teardown_failures.clone()
    }

    async fn wardnet_links(&self) -> anyhow::Result<Vec<String>> {
        let n = self
            .log
            .lock()
            .expect("poisoned")
            .iter()
            .filter(|c| *c == "wardnet_links")
            .count();
        self.record("wardnet_links");
        if self.links_error {
            anyhow::bail!("`ip link show` exited with 1");
        }
        // First call enumerates, second confirms what survived.
        Ok(if n == 0 {
            self.links_before.clone()
        } else {
            self.links_after.clone()
        })
    }

    async fn delete_link(&self, name: &str) {
        self.record(format!("delete_link:{name}"));
    }

    fn is_root(&self) -> bool {
        self.is_root
    }

    fn prompt(&self, _prompt: &str) -> anyhow::Result<String> {
        self.record("prompt");
        if self.prompt_available {
            Ok(self.prompt_answer.clone())
        } else {
            anyhow::bail!("refusing to uninstall without --yes: no terminal to confirm on")
        }
    }
}
