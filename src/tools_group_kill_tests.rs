//! `StreamingBashTool` kills the command's whole process group on timeout and
//! cancel, not just `bash` (same defect as yologdev/yoagent#277, fixed in
//! yoagent 0.25.2's `BashTool`). Unix-only: off Unix the group guard is a
//! no-op. Each test uses a unique sleep argument so parallel tests cannot see
//! each other's processes. Split out of `src/tools.rs` for the module-size gate.
#![cfg(unix)]

use crate::tools::StreamingBashTool;
use std::time::Duration;
use yoagent::types::AgentTool;

fn ctx() -> yoagent::types::ToolContext {
    yoagent::types::ToolContext::new("call-group-kill", "bash")
}

/// Is any process whose command line contains `needle` still alive?
/// Polls up to ~2s so a group that is dying is not reported as alive.
async fn survivor_alive(needle: &str) -> bool {
    for _ in 0..20 {
        let out = std::process::Command::new("pgrep")
            .arg("-f")
            .arg(needle)
            .output()
            .expect("pgrep runs");
        if out.stdout.is_empty() {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    true
}

/// Clean up after a test whose assertion failed, so a red run does not
/// leave a 77-second sleep behind.
fn reap(needle: &str) {
    let _ = std::process::Command::new("pkill")
        .arg("-f")
        .arg(needle)
        .status();
}

#[tokio::test]
async fn test_streaming_bash_timeout_kills_pipeline_stage() {
    // Unique per test so parallel tests cannot see each other's sleeps.
    let needle = "sleep 77.4101";
    let tool = StreamingBashTool::default();
    let params = serde_json::json!({
        "command": format!("{needle} | cat; echo done"),
        "timeout": 1
    });
    let result = tool.execute(params, ctx()).await;
    assert!(
        result
            .as_ref()
            .is_err_and(|e| e.to_string().contains("timed out")),
        "expected a timeout, got {result:?}"
    );
    let alive = survivor_alive(needle).await;
    reap(needle);
    assert!(
        !alive,
        "`{needle}` survived the timeout: only `bash` was killed, not its group"
    );
}

#[tokio::test]
async fn test_streaming_bash_cancel_kills_background_job() {
    let needle = "sleep 77.4102";
    let tool = StreamingBashTool::default();
    let params = serde_json::json!({ "command": format!("{needle} & wait") });
    let ctx = ctx();
    let cancel = ctx.cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        cancel.cancel();
    });
    let result = tool.execute(params, ctx).await;
    assert!(
        matches!(result, Err(yoagent::types::ToolError::Cancelled)),
        "expected Cancelled, got {result:?}"
    );
    let alive = survivor_alive(needle).await;
    reap(needle);
    assert!(
        !alive,
        "`{needle}` survived the cancel: only `bash` was killed, not its group"
    );
}

/// Near-miss: a command that FINISHES on its own is never group-killed,
/// so a job it deliberately detached keeps running (the same rule as
/// yoagent 0.25.2's `GroupKill::disarm`). Only timeout/cancel kill.
#[tokio::test]
async fn test_streaming_bash_normal_exit_leaves_detached_job_running() {
    let needle = "sleep 77.4103";
    let tool = StreamingBashTool::default();
    let params = serde_json::json!({
        "command": format!("{needle} >/dev/null 2>&1 & echo started")
    });
    let result = tool.execute(params, ctx()).await.unwrap();
    match &result.content[0] {
        yoagent::types::Content::Text { text } => {
            assert_eq!(text, "Exit code: 0\nstarted");
        }
        _ => panic!("Expected text content"),
    }
    let out = std::process::Command::new("pgrep")
        .arg("-f")
        .arg(needle)
        .output()
        .expect("pgrep runs");
    reap(needle);
    assert!(
        !out.stdout.is_empty(),
        "a detached job of a command that exited normally must not be killed"
    );
}

/// In its own process group, `bash` must NOT inherit yoyo's terminal stdin:
/// a background group reading the tty gets SIGTTIN and freezes until the
/// timeout. So stdin is null — a reader sees EOF at once and the command
/// finishes (yoagent 0.25.2 pairs `process_group(0)` with `Stdio::null()`).
#[tokio::test]
async fn test_streaming_bash_stdin_is_null_so_readers_get_eof() {
    let tool = StreamingBashTool::default();
    let params = serde_json::json!({
        "command": "if read -r x; then echo got-input; else echo eof; fi",
        "timeout": 5
    });
    let result = tool.execute(params, ctx()).await.unwrap();
    match &result.content[0] {
        yoagent::types::Content::Text { text } => assert_eq!(text, "Exit code: 0\neof"),
        _ => panic!("Expected text content"),
    }
    #[cfg(target_os = "linux")]
    {
        let params = serde_json::json!({ "command": "readlink /proc/$$/fd/0" });
        let result = tool.execute(params, ctx()).await.unwrap();
        match &result.content[0] {
            yoagent::types::Content::Text { text } => {
                assert_eq!(text, "Exit code: 0\n/dev/null")
            }
            _ => panic!("Expected text content"),
        }
    }
}
