//! Run a watch/check command without waiting for a backgrounded
//! grandchild's EOF (#1063).
//!
//! The old `run_watch_command` read stdout to EOF on the calling thread and
//! only then waited on `sh`. A command that backgrounds anything with
//! inherited stdout (`server &`, `sleep 600 &`) keeps the pipe's write end
//! open, so EOF never came until that grandchild died — and under auto-watch
//! that blocked the model's own edit tool call (`AutoCheckTool`).
//!
//! The shape here is the one `/run` already uses (`commands_run.rs`,
//! `run_shell_command`, itself mirroring `StreamingBashTool`, #1049): read
//! both pipes on their own threads, wait on `sh`, then give the readers
//! [`WATCH_READER_GRACE`] to drain and keep whatever they had. A grandchild
//! that still holds the pipe after the grace keeps running — it is the
//! command's business, exactly as with `/run` — and the caller is told so.

use std::io::BufRead;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the readers may keep draining after `sh` exits. Same value as
/// `/run`'s `RUN_READER_GRACE`, for the same reason: long enough for output
/// already in flight, short enough that a held pipe is not a hang.
pub(crate) const WATCH_READER_GRACE: Duration = Duration::from_secs(2);

/// What one watch command produced.
#[derive(Debug)]
pub(crate) struct WatchCapture {
    pub stdout_lines: Vec<String>,
    pub stderr_lines: Vec<String>,
    /// `sh` exited with status 0.
    pub success: bool,
    /// A reader was still open when the grace ran out: something the command
    /// started still holds stdout/stderr, and later output is not captured.
    pub held_open: bool,
}

#[derive(Default)]
struct Tap {
    lines: Vec<String>,
    /// Set when the caller stops waiting: nothing more is kept or reported.
    stopped: bool,
}

/// Read `pipe` line by line on a thread (the `BufRead::lines` rule: drop the
/// `\n`, then one `\r` before it), calling `on_line(count)` after each kept
/// line while the caller is still listening. Signals `done` at EOF.
fn spawn_reader<R: std::io::Read + Send + 'static>(
    pipe: R,
    done: mpsc::Sender<()>,
    on_line: Option<Box<dyn Fn(usize) + Send>>,
) -> (Arc<Mutex<Tap>>, std::thread::JoinHandle<()>) {
    let tap = Arc::new(Mutex::new(Tap::default()));
    let shared = Arc::clone(&tap);
    let handle = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(pipe);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if buf.last() == Some(&b'\n') {
                buf.pop();
                if buf.last() == Some(&b'\r') {
                    buf.pop();
                }
            }
            let line = String::from_utf8_lossy(&buf).into_owned();
            let mut t = crate::sync_util::lock_or_recover(&shared);
            if t.stopped {
                return;
            }
            t.lines.push(line);
            if let Some(f) = &on_line {
                f(t.lines.len());
            }
        }
        let _ = done.send(());
    });
    (tap, handle)
}

/// Run `cmd` under `sh -c`, calling `on_stdout_line(count)` as stdout lines
/// arrive (for progress display). Returns `Err` only if `sh` cannot spawn.
pub(crate) fn run_capture(
    cmd: &str,
    on_stdout_line: Box<dyn Fn(usize) + Send>,
) -> Result<WatchCapture, String> {
    run_capture_with_grace(cmd, on_stdout_line, WATCH_READER_GRACE)
}

fn run_capture_with_grace(
    cmd: &str,
    on_stdout_line: Box<dyn Fn(usize) + Send>,
    grace: Duration,
) -> Result<WatchCapture, String> {
    let mut child = Command::new("sh")
        .args(["-c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    let (done_tx, done_rx) = mpsc::channel::<()>();
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let readers = [
        spawn_reader(stdout, done_tx.clone(), Some(on_stdout_line)),
        spawn_reader(stderr, done_tx.clone(), None),
    ];
    drop(done_tx);

    let success = child.wait().map(|s| s.success()).unwrap_or(false);

    let deadline = Instant::now() + grace;
    let mut finished = 0;
    while finished < readers.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        match done_rx.recv_timeout(left) {
            Ok(()) => finished += 1,
            Err(_) => break,
        }
    }
    let held_open = finished < readers.len();

    let mut taken = Vec::with_capacity(2);
    for (tap, handle) in readers {
        let lines = {
            let mut t = crate::sync_util::lock_or_recover(&tap);
            t.stopped = true;
            std::mem::take(&mut t.lines)
        };
        if !held_open {
            let _ = handle.join();
        }
        taken.push(lines);
    }
    let stderr_lines = taken.pop().unwrap_or_default();
    let stdout_lines = taken.pop().unwrap_or_default();
    Ok(WatchCapture {
        stdout_lines,
        stderr_lines,
        success,
        held_open,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(cmd: &str) -> (WatchCapture, Duration) {
        let start = Instant::now();
        let cap = run_capture(cmd, Box::new(|_| {})).expect("sh spawns");
        (cap, start.elapsed())
    }

    #[test]
    fn backgrounded_grandchild_holding_stdout_does_not_block() {
        let (cap, took) = run("sleep 5 & echo hi");
        assert!(
            took < Duration::from_millis(3500),
            "waited for the grandchild's EOF: {took:?}"
        );
        assert_eq!(cap.stdout_lines, vec!["hi".to_string()]);
        assert!(cap.success);
        assert!(cap.held_open, "the sleep still holds the pipe");
    }

    #[test]
    fn plain_echo_is_unchanged() {
        let (cap, _) = run("echo hi");
        assert_eq!(cap.stdout_lines, vec!["hi".to_string()]);
        assert!(cap.stderr_lines.is_empty());
        assert!(cap.success);
        assert!(!cap.held_open);
    }

    #[test]
    fn both_streams_and_failure_status_are_kept() {
        let (cap, _) = run("echo out; echo err >&2; exit 3");
        assert_eq!(cap.stdout_lines, vec!["out".to_string()]);
        assert_eq!(cap.stderr_lines, vec!["err".to_string()]);
        assert!(!cap.success);
        assert!(!cap.held_open);
    }

    #[test]
    fn background_with_redirected_output_is_fast_and_not_held() {
        let (cap, took) = run("sleep 5 >/dev/null 2>&1 & echo hi");
        assert!(took < Duration::from_millis(1500), "{took:?}");
        assert_eq!(cap.stdout_lines, vec!["hi".to_string()]);
        assert!(cap.success);
        assert!(!cap.held_open);
    }

    #[test]
    fn crlf_and_trailing_partial_line_follow_the_lines_rule() {
        let (cap, _) = run("printf 'a\\r\\nb'");
        assert_eq!(cap.stdout_lines, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn progress_callback_sees_each_stdout_line_count() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = Arc::clone(&seen);
        let cap = run_capture(
            "echo a; echo b; echo c >&2",
            Box::new(move |n| s.lock().unwrap().push(n)),
        )
        .unwrap();
        assert_eq!(cap.stdout_lines.len(), 2);
        assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn the_door_run_watch_command_returns_promptly_too() {
        let start = Instant::now();
        let (ok, output) = crate::watch::run_watch_command("sleep 5 & echo hi");
        let took = start.elapsed();
        assert!(took < Duration::from_millis(3500), "{took:?}");
        assert!(ok);
        assert!(output.starts_with("hi"), "{output:?}");
    }

    #[test]
    fn zero_grace_still_returns_the_exit_status() {
        let cap = run_capture_with_grace("exit 0", Box::new(|_| {}), Duration::ZERO).unwrap();
        assert!(cap.success);
    }
}
