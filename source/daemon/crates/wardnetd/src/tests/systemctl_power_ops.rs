use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::system::SystemctlPowerOps;
use wardnetd_services::command::{CommandExecutor, CommandOutput};
use wardnetd_services::error::AppError;
use wardnetd_services::system::SystemPowerOps;

/// Recorded invocation of the executor — same shape the firewall
/// tests use, replicated locally so the two test suites can evolve
/// independently.
#[derive(Debug, Clone)]
struct RecordedCall {
    program: String,
    args: Vec<String>,
}

#[derive(Debug)]
struct MockExecutor {
    calls: Arc<Mutex<Vec<RecordedCall>>>,
    response: Mutex<CommandOutput>,
    /// When set, `run` returns `Err(io::Error)` instead of `Ok(...)`.
    spawn_error: Mutex<Option<std::io::ErrorKind>>,
}

impl MockExecutor {
    fn new(response: CommandOutput) -> (Self, Arc<Mutex<Vec<RecordedCall>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mock = Self {
            calls: Arc::clone(&calls),
            response: Mutex::new(response),
            spawn_error: Mutex::new(None),
        };
        (mock, calls)
    }

    fn with_spawn_error(kind: std::io::ErrorKind) -> (Self, Arc<Mutex<Vec<RecordedCall>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mock = Self {
            calls: Arc::clone(&calls),
            response: Mutex::new(success("")),
            spawn_error: Mutex::new(Some(kind)),
        };
        (mock, calls)
    }
}

fn success(stdout: &str) -> CommandOutput {
    CommandOutput {
        stdout: stdout.to_owned(),
        stderr: String::new(),
        success: true,
    }
}

fn failure(stderr: &str) -> CommandOutput {
    CommandOutput {
        stdout: String::new(),
        stderr: stderr.to_owned(),
        success: false,
    }
}

#[async_trait]
impl CommandExecutor for MockExecutor {
    async fn run(&self, program: &str, args: &[&str]) -> std::io::Result<CommandOutput> {
        self.calls.lock().await.push(RecordedCall {
            program: program.to_owned(),
            args: args.iter().map(|s| (*s).to_owned()).collect(),
        });

        if let Some(kind) = *self.spawn_error.lock().await {
            return Err(std::io::Error::new(kind, "mock spawn failure"));
        }
        Ok(self.response.lock().await.clone())
    }

    async fn run_with_stdin(
        &self,
        _program: &str,
        _args: &[&str],
        _stdin_data: &str,
    ) -> std::io::Result<CommandOutput> {
        unimplemented!("SystemctlPowerOps never invokes run_with_stdin")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reboot_invokes_systemctl_reboot_no_block() {
    let (executor, calls) = MockExecutor::new(success(""));
    let ops = SystemctlPowerOps::new(Arc::new(executor));

    ops.reboot().await.unwrap();

    let calls = calls.lock().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program, "systemctl");
    assert_eq!(calls[0].args, vec!["reboot", "--no-block"]);
}

#[tokio::test]
async fn poweroff_invokes_systemctl_poweroff_no_block() {
    let (executor, calls) = MockExecutor::new(success(""));
    let ops = SystemctlPowerOps::new(Arc::new(executor));

    ops.poweroff().await.unwrap();

    let calls = calls.lock().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program, "systemctl");
    assert_eq!(calls[0].args, vec!["poweroff", "--no-block"]);
}

#[tokio::test]
async fn reboot_propagates_nonzero_exit_as_internal_error() {
    let (executor, _calls) =
        MockExecutor::new(failure("Failed to reboot system via logind: Access denied"));
    let ops = SystemctlPowerOps::new(Arc::new(executor));

    let err = ops.reboot().await.expect_err("expected error on nonzero");
    match err {
        AppError::Internal(e) => {
            let msg = format!("{e}");
            assert!(msg.contains("systemctl reboot --no-block failed"));
            assert!(msg.contains("Access denied"));
        }
        other => panic!("expected AppError::Internal, got {other:?}"),
    }
}

#[tokio::test]
async fn poweroff_propagates_nonzero_exit_as_internal_error() {
    let (executor, _calls) = MockExecutor::new(failure("logind: not authorized"));
    let ops = SystemctlPowerOps::new(Arc::new(executor));

    let err = ops.poweroff().await.expect_err("expected error on nonzero");
    match err {
        AppError::Internal(e) => {
            let msg = format!("{e}");
            assert!(msg.contains("systemctl poweroff --no-block failed"));
            assert!(msg.contains("not authorized"));
        }
        other => panic!("expected AppError::Internal, got {other:?}"),
    }
}

#[tokio::test]
async fn reboot_propagates_spawn_error_as_internal_error() {
    let (executor, _calls) = MockExecutor::with_spawn_error(std::io::ErrorKind::NotFound);
    let ops = SystemctlPowerOps::new(Arc::new(executor));

    let err = ops
        .reboot()
        .await
        .expect_err("expected error on spawn fail");
    match err {
        AppError::Internal(e) => {
            let msg = format!("{e}");
            assert!(msg.contains("failed to spawn systemctl reboot"));
        }
        other => panic!("expected AppError::Internal, got {other:?}"),
    }
}
