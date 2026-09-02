//! Background process management for `/bg` commands.
//! REPL dispatch wiring comes in the next task — these items are public API
//! consumed from `commands.rs` but not yet called from the binary entry point.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::format::{
    safe_truncate, safe_truncate_with_suffix, BOLD, CYAN, DIM, GREEN, RED, RESET, YELLOW,
};
use crate::sync_util::lock_or_recover;

/// Maximum bytes of output to buffer per background job (256KB, same as StreamingBashTool).
const MAX_OUTPUT_BYTES: usize = 256 * 1024;

/// Default number of tail lines shown by `/bg output`.
const DEFAULT_TAIL_LINES: usize = 50;

/// A background shell job with shared output state.
pub struct BackgroundJob {
    pub id: u32,
    pub command: String,
    pub started_at: Instant,
    pub output: Arc<Mutex<String>>,
    pub finished: Arc<AtomicBool>,
    pub exit_code: Arc<std::sync::Mutex<Option<i32>>>,
    /// How long the job actually ran, stamped once at completion (#736).
    /// `None` while the job is still running. Every writer that sets
    /// `finished` must stamp this, or the elapsed column falls back to the
    /// live clock and starts reporting "time since launch" as "runtime".
    pub runtime: Arc<std::sync::Mutex<Option<std::time::Duration>>>,
}

/// Tracks all background jobs and their associated task handles.
#[derive(Clone)]
pub struct BackgroundJobTracker {
    jobs: Arc<std::sync::Mutex<HashMap<u32, BackgroundJob>>>,
    handles: Arc<std::sync::Mutex<HashMap<u32, tokio::task::JoinHandle<()>>>>,
    next_id: Arc<AtomicU32>,
}

impl BackgroundJobTracker {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(std::sync::Mutex::new(HashMap::new())),
            handles: Arc::new(std::sync::Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU32::new(1)),
        }
    }

    /// Spawn a command in the background. Returns the job ID.
    pub fn launch(&self, command: &str) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let output = Arc::new(Mutex::new(String::new()));
        let finished = Arc::new(AtomicBool::new(false));
        let exit_code = Arc::new(std::sync::Mutex::new(None));
        let runtime = Arc::new(std::sync::Mutex::new(None));

        let job = BackgroundJob {
            id,
            command: command.to_string(),
            started_at: Instant::now(),
            output: Arc::clone(&output),
            finished: Arc::clone(&finished),
            exit_code: Arc::clone(&exit_code),
            runtime: Arc::clone(&runtime),
        };

        // Spawn the process in a tokio task
        let cmd_string = command.to_string();
        let out = Arc::clone(&output);
        let fin = Arc::clone(&finished);
        let code = Arc::clone(&exit_code);
        let rt = Arc::clone(&runtime);
        let job_start = job.started_at;

        let handle = tokio::spawn(async move {
            run_background_command(&cmd_string, out, fin, code, rt, job_start).await;
        });

        {
            let mut jobs = lock_or_recover(&self.jobs);
            jobs.insert(id, job);
        }
        {
            let mut handles = lock_or_recover(&self.handles);
            handles.insert(id, handle);
        }

        id
    }

    /// List all jobs as snapshots (id, command, finished, exit_code, elapsed).
    ///
    /// `elapsed` is the frozen runtime once the job has stamped one (#736);
    /// only a still-running job reads the live clock.
    pub fn list(&self) -> Vec<JobSnapshot> {
        let jobs = lock_or_recover(&self.jobs);
        let mut snapshots: Vec<JobSnapshot> = jobs
            .values()
            .map(|j| {
                let runtime = *lock_or_recover(&j.runtime);
                JobSnapshot {
                    id: j.id,
                    command: j.command.clone(),
                    finished: j.finished.load(Ordering::Relaxed),
                    exit_code: *lock_or_recover(&j.exit_code),
                    elapsed: runtime.unwrap_or_else(|| j.started_at.elapsed()),
                    runtime,
                }
            })
            .collect();
        snapshots.sort_by_key(|s| s.id);
        snapshots
    }

    /// Get the accumulated output for a job.
    pub async fn get_output(&self, id: u32) -> Option<String> {
        let output_arc = {
            let jobs = lock_or_recover(&self.jobs);
            jobs.get(&id).map(|j| Arc::clone(&j.output))
        };
        match output_arc {
            Some(out) => {
                let guard = out.lock().await;
                Some(guard.clone())
            }
            None => None,
        }
    }

    /// Kill a running job. Returns true if the job existed and was killed.
    pub async fn kill(&self, id: u32) -> bool {
        // Abort the tokio task
        let handle = {
            let mut handles = lock_or_recover(&self.handles);
            handles.remove(&id)
        };

        if let Some(h) = handle {
            h.abort();
            // Mark the job as finished
            let jobs = lock_or_recover(&self.jobs);
            if let Some(j) = jobs.get(&id) {
                // Stamp the runtime BEFORE flipping `finished` — a killed job
                // has stopped running, so its elapsed column must freeze too (#736).
                stamp_runtime(&j.runtime, j.started_at);
                j.finished.store(true, Ordering::Relaxed);
                let mut code = lock_or_recover(&j.exit_code);
                if code.is_none() {
                    // We killed it, but the waiter has not reported a code yet,
                    // so we do not know one. Deliberately NOT `-1`: since #878
                    // that is the encoding of a SIGHUP death, and this row would
                    // render a confident `exit -1 (SIGHUP)` for a job we killed
                    // ourselves. "Killed" is real; the code is undetermined.
                    *code = Some(crate::tools::EXIT_CODE_UNDETERMINED);
                }
            }
            true
        } else {
            false
        }
    }

    /// Check if a job ID exists.
    pub fn exists(&self, id: u32) -> bool {
        let jobs = lock_or_recover(&self.jobs);
        jobs.contains_key(&id)
    }

    /// Check if a job is finished.
    pub fn is_finished(&self, id: u32) -> bool {
        let jobs = lock_or_recover(&self.jobs);
        jobs.get(&id)
            .map(|j| j.finished.load(Ordering::Relaxed))
            .unwrap_or(false)
    }
}

/// A snapshot of a job's state (no Arc/Mutex — safe to print).
pub struct JobSnapshot {
    pub id: u32,
    pub command: String,
    pub finished: bool,
    pub exit_code: Option<i32>,
    /// Frozen runtime for a finished job, live clock for a running one.
    pub elapsed: std::time::Duration,
    /// `Some` once the job stamped its runtime at completion (#736).
    /// A `finished` job with `None` here never stamped one — `elapsed` then
    /// falls back to the live clock, which is NOT a runtime, so the renderer
    /// must not print it as one (see `elapsed_column`).
    pub runtime: Option<std::time::Duration>,
}

/// Run a shell command, streaming output into the shared buffer.
async fn run_background_command(
    command: &str,
    output: Arc<Mutex<String>>,
    finished: Arc<AtomicBool>,
    exit_code: Arc<std::sync::Mutex<Option<i32>>>,
    runtime: Arc<std::sync::Mutex<Option<std::time::Duration>>>,
    started_at: Instant,
) {
    use tokio::io::AsyncReadExt;
    use tokio::process::Command;

    let child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            let mut out = output.lock().await;
            out.push_str(&format!("Failed to spawn: {e}\n"));
            stamp_runtime(&runtime, started_at);
            finished.store(true, Ordering::Relaxed);
            let mut code = lock_or_recover(&exit_code);
            *code = Some(crate::tools::EXIT_CODE_UNDETERMINED);
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Read stdout and stderr concurrently
    let out_clone = Arc::clone(&output);
    let stdout_task = tokio::spawn(async move {
        if let Some(mut reader) = stdout {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]);
                        let mut out = out_clone.lock().await;
                        // Cap output at MAX_OUTPUT_BYTES
                        if out.len() < MAX_OUTPUT_BYTES {
                            let remaining = MAX_OUTPUT_BYTES - out.len();
                            if text.len() <= remaining {
                                out.push_str(&text);
                            } else {
                                out.push_str(safe_truncate(&text, remaining));
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    });

    let err_clone = Arc::clone(&output);
    let stderr_task = tokio::spawn(async move {
        if let Some(mut reader) = stderr {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]);
                        let mut out = err_clone.lock().await;
                        if out.len() < MAX_OUTPUT_BYTES {
                            let remaining = MAX_OUTPUT_BYTES - out.len();
                            if text.len() <= remaining {
                                out.push_str(&text);
                            } else {
                                out.push_str(safe_truncate(&text, remaining));
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    });

    // Wait for both readers to finish
    let _ = stdout_task.await;
    let _ = stderr_task.await;

    // Wait for the process to exit
    match child.wait().await {
        Ok(status) => {
            let mut code = lock_or_recover(&exit_code);
            // Was `status.code().unwrap_or(-1)` — a signal death and "could not
            // determine" were the same number, and `-1` is literally SIGHUP (#878).
            *code = Some(crate::tools::exit_code_of(&status));
        }
        Err(_) => {
            let mut code = lock_or_recover(&exit_code);
            *code = Some(crate::tools::EXIT_CODE_UNDETERMINED);
        }
    }

    stamp_runtime(&runtime, started_at);
    finished.store(true, Ordering::Relaxed);
}

/// Record how long a job ran, once. Called by every writer that marks a job
/// finished (normal exit, spawn failure, kill) — see `BackgroundJob::runtime`.
fn stamp_runtime(
    runtime: &Arc<std::sync::Mutex<Option<std::time::Duration>>>,
    started_at: Instant,
) {
    let mut slot = lock_or_recover(runtime);
    if slot.is_none() {
        *slot = Some(started_at.elapsed());
    }
}

/// Shown in the elapsed column for a job that is finished but never stamped a
/// runtime. Absence gets its own value: borrowing the live clock there would
/// print "time since launch" as if it were "how long the job took" (#736).
const UNKNOWN_ELAPSED: &str = "--";

/// The elapsed column exactly as `/bg list` prints it.
fn elapsed_column(job: &JobSnapshot) -> String {
    match (job.finished, job.runtime) {
        (true, None) => UNKNOWN_ELAPSED.to_string(),
        _ => format_elapsed(job.elapsed),
    }
}

/// Format elapsed duration for display.
fn format_elapsed(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Tail the last `n` lines of a string.
///
/// The cut is made immediately after a `\n`, which is always a char boundary —
/// so this never byte-indexes into the middle of a multi-byte character. The
/// previous implementation walked `line.len() + 1` per line, but `str::lines`
/// strips a trailing `\r`, so CRLF output undercounted the offset by one byte
/// per line and could slice inside a multi-byte char, panicking `/bg output`
/// (blind round 37, Day 165).
fn tail_lines(s: &str, n: usize) -> &str {
    let total = s.lines().count();
    if total <= n {
        return s;
    }
    let skip = total - n;
    let mut seen = 0;
    for (idx, _) in s.match_indices('\n') {
        seen += 1;
        if seen == skip {
            return &s[idx + 1..];
        }
    }
    // Fewer newlines than lines to skip (no trailing newline): nothing is left.
    ""
}

/// Handle the `/bg` command with subcommands.
pub async fn handle_bg(input: &str, tracker: &BackgroundJobTracker) {
    let input = input.trim();

    // Parse subcommand
    let (sub, rest) = match input.find(char::is_whitespace) {
        Some(pos) => (&input[..pos], input[pos..].trim()),
        None => {
            if input.is_empty() {
                ("list", "")
            } else {
                (input, "")
            }
        }
    };

    match sub {
        "run" => handle_bg_run(rest, tracker),
        "list" => handle_bg_list(tracker),
        "output" => handle_bg_output(rest, tracker).await,
        "kill" => handle_bg_kill(rest, tracker).await,
        _ => {
            eprintln!(
                "{RED}Unknown /bg subcommand: {sub}{RESET}\n\
                 Usage: /bg run <cmd> | /bg list | /bg output <id> | /bg kill <id>"
            );
        }
    }
}

fn handle_bg_run(command: &str, tracker: &BackgroundJobTracker) {
    if command.is_empty() {
        eprintln!("{RED}Usage: /bg run <command>{RESET}");
        return;
    }

    let id = tracker.launch(command);
    println!(
        "{GREEN}⚡ Background job {BOLD}[{id}]{RESET}{GREEN} started:{RESET} {DIM}{}{RESET}",
        truncate_command(command, 60)
    );
}

fn handle_bg_list(tracker: &BackgroundJobTracker) {
    let jobs = tracker.list();
    if jobs.is_empty() {
        println!("{DIM}No background jobs{RESET}");
        return;
    }

    println!("{BOLD}{CYAN}Background Jobs{RESET}");
    for job in &jobs {
        let status = if job.finished {
            match job.exit_code {
                Some(0) => format!("{GREEN}✓ done{RESET}"),
                // Name the signal when there is one (#878) — an ordinary code
                // renders byte-identically to before.
                Some(code) => format!(
                    "{RED}✗ exit {}{RESET}",
                    crate::tools::describe_exit_code(code)
                ),
                None => format!("{RED}✗ done{RESET}"),
            }
        } else {
            format!("{YELLOW}● running{RESET}")
        };

        let elapsed = elapsed_column(job);
        let cmd = truncate_command(&job.command, 50);
        println!(
            "  {BOLD}[{}]{RESET}  {status}  {DIM}{elapsed}{RESET}  {cmd}",
            job.id
        );
    }
}

async fn handle_bg_output(args: &str, tracker: &BackgroundJobTracker) {
    let (id_str, flags) = match args.find(char::is_whitespace) {
        Some(pos) => (&args[..pos], args[pos..].trim()),
        None => (args, ""),
    };

    let id = match id_str.parse::<u32>() {
        Ok(id) => id,
        Err(_) => {
            eprintln!("{RED}Usage: /bg output <id> [--all]{RESET}");
            return;
        }
    };

    if !tracker.exists(id) {
        eprintln!("{RED}No job with ID {id}{RESET}");
        return;
    }

    let show_all = flags.contains("--all");

    match tracker.get_output(id).await {
        Some(output) => {
            if output.is_empty() {
                println!("{DIM}(no output yet){RESET}");
            } else if show_all {
                print!("{output}");
            } else {
                let tail = tail_lines(&output, DEFAULT_TAIL_LINES);
                let total_lines = output.lines().count();
                if total_lines > DEFAULT_TAIL_LINES {
                    println!(
                        "{DIM}... ({} lines omitted, use --all to see everything){RESET}",
                        total_lines - DEFAULT_TAIL_LINES
                    );
                }
                print!("{tail}");
            }
        }
        None => {
            eprintln!("{RED}No job with ID {id}{RESET}");
        }
    }
}

async fn handle_bg_kill(args: &str, tracker: &BackgroundJobTracker) {
    let id_str = args.split_whitespace().next().unwrap_or("");

    let id = match id_str.parse::<u32>() {
        Ok(id) => id,
        Err(_) => {
            eprintln!("{RED}Usage: /bg kill <id>{RESET}");
            return;
        }
    };

    if tracker.is_finished(id) {
        println!("{DIM}Job [{id}] already finished{RESET}");
        return;
    }

    if tracker.kill(id).await {
        println!("{YELLOW}Killed job [{id}]{RESET}");
    } else {
        eprintln!("{RED}No running job with ID {id}{RESET}");
    }
}

/// Truncate a command string for display.
fn truncate_command(cmd: &str, max: usize) -> String {
    let cmd = cmd.lines().next().unwrap_or(cmd); // first line only
    if cmd.len() <= max {
        cmd.to_string()
    } else {
        safe_truncate_with_suffix(cmd, max.saturating_sub(1), "…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_tracker() -> BackgroundJobTracker {
        BackgroundJobTracker::new()
    }

    #[tokio::test]
    async fn test_launch_and_list() {
        let tracker = create_tracker();
        let id = tracker.launch("echo hello");
        assert_eq!(id, 1);

        // Wait for the short command to finish
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let jobs = tracker.list();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, 1);
        assert!(jobs[0].finished);
        assert_eq!(jobs[0].exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_output_capture() {
        let tracker = create_tracker();
        let id = tracker.launch("echo hello && echo world");

        // Wait for the command to finish
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let output = tracker.get_output(id).await.unwrap();
        assert!(
            output.contains("hello"),
            "output should contain 'hello': {output}"
        );
        assert!(
            output.contains("world"),
            "output should contain 'world': {output}"
        );
    }

    #[tokio::test]
    async fn test_kill_running() {
        let tracker = create_tracker();
        let id = tracker.launch("sleep 60");

        // Give it a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Should be running
        assert!(!tracker.is_finished(id));

        // Kill it
        let killed = tracker.kill(id).await;
        assert!(killed);

        // Should be marked finished
        assert!(tracker.is_finished(id));
    }

    #[tokio::test]
    async fn test_job_ids_increment() {
        let tracker = create_tracker();
        let id1 = tracker.launch("echo one");
        let id2 = tracker.launch("echo two");
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    fn test_tail_lines() {
        let text = "line1\nline2\nline3\nline4\nline5\n";
        let tail = tail_lines(text, 2);
        assert!(tail.contains("line4"));
        assert!(tail.contains("line5"));
        assert!(!tail.contains("line3"));
    }

    #[test]
    fn test_tail_lines_short() {
        let text = "line1\nline2\n";
        let tail = tail_lines(text, 5);
        assert_eq!(tail, text);
    }

    #[test]
    fn test_truncate_command() {
        let short = "echo hi";
        assert_eq!(truncate_command(short, 20), "echo hi");

        let long = "echo this is a very long command that should be truncated";
        let truncated = truncate_command(long, 20);
        assert!(truncated.len() <= 24); // 20 + "…" (3 bytes)
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn test_truncate_command_multibyte() {
        let cmd = "echo ✓✓✓✓✓✓✓✓✓✓";
        let truncated = truncate_command(cmd, 10);
        // Should not panic on multi-byte chars
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(std::time::Duration::from_secs(5)), "5s");
        assert_eq!(format_elapsed(std::time::Duration::from_secs(65)), "1m5s");
        assert_eq!(format_elapsed(std::time::Duration::from_secs(3665)), "1h1m");
    }

    #[tokio::test]
    async fn test_exists() {
        let tracker = create_tracker();
        assert!(!tracker.exists(1));
        let id = tracker.launch("echo hi");
        assert!(tracker.exists(id));
        assert!(!tracker.exists(99));
    }

    #[tokio::test]
    async fn test_failed_command() {
        let tracker = create_tracker();
        tracker.launch("exit 42");

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let jobs = tracker.list();
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].finished);
        assert_eq!(jobs[0].exit_code, Some(42));
    }

    // ── format_elapsed edge cases ──────────────────────────────────

    #[test]
    fn test_format_elapsed_zero() {
        assert_eq!(format_elapsed(std::time::Duration::from_secs(0)), "0s");
    }

    #[test]
    fn test_format_elapsed_seconds_only() {
        assert_eq!(format_elapsed(std::time::Duration::from_secs(42)), "42s");
    }

    #[test]
    fn test_format_elapsed_exactly_60s() {
        assert_eq!(format_elapsed(std::time::Duration::from_secs(60)), "1m0s");
    }

    #[test]
    fn test_format_elapsed_minutes_and_seconds() {
        assert_eq!(format_elapsed(std::time::Duration::from_secs(195)), "3m15s");
    }

    #[test]
    fn test_format_elapsed_exactly_3600s() {
        assert_eq!(format_elapsed(std::time::Duration::from_secs(3600)), "1h0m");
    }

    #[test]
    fn test_format_elapsed_hours_minutes() {
        assert_eq!(
            format_elapsed(std::time::Duration::from_secs(5400)),
            "1h30m"
        );
    }

    #[test]
    fn test_format_elapsed_large_duration() {
        // 2h 15m = 8100s
        assert_eq!(
            format_elapsed(std::time::Duration::from_secs(8100)),
            "2h15m"
        );
    }

    #[test]
    fn test_format_elapsed_59_seconds() {
        assert_eq!(format_elapsed(std::time::Duration::from_secs(59)), "59s");
    }

    // ── tail_lines edge cases ──────────────────────────────────────

    #[test]
    fn test_tail_lines_empty_string() {
        let tail = tail_lines("", 5);
        assert_eq!(tail, "");
    }

    #[test]
    fn test_tail_lines_single_line_request_one() {
        let tail = tail_lines("hello", 1);
        assert_eq!(tail, "hello");
    }

    #[test]
    fn test_tail_lines_exact_count() {
        // Request exactly as many lines as exist
        let text = "a\nb\nc";
        let tail = tail_lines(text, 3);
        assert_eq!(tail, text);
    }

    #[test]
    fn test_tail_lines_n_zero() {
        let text = "line1\nline2\nline3\n";
        let tail = tail_lines(text, 0);
        // 3 lines > 0, so it should try to return last 0 lines
        // The function returns s[byte_offset..] where start_line = lines.len() - 0 = lines.len()
        // which means byte_offset walks past everything → empty or last bit
        assert!(tail.is_empty() || tail == "\n" || tail.len() <= 1);
    }

    #[test]
    fn test_tail_lines_no_trailing_newline() {
        let text = "line1\nline2\nline3";
        let tail = tail_lines(text, 2);
        assert!(tail.contains("line2"));
        assert!(tail.contains("line3"));
        assert!(!tail.contains("line1"));
    }

    #[test]
    fn test_tail_lines_single_newline() {
        let tail = tail_lines("\n", 5);
        assert_eq!(tail, "\n");
    }

    #[test]
    fn test_tail_lines_multiple_empty_lines() {
        let text = "\n\n\n\n\n";
        let tail = tail_lines(text, 2);
        // 5 empty lines via .lines(), take last 2 → two newlines
        assert!(tail.len() <= text.len());
        // Should not panic and should return a valid substring
    }

    /// Blind round 37 (Day 165): `s.lines()` strips a trailing `\r`, so an offset
    /// walked as `line.len() + 1` undercounts by one byte per CRLF line. With
    /// enough CRLF lines the resulting index lands inside a multi-byte character
    /// and `&s[offset..]` panics — `/bg output <id>` crashing on a job whose
    /// output has Windows line endings and any non-ASCII byte.
    #[test]
    fn test_tail_lines_crlf_with_multibyte_does_not_panic() {
        let text = "aa✓\r\nbb✓\r\ncc✓\r\ndd✓\r\nee✓\r\n";
        let tail = tail_lines(text, 2);
        assert!(
            tail.ends_with("ee✓\r\n"),
            "tail should end at the last CRLF line: {tail:?}"
        );
        assert!(
            tail.starts_with("dd✓"),
            "tail of 2 should start exactly at the 4th line: {tail:?}"
        );
    }

    #[test]
    fn test_tail_lines_crlf_ascii_starts_on_line_boundary() {
        let text = "one\r\ntwo\r\nthree\r\n";
        assert_eq!(tail_lines(text, 2), "two\r\nthree\r\n");
    }

    #[test]
    fn test_tail_lines_mixed_endings() {
        // A job that mixes `\r\n` and bare `\n` (common when a tool's stdout and
        // stderr both land in the same buffer) still cuts on a line boundary.
        let text = "a\r\nb\nc\r\nd\n";
        assert_eq!(tail_lines(text, 2), "c\r\nd\n");
    }

    // ── truncate_command edge cases ────────────────────────────────

    #[test]
    fn test_truncate_command_exact_length() {
        // Command length == max → unchanged
        let cmd = "echo hello"; // 10 chars
        assert_eq!(truncate_command(cmd, 10), "echo hello");
    }

    #[test]
    fn test_truncate_command_one_over_max() {
        let cmd = "echo helloo"; // 11 chars
        let result = truncate_command(cmd, 10);
        assert!(result.ends_with('…'));
        assert!(result.len() <= 13); // 9 + 3-byte ellipsis
    }

    #[test]
    fn test_truncate_command_max_zero() {
        let cmd = "echo hello";
        let result = truncate_command(cmd, 0);
        // max=0, saturating_sub(1) = 0, so we get "…" only
        assert_eq!(result, "…");
    }

    #[test]
    fn test_truncate_command_max_one() {
        let cmd = "echo hello";
        let result = truncate_command(cmd, 1);
        // Should not panic, should truncate heavily
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_command_multiline_takes_first() {
        let cmd = "echo first\necho second\necho third";
        let result = truncate_command(cmd, 100);
        assert_eq!(result, "echo first");
    }

    #[test]
    fn test_truncate_command_empty() {
        assert_eq!(truncate_command("", 10), "");
    }

    #[test]
    fn test_truncate_command_unicode_safe() {
        // Each emoji is 4 bytes. With max=6, we can't cut mid-emoji.
        let cmd = "🎉🎊🎈🎆🎇";
        let result = truncate_command(cmd, 6);
        assert!(result.ends_with('…'));
        // Should not panic — that's the main assertion
    }

    // ── BackgroundJobTracker struct tests ───────────────────────────

    #[test]
    fn test_new_tracker_empty_list() {
        let tracker = create_tracker();
        let jobs = tracker.list();
        assert!(jobs.is_empty());
    }

    #[test]
    fn test_exists_unknown_id() {
        let tracker = create_tracker();
        assert!(!tracker.exists(0));
        assert!(!tracker.exists(42));
        assert!(!tracker.exists(u32::MAX));
    }

    #[test]
    fn test_is_finished_unknown_id() {
        let tracker = create_tracker();
        assert!(!tracker.is_finished(0));
        assert!(!tracker.is_finished(99));
    }

    #[tokio::test]
    async fn test_launch_returns_incrementing_ids_from_one() {
        let tracker = create_tracker();
        let id1 = tracker.launch("true");
        let id2 = tracker.launch("true");
        let id3 = tracker.launch("true");
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    #[tokio::test]
    async fn test_list_sorted_by_id() {
        let tracker = create_tracker();
        // Launch several jobs
        tracker.launch("echo a");
        tracker.launch("echo b");
        tracker.launch("echo c");

        let jobs = tracker.list();
        assert_eq!(jobs.len(), 3);
        assert!(jobs[0].id < jobs[1].id);
        assert!(jobs[1].id < jobs[2].id);
    }

    #[tokio::test]
    async fn test_get_output_nonexistent() {
        let tracker = create_tracker();
        let output = tracker.get_output(99).await;
        assert!(output.is_none());
    }

    #[tokio::test]
    async fn test_kill_nonexistent() {
        let tracker = create_tracker();
        let killed = tracker.kill(99).await;
        assert!(!killed);
    }

    #[tokio::test]
    async fn test_list_captures_command_string() {
        let tracker = create_tracker();
        tracker.launch("echo hello world");

        let jobs = tracker.list();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].command, "echo hello world");
    }

    #[tokio::test]
    async fn test_finished_job_has_exit_code() {
        let tracker = create_tracker();
        let id = tracker.launch("true");

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        assert!(tracker.is_finished(id));
        let jobs = tracker.list();
        assert_eq!(jobs[0].exit_code, Some(0));
    }

    // --- #736: a finished job's elapsed must stop growing ---

    #[tokio::test]
    async fn test_running_job_elapsed_still_grows() {
        let tracker = create_tracker();
        tracker.launch("sleep 5");

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let first = tracker.list().remove(0);
        assert!(!first.finished, "job should still be running");
        assert!(first.runtime.is_none(), "running job has no frozen runtime");

        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let second = tracker.list().remove(0);
        assert!(
            second.elapsed > first.elapsed,
            "a running job's elapsed must keep growing: {:?} -> {:?}",
            first.elapsed,
            second.elapsed
        );

        tracker.kill(1).await;
    }

    #[tokio::test]
    async fn test_finished_job_elapsed_is_frozen() {
        let tracker = create_tracker();
        tracker.launch("true");

        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let first = tracker.list().remove(0);
        assert!(first.finished, "job should have finished");
        assert!(
            first.runtime.is_some(),
            "a finished job should carry a frozen runtime"
        );

        // A real sleep: on the buggy code elapsed would grow by ~400ms here.
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let second = tracker.list().remove(0);
        assert_eq!(
            first.elapsed, second.elapsed,
            "a finished job's elapsed must not move between list() calls"
        );
        assert_eq!(first.runtime, second.runtime);
    }

    #[tokio::test]
    async fn test_killed_job_elapsed_is_frozen() {
        let tracker = create_tracker();
        let id = tracker.launch("sleep 30");

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(tracker.kill(id).await);

        let first = tracker.list().remove(0);
        assert!(first.finished);
        assert!(
            first.runtime.is_some(),
            "kill() must stamp a runtime too — otherwise the elapsed column keeps growing"
        );

        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let second = tracker.list().remove(0);
        assert_eq!(
            first.elapsed, second.elapsed,
            "a killed job's elapsed must not move between list() calls"
        );
    }

    fn snapshot_for_test(
        finished: bool,
        runtime: Option<std::time::Duration>,
        elapsed: std::time::Duration,
    ) -> JobSnapshot {
        JobSnapshot {
            id: 1,
            command: "echo hi".to_string(),
            finished,
            exit_code: if finished { Some(0) } else { None },
            elapsed,
            runtime,
        }
    }

    #[test]
    fn test_elapsed_column_running_shows_live_clock() {
        let s = snapshot_for_test(false, None, std::time::Duration::from_secs(42));
        assert_eq!(elapsed_column(&s), "42s");
    }

    #[test]
    fn test_elapsed_column_finished_shows_frozen_runtime() {
        let s = snapshot_for_test(
            true,
            Some(std::time::Duration::from_secs(5)),
            std::time::Duration::from_secs(5),
        );
        assert_eq!(elapsed_column(&s), "5s");
    }

    #[test]
    fn test_elapsed_column_finished_without_runtime_is_unknown() {
        // The abstention case: finished, but no duration was ever stamped.
        // It must NOT borrow the live clock and present it as a runtime.
        let s = snapshot_for_test(true, None, std::time::Duration::from_secs(3600));
        assert_eq!(elapsed_column(&s), UNKNOWN_ELAPSED);
        assert_ne!(elapsed_column(&s), "1h0m");
    }
}
