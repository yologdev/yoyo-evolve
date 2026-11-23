//! #1234: a failed stdout write on the `-p` paths is a failed run.
//!
//! The answer's writers (streamed text, `--print`, the json envelope, each
//! stream-json line) used to drop their `io::Result` (`let _ = write!`) or
//! panic (`println!`), so `yoyo -p ... > out.txt` on a full disk exited 0
//! with the answer lost, or 101. Each writer now hands its result to
//! [`note`], which keeps the first real failure; the run's exit path calls
//! [`take_failure_message`], which also flushes stdout, and turns a recorded
//! failure into one stderr line and exit 1 — the shape `-o` already uses
//! (`error writing to <path>: <os error>`).
//!
//! `BrokenPipe` is never recorded: main.rs restores default SIGPIPE so
//! `yoyo -p ... | head -1` dies quietly like `cat`, and if a parent ignored
//! SIGPIPE the EPIPE that reaches us instead must stay just as quiet.

use crate::format::{RED, RESET};
use crate::sync_util::lock_or_recover;
use std::io::{self, ErrorKind, Write};
use std::sync::Mutex;

/// The first non-EPIPE stdout write failure of this process, as its message.
static FAILURE: Mutex<Option<String>> = Mutex::new(None);

/// Whether a write error is one #1234 reports (everything except EPIPE).
pub fn is_reportable(e: &io::Error) -> bool {
    e.kind() != ErrorKind::BrokenPipe
}

/// Record a stdout write result; only the first reportable error is kept.
/// Returns whether the write succeeded.
pub fn note(result: io::Result<()>) -> bool {
    match result {
        Ok(()) => true,
        Err(e) => {
            if is_reportable(&e) {
                let mut slot = lock_or_recover(&FAILURE);
                slot.get_or_insert_with(|| e.to_string());
            }
            false
        }
    }
}

/// Whether a reportable stdout write has already failed (writers stop then,
/// rather than keep writing into a dead stdout).
pub fn has_failed() -> bool {
    lock_or_recover(&FAILURE).is_some()
}

/// Write `s` to stdout and record any failure. Skips once stdout has failed.
pub fn write_str(s: &str) {
    if has_failed() {
        return;
    }
    note(io::stdout().lock().write_all(s.as_bytes()));
}

/// The stderr line for a stdout write failure, matching `-o`'s shape.
pub fn failure_line(err: &str) -> String {
    format!("error writing to stdout: {err}")
}

/// Flush stdout (buffered text can fail only here), then return the stderr
/// line for the first recorded failure, if any. Called once on exit paths.
pub fn take_failure_message() -> Option<String> {
    if !has_failed() {
        note(io::stdout().flush());
    }
    lock_or_recover(&FAILURE).take().map(|e| failure_line(&e))
}

/// Exit-code helper for the `-p` paths: on a recorded stdout failure print
/// the line and turn a 0 into 1 (a non-zero code already says "failed" and
/// keeps its more specific meaning).
pub fn exit_code_after_stdout(code: i32) -> i32 {
    match take_failure_message() {
        Some(line) => {
            eprintln!("{RED}  {line}{RESET}");
            if code == 0 {
                1
            } else {
                code
            }
        }
        None => code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_pipe_is_not_reportable_but_a_full_disk_is() {
        assert!(!is_reportable(&io::Error::from(ErrorKind::BrokenPipe)));
        assert!(is_reportable(&io::Error::from_raw_os_error(28)));
        assert!(is_reportable(&io::Error::from(ErrorKind::Other)));
    }

    #[test]
    fn failure_line_matches_the_output_flag_shape() {
        let e = io::Error::from_raw_os_error(28).to_string();
        assert_eq!(
            failure_line(&e),
            format!("error writing to stdout: {e}"),
            "same shape as `-o`'s `error writing to <path>: <err>`"
        );
        assert!(e.contains("No space left on device"), "{e}");
    }
}
