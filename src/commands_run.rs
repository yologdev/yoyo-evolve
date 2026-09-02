//! Run and loop command handlers: /run, /loop.

use crate::agent_builder::AgentConfig;
use crate::commands::auto_compact_if_needed;
use crate::format::*;
use crate::prompt::run_prompt_auto_retry;
use crate::session::SessionChanges;
use crate::sync_util::lock_or_recover;

use std::sync::Mutex;
use std::time::{Duration, Instant};
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

/// Bytes of the *head* of a captured stream kept in [`RunResult`].
///
/// Head + tail is 8 KB, deliberately the same budget as
/// `BANG_CAPTURE_MAX_BYTES` in `src/repl.rs` (the `!` shell-passthrough
/// capture). This comment is the only link between the two numbers — if one
/// moves, the other deserves a look.
///
/// Split head/tail rather than tail-only (the bang path's choice) because
/// compiler errors lead and test summaries trail: a tail-only cap throws away
/// the first `error[E0308]`, which is the most useful line for `/fix`.
const CAPTURE_HEAD_BYTES: usize = 4096;
/// Bytes of the *tail* of a captured stream kept in [`RunResult`]. See
/// [`CAPTURE_HEAD_BYTES`].
const CAPTURE_TAIL_BYTES: usize = 4096;

/// A bounded line collector for captured process output.
///
/// Live streaming is unaffected — the echo loops still print every line as it
/// arrives. Only the *stored* copy is bounded, so a `cargo test` that emits
/// 347 KB (or a `seq 1 500000` that emits 3.4 MB) can neither pin that much
/// memory in the `LAST_FAILED_RUN` global nor be pasted verbatim into a `/fix`
/// prompt. Every consumer of [`RunResult`] inherits the bound.
///
/// Under budget, [`CappedCapture::finish`] returns exactly what
/// `lines.join("\n")` returned before. Over budget, the cut is marked in-band:
/// a silent elision is the bug.
#[derive(Debug, Default)]
struct CappedCapture {
    head: Vec<String>,
    head_bytes: usize,
    tail: std::collections::VecDeque<String>,
    tail_bytes: usize,
    dropped_lines: usize,
    dropped_bytes: usize,
}

impl CappedCapture {
    fn push_line(&mut self, line: &str) {
        let total_budget = CAPTURE_HEAD_BYTES + CAPTURE_TAIL_BYTES;
        // A single line longer than the whole budget is truncated on a char
        // boundary — never a byte index, which panics inside a multi-byte char.
        let stored = if line.len() > total_budget {
            let mut b = total_budget;
            while b > 0 && !line.is_char_boundary(b) {
                b -= 1;
            }
            self.dropped_bytes += line.len() - b;
            line[..b].to_string()
        } else {
            line.to_string()
        };

        // A line costs its bytes plus the '\n' that will rejoin it, so the
        // budget describes the rendered string rather than the raw lines.
        let cost = stored.len() + 1;
        if self.head_bytes < CAPTURE_HEAD_BYTES {
            self.head_bytes += cost;
            self.head.push(stored);
            return;
        }

        self.tail_bytes += cost;
        self.tail.push_back(stored);
        // Keep at least one line so a single over-long line still shows.
        while self.tail_bytes > CAPTURE_TAIL_BYTES && self.tail.len() > 1 {
            if let Some(front) = self.tail.pop_front() {
                self.tail_bytes -= front.len() + 1;
                self.dropped_bytes += front.len();
                self.dropped_lines += 1;
            }
        }
    }

    fn elided(&self) -> bool {
        self.dropped_lines > 0 || self.dropped_bytes > 0
    }

    fn marker(&self) -> String {
        format!(
            "… [yoyo: {} lines / {} bytes elided — /run keeps the first {} KB and last {} KB]",
            self.dropped_lines,
            self.dropped_bytes,
            CAPTURE_HEAD_BYTES / 1024,
            CAPTURE_TAIL_BYTES / 1024,
        )
    }

    fn finish(self) -> String {
        if !self.elided() {
            // Byte-identical to the previous `lines.join("\n")`.
            let mut all = self.head;
            all.extend(self.tail);
            return all.join("\n");
        }
        let mut out = self.head.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&self.marker());
        if !self.tail.is_empty() {
            out.push('\n');
            out.push_str(
                &self
                    .tail
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        out
    }
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
                exit_code: crate::tools::EXIT_CODE_UNDETERMINED,
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
        let mut capture = CappedCapture::default();
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    eprintln!("{RED}{l}{RESET}");
                    capture.push_line(&l);
                }
                Err(_) => break,
            }
        }
        capture.finish()
    });

    // Stream stdout line-by-line on the main thread, collecting into a
    // *bounded* buffer — every line is still echoed, only the stored copy caps.
    let mut stdout_capture = CappedCapture::default();
    if let Some(stdout_pipe) = child.stdout.take() {
        let reader = BufReader::new(stdout_pipe);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    println!("{l}");
                    stdout_capture.push_line(&l);
                }
                Err(_) => break,
            }
        }
    }
    let stdout_text = stdout_capture.finish();

    // Wait for stderr thread to finish
    let stderr_text: String = stderr_handle.join().unwrap_or_default();
    let elapsed = start.elapsed();

    // Collect exit status
    match child.wait() {
        Ok(status) => {
            // Was `status.code().unwrap_or(-1)` — a signal death and "could not
            // determine" were the same number, and `-1` is literally SIGHUP (#878).
            let code = crate::tools::exit_code_of(&status);
            let success = code == 0;
            RunResult {
                exit_code: code,
                stdout: stdout_text,
                stderr: stderr_text,
                elapsed,
                success,
            }
        }
        Err(e) => {
            eprintln!("{RED}  error waiting for command: {e}{RESET}\n");
            RunResult {
                exit_code: crate::tools::EXIT_CODE_UNDETERMINED,
                stdout: stdout_text,
                stderr: format!("{stderr_text}\nerror waiting for command: {e}"),
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
        println!(
            "{RED}  ✗ exit {} ({elapsed}){RESET}",
            crate::tools::describe_exit_code(result.exit_code)
        );
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

    let loop_start = Instant::now();

    for i in 1..=max_iters {
        // Print iteration header.
        let label = match &mode {
            LoopMode::Count(n) => format!("--- loop iteration {i}/{n} ---"),
            LoopMode::UntilPass => {
                format!("--- loop iteration {i} (until-pass, max {MAX_UNTIL_PASS}) ---")
            }
        };
        println!("\n{BOLD}{CYAN}{label}{RESET}\n");

        let iter_start = Instant::now();
        let outcome =
            run_prompt_auto_retry(agent, &prompt, session_total, &agent_config.model, changes)
                .await;
        let iter_elapsed = iter_start.elapsed();
        let total_elapsed = loop_start.elapsed();

        // Print per-iteration timing.
        println!(
            "{DIM}⏱ Iteration {i} completed in {} (total: {}){RESET}",
            format_duration(iter_elapsed),
            format_duration(total_elapsed),
        );

        auto_compact_if_needed(agent);

        // For until-pass mode: stop when the last tool call succeeded (no error).
        if mode == LoopMode::UntilPass && outcome.last_tool_error.is_none() {
            println!(
                "\n{GREEN}{BOLD}✓ Loop complete — last tool call succeeded on iteration {i}.{RESET}"
            );
            let summary = format_loop_summary(
                i,
                max_iters,
                loop_start.elapsed(),
                true,
                &format!("condition met on iteration {i}"),
            );
            println!("\n{summary}");
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
            let summary = format_loop_summary(
                *n,
                *n,
                loop_start.elapsed(),
                true,
                "completed all iterations",
            );
            println!("\n{summary}");
        }
        LoopMode::UntilPass => {
            let summary = format_loop_summary(
                MAX_UNTIL_PASS,
                MAX_UNTIL_PASS,
                loop_start.elapsed(),
                false,
                &format!("condition not met after {MAX_UNTIL_PASS} iterations"),
            );
            println!("\n{summary}");
        }
    }
}

/// Format a structured loop summary for display.
///
/// `iterations` — number completed, `max` — maximum allowed,
/// `elapsed` — total wall-clock time, `success` — whether the goal was met,
/// `mode_label` — human-readable result description.
pub fn format_loop_summary(
    iterations: usize,
    max: usize,
    elapsed: Duration,
    success: bool,
    mode_label: &str,
) -> String {
    let time_str = format_duration(elapsed);
    if success {
        format!(
            "── Loop Summary ──────────────────────\n\
             Iterations: {iterations}/{max}\n\
             Total time: {time_str}\n\
             Result: {GREEN}✓ {mode_label}{RESET}"
        )
    } else {
        format!(
            "── Loop Summary ──────────────────────\n\
             Iterations: {iterations}/{max}\n\
             Total time: {time_str}\n\
             Result: {RED}✗ {mode_label}{RESET}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that read/write the global `LAST_FAILED_RUN` state
    /// to prevent race conditions when tests run in parallel.
    static FAILED_RUN_LOCK: Mutex<()> = Mutex::new(());

    fn capture_of(lines: &[&str]) -> CappedCapture {
        let mut c = CappedCapture::default();
        for l in lines {
            c.push_line(l);
        }
        c
    }

    /// The regression risk of the whole cap: small output is the common case
    /// and must not change by a single byte.
    #[test]
    fn test_capped_capture_under_budget_is_byte_identical_to_join() {
        let lines = vec!["error[E0308]: mismatched types", "  --> src/x.rs:1:1", ""];
        let joined = lines.join("\n");
        assert_eq!(capture_of(&lines).finish(), joined);
        // A stream that fills the head but stays under total budget still
        // round-trips exactly — head + tail rejoin with no marker.
        let big: Vec<String> = (0..200)
            .map(|i| format!("line {i} {}", "x".repeat(20)))
            .collect();
        let refs: Vec<&str> = big.iter().map(String::as_str).collect();
        let total: usize = big.iter().map(|l| l.len()).sum();
        assert!(total < CAPTURE_HEAD_BYTES + CAPTURE_TAIL_BYTES);
        assert!(total > CAPTURE_HEAD_BYTES, "should overflow the head");
        assert_eq!(capture_of(&refs).finish(), big.join("\n"));
    }

    #[test]
    fn test_capped_capture_empty_output_has_no_marker() {
        assert_eq!(CappedCapture::default().finish(), "");
    }

    #[test]
    fn test_capped_capture_over_budget_keeps_head_and_tail_and_marks_the_cut() {
        let lines: Vec<String> = (0..20_000).map(|i| format!("line {i}")).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let cap = capture_of(&refs);
        let dropped_lines = cap.dropped_lines;
        let dropped_bytes = cap.dropped_bytes;
        assert!(dropped_lines > 0, "expected lines to be elided");
        let out = cap.finish();

        assert!(
            out.starts_with("line 0\n"),
            "head must survive: {:?}",
            &out[..40]
        );
        assert!(out.ends_with("line 19999"), "tail must survive");
        assert!(out.contains(&format!(
            "{dropped_lines} lines / {dropped_bytes} bytes elided"
        )));
        assert!(out.contains("/run keeps the first 4 KB and last 4 KB"));
        // Bounded: head + tail + one marker line, nowhere near the raw size.
        assert!(out.len() < CAPTURE_HEAD_BYTES + CAPTURE_TAIL_BYTES + 512);
        assert!(lines.join("\n").len() > 10 * out.len());
        // Accounting adds up: what is kept plus what was dropped is the whole.
        let kept: usize = out
            .lines()
            .filter(|l| !l.contains("elided"))
            .map(str::len)
            .sum();
        assert!(kept + dropped_bytes <= lines.iter().map(|l| l.len()).sum::<usize>());
    }

    #[test]
    fn test_capped_capture_multibyte_content_across_the_boundary_is_valid_utf8() {
        // Every line is multi-byte, so any byte-index cut lands mid-character.
        let lines: Vec<String> = (0..4000).map(|i| format!("✓ 检查 {i} 🐙")).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let out = capture_of(&refs).finish();
        assert!(out.contains("elided"));
        assert!(out.starts_with("✓ 检查 0 🐙"));
        assert!(out.ends_with("✓ 检查 3999 🐙"));
        // Valid UTF-8 by construction (it is a String); assert the chars survived.
        assert!(out.chars().filter(|c| *c == '🐙').count() > 1);
    }

    #[test]
    fn test_capped_capture_single_line_longer_than_budget_is_cut_on_a_char_boundary() {
        // One line, no newlines, longer than head+tail — the only case where a
        // cut can land inside a character.
        let huge = "🐙".repeat(CAPTURE_HEAD_BYTES + CAPTURE_TAIL_BYTES);
        assert!(huge.len() > CAPTURE_HEAD_BYTES + CAPTURE_TAIL_BYTES);
        let out = capture_of(&[huge.as_str()]).finish();
        assert!(
            out.contains("bytes elided"),
            "cut must be marked: {out:.120}"
        );
        assert!(out.len() < huge.len());
        assert!(out.starts_with('🐙'));
        // No lone replacement chars / no panic: the kept prefix is whole octopi.
        let kept = out.lines().next().unwrap_or_default();
        assert!(kept.chars().all(|c| c == '🐙'), "prefix cut mid-character");
    }

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

    // --- signal deaths are named, `-1` no longer doubles as "could not wait" (#878) ---

    /// The emission point is `RunResult.exit_code` — the value `/run` prints and
    /// the value `/fix` consumes out of `LAST_FAILED_RUN`. Driven end-to-end
    /// through a real shell, the way every other test in this module does.
    #[cfg(unix)]
    #[test]
    fn run_shell_command_reports_a_signal_death_as_the_negative_signal() {
        // `kill -9 $$` kills the shell itself, so the child dies on SIGKILL and
        // has no exit code at all — the case that used to collapse to -1.
        let result = run_shell_command("kill -9 $$");
        assert!(!result.success);
        assert_eq!(
            result.exit_code,
            -9,
            "a SIGKILL death must be reported as -9, not as -1"
        );
        // And it must not be confused with the wait-error branch.
        assert_ne!(result.exit_code, crate::tools::EXIT_CODE_UNDETERMINED);
    }

    /// Near-miss guard, and the whole regression surface: every existing `/run`
    /// user is on this path. An ordinary non-zero exit is byte-identical to
    /// before — a discriminator tested only on the side that fires is vacuous
    /// green.
    #[test]
    fn run_shell_command_leaves_an_ordinary_exit_code_untouched() {
        let result = run_shell_command("exit 3");
        assert!(!result.success);
        assert_eq!(result.exit_code, 3);

        let ok = run_shell_command("exit 0");
        assert!(ok.success);
        assert_eq!(ok.exit_code, 0);

        // The rendered text `print_run_result` builds is the bare number for an
        // ordinary code, exactly as it was before #878.
        assert_eq!(crate::tools::describe_exit_code(3), "3");
        assert_eq!(crate::tools::describe_exit_code(0), "0");
    }

    /// The un-overloading itself: the "could not wait for the child" branch
    /// returns a sentinel no signal and no exit status can produce, so `-1`
    /// stops meaning two things at once.
    #[test]
    fn the_wait_error_sentinel_is_not_a_value_any_signal_could_produce() {
        let undetermined = crate::tools::EXIT_CODE_UNDETERMINED;
        // -1 is SIGHUP, so the sentinel must not be it.
        assert_ne!(undetermined, -1);
        // No signal number in the decodable range can produce it.
        for sig in 1..=64 {
            assert_ne!(-sig, undetermined);
        }
        // Nor can any ordinary exit code.
        for code in 0..=255 {
            assert_ne!(code, undetermined);
        }
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

    #[test]
    fn format_loop_summary_count_complete() {
        let summary = format_loop_summary(
            5,
            5,
            Duration::from_secs(252),
            true,
            "completed all iterations",
        );
        assert!(summary.contains("Iterations: 5/5"));
        assert!(summary.contains("Total time: 4m 12s"));
        assert!(summary.contains("completed all iterations"));
        assert!(summary.contains("✓"));
    }

    #[test]
    fn format_loop_summary_until_pass_success() {
        let summary = format_loop_summary(
            3,
            20,
            Duration::from_secs(154),
            true,
            "condition met on iteration 3",
        );
        assert!(summary.contains("Iterations: 3/20"));
        assert!(summary.contains("Total time: 2m 34s"));
        assert!(summary.contains("condition met on iteration 3"));
        assert!(summary.contains("✓"));
    }

    #[test]
    fn format_loop_summary_until_pass_exhausted() {
        let summary = format_loop_summary(
            20,
            20,
            Duration::from_secs(903),
            false,
            "condition not met after 20 iterations",
        );
        assert!(summary.contains("Iterations: 20/20"));
        assert!(summary.contains("Total time: 15m 3s"));
        assert!(summary.contains("condition not met after 20 iterations"));
        assert!(summary.contains("✗"));
    }

    #[test]
    fn format_loop_summary_has_header() {
        let summary = format_loop_summary(1, 1, Duration::from_secs(5), true, "done");
        assert!(summary.contains("── Loop Summary ──"));
    }
}
