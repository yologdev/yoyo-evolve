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

/// #1010: the deny list is the one fence that holds under `--yes`, `-p` and
/// "always", and it used to be one whole-string glob against the command as
/// the model spelled it. The property pinned here: no command a deny pattern
/// blocks when written plainly may run because of how it was spelled.
/// Spelling normalisation plus refusal of what cannot be normalised, not a
/// shell interpreter (`eval "$(printf ...)"` assembling a string is not seen).
#[test]
fn user_deny_checks_each_segment_and_refuses_indirect_words() {
    use crate::tools::user_deny_refusal;
    let list = vec!["rm -rf *".to_string()];
    let must_refuse = [
        "rm -rf ~",
        "cd x && rm -rf ~",
        "true; rm -rf ~",
        "ls | rm -rf ~",
        "TZ=\"$HOME\" rm -rf build",
        "A=1 B=2 rm -rf ~",
        "export A=1 && rm -rf ~",
        "if true; then rm -rf ~; fi",
        "X=rm; $X -rf ~",
        "$(echo rm) -rf ~",
        "`echo rm` -rf ~",
        "BASH_ARGV0=rm; $BASH_ARGV0 -rf ~",
    ];
    for cmd in must_refuse {
        assert!(
            user_deny_refusal(&list, cmd).is_some(),
            "{cmd:?} must be refused by deny 'rm -rf *'"
        );
    }
    // Near-misses: the regression surface. Quoted prose is one word to the
    // quote-aware lexer, so `;` / `&&` inside quotes never start a segment.
    let must_allow = [
        "cargo build && cargo test",
        "echo \"rm -rf ~\"",
        "git commit -m \"x; rm -rf ~\"",
        "rm file.txt",
        "X=1 cargo build",
        "export PATH=/x:$PATH && cargo build",
        "cat <<EOF\nrm -rf ~\nEOF",
    ];
    for cmd in must_allow {
        assert_eq!(user_deny_refusal(&list, cmd), None, "{cmd:?} must pass");
    }
}

/// The two refusal reasons are distinguishable, and the segment that matched
/// is named so the user can see which part of a compound tripped the rule.
#[test]
fn user_deny_refusal_names_segment_or_expansion() {
    use crate::tools::user_deny_refusal;
    let list = vec!["rm -rf *".to_string()];
    let seg = user_deny_refusal(&list, "cd x && rm -rf ~").unwrap();
    assert_eq!(
        seg,
        "Command blocked by your permission rules: the part `rm -rf ~` matches deny pattern 'rm -rf *'."
    );
    let ind = user_deny_refusal(&list, "X=rm; $X -rf ~").unwrap();
    assert!(ind.contains("`$X`"), "{ind}");
    assert!(ind.contains("write the command plainly"), "{ind}");
    assert!(!ind.contains("matches deny pattern"), "{ind}");
    // The original whole-string refusal text is unchanged for a plain hit.
    assert_eq!(
        user_deny_refusal(&list, "rm -rf ~").unwrap(),
        "Command blocked by your permission rules: matches deny pattern 'rm -rf *'."
    );
    // A hostile control byte in the model's spelling never reaches the text raw.
    let hostile = user_deny_refusal(&list, "cd x && rm -rf ~\u{1b}[2J").unwrap();
    assert!(
        "cd x && rm -rf ~\u{1b}[2J".as_bytes().contains(&0x1b),
        "fixture carries the hostile byte"
    );
    assert!(!hostile.as_bytes().contains(&0x1b), "{hostile:?}");
}

/// No deny list -> byte-identical to before: nothing is refused, including the
/// indirect spellings. With a deny list, `"$CARGO" build` IS refused. That is
/// the trade #1010 left open, decided here: a user who wrote a deny list asked
/// for a fence, and a fence any `$VAR` walks through is not one. The refusal
/// says how to proceed (write the command plainly). Users without a deny list,
/// including this repo's own evolve loop (its `.yoyo.toml` sets none), are
/// outside the narrowed population.
#[test]
fn indirect_command_word_refused_only_when_a_deny_list_exists() {
    use crate::tools::user_deny_refusal;
    let empty: Vec<String> = vec![];
    for cmd in [
        "\"$CARGO\" build",
        "X=rm; $X -rf ~",
        "$(echo rm) -rf ~",
        "cd x && rm -rf ~",
        "rm -rf ~",
    ] {
        assert_eq!(user_deny_refusal(&empty, cmd), None, "{cmd:?}");
    }
    let list = vec!["rm -rf *".to_string()];
    assert!(user_deny_refusal(&list, "\"$CARGO\" build").is_some());
}

/// #1011 (flipped from the Day-225 stated-limit pin, as that test asked its
/// closer to do): with a deny list set, `eval` of a command string assembled
/// at runtime is refused, because no spelling check can judge text that only
/// exists when the substitution runs. The refusal names eval and the way out.
#[test]
fn eval_of_an_assembled_command_string_is_refused_with_a_deny_list() {
    use crate::tools::user_deny_refusal;
    let list = vec!["rm -rf *".to_string()];
    // Anti-vacuous: the plain twin is refused by the ordinary segment match.
    assert!(user_deny_refusal(&list, "rm -rf x").is_some());
    for cmd in [
        "eval \"$(printf '%s %s %s' rm -rf ~)\"",
        "eval \"$(printf '%s %s' rm -rf x)\"",
        "eval `printf hi`",
        "FOO=1 eval \"$(ssh-agent)\"",
        "export A=1; eval \"$(ssh-agent)\"",
        "cd x && eval $(ssh-agent -s)",
        "builtin eval \"$(x)\"",
        "eval \"echo $(id)\"",
        "bash -c 'eval \"$(x)\"'",
    ] {
        let r = user_deny_refusal(&list, cmd).unwrap_or_else(|| panic!("{cmd:?} passed"));
        assert!(r.contains("eval"), "{cmd:?}: {r}");
        assert!(r.contains("outside yoyo"), "{cmd:?}: {r}");
    }
}

/// #1011 near-misses: only `eval` with a substitution in its arguments is
/// refused. A literal eval, a substitution under another program, and a
/// program whose name merely starts with "eval" all pass.
#[test]
fn eval_refusal_near_misses_pass_with_a_deny_list() {
    use crate::tools::user_deny_refusal;
    let list = vec!["rm -rf *".to_string()];
    for cmd in [
        "eval \"echo hi\"",
        "eval echo hi",
        "echo \"$(date)\"",
        "evaluate \"$(x)\"",
        "echo eval \"$(x)\"",
    ] {
        assert_eq!(user_deny_refusal(&list, cmd), None, "{cmd:?}");
    }
}

/// #1011: with no deny list, nothing changes: `eval "$(ssh-agent)"` and the
/// assembled form both pass the user-deny check (the analyser's own
/// `whole_substitution` exemption is untouched).
#[test]
fn eval_of_a_substitution_passes_without_a_deny_list() {
    use crate::tools::user_deny_refusal;
    let empty: Vec<String> = vec![];
    for cmd in [
        "eval \"$(ssh-agent)\"",
        "eval \"$(printf '%s %s' rm -rf x)\"",
        "eval `printf hi`",
    ] {
        assert_eq!(user_deny_refusal(&empty, cmd), None, "{cmd:?}");
    }
}
