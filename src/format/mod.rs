//! Formatting helpers: ANSI colors, cost, duration, tokens, context bar, truncation.

use std::collections::HashSet;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

// --- Color support with NO_COLOR and --no-color ---

/// Whether color output has been disabled (via NO_COLOR env or --no-color flag).
static COLOR_DISABLED: OnceLock<bool> = OnceLock::new();

// --- Quiet mode support with --quiet / -q ---

/// Whether informational stderr output has been suppressed (via --quiet/-q flag or
/// YOYO_QUIET env). Suppresses `config:` and `context:` progress lines for scripted usage.
static QUIET: OnceLock<bool> = OnceLock::new();

/// Enable quiet mode. Call from CLI arg parsing when -q/--quiet is encountered.
pub fn enable_quiet() {
    let _ = QUIET.set(true);
}

/// Check if quiet mode is active. Respects YOYO_QUIET env var.
pub fn is_quiet() -> bool {
    *QUIET.get_or_init(|| std::env::var("YOYO_QUIET").is_ok())
}

// --- Bell notification support with YOYO_NO_BELL and --no-bell ---

/// Whether bell notification has been disabled (via --no-bell flag or YOYO_NO_BELL env).
static BELL_DISABLED: OnceLock<bool> = OnceLock::new();

/// Disable bell notifications. Call from CLI arg parsing.
pub fn disable_bell() {
    let _ = BELL_DISABLED.set(true);
}

/// Check if bell is enabled. Respects YOYO_NO_BELL env var.
pub fn bell_enabled() -> bool {
    !*BELL_DISABLED.get_or_init(|| std::env::var("YOYO_NO_BELL").is_ok())
}

/// Elapsed-seconds threshold that makes a prompt count as "long" for the
/// terminal bell and the user-configured `notify_command`.
const LONG_PROMPT_THRESHOLD_SECS: u64 = 3;

/// Ring the terminal bell if enabled and elapsed time exceeds threshold.
/// The bell character (\x07) causes most terminal emulators to flash the tab
/// or play a sound, alerting multitasking developers.
/// Also sends a desktop notification for genuinely long waits (≥10s), and
/// runs the user-configured `notify_command` (if any) at the bell threshold.
pub fn maybe_ring_bell(elapsed: Duration) {
    if bell_enabled() && elapsed.as_secs() >= LONG_PROMPT_THRESHOLD_SECS {
        let _ = io::stdout().write_all(b"\x07");
        let _ = io::stdout().flush();
    }
    if notify_enabled() && should_send_notification(elapsed) {
        send_desktop_notification(elapsed);
    }
    if let Some(cmd) = notify_command() {
        if should_run_notify_command(elapsed, true) {
            run_notify_command(&cmd, elapsed);
        }
    }
}

// --- User notify_command support (opt-in via `notify_command` config key) ---

/// User-configured command to run when a long prompt finishes.
/// `None` (the default) means the feature is completely inert — no process
/// spawn, no PATH probing (product-safe default, issue #448 discipline).
static NOTIFY_COMMAND: OnceLock<Option<String>> = OnceLock::new();

/// Set the user-configured notify command (from `.yoyo.toml`'s
/// `notify_command` key). Call once during startup config loading;
/// subsequent calls are ignored.
pub fn set_notify_command(cmd: Option<String>) {
    let _ = NOTIFY_COMMAND.set(cmd);
}

/// The configured notify command, if any. Unset or empty = `None`.
fn notify_command() -> Option<String> {
    NOTIFY_COMMAND
        .get()
        .and_then(|c| c.as_deref())
        .filter(|c| !c.trim().is_empty())
        .map(|c| c.to_string())
}

/// Pure decision: should the user's notify command run? Only when one is
/// configured AND the prompt ran long enough (same threshold as the bell).
/// Separated from the side-effectful spawn for testability.
pub fn should_run_notify_command(elapsed: Duration, configured: bool) -> bool {
    configured && elapsed.as_secs() >= LONG_PROMPT_THRESHOLD_SECS
}

/// Spawn the user's notify command, fire-and-forget.
///
/// Runs via `sh -c` (Unix) or `cmd /C` (Windows), detached: stdin/stdout/
/// stderr are all null and we never wait on the child. Sets `YOYO_EVENT`
/// and `YOYO_ELAPSED_SECS` env vars so one command can serve future events.
/// Spawn failure is silent by default (one-line warning in verbose mode) —
/// never panics, never blocks the REPL.
pub fn run_notify_command(cmd: &str, elapsed: Duration) {
    #[cfg(windows)]
    let mut command = {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(cmd);
        c
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut c = std::process::Command::new("sh");
        c.arg("-c").arg(cmd);
        c
    };
    let result = command
        .env("YOYO_EVENT", "prompt_completed")
        .env("YOYO_ELAPSED_SECS", elapsed.as_secs().to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Err(e) = result {
        if crate::cli::is_verbose() {
            eprintln!("warning: notify_command failed to spawn: {e}");
        }
    }
}

// --- Desktop notification support with YOYO_NO_NOTIFY and --no-notify ---

/// Notification duration threshold in seconds.
const NOTIFICATION_THRESHOLD_SECS: u64 = 10;

/// Returns true if the elapsed duration meets the threshold for sending a
/// desktop notification (≥10s). This is the pure decision logic, separated
/// from the side-effectful `send_desktop_notification` for testability.
pub fn should_send_notification(elapsed: Duration) -> bool {
    elapsed.as_secs() >= NOTIFICATION_THRESHOLD_SECS
}

/// Whether desktop notifications have been disabled (via --no-notify flag or YOYO_NO_NOTIFY env).
static NOTIFY_DISABLED: OnceLock<bool> = OnceLock::new();

/// Disable desktop notifications. Call from CLI arg parsing.
pub fn disable_notify() {
    let _ = NOTIFY_DISABLED.set(true);
}

/// Check if desktop notifications are enabled. Respects YOYO_NO_NOTIFY env var.
pub fn notify_enabled() -> bool {
    !*NOTIFY_DISABLED.get_or_init(|| std::env::var("YOYO_NO_NOTIFY").is_ok())
}

/// Build a human-friendly notification message for a completed prompt.
pub fn build_notification_message(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs >= 60 {
        let mins = secs / 60;
        let rem = secs % 60;
        if rem == 0 {
            format!("yoyo finished after {}m", mins)
        } else {
            format!("yoyo finished after {}m {}s", mins, rem)
        }
    } else {
        format!("yoyo finished after {}s", secs)
    }
}

/// Send a desktop notification (best-effort, fire-and-forget).
///
/// Uses platform-native commands:
/// - macOS: `osascript -e 'display notification ...'`
/// - Linux: `notify-send`
/// - Windows: PowerShell toast notification
///
/// Silently ignores failures (command not found, etc.).
pub fn send_desktop_notification(elapsed: Duration) {
    let message = build_notification_message(elapsed);

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification \"{}\" with title \"yoyo\"",
            message.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("notify-send")
            .arg("yoyo")
            .arg(&message)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    #[cfg(target_os = "windows")]
    {
        let ps_script = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
             $template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
             $textNodes = $template.GetElementsByTagName('text'); \
             $textNodes.Item(0).AppendChild($template.CreateTextNode('yoyo')) > $null; \
             $textNodes.Item(1).AppendChild($template.CreateTextNode('{}')) > $null; \
             $toast = [Windows.UI.Notifications.ToastNotification]::new($template); \
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('yoyo').Show($toast)",
            message.replace('\'', "''")
        );
        let _ = std::process::Command::new("powershell")
            .arg("-Command")
            .arg(&ps_script)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// Disable color output. Call before any formatting happens (e.g., from CLI arg parsing).
pub fn disable_color() {
    let _ = COLOR_DISABLED.set(true);
}

/// The decision half of [`color_enabled`], with the environment read lifted out
/// (the same pure-core-plus-thin-wrapper split used by `apply_effort_hint_with`,
/// `usage_print_line` and `context_budget_warning_with`).
///
/// `no_color_set` is whether the `NO_COLOR` environment variable is present at
/// all — per <https://no-color.org/> its *value* is irrelevant, presence is the
/// signal.
pub(crate) fn color_enabled_from_env(no_color_set: bool) -> bool {
    !no_color_set
}

/// Check if color output is enabled. Cached after first call.
/// Respects the NO_COLOR environment variable (https://no-color.org/).
///
/// This is the only reader of the process-global `COLOR_DISABLED` state; the
/// decision itself lives in [`color_enabled_from_env`].
///
/// Under `cfg(test)` this is pinned to `true`. `COLOR_DISABLED` is a write-once
/// `OnceLock` seeded from the ambient `NO_COLOR`, with no setter that can force
/// it either way, so without the pin ~20 tests across 8 modules assert whichever
/// branch the *environment* happened to choose: `cargo test` is green and
/// `NO_COLOR=1 cargo test` fails 20 tests that never changed. In this loop a
/// `cargo test` failure means `git reset --hard`, so an inherited `NO_COLOR`
/// would revert a whole session's valid work and read as "the task broke the
/// build".
///
/// `true` is the byte-identical value: without `NO_COLOR` set — the state in
/// which the suite passes today — this already returns `true`, so the pin
/// changes nothing about `cargo test` and only makes the `NO_COLOR=1` run match
/// it. Production is untouched (`cfg(test)` code never ships).
///
/// Do not "helpfully" delete the pin to restore coverage of the disabled
/// branch: no test could observe that branch before either (write-once,
/// env-seeded, no setter) — it was ambient luck, not coverage. The decision is
/// covered instead by `color_enabled_from_env`'s table test, in both
/// directions. Note this is a *different* switch from `is_plain_output()`
/// (`--screen-reader`), which is an `AtomicBool` with a real setter and is
/// deliberately left alone.
fn color_enabled() -> bool {
    #[cfg(test)]
    {
        true
    }
    #[cfg(not(test))]
    {
        color_enabled_from_env(*COLOR_DISABLED.get_or_init(|| std::env::var("NO_COLOR").is_ok()))
    }
}

// --- Stderr TTY detection (cached) ---

/// Whether stderr is connected to a terminal. Cached via `OnceLock` to avoid
/// repeated syscalls. Used to suppress spinner/progress ANSI escape sequences
/// when stderr is not a TTY (e.g., piped output, CI logs).
static STDERR_IS_TTY: OnceLock<bool> = OnceLock::new();

/// Check if stderr is a terminal. Result is cached after first call.
pub fn stderr_is_terminal() -> bool {
    *STDERR_IS_TTY.get_or_init(|| std::io::IsTerminal::is_terminal(&std::io::stderr()))
}

/// A color code that respects the NO_COLOR convention.
/// When color is disabled, formats as an empty string.
pub struct Color(pub &'static str);

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if color_enabled() {
            f.write_str(self.0)
        } else {
            Ok(())
        }
    }
}

// ANSI color helpers — respect NO_COLOR env var and --no-color flag
pub static RESET: Color = Color("\x1b[0m");
pub static BOLD: Color = Color("\x1b[1m");
pub static DIM: Color = Color("\x1b[2m");
pub static GREEN: Color = Color("\x1b[32m");
pub static YELLOW: Color = Color("\x1b[33m");
pub static CYAN: Color = Color("\x1b[36m");
pub static RED: Color = Color("\x1b[31m");
pub static MAGENTA: Color = Color("\x1b[35m");
pub static ITALIC: Color = Color("\x1b[3m");
pub static BOLD_ITALIC: Color = Color("\x1b[1;3m");
pub static BOLD_CYAN: Color = Color("\x1b[1;36m");
pub static BOLD_YELLOW: Color = Color("\x1b[1;33m");

// --- Syntax highlighting for code blocks ---

mod cost;
mod diff;
/// Languages recognized for syntax highlighting.
mod highlight;
mod highlight_lang;
mod markdown;
mod output;
mod tools;

pub use cost::*;
pub use diff::*;
pub use highlight::*;
pub use markdown::*;
pub use output::*;
pub use tools::*;

/// Find the nearest valid UTF-8 char boundary at or before `target`.
/// Returns 0 if no boundary exists before `target`.
/// Use this when you need a safe byte offset for slicing (not truncation).
pub fn safe_byte_index(s: &str, target: usize) -> usize {
    let target = target.min(s.len());
    let mut b = target;
    while b > 0 && !s.is_char_boundary(b) {
        b -= 1;
    }
    b
}

/// Truncate a string at a safe UTF-8 char boundary, never exceeding `max_bytes`.
/// Returns a `&str` slice. Avoids panics from slicing mid-character.
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut b = max_bytes;
    while b > 0 && !s.is_char_boundary(b) {
        b -= 1;
    }
    &s[..b]
}

/// Truncate a string at a safe UTF-8 byte boundary and append a suffix (e.g. `"…"`).
/// Returns the original string unchanged if it fits within `max_bytes`.
pub fn safe_truncate_with_suffix(s: &str, max_bytes: usize, suffix: &str) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let truncated = safe_truncate(s, max_bytes);
    format!("{truncated}{suffix}")
}

pub fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

/// Truncate text at a word boundary, appending "…" if truncated.
///
/// Finds a safe UTF-8 char boundary at `max_bytes`, then backs up to
/// the last space (if one exists past position 10) to avoid splitting words.
pub fn truncate_at_word_boundary(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let truncated = safe_truncate(text, max_bytes);
    let mut end = truncated.len();
    if let Some(space_pos) = truncated.rfind(' ') {
        if space_pos > 10 {
            end = space_pos;
        }
    }
    // `end` is either `truncated.len()` (a valid char boundary from safe_truncate)
    // or a space position (ASCII, always a valid char boundary).
    format!("{}…", &text[..end])
}

/// Append the last `max_lines` of `output` to `summary`, preceded by a header.
///
/// Useful for including a tail preview of command output (e.g. error logs)
/// in a summary string for AI context.
pub fn append_tail_preview(summary: &mut String, output: &str, max_lines: usize) {
    let lines: Vec<&str> = output.lines().collect();
    let preview = if lines.len() > max_lines {
        &lines[lines.len() - max_lines..]
    } else {
        &lines[..]
    };
    summary.push_str("\n\nLast output:\n");
    for line in preview {
        summary.push_str(line);
        summary.push('\n');
    }
}

/// Decode HTML entities in a string.
///
/// Handles named entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, `&#39;`,
/// `&nbsp;`, `&#x27;`, `&mdash;`, `&ndash;`, `&hellip;`, `&copy;`, `&reg;`)
/// and numeric entities (decimal `&#NNN;` and hex `&#xHH;`).
pub fn decode_html_entities(s: &str) -> String {
    // Fast path: if there's no '&', there are no entities to decode
    if !s.contains('&') {
        return s.to_string();
    }

    // First pass: named entities
    let s = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&copy;", "©")
        .replace("&reg;", "®");

    // Second pass: remaining numeric entities (&#NNN; and &#xHH;)
    let mut decoded = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' && chars.peek() == Some(&'#') {
            let mut entity = String::from("&#");
            chars.next(); // consume '#'
            let mut saw_semicolon = false;
            while let Some(&nc) = chars.peek() {
                if nc == ';' {
                    chars.next();
                    saw_semicolon = true;
                    break;
                }
                entity.push(nc);
                chars.next();
            }
            let num_str = &entity[2..];
            let parsed = if let Some(hex) = num_str.strip_prefix('x').or(num_str.strip_prefix('X'))
            {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num_str.parse::<u32>().ok()
            };
            if let Some(ch) = parsed.and_then(char::from_u32) {
                decoded.push(ch);
            } else {
                // Failed to decode — re-emit the original text faithfully.
                // Only append ';' if the terminator was actually present in the
                // input; otherwise we'd invent a phantom character (Day-146 bug).
                decoded.push_str(&entity);
                if saw_semicolon {
                    decoded.push(';');
                }
            }
        } else {
            decoded.push(c);
        }
    }

    decoded
}
// --- Section headers and dividers for visual hierarchy ---

/// Get the terminal width from the COLUMNS environment variable, falling back to 80.
fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(80)
}
/// Render a turn boundary marker between agent turns.
///
/// Shows a subtle visual separator so users can distinguish
/// when the agent starts a new reasoning/action cycle.
/// Example: `  ╭─ Turn 3 ──────────────────────────╮`
pub fn turn_boundary(turn_number: usize) -> String {
    let width = terminal_width();
    let label = format!(" Turn {turn_number} ");
    let prefix = "  ╭─";
    let suffix = "╮";
    let used = prefix.len() + label.len() + suffix.len();
    let fill = width.saturating_sub(used);
    let trail = "─".repeat(fill);
    format!("{DIM}{prefix}{label}{trail}{suffix}{RESET}")
}

/// Render a labeled section header, e.g. `── Thinking ──────────────────────────`
/// Uses DIM style and thin box-drawing characters (─).
/// The label is centered between two runs of ─ characters.
pub fn section_header(label: &str) -> String {
    let width = terminal_width();
    if label.is_empty() {
        return section_divider();
    }
    // Format: "── Label ─────────..."
    let prefix = "── ";
    let separator = " ";
    let used = prefix.len() + label.len() + separator.len();
    let remaining = width.saturating_sub(used);
    let trail = "─".repeat(remaining);
    format!("{DIM}{prefix}{label}{separator}{trail}{RESET}")
}

/// Render a plain thin divider line: `──────────────────────────────────────`
/// Uses DIM style and thin box-drawing characters (─).
pub fn section_divider() -> String {
    let width = terminal_width();
    format!("{DIM}{}{RESET}", "─".repeat(width))
}

/// Format a human-readable summary for a tool execution.
///
/// Each tool gets a concise one-line description showing the key parameters:
/// - `bash` — `$ <command>` (first line + line count for multi-line scripts)
/// - `read_file` — `read <path>` with optional `:offset..end` or `(N lines)` range
/// - `write_file` — `write <path> (N lines)`
/// - `edit_file` — `edit <path> (old → new lines)`
/// - `list_files` — `ls <path> (pattern)`
/// - `search` — `search 'pattern' in <path> (include)`
pub fn format_tool_summary(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "bash" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            let line_count = cmd.lines().count();
            let first_line = cmd.lines().next().unwrap_or("...");
            if line_count > 1 {
                format!(
                    "$ {} ({line_count} lines)",
                    truncate_with_ellipsis(first_line, 60)
                )
            } else {
                format!("$ {}", truncate_with_ellipsis(cmd, 80))
            }
        }
        "read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let offset = args.get("offset").and_then(|v| v.as_u64());
            let limit = args.get("limit").and_then(|v| v.as_u64());
            match (offset, limit) {
                (Some(off), Some(lim)) => {
                    format!("read {path}:{off}..{}", off + lim)
                }
                (Some(off), None) => {
                    format!("read {path}:{off}..")
                }
                (None, Some(lim)) => {
                    let word = pluralize(lim as usize, "line", "lines");
                    format!("read {path} ({lim} {word})")
                }
                (None, None) => {
                    format!("read {path}")
                }
            }
        }
        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let line_info = args
                .get("content")
                .and_then(|v| v.as_str())
                .map(|c| {
                    let count = c.lines().count();
                    let word = pluralize(count, "line", "lines");
                    format!(" ({count} {word})")
                })
                .unwrap_or_default();
            format!("write {path}{line_info}")
        }
        "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let old_text = args.get("old_text").and_then(|v| v.as_str());
            let new_text = args.get("new_text").and_then(|v| v.as_str());
            match (old_text, new_text) {
                (Some(old), Some(new)) => {
                    let old_lines = old.lines().count();
                    let new_lines = new.lines().count();
                    format!("edit {path} ({old_lines} → {new_lines} lines)")
                }
                _ => format!("edit {path}"),
            }
        }
        "list_files" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let pattern = args.get("pattern").and_then(|v| v.as_str());
            match pattern {
                Some(pat) => format!("ls {path} ({pat})"),
                None => format!("ls {path}"),
            }
        }
        "search" => {
            let pat = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            let search_path = args.get("path").and_then(|v| v.as_str());
            let include = args.get("include").and_then(|v| v.as_str());
            let mut summary = format!("search '{}'", truncate_with_ellipsis(pat, 60));
            if let Some(p) = search_path {
                summary.push_str(&format!(" in {p}"));
            }
            if let Some(inc) = include {
                summary.push_str(&format!(" ({inc})"));
            }
            summary
        }
        "rename_symbol" => {
            let old = args.get("old_name").and_then(|v| v.as_str()).unwrap_or("?");
            let new = args.get("new_name").and_then(|v| v.as_str()).unwrap_or("?");
            let scope = args.get("path").and_then(|v| v.as_str());
            match scope {
                Some(p) => format!("rename {old} → {new} in {p}"),
                None => format!("rename {old} → {new}"),
            }
        }
        "todo" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("?");
            match action {
                "add" => {
                    let desc = args
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("...");
                    format!("todo add '{}'", truncate_with_ellipsis(desc, 50))
                }
                "done" | "wip" | "remove" => {
                    let id = args.get("id").and_then(|v| v.as_u64());
                    match id {
                        Some(n) => format!("todo {action} #{n}"),
                        None => format!("todo {action}"),
                    }
                }
                _ => format!("todo {action}"),
            }
        }
        "web_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("?");
            format!("web search '{}'", truncate_with_ellipsis(query, 60))
        }
        "sub_agent" => {
            let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("...");
            format!("sub-agent '{}'", truncate_with_ellipsis(task, 60))
        }
        _ => tool_name.to_string(),
    }
}

/// Format usage stats into a string (verbose or compact).
///
/// Verbose format (shown with `--verbose`):
///   `tokens: 1119 in / 47 out  [cache: ...]  (session: ...)  cost: ...  total: ...  ⏱ 1.0s`
///
/// Compact format (default):
///   `↳ 1.0s · 1119→47 tokens · $0.020`
pub fn format_usage_line(
    usage: &yoagent::Usage,
    total: &yoagent::Usage,
    model: &str,
    elapsed: std::time::Duration,
    verbose: bool,
) -> Option<String> {
    if usage.input == 0 && usage.output == 0 {
        return None;
    }

    let elapsed_str = format_duration(elapsed);

    // Calculate output tokens/sec (only meaningful when elapsed > 0.1s)
    let tok_per_sec = if elapsed.as_secs_f64() > 0.1 {
        Some((usage.output as f64 / elapsed.as_secs_f64()) as u32)
    } else {
        None
    };

    if verbose {
        let cache_info = if usage.cache_read > 0 || usage.cache_write > 0 {
            format!(
                "  [cache: {} read, {} write]",
                usage.cache_read, usage.cache_write
            )
        } else {
            String::new()
        };
        let cost_info = estimate_cost(usage, model)
            .map(|c| format!("  cost: {}", format_cost(c)))
            .unwrap_or_default();
        let total_cost_info = estimate_cost(total, model)
            .map(|c| format!("  total: {}", format_cost(c)))
            .unwrap_or_default();
        let speed_info = tok_per_sec
            .map(|s| format!("  speed: {} tok/s", s))
            .unwrap_or_default();
        Some(format!(
            "tokens: {} in / {} out{cache_info}  (session: {} in / {} out){cost_info}{total_cost_info}{speed_info}  ⏱ {elapsed_str}",
            usage.input, usage.output, total.input, total.output
        ))
    } else {
        let speed_suffix = tok_per_sec
            .map(|s| format!(" ({} tok/s)", s))
            .unwrap_or_default();
        let cost_suffix = estimate_cost(usage, model)
            .map(|c| format!(" · {}", format_cost(c)))
            .unwrap_or_default();
        Some(format!(
            "↳ {elapsed_str} · {}→{} tokens{speed_suffix}{cost_suffix}",
            usage.input, usage.output
        ))
    }
}

/// Print usage stats after a prompt response.
/// Decide what [`print_usage`] should emit. **Pure** — both process globals the
/// printing wrapper consults (`is_quiet()` and `cli::is_verbose()`) arrive as
/// arguments, so a test can drive the quiet branch without writing either one.
/// `None` means "print nothing".
pub(crate) fn usage_print_line(
    quiet: bool,
    verbose: bool,
    usage: &yoagent::Usage,
    total: &yoagent::Usage,
    model: &str,
    elapsed: std::time::Duration,
) -> Option<String> {
    if quiet {
        return None;
    }
    format_usage_line(usage, total, model, elapsed, verbose)
}

pub fn print_usage(
    usage: &yoagent::Usage,
    total: &yoagent::Usage,
    model: &str,
    elapsed: std::time::Duration,
) {
    let quiet = is_quiet();
    // `&&` short-circuits, so a quiet run never reads cli::VERBOSE — the same
    // read order the pre-seam body had, where the quiet check returned early.
    let verbose = !quiet && crate::cli::is_verbose();
    if let Some(line) = usage_print_line(quiet, verbose, usage, total, model, elapsed) {
        println!("\n{DIM}  {line}{RESET}");
    }
}

/// Return the color code for a context usage percentage.
/// Green if ≤50%, yellow if 51-80%, red if >80%.
pub fn context_usage_color(pct: u32) -> &'static Color {
    if pct > 80 {
        &RED
    } else if pct > 50 {
        &YELLOW
    } else {
        &GREEN
    }
}

/// Format the context usage label string.
/// Returns "0%" for true zero, "<1%" for non-zero usage that rounds to 0%,
/// otherwise the integer percentage like "42%".
pub fn context_usage_label(used_tokens: u64, max_tokens: u64) -> String {
    if max_tokens == 0 {
        return "0%".to_string();
    }
    let pct = ((used_tokens as f64 / max_tokens as f64) * 100.0).min(100.0) as u32;
    if used_tokens > 0 && pct == 0 {
        "<1%".to_string()
    } else {
        format!("{pct}%")
    }
}

/// Decide the context-usage line [`print_context_usage`] should emit. **Pure** —
/// the one process global the wrapper consults (`is_quiet()`) arrives as an
/// argument. `None` means "print nothing", which has **two** distinct causes
/// kept in one place: quiet mode, and an unknown context window (`max == 0`).
pub(crate) fn context_usage_line(quiet: bool, used_tokens: u64, max_tokens: u64) -> Option<String> {
    if quiet {
        return None;
    }
    if max_tokens == 0 {
        return None;
    }
    let pct = ((used_tokens as f64 / max_tokens as f64) * 100.0).min(100.0) as u32;
    let color = context_usage_color(pct);
    let label = context_usage_label(used_tokens, max_tokens);
    Some(format!(
        "{DIM}  {color}⬤{RESET}{DIM} {label} of context window used{RESET}"
    ))
}

/// Print a context window usage indicator line.
/// Shows percentage of context consumed, color-coded by fullness.
pub fn print_context_usage(used_tokens: u64, max_tokens: u64) {
    if let Some(line) = context_usage_line(is_quiet(), used_tokens, max_tokens) {
        println!("{line}");
    }
}

/// Tracks the last warned context budget threshold (0, 60, 80, 90, 95).
/// Used to avoid repeating the same warning every turn.
static LAST_WARNED_THRESHOLD: AtomicU32 = AtomicU32::new(0);

/// Return an escalating context budget warning if the usage crosses a new threshold.
///
/// Thresholds:
/// - Below 60%: `None`
/// - 60%: dim info suggesting `/compact`
/// - 80%: yellow warning suggesting `/compact` or `/save` + `/clear`
/// - 90%: red warning urging `/save` then `/clear`
/// - 95%+: bold red warning to `/clear` immediately
///
/// Only warns once per threshold crossing. Call `reset_context_budget_warning()`
/// after a `/clear` to re-arm.
pub fn context_budget_warning(used: u64, max: u64) -> Option<String> {
    context_budget_warning_with(&LAST_WARNED_THRESHOLD, used, max)
}

/// Pure classifier: which budget threshold (60/80/90/95) does this usage cross?
///
/// `max == 0` is an explicit "unknown budget" third value → `None`, not 0%.
/// Below 60% → `None`. No globals, no formatting, no escalation memory.
pub fn budget_warning_threshold(used: u64, max: u64) -> Option<u32> {
    if max == 0 {
        return None;
    }
    let pct = ((used as f64 / max as f64) * 100.0).min(100.0) as u32;
    if pct >= 95 {
        Some(95)
    } else if pct >= 90 {
        Some(90)
    } else if pct >= 80 {
        Some(80)
    } else if pct >= 60 {
        Some(60)
    } else {
        None
    }
}

/// Pure renderer: the message for a given threshold. Unknown threshold → `None`.
pub fn budget_warning_message(threshold: u32) -> Option<String> {
    let msg = match threshold {
        95 => format!(
            "{BOLD}{RED}  🔴 Context nearly full! /clear now or risk overflow errors{RESET}"
        ),
        90 => format!(
            "{RED}  🔴 Context is 90% full — /save your session, then /clear to avoid overflow{RESET}"
        ),
        80 => format!(
            "{YELLOW}  ⚠ Context is 80% full — /compact or /save + /clear recommended{RESET}"
        ),
        60 => format!("{DIM}  Context is 60% full — consider /compact to free space{RESET}"),
        _ => return None,
    };
    Some(msg)
}

/// Escalation logic over *injected* state: warn only when crossing a threshold
/// strictly higher than the last one recorded in `state`.
///
/// Taking the state as a parameter is what makes this testable without touching
/// the process-wide global — local state can't be raced by another test.
pub fn context_budget_warning_with(state: &AtomicU32, used: u64, max: u64) -> Option<String> {
    let threshold = budget_warning_threshold(used, max)?;
    let prev = state.load(Ordering::Relaxed);
    if threshold <= prev {
        return None;
    }
    state.store(threshold, Ordering::Relaxed);
    budget_warning_message(threshold)
}

/// Reset the context budget warning tracker so warnings re-arm after `/clear`.
pub fn reset_context_budget_warning() {
    LAST_WARNED_THRESHOLD.store(0, Ordering::Relaxed);
}

// ── Contextual command hints (discoverability) ─────────────────────────

/// Tracks which hint categories have already been shown this session.
/// Once a category fires, it won't repeat.
static SHOWN_HINTS: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();

fn shown_hints() -> &'static Mutex<HashSet<&'static str>> {
    SHOWN_HINTS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Reset shown hints (for testing or session clear).
#[cfg(test)]
pub fn reset_shown_hints() {
    if let Ok(mut guard) = shown_hints().lock() {
        guard.clear();
    }
}

/// Signals used by the contextual hint system to decide what to suggest.
pub struct HintContext {
    /// Which turn number this is (1-indexed).
    pub turn_count: usize,
    /// Whether files were modified in this turn.
    pub files_modified: bool,
    /// Whether a watch command is currently set.
    pub has_watch: bool,
    /// Whether the last tool call produced an error.
    pub had_tool_error: bool,
    /// Ratio of tokens used (0.0–1.0).
    pub context_usage_ratio: f64,
    /// Number of consecutive turns without any slash command.
    pub turns_since_slash_command: usize,
}

/// Return at most one contextual hint based on what just happened.
///
/// Rules are evaluated in priority order. Each hint category fires at most
/// once per session. Returns `None` when no rules match or the matching
/// category was already shown. Callers should gate on `is_quiet()` before
/// printing.
pub fn contextual_hint(ctx: &HintContext) -> Option<String> {
    let mut guard = shown_hints().lock().ok()?;
    contextual_hint_with(&mut guard, ctx)
}

/// The decision half of [`contextual_hint`]. **Pure** in the sense that matters
/// here: the once-per-session memory arrives as a `&mut` parameter instead of
/// being read out of the process-wide `SHOWN_HINTS`, so a test drives local
/// state and never calls `reset_shown_hints()`. Same shape as
/// [`context_budget_warning_with`], for the same reason.
pub(crate) fn contextual_hint_with(
    shown: &mut HashSet<&'static str>,
    ctx: &HintContext,
) -> Option<String> {
    // Priority-ordered rules. First match wins.
    let candidates: &[(&str, bool, &str)] = &[
        (
            "first_turn",
            ctx.turn_count == 1,
            "💡 Type /help to see available commands",
        ),
        (
            "watch",
            ctx.files_modified && !ctx.has_watch,
            "💡 /watch to auto-test after every prompt",
        ),
        (
            "retry",
            ctx.had_tool_error,
            "💡 /retry to re-run with the error context",
        ),
        (
            "compact",
            ctx.context_usage_ratio > 0.5,
            "💡 /compact to free context space",
        ),
        (
            "diff",
            ctx.files_modified,
            "💡 /diff to review changes, /commit to save",
        ),
        (
            "tips",
            ctx.turns_since_slash_command >= 3,
            "💡 Try /tips to discover features",
        ),
    ];

    for &(category, condition, message) in candidates {
        if condition && !shown.contains(category) {
            shown.insert(category);
            return Some(message.to_string());
        }
    }

    None
}

#[cfg(test)]
pub fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// The decision half of `color_enabled`, driven in **both** directions —
    /// a discriminator tested only on the side that fires is vacuous green.
    ///
    /// This is what replaces the coverage the `cfg(test)` pin in
    /// `color_enabled` appears to remove. Nothing was actually lost: the
    /// disabled branch was never observable from a test in the first place
    /// (`COLOR_DISABLED` is a write-once `OnceLock` seeded from the ambient
    /// environment, with no setter), so what looked like coverage was the
    /// runner's environment, not an assertion. Here the rule itself is pinned.
    #[test]
    fn color_enabled_from_env_table() {
        // NO_COLOR present (any value, per no-color.org) -> colour off.
        assert!(!color_enabled_from_env(true));
        // NO_COLOR absent -> colour on. This is the value the `cfg(test)` pin
        // hardcodes, which is why `cargo test` is byte-identical to before.
        assert!(color_enabled_from_env(false));
    }

    /// Serializes tests that touch the process-wide `LAST_WARNED_THRESHOLD`
    /// static. `cargo test` runs tests in parallel threads by default, so
    /// without this lock one test's `store(...)` could land between another
    /// test's `reset_context_budget_warning()` and its assertion — a real,
    /// schedule-dependent flake we saw in CI. Hold the guard for the whole
    /// test body to guarantee mutual exclusion. `unwrap_or_else(into_inner)`
    /// recovers from poisoning so one failing test doesn't cascade.
    #[cfg(test)]
    static BUDGET_WARNING_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // === truncate_with_ellipsis tests ===

    #[test]
    fn truncate_with_ellipsis_shorter_than_max_is_unchanged() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn truncate_with_ellipsis_exactly_max_chars_is_unchanged() {
        // Exactly `max` chars: char_indices().nth(max) is None → no ellipsis.
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn truncate_with_ellipsis_longer_than_max_appends_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello…");
    }

    #[test]
    fn truncate_with_ellipsis_counts_chars_not_bytes() {
        // "héllo" is 6 bytes but 5 chars — 4 chars kept, then ellipsis.
        // Slicing must land on a char boundary (é is 2 bytes) or this panics.
        assert_eq!(truncate_with_ellipsis("héllo", 4), "héll…");
    }

    #[test]
    fn truncate_with_ellipsis_multibyte_at_cut_point_does_not_panic() {
        // The char at the cut boundary is multi-byte (✓ = 3 bytes).
        // Regression guard against byte-indexing panics (CLAUDE.md UTF-8 rule).
        assert_eq!(truncate_with_ellipsis("ab✓cd", 3), "ab✓…");
    }

    #[test]
    fn truncate_with_ellipsis_empty_string() {
        assert_eq!(truncate_with_ellipsis("", 5), "");
    }

    #[test]
    fn truncate_with_ellipsis_zero_max() {
        // max=0 means keep nothing → just the ellipsis.
        assert_eq!(truncate_with_ellipsis("hello", 0), "…");
    }

    // === notify_command decision + spawn tests ===

    #[test]
    fn should_run_notify_command_just_below_threshold_is_false() {
        // 2s < LONG_PROMPT_THRESHOLD_SECS (3s) — even when configured.
        assert!(!should_run_notify_command(Duration::from_secs(2), true));
    }

    #[test]
    fn should_run_notify_command_at_threshold_is_true() {
        assert!(should_run_notify_command(Duration::from_secs(3), true));
    }

    #[test]
    fn should_run_notify_command_above_threshold_is_true() {
        assert!(should_run_notify_command(Duration::from_secs(120), true));
    }

    #[test]
    fn should_run_notify_command_not_configured_is_false() {
        // Long prompt but no command configured → completely inert.
        assert!(!should_run_notify_command(Duration::from_secs(60), false));
        assert!(!should_run_notify_command(Duration::from_secs(2), false));
    }

    #[cfg(unix)]
    #[test]
    fn run_notify_command_spawns_without_panicking() {
        // Fire-and-forget: spawn a trivially-successful command and a
        // guaranteed-missing binary; neither may panic or block.
        run_notify_command("true", Duration::from_secs(5));
        run_notify_command(
            "/nonexistent/definitely-not-a-binary",
            Duration::from_secs(5),
        );
    }

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_unicode() {
        assert_eq!(truncate("héllo wörld", 5), "héllo");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate("", 5), "");
    }

    // Issue #263: tiny non-zero usage rendered as "0%" because integer math
    // truncated to 0; the label should say "<1%" so the user can tell tokens
    // were actually consumed.
    #[test]
    fn context_usage_label_tiny_usage_shows_less_than_one_percent() {
        let label = context_usage_label(500, 200_000);
        assert_eq!(label, "<1%");
    }

    #[test]
    fn context_usage_label_zero_usage_is_zero_percent() {
        let label = context_usage_label(0, 200_000);
        assert_eq!(label, "0%");
    }

    #[test]
    fn context_usage_label_normal_usage_unchanged() {
        let label = context_usage_label(50_000, 200_000);
        assert_eq!(label, "25%");
    }

    #[test]
    fn context_usage_label_full_usage() {
        let label = context_usage_label(200_000, 200_000);
        assert_eq!(label, "100%");
    }

    #[test]
    fn context_usage_label_zero_max_safe() {
        // Defensive: should not divide by zero.
        let label = context_usage_label(100, 0);
        assert_eq!(label, "0%");
    }

    #[test]
    fn test_safe_truncate_empty_string() {
        assert_eq!(safe_truncate("", 10), "");
    }

    #[test]
    fn test_safe_truncate_ascii_shorter_than_max() {
        assert_eq!(safe_truncate("hello", 10), "hello");
    }

    #[test]
    fn test_safe_truncate_ascii_longer_than_max() {
        assert_eq!(safe_truncate("hello world", 5), "hello");
    }

    #[test]
    fn test_safe_truncate_multibyte_no_panic() {
        // ✓ is 3 bytes (E2 9C 93). "hello ✓ world" = 13 chars, 15 bytes
        let s = "hello ✓ world";
        // Truncating at byte 7 would land inside ✓ — should back up to byte 6
        let result = safe_truncate(s, 7);
        assert_eq!(result, "hello ");
        // Truncating at byte 9 should include ✓ (bytes 6-8)
        let result = safe_truncate(s, 9);
        assert_eq!(result, "hello ✓");
    }

    #[test]
    fn test_safe_truncate_all_multibyte() {
        // Each CJK char is 3 bytes: "日本語テスト" = 18 bytes, 6 chars
        let s = "日本語テスト";
        // Truncating at 4 bytes should back up to 3 (one char)
        let result = safe_truncate(s, 4);
        assert_eq!(result, "日");
        // Truncating at 7 should back up to 6 (two chars)
        let result = safe_truncate(s, 7);
        assert_eq!(result, "日本");
    }

    #[test]
    fn test_safe_truncate_zero_max() {
        assert_eq!(safe_truncate("hello", 0), "");
        assert_eq!(safe_truncate("日本語", 0), "");
    }

    #[test]
    fn test_safe_truncate_exact_boundary() {
        // "ab✓" = 5 bytes. Truncating at exactly 5 should return all.
        let s = "ab✓";
        assert_eq!(safe_truncate(s, 5), "ab✓");
        // Truncating at 4 lands mid-char, should back up to 2
        assert_eq!(safe_truncate(s, 4), "ab");
        // Truncating at 2 should give "ab"
        assert_eq!(safe_truncate(s, 2), "ab");
    }

    #[test]
    fn test_safe_byte_index_empty_string() {
        assert_eq!(safe_byte_index("", 0), 0);
        assert_eq!(safe_byte_index("", 5), 0);
    }

    #[test]
    fn test_safe_byte_index_ascii_within_bounds() {
        assert_eq!(safe_byte_index("hello world", 5), 5);
        assert_eq!(safe_byte_index("hello world", 0), 0);
    }

    #[test]
    fn test_safe_byte_index_multibyte_rounds_down() {
        // ✓ is 3 bytes (E2 9C 93). "hello ✓ world" byte layout:
        //   h(0) e(1) l(2) l(3) o(4) ' '(5) ✓(6,7,8) ' '(9) w(10)...
        let s = "hello ✓ world";
        // Target 7 lands inside ✓ → rounds down to 6
        assert_eq!(safe_byte_index(s, 7), 6);
        // Target 8 also inside ✓ → rounds down to 6
        assert_eq!(safe_byte_index(s, 8), 6);
        // Target 6 is start of ✓ → valid boundary
        assert_eq!(safe_byte_index(s, 6), 6);
        // Target 9 is after ✓ → valid boundary
        assert_eq!(safe_byte_index(s, 9), 9);
    }

    #[test]
    fn test_safe_byte_index_beyond_length() {
        let s = "hello";
        assert_eq!(safe_byte_index(s, 100), 5); // clamped to len
        let s2 = "日本"; // 6 bytes
        assert_eq!(safe_byte_index(s2, 999), 6);
    }

    #[test]
    fn test_truncate_adds_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello…");
        assert_eq!(truncate_with_ellipsis("hi", 5), "hi");
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn test_format_tool_summary_bash() {
        let args = serde_json::json!({"command": "echo hello"});
        assert_eq!(format_tool_summary("bash", &args), "$ echo hello");
    }

    #[test]
    fn test_format_tool_summary_bash_long_command() {
        let long_cmd = "a".repeat(100);
        let args = serde_json::json!({"command": long_cmd});
        let result = format_tool_summary("bash", &args);
        assert!(result.starts_with("$ "));
        assert!(result.ends_with('…'));
        assert!(result.len() < 100);
    }

    #[test]
    fn test_format_tool_summary_read_file() {
        let args = serde_json::json!({"path": "src/main.rs"});
        assert_eq!(format_tool_summary("read_file", &args), "read src/main.rs");
    }

    #[test]
    fn test_format_tool_summary_write_file() {
        let args = serde_json::json!({"path": "out.txt"});
        assert_eq!(format_tool_summary("write_file", &args), "write out.txt");
    }

    #[test]
    fn test_format_tool_summary_edit_file() {
        let args = serde_json::json!({"path": "foo.rs"});
        assert_eq!(format_tool_summary("edit_file", &args), "edit foo.rs");
    }

    #[test]
    fn test_format_tool_summary_list_files() {
        let args = serde_json::json!({"path": "src/"});
        assert_eq!(format_tool_summary("list_files", &args), "ls src/");
    }

    #[test]
    fn test_format_tool_summary_list_files_no_path() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("list_files", &args), "ls .");
    }

    #[test]
    fn test_format_tool_summary_search() {
        let args = serde_json::json!({"pattern": "TODO"});
        assert_eq!(format_tool_summary("search", &args), "search 'TODO'");
    }

    #[test]
    fn test_format_tool_summary_unknown_tool() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("custom_tool", &args), "custom_tool");
    }

    #[test]
    fn test_color_struct_display_outputs_ansi() {
        // Color struct should produce the ANSI code when color is enabled
        let c = Color("\x1b[1m");
        let formatted = format!("{c}");
        // We can't guarantee NO_COLOR isn't set in the test environment,
        // but the type itself should compile and format correctly.
        assert!(formatted == "\x1b[1m" || formatted.is_empty());
    }

    // --- format_tool_summary write_file with line count ---

    #[test]
    fn test_format_tool_summary_write_file_with_content() {
        let args = serde_json::json!({"path": "out.txt", "content": "line1\nline2\nline3"});
        let result = format_tool_summary("write_file", &args);
        assert_eq!(result, "write out.txt (3 lines)");
    }

    #[test]
    fn test_format_tool_summary_write_file_single_line() {
        let args = serde_json::json!({"path": "out.txt", "content": "hello"});
        let result = format_tool_summary("write_file", &args);
        assert_eq!(result, "write out.txt (1 line)");
    }

    #[test]
    fn test_format_tool_summary_write_file_no_content() {
        let args = serde_json::json!({"path": "out.txt"});
        let result = format_tool_summary("write_file", &args);
        assert_eq!(result, "write out.txt");
    }

    // --- format_tool_summary enriched details ---

    #[test]
    fn test_format_tool_summary_read_file_with_offset_and_limit() {
        let args = serde_json::json!({"path": "src/main.rs", "offset": 10, "limit": 50});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs:10..60");
    }

    #[test]
    fn test_format_tool_summary_read_file_with_offset_only() {
        let args = serde_json::json!({"path": "src/main.rs", "offset": 100});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs:100..");
    }

    #[test]
    fn test_format_tool_summary_read_file_with_limit_only() {
        let args = serde_json::json!({"path": "src/main.rs", "limit": 25});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs (25 lines)");
    }

    #[test]
    fn test_format_tool_summary_read_file_no_extras() {
        let args = serde_json::json!({"path": "src/main.rs"});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs");
    }

    #[test]
    fn test_format_tool_summary_edit_file_with_text() {
        let args = serde_json::json!({
            "path": "foo.rs",
            "old_text": "fn old() {\n}\n",
            "new_text": "fn new() {\n    // improved\n    do_stuff();\n}\n"
        });
        let result = format_tool_summary("edit_file", &args);
        assert_eq!(result, "edit foo.rs (2 → 4 lines)");
    }

    #[test]
    fn test_format_tool_summary_edit_file_no_text() {
        let args = serde_json::json!({"path": "foo.rs"});
        let result = format_tool_summary("edit_file", &args);
        assert_eq!(result, "edit foo.rs");
    }

    #[test]
    fn test_format_tool_summary_edit_file_same_lines() {
        let args = serde_json::json!({
            "path": "foo.rs",
            "old_text": "let x = 1;",
            "new_text": "let x = 2;"
        });
        let result = format_tool_summary("edit_file", &args);
        assert_eq!(result, "edit foo.rs (1 → 1 lines)");
    }

    #[test]
    fn test_format_tool_summary_search_with_path() {
        let args = serde_json::json!({"pattern": "TODO", "path": "src/"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'TODO' in src/");
    }

    #[test]
    fn test_format_tool_summary_search_with_include() {
        let args = serde_json::json!({"pattern": "fn main", "include": "*.rs"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'fn main' (*.rs)");
    }

    #[test]
    fn test_format_tool_summary_search_with_path_and_include() {
        let args = serde_json::json!({"pattern": "test", "path": "src/", "include": "*.rs"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'test' in src/ (*.rs)");
    }

    #[test]
    fn test_format_tool_summary_search_pattern_only() {
        let args = serde_json::json!({"pattern": "TODO"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'TODO'");
    }

    #[test]
    fn test_format_tool_summary_list_files_with_pattern() {
        let args = serde_json::json!({"path": "src/", "pattern": "*.rs"});
        let result = format_tool_summary("list_files", &args);
        assert_eq!(result, "ls src/ (*.rs)");
    }

    #[test]
    fn test_format_tool_summary_list_files_pattern_no_path() {
        let args = serde_json::json!({"pattern": "*.toml"});
        let result = format_tool_summary("list_files", &args);
        assert_eq!(result, "ls . (*.toml)");
    }

    #[test]
    fn test_format_tool_summary_bash_multiline_shows_first_line() {
        let args = serde_json::json!({"command": "cd src\ngrep -r 'test' ."});
        let result = format_tool_summary("bash", &args);
        assert!(
            result.starts_with("$ cd src"),
            "Should show first line: {result}"
        );
        assert!(
            result.contains("(2 lines)"),
            "Should indicate line count: {result}"
        );
    }

    // --- format_tool_summary: rename_symbol, todo, web_search, sub_agent ---

    #[test]
    fn test_format_tool_summary_rename_symbol() {
        let args = serde_json::json!({"old_name": "foo", "new_name": "bar"});
        let result = format_tool_summary("rename_symbol", &args);
        assert_eq!(result, "rename foo → bar");
    }

    #[test]
    fn test_format_tool_summary_rename_symbol_with_path() {
        let args = serde_json::json!({"old_name": "Foo", "new_name": "Bar", "path": "src/"});
        let result = format_tool_summary("rename_symbol", &args);
        assert_eq!(result, "rename Foo → Bar in src/");
    }

    #[test]
    fn test_format_tool_summary_todo_add() {
        let args = serde_json::json!({"action": "add", "description": "Fix the bug"});
        let result = format_tool_summary("todo", &args);
        assert_eq!(result, "todo add 'Fix the bug'");
    }

    #[test]
    fn test_format_tool_summary_todo_done() {
        let args = serde_json::json!({"action": "done", "id": 3});
        let result = format_tool_summary("todo", &args);
        assert_eq!(result, "todo done #3");
    }

    #[test]
    fn test_format_tool_summary_todo_wip() {
        let args = serde_json::json!({"action": "wip", "id": 1});
        let result = format_tool_summary("todo", &args);
        assert_eq!(result, "todo wip #1");
    }

    #[test]
    fn test_format_tool_summary_todo_list() {
        let args = serde_json::json!({"action": "list"});
        let result = format_tool_summary("todo", &args);
        assert_eq!(result, "todo list");
    }

    #[test]
    fn test_format_tool_summary_todo_clear() {
        let args = serde_json::json!({"action": "clear"});
        let result = format_tool_summary("todo", &args);
        assert_eq!(result, "todo clear");
    }

    #[test]
    fn test_format_tool_summary_todo_remove_no_id() {
        let args = serde_json::json!({"action": "remove"});
        let result = format_tool_summary("todo", &args);
        assert_eq!(result, "todo remove");
    }

    #[test]
    fn test_format_tool_summary_web_search() {
        let args = serde_json::json!({"query": "rust async tutorial"});
        let result = format_tool_summary("web_search", &args);
        assert_eq!(result, "web search 'rust async tutorial'");
    }

    #[test]
    fn test_format_tool_summary_web_search_long_query() {
        let query = "a]".repeat(40); // 80 chars
        let args = serde_json::json!({"query": query});
        let result = format_tool_summary("web_search", &args);
        assert!(
            result.contains("…"),
            "Long query should be truncated: {result}"
        );
    }

    #[test]
    fn test_format_tool_summary_sub_agent() {
        let args = serde_json::json!({"task": "Analyze the test failures"});
        let result = format_tool_summary("sub_agent", &args);
        assert_eq!(result, "sub-agent 'Analyze the test failures'");
    }

    #[test]
    fn test_format_tool_summary_sub_agent_long_task() {
        let task = "x".repeat(100);
        let args = serde_json::json!({"task": task});
        let result = format_tool_summary("sub_agent", &args);
        assert!(
            result.contains("…"),
            "Long task should be truncated: {result}"
        );
    }

    #[test]
    fn test_format_tool_summary_todo_add_long_description() {
        let desc = "y".repeat(80);
        let args = serde_json::json!({"action": "add", "description": desc});
        let result = format_tool_summary("todo", &args);
        assert!(
            result.contains("…"),
            "Long description should be truncated: {result}"
        );
    }

    // --- pluralize ---

    #[test]
    fn test_decode_html_entities_named() {
        assert_eq!(decode_html_entities("&amp;"), "&");
        assert_eq!(decode_html_entities("&lt;"), "<");
        assert_eq!(decode_html_entities("&gt;"), ">");
        assert_eq!(decode_html_entities("&quot;"), "\"");
        assert_eq!(decode_html_entities("&apos;"), "'");
        assert_eq!(decode_html_entities("&#39;"), "'");
        assert_eq!(decode_html_entities("&nbsp;"), " ");
        assert_eq!(decode_html_entities("&#x27;"), "'");
        assert_eq!(decode_html_entities("&mdash;"), "—");
        assert_eq!(decode_html_entities("&ndash;"), "–");
        assert_eq!(decode_html_entities("&hellip;"), "…");
        assert_eq!(decode_html_entities("&copy;"), "©");
        assert_eq!(decode_html_entities("&reg;"), "®");
    }

    #[test]
    fn test_decode_html_entities_numeric_decimal() {
        // &#65; = 'A'
        assert_eq!(decode_html_entities("&#65;"), "A");
        // &#8212; = '—' (em dash)
        assert_eq!(decode_html_entities("&#8212;"), "—");
    }

    #[test]
    fn test_decode_html_entities_numeric_hex() {
        // &#x41; = 'A'
        assert_eq!(decode_html_entities("&#x41;"), "A");
        // &#x2014; = '—' (em dash)
        assert_eq!(decode_html_entities("&#x2014;"), "—");
    }

    #[test]
    fn test_decode_html_entities_mixed() {
        assert_eq!(
            decode_html_entities("hello &amp; world &lt;3 &#8212; done"),
            "hello & world <3 — done"
        );
    }

    #[test]
    fn test_decode_html_entities_no_entities() {
        assert_eq!(decode_html_entities("plain text"), "plain text");
    }

    #[test]
    fn test_decode_html_entities_invalid_numeric() {
        // Invalid numeric entity — should be preserved as-is
        assert_eq!(decode_html_entities("&#xZZZZ;"), "&#xZZZZ;");
        assert_eq!(decode_html_entities("&#abc;"), "&#abc;");
    }

    #[test]
    fn test_decode_html_entities_incomplete() {
        // Ampersand not part of an entity
        assert_eq!(decode_html_entities("a & b"), "a & b");
    }

    #[test]
    fn test_decode_html_entities_no_phantom_semicolon() {
        // Invariant: decode never invents characters that weren't in the input.
        // An unterminated (no ';') AND unparseable numeric entity must not gain
        // a phantom ';'. Regression for the Day-146 phantom-semicolon bug.
        assert_eq!(decode_html_entities("a &#zz b"), "a &#zz b");
        assert_eq!(decode_html_entities("x &#"), "x &#");
        // Malformed hex with no terminator: also no phantom ';'.
        assert_eq!(decode_html_entities("&#xZZ"), "&#xZZ");
    }

    #[test]
    fn test_decode_html_entities_terminated_still_decodes() {
        // A valid, terminated entity still decodes.
        assert_eq!(decode_html_entities("&#65;"), "A");
    }

    #[test]
    fn test_decode_html_entities_unterminated_but_parseable() {
        // Unterminated-but-parseable numeric entity: HTML parsers commonly
        // accept these. We decode it (no ';' required to parse the digits).
        assert_eq!(decode_html_entities("&#65"), "A");
    }

    // --- Section header and divider tests ---

    #[test]
    fn test_section_header_contains_label_and_line_chars() {
        let header = section_header("Thinking");
        assert!(
            header.contains("Thinking"),
            "header should contain the label"
        );
        assert!(
            header.contains("─"),
            "header should contain box-drawing chars"
        );
    }

    #[test]
    fn test_section_header_empty_label_produces_divider() {
        let header = section_header("");
        // Empty label should produce the same as section_divider
        let divider = section_divider();
        assert_eq!(header, divider);
    }

    #[test]
    fn test_section_divider_nonempty_with_line_chars() {
        let divider = section_divider();
        assert!(!divider.is_empty(), "divider should not be empty");
        assert!(
            divider.contains("─"),
            "divider should contain box-drawing chars"
        );
    }

    #[test]
    fn test_section_header_no_color() {
        // When NO_COLOR is set, the output still contains the label and line chars
        // (Color codes render as empty strings, but the structural content remains)
        let header = section_header("Tools");
        assert!(header.contains("Tools"));
        assert!(header.contains("─"));
    }

    #[test]
    fn test_section_divider_no_color() {
        let divider = section_divider();
        assert!(divider.contains("─"));
    }

    #[test]
    fn test_terminal_width_default() {
        // terminal_width should return a reasonable default (80) when COLUMNS is not set
        // or it should return the value of COLUMNS if set
        let width = terminal_width();
        assert!(width > 0, "terminal width should be positive");
    }

    #[test]
    fn test_section_header_with_various_labels() {
        // Test with different labels to ensure formatting works
        for label in &[
            "Thinking",
            "Response",
            "A",
            "Very Long Section Label For Testing",
        ] {
            let header = section_header(label);
            assert!(header.contains(label), "header should contain '{}'", label);
            assert!(header.contains("──"), "header should have line prefix");
        }
    }

    // ── tool batch summary tests ──────────────────────────────────
    // ── turn boundary tests ──────────────────────────────────

    #[test]
    fn test_turn_boundary_contains_number() {
        let result = turn_boundary(1);
        assert!(result.contains("Turn 1"), "should show turn number");
        assert!(result.contains("╭"), "should have box-drawing start");
        assert!(result.contains("╮"), "should have box-drawing end");
    }

    #[test]
    fn test_turn_boundary_different_numbers() {
        for n in [1, 5, 10, 99] {
            let result = turn_boundary(n);
            assert!(
                result.contains(&format!("Turn {n}")),
                "should contain Turn {n}"
            );
        }
    }

    #[test]
    fn test_turn_boundary_has_fill_characters() {
        let result = turn_boundary(1);
        assert!(result.contains("─"), "should have fill characters");
    }

    // --- Streaming latency tests (issue #147) ---

    #[test]
    fn test_bell_enabled_default() {
        // Verify bell_enabled() is callable and returns a bool without panicking.
        // Since OnceLock is global, the value depends on test ordering and env,
        // but the function itself should never panic.
        let _result = bell_enabled();
    }

    #[test]
    fn test_maybe_ring_bell_short_duration_no_bell() {
        // Durations under 3s should never ring the bell, regardless of settings.
        // This just verifies no panic or error — the bell character is harmless
        // even if it does get emitted.
        maybe_ring_bell(Duration::from_secs(0));
        maybe_ring_bell(Duration::from_secs(1));
        maybe_ring_bell(Duration::from_secs(2));
        // No assertion needed — we're testing that it doesn't panic.
    }

    #[test]
    fn test_maybe_ring_bell_long_duration_no_panic() {
        // Durations >= 3s should attempt the bell if enabled.
        // In test environment this is harmless.
        maybe_ring_bell(Duration::from_secs(3));
        maybe_ring_bell(Duration::from_secs(60));
    }

    // ── format_usage_line tests ────────────────────────────────────

    #[test]
    fn test_format_usage_compact() {
        let usage = yoagent::Usage {
            input: 1119,
            output: 47,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = yoagent::Usage {
            input: 1119,
            output: 47,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let elapsed = Duration::from_secs_f64(1.0);
        let line = format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, false)
            .expect("should produce output");
        // Compact: ↳ 1.0s · 1119→47 tokens · $0.006
        assert!(line.starts_with("↳ 1.0s"), "got: {line}");
        assert!(line.contains("1119→47 tokens"), "got: {line}");
        // Should NOT contain verbose markers
        assert!(!line.contains("session:"), "got: {line}");
        assert!(!line.contains("in /"), "got: {line}");
    }

    #[test]
    fn test_format_usage_verbose() {
        let usage = yoagent::Usage {
            input: 500,
            output: 100,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = yoagent::Usage {
            input: 2000,
            output: 400,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let elapsed = Duration::from_secs(3);
        let line = format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, true)
            .expect("should produce output");
        // Verbose: tokens: 500 in / 100 out  (session: 2000 in / 400 out) ...
        assert!(line.contains("tokens: 500 in / 100 out"), "got: {line}");
        assert!(line.contains("session: 2000 in / 400 out"), "got: {line}");
        assert!(line.contains("⏱"), "got: {line}");
    }

    #[test]
    fn test_format_usage_zero_tokens_returns_none() {
        let usage = yoagent::Usage {
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = usage.clone();
        let elapsed = Duration::from_secs(1);
        assert!(
            format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, false).is_none()
        );
        assert!(
            format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, true).is_none()
        );
    }

    #[test]
    fn test_format_usage_verbose_with_cache() {
        let usage = yoagent::Usage {
            input: 1000,
            output: 200,
            cache_read: 500,
            cache_write: 100,
            total_tokens: 0,
        };
        let total = usage.clone();
        let elapsed = Duration::from_secs(2);
        let line = format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, true)
            .expect("should produce output");
        assert!(line.contains("[cache: 500 read, 100 write]"), "got: {line}");
    }

    #[test]
    fn test_format_usage_compact_includes_cost() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = usage.clone();
        let elapsed = Duration::from_secs(5);
        let line = format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, false)
            .expect("should produce output");
        // Should have cost separator
        assert!(line.contains(" · $"), "compact should include cost: {line}");
    }

    #[test]
    fn test_format_usage_compact_unknown_model_no_cost() {
        let usage = yoagent::Usage {
            input: 100,
            output: 50,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = usage.clone();
        let elapsed = Duration::from_millis(500);
        let line = format_usage_line(&usage, &total, "unknown-model-xyz", elapsed, false)
            .expect("should produce output");
        // No cost for unknown model
        assert!(
            !line.contains("$"),
            "unknown model should have no cost: {line}"
        );
        assert!(line.contains("100→50 tokens"), "got: {line}");
    }

    #[test]
    fn test_format_usage_compact_shows_tok_per_sec() {
        let usage = yoagent::Usage {
            input: 500,
            output: 100,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = usage.clone();
        // 100 output tokens in 2.0s = 50 tok/s
        let elapsed = Duration::from_secs_f64(2.0);
        let line = format_usage_line(&usage, &total, "unknown-model-xyz", elapsed, false)
            .expect("should produce output");
        assert!(
            line.contains("(50 tok/s)"),
            "compact should include tok/s: {line}"
        );
    }

    #[test]
    fn test_format_usage_compact_omits_tok_per_sec_when_fast() {
        let usage = yoagent::Usage {
            input: 500,
            output: 100,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = usage.clone();
        // elapsed < 0.1s → no tok/s
        let elapsed = Duration::from_millis(50);
        let line = format_usage_line(&usage, &total, "unknown-model-xyz", elapsed, false)
            .expect("should produce output");
        assert!(
            !line.contains("tok/s"),
            "should omit tok/s for tiny elapsed: {line}"
        );
    }

    #[test]
    fn test_format_usage_verbose_shows_speed() {
        let usage = yoagent::Usage {
            input: 1000,
            output: 200,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = usage.clone();
        // 200 output tokens in 4.0s = 50 tok/s
        let elapsed = Duration::from_secs_f64(4.0);
        let line = format_usage_line(&usage, &total, "unknown-model-xyz", elapsed, true)
            .expect("should produce output");
        assert!(
            line.contains("speed: 50 tok/s"),
            "verbose should include speed: {line}"
        );
    }

    #[test]
    fn test_format_usage_tok_per_sec_calculation() {
        let usage = yoagent::Usage {
            input: 300,
            output: 177,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = usage.clone();
        // 177 output tokens in 1.0s = 177 tok/s
        let elapsed = Duration::from_secs_f64(1.0);
        let line = format_usage_line(&usage, &total, "unknown-model-xyz", elapsed, false)
            .expect("should produce output");
        assert!(
            line.contains("(177 tok/s)"),
            "should show correct calculation: {line}"
        );
    }

    // ── ThinkBlockFilter tests ───────────────────────────────────────

    // ── context_usage_color tests ─────────────────────────────────────

    #[test]
    fn test_context_usage_color_green_at_zero() {
        let color = context_usage_color(0);
        assert_eq!(color.0, GREEN.0);
    }

    #[test]
    fn test_context_usage_color_green_at_50() {
        let color = context_usage_color(50);
        assert_eq!(color.0, GREEN.0);
    }

    #[test]
    fn test_context_usage_color_yellow_at_51() {
        let color = context_usage_color(51);
        assert_eq!(color.0, YELLOW.0);
    }

    #[test]
    fn test_context_usage_color_yellow_at_80() {
        let color = context_usage_color(80);
        assert_eq!(color.0, YELLOW.0);
    }

    #[test]
    fn test_context_usage_color_red_at_81() {
        let color = context_usage_color(81);
        assert_eq!(color.0, RED.0);
    }

    #[test]
    fn test_context_usage_color_red_at_100() {
        let color = context_usage_color(100);
        assert_eq!(color.0, RED.0);
    }

    // ── context_budget_warning tests ───────────────────────────────────

    // === context budget warning tests ===
    //
    // These drive a LOCAL `AtomicU32` through `context_budget_warning_with`, so
    // no two tests (here or anywhere else in the crate) can race each other via
    // the process-wide `LAST_WARNED_THRESHOLD`. The flake class is gone by
    // construction rather than by a wider lock. One test below still exercises
    // the global path, to prove the public function is actually wired to it.

    #[test]
    fn test_context_budget_warning_below_60_returns_none() {
        let state = AtomicU32::new(0);
        assert!(context_budget_warning_with(&state, 0, 100_000).is_none());
        assert!(context_budget_warning_with(&state, 30_000, 100_000).is_none());
        assert!(context_budget_warning_with(&state, 59_999, 100_000).is_none());
        // 59.999%
    }

    #[test]
    fn test_context_budget_warning_60_threshold() {
        let state = AtomicU32::new(0);
        let warn = context_budget_warning_with(&state, 60_000, 100_000);
        assert!(warn.is_some(), "should warn at 60%");
        let msg = warn.unwrap();
        assert!(msg.contains("60% full"), "got: {msg}");
        assert!(msg.contains("/compact"), "got: {msg}");
    }

    #[test]
    fn test_context_budget_warning_80_threshold() {
        let state = AtomicU32::new(0);
        let warn = context_budget_warning_with(&state, 80_000, 100_000);
        assert!(warn.is_some(), "should warn at 80%");
        let msg = warn.unwrap();
        assert!(msg.contains("80% full"), "got: {msg}");
        assert!(msg.contains("/compact"), "got: {msg}");
        assert!(msg.contains("/save"), "got: {msg}");
        assert!(msg.contains("/clear"), "got: {msg}");
    }

    #[test]
    fn test_context_budget_warning_90_threshold() {
        let state = AtomicU32::new(0);
        let warn = context_budget_warning_with(&state, 90_000, 100_000);
        assert!(warn.is_some(), "should warn at 90%");
        let msg = warn.unwrap();
        assert!(msg.contains("90% full"), "got: {msg}");
        assert!(msg.contains("/save"), "got: {msg}");
        assert!(msg.contains("/clear"), "got: {msg}");
    }

    #[test]
    fn test_context_budget_warning_95_threshold() {
        let state = AtomicU32::new(0);
        let warn = context_budget_warning_with(&state, 95_000, 100_000);
        assert!(warn.is_some(), "should warn at 95%");
        let msg = warn.unwrap();
        assert!(msg.contains("nearly full"), "got: {msg}");
        assert!(msg.contains("/clear"), "got: {msg}");
    }

    #[test]
    fn test_context_budget_warning_same_threshold_no_repeat() {
        let state = AtomicU32::new(0);
        // First call at 60% should warn
        let first = context_budget_warning_with(&state, 60_000, 100_000);
        assert!(first.is_some(), "first call should warn");
        // Second call at same threshold should NOT warn
        let second = context_budget_warning_with(&state, 65_000, 100_000);
        assert!(second.is_none(), "same threshold should not repeat");
    }

    #[test]
    fn test_context_budget_warning_escalates() {
        let state = AtomicU32::new(0);
        let w60 = context_budget_warning_with(&state, 60_000, 100_000);
        assert!(w60.is_some());
        // Jumping to 80% should warn again (higher threshold)
        let w80 = context_budget_warning_with(&state, 80_000, 100_000);
        assert!(w80.is_some(), "should warn at new higher threshold");
        assert!(w80.unwrap().contains("80% full"));
    }

    #[test]
    fn test_context_budget_warning_reset_rearms() {
        let state = AtomicU32::new(0);
        let w1 = context_budget_warning_with(&state, 60_000, 100_000);
        assert!(w1.is_some());
        // Resetting the state should allow the same threshold to warn again
        state.store(0, Ordering::Relaxed);
        let w2 = context_budget_warning_with(&state, 60_000, 100_000);
        assert!(w2.is_some(), "should warn again after reset");
    }

    #[test]
    fn test_context_budget_warning_zero_max_returns_none() {
        let state = AtomicU32::new(0);
        assert!(context_budget_warning_with(&state, 100, 0).is_none());
    }

    // === pure-half tests: threshold classifier and message renderer ===

    #[test]
    fn test_budget_warning_threshold_ladder() {
        assert_eq!(budget_warning_threshold(0, 100_000), None);
        assert_eq!(budget_warning_threshold(59_999, 100_000), None);
        assert_eq!(budget_warning_threshold(60_000, 100_000), Some(60));
        assert_eq!(budget_warning_threshold(79_999, 100_000), Some(60));
        assert_eq!(budget_warning_threshold(80_000, 100_000), Some(80));
        assert_eq!(budget_warning_threshold(90_000, 100_000), Some(90));
        assert_eq!(budget_warning_threshold(95_000, 100_000), Some(95));
        // Over-budget clamps to 100% → still the top threshold, never a panic.
        assert_eq!(budget_warning_threshold(500_000, 100_000), Some(95));
        // Unknown budget is an explicit third value, not 0%.
        assert_eq!(budget_warning_threshold(100, 0), None);
    }

    #[test]
    fn test_budget_warning_message_unknown_threshold_is_none() {
        assert!(budget_warning_message(0).is_none());
        assert!(budget_warning_message(70).is_none());
        assert!(budget_warning_message(100).is_none());
        for t in [60, 80, 90, 95] {
            assert!(
                budget_warning_message(t).is_some(),
                "threshold {t} should render"
            );
        }
    }

    /// The refactor could silently disconnect the public function from its
    /// global state and every local-state test above would stay green. This is
    /// the one test that asserts the wiring itself: `context_budget_warning`
    /// escalates against `LAST_WARNED_THRESHOLD`, and
    /// `reset_context_budget_warning` re-arms *that* state.
    ///
    /// Still takes the shared lock (and `#[serial]`) because it genuinely
    /// mutates process-wide state.
    #[test]
    #[serial]
    fn test_context_budget_warning_global_is_wired_to_shared_state() {
        let _guard = BUDGET_WARNING_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        reset_context_budget_warning();
        assert_eq!(LAST_WARNED_THRESHOLD.load(Ordering::Relaxed), 0);

        // Public fn writes the global...
        assert!(context_budget_warning(80_000, 100_000).is_some());
        assert_eq!(LAST_WARNED_THRESHOLD.load(Ordering::Relaxed), 80);
        // ...and reads it back to suppress a repeat.
        assert!(context_budget_warning(80_000, 100_000).is_none());

        // reset re-arms the same global.
        reset_context_budget_warning();
        assert_eq!(LAST_WARNED_THRESHOLD.load(Ordering::Relaxed), 0);
        assert!(context_budget_warning(80_000, 100_000).is_some());

        reset_context_budget_warning();
    }

    #[test]
    fn test_stderr_is_terminal_returns_bool() {
        // Basic smoke test — stderr_is_terminal() should return a bool without
        // panicking. In CI/test environments stderr is typically not a TTY,
        // so we just verify it runs and returns a deterministic result.
        let result = stderr_is_terminal();
        // Call again to verify caching works (OnceLock returns same value)
        assert_eq!(result, stderr_is_terminal());
    }

    #[test]
    fn test_is_quiet_returns_bool() {
        // is_quiet() should return a bool without panicking.
        // Since OnceLock is global and test ordering is non-deterministic,
        // we just verify it's callable and stable.
        let result = is_quiet();
        assert_eq!(result, is_quiet());
    }

    #[test]
    fn test_enable_quiet_is_callable() {
        // enable_quiet() should not panic even if called after is_quiet()
        // has already initialized the OnceLock. The set() is a no-op if
        // the lock is already initialized.
        enable_quiet();
        // After calling enable_quiet, is_quiet should be true
        // (unless a prior test already initialized it to false — OnceLock is global).
        // We verify it's at least callable and stable.
        let result = is_quiet();
        assert_eq!(result, is_quiet());
    }

    #[test]
    fn test_send_desktop_notification_does_not_panic() {
        // Best-effort fire-and-forget — should never panic regardless of platform.
        send_desktop_notification(Duration::from_secs(15));
    }

    #[test]
    fn test_notify_enabled_returns_bool() {
        // Like bell_enabled, just verify it's callable and stable (OnceLock is global).
        let result = notify_enabled();
        assert_eq!(result, notify_enabled());
    }

    #[test]
    fn test_disable_notify_is_callable() {
        // Should not panic even if OnceLock is already initialized.
        disable_notify();
        let result = notify_enabled();
        assert_eq!(result, notify_enabled());
    }

    #[test]
    fn test_build_notification_message_contains_yoyo() {
        let msg = build_notification_message(Duration::from_secs(15));
        assert!(msg.contains("yoyo"), "message should contain 'yoyo': {msg}");
    }

    #[test]
    fn test_build_notification_message_contains_duration_seconds() {
        let msg = build_notification_message(Duration::from_secs(42));
        assert!(
            msg.contains("42s"),
            "message should contain duration: {msg}"
        );
    }

    #[test]
    fn test_build_notification_message_minutes_format() {
        let msg = build_notification_message(Duration::from_secs(125));
        assert!(
            msg.contains("2m 5s"),
            "message should format minutes and seconds: {msg}"
        );
    }

    #[test]
    fn test_build_notification_message_exact_minutes() {
        let msg = build_notification_message(Duration::from_secs(120));
        assert!(
            msg.contains("2m") && !msg.contains("0s"),
            "exact minutes should omit seconds: {msg}"
        );
    }

    // --- Notification threshold tests ---

    #[test]
    fn test_should_send_notification_below_threshold() {
        // Durations below 10s should NOT trigger a notification.
        assert!(!should_send_notification(Duration::from_secs(0)));
        assert!(!should_send_notification(Duration::from_secs(1)));
        assert!(!should_send_notification(Duration::from_secs(5)));
        assert!(!should_send_notification(Duration::from_secs(9)));
        assert!(!should_send_notification(Duration::from_millis(9999)));
    }

    #[test]
    fn test_should_send_notification_at_threshold() {
        // Exactly 10s should trigger.
        assert!(should_send_notification(Duration::from_secs(10)));
    }

    #[test]
    fn test_should_send_notification_above_threshold() {
        // Durations above 10s should trigger.
        assert!(should_send_notification(Duration::from_secs(11)));
        assert!(should_send_notification(Duration::from_secs(30)));
        assert!(should_send_notification(Duration::from_secs(120)));
        assert!(should_send_notification(Duration::from_secs(3600)));
    }

    #[test]
    fn test_notification_threshold_constant() {
        // The threshold should be 10 seconds.
        assert_eq!(NOTIFICATION_THRESHOLD_SECS, 10);
    }

    #[test]
    fn test_maybe_ring_bell_does_not_panic_short_duration() {
        // Short durations should not cause any issues (no notification sent).
        maybe_ring_bell(Duration::from_secs(1));
        maybe_ring_bell(Duration::from_secs(0));
    }

    #[test]
    fn test_maybe_ring_bell_does_not_panic_long_duration() {
        // Long durations trigger the notification path — should not panic.
        maybe_ring_bell(Duration::from_secs(15));
        maybe_ring_bell(Duration::from_secs(60));
    }

    #[test]
    fn test_notification_platform_command_selection() {
        // Since platform detection uses #[cfg], we can only verify the logic
        // for the current compile target. The key assertion is that
        // send_desktop_notification doesn't panic and is fire-and-forget.
        // On macOS: osascript, on Linux: notify-send, on Windows: powershell.
        // We verify the function is safe to call regardless of platform.
        send_desktop_notification(Duration::from_secs(20));
        // If we got here, the platform-specific branch didn't panic.
    }

    /// Reads this file's own source to check the three wrappers whose decision
    /// halves were moved to `*_with`-style pure cores still perform the one
    /// global read each. **This proves the call is PRESENT, not that its result
    /// is used** — it guards against silent deletion and nothing more. Needles
    /// are built at runtime so this test cannot match itself.
    #[test]
    fn test_print_wrappers_still_read_their_globals() {
        let src =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/format/mod.rs"))
                .expect("read src/format/mod.rs");
        let quiet_needle = format!("{}{}", "is_quiet", "()");
        for (sig, needle) in [
            ("pub fn print_usage(", quiet_needle.clone()),
            (
                "pub fn print_context_usage(used_tokens: u64, max_tokens: u64) {",
                quiet_needle,
            ),
            (
                "pub fn contextual_hint(ctx: &HintContext) -> Option<String> {",
                format!("{}{}", "shown_hints", "()"),
            ),
        ] {
            let body = src
                .split(sig)
                .nth(1)
                .and_then(|rest| rest.split("\n}").next())
                .unwrap_or_else(|| panic!("locate body after `{sig}`"));
            assert!(
                body.contains(&needle),
                "the wrapper at `{sig}` no longer calls `{needle}`. The wrapper is the \
                 ONLY place that global is read now that the decision lives in a pure \
                 core; without it the core is driven with a default and the global is \
                 dead. Body was:\n{body}"
            );
        }
    }

    #[test]
    fn test_print_usage_quiet_suppressed() {
        // Drives the pure core, not the wrapper. It used to call the
        // quiet-mode setter and then `print_usage`, and asserted nothing at
        // all ("must not panic"). That setter is a `OnceLock::set`, i.e. a
        // ONE-WAY door that left QUIET=true for every other test in the
        // binary — the hazard here is order-dependent process poisoning, not
        // only a data race.
        // (Setter names are deliberately written without call parens in this
        // comment: tests/global_state_races.rs matches them as plain text.)
        let usage = yoagent::Usage {
            input: 100,
            output: 50,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 150,
        };
        let elapsed = std::time::Duration::from_secs(2);
        assert!(
            usage_print_line(true, false, &usage, &usage, "test-model", elapsed).is_none(),
            "quiet must suppress the usage line entirely"
        );
        // Near-miss guard: a discriminator tested only on the side that blocks
        // is vacuous green, so pin that the non-quiet side still emits.
        let shown = usage_print_line(false, false, &usage, &usage, "test-model", elapsed);
        assert!(
            shown.is_some(),
            "non-quiet must still produce a usage line (else the quiet \
             assertion above is satisfied by a function that never emits)"
        );
    }

    #[test]
    fn test_print_context_usage_quiet_suppressed() {
        // Same move as the sibling above: the pure core takes `quiet` as an
        // argument, so this no longer writes the process-wide QUIET.
        assert!(
            context_usage_line(true, 5000, 200_000).is_none(),
            "quiet must suppress the context-usage line"
        );
        // Near-miss guard, plus the OTHER cause of `None`, kept distinct:
        // an unknown context window suppresses the line even when not quiet.
        let shown = context_usage_line(false, 5000, 200_000)
            .expect("non-quiet with a known window must emit a line");
        assert!(
            shown.contains("of context window used"),
            "unexpected context-usage line: {shown}"
        );
        assert!(
            context_usage_line(false, 5000, 0).is_none(),
            "max_tokens == 0 must suppress the line for its own reason"
        );
    }

    // ── Contextual hint tests ──────────────────────────────────────────

    fn make_hint_ctx() -> HintContext {
        HintContext {
            turn_count: 5,
            files_modified: false,
            has_watch: false,
            had_tool_error: false,
            context_usage_ratio: 0.1,
            turns_since_slash_command: 0,
        }
    }

    #[test]
    #[serial]
    fn test_hint_first_turn() {
        reset_shown_hints();
        let ctx = HintContext {
            turn_count: 1,
            ..make_hint_ctx()
        };
        let hint = contextual_hint(&ctx);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("/help"));
    }

    #[test]
    #[serial]
    fn test_hint_watch_when_files_modified_no_watch() {
        reset_shown_hints();
        let ctx = HintContext {
            turn_count: 2,
            files_modified: true,
            has_watch: false,
            ..make_hint_ctx()
        };
        let hint = contextual_hint(&ctx);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("/watch"));
    }

    #[test]
    #[serial]
    fn test_hint_no_watch_hint_when_watch_set() {
        reset_shown_hints();
        let ctx = HintContext {
            turn_count: 2,
            files_modified: true,
            has_watch: true,
            ..make_hint_ctx()
        };
        let hint = contextual_hint(&ctx);
        // Should not get the watch hint; might get diff hint instead
        if let Some(ref h) = hint {
            assert!(!h.contains("/watch"));
        }
    }

    #[test]
    #[serial]
    fn test_hint_retry_on_tool_error() {
        reset_shown_hints();
        let ctx = HintContext {
            turn_count: 3,
            had_tool_error: true,
            ..make_hint_ctx()
        };
        let hint = contextual_hint(&ctx);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("/retry"));
    }

    #[test]
    #[serial]
    fn test_hint_compact_on_high_usage() {
        reset_shown_hints();
        let ctx = HintContext {
            turn_count: 3,
            context_usage_ratio: 0.6,
            ..make_hint_ctx()
        };
        let hint = contextual_hint(&ctx);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("/compact"));
    }

    #[test]
    #[serial]
    fn test_hint_diff_when_files_modified() {
        reset_shown_hints();
        // watch hint fires first for files_modified + no watch, so set has_watch
        let ctx = HintContext {
            turn_count: 3,
            files_modified: true,
            has_watch: true,
            ..make_hint_ctx()
        };
        let hint = contextual_hint(&ctx);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("/diff"));
    }

    #[test]
    #[serial]
    fn test_hint_tips_after_no_slash_commands() {
        reset_shown_hints();
        let ctx = HintContext {
            turn_count: 5,
            turns_since_slash_command: 3,
            ..make_hint_ctx()
        };
        let hint = contextual_hint(&ctx);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("/tips"));
    }

    #[test]
    #[serial]
    fn test_hint_no_repeat_same_category() {
        reset_shown_hints();
        let ctx = HintContext {
            turn_count: 3,
            had_tool_error: true,
            ..make_hint_ctx()
        };
        // First time: should get retry hint
        let hint1 = contextual_hint(&ctx);
        assert!(hint1.is_some());
        assert!(hint1.unwrap().contains("/retry"));

        // Second time: retry already shown, should not repeat
        let hint2 = contextual_hint(&ctx);
        // Could be None or a different hint, but NOT retry
        if let Some(ref h) = hint2 {
            assert!(!h.contains("/retry"));
        }
    }

    #[test]
    #[serial]
    fn test_hint_none_when_no_conditions_match() {
        reset_shown_hints();
        let ctx = HintContext {
            turn_count: 3,
            files_modified: false,
            has_watch: true,
            had_tool_error: false,
            context_usage_ratio: 0.1,
            turns_since_slash_command: 0,
        };
        let hint = contextual_hint(&ctx);
        assert!(hint.is_none());
    }

    #[test]
    fn test_hint_priority_first_turn_wins() {
        // Drives the pure core with a LOCAL set, so this no longer calls the
        // global hint-reset setter. Same seam as `context_budget_warning_with`.
        let mut shown: HashSet<&'static str> = HashSet::new();
        // Multiple conditions true, but first_turn is highest priority
        let ctx = HintContext {
            turn_count: 1,
            files_modified: true,
            has_watch: false,
            had_tool_error: true,
            context_usage_ratio: 0.8,
            turns_since_slash_command: 5,
        };
        let hint = contextual_hint_with(&mut shown, &ctx);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("/help"));
        // The once-per-session memory is real, and now checkable without
        // touching the process global: the same ctx must not fire twice.
        assert!(shown.contains("first_turn"));
        let second = contextual_hint_with(&mut shown, &ctx);
        assert!(
            second.is_some_and(|h| !h.contains("/help")),
            "first_turn must not repeat; a lower-priority hint should win"
        );
    }
}
