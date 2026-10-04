//! A user `deny` pattern (`--deny` / `[permissions] deny`) must block bash on
//! EVERY approval path: `--yes` (no confirm closure at all), after the user
//! pressed "always" (the shared flag short-circuits the closure), and on a
//! command the safety analyzer flags (the closure then receives a decorated
//! `⚠️ …\nCommand: …` string that an anchored glob cannot match).

use crate::cli;
use crate::hooks::HookRegistry;
use crate::safety::analyze_bash_command;
use crate::tools::{build_bash_tool, build_tools_with_hooks, StreamingBashTool};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use yoagent::types::AgentTool;

fn deny(patterns: &[&str]) -> cli::PermissionConfig {
    cli::PermissionConfig {
        allow: vec![],
        deny: patterns.iter().map(|s| s.to_string()).collect(),
    }
}

async fn run(tool: &dyn AgentTool, cmd: &str) -> Result<String, String> {
    let ctx = yoagent::types::ToolContext::new("call-deny", "bash");
    match tool
        .execute(serde_json::json!({ "command": cmd }), ctx)
        .await
    {
        Ok(r) => Ok(format!("{:?}", r.content)),
        Err(e) => Err(e.to_string()),
    }
}

#[tokio::test]
async fn user_deny_blocks_bash_under_yes() {
    let perms = deny(&["echo BLOCKED_ZZ"]);
    let tools = build_tools_with_hooks(
        true,
        &perms,
        &cli::DirectoryRestrictions::default(),
        crate::format::TOOL_OUTPUT_MAX_CHARS,
        &Arc::new(HookRegistry::new()),
        None,
    );
    let bash = tools.iter().find(|t| t.name() == "bash").expect("bash");
    let err = run(bash.as_ref(), "echo BLOCKED_ZZ")
        .await
        .expect_err("a user deny pattern must block bash under --yes");
    assert!(
        err.contains("echo BLOCKED_ZZ"),
        "must name the pattern: {err}"
    );
    // Near-miss: --yes still runs a command no deny pattern matches.
    let out = run(bash.as_ref(), "echo ok_ZZ")
        .await
        .expect("a non-denied command must still run under --yes");
    assert!(out.contains("ok_ZZ"), "{out}");
}

#[tokio::test]
async fn user_deny_blocks_bash_after_always() {
    let perms = deny(&["echo BLOCKED_ZZ"]);
    // The user already pressed "a": the shared flag is set, so the confirm
    // closure would return true without reading stdin.
    let always = Arc::new(AtomicBool::new(true));
    let bash = build_bash_tool(false, &perms, &always, None);
    let err = run(&bash, "echo BLOCKED_ZZ")
        .await
        .expect_err("a user deny pattern must block bash after \"always\"");
    assert!(
        err.contains("echo BLOCKED_ZZ"),
        "must name the pattern: {err}"
    );
    // Near-miss: "always" still auto-approves a non-denied command.
    let out = run(&bash, "echo ok_ZZ")
        .await
        .expect("a non-denied command must still run after always");
    assert!(out.contains("ok_ZZ"), "{out}");
}

/// The analyzer flags `git push --force`, and before this fix the closure
/// matched user deny against the decorated `⚠️ …\nCommand: …` prompt text,
/// which an anchored glob like `git push --force*` never matches — so the
/// user was PROMPTED for a command they had denied. Deny must be read from
/// the raw command, before the confirm callback is ever asked. The callback
/// refuses and the cwd is a tempdir, so even a regressed build runs nothing.
#[tokio::test]
async fn user_deny_matches_raw_command_before_the_warning_prompt() {
    let dir = tempfile::tempdir().expect("tempdir");
    let asked = Arc::new(AtomicBool::new(false));
    let asked2 = Arc::clone(&asked);
    let mut bash = StreamingBashTool::default()
        .with_cwd(dir.path().to_string_lossy().to_string())
        .with_confirm(move |_| {
            asked2.store(true, Ordering::Relaxed);
            false
        });
    bash.user_deny = vec!["git push --force*".to_string()];
    let cmd = "git push --force origin zz_user_deny_branch";
    assert!(
        analyze_bash_command(cmd).is_some(),
        "fixture must exercise the analyzer-warning path"
    );
    let err = run(&bash, cmd).await.expect_err("deny must block");
    assert!(
        err.contains("git push --force*"),
        "must name the pattern: {err}"
    );
    assert!(
        !asked.load(Ordering::Relaxed),
        "a denied command must never reach the confirm prompt"
    );
}

// ---------------------------------------------------------------------------
// The child door: `sub_agent` and `explore_agent` build their bash in
// `sub_agent_child_tools`, from yoagent's raw `BashTool`, which never sees
// `StreamingBashTool::user_deny`. Both production callers pass
// `&config.permissions.deny`; these tests drive that same seam with the two
// disallow lists production uses (the sub_agent one and the explore fold).
// Every command runs in a tempdir, never the repo cwd.
// ---------------------------------------------------------------------------

fn child_bash(user_deny: &[String], explore: bool) -> Arc<dyn AgentTool> {
    let disallowed = if explore {
        crate::tools::read_only_child_disallowed(&[])
    } else {
        Vec::new()
    };
    crate::tools::sub_agent_child_tools(
        &cli::DirectoryRestrictions::default(),
        &disallowed,
        user_deny,
        false,
        None,
    )
    .into_iter()
    .find(|t| t.name() == "bash")
    .expect("the child must carry a bash tool, or these tests are vacuous")
}

#[tokio::test]
async fn sub_agent_and_explore_child_bash_honour_user_deny_before_running() {
    for explore in [false, true] {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("MARKER_ZZ");
        let bash = child_bash(&["touch *".to_string()], explore);
        let cmd = format!("touch {}", marker.display());
        let err = run(bash.as_ref(), &cmd)
            .await
            .expect_err("a user deny pattern must block the child's bash");
        assert_eq!(
            err, "Command blocked by your permission rules: matches deny pattern 'touch *'.",
            "explore={explore}: the child must refuse with the parent's exact text"
        );
        assert!(
            !marker.exists(),
            "explore={explore}: the denied command ran anyway"
        );
    }
}

#[tokio::test]
async fn child_refusal_text_is_the_parents() {
    let deny = vec!["echo BLOCKED_ZZ".to_string()];
    let parent = StreamingBashTool {
        user_deny: deny.clone(),
        ..Default::default()
    };
    let parent_err = run(&parent, "echo BLOCKED_ZZ").await.expect_err("parent");
    let child_err = run(child_bash(&deny, false).as_ref(), "echo BLOCKED_ZZ")
        .await
        .expect_err("child");
    assert_eq!(child_err, parent_err);
}

#[tokio::test]
async fn child_bash_still_runs_a_non_denied_command() {
    for explore in [false, true] {
        let bash = child_bash(&["touch *".to_string()], explore);
        let out = run(bash.as_ref(), "echo ok_ZZ")
            .await
            .expect("a non-denied command must still run in the child");
        assert!(out.contains("ok_ZZ"), "explore={explore}: {out}");
    }
}

/// The regression surface: with no deny list the child's bash still presents
/// the raw tool's description and schema, and ordinary commands still run.
#[tokio::test]
async fn empty_deny_list_leaves_child_bash_unchanged() {
    let bash = child_bash(&[], false);
    let out = run(bash.as_ref(), "echo ok_ZZ").await.expect("runs");
    assert!(out.contains("ok_ZZ"), "{out}");
    let raw = yoagent::tools::bash::BashTool::default();
    assert_eq!(bash.description(), raw.description());
    assert_eq!(bash.parameters_schema(), raw.parameters_schema());
}

/// Weak source-level guard: both production builders hand the user's deny
/// list to the child seam. Proves the argument is passed, not that it is read.
#[test]
fn both_child_builders_pass_the_users_deny_list() {
    let src = include_str!("tools.rs");
    let prod = &src[..src.find("#[cfg(test)]").expect("test module")];
    let needle = ["&config.permissions", ".deny,"].concat();
    assert_eq!(prod.matches(needle.as_str()).count(), 2, "{needle}");
}
