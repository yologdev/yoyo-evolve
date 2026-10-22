//! #1068: every refusal the bash door can hand the model is a *deterministic*
//! refusal — `is_deterministic_tool_error` says so, so the whole prompt is
//! not auto-retried (#662) and `RecoveryHintTool` does not append "try a
//! simpler version" (#710), which would coach the model around yoyo's guard.
//!
//! Census by text (Day 236), every row produced by calling the real producer:
//! - `Command blocked by safety policy: ` — `hard_deny::hard_deny_refusal`
//!   (parent `StreamingBashTool` and child `UserDenyBashTool`).
//! - `Command blocked by your permission rules: ` — `tools::user_deny_refusal`
//!   (glob match) and `safety::user_deny_spelling_refusal` (#1010 segment,
//!   #1011 eval-of-substitution, indirect command word); `/cd` deny via
//!   `cd_deny::effective_user_deny_refusal` reuses the same text.
//! - `Command needs user confirmation that a sub-agent cannot obtain: ` —
//!   `tools::child_confirm_refusal`.
//! - `... git may not be redirected outside it.` — `safety::git_redirection_refusal_message`
//!   (parent pinned cwd and `tools::child_git_redirection_refusal`).
//! - `Command was not confirmed by the user.` — `StreamingBashTool::execute`
//!   when the user answers no (the bash twin of `User denied `).
//!
//! Not rows: `--restricted` and read-only children *remove* tools rather than
//! refuse them, so they produce no text; `Blocked by hook: ` (hooks.rs) is a
//! user hook's verdict on any tool and may depend on outside state, so it is
//! left a real failure here.

use crate::prompt_retry::is_deterministic_tool_error;
use crate::tool_wrappers::{with_recovery_hints, ToolFailureTracker};
use crate::tools::StreamingBashTool;
use yoagent::types::AgentTool;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

async fn run(tool: &dyn AgentTool, cmd: &str) -> Result<String, String> {
    let ctx = yoagent::types::ToolContext::new("call-1068", "bash");
    match tool
        .execute(serde_json::json!({ "command": cmd }), ctx)
        .await
    {
        Ok(r) => Ok(format!("{:?}", r.content)),
        Err(e) => Err(e.to_string()),
    }
}

/// Every census row, from its real producer. Each `expect` is anti-vacuous:
/// a producer that stops refusing fails here by name, not by a silent skip.
fn census() -> Vec<(&'static str, String)> {
    let deny = s(&["rm *"]);
    let wt = std::env::temp_dir().join("yoyo_1068_wt");
    let wt = wt.to_string_lossy().to_string();
    vec![
        (
            "hard deny",
            crate::tools::hard_deny_refusal(crate::tools::HARD_DENY_PATTERNS, "rm -rf /")
                .expect("hard deny refuses rm -rf /"),
        ),
        (
            "user deny glob",
            crate::tools::user_deny_refusal(&deny, "rm x").expect("glob refuses"),
        ),
        (
            "user deny segment (#1010)",
            crate::tools::user_deny_refusal(&deny, "true && rm x").expect("segment refuses"),
        ),
        (
            "user deny eval of substitution (#1011)",
            crate::tools::user_deny_refusal(&deny, "eval \"$(echo hi)\"").expect("eval refuses"),
        ),
        (
            "user deny indirect command word",
            crate::tools::user_deny_refusal(&deny, "$X foo").expect("indirect refuses"),
        ),
        (
            "child confirm",
            crate::tools::child_confirm_refusal(true, "git push --force").expect("child confirm"),
        ),
        (
            "git redirection (pinned cwd)",
            crate::tools::child_git_redirection_refusal(Some(&wt), "git --work-tree=/other add .")
                .expect("redirection refused"),
        ),
    ]
}

#[test]
fn every_bash_refusal_in_the_census_is_deterministic() {
    let rows = census();
    assert_eq!(rows.len(), 7, "census rows");
    for (name, msg) in &rows {
        assert!(
            is_deterministic_tool_error(msg),
            "{name}: refusal not classified deterministic: {msg:?}"
        );
    }
}

#[tokio::test]
async fn bash_user_decline_is_deterministic() {
    let tool = StreamingBashTool::default().with_confirm(|_| false);
    let err = run(&tool, "echo yoyo_1068").await.expect_err("declined");
    assert_eq!(err, "Command was not confirmed by the user.");
    assert!(is_deterministic_tool_error(&err), "{err:?}");
}

#[test]
fn real_failures_and_near_misses_stay_retryable() {
    for msg in [
        "bash: foo: command not found",
        "Exit code: 1",
        "Exit code: 127\nbash: line 1: zz: command not found",
        "the request was blocked by the proxy, try again",
        "Command blocked",
        "Failed to spawn: No such file or directory",
    ] {
        assert!(
            !is_deterministic_tool_error(msg),
            "near-miss wrongly deterministic: {msg:?}"
        );
    }
}

/// Emission point: what the model receives from the wrapped parent bash.
#[tokio::test]
async fn hint_wrapper_passes_a_hard_deny_refusal_through_byte_identical() {
    let tracker = ToolFailureTracker::new();
    let tool = with_recovery_hints(Box::new(StreamingBashTool::default()), &tracker);
    let want = crate::tools::hard_deny_refusal(crate::tools::HARD_DENY_PATTERNS, "rm -rf /")
        .expect("refuses");
    for _ in 0..2 {
        let err = run(tool.as_ref(), "rm -rf /").await.expect_err("refused");
        assert_eq!(err, want);
    }
    assert_eq!(
        tracker.get("bash", "rm -rf /"),
        0,
        "a refusal is not a failure"
    );

    // Near-miss through the same wrapper: a real tool error still gets the hint.
    let ctx = yoagent::types::ToolContext::new("call-1068", "bash");
    let bad = tool
        .execute(serde_json::json!({}), ctx)
        .await
        .expect_err("missing command is an error")
        .to_string();
    assert!(bad.contains("💡 Recovery hint:"), "{bad:?}");
    assert_eq!(tracker.get("bash", "_"), 1);

    // A user-deny refusal through the wrapper, too: byte-identical.
    let deny_tool = StreamingBashTool {
        user_deny: s(&["echo yoyo_1068*"]),
        ..Default::default()
    };
    let deny_wrapped = with_recovery_hints(Box::new(deny_tool), &tracker);
    let err = run(deny_wrapped.as_ref(), "echo yoyo_1068_x")
        .await
        .expect_err("user deny refuses");
    assert_eq!(
        err,
        "Command blocked by your permission rules: matches deny pattern 'echo yoyo_1068*'."
    );
    assert_eq!(tracker.get("bash", "echo yoyo_1068_x"), 0);
}
