//! A timed-out bash call returns what the command printed before the timeout,
//! not only "timed out" — matching yoagent 0.25.15's own bash tool
//! (`"Command timed out after {}s. Output so far:\n{}"`). It stays an error,
//! so the timeout is still loud; a silent command keeps today's exact string.
//! Split out of `src/tools.rs` for the module-size gate.
#![cfg(unix)]

use crate::tools::{timed_out_message, StreamingBashTool};
use std::time::Duration;
use yoagent::types::{AgentTool, Content};

fn ctx() -> yoagent::types::ToolContext {
    yoagent::types::ToolContext::new("call-bash-timeout", "bash")
}

fn tool(timeout_ms: u64) -> StreamingBashTool {
    StreamingBashTool {
        timeout: Duration::from_millis(timeout_ms),
        ..Default::default()
    }
}

async fn timeout_error(tool: StreamingBashTool, command: &str) -> String {
    let params = serde_json::json!({ "command": command });
    match tool.execute(params, ctx()).await {
        Err(e) => e.to_string(),
        Ok(r) => panic!("expected a timeout error, got success: {:?}", r.content),
    }
}

#[tokio::test]
async fn timed_out_command_reports_the_output_it_printed_so_far() {
    let err = timeout_error(tool(400), "echo started; sleep 30").await;
    assert!(err.contains("timed out after"), "still a timeout: {err}");
    assert!(err.contains("Output so far:"), "header present: {err}");
    assert!(err.contains("started"), "partial output kept: {err}");
}

#[tokio::test]
async fn silent_timed_out_command_keeps_the_exact_old_string() {
    let err = timeout_error(tool(200), "sleep 30").await;
    // `ToolError::Failed`'s Display prefix, then today's message byte-for-byte:
    // no empty "Output so far:" header when nothing was printed.
    let expected =
        yoagent::types::ToolError::Failed("Command timed out after 0s".into()).to_string();
    assert_eq!(err, expected);
}

#[tokio::test]
async fn finished_command_is_unchanged() {
    let params = serde_json::json!({ "command": "echo hi" });
    let r = tool(10_000)
        .execute(params, ctx())
        .await
        .expect("echo succeeds");
    match &r.content[0] {
        Content::Text { text } => assert_eq!(text, "Exit code: 0\nhi"),
        other => panic!("expected text, got {other:?}"),
    }
}

#[tokio::test]
async fn oversized_partial_output_goes_through_the_shared_cap() {
    let t = StreamingBashTool {
        timeout: Duration::from_millis(600),
        max_output_bytes: 100,
        ..Default::default()
    };
    let err = timeout_error(
        t,
        "for i in $(seq 1 100); do echo \"line number $i of the output\"; done; sleep 30",
    )
    .await;
    assert!(err.contains("Output so far:"), "{err}");
    assert!(
        err.contains("... (output truncated)"),
        "capped by the reader: {err}"
    );
    assert!(
        !err.contains("line number 100 "),
        "tail beyond the cap dropped: {err}"
    );
    assert!(
        err.len() < 400,
        "partial was capped, got {} bytes",
        err.len()
    );
}

#[test]
fn timed_out_message_table() {
    assert_eq!(timed_out_message(5, ""), "Command timed out after 5s");
    assert_eq!(
        timed_out_message(5, "a\nb"),
        "Command timed out after 5s. Output so far:\na\nb"
    );
    // Multi-byte output passes through whole (no byte slicing here; the cap lives in the reader).
    assert_eq!(
        timed_out_message(1, "✓ done"),
        "Command timed out after 1s. Output so far:\n✓ done"
    );
}
