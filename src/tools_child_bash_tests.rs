//! #977: a sub-agent / explore-agent child's bash obeys the parent's hard deny
//! list unconditionally, and refuses what the parent would have asked the user
//! about (`analyze_bash_command`) when the parent has a confirm prompt.

use crate::cli;
use crate::safety::analyze_bash_command;
use crate::tools::{
    child_confirm_refusal, hard_deny_refusal, StreamingBashTool, HARD_DENY_PATTERNS,
};
use std::sync::Arc;
use yoagent::types::AgentTool;

// Assembled at runtime so no literal catastrophic command sits in this file.
fn root_wipe() -> String {
    ["rm", "-rf", "/"].join(" ")
}

const FLAGGED: &str = "git push --force";
const NEAR_MISS: [&str; 3] = ["ls", "cargo test", "git status"];

fn child_bash(parent_confirms: bool, explore: bool) -> Arc<dyn AgentTool> {
    let disallowed = if explore {
        crate::tools::read_only_child_disallowed(&[])
    } else {
        Vec::new()
    };
    crate::tools::sub_agent_child_tools(
        &cli::DirectoryRestrictions::default(),
        &disallowed,
        &[],
        parent_confirms,
        None,
    )
    .into_iter()
    .find(|t| t.name() == "bash")
    .expect("the child must carry a bash tool, or these tests are vacuous")
}

async fn run(tool: &dyn AgentTool, cmd: &str) -> Result<String, String> {
    let ctx = yoagent::types::ToolContext::new("call-977", "bash");
    match tool
        .execute(serde_json::json!({ "command": cmd }), ctx)
        .await
    {
        Ok(r) => Ok(format!("{:?}", r.content)),
        Err(e) => Err(e.to_string()),
    }
}

#[tokio::test]
async fn child_refuses_hard_denied_command_in_both_approval_states() {
    let cmd = root_wipe();
    let parent_err = run(&StreamingBashTool::default(), &cmd)
        .await
        .expect_err("parent must refuse");
    for parent_confirms in [false, true] {
        for explore in [false, true] {
            let err = run(child_bash(parent_confirms, explore).as_ref(), &cmd)
                .await
                .expect_err("child must refuse a hard-denied command");
            assert_eq!(
                err,
                "Command blocked by safety policy: contains 'rm -rf /'. This pattern is denied for safety.",
                "parent_confirms={parent_confirms} explore={explore}"
            );
            assert_eq!(err, parent_err, "child text must be the parent's");
        }
    }
}

#[test]
fn child_layer_two_refuses_only_when_the_parent_would_have_asked() {
    let warning = analyze_bash_command(FLAGGED).expect("fixture must be flagged");
    assert_eq!(
        child_confirm_refusal(true, FLAGGED),
        Some(format!(
            "Command needs user confirmation that a sub-agent cannot obtain: {warning} \
             Return this step to the parent agent and let it run the command itself, \
             where the user can confirm it."
        ))
    );
    assert_eq!(child_confirm_refusal(false, FLAGGED), None);
}

#[tokio::test]
async fn child_bash_refuses_flagged_command_when_parent_confirms() {
    // Refused before execution, so the flagged command never runs here.
    for explore in [false, true] {
        let err = run(child_bash(true, explore).as_ref(), FLAGGED)
            .await
            .expect_err("parent would have asked, so the child must refuse");
        assert_eq!(Some(err), child_confirm_refusal(true, FLAGGED));
    }
}

#[tokio::test]
async fn child_bash_allows_flagged_command_when_parent_would_not_ask() {
    // A harmless flagged command, so the allow path can actually run.
    let cmd = "echo 'TRUNCATE TABLE logs_ZZ'";
    assert!(
        analyze_bash_command(cmd).is_some(),
        "fixture must be flagged"
    );
    let out = run(child_bash(false, false).as_ref(), cmd)
        .await
        .expect("no parent prompt: the child allows what the parent allows");
    assert!(out.contains("logs_ZZ"), "{out}");
}

#[tokio::test]
async fn near_miss_commands_pass_through_in_both_states() {
    for cmd in NEAR_MISS {
        assert_eq!(analyze_bash_command(cmd), None, "anti-vacuous: {cmd}");
        assert_eq!(hard_deny_refusal(HARD_DENY_PATTERNS, cmd), None, "{cmd}");
        for parent_confirms in [false, true] {
            assert_eq!(child_confirm_refusal(parent_confirms, cmd), None, "{cmd}");
        }
    }
    // Execute only the cheap one; `cargo test` would recurse into this suite.
    for parent_confirms in [false, true] {
        let out = run(child_bash(parent_confirms, false).as_ref(), "ls").await;
        assert!(out.is_ok(), "parent_confirms={parent_confirms}: {out:?}");
    }
}

#[test]
fn parent_default_deny_list_is_the_shared_authority() {
    let parent = StreamingBashTool::default();
    let authority: Vec<String> = HARD_DENY_PATTERNS.iter().map(|p| p.to_string()).collect();
    assert_eq!(parent.deny_patterns, authority);
    assert!(!authority.is_empty());
}

/// Weak source guard: both production builders derive `parent_confirms` from
/// the parent's own approval state. Proves it is passed, not that it fires.
#[test]
fn both_child_builders_pass_parent_confirms_from_auto_approve() {
    let src = include_str!("tools.rs");
    let needle = ["!config", ".auto_approve,"].concat();
    assert_eq!(src.matches(needle.as_str()).count(), 2);
}

// === Layer 3 (#977): git redirection escapes under a pinned-cwd parent ===

const ESCAPES: [&str; 3] = [
    "git --git-dir=/elsewhere/.git status",
    "git -C /elsewhere log",
    "GIT_DIR=/x git status",
];

fn pinned_root() -> String {
    let root = std::env::temp_dir().join("yoyo_child_bash_pinned_root");
    std::fs::create_dir_all(&root).expect("create pinned root");
    std::fs::canonicalize(&root)
        .expect("canonicalize pinned root")
        .display()
        .to_string()
}

fn pinned_child_bash(pin: Option<&str>, explore: bool) -> Arc<dyn AgentTool> {
    let disallowed = if explore {
        crate::tools::read_only_child_disallowed(&[])
    } else {
        Vec::new()
    };
    crate::tools::sub_agent_child_tools(
        &cli::DirectoryRestrictions::default(),
        &disallowed,
        &[],
        false,
        pin,
    )
    .into_iter()
    .find(|t| t.name() == "bash")
    .expect("the child must carry a bash tool, or these tests are vacuous")
}

#[tokio::test]
async fn pinned_child_refuses_git_redirection_with_the_parents_text() {
    let pin = pinned_root();
    let parent = StreamingBashTool::default().with_cwd(pin.clone());
    for cmd in ESCAPES {
        // Anti-vacuous: the detector really fires on this input against the pin.
        let reason = crate::safety::detect_git_redirection_escape(cmd, std::path::Path::new(&pin))
            .unwrap_or_else(|| panic!("fixture `{cmd}` must escape the pin"));
        let parent_err = run(&parent, cmd).await.expect_err("parent must refuse");
        for explore in [false, true] {
            let err = run(pinned_child_bash(Some(&pin), explore).as_ref(), cmd)
                .await
                .expect_err("pinned child must refuse a git redirection escape");
            assert_eq!(
                err,
                crate::safety::git_redirection_refusal_message(
                    &reason,
                    &pin,
                    crate::format::is_plain_output()
                ),
                "`{cmd}` explore={explore}"
            );
            assert_eq!(err, parent_err, "child text must be the parent's: `{cmd}`");
        }
    }
}

#[tokio::test]
async fn pinned_child_runs_in_the_pin_and_lets_bare_git_through() {
    let pin = pinned_root();
    for cmd in ["git status", "git log --oneline"] {
        assert_eq!(
            crate::tools::child_git_redirection_refusal(Some(&pin), cmd),
            None,
            "`{cmd}`"
        );
    }
    // The child's bash runs where the parent's does, not in the process cwd.
    let out = run(pinned_child_bash(Some(&pin), false).as_ref(), "pwd -P")
        .await
        .expect("pwd must run");
    assert!(out.contains(&pin), "child cwd must be the pin: {out}");
}

#[tokio::test]
async fn unpinned_child_never_applies_the_redirection_check() {
    // Every ordinary session: no pin, so the escape inputs are not refused here.
    for cmd in ESCAPES {
        assert_eq!(
            crate::tools::child_git_redirection_refusal(None, cmd),
            None,
            "`{cmd}`"
        );
        let out = run(pinned_child_bash(None, false).as_ref(), cmd).await;
        if let Err(e) = &out {
            assert!(!e.contains("pinned worktree"), "`{cmd}` was refused: {e}");
        }
    }
}

/// Weak source guard: both production builders pass the parent's pin.
/// Proves it is passed, not that it fires.
#[test]
fn both_child_builders_pass_the_parents_pinned_cwd() {
    let src = include_str!("tools.rs");
    let needle = ["config.bash_cwd", ".as_deref(),"].concat();
    assert_eq!(src.matches(needle.as_str()).count(), 2);
}
