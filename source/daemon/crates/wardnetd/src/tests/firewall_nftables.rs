use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::firewall_nftables::{NftablesFirewallManager, parse_rule_handle};
use wardnetd_services::command::{CommandExecutor, CommandOutput};
use wardnetd_services::routing::firewall::FirewallManager;

// ---------------------------------------------------------------------------
// Mock infrastructure
// ---------------------------------------------------------------------------

/// A recorded invocation of the command executor.
#[derive(Debug, Clone)]
struct RecordedCall {
    program: String,
    args: Vec<String>,
    stdin_data: Option<String>,
}

/// A mock [`CommandExecutor`] that records every call and returns pre-configured responses.
#[derive(Debug)]
struct MockCommandExecutor {
    calls: Arc<Mutex<Vec<RecordedCall>>>,
    responses: Arc<Mutex<Vec<CommandOutput>>>,
}

impl MockCommandExecutor {
    /// Create a mock that returns a default success response for every call.
    fn new() -> (Self, Arc<Mutex<Vec<RecordedCall>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mock = Self {
            calls: Arc::clone(&calls),
            responses: Arc::new(Mutex::new(Vec::new())),
        };
        (mock, calls)
    }

    /// Create a mock with a queue of responses to return in order.
    /// Once the queue is exhausted, subsequent calls return a default success.
    fn with_responses(responses: Vec<CommandOutput>) -> (Self, Arc<Mutex<Vec<RecordedCall>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mock = Self {
            calls: Arc::clone(&calls),
            responses: Arc::new(Mutex::new(responses)),
        };
        (mock, calls)
    }
}

fn success_output(stdout: &str) -> CommandOutput {
    CommandOutput {
        stdout: stdout.to_owned(),
        stderr: String::new(),
        success: true,
    }
}

fn failure_output(stderr: &str) -> CommandOutput {
    CommandOutput {
        stdout: String::new(),
        stderr: stderr.to_owned(),
        success: false,
    }
}

#[async_trait]
impl CommandExecutor for MockCommandExecutor {
    async fn run(&self, program: &str, args: &[&str]) -> std::io::Result<CommandOutput> {
        self.calls.lock().await.push(RecordedCall {
            program: program.to_owned(),
            args: args.iter().map(|s| (*s).to_owned()).collect(),
            stdin_data: None,
        });

        let mut responses = self.responses.lock().await;
        if responses.is_empty() {
            Ok(success_output(""))
        } else {
            Ok(responses.remove(0))
        }
    }

    async fn run_with_stdin(
        &self,
        program: &str,
        args: &[&str],
        stdin_data: &str,
    ) -> std::io::Result<CommandOutput> {
        self.calls.lock().await.push(RecordedCall {
            program: program.to_owned(),
            args: args.iter().map(|s| (*s).to_owned()).collect(),
            stdin_data: Some(stdin_data.to_owned()),
        });

        let mut responses = self.responses.lock().await;
        if responses.is_empty() {
            Ok(success_output(""))
        } else {
            Ok(responses.remove(0))
        }
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const NFT_POSTROUTING_OUTPUT: &str = "\
table inet wardnet {
    chain postrouting {
        type nat hook postrouting priority 100; policy accept;
        oifname \"wg_ward0\" masquerade comment \"wardnet:wg_ward0\" # handle 4
        oifname \"wg_ward1\" masquerade comment \"wardnet:wg_ward1\" # handle 5
    }
}
";

const NFT_PREROUTING_OUTPUT: &str = "\
table inet wardnet {
    chain prerouting {
        type nat hook prerouting priority -100; policy accept;
        ip saddr 192.168.1.100 udp dport 53 dnat to 10.5.0.1 comment \"wardnet:dns:192.168.1.100\" # handle 7
        ip saddr 192.168.1.101 udp dport 53 dnat to 10.5.0.2 comment \"wardnet:dns:192.168.1.101\" # handle 8
    }
}
";

// ---------------------------------------------------------------------------
// Parser tests (pure function, no mock needed)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn parse_rule_handle_finds_masquerade() {
    let handle = parse_rule_handle(NFT_POSTROUTING_OUTPUT, "wardnet:wg_ward0");
    assert_eq!(handle, Some(4));
}

#[tokio::test]
async fn parse_rule_handle_finds_dns_redirect() {
    let handle = parse_rule_handle(NFT_PREROUTING_OUTPUT, "wardnet:dns:192.168.1.100");
    assert_eq!(handle, Some(7));
}

#[tokio::test]
async fn parse_rule_handle_returns_none_when_not_found() {
    let handle = parse_rule_handle(NFT_POSTROUTING_OUTPUT, "wardnet:wg_nonexistent");
    assert_eq!(handle, None);
}

#[tokio::test]
async fn parse_rule_handle_handles_empty_output() {
    let handle = parse_rule_handle("", "wardnet:wg_ward0");
    assert_eq!(handle, None);
}

#[tokio::test]
async fn parse_rule_handle_handles_multiple_rules() {
    let handle = parse_rule_handle(NFT_POSTROUTING_OUTPUT, "wardnet:wg_ward1");
    assert_eq!(handle, Some(5));

    let handle = parse_rule_handle(NFT_PREROUTING_OUTPUT, "wardnet:dns:192.168.1.101");
    assert_eq!(handle, Some(8));
}

/// A line contains the comment but `# handle` is followed by non-numeric text.
#[tokio::test]
async fn parse_rule_handle_returns_none_for_malformed_handle() {
    let output = r#"        oifname "wg_ward0" masquerade comment "wardnet:wg_ward0" # handle abc"#;
    let handle = parse_rule_handle(output, "wardnet:wg_ward0");
    assert_eq!(handle, None, "non-numeric handle should return None");
}

/// A line has the comment but no `# handle` suffix at all.
#[tokio::test]
async fn parse_rule_handle_returns_none_when_no_handle_suffix() {
    let output = r#"        oifname "wg_ward0" masquerade comment "wardnet:wg_ward0""#;
    let handle = parse_rule_handle(output, "wardnet:wg_ward0");
    assert_eq!(handle, None, "line without # handle should return None");
}

// ---------------------------------------------------------------------------
// Integration tests (with MockCommandExecutor)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn init_wardnet_table_sends_correct_script() {
    let (mock, calls) = MockCommandExecutor::new();
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.init_wardnet_table().await.expect("init should succeed");

    let recorded = calls.lock().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].program, "nft");
    assert_eq!(recorded[0].args, vec!["-f", "-"]);

    let stdin = recorded[0]
        .stdin_data
        .as_deref()
        .expect("should have stdin");
    assert!(
        stdin.contains("add table inet wardnet"),
        "script should create the table"
    );
    assert!(
        stdin.contains("add chain inet wardnet postrouting"),
        "script should create postrouting chain"
    );
    assert!(
        stdin.contains("add chain inet wardnet prerouting"),
        "script should create prerouting chain"
    );
}

#[tokio::test]
async fn flush_wardnet_table_sends_correct_command() {
    let (mock, calls) = MockCommandExecutor::new();
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.flush_wardnet_table()
        .await
        .expect("flush should succeed");

    let recorded = calls.lock().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].program, "nft");
    assert_eq!(recorded[0].args, vec!["flush", "table", "inet", "wardnet"]);
    assert!(recorded[0].stdin_data.is_none());
}

#[tokio::test]
async fn add_masquerade_sends_correct_command() {
    let (mock, calls) = MockCommandExecutor::new();
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.add_masquerade("wg_ward0")
        .await
        .expect("add_masquerade should succeed");

    let recorded = calls.lock().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].program, "nft");
    assert_eq!(
        recorded[0].args,
        vec![
            "add",
            "rule",
            "inet",
            "wardnet",
            "postrouting",
            "oifname",
            "wg_ward0",
            "masquerade",
            "comment",
            "\"wardnet:wg_ward0\"",
        ]
    );
}

#[tokio::test]
async fn remove_masquerade_finds_handle_and_deletes() {
    let (mock, calls) = MockCommandExecutor::with_responses(vec![
        // First call: list chain returns our fixture with handle 4.
        success_output(NFT_POSTROUTING_OUTPUT),
        // Second call: delete rule succeeds.
        success_output(""),
    ]);
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.remove_masquerade("wg_ward0")
        .await
        .expect("remove_masquerade should succeed");

    let recorded = calls.lock().await;
    assert_eq!(recorded.len(), 2, "should issue list then delete");

    // First call: list chain with annotated handles.
    assert_eq!(recorded[0].program, "nft");
    assert_eq!(
        recorded[0].args,
        vec!["-a", "list", "chain", "inet", "wardnet", "postrouting"]
    );

    // Second call: delete rule by handle.
    assert_eq!(recorded[1].program, "nft");
    assert_eq!(
        recorded[1].args,
        vec![
            "delete",
            "rule",
            "inet",
            "wardnet",
            "postrouting",
            "handle",
            "4"
        ]
    );
}

#[tokio::test]
async fn remove_masquerade_warns_when_no_handle() {
    // Return output that contains no matching rule for the requested interface.
    let no_match_output = "\
table inet wardnet {
    chain postrouting {
        type nat hook postrouting priority 100; policy accept;
        oifname \"wg_other\" masquerade comment \"wardnet:wg_other\" # handle 10
    }
}
";
    let (mock, calls) = MockCommandExecutor::with_responses(vec![success_output(no_match_output)]);
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.remove_masquerade("wg_ward0")
        .await
        .expect("remove_masquerade should succeed even when rule not found");

    let recorded = calls.lock().await;
    assert_eq!(
        recorded.len(),
        1,
        "should only issue the list call, no delete"
    );
}

#[tokio::test]
async fn add_dns_redirect_sends_correct_command() {
    let (mock, calls) = MockCommandExecutor::new();
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.add_dns_redirect("192.168.1.100", "10.5.0.1")
        .await
        .expect("add_dns_redirect should succeed");

    let recorded = calls.lock().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].program, "nft");
    assert_eq!(
        recorded[0].args,
        vec![
            "add",
            "rule",
            "inet",
            "wardnet",
            "prerouting",
            "ip",
            "saddr",
            "192.168.1.100",
            "udp",
            "dport",
            "53",
            "dnat",
            "to",
            "10.5.0.1",
            "comment",
            "\"wardnet:dns:192.168.1.100\"",
        ]
    );
}

#[tokio::test]
async fn check_tools_sends_version_command() {
    let (mock, calls) = MockCommandExecutor::with_responses(vec![success_output(
        "nftables v1.0.9 (Old Doc Yak #3)\n",
    )]);
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.check_tools_available()
        .await
        .expect("check_tools should succeed");

    let recorded = calls.lock().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].program, "nft");
    assert_eq!(recorded[0].args, vec!["--version"]);
}

#[tokio::test]
async fn remove_dns_redirect_finds_handle_and_deletes() {
    let (mock, calls) = MockCommandExecutor::with_responses(vec![
        // list chain returns our fixture with handle 7.
        success_output(NFT_PREROUTING_OUTPUT),
        // delete rule succeeds.
        success_output(""),
    ]);
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.remove_dns_redirect("192.168.1.100")
        .await
        .expect("remove_dns_redirect should succeed");

    let recorded = calls.lock().await;
    assert_eq!(recorded.len(), 2, "should issue list then delete");
    assert_eq!(
        recorded[1].args,
        vec![
            "delete",
            "rule",
            "inet",
            "wardnet",
            "prerouting",
            "handle",
            "7"
        ]
    );
}

#[tokio::test]
async fn remove_dns_redirect_warns_when_no_handle() {
    let no_match = "\
table inet wardnet {
    chain prerouting {
        type nat hook prerouting priority -100; policy accept;
    }
}
";
    let (mock, calls) = MockCommandExecutor::with_responses(vec![success_output(no_match)]);
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.remove_dns_redirect("192.168.1.100")
        .await
        .expect("should succeed even when rule not found");

    let recorded = calls.lock().await;
    assert_eq!(
        recorded.len(),
        1,
        "should only issue the list call, no delete"
    );
}

#[tokio::test]
async fn destroy_wardnet_table_sends_correct_command() {
    let (mock, calls) = MockCommandExecutor::new();
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.destroy_wardnet_table()
        .await
        .expect("destroy should succeed");

    let recorded = calls.lock().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].program, "nft");
    assert_eq!(recorded[0].args, vec!["delete", "table", "inet", "wardnet"]);
}

#[tokio::test]
async fn destroy_wardnet_table_ignores_error() {
    let (mock, _calls) = MockCommandExecutor::with_responses(vec![failure_output(
        "Error: No such file or directory",
    )]);
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    // Should NOT propagate the error.
    mgr.destroy_wardnet_table()
        .await
        .expect("destroy should succeed even when table doesn't exist");
}

#[tokio::test]
async fn command_failure_returns_error() {
    let (mock, _calls) = MockCommandExecutor::with_responses(vec![failure_output(
        "Error: No such file or directory; did you mean table 'inet wardnet'?",
    )]);
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    let result = mgr.flush_wardnet_table().await;
    assert!(result.is_err(), "should return an error on command failure");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("No such file or directory"),
        "error should contain the stderr message, got: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// TCP RST reject
// ---------------------------------------------------------------------------

#[tokio::test]
async fn add_tcp_reset_reject_runs_nft_with_correct_args() {
    let (mock, calls) = MockCommandExecutor::new();
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.add_tcp_reset_reject("192.168.1.10")
        .await
        .expect("add_tcp_reset_reject should succeed");

    let recorded = calls.lock().await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].program, "nft");
    let args_str = recorded[0].args.join(" ");
    assert!(
        args_str.contains("forward")
            && args_str.contains("reject")
            && args_str.contains("192.168.1.10"),
        "expected forward chain reject rule for device IP, got: {args_str}"
    );
}

#[tokio::test]
async fn remove_tcp_reset_reject_finds_and_deletes_rule() {
    let chain_output = r#"table inet wardnet {
    chain forward {
        type filter hook forward priority filter; policy accept;
        ip saddr 192.168.1.10 tcp flags & (fin|syn|rst) == 0x0 reject with tcp reset comment "wardnet:rst:192.168.1.10" # handle 7
    }
}"#;

    let (mock, calls) = MockCommandExecutor::with_responses(vec![
        // First call: list chain (returns the rule with handle)
        success_output(chain_output),
        // Second call: delete rule by handle
        success_output(""),
    ]);
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    mgr.remove_tcp_reset_reject("192.168.1.10")
        .await
        .expect("remove_tcp_reset_reject should succeed");

    let recorded = calls.lock().await;
    assert_eq!(recorded.len(), 2);
    // Second call should delete by handle 7.
    let delete_args = recorded[1].args.join(" ");
    assert!(
        delete_args.contains("delete")
            && delete_args.contains("handle")
            && delete_args.contains("handle 7"),
        "expected delete by handle 7, got: {delete_args}"
    );
}

#[tokio::test]
async fn remove_tcp_reset_reject_noop_when_rule_not_found() {
    let chain_output = r"table inet wardnet {
    chain forward {
        type filter hook forward priority filter; policy accept;
    }
}";

    let (mock, calls) = MockCommandExecutor::with_responses(vec![success_output(chain_output)]);
    let mgr = NftablesFirewallManager::new(Arc::new(mock));

    // Should succeed even when the rule doesn't exist.
    mgr.remove_tcp_reset_reject("192.168.1.10")
        .await
        .expect("remove_tcp_reset_reject should succeed when rule not found");

    let recorded = calls.lock().await;
    // Only the list call, no delete call.
    assert_eq!(recorded.len(), 1);
}
