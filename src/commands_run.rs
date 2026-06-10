//! Run and loop command handlers: /run, /loop.

use crate::agent_builder::AgentConfig;
use crate::commands::auto_compact_if_needed;
use crate::format::*;
use crate::prompt::run_prompt_auto_retry;
use crate::session::SessionChanges;
use crate::sync_util::lock_or_recover;

use std::sync::Mutex;
use yoagent::agent::Agent;
use yoagent::*;

/// Result of running a shell command via `/run` or `!`.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub elapsed: std::time::Duration,
    pub success: bool,
}

/// Last failed run result, stored so `/fix` or the agent can reference it.
static LAST_FAILED_RUN: Mutex<Option<RunResult>> = Mutex::new(None);

/// Retrieve the last failed run result (if any).
pub fn get_last_failed_run() -> Option<RunResult> {
    lock_or_recover(&LAST_FAILED_RUN).clone()
}

/// Store a failed run result.
fn set_last_failed_run(result: RunResult) {
    *lock_or_recover(&LAST_FAILED_RUN) = Some(result);
}

/// Clear the last failed run result (e.g. after a successful run).
fn clear_last_failed_run() {
    *lock_or_recover(&LAST_FAILED_RUN) = None;
}

/// Run a shell command, streaming output in real-time and returning a [`RunResult`].
pub fn run_shell_command(cmd: &str) -> RunResult {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let start = std::time::Instant::now();
    let child = Command::new("sh")
        .args(["-c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}  error running command: {e}{RESET}\n");
            return RunResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: format!("error running command: {e}"),
                elapsed: start.elapsed(),
                success: false,
            };
        }
    };

    // Read stderr in a background thread so we don't block on either pipe.
    // Collect lines into a buffer alongside printing.
    let stderr_pipe = child.stderr.take().expect("stderr was piped");
    let stderr_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr_pipe);
        let mut lines = Vec::new();
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    eprintln!("{RED}{l}{RESET}");
                    lines.push(l);
                }
                Err(_) => break,
            }
        }
        lines
    });

    // Stream stdout line-by-line on the main thread, collecting into a buffer.
    let mut stdout_lines = Vec::new();
    if let Some(stdout_pipe) = child.stdout.take() {
        let reader = BufReader::new(stdout_pipe);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    println!("{l}");
                    stdout_lines.push(l);
                }
                Err(_) => break,
            }
        }
    }

    // Wait for stderr thread to finish
    let stderr_lines = stderr_handle.join().unwrap_or_default();
    let elapsed = start.elapsed();

    // Collect exit status
    match child.wait() {
        Ok(status) => {
            let code = status.code().unwrap_or(-1);
            let success = code == 0;
            RunResult {
                exit_code: code,
                stdout: stdout_lines.join("\n"),
                stderr: stderr_lines.join("\n"),
                elapsed,
                success,
            }
        }
        Err(e) => {
            eprintln!("{RED}  error waiting for command: {e}{RESET}\n");
            RunResult {
                exit_code: -1,
                stdout: stdout_lines.join("\n"),
                stderr: format!(
                    "{}\nerror waiting for command: {e}",
                    stderr_lines.join("\n")
                ),
                elapsed,
                success: false,
            }
        }
    }
}

/// Print a [`RunResult`] summary line (exit code + elapsed time).
pub fn print_run_result(result: &RunResult) {
    let elapsed = format_duration(result.elapsed);
    if result.success {
        println!("{DIM}  ✓ exit {} ({elapsed}){RESET}\n", result.exit_code);
    } else {
        println!("{RED}  ✗ exit {} ({elapsed}){RESET}", result.exit_code);
        // Show a brief stderr preview if available
        if !result.stderr.is_empty() {
            let preview: String = result
                .stderr
                .lines()
                .take(3)
                .collect::<Vec<_>>()
                .join("\n    ");
            println!("{DIM}    {preview}{RESET}");
        }
        if !result.stdout.is_empty() {
            // Check if stdout has error-like content (common for test runners)
            let error_lines: Vec<&str> = result
                .stdout
                .lines()
                .filter(|l| {
                    let lower = l.to_lowercase();
                    lower.contains("error") || lower.contains("failed") || lower.contains("panic")
                })
                .take(3)
                .collect();
            if !error_lines.is_empty() {
                let preview = error_lines.join("\n    ");
                println!("{DIM}    {preview}{RESET}");
            }
        }
        println!(
            "{DIM}  💡 Command failed. Ask me to analyze the error, or say /fix to auto-fix.{RESET}\n"
        );
    }
}

pub fn handle_run(input: &str) {
    let cmd = if input.starts_with("/run ") {
        input.trim_start_matches("/run ").trim()
    } else if input.starts_with('!') && input.len() > 1 {
        input[1..].trim()
    } else {
        ""
    };
    if cmd.is_empty() {
        println!("{DIM}  usage: /run <command>  or  !<command>{RESET}\n");
    } else {
        let result = run_shell_command(cmd);
        print_run_result(&result);
        if result.success {
            clear_last_failed_run();
        } else {
            set_last_failed_run(result);
        }
    }
}

pub fn handle_run_usage() {
    println!("{DIM}  usage: /run <command>  or  !<command>");
    println!("  Runs a shell command directly (no AI, no tokens).{RESET}\n");
}

/// How many times to iterate in a `/loop` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopMode {
    /// Run exactly N times (1..=100).
    Count(usize),
    /// Run until the last tool call succeeds (max 20 iterations).
    UntilPass,
}

const MAX_UNTIL_PASS: usize = 20;
const MAX_LOOP_COUNT: usize = 100;

/// Parse `/loop <N|until-pass> <prompt>`.
///
/// Returns `None` if the input is malformed (missing args, zero count, etc.).
/// Counts above [`MAX_LOOP_COUNT`] are clamped silently.
pub fn parse_loop_args(input: &str) -> Option<(LoopMode, String)> {
    let rest = input.strip_prefix("/loop").unwrap_or(input).trim_start();
    if rest.is_empty() {
        return None;
    }

    // Split into mode token and the remaining prompt.
    let (mode_tok, prompt) = match rest.split_once(char::is_whitespace) {
        Some((m, p)) => (m, p.trim()),
        None => return None, // e.g. "/loop 5" with no prompt
    };

    if prompt.is_empty() {
        return None;
    }

    let mode = if mode_tok == "until-pass" {
        LoopMode::UntilPass
    } else if let Ok(n) = mode_tok.parse::<usize>() {
        if n == 0 {
            return None;
        }
        LoopMode::Count(n.min(MAX_LOOP_COUNT))
    } else {
        return None;
    };

    Some((mode, prompt.to_string()))
}

/// Run a prompt in a polling loop.
pub async fn handle_loop(
    input: &str,
    agent: &mut Agent,
    session_total: &mut Usage,
    agent_config: &AgentConfig,
    changes: &SessionChanges,
) {
    let (mode, prompt) = match parse_loop_args(input) {
        Some(v) => v,
        None => {
            println!(
                "{DIM}Usage: /loop <N|until-pass> <prompt>\n\
                 \n  /loop 5 run the tests and fix any failures\
                 \n  /loop until-pass run cargo test{RESET}"
            );
            return;
        }
    };

    let max_iters = match &mode {
        LoopMode::Count(n) => *n,
        LoopMode::UntilPass => MAX_UNTIL_PASS,
    };

    for i in 1..=max_iters {
        // Print iteration header.
        let label = match &mode {
            LoopMode::Count(n) => format!("--- loop iteration {i}/{n} ---"),
            LoopMode::UntilPass => {
                format!("--- loop iteration {i} (until-pass, max {MAX_UNTIL_PASS}) ---")
            }
        };
        println!("\n{BOLD}{CYAN}{label}{RESET}\n");

        let outcome =
            run_prompt_auto_retry(agent, &prompt, session_total, &agent_config.model, changes)
                .await;

        auto_compact_if_needed(agent);

        // For until-pass mode: stop when the last tool call succeeded (no error).
        if mode == LoopMode::UntilPass && outcome.last_tool_error.is_none() {
            println!(
                "\n{GREEN}{BOLD}✓ Loop complete — last tool call succeeded on iteration {i}.{RESET}"
            );
            return;
        }

        // Don't sleep after the last iteration.
        if i < max_iters {
            // Brief pause so the user can Ctrl+C between iterations.
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    // Finished all iterations.
    match &mode {
        LoopMode::Count(n) => {
            println!("\n{DIM}Loop complete — {n} iterations finished.{RESET}");
        }
        LoopMode::UntilPass => {
            println!(
                "\n{YELLOW}Loop exhausted {MAX_UNTIL_PASS} iterations without a passing tool call.{RESET}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that read/write the global `LAST_FAILED_RUN` state
    /// to prevent race conditions when tests run in parallel.
    static FAILED_RUN_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_run_result_success() {
        let result = run_shell_command("echo hello");
        assert!(result.success);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello");
        assert!(result.stderr.is_empty());
        assert!(result.elapsed.as_secs() < 10);
    }

    #[test]
    fn test_run_result_failure() {
        let result = run_shell_command("echo oops >&2; exit 42");
        assert!(!result.success);
        assert_eq!(result.exit_code, 42);
        assert_eq!(result.stderr, "oops");
    }

    #[test]
    fn test_run_shell_command_streams_multiline() {
        let result = run_shell_command("echo line1; echo line2; echo line3");
        assert!(result.success);
        assert_eq!(result.stdout, "line1\nline2\nline3");
    }

    #[test]
    fn test_run_shell_command_mixed_stdout_stderr() {
        // Both stdout and stderr should be handled without deadlock or panic
        let result = run_shell_command("echo out; echo err >&2; echo out2");
        assert!(result.stdout.contains("out"));
        assert!(result.stderr.contains("err"));
    }

    #[test]
    fn test_run_shell_command_large_output() {
        // Ensure streaming handles larger output without buffering issues
        let result = run_shell_command("seq 1 100");
        assert!(result.success);
        assert!(result.stdout.contains("100"));
    }

    #[test]
    fn test_last_failed_run_initially_none() {
        let _guard = FAILED_RUN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Clear any state from other tests
        clear_last_failed_run();
        assert!(get_last_failed_run().is_none());
    }

    #[test]
    fn test_last_failed_run_store_and_retrieve() {
        let _guard = FAILED_RUN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let result = RunResult {
            exit_code: 1,
            stdout: "some output".to_string(),
            stderr: "error msg".to_string(),
            elapsed: std::time::Duration::from_millis(123),
            success: false,
        };
        set_last_failed_run(result);
        let stored = get_last_failed_run();
        assert!(stored.is_some());
        let stored = stored.unwrap();
        assert_eq!(stored.exit_code, 1);
        assert_eq!(stored.stdout, "some output");
        assert_eq!(stored.stderr, "error msg");
        assert!(!stored.success);
        // Clean up
        clear_last_failed_run();
    }

    #[test]
    fn test_last_failed_run_cleared_on_success() {
        let _guard = FAILED_RUN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_last_failed_run(RunResult {
            exit_code: 1,
            stdout: String::new(),
            stderr: "fail".to_string(),
            elapsed: std::time::Duration::from_millis(10),
            success: false,
        });
        assert!(get_last_failed_run().is_some());
        clear_last_failed_run();
        assert!(get_last_failed_run().is_none());
    }

    #[test]
    fn test_print_run_result_hint_on_failure() {
        // Just verify it doesn't panic — output goes to stdout
        let result = RunResult {
            exit_code: 1,
            stdout: String::new(),
            stderr: "compile error".to_string(),
            elapsed: std::time::Duration::from_millis(500),
            success: false,
        };
        print_run_result(&result);
    }

    #[test]
    fn test_print_run_result_no_hint_on_success() {
        let result = RunResult {
            exit_code: 0,
            stdout: "ok".to_string(),
            stderr: String::new(),
            elapsed: std::time::Duration::from_millis(100),
            success: true,
        };
        print_run_result(&result);
    }

    #[test]
    fn test_bang_shortcut_matching() {
        // ! prefix should match for /run shortcut
        let bang_matches = |s: &str| s.starts_with('!') && s.len() > 1;
        assert!(bang_matches("!ls"));
        assert!(bang_matches("!echo hello"));
        assert!(bang_matches("! ls")); // space after bang is fine
        assert!(!bang_matches("!")); // bare bang alone should not match
    }

    #[test]
    fn test_run_command_matching() {
        // /run should only match /run or /run <cmd>, not /running
        let run_matches = |s: &str| s == "/run" || s.starts_with("/run ");
        assert!(run_matches("/run"));
        assert!(run_matches("/run echo hello"));
        assert!(!run_matches("/running"));
        assert!(!run_matches("/runaway"));
    }

    #[test]
    fn parse_loop_count_with_prompt() {
        let result = parse_loop_args("/loop 5 fix the tests");
        assert_eq!(
            result,
            Some((LoopMode::Count(5), "fix the tests".to_string()))
        );
    }

    #[test]
    fn parse_loop_until_pass() {
        let result = parse_loop_args("/loop until-pass cargo test");
        assert_eq!(
            result,
            Some((LoopMode::UntilPass, "cargo test".to_string()))
        );
    }

    #[test]
    fn parse_loop_missing_args() {
        assert_eq!(parse_loop_args("/loop"), None);
    }

    #[test]
    fn parse_loop_missing_prompt() {
        assert_eq!(parse_loop_args("/loop 5"), None);
    }

    #[test]
    fn parse_loop_zero_not_valid() {
        assert_eq!(parse_loop_args("/loop 0 something"), None);
    }

    #[test]
    fn parse_loop_capped_at_100() {
        let result = parse_loop_args("/loop 200 something");
        assert_eq!(
            result,
            Some((LoopMode::Count(100), "something".to_string()))
        );
    }

    #[test]
    fn parse_loop_invalid_mode() {
        assert_eq!(parse_loop_args("/loop abc do stuff"), None);
    }

    #[test]
    fn parse_loop_one_iteration() {
        let result = parse_loop_args("/loop 1 check it");
        assert_eq!(result, Some((LoopMode::Count(1), "check it".to_string())));
    }

    #[test]
    fn parse_loop_prompt_preserves_spaces() {
        let result = parse_loop_args("/loop 3 check if the server is responding");
        assert_eq!(
            result,
            Some((
                LoopMode::Count(3),
                "check if the server is responding".to_string()
            ))
        );
    }
}
