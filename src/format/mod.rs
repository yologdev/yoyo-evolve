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

/// Ring the terminal bell if enabled and elapsed time exceeds threshold.
/// The bell character (\x07) causes most terminal emulators to flash the tab
/// or play a sound, alerting multitasking developers.
/// Also sends a desktop notification for genuinely long waits (≥10s).
pub fn maybe_ring_bell(elapsed: Duration) {
    if bell_enabled() && elapsed.as_secs() >= 3 {
        let _ = io::stdout().write_all(b"\x07");
        let _ = io::stdout().flush();
    }
    if notify_enabled() && should_send_notification(elapsed) {
        send_desktop_notification(elapsed);
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

/// Check if color output is enabled. Cached after first call.
/// Respects the NO_COLOR environment variable (https://no-color.org/).
fn color_enabled() -> bool {
    !*COLOR_DISABLED.get_or_init(|| std::env::var("NO_COLOR").is_ok())
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
mod markdown;
mod output;
mod tools;

pub use cost::*;
pub use diff::*;
pub use highlight::*;
pub use markdown::*;
pub use output::*;
pub use tools::*;

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
            while let Some(&nc) = chars.peek() {
                if nc == ';' {
                    chars.next();
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
                // Failed to decode — emit original
                decoded.push_str(&entity);
                decoded.push(';');
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
pub fn print_usage(
    usage: &yoagent::Usage,
    total: &yoagent::Usage,
    model: &str,
    elapsed: std::time::Duration,
) {
    if is_quiet() {
        return;
    }
    if let Some(line) = format_usage_line(usage, total, model, elapsed, crate::cli::is_verbose()) {
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

/// Print a context window usage indicator line.
/// Shows percentage of context consumed, color-coded by fullness.
pub fn print_context_usage(used_tokens: u64, max_tokens: u64) {
    if is_quiet() {
        return;
    }
    if max_tokens == 0 {
        return;
    }
    let pct = ((used_tokens as f64 / max_tokens as f64) * 100.0).min(100.0) as u32;
    let color = context_usage_color(pct);
    let label = context_usage_label(used_tokens, max_tokens);
    println!("{DIM}  {color}⬤{RESET}{DIM} {label} of context window used{RESET}");
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
    if max == 0 {
        return None;
    }
    let pct = ((used as f64 / max as f64) * 100.0).min(100.0) as u32;

    let threshold = if pct >= 95 {
        95
    } else if pct >= 90 {
        90
    } else if pct >= 80 {
        80
    } else if pct >= 60 {
        60
    } else {
        return None;
    };

    let prev = LAST_WARNED_THRESHOLD.load(Ordering::Relaxed);
    if threshold <= prev {
        return None;
    }
    LAST_WARNED_THRESHOLD.store(threshold, Ordering::Relaxed);

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
        60 => format!(
            "{DIM}  Context is 60% full — consider /compact to free space{RESET}"
        ),
        _ => return None,
    };

    Some(msg)
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
        if condition && !guard.contains(category) {
            guard.insert(category);
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

    #[test]
    fn test_context_budget_warning_below_60_returns_none() {
        reset_context_budget_warning();
        assert!(context_budget_warning(0, 100_000).is_none());
        assert!(context_budget_warning(10_000, 100_000).is_none()); // 10%
        assert!(context_budget_warning(50_000, 100_000).is_none()); // 50%
        assert!(context_budget_warning(59_999, 100_000).is_none()); // 59.999%
    }

    #[test]
    fn test_context_budget_warning_60_threshold() {
        reset_context_budget_warning();
        let warn = context_budget_warning(60_000, 100_000);
        assert!(warn.is_some(), "should warn at 60%");
        let msg = warn.unwrap();
        assert!(msg.contains("60% full"), "got: {msg}");
        assert!(msg.contains("/compact"), "got: {msg}");
    }

    #[test]
    fn test_context_budget_warning_80_threshold() {
        reset_context_budget_warning();
        let warn = context_budget_warning(80_000, 100_000);
        assert!(warn.is_some(), "should warn at 80%");
        let msg = warn.unwrap();
        assert!(msg.contains("80% full"), "got: {msg}");
        assert!(msg.contains("/compact"), "got: {msg}");
        assert!(msg.contains("/save"), "got: {msg}");
        assert!(msg.contains("/clear"), "got: {msg}");
    }

    #[test]
    fn test_context_budget_warning_90_threshold() {
        reset_context_budget_warning();
        let warn = context_budget_warning(90_000, 100_000);
        assert!(warn.is_some(), "should warn at 90%");
        let msg = warn.unwrap();
        assert!(msg.contains("90% full"), "got: {msg}");
        assert!(msg.contains("/save"), "got: {msg}");
        assert!(msg.contains("/clear"), "got: {msg}");
    }

    #[test]
    fn test_context_budget_warning_95_threshold() {
        reset_context_budget_warning();
        let warn = context_budget_warning(95_000, 100_000);
        assert!(warn.is_some(), "should warn at 95%");
        let msg = warn.unwrap();
        assert!(msg.contains("nearly full"), "got: {msg}");
        assert!(msg.contains("/clear"), "got: {msg}");
    }

    #[test]
    fn test_context_budget_warning_same_threshold_no_repeat() {
        reset_context_budget_warning();
        // First call at 60% should warn
        let first = context_budget_warning(60_000, 100_000);
        assert!(first.is_some(), "first call should warn");
        // Second call at same threshold should NOT warn
        let second = context_budget_warning(65_000, 100_000);
        assert!(second.is_none(), "same threshold should not repeat");
    }

    #[test]
    fn test_context_budget_warning_escalates() {
        reset_context_budget_warning();
        let w60 = context_budget_warning(60_000, 100_000);
        assert!(w60.is_some());
        // Jumping to 80% should warn again (higher threshold)
        let w80 = context_budget_warning(80_000, 100_000);
        assert!(w80.is_some(), "should warn at new higher threshold");
        assert!(w80.unwrap().contains("80% full"));
    }

    #[test]
    fn test_context_budget_warning_reset_rearms() {
        reset_context_budget_warning();
        let w1 = context_budget_warning(60_000, 100_000);
        assert!(w1.is_some());
        // Reset should allow the same threshold to warn again
        reset_context_budget_warning();
        let w2 = context_budget_warning(60_000, 100_000);
        assert!(w2.is_some(), "should warn again after reset");
    }

    #[test]
    fn test_context_budget_warning_zero_max_returns_none() {
        reset_context_budget_warning();
        assert!(context_budget_warning(100, 0).is_none());
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

    #[test]
    fn test_print_usage_quiet_suppressed() {
        // When quiet mode is active, print_usage should return early
        // without panicking. Since we can't easily capture stdout in
        // parallel tests, we verify the guard logic: is_quiet() gates
        // the function. enable_quiet() + print_usage() must not panic.
        enable_quiet();
        let usage = yoagent::Usage {
            input: 100,
            output: 50,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 150,
        };
        // This should be a no-op (early return) when quiet.
        print_usage(
            &usage,
            &usage,
            "test-model",
            std::time::Duration::from_secs(2),
        );
    }

    #[test]
    fn test_print_context_usage_quiet_suppressed() {
        // When quiet mode is active, print_context_usage should return early.
        enable_quiet();
        // This should be a no-op (early return) when quiet.
        print_context_usage(5000, 200_000);
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
        reset_shown_hints();
        // Multiple conditions true, but first_turn is highest priority
        let ctx = HintContext {
            turn_count: 1,
            files_modified: true,
            has_watch: false,
            had_tool_error: true,
            context_usage_ratio: 0.8,
            turns_since_slash_command: 5,
        };
        let hint = contextual_hint(&ctx);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("/help"));
    }
}
