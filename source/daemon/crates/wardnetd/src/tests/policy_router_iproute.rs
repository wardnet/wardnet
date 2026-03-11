use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::command::{CommandExecutor, CommandOutput};
use crate::policy_router::PolicyRouter;
use crate::policy_router_iproute::{IproutePolicyRouter, parse_wardnet_rules};

/// A single recorded invocation of the command executor.
#[derive(Debug, Clone)]
struct RecordedCall {
    program: String,
    args: Vec<String>,
}

/// Mock command executor that records calls and returns pre-configured responses.
#[derive(Debug)]
struct MockCommandExecutor {
    /// All calls recorded in order.
    calls: Arc<Mutex<Vec<RecordedCall>>>,
    /// Pre-configured responses returned in FIFO order.
    responses: Arc<Mutex<Vec<CommandOutput>>>,
}

impl MockCommandExecutor {
    /// Create a new mock with the given list of responses.
    ///
    /// Responses are consumed in order. If all responses are exhausted, a default
    /// success response with empty stdout/stderr is returned.
    fn new(responses: Vec<CommandOutput>) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(responses)),
        }
    }

    /// Return a snapshot of all recorded calls.
    async fn recorded_calls(&self) -> Vec<RecordedCall> {
        self.calls.lock().await.clone()
    }
}

#[async_trait]
impl CommandExecutor for MockCommandExecutor {
    async fn run(&self, program: &str, args: &[&str]) -> std::io::Result<CommandOutput> {
        self.calls.lock().await.push(RecordedCall {
            program: program.to_owned(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
        });

        let response = {
            let mut responses = self.responses.lock().await;
            if responses.is_empty() {
                CommandOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    success: true,
                }
            } else {
                responses.remove(0)
            }
        };

        Ok(response)
    }

    async fn run_with_stdin(
        &self,
        program: &str,
        args: &[&str],
        _stdin_data: &str,
    ) -> std::io::Result<CommandOutput> {
        self.calls.lock().await.push(RecordedCall {
            program: program.to_owned(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
        });

        let response = {
            let mut responses = self.responses.lock().await;
            if responses.is_empty() {
                CommandOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    success: true,
                }
            } else {
                responses.remove(0)
            }
        };

        Ok(response)
    }
}

/// Helper to build a successful `CommandOutput` with the given stdout.
fn success_output(stdout: &str) -> CommandOutput {
    CommandOutput {
        stdout: stdout.to_owned(),
        stderr: String::new(),
        success: true,
    }
}

/// Helper to build a failed `CommandOutput` with the given stderr.
fn failure_output(stderr: &str) -> CommandOutput {
    CommandOutput {
        stdout: String::new(),
        stderr: stderr.to_owned(),
        success: false,
    }
}

// ---------------------------------------------------------------------------
// Parser tests (pure function, no mock needed)
// ---------------------------------------------------------------------------

#[test]
fn parse_wardnet_rules_basic() {
    let json = r#"[
        {"priority":0,"src":"all","table":"local"},
        {"priority":100,"src":"192.168.1.50","table":"100"},
        {"priority":100,"src":"192.168.1.51","table":"200"},
        {"priority":32766,"src":"all","table":"main"},
        {"priority":32767,"src":"all","table":"default"}
    ]"#;

    let rules = parse_wardnet_rules(json).unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0], ("192.168.1.50".to_owned(), 100));
    assert_eq!(rules[1], ("192.168.1.51".to_owned(), 200));
}

#[test]
fn parse_wardnet_rules_filters_low_tables() {
    // Named tables ("local", "main", "default") and numeric tables < 100 are excluded.
    let json = r#"[
        {"priority":0,"src":"all","table":"local"},
        {"priority":100,"src":"10.0.0.1","table":"50"},
        {"priority":100,"src":"10.0.0.2","table":"99"},
        {"priority":32766,"src":"all","table":"main"},
        {"priority":32767,"src":"all","table":"default"}
    ]"#;

    let rules = parse_wardnet_rules(json).unwrap();
    assert!(rules.is_empty());
}

#[test]
fn parse_wardnet_rules_handles_empty_array() {
    let rules = parse_wardnet_rules("[]").unwrap();
    assert!(rules.is_empty());
}

#[test]
fn parse_wardnet_rules_skips_entries_without_src() {
    let json = r#"[
        {"priority":100,"table":"100"},
        {"priority":100,"src":"192.168.1.50"},
        {"priority":100,"src":"192.168.1.51","table":"200"}
    ]"#;

    let rules = parse_wardnet_rules(json).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0], ("192.168.1.51".to_owned(), 200));
}

#[test]
fn parse_wardnet_rules_invalid_json_returns_error() {
    let result = parse_wardnet_rules("not json");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Integration tests (with MockCommandExecutor)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn enable_ip_forwarding_sends_correct_command() {
    let mock = Arc::new(MockCommandExecutor::new(vec![success_output("")]));
    let router = IproutePolicyRouter::new(mock.clone());

    router.enable_ip_forwarding().await.unwrap();

    let calls = mock.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program, "sysctl");
    assert_eq!(calls[0].args, vec!["-w", "net.ipv4.ip_forward=1"]);
}

#[tokio::test]
async fn add_route_table_sends_correct_command() {
    let mock = Arc::new(MockCommandExecutor::new(vec![success_output("")]));
    let router = IproutePolicyRouter::new(mock.clone());

    router.add_route_table("wg_ward0", 100).await.unwrap();

    let calls = mock.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program, "ip");
    assert_eq!(
        calls[0].args,
        vec!["route", "add", "default", "dev", "wg_ward0", "table", "100"]
    );
}

#[tokio::test]
async fn remove_route_table_sends_correct_command() {
    let mock = Arc::new(MockCommandExecutor::new(vec![success_output("")]));
    let router = IproutePolicyRouter::new(mock.clone());

    router.remove_route_table(100).await.unwrap();

    let calls = mock.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program, "ip");
    assert_eq!(
        calls[0].args,
        vec!["route", "del", "default", "table", "100"]
    );
}

#[tokio::test]
async fn has_route_table_returns_true_when_output_nonempty() {
    let mock = Arc::new(MockCommandExecutor::new(vec![success_output(
        "default dev wg_ward0 scope link\n",
    )]));
    let router = IproutePolicyRouter::new(mock.clone());

    let result = router.has_route_table(100).await.unwrap();
    assert!(result);

    let calls = mock.recorded_calls().await;
    assert_eq!(calls[0].program, "ip");
    assert_eq!(calls[0].args, vec!["route", "show", "table", "100"]);
}

#[tokio::test]
async fn has_route_table_returns_false_when_output_empty() {
    let mock = Arc::new(MockCommandExecutor::new(vec![success_output("")]));
    let router = IproutePolicyRouter::new(mock.clone());

    let result = router.has_route_table(100).await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn add_ip_rule_sends_correct_command() {
    let mock = Arc::new(MockCommandExecutor::new(vec![success_output("")]));
    let router = IproutePolicyRouter::new(mock.clone());

    router.add_ip_rule("192.168.1.50", 100).await.unwrap();

    let calls = mock.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program, "ip");
    assert_eq!(
        calls[0].args,
        vec!["rule", "add", "from", "192.168.1.50", "lookup", "100"]
    );
}

#[tokio::test]
async fn remove_ip_rule_sends_correct_command() {
    let mock = Arc::new(MockCommandExecutor::new(vec![success_output("")]));
    let router = IproutePolicyRouter::new(mock.clone());

    router.remove_ip_rule("192.168.1.50", 100).await.unwrap();

    let calls = mock.recorded_calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program, "ip");
    assert_eq!(
        calls[0].args,
        vec!["rule", "del", "from", "192.168.1.50", "lookup", "100"]
    );
}

#[tokio::test]
async fn list_wardnet_rules_calls_ip_json_and_parses() {
    let json = r#"[
        {"priority":0,"src":"all","table":"local"},
        {"priority":100,"src":"192.168.1.50","table":"100"},
        {"priority":32766,"src":"all","table":"main"}
    ]"#;
    let mock = Arc::new(MockCommandExecutor::new(vec![success_output(json)]));
    let router = IproutePolicyRouter::new(mock.clone());

    let rules = router.list_wardnet_rules().await.unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0], ("192.168.1.50".to_owned(), 100));

    let calls = mock.recorded_calls().await;
    assert_eq!(calls[0].program, "ip");
    assert_eq!(calls[0].args, vec!["-json", "rule", "list"]);
}

#[tokio::test]
async fn check_tools_calls_ip_and_sysctl() {
    let mock = Arc::new(MockCommandExecutor::new(vec![
        success_output("ip utility, iproute2-5.10"),
        success_output("sysctl from procps-ng 3.3.16"),
    ]));
    let router = IproutePolicyRouter::new(mock.clone());

    router.check_tools_available().await.unwrap();

    let calls = mock.recorded_calls().await;
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].program, "ip");
    assert_eq!(calls[0].args, vec!["-V"]);
    assert_eq!(calls[1].program, "sysctl");
    assert_eq!(calls[1].args, vec!["--version"]);
}

#[tokio::test]
async fn check_tools_fails_when_ip_is_missing() {
    let mock = Arc::new(MockCommandExecutor::new(vec![failure_output(
        "ip: command not found",
    )]));
    let router = IproutePolicyRouter::new(mock.clone());

    let err = router.check_tools_available().await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("ip"),
        "error should mention ip command, got: {msg}"
    );
}

#[tokio::test]
async fn check_tools_fails_when_sysctl_is_missing() {
    let mock = Arc::new(MockCommandExecutor::new(vec![
        // ip -V succeeds
        success_output("ip utility, iproute2-5.10"),
        // sysctl --version fails
        failure_output("sysctl: command not found"),
    ]));
    let router = IproutePolicyRouter::new(mock.clone());

    let err = router.check_tools_available().await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("sysctl"),
        "error should mention sysctl, got: {msg}"
    );
}

#[tokio::test]
async fn command_failure_returns_error() {
    let mock = Arc::new(MockCommandExecutor::new(vec![failure_output(
        "RTNETLINK answers: Operation not permitted",
    )]));
    let router = IproutePolicyRouter::new(mock.clone());

    let err = router.enable_ip_forwarding().await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("RTNETLINK answers: Operation not permitted"),
        "error should contain stderr, got: {msg}"
    );
}
