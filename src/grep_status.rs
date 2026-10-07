//! Exit-status classification for `/grep` and `yoyo grep` (#982).
//!
//! Split out of `commands_search.rs` (at its module-size gate). Before this,
//! nothing read grep's exit status, so a missing or unreadable path printed
//! "No matches found." and `yoyo grep` exited 0.

/// What one grep run produced (#982): whatever it found, plus the error grep
/// reported, if any. Both halves are kept because `grep -r` over a tree with
/// one unreadable directory exits 2 *and* prints the matches it could reach —
/// dropping either half would be a lie (no matches, or no problem).
#[derive(Debug, Clone, PartialEq)]
pub struct GrepRun<T> {
    pub found: T,
    pub error: Option<String>,
}

/// Most stderr lines a grep error shows; the rest are counted, not printed.
pub(crate) const GREP_ERROR_MAX_LINES: usize = 5;
/// Byte cap per shown stderr line, cut on a char boundary (#250).
pub(crate) const GREP_ERROR_LINE_MAX_BYTES: usize = 200;

/// Classify a finished grep/git-grep by its exit status (#982).
///
/// grep and `git grep` both use 0 = matches, 1 = no match; anything else
/// (grep's 2, git's 128, death by signal → `None`) is an error. Before this,
/// nothing read the status, so `yoyo grep x /nonexistent` printed
/// "No matches found." and exited 0. Returns `None` for 0/1, otherwise the
/// error naming the searched path and grep's own stderr — sanitized (it may
/// echo repo-authored file names) and capped per line on a char boundary.
pub(crate) fn grep_failure_message(code: Option<i32>, stderr: &str, path: &str) -> Option<String> {
    if matches!(code, Some(0) | Some(1)) {
        return None;
    }
    let status = match code {
        Some(c) => format!("exit status {c}"),
        None => "killed by a signal".to_string(),
    };
    let path = crate::cli::sanitize_for_display(path);
    let mut msg = format!("grep could not search `{path}` ({status})");
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    for line in lines.iter().take(GREP_ERROR_MAX_LINES) {
        let mut shown = crate::cli::sanitize_for_display(line);
        if shown.len() > GREP_ERROR_LINE_MAX_BYTES {
            let mut b = GREP_ERROR_LINE_MAX_BYTES;
            while b > 0 && !shown.is_char_boundary(b) {
                b -= 1;
            }
            shown.truncate(b);
            shown.push('…');
        }
        msg.push_str("\n    ");
        msg.push_str(&shown);
    }
    if lines.len() > GREP_ERROR_MAX_LINES {
        msg.push_str(&format!(
            "\n    (… {} more)",
            lines.len() - GREP_ERROR_MAX_LINES
        ));
    }
    Some(msg)
}

/// Turn a grep `Output` into its stdout text plus the classified error.
pub(crate) fn grep_output_parts(
    out: &std::process::Output,
    path: &str,
) -> (String, Option<String>) {
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr);
    (
        stdout,
        grep_failure_message(out.status.code(), &stderr, path),
    )
}
