use async_trait::async_trait;

/// Result of executing an OS command.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    /// Standard output as a string.
    pub stdout: String,
    /// Standard error as a string.
    pub stderr: String,
    /// Whether the command exited successfully (exit code 0).
    pub success: bool,
}

/// Abstraction over OS command execution.
///
/// Enables testing implementations that shell out to system tools (ip, nft, sysctl)
/// without executing actual commands. The production implementation uses
/// `tokio::process::Command`.
#[async_trait]
pub trait CommandExecutor: Send + Sync + std::fmt::Debug {
    /// Execute a command with the given arguments and return its output.
    async fn run(&self, program: &str, args: &[&str]) -> std::io::Result<CommandOutput>;

    /// Execute a command with stdin data piped in and return its output.
    async fn run_with_stdin(
        &self,
        program: &str,
        args: &[&str],
        stdin_data: &str,
    ) -> std::io::Result<CommandOutput>;
}

/// Search PATH including sbin directories that may be absent from non-login shells.
///
/// System tools like `ip`, `sysctl`, and `nft` live in `/usr/sbin` or `/sbin`,
/// which are not always on PATH when the daemon is started via SSH or systemd
/// with a restricted environment.
const SBIN_SEARCH_DIRS: &[&str] = &["/usr/sbin", "/sbin"];

/// Resolve a program name to its full path if it lives in an sbin directory.
///
/// Returns the original program name unchanged if no sbin match is found
/// (letting the OS search the regular PATH).
fn resolve_program(program: &str) -> std::borrow::Cow<'_, str> {
    // Already an absolute path — use as-is.
    if program.starts_with('/') {
        return std::borrow::Cow::Borrowed(program);
    }
    for dir in SBIN_SEARCH_DIRS {
        let candidate = format!("{dir}/{program}");
        if std::path::Path::new(&candidate).exists() {
            return std::borrow::Cow::Owned(candidate);
        }
    }
    std::borrow::Cow::Borrowed(program)
}

/// Production command executor using `tokio::process::Command`.
#[derive(Debug)]
pub struct ShellCommandExecutor;

#[async_trait]
impl CommandExecutor for ShellCommandExecutor {
    async fn run(&self, program: &str, args: &[&str]) -> std::io::Result<CommandOutput> {
        let resolved = resolve_program(program);
        let output = tokio::process::Command::new(resolved.as_ref())
            .args(args)
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
        })
    }

    async fn run_with_stdin(
        &self,
        program: &str,
        args: &[&str],
        stdin_data: &str,
    ) -> std::io::Result<CommandOutput> {
        use tokio::io::AsyncWriteExt;

        let resolved = resolve_program(program);
        let mut child = tokio::process::Command::new(resolved.as_ref())
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(stdin_data.as_bytes()).await?;
            drop(stdin);
        }

        let output = child.wait_with_output().await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
        })
    }
}
