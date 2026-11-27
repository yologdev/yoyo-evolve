//! #1231: one row per population of the policy table, the refusal at the
//! bash tool, and the remedy's round trip through the real parser.

use super::*;
use crate::tools::{child_unattended_refusal, StreamingBashTool};
use serial_test::serial;
use std::collections::HashMap;
use yoagent::types::AgentTool;
use WarnedCommandPolicy::{Ask, Refuse, Run};

/// Warned (recursive chmod 777) and harmless: it only touches a file this
/// test creates inside its own tempdir, so a red run (guard neutered) does
/// nothing outside it.
const WARNED: &str = "touch RAN && chmod -R 777 RAN";

#[test]
fn every_row_of_the_table() {
    // (config_auto_approve, explicit_yes, prompt_on_command_line, tty) -> policy
    type Inputs = (bool, bool, bool, bool);
    let rows: &[(Inputs, WarnedCommandPolicy, &str)] = &[
        ((false, false, false, true), Ask, "REPL, no --yes"),
        ((true, true, false, true), Run, "REPL with --yes"),
        ((true, true, true, true), Run, "--yes -p from a terminal"),
        ((true, true, true, false), Run, "--yes -p, piped"),
        ((false, false, true, true), Refuse, "-p from a terminal"),
        (
            (false, false, true, false),
            Refuse,
            "-p with stdin piped too",
        ),
        (
            (true, false, true, true),
            Refuse,
            "--print -p (print is not --yes)",
        ),
        (
            (false, false, false, false),
            Run,
            "piped stdin, no -p: unchanged",
        ),
        (
            (true, false, false, false),
            Run,
            "--print, piped: unchanged",
        ),
        (
            (true, false, false, true),
            Run,
            "--print in a REPL: unchanged",
        ),
    ];
    for &((auto, yes, prompt, tty), want, label) in rows {
        assert_eq!(
            warned_command_policy(auto, yes, prompt, tty),
            want,
            "row: {label}"
        );
    }
}

fn parsed(rest: &[&str]) -> crate::cli_config::Config {
    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    let mut args = vec!["yoyo".to_string()];
    args.extend(rest.iter().map(|s| s.to_string()));
    crate::cli::parse_args_with_config(&args, HashMap::new(), String::new()).expect("should parse")
}

fn policy_of(rest: &[&str]) -> WarnedCommandPolicy {
    let c = parsed(rest);
    warned_command_policy(c.auto_approve, c.explicit_yes, c.prompt_arg.is_some(), true)
}

/// The remedy names `--yes`; that flag must really parse as an explicit yes
/// beside `-p`, so following the advice moves the run to the Run row.
#[test]
#[serial]
fn the_remedy_flag_round_trips_through_the_parser() {
    assert!(UNATTENDED_REMEDY.contains("rerun with --yes."));
    assert_eq!(policy_of(&["-p", "clean up"]), Refuse);
    assert_eq!(policy_of(&["--yes", "-p", "clean up"]), Run);
    assert_eq!(policy_of(&["-p", "clean up", "--yes"]), Run);
    assert_eq!(policy_of(&["-y", "--prompt", "clean up"]), Run);
    assert_eq!(policy_of(&["--print", "-p", "clean up"]), Refuse);
    assert_eq!(policy_of(&["clean up"]), Refuse, "bare prompt word");
    assert_eq!(policy_of(&[]), Ask, "REPL on a terminal");
}

fn bash_in(dir: &std::path::Path, refuse: bool) -> StreamingBashTool {
    let mut bash = StreamingBashTool::default().with_cwd(dir.to_string_lossy().to_string());
    bash.refuse_warned = refuse;
    bash
}

async fn run(tool: &StreamingBashTool, cmd: &str) -> Result<String, String> {
    let ctx = yoagent::types::ToolContext::new("call-1231", "bash");
    match tool
        .execute(serde_json::json!({ "command": cmd }), ctx)
        .await
    {
        Ok(r) => Ok(format!("{:?}", r.content)),
        Err(e) => Err(e.to_string()),
    }
}

#[tokio::test]
async fn a_warned_command_is_refused_with_reason_and_remedy_and_never_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bash = bash_in(dir.path(), true);
    let err = run(&bash, WARNED).await.expect_err("must be refused");
    assert_eq!(
        err,
        "Command needs user confirmation that this run cannot obtain: Recursive \
         permission change: 'chmod -R 777' makes everything world-writable Nobody can \
         answer a confirmation prompt in this run (a prompt given on the command line \
         without --yes), so it was not run. To let commands like this run without \
         asking, rerun with --yes."
    );
    assert!(!dir.path().join("RAN").exists(), "the refused command ran");
    // Near-miss: an unwarned command still runs under the same tool.
    run(&bash, "touch OK").await.expect("unwarned command runs");
    assert!(dir.path().join("OK").exists());
}

/// Explicit --yes and piped stdin: the warned command runs, as before.
#[tokio::test]
async fn without_the_refusal_the_same_warned_command_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    run(&bash_in(dir.path(), false), WARNED)
        .await
        .expect("runs when not refusing");
    assert!(dir.path().join("RAN").exists());
}

/// A sub-agent under a refusing parent says exactly what the parent says.
#[test]
fn a_child_refuses_with_the_parents_text() {
    let warning = crate::safety::analyze_bash_command(WARNED).expect("fixture is warned");
    assert_eq!(
        child_unattended_refusal(true, WARNED, None),
        Some(unattended_refusal(&warning))
    );
    assert_eq!(child_unattended_refusal(false, WARNED, None), None);
    assert_eq!(child_unattended_refusal(true, "touch OK", None), None);
}

/// #1229: the working folder is the one the bash tool runs in (its pinned
/// `with_cwd`, as spawn workers set it), not the process's. If this ever fails
/// the command runs, and what it deletes is a folder inside this temp dir.
#[tokio::test]
async fn the_pinned_working_folder_by_its_absolute_path_is_refused_unattended() {
    let dir = tempfile::tempdir().expect("tempdir");
    let proj = std::fs::canonicalize(dir.path()).unwrap().join("proj");
    std::fs::create_dir(&proj).unwrap();
    let cmd = format!("rm -rf {}", proj.display());
    let err = run(&bash_in(&proj, true), &cmd).await.expect_err("refused");
    assert!(
        err.contains(&format!(
            "recursive delete targeting the working folder '{}'",
            proj.display()
        )),
        "{err}"
    );
    assert!(proj.exists(), "the refused delete ran");
}
