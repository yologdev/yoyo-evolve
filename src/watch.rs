//! Watch mode — auto-run a test/lint command after agent edits.
//!
//! Extracted from `prompt.rs` (Day 58). The watch system lets users set a
//! command (e.g. `cargo test`) that runs automatically after each agent turn.
//! If the command fails, the agent gets a fix prompt and retries up to
//! [`MAX_WATCH_FIX_ATTEMPTS`] times.

use crate::commands_file::extract_file_paths_from_output;
use crate::commands_lint::{lint_command_for_project, test_command_for_project, LintStrictness};
use crate::commands_project::detect_project_type;
use crate::commands_risk::{format_risk_context, risk_context_for_files};
use crate::format::*;
use crate::memory::{auto_remember, build_fix_memory_note, build_learn_memory_note};
use crate::prompt::run_prompt_auto_retry;
use crate::prompt_budget::session_budget_exhausted;
use crate::session::SessionChanges;
use crate::sync_util::{rw_read_or_recover, rw_write_or_recover};
use std::io::{self, IsTerminal, Write};
use std::sync::RwLock;
use yoagent::agent::Agent;
use yoagent::*;

// Global state for `/watch` — auto-run a test command after agent edits.

/// The currently active watch commands (empty = watch mode off).
/// When multiple commands are stored, each is run as its own phase with
/// its own fix loop (e.g. lint → fix lint → test → fix test).
static WATCH_COMMANDS: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Set a single watch command, enabling watch mode.
/// This is the backward-compatible API — stores a single-element vec internally.
pub fn set_watch_command(cmd: &str) {
    let mut guard = rw_write_or_recover(&WATCH_COMMANDS);
    *guard = vec![cmd.to_string()];
}

/// Set multiple watch commands for multi-phase execution.
/// Each command runs as its own phase with its own fix loop.
/// For example: `["cargo clippy ...", "cargo test"]` runs lint first,
/// fixes lint errors, then runs tests, fixes test errors.
pub fn set_watch_commands(cmds: &[&str]) {
    let mut guard = rw_write_or_recover(&WATCH_COMMANDS);
    *guard = cmds.iter().map(|s| s.to_string()).collect();
}

/// Get the current watch command for display purposes.
/// If multiple commands are stored, returns them joined with ` && `.
/// Returns None if watch mode is off.
pub fn get_watch_command() -> Option<String> {
    let guard = rw_read_or_recover(&WATCH_COMMANDS);
    if guard.is_empty() {
        None
    } else {
        Some(guard.join(" && "))
    }
}

/// Get the individual watch commands (phases).
/// Returns an empty vec if watch mode is off.
pub fn get_watch_commands() -> Vec<String> {
    let guard = rw_read_or_recover(&WATCH_COMMANDS);
    guard.clone()
}

/// Clear the watch command, disabling watch mode.
pub fn clear_watch_command() {
    let mut guard = rw_write_or_recover(&WATCH_COMMANDS);
    *guard = Vec::new();
}

/// Maximum characters of watch command output to include in fix prompts.
const WATCH_OUTPUT_MAX: usize = 5000;

/// Maximum number of auto-fix attempts when watch mode detects failures.
pub const MAX_WATCH_FIX_ATTEMPTS: usize = 3;

/// Result from [`run_watch_after_prompt`] — carries pass/fail status plus
/// the last tool error from any auto-fix attempts (if the watch failed).
#[derive(Debug, Clone)]
pub struct WatchResult {
    /// Whether the watch command ultimately passed.
    pub passed: bool,
    /// The last tool error from auto-fix attempts, if any.
    pub last_tool_error: Option<String>,
}

// ---------------------------------------------------------------------------
// Structured Rust compiler error parsing
// ---------------------------------------------------------------------------

/// Category of a Rust compiler/clippy/test error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCategory {
    Borrow,
    Type,
    Lifetime,
    Import,
    Unused,
    Syntax,
    TestAssertion,
    Other,
}

impl ErrorCategory {
    /// Short label for display in summaries.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Borrow => "borrow",
            Self::Type => "type",
            Self::Lifetime => "lifetime",
            Self::Import => "import",
            Self::Unused => "unused",
            Self::Syntax => "syntax",
            Self::TestAssertion => "test_assertion",
            Self::Other => "other",
        }
    }
}

/// A single structured compiler error extracted from Rust toolchain output.
#[derive(Debug, Clone)]
pub struct CompilerError {
    /// The error code, e.g. `"E0382"`. `None` for unstructured errors.
    pub code: Option<String>,
    /// The primary error/warning message.
    pub message: String,
    /// Source file path, if found.
    pub file: Option<String>,
    /// Line number in the source file, if found.
    pub line: Option<u32>,
    /// Classified category for targeted hints.
    pub category: ErrorCategory,
}

/// Classify an error code (e.g. `"E0382"`) into an [`ErrorCategory`].
fn categorize_error_code(code: &str) -> ErrorCategory {
    match code {
        // Borrow checker errors
        "E0382" | "E0505" | "E0502" | "E0499" | "E0507" | "E0515" | "E0716" => {
            ErrorCategory::Borrow
        }
        // Type errors
        "E0308" | "E0277" | "E0271" | "E0369" | "E0609" | "E0614" | "E0618" => ErrorCategory::Type,
        // Lifetime errors
        "E0106" | "E0621" | "E0495" | "E0623" | "E0759" | "E0700" => ErrorCategory::Lifetime,
        // Import / path resolution
        "E0433" | "E0432" | "E0412" | "E0425" | "E0531" => ErrorCategory::Import,
        // Syntax errors
        "E0063" | "E0064" | "E0065" => ErrorCategory::Syntax,
        _ => ErrorCategory::Other,
    }
}

/// Classify an error/warning message into an [`ErrorCategory`] using text heuristics.
fn categorize_message(msg: &str) -> ErrorCategory {
    let lower = msg.to_lowercase();
    // Borrow checker
    if lower.contains("borrow")
        || lower.contains("moved value")
        || lower.contains("move out of")
        || lower.contains("cannot move")
        || lower.contains("does not live long enough")
    {
        return ErrorCategory::Borrow;
    }
    // Lifetime
    if lower.contains("lifetime")
        || lower.contains("missing lifetime")
        || lower.contains("outlives")
    {
        return ErrorCategory::Lifetime;
    }
    // Type
    if lower.contains("mismatched types")
        || lower.contains("type mismatch")
        || lower.contains("expected type")
        || lower.contains("the trait bound")
        || lower.contains("doesn't implement")
        || lower.contains("no method named")
        || lower.contains("no field")
    {
        return ErrorCategory::Type;
    }
    // Import / path
    if lower.contains("cannot find")
        || lower.contains("unresolved import")
        || lower.contains("not found in")
        || lower.contains("no external crate")
    {
        return ErrorCategory::Import;
    }
    // Unused
    if lower.contains("unused") {
        return ErrorCategory::Unused;
    }
    // Test assertion (panic)
    if lower.contains("panicked at")
        || lower.contains("assertion")
        || lower.contains("thread '") && (lower.contains("failed") || lower.contains("panicked"))
    {
        return ErrorCategory::TestAssertion;
    }
    // Syntax
    if lower.contains("expected")
        && (lower.contains("found `")
            || lower.contains("found `")
            || lower.contains("unexpected token"))
    {
        return ErrorCategory::Syntax;
    }
    ErrorCategory::Other
}

/// Strip ANSI escape sequences from a single line of tool output.
///
/// **Why this exists (#859).** [`parse_rust_errors`] anchors two of its three
/// patterns with `strip_prefix("error[")` / `strip_prefix("error: ")`. An escape
/// sequence in front of the word `error` defeats an anchor where a `contains()`
/// would survive it — so a user with `color = "always"` in `.cargo/config.toml`,
/// or `CARGO_TERM_COLOR=always` in CI, got **zero** parsed compiler errors and a
/// fix prompt with no `--> file:line`, no source context and no category hint.
/// The asymmetry is what made it confusing rather than merely broken: pattern 3
/// (test panics) uses `contains()`, so coloured *panics* parsed while coloured
/// *compile errors* did not.
///
/// **Scope, measured rather than assumed** (cargo 1.98.0 / rustc 1.98.0,
/// `CARGO_TERM_COLOR=always cargo build 2>&1 | cat > file`, a deliberate
/// `E0308`): the capture carried **49 escape bytes, all 49 of them CSI**
/// (`ESC [` … final byte in `0x40..=0x7E`, i.e. the SGR colour form), **zero**
/// OSC (`ESC ]`) and zero other `ESC` forms — and CSI-stripping the coloured
/// capture produced output **byte-identical** to the `CARGO_TERM_COLOR=never`
/// twin. So this handles CSI and nothing else.
///
/// **What it deliberately does NOT handle, stated rather than implied:** OSC-8
/// terminal hyperlinks (`ESC ]8;;…`) are **not** stripped, because they were not
/// observed — newer rustc may wrap error codes in them, and a branch no fixture
/// exercises is a claim no test can grade. If they ever appear, an `ESC ]` will
/// survive into the message; it will not panic and it will not swallow the line.
///
/// **The sibling parsers are NOT swept, deliberately (#861).**
/// [`parse_typescript_errors`] and [`parse_python_errors`] anchor the same way
/// (`strip_prefix`/`starts_with` on a trimmed line), so they are structurally
/// exposed to the same mechanism — but that is read off the source, **not
/// observed**: no capture of `tsc`, `eslint`, `pytest` or `mypy` output exists,
/// and several tools disable colour on a pipe by default, which is exactly why
/// it is worth checking rather than assuming. A sweep would transfer the fix and
/// silently drop the burden of proof, so the gap is filed as #861 instead.
///
/// An **unterminated** CSI at end of line (no final byte) drops the escape
/// introducer and keeps the remaining bytes, rather than swallowing the rest of
/// the line silently — a truncated capture should lose the escape, not the text.
///
/// A line with no `ESC` is returned **borrowed and byte-identical**, so the
/// common path — every user who has never set `color = "always"` — allocates
/// nothing and parses exactly as it did before.
///
/// No `&str` is ever indexed at a byte offset that could fall inside a `char`
/// (rule #250): the scan walks `char_indices`, and every slice boundary is
/// either a `char` boundary reported by that iterator or the position just past
/// an ASCII escape byte, which is a `char` boundary by construction.
pub fn strip_ansi_escapes(line: &str) -> std::borrow::Cow<'_, str> {
    if !line.contains('\u{1b}') {
        return std::borrow::Cow::Borrowed(line);
    }

    let mut out = String::with_capacity(line.len());
    let mut chars = line.char_indices();
    while let Some((_, ch)) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        // ESC seen. A CSI is `ESC [` <params> <final byte 0x40..=0x7E>.
        match chars.next() {
            Some((_, '[')) => {
                // Consume up to and including the final byte. If the line ends
                // first the sequence is unterminated: we have already dropped
                // `ESC [` and simply keep whatever followed.
                for (_, c) in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            // Any other ESC form (including OSC `ESC ]`) is not a class we
            // observed; drop the ESC itself and keep the rest verbatim.
            Some((_, other)) => out.push(other),
            None => {}
        }
    }
    std::borrow::Cow::Owned(out)
}

/// Parse Rust compiler/clippy/test output into structured [`CompilerError`]s.
///
/// Recognises patterns like:
/// - `error[E0382]: borrow of moved value: \`x\``
/// - `error: cannot find value \`foo\``
/// - `warning: unused import: \`std::io\``
/// - `thread 'test_name' panicked at 'assertion failed: ...'`
///
/// File locations are extracted from the ` --> path:line:col` lines that
/// follow each diagnostic.
pub fn parse_rust_errors(output: &str) -> Vec<CompilerError> {
    let mut errors: Vec<CompilerError> = Vec::new();
    // #859: strip ANSI **once**, here, so all three patterns and every helper
    // that reads `lines` (notably `extract_location`, whose `--> ` anchor is
    // equally defeated by a colour code) inherit it — never per-pattern.
    // `trim_start` rides along because the anchors below are not merely
    // uncoloured-only, they are column-0-only; an indented diagnostic is cheap
    // to survive. On uncoloured input every `Cow` is `Borrowed`, so this is
    // byte-identical to the previous `output.lines().collect()`.
    let stripped: Vec<std::borrow::Cow<'_, str>> = output.lines().map(strip_ansi_escapes).collect();
    let lines: Vec<&str> = stripped.iter().map(|c| c.as_ref().trim_start()).collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        // Pattern 1: `error[EXXXX]: message` or `warning[...]: message`
        if let Some(rest) = line
            .strip_prefix("error[")
            .or_else(|| line.strip_prefix("warning["))
        {
            if let Some(bracket_end) = rest.find(']') {
                let code = &rest[..bracket_end];
                let msg = rest[bracket_end + 1..].trim_start_matches(':').trim();
                let is_warning = line.starts_with("warning");

                let category = if is_warning && msg.to_lowercase().contains("unused") {
                    ErrorCategory::Unused
                } else {
                    let cat = categorize_error_code(code);
                    if cat == ErrorCategory::Other {
                        categorize_message(msg)
                    } else {
                        cat
                    }
                };

                let (file, file_line) = extract_location(&lines, i);

                errors.push(CompilerError {
                    code: Some(code.to_string()),
                    message: msg.to_string(),
                    file,
                    line: file_line,
                    category,
                });
            }
        }
        // Pattern 2: `error: message` (no code) or `warning: message` (no code)
        else if let Some(msg) = line
            .strip_prefix("error: ")
            .or_else(|| line.strip_prefix("warning: "))
        {
            // Skip aborting lines and cargo summary lines
            let lower = msg.to_lowercase();
            if !lower.starts_with("aborting")
                && !lower.starts_with("could not compile")
                && !lower.contains("generated")
                && !lower.starts_with("build failed")
            {
                let is_warning = line.starts_with("warning");
                let category = if is_warning && lower.contains("unused") {
                    ErrorCategory::Unused
                } else {
                    categorize_message(msg)
                };

                let (file, file_line) = extract_location(&lines, i);

                errors.push(CompilerError {
                    code: None,
                    message: msg.to_string(),
                    file,
                    line: file_line,
                    category,
                });
            }
        }
        // Pattern 3: `thread 'test_name' panicked at ...`
        else if line.contains("thread '") && line.contains("panicked at") {
            let (file, file_line) = match modern_panic_location(line) {
                Some((f, l)) => (Some(f), Some(l)),
                None => (None, None),
            };

            // A modern panic header carries only the location; the assertion
            // itself is on the FOLLOWING line. Without it the fix agent is told
            // that a test panicked and not what the assertion was.
            let mut message = line.trim().to_string();
            if file.is_some() {
                if let Some(detail) = lines.get(i + 1) {
                    let detail = detail.trim();
                    if !detail.is_empty() && !detail.starts_with("note:") {
                        message.push(' ');
                        message.push_str(detail);
                    }
                }
            }

            errors.push(CompilerError {
                code: None,
                message,
                file,
                line: file_line,
                category: ErrorCategory::TestAssertion,
            });
        }

        i += 1;
    }

    errors
}

/// Extract `(file, line)` from the location tail of a **modern** (Rust 1.65+)
/// panic header, e.g. `thread 'tests::t' (4861) panicked at src/main.rs:7:9:`.
///
/// Returns `None` for the pre-1.65 shape
/// (`thread 'x' panicked at 'assertion failed', src/lib.rs:99:9`), which carries
/// **no trailing colon**. That discriminator is the whole reason the legacy path
/// stays byte-identical: the old format put the message in quotes and the
/// location last, the new one puts the location in the header and the message on
/// the next line.
///
/// Both the line and the column must parse as integers, so a message that merely
/// happens to end in `:` cannot be mistaken for a location.
fn modern_panic_location(line: &str) -> Option<(String, u32)> {
    let rest = line.trim_end().strip_suffix(':')?;
    let tail = rest.rsplit_once("panicked at ")?.1.trim();
    // A quoted tail is the legacy shape's message, not a path.
    if tail.is_empty() || tail.starts_with('\'') {
        return None;
    }
    let mut parts = tail.rsplitn(3, ':');
    parts.next()?.parse::<u32>().ok()?; // column
    let line_no = parts.next()?.parse::<u32>().ok()?;
    let file = parts.next()?;
    if file.is_empty() {
        return None;
    }
    Some((file.to_string(), line_no))
}

/// Does this line begin a **new** diagnostic?
///
/// #860: the bound for [`extract_location`]'s lookahead. rustc emits one
/// diagnostic per header, and a genuine diagnostic's own `-->` always precedes
/// the next header — so anything at or past the next header belongs to that
/// *next* diagnostic, never to this one.
///
/// The three shapes are the same three [`parse_rust_errors`] itself anchors on
/// (`error…`, `warning…`, `thread '…' panicked at …`), so a line that would
/// start a new entry also stops the previous entry's search. `error` and
/// `warning` are matched as bare prefixes rather than requiring `: ` or `[`,
/// because both the coded (`error[E0308]:`) and uncoded (`error:`) forms must
/// bound, as must cargo's own `warning: \`crate\` (manifest) generated …`
/// summary lines, which carry no location and sit between the two.
///
/// Input is already ANSI-stripped and `trim_start`ed by `parse_rust_errors`
/// (#859), so this needs no second matcher — without that, the bound would
/// silently never fire for anyone running `color = "always"`.
fn begins_new_diagnostic(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("error") || t.starts_with("warning") || t.starts_with("thread '")
}

/// Find the `--> path:line:col` belonging to the diagnostic whose header is at
/// `start`, searching the next 5 lines.
///
/// #860: the search **stops at the next diagnostic header**. Without that
/// bound a diagnostic carrying no location of its own — a manifest warning, a
/// link failure, `error[E0463]: can't find crate` — absorbed the location of
/// whichever diagnostic happened to follow it within the window, and reported
/// it *confidently*. That is worse than `None`: `build_watch_fix_prompt` feeds
/// `file`/`line` to `extract_error_source_context`, so the fix agent was handed
/// a quoted excerpt of innocent code.
///
/// **The bound is a pure narrowing, and that is why it is safe.** It can only
/// turn a *wrong* `Some(location)` into `None`; it can never invent a location,
/// and it can never move one that was already correct, because a genuine
/// diagnostic's own `-->` always precedes the next header. Please do not
/// "simplify" the bound away — it fires on real, ordinary cargo output (any
/// typo'd manifest key beside any type error; see the #860 test).
fn extract_location(lines: &[&str], start: usize) -> (Option<String>, Option<u32>) {
    let end = std::cmp::min(start + 6, lines.len());
    for line in &lines[start + 1..end] {
        let trimmed = line.trim();
        // #860: past this point we are reading someone else's diagnostic.
        if begins_new_diagnostic(trimmed) {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("--> ") {
            // Format: path:line:col
            let parts: Vec<&str> = rest.rsplitn(3, ':').collect();
            if parts.len() >= 3 {
                let file = parts[2].to_string();
                let line_num = parts[1].parse::<u32>().ok();
                return (Some(file), line_num);
            } else if parts.len() == 2 {
                let file = parts[1].to_string();
                let line_num = parts[0].parse::<u32>().ok();
                return (Some(file), line_num);
            }
        }
    }
    (None, None)
}

// ---------------------------------------------------------------------------
// TypeScript error parser
// ---------------------------------------------------------------------------

/// Classify a TypeScript error code (e.g. `"TS2345"`) into an [`ErrorCategory`].
fn categorize_ts_code(code: &str) -> ErrorCategory {
    // Strip optional "TS" prefix to get the numeric part
    let num_str = code.strip_prefix("TS").unwrap_or(code);
    if let Ok(num) = num_str.parse::<u32>() {
        match num {
            // Type errors: TS2xxx family (common type mismatches)
            2000..=2999 => {
                match num {
                    // Import / module resolution errors within the 2xxx range
                    2307 | 2305 | 2306 | 2314 | 2315 => ErrorCategory::Import,
                    _ => ErrorCategory::Type,
                }
            }
            // Syntax errors: TS1xxx family
            1000..=1999 => ErrorCategory::Syntax,
            // Unused variable/parameter: TS6133
            6133 => ErrorCategory::Unused,
            _ => ErrorCategory::Other,
        }
    } else {
        ErrorCategory::Other
    }
}

/// Classify a TypeScript/eslint error message using text heuristics.
fn categorize_ts_message(msg: &str) -> ErrorCategory {
    let lower = msg.to_lowercase();
    if lower.contains("cannot find module")
        || lower.contains("has no exported member")
        || lower.contains("module not found")
        || lower.contains("unable to resolve")
    {
        return ErrorCategory::Import;
    }
    if lower.contains("type") && (lower.contains("not assignable") || lower.contains("mismatch")) {
        return ErrorCategory::Type;
    }
    if lower.contains("no-unused-vars")
        || lower.contains("is declared but")
        || lower.contains("is defined but never used")
    {
        return ErrorCategory::Unused;
    }
    if lower.contains("parsing error")
        || lower.contains("unexpected token")
        || lower.contains("expression expected")
    {
        return ErrorCategory::Syntax;
    }
    if lower.contains("assertion") || lower.contains("expect(") || lower.contains("test failed") {
        return ErrorCategory::TestAssertion;
    }
    ErrorCategory::Other
}

/// Parse TypeScript compiler (`tsc`) and eslint output into structured [`CompilerError`]s.
///
/// Handles two main formats:
/// - **tsc**: `src/file.ts(line,col): error TS2345: message`
/// - **eslint**: `src/file.ts:line:col: error message [rule-name]`
/// - **jest/vitest**: `FAIL src/file.test.ts` + assertion messages
///
/// **NOT swept for ANSI (#859), deliberately — see follow-up issue.** The Rust
/// parser strips CSI escapes because a coloured `error[` defeated its anchored
/// `strip_prefix`; this parser is left alone because it reads a *different
/// tool's* output and no capture of that tool's coloured form was taken. A
/// sweep transfers the fix and silently drops the burden of proof, so the gap
/// is named here and filed rather than guessed at.
pub fn parse_typescript_errors(output: &str) -> Vec<CompilerError> {
    let mut errors: Vec<CompilerError> = Vec::new();

    for raw_line in output.lines() {
        // #861: strip ANSI *before* any anchor below runs. `tsc` emits CSI
        // colour codes whenever it believes it is talking to a terminal —
        // `"pretty": true` in tsconfig.json, `FORCE_COLOR`, or a real TTY —
        // and an escape sitting in front of an anchored token defeats every
        // `strip_prefix` / `starts_with` / `find` in this loop. Applied at
        // this ONE site rather than per-pattern so every helper reading these
        // lines inherits it: a per-pattern fix restores the error and still
        // loses the `file`/`line`, because the location anchors are defeated
        // by exactly the same mechanism. This is #859's reasoning verbatim,
        // one parser over. A line with no ESC returns `Cow::Borrowed` and is
        // byte-identical, which is the entire regression surface.
        let stripped_line = strip_ansi_escapes(raw_line);
        let trimmed = stripped_line.trim();

        // Pattern 1: tsc format — `path(line,col): error TSxxxx: message`
        // Also matches: `path(line,col): warning TSxxxx: message`
        if let Some(paren_pos) = trimmed.find('(') {
            if let Some(paren_end) = trimmed[paren_pos..].find(')') {
                let after_paren = &trimmed[paren_pos + paren_end + 1..];
                // Check for ": error TS" or ": warning TS" pattern
                if let Some(rest) = after_paren
                    .strip_prefix(": error ")
                    .or_else(|| after_paren.strip_prefix(": warning "))
                {
                    // Try to extract TSxxxx code
                    if let Some(colon) = rest.find(": ") {
                        let code_part = &rest[..colon];
                        if code_part.starts_with("TS") {
                            let msg = rest[colon + 2..].trim();
                            let file = trimmed[..paren_pos].to_string();
                            let loc_str = &trimmed[paren_pos + 1..paren_pos + paren_end];
                            let line_num = loc_str
                                .split(',')
                                .next()
                                .and_then(|s| s.parse::<u32>().ok());

                            let category = {
                                let cat = categorize_ts_code(code_part);
                                if cat == ErrorCategory::Other {
                                    categorize_ts_message(msg)
                                } else {
                                    cat
                                }
                            };

                            errors.push(CompilerError {
                                code: Some(code_part.to_string()),
                                message: msg.to_string(),
                                file: Some(file),
                                line: line_num,
                                category,
                            });
                            continue;
                        }
                    }
                }
            }
        }

        // Pattern 1b: tsc --pretty format — `path:line:col - error TSxxxx: message`
        // Structurally different from Pattern 1: the location is
        // colon-separated rather than parenthesised, and the separator before
        // the severity word is ` - `, not `): `. This is what a user gets with
        // `"pretty": true` in tsconfig.json (a common setting), with
        // FORCE_COLOR set, or on a real TTY. Measured Day 182: stripping ANSI
        // alone still yielded ZERO errors from this shape, so the escapes and
        // the line shape are two independent defects and both are needed.
        if let Some((sep_pos, sep)) = trimmed
            .find(" - error ")
            .map(|i| (i, " - error "))
            .or_else(|| trimmed.find(" - warning ").map(|i| (i, " - warning ")))
        {
            let loc = &trimmed[..sep_pos];
            let rest = &trimmed[sep_pos + sep.len()..];
            if let Some(colon) = rest.find(": ") {
                let code_part = &rest[..colon];
                if code_part.starts_with("TS") {
                    let msg = rest[colon + 2..].trim();
                    // `file:line:col` — split from the RIGHT so a Windows
                    // drive letter (`C:\src\app.ts:1:7`) keeps its own colon.
                    let mut loc_parts = loc.rsplitn(3, ':');
                    let col = loc_parts.next().and_then(|s| s.trim().parse::<u32>().ok());
                    let line_num = loc_parts.next().and_then(|s| s.trim().parse::<u32>().ok());
                    let file = loc_parts.next().map(|s| s.trim().to_string());
                    // Require BOTH numbers to parse: that is what stops an
                    // ordinary prose line containing " - error TS" from being
                    // mistaken for a diagnostic.
                    if let (Some(_), Some(line_num), Some(file)) = (col, line_num, file) {
                        if !file.is_empty() {
                            let category = {
                                let cat = categorize_ts_code(code_part);
                                if cat == ErrorCategory::Other {
                                    categorize_ts_message(msg)
                                } else {
                                    cat
                                }
                            };

                            errors.push(CompilerError {
                                code: Some(code_part.to_string()),
                                message: msg.to_string(),
                                file: Some(file),
                                line: Some(line_num),
                                category,
                            });
                            continue;
                        }
                    }
                }
            }
        }

        // Pattern 2: eslint format — `path:line:col: error message`
        // Also: `path:line:col: warning message`
        // eslint often ends with a rule name in parens or brackets
        {
            let parts: Vec<&str> = trimmed.splitn(4, ':').collect();
            if parts.len() == 4 {
                // parts[0]=file, parts[1]=line, parts[2]=col, parts[3]=" error msg"
                if let (Ok(line_num), Ok(_col)) = (
                    parts[1].trim().parse::<u32>(),
                    parts[2].trim().parse::<u32>(),
                ) {
                    let rest = parts[3].trim();
                    if let Some(msg) = rest
                        .strip_prefix("error ")
                        .or_else(|| rest.strip_prefix("warning "))
                    {
                        let msg = msg.trim();
                        let file = parts[0].to_string();
                        // Check if there's a rule name like [no-unused-vars]
                        let category = categorize_ts_message(msg);
                        errors.push(CompilerError {
                            code: None,
                            message: msg.to_string(),
                            file: Some(file),
                            line: Some(line_num),
                            category,
                        });
                        continue;
                    }
                }
            }
        }

        // Pattern 3: jest/vitest failure line — `FAIL src/file.test.ts`
        if let Some(file) = trimmed.strip_prefix("FAIL ") {
            let file = file.trim();
            if !file.is_empty() {
                errors.push(CompilerError {
                    code: None,
                    message: format!("Test suite failed: {file}"),
                    file: Some(file.to_string()),
                    line: None,
                    category: ErrorCategory::TestAssertion,
                });
            }
        }

        // Pattern 4: jest/vitest assertion — `expect(received).toEqual(expected)`
        // or `● test name` (jest test name indicator)
        if trimmed.starts_with("● ") || trimmed.starts_with("× ") {
            errors.push(CompilerError {
                code: None,
                message: trimmed.to_string(),
                file: None,
                line: None,
                category: ErrorCategory::TestAssertion,
            });
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Python error parser
// ---------------------------------------------------------------------------

/// Classify a Python error message using text heuristics.
fn categorize_python_message(msg: &str) -> ErrorCategory {
    let lower = msg.to_lowercase();
    if lower.contains("incompatible type")
        || lower.contains("has no attribute")
        || lower.contains("unexpected type")
        || lower.contains("invalid type")
        || (lower.contains("error:") && lower.contains("type"))
    {
        return ErrorCategory::Type;
    }
    if lower.contains("modulenotfounderror")
        || lower.contains("importerror")
        || lower.contains("no module named")
        || lower.contains("cannot find implementation")
    {
        return ErrorCategory::Import;
    }
    if lower.contains("syntaxerror")
        || lower.contains("indentationerror")
        || lower.contains("unexpected indent")
        || lower.contains("invalid syntax")
    {
        return ErrorCategory::Syntax;
    }
    if lower.contains("assertionerror")
        || lower.contains("assert ")
        || lower.contains("failed")
        || lower.contains("assertion")
    {
        return ErrorCategory::TestAssertion;
    }
    ErrorCategory::Other
}

/// Parse Python tool output (pytest, mypy, tracebacks) into structured [`CompilerError`]s.
///
/// Handles three main formats:
/// - **pytest**: `FAILED tests/test_foo.py::test_bar - ErrorType: message`
/// - **mypy**: `src/foo.py:42: error: message`
/// - **traceback**: `File "foo.py", line 42, in func_name` (extracts last frame)
///
/// **NOT swept for ANSI (#859), deliberately — see follow-up issue.** The Rust
/// parser strips CSI escapes because a coloured `error[` defeated its anchored
/// `strip_prefix`; this parser is left alone because it reads a *different
/// tool's* output and no capture of that tool's coloured form was taken. A
/// sweep transfers the fix and silently drops the burden of proof, so the gap
/// is named here and filed rather than guessed at.
pub fn parse_python_errors(output: &str) -> Vec<CompilerError> {
    let mut errors: Vec<CompilerError> = Vec::new();
    let lines: Vec<&str> = output.lines().collect();

    // Track the last traceback frame so we can attach it to the error line
    let mut last_tb_file: Option<String> = None;
    let mut last_tb_line: Option<u32> = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Pattern 1: pytest — `FAILED tests/test_foo.py::test_bar - ErrorType: message`
        if trimmed.starts_with("FAILED ") {
            let rest = trimmed.strip_prefix("FAILED ").unwrap();
            let (test_path, msg) = if let Some(dash_pos) = rest.find(" - ") {
                (&rest[..dash_pos], rest[dash_pos + 3..].trim())
            } else {
                (rest.trim(), rest.trim())
            };
            // Extract file from `path::test_name`
            let file = test_path.split("::").next().map(|s| s.to_string());
            let category = categorize_python_message(msg);
            errors.push(CompilerError {
                code: None,
                message: msg.to_string(),
                file,
                line: None,
                category,
            });
            continue;
        }

        // Pattern 2: mypy — `src/foo.py:42: error: message` or `src/foo.py:42: note: ...`
        // Also matches: `src/foo.py:42:10: error: message` (with column)
        {
            // Try splitting into at least 3 parts: file, line, rest
            let parts: Vec<&str> = trimmed.splitn(3, ':').collect();
            if parts.len() >= 3 && parts[0].ends_with(".py") {
                if let Ok(line_num) = parts[1].trim().parse::<u32>() {
                    let rest = parts[2..].join(":");
                    // Check for " error: " or "error: " mypy prefixes (before/after trim)
                    let msg_opt = rest
                        .strip_prefix(" error: ")
                        .or_else(|| rest.trim().strip_prefix("error: "))
                        .or_else(|| {
                            // Handle `col: error: msg` format (e.g. `10: error: msg`)
                            if let Some(colon_pos) = rest.find(": error: ") {
                                Some(&rest[colon_pos + 9..])
                            } else {
                                None
                            }
                        });
                    if let Some(msg) = msg_opt {
                        let msg = msg.trim();
                        let category = categorize_python_message(msg);
                        errors.push(CompilerError {
                            code: None,
                            message: msg.to_string(),
                            file: Some(parts[0].to_string()),
                            line: Some(line_num),
                            category,
                        });
                        continue;
                    }
                }
            }
        }

        // Pattern 3: Python traceback — `File "foo.py", line 42, in func_name`
        if let Some(after_prefix) = trimmed.strip_prefix("File \"") {
            if let Some(quote_end) = after_prefix.find('"') {
                let file = after_prefix[..quote_end].to_string();
                // Extract line number: ", line 42"
                let after_file = &after_prefix[quote_end + 1..];
                if let Some(line_start) = after_file.find("line ") {
                    let line_str = &after_file[line_start + 5..];
                    let line_num = line_str
                        .split(|c: char| !c.is_ascii_digit())
                        .next()
                        .and_then(|s| s.parse::<u32>().ok());
                    last_tb_file = Some(file);
                    last_tb_line = line_num;
                }
            }
            continue;
        }

        // Pattern 4: Error line following a traceback — `ErrorType: message`
        // Only capture if we have a preceding traceback frame
        if last_tb_file.is_some() && !trimmed.is_empty() && !trimmed.starts_with("File ") {
            // Check if it looks like an error: `SomeError: message` or `SomeError(message)`
            if let Some(colon_pos) = trimmed.find(": ") {
                let error_type = &trimmed[..colon_pos];
                // Error type names are typically CamelCase and end with Error/Exception
                if error_type.chars().next().is_some_and(|c| c.is_uppercase())
                    && !error_type.contains(' ')
                {
                    let category = categorize_python_message(trimmed);
                    errors.push(CompilerError {
                        code: None,
                        message: trimmed.to_string(),
                        file: last_tb_file.take(),
                        line: last_tb_line.take(),
                        category,
                    });
                    continue;
                }
            }
        }

        // Pattern 5: Short summary lines — `1 failed, 2 passed` (pytest summary)
        // Also: `E   AssertionError: ...` (pytest assertion detail)
        if trimmed.starts_with("E   ") || trimmed.starts_with("E\t") {
            let msg = trimmed[1..].trim();
            if !msg.is_empty() {
                let category = categorize_python_message(msg);
                errors.push(CompilerError {
                    code: None,
                    message: msg.to_string(),
                    file: None,
                    line: None,
                    category,
                });
            }
            continue;
        }

        // Reset traceback state on blank lines
        if trimmed.is_empty() {
            last_tb_file = None;
            last_tb_line = None;
        }

        // Suppress unused-variable warning for loop index
        let _ = i;
    }

    errors
}

// ---------------------------------------------------------------------------
// Language detection for watch commands
// ---------------------------------------------------------------------------

/// Detected language from a watch command, used to select the right error parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchLanguage {
    Rust,
    TypeScript,
    Python,
    Unknown,
}

/// Detect the language associated with a watch command.
fn detect_watch_language(watch_cmd: &str) -> WatchLanguage {
    let lower = watch_cmd.to_lowercase();
    // Rust
    if lower.contains("cargo") || lower.contains("rustc") {
        return WatchLanguage::Rust;
    }
    // TypeScript / JavaScript
    if lower.contains("tsc")
        || lower.contains("npm ")
        || lower.contains("npx ")
        || lower.contains("eslint")
        || lower.contains("jest")
        || lower.contains("vitest")
        || lower.contains("yarn ")
        || lower.contains("pnpm ")
        || lower.contains("node ")
        || lower.contains("bun ")
    {
        return WatchLanguage::TypeScript;
    }
    // Python
    if lower.contains("pytest")
        || lower.contains("python")
        || lower.contains("mypy")
        || lower.contains("ruff")
        || lower.contains("flake8")
        || lower.contains("pylint")
        || lower.contains("pyright")
    {
        return WatchLanguage::Python;
    }
    WatchLanguage::Unknown
}

/// Return a targeted fix hint for a given error category.
pub fn error_category_hint(category: &ErrorCategory) -> &'static str {
    match category {
        ErrorCategory::Borrow => {
            "This is a borrow checker error. Consider cloning the value, \
             restructuring ownership, or using references."
        }
        ErrorCategory::Type => {
            "This is a type mismatch. Check the expected vs actual types, \
             consider conversions or generics."
        }
        ErrorCategory::Lifetime => {
            "This is a lifetime error. Consider adding explicit lifetime \
             annotations or restructuring borrows."
        }
        ErrorCategory::Import => {
            "Missing import or unresolved name. Add the missing `use` statement \
             or check the module path."
        }
        ErrorCategory::Unused => {
            "Unused code warnings. Remove the unused items or prefix with \
             underscore if intentionally unused."
        }
        ErrorCategory::TestAssertion => {
            "Test assertion failed. Read the expected vs actual values, fix the \
             implementation or update the test."
        }
        ErrorCategory::Syntax => {
            "Syntax error. Check for missing brackets, semicolons, or incorrect token usage."
        }
        ErrorCategory::Other => "Read the error messages carefully and apply targeted fixes.",
    }
}

/// Return a targeted fix hint for TypeScript errors.
fn ts_error_category_hint(category: &ErrorCategory) -> &'static str {
    match category {
        ErrorCategory::Type => {
            "This is a TypeScript type error. Check the expected vs actual types, \
             consider type assertions, generics, or updating interface definitions."
        }
        ErrorCategory::Import => {
            "Missing module or export. Run `npm install` if the package is missing, \
             check import paths, or add type declarations for untyped modules."
        }
        ErrorCategory::Unused => {
            "Unused variable/import warnings. Remove the unused items or prefix with \
             underscore (e.g. `_unusedVar`)."
        }
        ErrorCategory::Syntax => {
            "TypeScript syntax error. Check for missing brackets, semicolons, \
             or incorrect JSX/TSX syntax."
        }
        ErrorCategory::TestAssertion => {
            "Test assertion failed. Read the expected vs actual values in the \
             jest/vitest output and fix the implementation or update the test."
        }
        _ => "Read the error messages carefully and apply targeted fixes.",
    }
}

/// Return a targeted fix hint for Python errors.
fn python_error_category_hint(category: &ErrorCategory) -> &'static str {
    match category {
        ErrorCategory::Type => {
            "This is a type error (mypy/pyright). Check the expected vs actual types, \
             add type annotations, or use `cast()` / `# type: ignore` if correct."
        }
        ErrorCategory::Import => {
            "Missing module or import. Run `pip install` for missing packages, \
             check import paths, or fix circular imports."
        }
        ErrorCategory::Syntax => {
            "Python syntax error. Check for missing colons, incorrect indentation, \
             unclosed brackets, or invalid syntax."
        }
        ErrorCategory::TestAssertion => {
            "Test assertion failed. Read the expected vs actual values in the \
             pytest output and fix the implementation or update the test."
        }
        ErrorCategory::Unused => {
            "Unused import or variable. Remove the unused items or use the `noqa` \
             comment to suppress if intentional."
        }
        _ => "Read the error messages carefully and apply targeted fixes.",
    }
}

/// Return the appropriate hint function for a detected language.
fn hint_for_language(lang: WatchLanguage) -> fn(&ErrorCategory) -> &'static str {
    match lang {
        WatchLanguage::Rust => error_category_hint,
        WatchLanguage::TypeScript => ts_error_category_hint,
        WatchLanguage::Python => python_error_category_hint,
        WatchLanguage::Unknown => error_category_hint,
    }
}

/// Language label for error summary display.
fn language_label(lang: WatchLanguage) -> &'static str {
    match lang {
        WatchLanguage::Rust => "Rust",
        WatchLanguage::TypeScript => "TypeScript",
        WatchLanguage::Python => "Python",
        WatchLanguage::Unknown => "Rust",
    }
}

/// Build a structured error summary from parsed compiler/tool output.
///
/// Returns `None` if no errors were parsed (output falls through to generic prompt).
fn build_error_summary_for(errors: &[CompilerError], lang: WatchLanguage) -> Option<String> {
    if errors.is_empty() {
        return None;
    }

    // Count by category
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for e in errors {
        *counts.entry(e.category.label()).or_insert(0) += 1;
    }

    // Build summary line
    let total = errors.len();
    let mut parts: Vec<String> = Vec::new();
    // Sort by count descending for readability
    let mut sorted: Vec<(&&str, &usize)> = counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (cat, count) in &sorted {
        parts.push(format!("{count} {cat}"));
    }
    let lang_name = language_label(lang);
    let summary_line = format!(
        "**Parsed {total} {lang_name} error(s):** {}",
        parts.join(", ")
    );

    // Find dominant category (highest count)
    let dominant = sorted.first().map(|(cat, _)| **cat).unwrap_or("other");
    let dominant_category = errors
        .iter()
        .find(|e| e.category.label() == dominant)
        .map(|e| &e.category)
        .unwrap_or(&ErrorCategory::Other);

    let hint_fn = hint_for_language(lang);
    let hint = hint_fn(dominant_category);

    // Show up to 5 specific errors with file locations
    let mut detail_lines: Vec<String> = Vec::new();
    for (idx, e) in errors.iter().take(5).enumerate() {
        let code_str = e
            .code
            .as_ref()
            .map(|c| format!("[{c}] "))
            .unwrap_or_default();
        let loc = match (&e.file, e.line) {
            (Some(f), Some(l)) => format!(" at {f}:{l}"),
            (Some(f), None) => format!(" in {f}"),
            _ => String::new(),
        };
        detail_lines.push(format!("  {}. {code_str}{}{loc}", idx + 1, e.message));
    }
    if errors.len() > 5 {
        detail_lines.push(format!("  ... and {} more", errors.len() - 5));
    }

    Some(format!(
        "{summary_line}\n{}\n\n**Hint:** {hint}",
        detail_lines.join("\n")
    ))
}

/// Classify a watch command as "lint", "test", or "command" for fix prompt hints.
fn classify_watch_command(cmd: &str) -> &'static str {
    let lower = cmd.to_lowercase();
    // Check for lint-like commands
    if lower.contains("clippy")
        || lower.contains("eslint")
        || lower.contains("pylint")
        || lower.contains("flake8")
        || lower.contains("ruff")
        || lower.contains("golint")
        || lower.contains("lint")
    {
        "lint"
    // Check for test-like commands
    } else if lower.contains("test")
        || lower.contains("pytest")
        || lower.contains("jest")
        || lower.contains("vitest")
        || lower.contains("mocha")
    {
        "test"
    } else {
        "command"
    }
}

/// Maximum total bytes of source context to inject into fix prompts.
const SOURCE_CONTEXT_MAX_BYTES: usize = 3072;

/// Extract relevant source file lines around error locations.
///
/// For each unique `(file, line)` pair in the error list, reads the file and
/// extracts a window of context (±5 lines). Results are deduped by file+line,
/// capped at [`SOURCE_CONTEXT_MAX_BYTES`] total, and formatted as markdown.
///
/// Files that don't exist or can't be read are silently skipped.
pub fn extract_error_source_context(errors: &[CompilerError]) -> String {
    use std::collections::HashSet;
    use std::fs;

    if errors.is_empty() {
        return String::new();
    }

    // Deduplicate (file, line) pairs — preserve insertion order via Vec
    let mut seen = HashSet::new();
    let mut locations: Vec<(String, u32)> = Vec::new();
    for e in errors {
        if let (Some(file), Some(line)) = (&e.file, e.line) {
            let key = (file.clone(), line);
            if seen.insert(key.clone()) {
                locations.push(key);
            }
        }
    }

    if locations.is_empty() {
        return String::new();
    }

    let mut sections: Vec<String> = Vec::new();
    let mut total_bytes: usize = 0;

    for (file, line) in &locations {
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let lines: Vec<&str> = content.lines().collect();
        let line_idx = (*line as usize).saturating_sub(1); // 0-indexed
        let start = line_idx.saturating_sub(5);
        let end = (line_idx + 6).min(lines.len()); // exclusive, +5 lines after

        if start >= lines.len() {
            continue;
        }

        let mut snippet = format!("**{}** (around line {}):\n```\n", file, line);
        for (i, src_line) in lines.iter().enumerate().take(end).skip(start) {
            snippet.push_str(&format!("{:>4}: {}\n", i + 1, src_line));
        }
        snippet.push_str("```\n");

        // Check if adding this snippet would exceed our budget
        if total_bytes + snippet.len() > SOURCE_CONTEXT_MAX_BYTES {
            break;
        }
        total_bytes += snippet.len();
        sections.push(snippet);
    }

    if sections.is_empty() {
        return String::new();
    }

    format!("\n## Relevant source context\n\n{}", sections.join("\n"))
}

/// Build a prompt asking the agent to fix failures from a watch command.
///
/// Includes a hint about the command type (lint, test, or general command)
/// so the agent can choose an appropriate fix strategy. Lint failures are
/// usually mechanical (unused imports, formatting), while test failures
/// require understanding the intended behavior.
pub fn build_watch_fix_prompt(watch_cmd: &str, output: &str) -> String {
    let truncated = if output.len() > WATCH_OUTPUT_MAX {
        format!("{}... (truncated)", safe_truncate(output, WATCH_OUTPUT_MAX))
    } else {
        output.to_string()
    };
    let cmd_type = classify_watch_command(watch_cmd);

    // Detect language from the watch command, then try the appropriate parser.
    // Fall through to generic prompt if no structured errors are found.
    let lang = detect_watch_language(watch_cmd);
    let (errors, detected_lang) = match lang {
        WatchLanguage::Rust => {
            let errs = parse_rust_errors(output);
            (errs, WatchLanguage::Rust)
        }
        WatchLanguage::TypeScript => {
            let errs = parse_typescript_errors(output);
            (errs, WatchLanguage::TypeScript)
        }
        WatchLanguage::Python => {
            let errs = parse_python_errors(output);
            (errs, WatchLanguage::Python)
        }
        WatchLanguage::Unknown => {
            // Try each parser in order: Rust → TypeScript → Python
            let rust_errs = parse_rust_errors(output);
            if !rust_errs.is_empty() {
                (rust_errs, WatchLanguage::Rust)
            } else {
                let ts_errs = parse_typescript_errors(output);
                if !ts_errs.is_empty() {
                    (ts_errs, WatchLanguage::TypeScript)
                } else {
                    let py_errs = parse_python_errors(output);
                    (py_errs, WatchLanguage::Python)
                }
            }
        }
    };
    let structured_section = build_error_summary_for(&errors, detected_lang);
    let source_context = extract_error_source_context(&errors);

    let hint = match cmd_type {
        "lint" => "\n\nThis is a **lint** failure — fixes are usually mechanical (unused imports, \
                   missing derives, formatting issues). Apply targeted fixes without changing logic.",
        "test" => "\n\nThis is a **test** failure — understand what the test expects before \
                   changing code. Fix the implementation to match the intended behavior, \
                   or fix the test if the new behavior is correct.",
        _ => "",
    };

    let base = if let Some(summary) = structured_section {
        format!(
            "Your changes caused {cmd_type} failures. Here's the output from `{watch_cmd}`:\n\
             ```\n{truncated}\n```\n\n\
             {summary}{source_context}{hint}"
        )
    } else {
        format!(
            "Your changes caused {cmd_type} failures. Here's the output from `{watch_cmd}`:\n\
             ```\n{truncated}\n```\n\
             Please fix the issues.{source_context}{hint}"
        )
    };

    // Enrich the prompt with file paths extracted from the error output
    // so the agent knows exactly which files to focus on.
    let error_files = extract_file_paths_from_output(output);
    let risk_section = if !error_files.is_empty() {
        let risk_entries = risk_context_for_files(&error_files);
        format_risk_context(&risk_entries)
    } else {
        String::new()
    };

    // Day 163 (#726): the emerging-risk annotation that used to live here is gone.
    // The forecast it injected measured 0% recall over 10 graded failure days against
    // a ~39% pooled achievable ceiling (#724), so it steered fix agents on nothing.
    if error_files.is_empty() {
        base
    } else {
        let file_list = error_files.join(", ");
        format!(
            "{base}\n\nFiles referenced in errors: {file_list}. Focus your fixes on these files.{risk_section}"
        )
    }
}

/// What live progress the watch-command streamer should print while
/// collecting output. Pure decision helper so both modes are unit-testable
/// (mirrors the plain-output gating on Spinner/ToolProgressTimer in
/// `format/tools.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WatchProgress {
    /// Animated `\r` repaint with a live line counter (tty, plain output off).
    Repaint,
    /// One static announcement line, no repaints (screen-reader mode on a tty).
    StaticLine,
    /// Nothing (stderr is not a terminal).
    Silent,
}

/// Decide how to render live watch-command progress. `\r` repaints are only
/// allowed on a terminal with plain (screen-reader) output off.
pub(crate) fn watch_progress_mode(is_tty: bool, plain_output: bool) -> WatchProgress {
    if !is_tty {
        WatchProgress::Silent
    } else if plain_output {
        WatchProgress::StaticLine
    } else {
        WatchProgress::Repaint
    }
}

/// Run a watch command and return (success, output).
///
/// Streams output line-by-line in real time: when stderr is a terminal,
/// prints a compact progress indicator (`⟳ 42 lines...`) so the user
/// sees something happening during long test/build runs — or, in plain
/// (screen-reader) mode, a single static announcement line with no `\r`
/// repaints.  The full combined stdout+stderr is still collected and
/// returned for the agent to analyse.
pub fn run_watch_command(cmd: &str) -> (bool, String) {
    use std::io::BufRead;
    use std::process::{Command, Stdio};

    let progress =
        watch_progress_mode(io::stderr().is_terminal(), crate::format::is_plain_output());

    let child = Command::new("sh")
        .args(["-c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to run watch command: {e}")),
    };

    // Collect stderr lines in a background thread.
    let stderr_pipe = child.stderr.take().expect("stderr was piped");
    let stderr_handle = std::thread::spawn(move || {
        let reader = io::BufReader::new(stderr_pipe);
        let mut lines = Vec::new();
        for line in reader.lines() {
            match line {
                Ok(l) => lines.push(l),
                Err(_) => break,
            }
        }
        lines
    });

    // Stream stdout on the main thread, collecting lines.
    let mut stdout_lines: Vec<String> = Vec::new();
    if let Some(stdout_pipe) = child.stdout.take() {
        let reader = io::BufReader::new(stdout_pipe);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    stdout_lines.push(l);
                    match progress {
                        WatchProgress::Repaint => {
                            let count = stdout_lines.len();
                            eprint!("\r{DIM}  ⟳ {count} lines...{RESET}");
                            let _ = io::stderr().flush();
                        }
                        WatchProgress::StaticLine => {
                            // Announce once, when output starts arriving —
                            // no repaints for screen readers.
                            if stdout_lines.len() == 1 {
                                eprintln!("{}", crate::format::plain_watch_progress_line());
                            }
                        }
                        WatchProgress::Silent => {}
                    }
                }
                Err(_) => break,
            }
        }
    }

    let stderr_lines = stderr_handle.join().unwrap_or_default();

    // Clear the progress indicator if we repainted one (plain mode's static
    // line needs no clearing — clearing would itself be a `\r` repaint).
    if progress == WatchProgress::Repaint && !stdout_lines.is_empty() {
        eprint!("\r{DIM}                          {RESET}\r");
        let _ = io::stderr().flush();
    }

    let status = match child.wait() {
        Ok(s) => s.success(),
        Err(_) => false,
    };

    // Combine stdout + stderr the same way the old implementation did.
    let stdout_text = stdout_lines.join("\n");
    let stderr_text = stderr_lines.join("\n");
    let combined = if stderr_text.is_empty() {
        stdout_text
    } else if stdout_text.is_empty() {
        stderr_text
    } else {
        format!("{stdout_text}\n{stderr_text}")
    };

    (status, combined)
}

/// Try to extract a project learning from a successful watch-mode fix.
///
/// Parses structured errors from the failed output, identifies the dominant
/// error category, and (if recognizable) builds a learning note via
/// [`build_learn_memory_note`]. Conservative: only learns from clear patterns,
/// not every fix.
///
/// Called after a successful fix attempt — `failed_output` is the error output
/// that the agent just fixed.
pub fn maybe_learn_from_fix(watch_cmd: &str, failed_output: &str) {
    // Detect language and parse structured errors
    let lang = detect_watch_language(watch_cmd);
    let errors: Vec<CompilerError> = match lang {
        WatchLanguage::Rust => parse_rust_errors(failed_output),
        WatchLanguage::TypeScript => parse_typescript_errors(failed_output),
        WatchLanguage::Python => parse_python_errors(failed_output),
        WatchLanguage::Unknown => {
            let rust_errs = parse_rust_errors(failed_output);
            if !rust_errs.is_empty() {
                rust_errs
            } else {
                let ts_errs = parse_typescript_errors(failed_output);
                if !ts_errs.is_empty() {
                    ts_errs
                } else {
                    parse_python_errors(failed_output)
                }
            }
        }
    };

    if errors.is_empty() {
        return;
    }

    // Find the dominant error category (most frequent non-Other)
    let mut category_counts: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for e in &errors {
        *category_counts.entry(e.category.label()).or_insert(0) += 1;
    }
    // Pick the most frequent category, preferring non-"other"
    let dominant = category_counts
        .iter()
        .filter(|(k, _)| **k != "other")
        .max_by_key(|(_, v)| **v)
        .map(|(k, _)| *k);

    let dominant = match dominant {
        Some(cat) => cat,
        None => return, // all errors are "other" — too generic to learn from
    };

    // Collect error messages for the dominant category
    let messages: Vec<String> = errors
        .iter()
        .filter(|e| e.category.label() == dominant)
        .map(|e| e.message.clone())
        .collect();

    if let Some(note) = build_learn_memory_note(watch_cmd, &messages, dominant) {
        auto_remember(&note);
    }
}

/// Decide whether the post-prompt watch cycle should run at all.
///
/// Returns `false` when the prompt changed zero files (no write/edit tool
/// calls this turn). Running clippy/tests after a read-only prompt burns
/// wall-clock time and — worse — misattributes any pre-existing failures
/// to "your changes" in the auto-fix loop. Pure so it's trivially testable.
///
/// `edits_before` is the caller's own [`SessionChanges::edit_count`] sampled
/// *before* the prompt ran, which is what makes this a **per-turn** question.
/// Day 175 (#818, found by blind round 71): this used to be
/// `!changes.snapshot().is_empty()` — the *session-wide* deduped file list,
/// which `clear()` only empties on `/clear`. Once anything had been edited in
/// the session it returned `true` forever, so the predicate could not answer
/// the question the paragraph above claims it answers. Three of its four
/// callers happened to be safe for three different reasons (`repl.rs` has its
/// own outer per-turn delta guard; both `main.rs` sites are per-process, where
/// session-wide *is* per-turn), and the fourth — `/extended`, at
/// `conversations.rs` — had no outer guard at all, so a zero-change
/// `/extended` turn ran the full lint→fix→test cycle and was then told
/// "Your changes caused failures" about a failure it did not cause.
///
/// The counter, not the vec length, is the right input: a turn that re-edits
/// an already-touched file grows `edit_count()` but leaves `snapshot().len()`
/// unchanged (that is why #774 added it).
fn should_run_watch_after_prompt(changes: &SessionChanges, edits_before: usize) -> bool {
    changes.edit_count() > edits_before
}

/// Run the watch command(s) after a prompt completes.
///
/// If the prompt changed zero files, the whole cycle is skipped (see
/// [`should_run_watch_after_prompt`]) — there is nothing the watch could
/// be validating, and pre-existing failures would be misattributed to
/// the prompt.
///
/// If watch commands are active, iterates through each phase in order.
/// For each phase: runs the command, and if it fails, enters the fix loop
/// (up to [`MAX_WATCH_FIX_ATTEMPTS`] times). Only proceeds to the next
/// phase if the current one passes. This means lint gets fixed before tests
/// even run.
///
/// Returns a [`WatchResult`] with pass/fail status and the last tool error
/// from any fix attempts. If no watch command is set, returns
/// `WatchResult { passed: true, last_tool_error: None }`.
pub async fn run_watch_after_prompt(
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
    changes: &SessionChanges,
    edits_before: usize,
) -> WatchResult {
    let commands = get_watch_commands();
    if commands.is_empty() {
        return WatchResult {
            passed: true,
            last_tool_error: None,
        };
    }

    if !should_run_watch_after_prompt(changes, edits_before) {
        eprintln!("{DIM}  watch: no files changed this turn — skipping{RESET}");
        return WatchResult {
            passed: true,
            last_tool_error: None,
        };
    }

    let total_phases = commands.len();
    let mut last_tool_error: Option<String> = None;

    for (phase_idx, watch_cmd) in commands.iter().enumerate() {
        let phase_num = phase_idx + 1;
        let phase_label = if total_phases > 1 {
            format!(" (phase {phase_num}/{total_phases})")
        } else {
            String::new()
        };

        let (ok, output) = run_watch_command(watch_cmd);
        if ok {
            eprintln!("{GREEN}  ✓ Watch passed{phase_label}: `{watch_cmd}`{RESET}");
            continue;
        }

        eprintln!("{RED}  ✗ Watch failed{phase_label}: `{watch_cmd}`{RESET}");
        let display_output = if output.len() > 2000 {
            format!("{}...\n(truncated)", safe_truncate(&output, 2000))
        } else {
            output.clone()
        };
        eprintln!("{DIM}{display_output}{RESET}");

        // Multi-attempt auto-fix loop for this phase
        let mut current_output = output;
        let mut phase_passed = false;
        for attempt in 1..=MAX_WATCH_FIX_ATTEMPTS {
            if session_budget_exhausted(30) {
                eprintln!(
                    "{DIM}  ⏱ session budget nearly exhausted, stopping watch fix loop early{RESET}"
                );
                let changed: Vec<String> =
                    changes.snapshot().iter().map(|c| c.path.clone()).collect();
                crate::commands_risk::auto_validate_after_failure(&changed, "watch_failure");
                return WatchResult {
                    passed: false,
                    last_tool_error,
                };
            }
            eprintln!("{YELLOW}  → Auto-fixing{phase_label} (attempt {attempt}/{MAX_WATCH_FIX_ATTEMPTS})...{RESET}");

            let fix_prompt = build_watch_fix_prompt(watch_cmd, &current_output);
            let fix_outcome =
                run_prompt_auto_retry(agent, &fix_prompt, session_total, model, changes).await;
            last_tool_error = fix_outcome.last_tool_error.clone();

            // Re-run this phase's command to see if fix worked
            let (fix_ok, fix_output) = run_watch_command(watch_cmd);
            if fix_ok {
                eprintln!(
                    "{GREEN}  ✓ Watch passed{phase_label} after fix (attempt {attempt}){RESET}"
                );
                let note = build_fix_memory_note(watch_cmd, attempt);
                auto_remember(&note);
                // Try to learn a project fact from the error that was fixed
                maybe_learn_from_fix(watch_cmd, &current_output);
                phase_passed = true;
                break;
            } else if attempt == MAX_WATCH_FIX_ATTEMPTS {
                eprintln!(
                    "{RED}  ✗ Watch still failing{phase_label} after {MAX_WATCH_FIX_ATTEMPTS} attempts — manual fix needed{RESET}"
                );
            } else {
                eprintln!("{RED}  ✗ Attempt {attempt} failed{phase_label}, retrying...{RESET}");
                current_output = fix_output;
            }
        }

        if !phase_passed {
            // Stop: don't proceed to later phases if this one can't be fixed
            let changed: Vec<String> = changes.snapshot().iter().map(|c| c.path.clone()).collect();
            crate::commands_risk::auto_validate_after_failure(&changed, "watch_failure");
            return WatchResult {
                passed: false,
                last_tool_error,
            };
        }
    }

    // Validate risk predictions even on success — knowing which predicted-risky
    // files survived is valuable data for the prediction-validation loop.
    let changed: Vec<String> = changes.snapshot().iter().map(|c| c.path.clone()).collect();
    crate::commands_risk::auto_validate_after_failure(&changed, "watch_success");

    WatchResult {
        passed: true,
        last_tool_error,
    }
}

/// Auto-detect the appropriate watch command for the current project.
/// Returns a lint+test combo command (e.g. `cargo clippy … && cargo test`) when
/// both are available, falls back to test-only, or `None` for unknown project types.
pub fn auto_detect_watch_command() -> Option<String> {
    detect_watch_all_command()
}

/// Print a hint to stderr when a test runner is detected but auto-watch is disabled.
/// Call this from startup paths when `auto_watch` is false.
pub fn hint_auto_watch_available() {
    if let Some(cmd) = auto_detect_watch_command() {
        eprintln!(
            "{DIM}  💡 Detected `{cmd}` — enable auto-watch with `auto_watch = true` in .yoyo.toml{RESET}"
        );
    }
}

/// Return true if `tool` resolves to an executable: either an existing path
/// (when it contains a path separator) or a file found in one of the `PATH`
/// directories. Used as a pre-flight check before arming auto-watch so a
/// detected-but-uninstalled runner fails once at startup instead of at every
/// watch cycle.
pub fn tool_on_path(tool: &str) -> bool {
    if tool.is_empty() {
        return false;
    }
    let p = std::path::Path::new(tool);
    // Explicit path (relative like ./run.sh or absolute): just check existence.
    if p.components().count() > 1 || p.is_absolute() {
        return p.exists();
    }
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        if dir.join(tool).is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            for ext in ["exe", "cmd", "bat", "com"] {
                if dir.join(format!("{tool}.{ext}")).is_file() {
                    return true;
                }
            }
        }
    }
    false
}

/// Check every `&&`-separated phase of a watch command and return the first
/// leading token that is NOT on PATH, or `None` if all phases look runnable.
/// (Detected commands are either a single command or `lint && test` combos.)
pub fn missing_watch_tool(cmd: &str) -> Option<String> {
    for phase in cmd.split("&&") {
        if let Some(tok) = phase.split_whitespace().next() {
            if !tool_on_path(tok) {
                return Some(tok.to_string());
            }
        }
    }
    None
}

/// Shared arming path for `auto_watch = true` (issue #557, follow-up to #448).
/// Detects the project's watch command, validates its tool(s) exist on PATH,
/// and arms the watch command — or explains why it couldn't:
///
/// 1. Nothing detected → warn (no more silent no-op behind the opt-in flag).
/// 2. Detected but the tool isn't installed → warn once at startup instead of
///    failing at every watch cycle.
/// 3. Otherwise arm and announce.
///
/// `disable_hint` is interpolated into the announcement (call sites differ:
/// the REPL also offers `/watch off`). When `quiet` is true the success
/// announcement is suppressed (piped/print mode), but warnings still go to
/// stderr — surfacing misconfiguration is the point.
pub fn arm_auto_watch(disable_hint: &str, quiet: bool) -> Option<String> {
    let Some(cmd) = auto_detect_watch_command() else {
        eprintln!(
            "{DIM}  auto_watch is enabled but no test command was detected for this project — set one with /watch <cmd>{RESET}"
        );
        return None;
    };
    if let Some(tool) = missing_watch_tool(&cmd) {
        eprintln!(
            "{DIM}  auto_watch detected `{cmd}` but `{tool}` is not on PATH — watch not armed{RESET}"
        );
        return None;
    }
    set_watch_command(&cmd);
    if !quiet {
        eprintln!("{DIM}  👀 Auto-watch: `{cmd}` (disable with {disable_hint}){RESET}");
    }
    Some(cmd)
}

/// Auto-detect a combined lint + test command for the given directory.
/// Returns both commands chained with `&&` so the first failure stops execution.
/// Falls back to just the test command if no lint command is available,
/// or `None` if neither can be detected.
pub fn detect_watch_all_command_for_dir(dir: &std::path::Path) -> Option<String> {
    let project_type = detect_project_type(dir);
    let lint = lint_command_for_project(&project_type, LintStrictness::Default);
    let test = test_command_for_project(&project_type, dir);
    match (lint, test) {
        (Some((lint_label, _)), Some((test_label, _))) => {
            Some(format!("{lint_label} && {test_label}"))
        }
        (None, Some((test_label, _))) => Some(test_label.to_string()),
        (Some((lint_label, _)), None) => Some(lint_label),
        (None, None) => None,
    }
}

/// Auto-detect a combined lint + test command for the current working directory.
/// Thin wrapper around [`detect_watch_all_command_for_dir`].
pub fn detect_watch_all_command() -> Option<String> {
    let dir = std::env::current_dir().unwrap_or_default();
    detect_watch_all_command_for_dir(&dir)
}

/// Auto-detect separate lint and test commands for two-phase watch in the given directory.
/// Returns a vec of individual commands (lint first, then test).
/// Falls back to a single-element vec if only one is available.
pub fn detect_watch_all_phases_for_dir(dir: &std::path::Path) -> Option<Vec<String>> {
    let project_type = detect_project_type(dir);
    let lint = lint_command_for_project(&project_type, LintStrictness::Default);
    let test = test_command_for_project(&project_type, dir);
    match (lint, test) {
        (Some((lint_label, _)), Some((test_label, _))) => {
            Some(vec![lint_label.to_string(), test_label.to_string()])
        }
        (None, Some((test_label, _))) => Some(vec![test_label.to_string()]),
        (Some((lint_label, _)), None) => Some(vec![lint_label]),
        (None, None) => None,
    }
}

/// Watch subcommand names for tab completion.
pub const WATCH_SUBCOMMANDS: &[&str] = &["off", "status", "all", "lint"];

/// Near-miss typo guard for `/watch`'s free-text argument.
///
/// A single word that isn't a known subcommand but sits within edit distance 2
/// of one (e.g. "of" → "off") is almost certainly a typo — silently installing
/// it as the custom watch command would make every subsequent prompt turn fail.
/// Returns the suggested subcommand when the guard should fire. Multi-word
/// custom commands (the real use case, e.g. `npm test`) and single words far
/// from any subcommand (e.g. `pytest`) are never touched.
pub fn watch_near_miss(arg: &str) -> Option<&'static str> {
    if arg.is_empty() || arg.contains(char::is_whitespace) {
        return None;
    }
    // Exact subcommands are handled by their own match arms.
    if WATCH_SUBCOMMANDS.contains(&arg) {
        return None;
    }
    crate::commands::closest_match(arg, WATCH_SUBCOMMANDS, 2)
}

/// Auto-detect a lint-only command for the given directory.
/// Thin dir-taking sibling of the cwd-reading detection used by `/watch lint`.
pub fn detect_watch_lint_command_for_dir(dir: &std::path::Path) -> Option<String> {
    let project_type = detect_project_type(dir);
    lint_command_for_project(&project_type, LintStrictness::Default).map(|(label, _)| label)
}

/// Handle the /watch command: toggle auto-test-on-edit mode.
///
/// Thin wrapper that resolves the current working directory and delegates to
/// [`handle_watch_in`], which is the dir-taking seam. Behaviour is unchanged:
/// project-type detection has always read the process cwd here.
pub fn handle_watch(input: &str) {
    let dir = std::env::current_dir().unwrap_or_default();
    handle_watch_in(&dir, input);
}

/// Handle the /watch command against an explicit project directory.
///
/// Exists so tests can exercise `/watch`, `/watch all` and `/watch lint`
/// without moving the process CWD (#780) — the only reason those tests ever
/// moved the cwd was to let project-type detection see a
/// `Cargo.toml`.
pub(crate) fn handle_watch_in(dir: &std::path::Path, input: &str) {
    let arg = input.strip_prefix("/watch").unwrap_or("").trim();

    match arg {
        "" | "all" => {
            // Auto-detect lint+test as separate phases
            match detect_watch_all_phases_for_dir(dir) {
                Some(phases) => {
                    let display = phases.join(" && ");
                    let phase_refs: Vec<&str> = phases.iter().map(|s| s.as_str()).collect();
                    set_watch_commands(&phase_refs);
                    if phases.len() > 1 {
                        println!(
                            "{GREEN}  👀 Watch mode ON — {n} phases: `{display}`{RESET}\n",
                            n = phases.len()
                        );
                    } else {
                        println!(
                            "{GREEN}  👀 Watch mode ON — will run `{display}` after agent edits{RESET}\n"
                        );
                    }
                }
                None => {
                    println!("{DIM}  No lint or test command detected. Specify one:{RESET}");
                    println!("{DIM}    /watch cargo clippy && cargo test{RESET}");
                    println!("{DIM}    /watch npm run lint && npm test{RESET}\n");
                }
            }
        }
        "off" => {
            clear_watch_command();
            println!("{DIM}  👀 Watch mode OFF{RESET}\n");
        }
        "status" => match get_watch_command() {
            Some(cmd) => {
                let phases = get_watch_commands();
                println!("{DIM}  👀 Watch mode: ON{RESET}");
                if phases.len() > 1 {
                    println!("{DIM}  Phases ({}):{RESET}", phases.len());
                    for (i, phase) in phases.iter().enumerate() {
                        println!("{DIM}    {}. `{phase}`{RESET}", i + 1);
                    }
                    println!();
                } else {
                    println!("{DIM}  Command: `{cmd}`{RESET}\n");
                }
            }
            None => {
                println!("{DIM}  👀 Watch mode: OFF{RESET}\n");
            }
        },
        "lint" => {
            // Auto-detect lint-only command
            match detect_watch_lint_command_for_dir(dir) {
                Some(lint_label) => {
                    set_watch_command(&lint_label);
                    println!("{GREEN}  👀 Watch set to: {lint_label}{RESET}\n");
                }
                None => {
                    println!("{DIM}  No lint command detected for this project type.{RESET}");
                    println!("{DIM}    /watch cargo clippy{RESET}");
                    println!("{DIM}    /watch npx eslint .{RESET}\n");
                }
            }
        }
        custom_cmd => {
            // Near-miss typo guard: "/watch of" should not silently install
            // "of" as the watch command that then fails on every prompt turn.
            if let Some(suggestion) = watch_near_miss(custom_cmd) {
                eprintln!(
                    "{RED}  ✗ unknown word '{custom_cmd}' — did you mean /watch {suggestion}?{RESET}"
                );
                println!(
                    "{DIM}  (to set '{custom_cmd}' as a custom watch command anyway, use the full command, e.g. /watch {custom_cmd} --flags){RESET}\n"
                );
                return;
            }
            set_watch_command(custom_cmd);
            println!(
                "{GREEN}  👀 Watch mode ON — will run `{custom_cmd}` after agent edits{RESET}\n"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // Tests that read/write the global WATCH_COMMANDS state must be annotated with
    // #[serial] to prevent flaky failures from parallel test execution. The
    // `serial_test` crate ensures these tests run one at a time. Any test calling
    // set_watch_command, set_watch_commands, get_watch_command, get_watch_commands,
    // clear_watch_command, handle_watch, detect_watch_all_phases,
    // detect_watch_all_command, or auto_detect_watch_command must use #[serial].

    /// Drop guard that clears global watch state on drop (even on panic).
    /// Use via `with_clean_watch_state` to ensure every serial test starts
    /// and ends with a clean `WATCH_COMMANDS`.
    struct WatchStateGuard;

    impl Drop for WatchStateGuard {
        fn drop(&mut self) {
            clear_watch_command();
        }
    }

    /// Run a closure with clean global watch state. Clears WATCH_COMMANDS
    /// before the closure runs, and clears again on return — even if the
    /// closure panics (via a drop guard).
    fn with_clean_watch_state<F: FnOnce()>(f: F) {
        clear_watch_command();
        let _guard = WatchStateGuard;
        f();
    }

    #[test]
    fn test_maybe_learn_from_fix_import_error() {
        // Simulate a Rust import error output
        let output = "error[E0433]: failed to resolve: use of undeclared crate or module `serde`\n\
                       --> src/main.rs:1:5\n\
                       |\n\
                       1 | use serde::Deserialize;\n\
                       |     ^^^^^ use of undeclared crate or module `serde`\n";
        let path = std::env::temp_dir().join("yoyo_test_learn_import");
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
        let mem_path = path.join("memory.json");
        let _ = std::fs::remove_file(&mem_path);

        // Verify errors are parsed
        let errors = parse_rust_errors(output);
        assert!(!errors.is_empty(), "should parse at least one error");
        assert_eq!(
            errors[0].category,
            ErrorCategory::Import,
            "should classify as import error"
        );

        // The function runs auto_remember internally (to the global path),
        // so we test the building blocks instead:
        let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
        let note = crate::memory::build_learn_memory_note("cargo build", &messages, "import");
        assert!(
            note.is_some(),
            "import errors should produce a learning note"
        );
        let note = note.unwrap();
        assert!(
            note.starts_with("[build]"),
            "note should be categorized: {note}"
        );
        assert!(
            note.contains("cargo build"),
            "note should mention the command: {note}"
        );

        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn test_maybe_learn_from_fix_no_pattern() {
        // Output with no recognizable structured errors → should not learn
        let output = "some random output that is not an error";
        let errors = parse_rust_errors(output);
        assert!(
            errors.is_empty(),
            "random output should not parse as errors"
        );
        // With no errors, maybe_learn_from_fix returns without calling auto_remember
        // (we just verify the logic path — the function is a no-op)
        maybe_learn_from_fix("cargo test", output);
    }

    #[test]
    fn test_maybe_learn_from_fix_other_category_skipped() {
        // Errors that are all "other" category should not produce a learning
        let messages = vec!["some unknown error".to_string()];
        let note = crate::memory::build_learn_memory_note("cargo build", &messages, "other");
        assert!(
            note.is_none(),
            "other-category errors should not produce learning notes"
        );
    }

    #[test]
    fn test_maybe_learn_from_fix_type_error() {
        let output = "error[E0308]: mismatched types\n\
                       --> src/lib.rs:10:5\n\
                       |\n\
                       10 |     foo()\n\
                       |     ^^^^^ expected `usize`, found `&str`\n";
        let errors = parse_rust_errors(output);
        assert!(!errors.is_empty());
        assert_eq!(errors[0].category, ErrorCategory::Type);

        let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
        let note = crate::memory::build_learn_memory_note("cargo test", &messages, "type");
        assert!(note.is_some());
        assert!(note.unwrap().contains("Type issue"));
    }

    #[test]
    fn test_build_watch_fix_prompt() {
        let prompt = build_watch_fix_prompt("cargo test", "error[E0308]: mismatched types");
        assert!(
            prompt.contains("cargo test"),
            "prompt should include the command name"
        );
        assert!(
            prompt.contains("error[E0308]: mismatched types"),
            "prompt should include the output"
        );
        // With structured parsing, we get a detailed summary instead of "Please fix"
        assert!(
            prompt.contains("Parsed 1 Rust error"),
            "prompt should include structured error summary: {prompt}"
        );
        assert!(
            prompt.contains("```"),
            "prompt should wrap output in code fence"
        );
    }

    #[test]
    fn test_max_watch_fix_attempts_constant() {
        // The constant should exist and be a reasonable retry count (1..=10)
        let attempts = MAX_WATCH_FIX_ATTEMPTS;
        assert!(attempts >= 1, "should allow at least 1 attempt");
        assert!(attempts <= 10, "should not retry excessively");
        assert_eq!(attempts, 3, "default should be 3 attempts");
    }

    #[test]
    fn test_build_watch_fix_prompt_truncates_long_output() {
        let long_output = "x".repeat(6000);
        let prompt = build_watch_fix_prompt("cargo test", &long_output);
        assert!(
            prompt.contains("... (truncated)"),
            "long output should be truncated"
        );
        // The output in the prompt should not contain the full 6000 chars
        assert!(
            !prompt.contains(&"x".repeat(6000)),
            "full output should not appear"
        );
        // But should contain the first 5000
        assert!(
            prompt.contains(&"x".repeat(5000)),
            "first 5000 chars should appear"
        );
    }

    #[test]
    fn test_run_watch_command_success() {
        let (ok, output) = run_watch_command("echo hello");
        assert!(ok, "echo should succeed");
        assert_eq!(output.trim(), "hello");
    }

    #[test]
    fn test_run_watch_command_failure() {
        let (ok, _output) = run_watch_command("exit 1");
        assert!(!ok, "exit 1 should fail");
    }

    #[test]
    fn test_run_watch_command_captures_all_output() {
        let (ok, output) = run_watch_command("for i in 1 2 3 4 5; do echo line$i; done");
        assert!(ok);
        assert!(output.contains("line1"));
        assert!(output.contains("line5"));
        // Should have all 5 lines
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 5, "should capture all 5 lines");
    }

    #[test]
    fn test_run_watch_command_captures_stderr() {
        let (ok, output) = run_watch_command("echo err_msg >&2");
        assert!(ok, "writing to stderr is not a failure");
        assert!(
            output.contains("err_msg"),
            "stderr should be captured: {output}"
        );
    }

    #[test]
    fn test_run_watch_command_combines_stdout_stderr() {
        let (ok, output) = run_watch_command("echo out_msg; echo err_msg >&2");
        assert!(ok);
        assert!(output.contains("out_msg"), "should contain stdout");
        assert!(output.contains("err_msg"), "should contain stderr");
    }

    #[test]
    fn test_run_watch_command_invalid_command() {
        let (ok, output) = run_watch_command("nonexistent_command_xyz_123");
        assert!(!ok, "nonexistent command should fail");
        assert!(
            !output.is_empty(),
            "should have some error output: {output}"
        );
    }

    #[serial]
    #[test]
    fn test_watch_command_none_by_default() {
        with_clean_watch_state(|| {
            assert!(
                get_watch_command().is_none(),
                "should have no watch command after clear"
            );
        });
    }

    #[serial]
    #[test]
    fn test_watch_command_roundtrip() {
        with_clean_watch_state(|| {
            set_watch_command("cargo test --release");
            let cmd = get_watch_command();
            assert_eq!(cmd.as_deref(), Some("cargo test --release"));
            clear_watch_command();
            assert!(get_watch_command().is_none());
        });
    }

    #[test]
    fn test_should_run_watch_after_prompt_false_when_no_changes() {
        // Negative case: a fresh SessionChanges has zero recorded file
        // changes — the post-prompt watch cycle must be skipped.
        let changes = SessionChanges::new();
        assert!(
            !should_run_watch_after_prompt(&changes, 0),
            "watch should be skipped when the prompt changed zero files"
        );
    }

    #[test]
    fn test_should_run_watch_after_prompt_true_with_one_change() {
        // Paired positive case (Day 122 lesson): the minimum possible
        // difference — one recorded change — must flip the decision.
        let changes = SessionChanges::new();
        changes.record("src/main.rs", crate::session::ChangeKind::Edit);
        assert!(
            should_run_watch_after_prompt(&changes, 0),
            "watch should run when the prompt changed at least one file"
        );
    }

    #[test]
    fn watch_gate_reads_a_per_turn_delta_not_session_wide_state() {
        // #818, found by blind round 71. The gate used to be
        // `!changes.snapshot().is_empty()` — the SESSION-WIDE deduped file
        // list, which `clear()` only empties on `/clear`. Both tests above
        // drive a FRESH tracker, where session-wide state and per-turn state
        // are identical, so neither could ever distinguish the two readings:
        // vacuous green in exactly the direction that mattered. This test
        // drives a NON-fresh tracker, which is the case they could not reach.
        let changes = SessionChanges::new();

        // An earlier turn in this session edited a file.
        changes.record("src/main.rs", crate::session::ChangeKind::Edit);

        // A later turn begins: sample the baseline, then change nothing.
        let edits_before = changes.edit_count();
        assert!(
            !should_run_watch_after_prompt(&changes, edits_before),
            "a zero-edit turn must be skipped even when the session already \
             has recorded changes — under the old session-wide reading this \
             returned true forever after the session's first edit"
        );

        // Near-miss guard (Day 124 lesson): a discriminator tested only on the
        // side that blocks is vacuous green. Re-edit the SAME file — the
        // deduped vec does not grow, so `snapshot().len()` is unchanged and
        // only the monotonic counter (#774) can see this turn's work.
        let files_before = changes.snapshot().len();
        changes.record("src/main.rs", crate::session::ChangeKind::Edit);
        assert_eq!(
            changes.snapshot().len(),
            files_before,
            "precondition: re-editing the same file must not grow the deduped \
             set — that is why the counter, not the length, is the right input"
        );
        assert!(
            should_run_watch_after_prompt(&changes, edits_before),
            "re-editing an already-touched file is still work this turn"
        );
    }

    #[serial]
    #[test]
    fn test_run_watch_after_prompt_no_watch_returns_passed() {
        with_clean_watch_state(|| {
            // When no watch command is set, run_watch_after_prompt should return
            // WatchResult { passed: true, last_tool_error: None } immediately.
            // We verify the guard condition that makes it return early.
            assert!(
                get_watch_command().is_none(),
                "precondition: no watch command set"
            );
            // The function checks get_watch_command() first and returns a passing
            // WatchResult if None. We can't call the async function in a sync test,
            // but we verify the guard condition that makes it return early.
        });
    }

    #[serial]
    #[test]
    fn test_run_watch_command_pass_with_set_watch() {
        with_clean_watch_state(|| {
            set_watch_command("echo ok");
            if let Some(cmd) = get_watch_command() {
                let (ok, output) = run_watch_command(&cmd);
                assert!(ok, "echo ok should succeed");
                assert!(output.contains("ok"));
            } else {
                panic!("watch command should be set");
            }
        });
    }

    #[serial]
    #[test]
    fn test_run_watch_command_fail_with_set_watch() {
        with_clean_watch_state(|| {
            set_watch_command("sh -c 'echo FAIL; exit 1'");
            if let Some(cmd) = get_watch_command() {
                let (ok, output) = run_watch_command(&cmd);
                assert!(!ok, "command should fail");
                assert!(output.contains("FAIL"), "output should contain FAIL");
                // Verify build_watch_fix_prompt works with the output
                let fix_prompt = build_watch_fix_prompt(&cmd, &output);
                assert!(fix_prompt.contains("FAIL"));
                assert!(fix_prompt.contains("Please fix"));
            } else {
                panic!("watch command should be set");
            }
        });
    }

    #[test]
    fn test_watch_result_passed() {
        let result = WatchResult {
            passed: true,
            last_tool_error: None,
        };
        assert!(result.passed);
        assert!(result.last_tool_error.is_none());
    }

    #[test]
    fn test_watch_result_failed_with_error() {
        let result = WatchResult {
            passed: false,
            last_tool_error: Some("compilation error".to_string()),
        };
        assert!(!result.passed);
        assert_eq!(result.last_tool_error.as_deref(), Some("compilation error"));
    }

    #[test]
    fn test_watch_result_clone() {
        let result = WatchResult {
            passed: false,
            last_tool_error: Some("test failure".to_string()),
        };
        let cloned = result.clone();
        assert_eq!(cloned.passed, result.passed);
        assert_eq!(cloned.last_tool_error, result.last_tool_error);
    }

    #[test]
    fn test_watch_result_debug() {
        let result = WatchResult {
            passed: true,
            last_tool_error: None,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("passed: true"));
        assert!(debug.contains("last_tool_error: None"));
    }

    // --- Multi-phase watch tests ---

    #[serial]
    #[test]
    fn test_set_get_watch_commands_roundtrip() {
        with_clean_watch_state(|| {
            set_watch_commands(&["cargo clippy", "cargo test"]);
            let cmds = get_watch_commands();
            assert_eq!(cmds.len(), 2);
            assert_eq!(cmds[0], "cargo clippy");
            assert_eq!(cmds[1], "cargo test");
            clear_watch_command();
            assert!(get_watch_commands().is_empty());
        });
    }

    #[serial]
    #[test]
    fn test_get_watch_command_joins_multi_phase() {
        with_clean_watch_state(|| {
            set_watch_commands(&["cargo clippy", "cargo test"]);
            let display = get_watch_command();
            assert_eq!(
                display.as_deref(),
                Some("cargo clippy && cargo test"),
                "get_watch_command should join phases with &&"
            );
        });
    }

    #[serial]
    #[test]
    fn test_single_command_still_works() {
        with_clean_watch_state(|| {
            set_watch_command("cargo test");
            let cmds = get_watch_commands();
            assert_eq!(cmds.len(), 1, "single command should store one-element vec");
            assert_eq!(cmds[0], "cargo test");
            let display = get_watch_command();
            assert_eq!(display.as_deref(), Some("cargo test"));
        });
    }

    #[serial]
    #[test]
    fn test_clear_clears_multi_phase() {
        with_clean_watch_state(|| {
            set_watch_commands(&["a", "b", "c"]);
            assert_eq!(get_watch_commands().len(), 3);
            clear_watch_command();
            assert!(get_watch_commands().is_empty());
            assert!(get_watch_command().is_none());
        });
    }

    #[test]
    fn test_classify_watch_command_lint() {
        assert_eq!(classify_watch_command("cargo clippy"), "lint");
        assert_eq!(
            classify_watch_command("cargo clippy --all-targets -- -D warnings"),
            "lint"
        );
        assert_eq!(classify_watch_command("npx eslint ."), "lint");
        assert_eq!(classify_watch_command("ruff check ."), "lint");
        assert_eq!(classify_watch_command("npm run lint"), "lint");
    }

    #[test]
    fn test_classify_watch_command_test() {
        assert_eq!(classify_watch_command("cargo test"), "test");
        assert_eq!(classify_watch_command("npm test"), "test");
        assert_eq!(classify_watch_command("python -m pytest"), "test");
        assert_eq!(classify_watch_command("npx jest"), "test");
        assert_eq!(classify_watch_command("npx vitest"), "test");
    }

    #[test]
    fn test_classify_watch_command_general() {
        assert_eq!(classify_watch_command("cargo build"), "command");
        assert_eq!(classify_watch_command("make"), "command");
        assert_eq!(classify_watch_command("echo hello"), "command");
    }

    #[test]
    fn test_fix_prompt_includes_lint_hint() {
        let prompt = build_watch_fix_prompt("cargo clippy --all-targets", "warning: unused import");
        assert!(
            prompt.contains("lint"),
            "lint command prompt should mention lint: {prompt}"
        );
        assert!(
            prompt.contains("mechanical"),
            "lint prompt should mention mechanical fixes: {prompt}"
        );
    }

    #[test]
    fn test_fix_prompt_includes_test_hint() {
        let prompt = build_watch_fix_prompt("cargo test", "test result: FAILED");
        assert!(
            prompt.contains("test"),
            "test command prompt should mention test: {prompt}"
        );
        assert!(
            prompt.contains("intended behavior"),
            "test prompt should mention understanding behavior: {prompt}"
        );
    }

    #[test]
    fn test_fix_prompt_general_command_no_extra_hint() {
        let prompt = build_watch_fix_prompt("cargo build", "error: linking failed");
        assert!(
            prompt.contains("command failures"),
            "general command should say 'command failures': {prompt}"
        );
        // Should NOT contain the lint or test specific hints
        assert!(
            !prompt.contains("mechanical"),
            "general command should not have lint hint"
        );
        assert!(
            !prompt.contains("intended behavior"),
            "general command should not have test hint"
        );
    }

    #[serial]
    #[test]
    fn test_run_watch_after_prompt_empty_commands_returns_passed() {
        with_clean_watch_state(|| {
            // When no watch commands are set, should return passed immediately
            assert!(
                get_watch_commands().is_empty(),
                "precondition: no commands set"
            );
            // The function checks get_watch_commands() first and returns a passing
            // WatchResult if empty. We verify the guard condition.
        });
    }

    #[serial]
    #[test]
    fn auto_detect_watch_command_returns_lint_and_test_in_rust_project() {
        with_clean_watch_state(|| {
            // We're running from a directory with Cargo.toml, so this should detect Rust
            // After the Day 58 change, auto-detect defaults to lint+test (not test-only)
            let cmd = auto_detect_watch_command();
            assert!(
                cmd.is_some(),
                "should detect a watch command in a Rust project"
            );
            let cmd = cmd.unwrap();
            assert!(
                cmd.contains("clippy"),
                "auto-detect should include lint (clippy): {cmd}"
            );
            assert!(
                cmd.contains("cargo test"),
                "auto-detect should include test: {cmd}"
            );
            assert!(
                cmd.contains("&&"),
                "auto-detect should chain lint && test: {cmd}"
            );
        });
    }

    // --- tool_on_path / missing_watch_tool (#557): paired positive/negative
    // cases per Day 122's rule — verify the discriminator both fires and
    // stays silent.

    #[test]
    fn tool_on_path_finds_sh() {
        // `sh` is on PATH everywhere CI runs (POSIX shells, Git Bash on Windows CI).
        assert!(tool_on_path("sh"), "sh should be found on PATH");
    }

    #[test]
    fn tool_on_path_rejects_nonexistent_tool() {
        assert!(
            !tool_on_path("definitely-not-a-real-tool-xyz"),
            "made-up tool must not be found on PATH"
        );
    }

    #[test]
    fn tool_on_path_rejects_empty_string() {
        assert!(!tool_on_path(""), "empty token is never a valid tool");
    }

    #[test]
    fn tool_on_path_checks_explicit_paths_directly() {
        // A token containing a path separator is checked for existence, not PATH.
        assert!(
            !tool_on_path("./definitely-not-a-real-script-xyz.sh"),
            "nonexistent relative path must be rejected"
        );
        #[cfg(unix)]
        assert!(
            tool_on_path("/bin/sh"),
            "absolute path to an existing binary must be accepted"
        );
    }

    #[test]
    fn missing_watch_tool_none_when_tool_exists() {
        assert_eq!(
            missing_watch_tool("sh -c 'echo hi'"),
            None,
            "sh is on PATH, so nothing should be reported missing"
        );
    }

    #[test]
    fn missing_watch_tool_reports_first_token_of_bad_command() {
        assert_eq!(
            missing_watch_tool("definitely-not-a-real-tool-xyz test"),
            Some("definitely-not-a-real-tool-xyz".to_string())
        );
    }

    #[test]
    fn missing_watch_tool_validates_each_phase_of_combo() {
        // First phase valid, second phase's tool missing → report the second.
        assert_eq!(
            missing_watch_tool("sh -c true && bogus-tool-xyz check"),
            Some("bogus-tool-xyz".to_string())
        );
        // Both phases valid → None (paired negative for the combo case).
        assert_eq!(missing_watch_tool("sh -c true && sh -c true"), None);
    }

    #[test]
    fn missing_watch_tool_empty_command_is_none() {
        // No tokens at all → nothing to validate, nothing missing.
        assert_eq!(missing_watch_tool(""), None);
        assert_eq!(missing_watch_tool("   "), None);
    }

    #[serial]
    #[test]
    fn detect_watch_all_command_returns_lint_and_test_for_rust() {
        with_clean_watch_state(|| {
            // We're running from a directory with Cargo.toml, so this should detect Rust
            let cmd = detect_watch_all_command();
            assert!(
                cmd.is_some(),
                "should detect a combined command in a Rust project"
            );
            let cmd = cmd.unwrap();
            assert!(
                cmd.contains("clippy"),
                "combined command should include lint (clippy): {cmd}"
            );
            assert!(
                cmd.contains("cargo test"),
                "combined command should include test: {cmd}"
            );
            assert!(
                cmd.contains("&&"),
                "combined command should chain with &&: {cmd}"
            );
        });
    }

    #[test]
    fn watch_subcommands_includes_all() {
        assert!(
            WATCH_SUBCOMMANDS.contains(&"all"),
            "WATCH_SUBCOMMANDS should include 'all'"
        );
    }

    #[serial]
    #[test]
    fn handle_watch_all_sets_combined_command() {
        with_clean_watch_state(|| {
            // A temp Rust project, so project-type detection sees a Cargo.toml
            // without the test having to move the process CWD (#780).
            let tmp = tempfile::TempDir::new().expect("create temp dir");
            std::fs::write(
                tmp.path().join("Cargo.toml"),
                "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
            )
            .expect("write temp Cargo.toml");

            handle_watch_in(tmp.path(), "/watch all");
            let cmd = get_watch_command();
            assert!(
                cmd.is_some(),
                "watch command should be set after /watch all"
            );
            let cmd = cmd.unwrap();
            assert!(
                cmd.contains("clippy") && cmd.contains("cargo test"),
                "watch all should set lint && test: {cmd}"
            );
            // Verify multi-phase: should have 2 separate commands
            let phases = get_watch_commands();
            assert_eq!(
                phases.len(),
                2,
                "watch all should set 2 separate phases: {phases:?}"
            );
            assert!(
                phases[0].contains("clippy"),
                "first phase should be lint: {}",
                phases[0]
            );
            assert!(
                phases[1].contains("test"),
                "second phase should be test: {}",
                phases[1]
            );
        });
    }

    #[test]
    fn watch_subcommands_includes_lint() {
        assert!(
            WATCH_SUBCOMMANDS.contains(&"lint"),
            "WATCH_SUBCOMMANDS should include 'lint'"
        );
    }

    #[serial]
    #[test]
    fn handle_watch_lint_sets_lint_only_command() {
        with_clean_watch_state(|| {
            // A temp Rust project, so project-type detection sees a Cargo.toml
            // without the test having to move the process CWD (#780).
            let tmp = tempfile::TempDir::new().expect("create temp dir");
            std::fs::write(
                tmp.path().join("Cargo.toml"),
                "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
            )
            .expect("write temp Cargo.toml");

            handle_watch_in(tmp.path(), "/watch lint");
            let cmd = get_watch_command();
            assert!(
                cmd.is_some(),
                "watch command should be set after /watch lint"
            );
            let cmd = cmd.unwrap();
            assert!(
                cmd.contains("clippy"),
                "watch lint should set lint command: {cmd}"
            );
            assert!(
                !cmd.contains("cargo test"),
                "watch lint should NOT include test: {cmd}"
            );
        });
    }

    #[serial]
    #[test]
    fn handle_watch_bare_sets_lint_and_test() {
        with_clean_watch_state(|| {
            // A temp Rust project, so project-type detection sees a Cargo.toml
            // without the test having to move the process CWD (#780).
            let tmp = tempfile::TempDir::new().expect("create temp dir");
            std::fs::write(
                tmp.path().join("Cargo.toml"),
                "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
            )
            .expect("write temp Cargo.toml");

            handle_watch_in(tmp.path(), "/watch");
            let cmd = get_watch_command();
            assert!(
                cmd.is_some(),
                "watch command should be set after bare /watch"
            );
            let cmd = cmd.unwrap();
            // Accept either clippy or cargo check as valid lint detection
            let has_lint = cmd.contains("clippy") || cmd.contains("cargo check");
            let has_test = cmd.contains("cargo test") || cmd.contains("test");
            assert!(
                has_lint && has_test,
                "bare /watch should set lint && test: {cmd}"
            );
            // Verify multi-phase
            let phases = get_watch_commands();
            assert_eq!(
                phases.len(),
                2,
                "bare /watch should set 2 phases: {phases:?}"
            );
        });
    }

    #[test]
    fn detect_watch_all_phases_returns_separate_commands() {
        // Use a temp directory with Cargo.toml so the test doesn't depend on CWD.
        let tmp = std::env::temp_dir().join("yoyo_test_watch_phases_separate");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(
            tmp.join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .expect("failed to write temp Cargo.toml");

        let phases = detect_watch_all_phases_for_dir(&tmp);
        assert!(phases.is_some(), "should detect phases in a Rust project");
        let phases = phases.unwrap();
        assert_eq!(
            phases.len(),
            2,
            "should have lint + test phases: {phases:?}"
        );
        let has_lint = phases[0].contains("clippy") || phases[0].contains("check");
        assert!(
            has_lint,
            "first phase should be lint (clippy or check): {}",
            phases[0]
        );
        assert!(
            phases[1].contains("test"),
            "second phase should be test: {}",
            phases[1]
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[serial]
    #[test]
    fn handle_watch_custom_command_single_phase() {
        clear_watch_command();
        handle_watch("/watch make check");
        let phases = get_watch_commands();
        assert_eq!(
            phases.len(),
            1,
            "custom command should be single-phase: {phases:?}"
        );
        assert_eq!(phases[0], "make check");
        clear_watch_command();
    }

    #[test]
    fn detect_watch_all_phases_for_dir_rust_project() {
        // Create a temp directory with a Cargo.toml to simulate a Rust project.
        // This test is independent of CWD — it always uses the temp dir.
        let tmp = std::env::temp_dir().join("yoyo_test_watch_rust_project");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(
            tmp.join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .expect("failed to write temp Cargo.toml");

        let phases = detect_watch_all_phases_for_dir(&tmp);
        assert!(
            phases.is_some(),
            "should detect phases for a Rust project dir"
        );
        let phases = phases.unwrap();
        assert_eq!(
            phases.len(),
            2,
            "Rust project should have lint + test phases: {phases:?}"
        );
        let has_lint = phases[0].contains("clippy") || phases[0].contains("check");
        assert!(
            has_lint,
            "first phase should be lint (clippy or check): {}",
            phases[0]
        );
        assert!(
            phases[1].contains("test"),
            "second phase should be test: {}",
            phases[1]
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn detect_watch_all_phases_for_dir_empty_dir_returns_none() {
        // An empty directory should not detect any project type.
        let tmp = std::env::temp_dir().join("yoyo_test_watch_empty_dir");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::create_dir_all(&tmp);

        let phases = detect_watch_all_phases_for_dir(&tmp);
        // Unknown project type may still match Makefile detection etc.,
        // but an empty dir should yield None or a minimal result
        if let Some(ref p) = phases {
            // If something is detected, it's fine — just verify it's non-empty
            assert!(!p.is_empty(), "detected phases should not be empty");
        }
        // Either None or Some with valid commands — both are acceptable
        // The key is that it doesn't panic.

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn detect_watch_all_command_for_dir_rust_project() {
        let tmp = std::env::temp_dir().join("yoyo_test_watch_cmd_rust");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(
            tmp.join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .expect("failed to write temp Cargo.toml");

        let cmd = detect_watch_all_command_for_dir(&tmp);
        assert!(cmd.is_some(), "should detect command for Rust project");
        let cmd = cmd.unwrap();
        assert!(
            cmd.contains("&&"),
            "Rust project should chain lint && test: {cmd}"
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // -----------------------------------------------------------------------
    // Structured Rust compiler error parsing tests
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // #859: ANSI-coloured cargo output must parse identically to uncoloured.
    //
    // Both fixtures below were CAPTURED VERBATIM from a real scratch crate
    // (cargo 1.98.0 / rustc 1.98.0, a deliberate E0308), the coloured one with
    // `CARGO_TERM_COLOR=always cargo build 2>&1 | cat > file` and the plain one
    // with `CARGO_TERM_COLOR=never`. They are not hand-typed: round 81 and
    // round 87 each lost a bet by asserting an external tool's output format
    // without running it once.
    // -----------------------------------------------------------------------

    /// The real coloured capture, escape bytes and all.
    const ANSI_E0308_CAPTURE: &str = "\x1b[1m\x1b[92m   Compiling\x1b[0m ansi_probe v0.1.0 (/tmp/ansi_probe)\n\
\x1b[1m\x1b[91merror[E0308]\x1b[0m\x1b[1m: mismatched types\x1b[0m\n\
 \x1b[1m\x1b[94m--> \x1b[0msrc/main.rs:2:18\n\
  \x1b[1m\x1b[94m|\x1b[0m\n\
\x1b[1m\x1b[94m2\x1b[0m \x1b[1m\x1b[94m|\x1b[0m     let x: i32 = \"not an int\";\n\
  \x1b[1m\x1b[94m|\x1b[0m            \x1b[1m\x1b[94m---\x1b[0m   \x1b[1m\x1b[91m^^^^^^^^^^^^\x1b[0m \x1b[1m\x1b[91mexpected `i32`, found `&str`\x1b[0m\n\
  \x1b[1m\x1b[94m|\x1b[0m            \x1b[1m\x1b[94m|\x1b[0m\n\
  \x1b[1m\x1b[94m|\x1b[0m            \x1b[1m\x1b[94mexpected due to this\x1b[0m\n\
\n\
\x1b[1mFor more information about this error, try `rustc --explain E0308`.\x1b[0m\n\
\x1b[1m\x1b[91merror\x1b[0m: could not compile `ansi_probe` (bin \"ansi_probe\") due to 1 previous error\n\
";

    /// The same build, `CARGO_TERM_COLOR=never`.
    const PLAIN_E0308_CAPTURE: &str = "   Compiling ansi_probe v0.1.0 (/tmp/ansi_probe)\n\
error[E0308]: mismatched types\n\
 --> src/main.rs:2:18\n\
  |\n\
2 |     let x: i32 = \"not an int\";\n\
  |            ---   ^^^^^^^^^^^^ expected `i32`, found `&str`\n\
  |            |\n\
  |            expected due to this\n\
\n\
For more information about this error, try `rustc --explain E0308`.\n\
error: could not compile `ansi_probe` (bin \"ansi_probe\") due to 1 previous error\n\
";

    #[test]
    fn parse_rust_errors_ansi_coloured_matches_the_uncoloured_twin_859() {
        // Sanity: the fixture really does carry escapes, or this test is
        // vacuous green over a fixture that lost them in transcription.
        assert!(
            ANSI_E0308_CAPTURE.contains('\u{1b}'),
            "coloured fixture must actually contain ESC bytes"
        );
        assert!(
            !PLAIN_E0308_CAPTURE.contains('\u{1b}'),
            "plain fixture must not contain ESC bytes"
        );

        let coloured = parse_rust_errors(ANSI_E0308_CAPTURE);
        let plain = parse_rust_errors(PLAIN_E0308_CAPTURE);

        // The defect: this was 0 before the fix while `plain` was 1.
        assert_eq!(
            plain.len(),
            1,
            "uncoloured capture should yield exactly one error"
        );
        assert_eq!(
            coloured.len(),
            plain.len(),
            "coloured output must yield the same number of errors as uncoloured"
        );

        for (c, p) in coloured.iter().zip(plain.iter()) {
            assert_eq!(c.code, p.code, "code must match the uncoloured twin");
            assert_eq!(c.message, p.message, "message must match");
            assert_eq!(c.file, p.file, "file must match");
            assert_eq!(c.line, p.line, "line must match");
            assert_eq!(c.category, p.category, "category must match");
        }

        // And the values themselves, so a regression that broke BOTH fixtures
        // identically could not pass by agreeing with itself.
        assert_eq!(coloured[0].code.as_deref(), Some("E0308"));
        assert_eq!(coloured[0].message, "mismatched types");
        assert_eq!(coloured[0].file.as_deref(), Some("src/main.rs"));
        assert_eq!(coloured[0].line, Some(2));
        assert_eq!(coloured[0].category, ErrorCategory::Type);

        // The stored message goes into an API prompt: no escape bytes there.
        assert!(
            !coloured[0].message.contains('\u{1b}'),
            "stored message must not carry escape codes into the fix prompt"
        );
    }

    /// Near-miss guard: uncoloured input must be BYTE-IDENTICAL to before.
    ///
    /// A discriminator tested only on the side that fires is vacuous green, and
    /// this parser's whole history is discriminators pointed at the wrong input.
    #[test]
    fn parse_rust_errors_uncoloured_input_is_unchanged_859() {
        let output = "error[E0382]: borrow of moved value: `x`\n  \
                      --> src/main.rs:10:5\n";
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code.as_deref(), Some("E0382"));
        assert_eq!(errors[0].message, "borrow of moved value: `x`");
        assert_eq!(errors[0].file.as_deref(), Some("src/main.rs"));
        assert_eq!(errors[0].line, Some(10));
        assert_eq!(errors[0].category, ErrorCategory::Borrow);
    }

    #[test]
    fn strip_ansi_escapes_table_859() {
        use std::borrow::Cow;

        // 1. No ESC -> byte-identical AND borrowed (the common path must not
        //    allocate). Asserted with assert_eq! on the whole string, never a
        //    `contains`.
        for plain in [
            "error[E0308]: mismatched types",
            "",
            "  --> src/main.rs:2:18",
            "thread 'tests::t' (4861) panicked at src/main.rs:7:9:",
            "let s = \"a [1m b\";",
        ] {
            let got = strip_ansi_escapes(plain);
            assert_eq!(got, plain, "uncoloured line must be byte-identical");
            assert!(
                matches!(got, Cow::Borrowed(_)),
                "uncoloured line must borrow, not allocate: {plain:?}"
            );
        }

        // 2. CSI/SGR, the one class actually observed in cargo output.
        let cases: &[(&str, &str)] = &[
            (
                "\u{1b}[1m\u{1b}[91merror[E0308]\u{1b}[0m\u{1b}[1m: mismatched types\u{1b}[0m",
                "error[E0308]: mismatched types",
            ),
            (
                " \u{1b}[1m\u{1b}[94m--> \u{1b}[0msrc/main.rs:2:18",
                " --> src/main.rs:2:18",
            ),
            ("\u{1b}[0m", ""),
            ("\u{1b}[38;5;9mred\u{1b}[0m", "red"),
        ];
        for (input, want) in cases {
            assert_eq!(&strip_ansi_escapes(input), want, "CSI strip: {input:?}");
        }

        // 3. Unterminated CSI at end of line: must not panic and must not
        //    swallow the line. We drop the `ESC [` introducer and keep the rest.
        assert_eq!(&strip_ansi_escapes("tail\u{1b}["), "tail");
        assert_eq!(&strip_ansi_escapes("tail\u{1b}"), "tail");
        assert_eq!(&strip_ansi_escapes("a\u{1b}[38;5"), "a");

        // 4. A non-CSI ESC form (e.g. OSC) is NOT a class we observed. It must
        //    degrade quietly rather than eat the line.
        let osc = strip_ansi_escapes("\u{1b}]8;;http://x\u{7}link");
        assert!(
            osc.contains("link"),
            "non-CSI ESC must not swallow the line"
        );

        // 5. Multi-byte input must survive (rule #250 - no raw byte indexing).
        assert_eq!(
            &strip_ansi_escapes("\u{1b}[1m\u{1f419} octopus\u{1b}[0m"),
            "\u{1f419} octopus"
        );
    }

    #[test]
    fn parse_rust_errors_borrow_checker() {
        let output = r#"error[E0382]: borrow of moved value: `x`
  --> src/main.rs:10:5
   |
10 |     println!("{}", x);
   |                    ^ value borrowed here after move
"#;
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code.as_deref(), Some("E0382"));
        assert_eq!(errors[0].category, ErrorCategory::Borrow);
        assert_eq!(errors[0].file.as_deref(), Some("src/main.rs"));
        assert_eq!(errors[0].line, Some(10));
    }

    #[test]
    fn parse_rust_errors_type_mismatch() {
        let output = r#"error[E0308]: mismatched types
  --> src/lib.rs:42:5
   |
42 |     let x: u32 = "hello";
   |                  ^^^^^^^ expected `u32`, found `&str`
"#;
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code.as_deref(), Some("E0308"));
        assert_eq!(errors[0].category, ErrorCategory::Type);
        assert_eq!(errors[0].file.as_deref(), Some("src/lib.rs"));
        assert_eq!(errors[0].line, Some(42));
    }

    #[test]
    fn parse_rust_errors_lifetime() {
        let output = "error[E0106]: missing lifetime specifier\n  --> src/foo.rs:7:20\n";
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].category, ErrorCategory::Lifetime);
        assert_eq!(errors[0].file.as_deref(), Some("src/foo.rs"));
        assert_eq!(errors[0].line, Some(7));
    }

    #[test]
    fn parse_rust_errors_import() {
        let output = "error[E0433]: failed to resolve: use of undeclared crate or module `foo`\n  --> src/bar.rs:1:5\n";
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].category, ErrorCategory::Import);
    }

    #[test]
    fn parse_rust_errors_unused_warning() {
        let output = "warning: unused import: `std::io`\n  --> src/main.rs:3:5\n";
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].category, ErrorCategory::Unused);
        assert!(errors[0].code.is_none());
    }

    #[test]
    fn parse_rust_errors_unused_with_code() {
        let output = "warning[unused_imports]: unused import: `std::io`\n  --> src/main.rs:3:5\n";
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].category, ErrorCategory::Unused);
    }

    #[test]
    fn parse_rust_errors_test_panic() {
        let output = "thread 'tests::my_test' panicked at 'assertion failed: `(left == right)`\n  left: `1`,\n right: `2`', src/lib.rs:99:9\n";
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].category, ErrorCategory::TestAssertion);
    }

    /// Captured VERBATIM from a real `cargo test` failure (rustc 1.65+ format),
    /// not hand-typed. Round 81's own losing bet asserted an external tool's
    /// output format without running it once; this fixture is the correction.
    ///
    /// The modern header carries the location and puts the assertion on the
    /// NEXT line. Before this fix both were thrown away: `file`/`line` were
    /// `None` while `src/main.rs:7:9` sat right there in the header, so
    /// `extract_error_source_context` could pull no source, and the fix agent
    /// was told a test panicked but never what the assertion was.
    #[test]
    fn parse_rust_errors_modern_panic_carries_location_and_assertion() {
        let output = "thread 'tests::t' (4861) panicked at src/main.rs:7:9:\n\
                      assertion `left == right` failed: math is broken\n  \
                      left: 4\n right: 5\n\
                      note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n";
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].category, ErrorCategory::TestAssertion);
        assert_eq!(errors[0].file.as_deref(), Some("src/main.rs"));
        assert_eq!(errors[0].line, Some(7));
        assert!(
            errors[0]
                .message
                .contains("assertion `left == right` failed: math is broken"),
            "the assertion text must reach the fix agent: {:?}",
            errors[0].message
        );
    }

    /// Near-miss guard: the pre-1.65 shape must stay BYTE-IDENTICAL. A
    /// discriminator tested only on the side that fires is vacuous green.
    #[test]
    fn parse_rust_errors_legacy_panic_shape_is_byte_identical() {
        let output = "thread 'tests::my_test' panicked at 'assertion failed: `(left == right)`\n  left: `1`,\n right: `2`', src/lib.rs:99:9\n";
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].file, None, "legacy shape must not gain a file");
        assert_eq!(errors[0].line, None, "legacy shape must not gain a line");
        assert_eq!(
            errors[0].message,
            "thread 'tests::my_test' panicked at 'assertion failed: `(left == right)`"
        );
    }

    /// #860: a diagnostic with no location of its own must NOT absorb the
    /// `-->` belonging to the next diagnostic.
    ///
    /// **This fixture is a real capture, not fabricated** (dated 2026-08-30,
    /// cargo 1.x on this runner). Produced in a scratch crate under `/tmp` —
    /// never this repo, per `run_git`'s `#[cfg(test)]` destructive guard and
    /// #780's cwd lessons — by a `Cargo.toml` carrying one bogus key plus a
    /// `let x: i32 = "not an int";`, captured with
    /// `CARGO_TERM_COLOR=never cargo build 2>&1`. The shape is ordinary rather
    /// than exotic: **any** typo'd manifest key plus **any** type error yields
    /// it, and the located neighbour lands 4 lines below the location-less
    /// header — squarely inside `extract_location`'s 5-line window.
    ///
    /// Before the bound, `warning: Cargo.toml: unused manifest key: …` came
    /// back carrying `src/main.rs:2` — a confident pointer at innocent code,
    /// which `build_watch_fix_prompt` then feeds to
    /// `extract_error_source_context` to quote the wrong source lines at the
    /// fix agent.
    #[test]
    fn parse_rust_errors_location_less_diagnostic_does_not_absorb_its_neighbour_860() {
        let output = "\
warning: Cargo.toml: unused manifest key: package.bogus_key_here
warning: `loc860` (manifest) generated 1 warning
   Compiling loc860 v0.1.0 (/tmp/loc860.bDyper)
error[E0308]: mismatched types
 --> src/main.rs:2:18
";

        // Anti-vacuous: pin that the fixture really has the shape the test
        // claims, so a broken parser cannot pass by having both sides agree on
        // nothing. The manifest warning owns no `-->`; the E0308 header does,
        // on the very next line; and the two sit inside one 5-line window.
        let raw: Vec<&str> = output.lines().collect();
        assert!(
            raw[0].starts_with("warning: Cargo.toml: unused manifest key"),
            "fixture drifted: first line is not the location-less warning: {:?}",
            raw[0],
        );
        assert!(
            !raw[1].trim().starts_with("--> ") && !raw[2].trim().starts_with("--> "),
            "fixture drifted: the location-less warning must own no `-->`",
        );
        assert!(
            raw[3].starts_with("error[E0308]") && raw[4].trim().starts_with("--> "),
            "fixture drifted: the located neighbour must carry its own `-->`",
        );
        let arrow_at = raw
            .iter()
            .position(|l| l.trim().starts_with("--> "))
            .expect("fixture drifted: no `-->` anywhere");
        assert!(
            (1..6).contains(&arrow_at),
            "fixture drifted: the neighbour's `-->` is at {arrow_at}, outside the 5-line \
             window scanned from the location-less header at index 0",
        );

        let errors = parse_rust_errors(output);
        assert_eq!(
            errors.len(),
            2,
            "expected the manifest warning and the E0308: {errors:?}",
        );

        // The fix: the location-less warning keeps its honest `None`.
        assert_eq!(
            errors[0].file, None,
            "location-less warning absorbed a neighbour's file: {:?}",
            errors[0],
        );
        assert_eq!(
            errors[0].line, None,
            "location-less warning absorbed a neighbour's line: {:?}",
            errors[0],
        );

        // The neighbour keeps its OWN location — the bound must not cost the
        // located diagnostic anything.
        assert_eq!(errors[1].code.as_deref(), Some("E0308"));
        assert_eq!(errors[1].file.as_deref(), Some("src/main.rs"));
        assert_eq!(errors[1].line, Some(2));
    }

    /// Near-miss guard for #860, and the half that matters: the bound's whole
    /// risk is cutting the window short, so a genuine `-->` at **every**
    /// in-window distance must still be found.
    ///
    /// A discriminator tested only on the side that fires is vacuous green.
    #[test]
    fn parse_rust_errors_located_diagnostic_still_finds_its_arrow_at_every_distance_860() {
        for distance in 1..=5usize {
            let filler = "  |\n".repeat(distance - 1);
            let output =
                format!("error[E0308]: mismatched types\n{filler} --> src/main.rs:42:18\n  |\n");
            let errors = parse_rust_errors(&output);
            assert_eq!(errors.len(), 1, "distance {distance}: {errors:?}");
            assert_eq!(
                errors[0].file.as_deref(),
                Some("src/main.rs"),
                "distance {distance}: bound cut the window short",
            );
            assert_eq!(
                errors[0].line,
                Some(42),
                "distance {distance}: bound cut the window short",
            );
        }
    }

    /// Near-miss guard for #860: `thread '` is now also a bound marker, so a
    /// panic must not truncate its own parse. Both shapes, byte-identical to
    /// the pre-#860 behaviour pinned by the two tests above.
    #[test]
    fn parse_rust_errors_panic_shapes_unaffected_by_the_header_bound_860() {
        let modern = "thread 'tests::t' (4861) panicked at src/main.rs:7:9:\n\
                      assertion `left == right` failed\n";
        let errors = parse_rust_errors(modern);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].file.as_deref(), Some("src/main.rs"));
        assert_eq!(errors[0].line, Some(7));

        let legacy = "thread 'tests::t' panicked at 'boom', src/lib.rs:99:9\n";
        let errors = parse_rust_errors(legacy);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].file, None, "legacy shape must not gain a file");
        assert_eq!(errors[0].line, None, "legacy shape must not gain a line");
    }

    #[test]
    fn modern_panic_location_table() {
        // Fires: real modern headers, with and without the thread id.
        assert_eq!(
            modern_panic_location("thread 'tests::t' (4861) panicked at src/main.rs:7:9:"),
            Some(("src/main.rs".to_string(), 7))
        );
        assert_eq!(
            modern_panic_location("thread 'main' panicked at src/a/b.rs:120:5:"),
            Some(("src/a/b.rs".to_string(), 120))
        );
        // Must NOT fire: legacy shape (no trailing colon), quoted tail,
        // non-numeric line/col, and a message that merely ends in a colon.
        assert_eq!(
            modern_panic_location("thread 'x' panicked at 'assertion failed', src/lib.rs:99:9"),
            None
        );
        assert_eq!(
            modern_panic_location("thread 'x' panicked at 'boom':"),
            None
        );
        assert_eq!(modern_panic_location("thread 'x' panicked at wat:"), None);
        assert_eq!(
            modern_panic_location("thread 'x' panicked at src/a.rs:oops:9:"),
            None
        );
    }

    #[test]
    fn parse_rust_errors_unresolved_name_no_code() {
        let output = "error: cannot find value `foo` in this scope\n  --> src/main.rs:5:10\n";
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].category, ErrorCategory::Import);
        assert!(errors[0].code.is_none());
    }

    #[test]
    fn parse_rust_errors_empty_output() {
        let errors = parse_rust_errors("");
        assert!(errors.is_empty());
    }

    #[test]
    fn parse_rust_errors_non_rust_output() {
        let output = "npm ERR! code E404\nnpm ERR! 404 Not Found\nSome random build output\n";
        let errors = parse_rust_errors(output);
        assert!(
            errors.is_empty(),
            "non-Rust output should not parse: {errors:?}"
        );
    }

    #[test]
    fn parse_rust_errors_skips_aborting() {
        let output = "error[E0308]: mismatched types\n  --> src/lib.rs:1:1\nerror: aborting due to previous error\n";
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1, "should skip 'aborting' lines");
        assert_eq!(errors[0].code.as_deref(), Some("E0308"));
    }

    #[test]
    fn parse_rust_errors_mixed_errors() {
        let output = r#"error[E0382]: borrow of moved value: `x`
  --> src/main.rs:10:5
error[E0308]: mismatched types
  --> src/lib.rs:20:10
warning: unused import: `std::io`
  --> src/foo.rs:1:5
thread 'tests::broken' panicked at 'assertion failed'
error: aborting due to 2 previous errors
"#;
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 4, "should find 4 errors: {errors:?}");
        assert_eq!(errors[0].category, ErrorCategory::Borrow);
        assert_eq!(errors[1].category, ErrorCategory::Type);
        assert_eq!(errors[2].category, ErrorCategory::Unused);
        assert_eq!(errors[3].category, ErrorCategory::TestAssertion);
    }

    #[test]
    fn error_category_hint_returns_nonempty() {
        let categories = [
            ErrorCategory::Borrow,
            ErrorCategory::Type,
            ErrorCategory::Lifetime,
            ErrorCategory::Import,
            ErrorCategory::Unused,
            ErrorCategory::TestAssertion,
            ErrorCategory::Syntax,
            ErrorCategory::Other,
        ];
        for cat in &categories {
            let hint = error_category_hint(cat);
            assert!(!hint.is_empty(), "hint for {:?} should not be empty", cat);
        }
    }

    #[test]
    fn error_category_label_is_consistent() {
        assert_eq!(ErrorCategory::Borrow.label(), "borrow");
        assert_eq!(ErrorCategory::Type.label(), "type");
        assert_eq!(ErrorCategory::Lifetime.label(), "lifetime");
        assert_eq!(ErrorCategory::Import.label(), "import");
        assert_eq!(ErrorCategory::Unused.label(), "unused");
        assert_eq!(ErrorCategory::TestAssertion.label(), "test_assertion");
        assert_eq!(ErrorCategory::Syntax.label(), "syntax");
        assert_eq!(ErrorCategory::Other.label(), "other");
    }

    #[test]
    fn build_watch_fix_prompt_includes_structured_summary() {
        let output = "error[E0382]: borrow of moved value: `x`\n  --> src/main.rs:10:5\n\
                      error[E0308]: mismatched types\n  --> src/lib.rs:20:10\n";
        let prompt = build_watch_fix_prompt("cargo build", output);
        assert!(
            prompt.contains("Parsed 2 Rust error"),
            "should include structured summary: {prompt}"
        );
        assert!(
            prompt.contains("borrow") && prompt.contains("type"),
            "should include category counts: {prompt}"
        );
        assert!(
            prompt.contains("Hint:"),
            "should include targeted hint: {prompt}"
        );
    }

    #[test]
    fn build_watch_fix_prompt_non_rust_falls_through() {
        let output = "Some random output that isn't Rust compiler output\nBuild failed!\n";
        let prompt = build_watch_fix_prompt("make build", output);
        assert!(
            prompt.contains("Please fix"),
            "non-Rust output should use generic prompt: {prompt}"
        );
        assert!(
            !prompt.contains("Parsed"),
            "non-Rust output should not have structured section: {prompt}"
        );
    }

    #[test]
    fn build_error_summary_empty_returns_none() {
        assert!(build_error_summary_for(&[], WatchLanguage::Rust).is_none());
    }

    #[test]
    fn build_error_summary_shows_file_locations() {
        let errors = vec![CompilerError {
            code: Some("E0382".to_string()),
            message: "borrow of moved value".to_string(),
            file: Some("src/main.rs".to_string()),
            line: Some(42),
            category: ErrorCategory::Borrow,
        }];
        let summary =
            build_error_summary_for(&errors, WatchLanguage::Rust).expect("should return summary");
        assert!(
            summary.contains("src/main.rs:42"),
            "should include file:line: {summary}"
        );
    }

    #[test]
    fn build_error_summary_limits_detail_lines() {
        let errors: Vec<CompilerError> = (0..8)
            .map(|i| CompilerError {
                code: Some("E0308".to_string()),
                message: format!("error {i}"),
                file: Some(format!("src/file{i}.rs")),
                line: Some(i as u32 + 1),
                category: ErrorCategory::Type,
            })
            .collect();
        let summary =
            build_error_summary_for(&errors, WatchLanguage::Rust).expect("should return summary");
        assert!(
            summary.contains("and 3 more"),
            "should indicate truncated errors: {summary}"
        );
    }

    #[test]
    fn parse_rust_errors_clippy_warning() {
        let output = "warning: this expression creates a reference which is immediately dereferenced by the compiler\n  --> src/watch.rs:100:22\n";
        let errors = parse_rust_errors(output);
        assert_eq!(errors.len(), 1);
        // This is a general clippy warning, not specifically unused
        assert!(errors[0].code.is_none());
    }

    // -----------------------------------------------------------------------
    // TypeScript error parser tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_typescript_errors_tsc_type_error() {
        let output = "src/app.ts(15,3): error TS2345: Argument of type 'string' is not assignable to parameter of type 'number'.\n";
        let errors = parse_typescript_errors(output);
        assert_eq!(errors.len(), 1, "should parse one error");
        assert_eq!(errors[0].code.as_deref(), Some("TS2345"));
        assert_eq!(errors[0].file.as_deref(), Some("src/app.ts"));
        assert_eq!(errors[0].line, Some(15));
        assert_eq!(errors[0].category, ErrorCategory::Type);
        assert!(errors[0].message.contains("not assignable"));
    }

    #[test]
    fn parse_typescript_errors_tsc_import_error() {
        let output = "src/index.ts(1,22): error TS2307: Cannot find module 'nonexistent' or its corresponding type declarations.\n";
        let errors = parse_typescript_errors(output);
        assert_eq!(errors.len(), 1, "should parse one import error");
        assert_eq!(errors[0].code.as_deref(), Some("TS2307"));
        assert_eq!(errors[0].category, ErrorCategory::Import);
        assert_eq!(errors[0].file.as_deref(), Some("src/index.ts"));
    }

    #[test]
    fn parse_typescript_errors_eslint_output() {
        let output = "\
/home/user/project/src/utils.ts:10:5: warning 'foo' is defined but never used  no-unused-vars
/home/user/project/src/utils.ts:20:1: error Parsing error: Unexpected token
";
        let errors = parse_typescript_errors(output);
        assert_eq!(
            errors.len(),
            2,
            "should parse two eslint errors: {errors:?}"
        );
        // First: unused variable warning
        assert_eq!(errors[0].category, ErrorCategory::Unused);
        assert_eq!(
            errors[0].file.as_deref(),
            Some("/home/user/project/src/utils.ts")
        );
        assert_eq!(errors[0].line, Some(10));
        // Second: parsing error
        assert_eq!(errors[1].category, ErrorCategory::Syntax);
    }

    #[test]
    fn parse_typescript_errors_jest_failure() {
        let output = "\
FAIL src/components/Button.test.tsx
  ● Button component › should render correctly

    expect(received).toBe(expected)
";
        let errors = parse_typescript_errors(output);
        assert!(
            errors.len() >= 2,
            "should parse FAIL + assertion: {errors:?}"
        );
        // FAIL line
        let fail = errors.iter().find(|e| e.message.contains("Test suite"));
        assert!(fail.is_some(), "should have FAIL entry");
        assert_eq!(fail.unwrap().category, ErrorCategory::TestAssertion);
        // ● line
        let assertion = errors.iter().find(|e| e.message.contains("●"));
        assert!(assertion.is_some(), "should have assertion entry");
        assert_eq!(assertion.unwrap().category, ErrorCategory::TestAssertion);
    }

    #[test]
    fn parse_typescript_errors_multiple_tsc_errors() {
        let output = "\
src/api.ts(5,10): error TS2304: Cannot find name 'Response'.
src/api.ts(12,3): error TS1005: ';' expected.
src/api.ts(20,7): error TS6133: 'unused' is declared but its value is never read.
";
        let errors = parse_typescript_errors(output);
        assert_eq!(errors.len(), 3, "should parse all three errors: {errors:?}");
        // TS2304 is a 2xxx error → Type by default
        assert_eq!(errors[0].category, ErrorCategory::Type);
        // TS1005 is 1xxx → Syntax
        assert_eq!(errors[1].category, ErrorCategory::Syntax);
        // TS6133 → Unused
        assert_eq!(errors[2].category, ErrorCategory::Unused);
    }

    /// Real `tsc --noEmit` output, piped (no TTY). Captured verbatim on
    /// 2026-08-29 from TypeScript 7.0.2 / node v22.23.2. This is the shape
    /// yoyo's own watch loop sees, and it carries NO escape bytes.
    const TSC_PLAIN_CAPTURE: &str = "bad.ts(1,7): error TS2322: Type 'string' is not assignable to type 'number'.\nbad.ts(2,33): error TS2322: Type 'number' is not assignable to type 'string'.\n";

    /// Real `tsc --pretty --noEmit` output, captured verbatim in the same run.
    /// `FORCE_COLOR=1 tsc --noEmit` produced a byte-identical string. Note the
    /// line shape differs from the plain capture (`file:line:col - error` vs
    /// `file(line,col): error`) *and* it carries CSI colour codes — two
    /// independent defects, which is why both halves of the #861 fix exist.
    const TSC_PRETTY_CAPTURE: &str = "\x1b[96mbad.ts\x1b[0m:\x1b[93m1\x1b[0m:\x1b[93m7\x1b[0m - \x1b[91merror\x1b[0m\x1b[90m TS2322: \x1b[0mType 'string' is not assignable to type 'number'.\n\n\x1b[7m1\x1b[0m const n: number = \"hello\";\n\x1b[7m \x1b[0m \x1b[91m      ~\x1b[0m\n\n\x1b[96mbad.ts\x1b[0m:\x1b[93m2\x1b[0m:\x1b[93m33\x1b[0m - \x1b[91merror\x1b[0m\x1b[90m TS2322: \x1b[0mType 'number' is not assignable to type 'string'.\n\n\x1b[7m2\x1b[0m function f(a: number): string { return a; }\n\x1b[7m \x1b[0m \x1b[91m                                ~~~~~~\x1b[0m\n\n\nFound 2 errors in the same file, starting at: bad.ts\x1b[90m:1\x1b[0m\n\n";

    /// #861: the coloured/pretty capture must parse. Before this fix it
    /// yielded ZERO errors, so `build_watch_fix_prompt` handed the fix agent a
    /// blank page — and `scripts/evolve.sh` then spends up to 10 more attempts
    /// on that blank page. Asserted at the emission point (the
    /// `Vec<CompilerError>` a caller receives), never on the stripper below.
    #[test]
    fn parse_typescript_errors_reads_the_real_pretty_coloured_capture() {
        // Anti-vacuous: a transcription slip that dropped the escapes would
        // make this test pass by agreeing with itself.
        assert!(
            TSC_PRETTY_CAPTURE.contains('\u{1b}'),
            "fixture must really carry escape bytes, or it proves nothing"
        );

        let errors = parse_typescript_errors(TSC_PRETTY_CAPTURE);
        assert_eq!(errors.len(), 2, "should parse both errors: {errors:?}");

        assert_eq!(errors[0].code.as_deref(), Some("TS2322"));
        assert_eq!(errors[0].file.as_deref(), Some("bad.ts"));
        assert_eq!(errors[0].line, Some(1));
        assert_eq!(
            errors[0].message,
            "Type 'string' is not assignable to type 'number'."
        );
        assert_eq!(errors[0].category, ErrorCategory::Type);

        assert_eq!(errors[1].code.as_deref(), Some("TS2322"));
        assert_eq!(errors[1].file.as_deref(), Some("bad.ts"));
        assert_eq!(errors[1].line, Some(2));
        assert_eq!(
            errors[1].message,
            "Type 'number' is not assignable to type 'string'."
        );

        // No escape byte may survive into any stored field: these go straight
        // into an API prompt, where escapes are pure token waste.
        for e in &errors {
            assert!(
                !e.message.contains('\u{1b}'),
                "escape leaked into message: {:?}",
                e.message
            );
            assert!(
                !e.file.as_deref().unwrap_or("").contains('\u{1b}'),
                "escape leaked into file: {:?}",
                e.file
            );
            assert!(
                !e.code.as_deref().unwrap_or("").contains('\u{1b}'),
                "escape leaked into code: {:?}",
                e.code
            );
        }
    }

    /// Near-miss guard, and the half that matters: the plain piped capture is
    /// the path yoyo actually takes and was ALREADY correct before #861. It
    /// must stay byte-identical — a discriminator tested only on the side that
    /// fires is vacuous green.
    #[test]
    fn parse_typescript_errors_plain_capture_is_byte_identical() {
        assert!(
            !TSC_PLAIN_CAPTURE.contains('\u{1b}'),
            "the piped capture must NOT carry escapes, or it is not the plain shape"
        );

        let errors = parse_typescript_errors(TSC_PLAIN_CAPTURE);
        assert_eq!(errors.len(), 2, "should parse both errors: {errors:?}");
        assert_eq!(errors[0].code.as_deref(), Some("TS2322"));
        assert_eq!(errors[0].file.as_deref(), Some("bad.ts"));
        assert_eq!(errors[0].line, Some(1));
        assert_eq!(
            errors[0].message,
            "Type 'string' is not assignable to type 'number'."
        );
        assert_eq!(errors[1].line, Some(2));
        assert_eq!(
            errors[1].message,
            "Type 'number' is not assignable to type 'string'."
        );
    }

    /// The two captures describe the same two errors, so the parser must agree
    /// about them. Strictly stronger than either test alone: this fails both
    /// when colour HIDES an error and when the pretty branch INVENTS one.
    #[test]
    fn parse_typescript_errors_pretty_and_plain_agree() {
        let plain = parse_typescript_errors(TSC_PLAIN_CAPTURE);
        let pretty = parse_typescript_errors(TSC_PRETTY_CAPTURE);
        assert_eq!(
            plain.len(),
            pretty.len(),
            "same errors, different rendering: {plain:?} vs {pretty:?}"
        );
        for (p, c) in plain.iter().zip(pretty.iter()) {
            assert_eq!(p.code, c.code);
            assert_eq!(p.file, c.file);
            assert_eq!(p.line, c.line);
            assert_eq!(p.message, c.message);
            assert_eq!(p.category, c.category);
        }
    }

    /// The pretty branch must not fire on prose that merely contains the
    /// marker: both location numbers have to parse. Near-miss guard against
    /// the over-matching direction.
    #[test]
    fn parse_typescript_errors_pretty_branch_needs_real_line_and_col() {
        let not_a_diagnostic = "see also:foo:bar - error TS9999: this is prose\n";
        let errors = parse_typescript_errors(not_a_diagnostic);
        assert!(
            errors.is_empty(),
            "prose must not parse as a diagnostic: {errors:?}"
        );
    }

    /// A Windows path keeps its drive-letter colon: the location is split from
    /// the right, so `C:\src\app.ts:12:5` yields the whole path as the file.
    #[test]
    fn parse_typescript_errors_pretty_windows_path_keeps_drive_letter() {
        let output = "C:\\src\\app.ts:12:5 - error TS2304: Cannot find name 'Foo'.\n";
        let errors = parse_typescript_errors(output);
        assert_eq!(errors.len(), 1, "should parse one error: {errors:?}");
        assert_eq!(errors[0].file.as_deref(), Some("C:\\src\\app.ts"));
        assert_eq!(errors[0].line, Some(12));
        assert_eq!(errors[0].code.as_deref(), Some("TS2304"));
    }

    // -----------------------------------------------------------------------
    // Python error parser tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_python_errors_pytest_failure() {
        let output = "\
FAILED tests/test_auth.py::test_login - AssertionError: expected 200 but got 401
FAILED tests/test_auth.py::test_signup - ValueError: invalid email
";
        let errors = parse_python_errors(output);
        assert_eq!(
            errors.len(),
            2,
            "should parse two pytest failures: {errors:?}"
        );
        assert_eq!(
            errors[0].file.as_deref(),
            Some("tests/test_auth.py"),
            "should extract file from test path"
        );
        assert!(errors[0].message.contains("AssertionError"));
        assert_eq!(errors[0].category, ErrorCategory::TestAssertion);
        assert!(errors[1].message.contains("ValueError"));
    }

    #[test]
    fn parse_python_errors_mypy_output() {
        let output = "\
src/models.py:42: error: Incompatible types in assignment (expression has type \"str\", variable has type \"int\")
src/views.py:15: error: Module \"flask\" has no attribute \"missing\"
";
        let errors = parse_python_errors(output);
        assert_eq!(errors.len(), 2, "should parse two mypy errors: {errors:?}");
        // First: type error
        assert_eq!(errors[0].file.as_deref(), Some("src/models.py"));
        assert_eq!(errors[0].line, Some(42));
        assert_eq!(errors[0].category, ErrorCategory::Type);
        // Second: attribute error (categorized as Other or Type)
        assert_eq!(errors[1].file.as_deref(), Some("src/views.py"));
        assert_eq!(errors[1].line, Some(15));
    }

    #[test]
    fn parse_python_errors_traceback() {
        let output = "\
Traceback (most recent call last):
  File \"app.py\", line 10, in main
    import nonexistent_module
ModuleNotFoundError: No module named 'nonexistent_module'
";
        let errors = parse_python_errors(output);
        assert!(
            !errors.is_empty(),
            "should parse at least one error from traceback: {errors:?}"
        );
        let import_err = errors.iter().find(|e| e.category == ErrorCategory::Import);
        assert!(
            import_err.is_some(),
            "should find an import error: {errors:?}"
        );
        let err = import_err.unwrap();
        assert_eq!(err.file.as_deref(), Some("app.py"));
        assert_eq!(err.line, Some(10));
        assert!(err.message.contains("ModuleNotFoundError"));
    }

    #[test]
    fn parse_python_errors_pytest_assertion_detail() {
        let output = "\
FAILED tests/test_math.py::test_add - AssertionError: assert 3 == 4
E       AssertionError: assert add(1, 2) == 4
E       assert 3 == 4
";
        let errors = parse_python_errors(output);
        assert!(
            errors.len() >= 2,
            "should parse FAILED line + E lines: {errors:?}"
        );
        // The FAILED line
        let failed = errors
            .iter()
            .find(|e| e.message.contains("AssertionError: assert 3 == 4"));
        assert!(failed.is_some(), "should have FAILED line: {errors:?}");
        // E lines
        let e_lines: Vec<_> = errors
            .iter()
            .filter(|e| e.message.starts_with("AssertionError") || e.message.contains("assert"))
            .collect();
        assert!(
            !e_lines.is_empty(),
            "should have assertion detail lines: {errors:?}"
        );
    }

    // -----------------------------------------------------------------------
    // build_watch_fix_prompt integration tests for TS and Python
    // -----------------------------------------------------------------------

    #[test]
    fn build_watch_fix_prompt_npm_test_typescript_errors() {
        let output = "src/app.ts(15,3): error TS2345: Argument of type 'string' is not assignable to parameter of type 'number'.";
        let prompt = build_watch_fix_prompt("npm test", output);
        assert!(
            prompt.contains("Parsed 1 TypeScript error"),
            "should show structured TypeScript summary: {prompt}"
        );
        assert!(
            prompt.contains("TS2345"),
            "should include error code: {prompt}"
        );
        assert!(
            prompt.contains("type"),
            "should include type hint: {prompt}"
        );
    }

    #[test]
    fn build_watch_fix_prompt_pytest_python_errors() {
        let output =
            "FAILED tests/test_auth.py::test_login - AssertionError: expected 200 but got 401";
        let prompt = build_watch_fix_prompt("pytest", output);
        assert!(
            prompt.contains("Parsed") && prompt.contains("Python error"),
            "should show structured Python summary: {prompt}"
        );
        assert!(
            prompt.contains("test_auth.py"),
            "should include file name: {prompt}"
        );
    }

    #[test]
    fn build_watch_fix_prompt_unknown_cmd_falls_through_to_generic() {
        let output = "Something went wrong but it's not any recognizable format";
        let prompt = build_watch_fix_prompt("make build", output);
        assert!(
            prompt.contains("Please fix the issues"),
            "non-matching output should get generic prompt: {prompt}"
        );
        assert!(
            !prompt.contains("Parsed"),
            "should not have structured summary for unrecognized output: {prompt}"
        );
    }

    #[test]
    fn build_watch_fix_prompt_tsc_uses_ts_hints() {
        let output = "src/index.ts(1,22): error TS2307: Cannot find module 'foo' or its corresponding type declarations.";
        let prompt = build_watch_fix_prompt("npx tsc --noEmit", output);
        assert!(
            prompt.contains("TypeScript"),
            "should identify as TypeScript: {prompt}"
        );
        assert!(
            prompt.contains("npm install") || prompt.contains("import path"),
            "should give TS-specific import hint: {prompt}"
        );
    }

    #[test]
    fn build_watch_fix_prompt_mypy_uses_python_hints() {
        let output =
            "src/models.py:42: error: Incompatible types in assignment (expression has type \"str\", variable has type \"int\")";
        let prompt = build_watch_fix_prompt("mypy src/", output);
        assert!(
            prompt.contains("Python"),
            "should identify as Python: {prompt}"
        );
        assert!(
            prompt.contains("type") || prompt.contains("annotation"),
            "should give Python-specific type hint: {prompt}"
        );
    }

    #[test]
    fn detect_watch_language_classification() {
        assert_eq!(detect_watch_language("cargo test"), WatchLanguage::Rust);
        assert_eq!(detect_watch_language("cargo clippy"), WatchLanguage::Rust);
        assert_eq!(detect_watch_language("npm test"), WatchLanguage::TypeScript);
        assert_eq!(detect_watch_language("npx tsc"), WatchLanguage::TypeScript);
        assert_eq!(
            detect_watch_language("npx eslint ."),
            WatchLanguage::TypeScript
        );
        assert_eq!(detect_watch_language("jest"), WatchLanguage::TypeScript);
        assert_eq!(detect_watch_language("vitest"), WatchLanguage::TypeScript);
        assert_eq!(detect_watch_language("pytest"), WatchLanguage::Python);
        assert_eq!(
            detect_watch_language("python -m pytest"),
            WatchLanguage::Python
        );
        assert_eq!(detect_watch_language("mypy src/"), WatchLanguage::Python);
        assert_eq!(detect_watch_language("ruff check ."), WatchLanguage::Python);
        assert_eq!(detect_watch_language("make build"), WatchLanguage::Unknown);
    }

    #[test]
    fn build_error_summary_for_typescript_label() {
        let errors = vec![CompilerError {
            code: Some("TS2345".to_string()),
            message: "type mismatch".to_string(),
            file: Some("src/app.ts".to_string()),
            line: Some(10),
            category: ErrorCategory::Type,
        }];
        let summary = build_error_summary_for(&errors, WatchLanguage::TypeScript)
            .expect("should return summary");
        assert!(
            summary.contains("TypeScript"),
            "should label as TypeScript: {summary}"
        );
        assert!(!summary.contains("Rust"), "should not say Rust: {summary}");
    }

    #[test]
    fn build_error_summary_for_python_label() {
        let errors = vec![CompilerError {
            code: None,
            message: "AssertionError: expected True".to_string(),
            file: Some("tests/test_foo.py".to_string()),
            line: None,
            category: ErrorCategory::TestAssertion,
        }];
        let summary =
            build_error_summary_for(&errors, WatchLanguage::Python).expect("should return summary");
        assert!(
            summary.contains("Python"),
            "should label as Python: {summary}",
        );
    }

    #[test]
    fn extract_error_source_context_empty_errors() {
        let result = extract_error_source_context(&[]);
        assert!(
            result.is_empty(),
            "empty errors should produce empty string"
        );
    }

    #[test]
    fn extract_error_source_context_nonexistent_files() {
        let errors = vec![CompilerError {
            code: Some("E0308".to_string()),
            message: "mismatched types".to_string(),
            file: Some("/tmp/nonexistent_file_xyz_12345.rs".to_string()),
            line: Some(10),
            category: ErrorCategory::Type,
        }];
        let result = extract_error_source_context(&errors);
        assert!(
            result.is_empty(),
            "non-existent files should produce empty string: {result}",
        );
    }

    #[test]
    fn extract_error_source_context_no_file_field() {
        let errors = vec![CompilerError {
            code: Some("E0308".to_string()),
            message: "mismatched types".to_string(),
            file: None,
            line: Some(10),
            category: ErrorCategory::Type,
        }];
        let result = extract_error_source_context(&errors);
        assert!(
            result.is_empty(),
            "errors without file field should produce empty string",
        );
    }

    #[test]
    fn extract_error_source_context_reads_real_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_source.rs");
        {
            let mut f = std::fs::File::create(&file_path).unwrap();
            for i in 1..=20 {
                writeln!(f, "// line {i}").unwrap();
            }
        }
        let errors = vec![CompilerError {
            code: Some("E0308".to_string()),
            message: "mismatched types".to_string(),
            file: Some(file_path.to_str().unwrap().to_string()),
            line: Some(10),
            category: ErrorCategory::Type,
        }];
        let result = extract_error_source_context(&errors);
        assert!(
            result.contains("## Relevant source context"),
            "should include header: {result}",
        );
        assert!(
            result.contains("around line 10"),
            "should indicate target line: {result}",
        );
        // Should include lines 5-15 (10 ± 5)
        assert!(
            result.contains("// line 5"),
            "should include line 5: {result}"
        );
        assert!(
            result.contains("// line 15"),
            "should include line 15: {result}",
        );
        // Should NOT include line 1 (out of ±5 window from line 10)
        assert!(
            !result.contains("   1: // line 1"),
            "should not include line 1: {result}",
        );
    }

    #[test]
    fn extract_error_source_context_deduplicates() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("dup_test.rs");
        {
            let mut f = std::fs::File::create(&file_path).unwrap();
            for i in 1..=20 {
                writeln!(f, "// line {i}").unwrap();
            }
        }
        let path_str = file_path.to_str().unwrap().to_string();
        let errors = vec![
            CompilerError {
                code: Some("E0308".to_string()),
                message: "error one".to_string(),
                file: Some(path_str.clone()),
                line: Some(10),
                category: ErrorCategory::Type,
            },
            CompilerError {
                code: Some("E0277".to_string()),
                message: "error two".to_string(),
                file: Some(path_str.clone()),
                line: Some(10), // same file+line
                category: ErrorCategory::Type,
            },
        ];
        let result = extract_error_source_context(&errors);
        // Should only appear once
        let count = result.matches("around line 10").count();
        assert_eq!(count, 1, "same file+line should appear only once: {result}");
    }

    #[test]
    fn extract_error_source_context_respects_cap() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        // Create many files with long lines to exceed the 3KB cap
        let mut errors = Vec::new();
        for i in 0..50 {
            let file_path = dir.path().join(format!("file_{i}.rs"));
            {
                let mut f = std::fs::File::create(&file_path).unwrap();
                for j in 1..=20 {
                    // Long lines to fill up the budget quickly
                    writeln!(f, "// line {j} {}", "x".repeat(80)).unwrap();
                }
            }
            errors.push(CompilerError {
                code: Some("E0308".to_string()),
                message: format!("error in file {i}"),
                file: Some(file_path.to_str().unwrap().to_string()),
                line: Some(10),
                category: ErrorCategory::Type,
            });
        }
        let result = extract_error_source_context(&errors);
        // The result should be capped at roughly SOURCE_CONTEXT_MAX_BYTES
        assert!(
            result.len() <= SOURCE_CONTEXT_MAX_BYTES + 200, // small margin for the header
            "result should respect cap: got {} bytes",
            result.len(),
        );
        // But should have at least some content
        assert!(
            !result.is_empty(),
            "should include at least one file snippet",
        );
    }

    #[test]
    fn build_watch_fix_prompt_includes_source_context() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("src_context_test.rs");
        {
            let mut f = std::fs::File::create(&file_path).unwrap();
            for i in 1..=20 {
                writeln!(f, "fn line_{i}() {{}}").unwrap();
            }
        }
        let path_str = file_path.to_str().unwrap();
        // Construct output that parse_rust_errors will parse with our file path
        let output = format!("error[E0308]: mismatched types\n  --> {}:10:5\n", path_str);
        let prompt = build_watch_fix_prompt("cargo build", &output);
        assert!(
            prompt.contains("## Relevant source context"),
            "prompt should include source context section: {prompt}",
        );
        assert!(
            prompt.contains("around line 10"),
            "prompt should reference the error line: {prompt}",
        );
    }

    // --- Near-miss typo guard for /watch free-text args ---

    #[test]
    fn test_watch_near_miss_fires_on_typo() {
        // "/watch of" is a typo for "off" — refuse instead of silently
        // installing "of" as a custom watch command that fails every turn.
        assert_eq!(watch_near_miss("of"), Some("off"));
        assert_eq!(watch_near_miss("statu"), Some("status"));
        assert_eq!(watch_near_miss("lnt"), Some("lint"));
    }

    #[test]
    fn test_watch_near_miss_silent_on_exact_subcommand() {
        // Exact subcommands are handled by their own match arms — the guard
        // must never fire on them.
        for sub in WATCH_SUBCOMMANDS {
            assert_eq!(watch_near_miss(sub), None, "guard fired on exact '{sub}'");
        }
    }

    #[test]
    fn test_watch_near_miss_silent_on_multi_word_command() {
        // Multi-word custom commands are the real use case — untouched.
        assert_eq!(watch_near_miss("npm test"), None);
        assert_eq!(watch_near_miss("cargo clippy --all-targets"), None);
    }

    #[test]
    fn test_watch_near_miss_silent_on_far_single_word() {
        // Negative side (Days 122-124 lesson): a single word FAR from any
        // subcommand (e.g. "pytest") must still be accepted as a custom
        // watch command — the guard only fires on near-misses.
        assert_eq!(watch_near_miss("pytest"), None);
        assert_eq!(watch_near_miss("make"), None);
    }

    #[test]
    fn test_watch_near_miss_silent_on_empty() {
        assert_eq!(watch_near_miss(""), None);
    }

    // --- Multibyte-output char-boundary safety (CLAUDE.md byte-index rule, #250) ---
    //
    // All three parsers slice raw compiler/test *output* by byte offset
    // (`&rest[..bracket_end]`, `&rest[..colon]`, `&rest[..dash_pos]`, etc.).
    // The slices are safe today only because every anchor delimiter (`]`, `: `,
    // ` - `, `(`, `)`, `"`) is ASCII — a subtle invariant with, until now, no
    // net. Real compiler/test output routinely carries multibyte characters
    // (Rust diagnostics use `→ ╰ ─`, quoted non-ASCII identifiers; pytest/mypy
    // messages carry accented text and emoji). These tests feed multibyte
    // content through each parser's byte-slice paths and assert it neither
    // panics nor corrupts the extracted message. If a future edit swaps an
    // ASCII anchor for a `find` on a multibyte pattern, or slices at a
    // non-boundary offset, these fail loudly instead of panicking in production
    // (the #250 planning-agent crash class).

    #[test]
    fn parse_rust_errors_multibyte_output_does_not_panic() {
        // `error[EXXXX]:` line whose message is loaded with multibyte chars,
        // plus Rust's Unicode diagnostic gutter glyphs (→ ╰ ─ │).
        let output = "\
error[E0308]: mismatched types — expected ` Résumé`, found `naïve` ✓ 日本語
  --> src/café.rs:42:5
   │
42 │     let x: Résumé = naïve();
   │            ─────── 型が違います
   ╰─→ note: consider changing this";
        // Must not panic on any byte-slice path.
        let errs = parse_rust_errors(output);
        assert_eq!(errs.len(), 1, "should parse exactly one error");
        assert_eq!(errs[0].code.as_deref(), Some("E0308"));
        // Message extraction must round-trip the multibyte text intact.
        assert!(
            errs[0].message.contains("Résumé") && errs[0].message.contains("naïve"),
            "multibyte message corrupted: {:?}",
            errs[0].message,
        );
    }

    #[test]
    fn parse_typescript_errors_multibyte_output_does_not_panic() {
        // tsc format with multibyte in the file path AND the message.
        let output =
            "src/café.ts(12,7): error TS2345: Argument «naïve» is not assignable — 型不一致 ✗";
        let errs = parse_typescript_errors(output);
        assert_eq!(errs.len(), 1, "should parse exactly one TS error");
        assert_eq!(errs[0].code.as_deref(), Some("TS2345"));
        assert!(
            errs[0].message.contains("naïve") && errs[0].message.contains("型不一致"),
            "multibyte TS message corrupted: {:?}",
            errs[0].message,
        );
    }

    #[test]
    fn parse_python_errors_multibyte_output_does_not_panic() {
        // pytest FAILED line and a traceback whose error message carry
        // multibyte text — exercises `rest[..dash_pos]` and the traceback
        // `after_prefix[..quote_end]` byte slices.
        let output = "\
FAILED tests/test_café.py::test_naïve - AssertionError: 期待値 ≠ 実際値 ✗
Traceback (most recent call last):
  File \"src/café.py\", line 7, in résumé
ValueError: naïve — 不正な入力 ✗";
        let errs = parse_python_errors(output);
        assert!(!errs.is_empty(), "should parse at least one Python error");
        // The FAILED line's message must round-trip the multibyte text.
        assert!(
            errs.iter().any(|e| e.message.contains("期待値")),
            "multibyte pytest message corrupted: {:?}",
            errs.iter().map(|e| &e.message).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn build_watch_fix_prompt_multibyte_output_does_not_panic() {
        // End-to-end: multibyte raw output through the full prompt builder,
        // including the WATCH_OUTPUT_MAX truncation path (safe_truncate must
        // never split a multibyte char). Pad with multibyte content well past
        // the truncation threshold.
        let big = "café → naïve ✓ 日本語 ".repeat(2000); // >> WATCH_OUTPUT_MAX bytes
        let output = format!("error[E0308]: mismatched types — {big}");
        // Must not panic on truncation or parsing.
        let prompt = build_watch_fix_prompt("cargo build", &output);
        assert!(
            prompt.contains("cargo build"),
            "prompt should reference the watch command",
        );
    }
}
