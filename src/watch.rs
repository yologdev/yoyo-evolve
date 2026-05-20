//! Watch mode — auto-run a test/lint command after agent edits.
//!
//! Extracted from `prompt.rs` (Day 58). The watch system lets users set a
//! command (e.g. `cargo test`) that runs automatically after each agent turn.
//! If the command fails, the agent gets a fix prompt and retries up to
//! [`MAX_WATCH_FIX_ATTEMPTS`] times.

use crate::commands_lint::{lint_command_for_project, test_command_for_project, LintStrictness};
use crate::commands_project::detect_project_type;
use crate::format::*;
use crate::prompt::run_prompt_auto_retry;
use crate::prompt_budget::session_budget_exhausted;
use crate::session::SessionChanges;
use std::io::{self, IsTerminal, Write};
use std::sync::RwLock;
use yoagent::agent::Agent;
use yoagent::*;

/// Acquire a read-guard, recovering from a poisoned RwLock instead of panicking.
fn rw_read_or_recover<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| e.into_inner())
}

/// Acquire a write-guard, recovering from a poisoned RwLock instead of panicking.
fn rw_write_or_recover<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| e.into_inner())
}

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
    let lines: Vec<&str> = output.lines().collect();

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
            errors.push(CompilerError {
                code: None,
                message: line.trim().to_string(),
                file: None,
                line: None,
                category: ErrorCategory::TestAssertion,
            });
        }

        i += 1;
    }

    errors
}

/// Look ahead from line `start` for a `  --> path:line:col` location line.
/// Returns (file, line) if found within the next 5 lines.
fn extract_location(lines: &[&str], start: usize) -> (Option<String>, Option<u32>) {
    let end = std::cmp::min(start + 6, lines.len());
    for line in &lines[start + 1..end] {
        let trimmed = line.trim();
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
pub fn parse_typescript_errors(output: &str) -> Vec<CompilerError> {
    let mut errors: Vec<CompilerError> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

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
        if trimmed.starts_with("FAIL ") {
            let file = trimmed.strip_prefix("FAIL ").unwrap().trim();
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

    let hint = match cmd_type {
        "lint" => "\n\nThis is a **lint** failure — fixes are usually mechanical (unused imports, \
                   missing derives, formatting issues). Apply targeted fixes without changing logic.",
        "test" => "\n\nThis is a **test** failure — understand what the test expects before \
                   changing code. Fix the implementation to match the intended behavior, \
                   or fix the test if the new behavior is correct.",
        _ => "",
    };

    if let Some(summary) = structured_section {
        format!(
            "Your changes caused {cmd_type} failures. Here's the output from `{watch_cmd}`:\n\
             ```\n{truncated}\n```\n\n\
             {summary}{hint}"
        )
    } else {
        format!(
            "Your changes caused {cmd_type} failures. Here's the output from `{watch_cmd}`:\n\
             ```\n{truncated}\n```\n\
             Please fix the issues.{hint}"
        )
    }
}

/// Run a watch command and return (success, output).
///
/// Streams output line-by-line in real time: when stderr is a terminal,
/// prints a compact progress indicator (`⟳ 42 lines...`) so the user
/// sees something happening during long test/build runs.  The full
/// combined stdout+stderr is still collected and returned for the agent
/// to analyse.
pub fn run_watch_command(cmd: &str) -> (bool, String) {
    use std::io::BufRead;
    use std::process::{Command, Stdio};

    let is_tty = io::stderr().is_terminal();

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
                    if is_tty {
                        let count = stdout_lines.len();
                        eprint!("\r{DIM}  ⟳ {count} lines...{RESET}");
                        let _ = io::stderr().flush();
                    }
                }
                Err(_) => break,
            }
        }
    }

    let stderr_lines = stderr_handle.join().unwrap_or_default();

    // Clear the progress indicator if we printed one.
    if is_tty && !stdout_lines.is_empty() {
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

/// Run the watch command(s) after a prompt completes.
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
) -> WatchResult {
    let commands = get_watch_commands();
    if commands.is_empty() {
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
            return WatchResult {
                passed: false,
                last_tool_error,
            };
        }
    }

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

/// Auto-detect a combined lint + test command for the current project.
/// Returns both commands chained with `&&` so the first failure stops execution.
/// Falls back to just the test command if no lint command is available,
/// or `None` if neither can be detected.
pub fn detect_watch_all_command() -> Option<String> {
    let dir = std::env::current_dir().unwrap_or_default();
    let project_type = detect_project_type(&dir);
    let lint = lint_command_for_project(&project_type, LintStrictness::Default);
    let test = test_command_for_project(&project_type);
    match (lint, test) {
        (Some((lint_label, _)), Some((test_label, _))) => {
            Some(format!("{lint_label} && {test_label}"))
        }
        (None, Some((test_label, _))) => Some(test_label.to_string()),
        (Some((lint_label, _)), None) => Some(lint_label),
        (None, None) => None,
    }
}

/// Auto-detect separate lint and test commands for two-phase watch.
/// Returns a vec of individual commands (lint first, then test).
/// Falls back to a single-element vec if only one is available.
pub fn detect_watch_all_phases() -> Option<Vec<String>> {
    let dir = std::env::current_dir().unwrap_or_default();
    let project_type = detect_project_type(&dir);
    let lint = lint_command_for_project(&project_type, LintStrictness::Default);
    let test = test_command_for_project(&project_type);
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

/// Handle the /watch command: toggle auto-test-on-edit mode.
pub fn handle_watch(input: &str) {
    let arg = input.strip_prefix("/watch").unwrap_or("").trim();

    match arg {
        "" => {
            // Auto-detect lint+test as separate phases
            match detect_watch_all_phases() {
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
        "all" => {
            // Auto-detect lint + test as separate phases
            match detect_watch_all_phases() {
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
        "lint" => {
            // Auto-detect lint-only command
            let dir = std::env::current_dir().unwrap_or_default();
            let project_type = detect_project_type(&dir);
            match lint_command_for_project(&project_type, LintStrictness::Default) {
                Some((lint_label, _)) => {
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
    // clear_watch_command, or handle_watch must use #[serial].

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
        // After clearing, there should be no watch command
        clear_watch_command();
        assert!(
            get_watch_command().is_none(),
            "should have no watch command after clear"
        );
    }

    #[serial]
    #[test]
    fn test_watch_command_roundtrip() {
        // Set a command, get it back, clear it
        set_watch_command("cargo test --release");
        let cmd = get_watch_command();
        assert_eq!(cmd.as_deref(), Some("cargo test --release"));
        clear_watch_command();
        assert!(get_watch_command().is_none());
    }

    #[serial]
    #[test]
    fn test_run_watch_after_prompt_no_watch_returns_passed() {
        // When no watch command is set, run_watch_after_prompt should return
        // WatchResult { passed: true, last_tool_error: None } immediately.
        // We verify the guard condition that makes it return early.
        clear_watch_command();
        assert!(
            get_watch_command().is_none(),
            "precondition: no watch command set"
        );
        // The function checks get_watch_command() first and returns a passing
        // WatchResult if None. We can't call the async function in a sync test,
        // but we verify the guard condition that makes it return early.
    }

    #[serial]
    #[test]
    fn test_run_watch_command_pass_with_set_watch() {
        // Simulate: set a watch command that passes, run it
        set_watch_command("echo ok");
        if let Some(cmd) = get_watch_command() {
            let (ok, output) = run_watch_command(&cmd);
            assert!(ok, "echo ok should succeed");
            assert!(output.contains("ok"));
        } else {
            panic!("watch command should be set");
        }
        clear_watch_command();
    }

    #[serial]
    #[test]
    fn test_run_watch_command_fail_with_set_watch() {
        // Simulate: set a watch command that fails, run it, check output
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
        clear_watch_command();
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
        set_watch_commands(&["cargo clippy", "cargo test"]);
        let cmds = get_watch_commands();
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0], "cargo clippy");
        assert_eq!(cmds[1], "cargo test");
        clear_watch_command();
        assert!(get_watch_commands().is_empty());
    }

    #[serial]
    #[test]
    fn test_get_watch_command_joins_multi_phase() {
        set_watch_commands(&["cargo clippy", "cargo test"]);
        let display = get_watch_command();
        assert_eq!(
            display.as_deref(),
            Some("cargo clippy && cargo test"),
            "get_watch_command should join phases with &&"
        );
        clear_watch_command();
    }

    #[serial]
    #[test]
    fn test_single_command_still_works() {
        set_watch_command("cargo test");
        let cmds = get_watch_commands();
        assert_eq!(cmds.len(), 1, "single command should store one-element vec");
        assert_eq!(cmds[0], "cargo test");
        let display = get_watch_command();
        assert_eq!(display.as_deref(), Some("cargo test"));
        clear_watch_command();
    }

    #[serial]
    #[test]
    fn test_clear_clears_multi_phase() {
        set_watch_commands(&["a", "b", "c"]);
        assert_eq!(get_watch_commands().len(), 3);
        clear_watch_command();
        assert!(get_watch_commands().is_empty());
        assert!(get_watch_command().is_none());
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
        // When no watch commands are set, should return passed immediately
        clear_watch_command();
        assert!(
            get_watch_commands().is_empty(),
            "precondition: no commands set"
        );
        // The function checks get_watch_commands() first and returns a passing
        // WatchResult if empty. We verify the guard condition.
    }

    #[test]
    fn auto_detect_watch_command_returns_lint_and_test_in_rust_project() {
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
    }

    #[test]
    fn detect_watch_all_command_returns_lint_and_test_for_rust() {
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
        // Clear any previous watch command
        clear_watch_command();
        // Run /watch all — since we're in a Rust project, it should set separate phases
        handle_watch("/watch all");
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
        // Cleanup
        clear_watch_command();
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
        // Clear any previous watch command
        clear_watch_command();
        // Run /watch lint — since we're in a Rust project, it should set clippy only
        handle_watch("/watch lint");
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
        // Cleanup
        clear_watch_command();
    }

    #[serial]
    #[test]
    fn handle_watch_bare_sets_lint_and_test() {
        // Clear any previous watch command
        clear_watch_command();
        // Run bare /watch — should now set lint+test as separate phases
        handle_watch("/watch");
        let cmd = get_watch_command();
        assert!(
            cmd.is_some(),
            "watch command should be set after bare /watch"
        );
        let cmd = cmd.unwrap();
        assert!(
            cmd.contains("clippy") && cmd.contains("cargo test"),
            "bare /watch should set lint && test: {cmd}"
        );
        // Verify multi-phase
        let phases = get_watch_commands();
        assert_eq!(
            phases.len(),
            2,
            "bare /watch should set 2 phases: {phases:?}"
        );
        // Cleanup
        clear_watch_command();
    }

    #[test]
    fn detect_watch_all_phases_returns_separate_commands() {
        // In a Rust project, should return 2 separate commands
        let phases = detect_watch_all_phases();
        assert!(phases.is_some(), "should detect phases in a Rust project");
        let phases = phases.unwrap();
        assert_eq!(
            phases.len(),
            2,
            "should have lint + test phases: {phases:?}"
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

    // -----------------------------------------------------------------------
    // Structured Rust compiler error parsing tests
    // -----------------------------------------------------------------------

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
            "should label as Python: {summary}"
        );
    }
}
