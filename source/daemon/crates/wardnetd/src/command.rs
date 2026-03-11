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

/// Production command executor using `tokio::process::Command`.
#[derive(Debug)]
pub struct ShellCommandExecutor;

#[async_trait]
impl CommandExecutor for ShellCommandExecutor {
    async fn run(&self, program: &str, args: &[&str]) -> std::io::Result<CommandOutput> {
        let output = tokio::process::Command::new(program)
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

        let mut child = tokio::process::Command::new(program)
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
