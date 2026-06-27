//! `/goal` command handler — persistent session/project goals.
//!
//! Goals are stored as plain text in `.yoyo/goal.md`, making them
//! human-readable and version-controllable.

use crate::dispatch::CommandResult;
use crate::format::*;
use std::fs;
use std::path::Path;

/// Default goal file path (project-local).
const GOAL_FILE: &str = ".yoyo/goal.md";

/// Verify command file path (project-local).
const VERIFY_FILE: &str = ".yoyo/goal_verify.md";

/// Maximum characters of verify output to include in prompts.
const VERIFY_OUTPUT_MAX: usize = 2000;

/// Load the current goal from `.yoyo/goal.md`, if it exists.
pub fn load_goal() -> Option<String> {
    let path = Path::new(GOAL_FILE);
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
            let truncated = safe_truncate(&combined, VERIFY_OUTPUT_MAX).to_string();
            (code, truncated)
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
    let cmd = load_verify_command()?;
    let (code, output) = run_verify_command(&cmd);
    let passed = code == 0;

    if passed {
        eprintln!("{GREEN}  ✓ Goal verify passed{RESET}");
    } else {
        // Show truncated output so the agent knows what failed
        let display = if output.len() > VERIFY_OUTPUT_MAX {
            let truncated = safe_truncate(&output, VERIFY_OUTPUT_MAX);
            format!("{truncated}… (truncated)")
        } else {
            output.clone()
        };
        eprintln!("{YELLOW}  ⚠ Goal verify failed (exit {code}):{RESET}");
        for line in display.lines().take(10) {
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
        match load_goal() {
            Some(goal) => {
                let verify_section = if let Some(vcmd) = load_verify_command() {
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
            save_verify_command("exit 1").unwrap();
            let result = run_goal_verify_after_prompt();
            assert!(result.is_some());
            let (passed, _output) = result.unwrap();
            assert!(!passed);
        });
    }
}
