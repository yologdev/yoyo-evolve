//! Tool output compression, filtering, and truncation.
//
//! Reduces token usage when feeding tool results back to the LLM by:
//! - Stripping ANSI escape codes
//! - Filtering noisy CLI patterns (cargo, npm, pip, progress bars)
//! - Detecting and compressing test framework output
//! - Collapsing repetitive line sequences
//! - Truncating to head/tail with a clear omission marker

use super::{format_duration, pluralize, DIM, GREEN, RED, RESET};

/// Default character threshold for tool output truncation.
/// Outputs longer than this get the head/tail treatment.
pub const TOOL_OUTPUT_MAX_CHARS: usize = 30_000;

/// Maximum tool output size in piped/CI mode (half of interactive).
/// Reduces context growth rate during evolution sessions and CI runs
/// where the user isn't watching live output anyway.
pub const TOOL_OUTPUT_MAX_CHARS_PIPED: usize = 15_000;

/// Number of lines to keep from the start of truncated output.
const TRUNCATION_HEAD_LINES: usize = 100;

/// Number of lines to keep from the end of truncated output.
const TRUNCATION_TAIL_LINES: usize = 50;

/// Minimum number of consecutive similar lines to trigger collapsing.
const COLLAPSE_MIN_LINES: usize = 4;

/// Maximum prefix length used for line category comparison.
const CATEGORY_PREFIX_MAX: usize = 20;

/// Strip ANSI escape codes and collapse runs of similar lines.
///
/// This reduces token usage when tool output is fed back to the LLM:
/// - **ANSI stripping**: removes `\x1b[...X` sequences (SGR, cursor, erase)
/// - **Repetitive line collapsing**: when 4+ consecutive lines share a category
///   prefix (first word(s) up to 20 chars), replaces with first line,
///   `"... (N more similar lines)"`, and last line.
///
/// Called before head/tail truncation so the truncation operates on
/// already-compressed output.
pub fn compress_tool_output(output: &str) -> String {
    if output.is_empty() {
        return String::new();
    }

    // Phase 1: strip ANSI escape codes
    let stripped = strip_ansi_codes(output);

    // Phase 2: filter test framework output (more specific, runs first)
    let filtered = filter_test_output(&stripped);

    // Phase 3: filter noisy CLI patterns (cargo, npm, pip, progress bars, etc.)
    let denoised = filter_noisy_patterns(&filtered);

    // Phase 4: collapse repetitive line sequences
    collapse_repetitive_lines(&denoised)
}

/// Remove ANSI escape sequences from a string.
///
/// Handles the full set of terminal escape sequences:
/// 1. **CSI** (Control Sequence Introducer): `ESC [` params letter — colors, cursor movement
/// 2. **OSC** (Operating System Command): `ESC ]` ... ST — hyperlinks, window titles
///    (modern `rustc` emits OSC 8 hyperlinks in error output)
/// 3. **Two-character sequences**: `ESC` + single letter (e.g., `ESC M`, `ESC D`)
///
/// Uses char-based iteration to correctly handle multi-byte UTF-8 content.
/// ANSI escape sequences are purely ASCII, so we can safely detect them
/// by checking for ESC (\x1b) and then consuming ASCII parameter/final bytes.
fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                // CSI sequence: ESC [ <params> <final byte>
                Some(&'[') => {
                    chars.next(); // consume '['
                                  // Skip parameter bytes: digits, semicolons, and private markers
                    while let Some(&p) = chars.peek() {
                        if p.is_ascii_digit()
                            || p == ';'
                            || p == '?'
                            || p == '>'
                            || p == '<'
                            || p == '='
                            || p == '!'
                        {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    // Skip intermediate bytes (space 0x20 through / 0x2F)
                    while let Some(&p) = chars.peek() {
                        if (' '..='/').contains(&p) {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    // Skip final byte (@ 0x40 through ~ 0x7E)
                    if let Some(&f) = chars.peek() {
                        if ('@'..='~').contains(&f) {
                            chars.next();
                        }
                    }
                }
                // OSC sequence: ESC ] ... ST (terminated by BEL \x07 or ESC \)
                Some(&']') => {
                    chars.next(); // consume ']'
                                  // Consume until String Terminator (ST = ESC \) or BEL (\x07)
                    loop {
                        match chars.next() {
                            None => break,
                            Some('\x07') => break, // BEL terminates OSC
                            Some('\x1b') => {
                                // Check for ESC \ (ST)
                                if chars.peek() == Some(&'\\') {
                                    chars.next(); // consume '\'
                                }
                                break;
                            }
                            Some(_) => {} // skip OSC content
                        }
                    }
                }
                // Two-character escape: ESC + single ASCII letter (@ through ~)
                Some(&ch) if ('@'..='~').contains(&ch) => {
                    chars.next(); // consume the letter
                }
                // Unknown/malformed: just skip the ESC character
                _ => {}
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Returns true if the line looks like a progress bar / spinner
/// (contains 6+ consecutive block/bar characters).
fn is_progress_bar_line(line: &str) -> bool {
    let mut count = 0u32;
    for c in line.chars() {
        if matches!(
            c,
            '━' | '█' | '▓' | '░' | '─' | '▏' | '▎' | '▍' | '▌' | '▋' | '▊' | '▉'
        ) {
            count += 1;
            if count >= 6 {
                return true;
            }
        } else {
            count = 0;
        }
    }
    false
}

/// Returns true if `line` matches `Compiling <something> v<version>`.
fn is_compiling_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("Compiling ") && t.contains(" v")
}

/// Returns true if `line` matches `Downloading <something> v<version>`.
fn is_downloading_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("Downloading ") && t.contains(" v")
}

/// Filter noisy CLI output patterns that waste tokens.
///
/// Handles:
/// - Cargo `Compiling`/`Downloading` sequences (keep first + last, collapse middle)
/// - Cargo lock-waiting lines (remove entirely)
/// - Progress bars and spinner lines (remove)
/// - npm warn lines (keep only if they mention "deprecated" or "vulnerability")
/// - pip "already satisfied" lines (remove)
/// - Git commit hash abbreviation (`commit <40-hex>` → `commit <7-hex>...`)
/// - Git Author/Date whitespace consolidation
/// - Runs of 3+ consecutive empty lines collapsed to 2
fn filter_noisy_patterns(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // ── Cargo Compiling / Downloading sequences ───────────────
        if is_compiling_line(line) || is_downloading_line(line) {
            let is_compiling = is_compiling_line(line);
            let first = i;
            let mut run_end = i + 1;
            while run_end < lines.len() {
                let matches = if is_compiling {
                    is_compiling_line(lines[run_end])
                } else {
                    is_downloading_line(lines[run_end])
                };
                if matches {
                    run_end += 1;
                } else {
                    break;
                }
            }
            let run_len = run_end - first;
            if run_len >= 3 {
                // Keep first and last, collapse middle
                result.push(lines[first].to_string());
                let collapsed = run_len - 2;
                result.push(format!("... ({collapsed} more)"));
                result.push(lines[run_end - 1].to_string());
            } else {
                // Short run — keep all
                for item in lines.iter().take(run_end).skip(first) {
                    result.push((*item).to_string());
                }
            }
            i = run_end;
            continue;
        }

        // ── Cargo lock-waiting lines → remove ────────────────────
        if trimmed.starts_with("Blocking waiting for file lock on") {
            i += 1;
            continue;
        }

        // ── Progress bars / spinners → remove ────────────────────
        if is_progress_bar_line(line) {
            i += 1;
            continue;
        }

        // ── npm warn lines → keep only important ones ────────────
        if trimmed.starts_with("npm warn") || trimmed.starts_with("npm WARN") {
            let lower = trimmed.to_lowercase();
            if lower.contains("deprecated") || lower.contains("vulnerability") {
                result.push(line.to_string());
            }
            i += 1;
            continue;
        }

        // ── pip "already satisfied" lines → remove ───────────────
        if trimmed.starts_with("Requirement already satisfied") {
            i += 1;
            continue;
        }

        // ── Git commit hash abbreviation ─────────────────────────
        if trimmed.starts_with("commit ") && trimmed.len() >= 47 {
            let hash_part = &trimmed[7..];
            // Check that we have a 40-char hex hash
            if hash_part.len() >= 40 && hash_part[..40].chars().all(|c| c.is_ascii_hexdigit()) {
                result.push(format!("commit {}...", &hash_part[..7]));
                i += 1;
                continue;
            }
        }

        // ── Git Author/Date whitespace consolidation ─────────────
        if trimmed.starts_with("Author:") || trimmed.starts_with("Date:") {
            // Collapse multiple internal spaces to single space
            let consolidated: String = trimmed.split_whitespace().collect::<Vec<&str>>().join(" ");
            result.push(consolidated);
            i += 1;
            continue;
        }

        // ── Consecutive empty lines → max 2 ─────────────────────
        if trimmed.is_empty() {
            let mut empty_count = 1u32;
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                empty_count += 1;
                j += 1;
            }
            // Keep at most 2
            let keep = empty_count.min(2);
            for _ in 0..keep {
                result.push(String::new());
            }
            i = j;
            continue;
        }

        // ── Default: pass through ────────────────────────────────
        result.push(line.to_string());
        i += 1;
    }

    result.join("\n")
}

/// Extract a "category" from a line for grouping similar lines.
///
/// Takes the leading whitespace + first word (up to CATEGORY_PREFIX_MAX chars).
/// Lines with the same category are considered similar.
fn line_category(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return "";
    }

    // Find end of first word in the trimmed content
    let first_word_end = trimmed
        .find(|c: char| c.is_whitespace())
        .unwrap_or(trimmed.len());

    // Include leading whitespace length + first word
    let prefix_len = (line.len() - trimmed.len()) + first_word_end;
    let mut end = prefix_len.min(CATEGORY_PREFIX_MAX).min(line.len());

    // Ensure we don't slice inside a multi-byte UTF-8 character
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }

    &line[..end]
}

/// Collapse runs of 4+ consecutive lines that share a category prefix.
fn collapse_repetitive_lines(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    if lines.len() < COLLAPSE_MIN_LINES {
        return s.to_string();
    }

    let mut result = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        let cat = line_category(lines[i]);

        // Count consecutive lines with the same non-empty category
        if !cat.is_empty() {
            let mut run_end = i + 1;
            while run_end < lines.len() && line_category(lines[run_end]) == cat {
                run_end += 1;
            }
            let run_len = run_end - i;

            if run_len >= COLLAPSE_MIN_LINES {
                // Collapse: first line, marker, last line
                result.push(lines[i].to_string());
                let collapsed = run_len - 2; // exclude first and last
                result.push(format!("... ({collapsed} more similar lines)"));
                result.push(lines[run_end - 1].to_string());
                i = run_end;
                continue;
            }
        }

        result.push(lines[i].to_string());
        i += 1;
    }

    result.join("\n")
}

/// Minimum number of test-pass lines required to activate the test filter.
const TEST_FILTER_MIN_PASS_LINES: usize = 5;

/// Detect and filter test framework output, keeping only failures + summary.
///
/// Supports:
/// - **cargo test**: `test ... ok` / `test ... FAILED`, `test result:` summary
/// - **pytest**: `PASSED` / `FAILED` lines, summary with pass/fail counts
/// - **jest/vitest**: `✓` (pass) / `✕`/`✗` (fail) markers, `Tests:` summary
/// - **go test**: `--- PASS:` / `--- FAIL:`, `ok`/`FAIL` summary
/// - **rspec**: lines with `examples` and `failures` count
///
/// When ≥5 test-pass lines are detected, replaces them with a count marker.
/// Failure lines, their context, error sections, and summaries are preserved.
/// Non-test output passes through unchanged.
pub fn filter_test_output(output: &str) -> String {
    if output.is_empty() {
        return String::new();
    }

    let lines: Vec<&str> = output.lines().collect();

    // Phase 1: classify each line
    let mut classifications: Vec<TestLineKind> = Vec::with_capacity(lines.len());
    for line in &lines {
        classifications.push(classify_test_line(line));
    }

    // Count pass lines to decide if we should filter
    let pass_count = classifications
        .iter()
        .filter(|k| matches!(k, TestLineKind::Pass))
        .count();

    if pass_count < TEST_FILTER_MIN_PASS_LINES {
        return output.to_string();
    }

    // Phase 2: mark lines in failure sections as kept
    // Once we see a "failures:" header, everything until the summary is a failure section
    let mut in_failure_section = false;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == "failures:"
            || trimmed.starts_with("---- ") && trimmed.ends_with(" stdout ----")
        {
            in_failure_section = true;
        }
        if in_failure_section {
            if matches!(classifications[i], TestLineKind::Pass) {
                // Don't reclassify pass lines even in failure sections
            } else if matches!(classifications[i], TestLineKind::Other) {
                classifications[i] = TestLineKind::FailureDetail;
            }
        }
        // Summary lines end the failure section
        if matches!(classifications[i], TestLineKind::Summary) {
            in_failure_section = false;
        }
    }

    // Phase 3: build filtered output
    let mut result_lines: Vec<String> = Vec::new();
    let mut omitted_pass_count: usize = 0;

    for (i, line) in lines.iter().enumerate() {
        match classifications[i] {
            TestLineKind::Pass => {
                omitted_pass_count += 1;
            }
            TestLineKind::Fail | TestLineKind::FailureDetail | TestLineKind::Summary => {
                // Flush any accumulated pass count before this line
                if omitted_pass_count > 0 {
                    result_lines.push(format!("... ({omitted_pass_count} passing tests omitted)"));
                    omitted_pass_count = 0;
                }
                result_lines.push(line.to_string());
            }
            TestLineKind::Other => {
                // Flush any accumulated pass count before non-test content
                if omitted_pass_count > 0 {
                    result_lines.push(format!("... ({omitted_pass_count} passing tests omitted)"));
                    omitted_pass_count = 0;
                }
                result_lines.push(line.to_string());
            }
        }
    }

    // Flush trailing pass count
    if omitted_pass_count > 0 {
        result_lines.push(format!("... ({omitted_pass_count} passing tests omitted)"));
    }

    result_lines.join("\n")
}

/// Classification of a line in test output.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TestLineKind {
    /// A passing test line (will be omitted)
    Pass,
    /// A failing test line (will be kept)
    Fail,
    /// Detail lines inside a failure section (stack traces, assertions)
    FailureDetail,
    /// Summary/result line (will be kept)
    Summary,
    /// Non-test output (will be kept)
    Other,
}

/// Classify a single line as test pass, fail, summary, or other.
fn classify_test_line(line: &str) -> TestLineKind {
    let trimmed = line.trim();

    // --- cargo test ---
    if trimmed.starts_with("test ") && trimmed.ends_with("... ok") {
        return TestLineKind::Pass;
    }
    if trimmed.starts_with("test ") && trimmed.ends_with("... FAILED") {
        return TestLineKind::Fail;
    }
    if trimmed.starts_with("test result:") {
        return TestLineKind::Summary;
    }

    // --- pytest ---
    if trimmed.ends_with(" PASSED") && trimmed.contains("::") {
        return TestLineKind::Pass;
    }
    if trimmed.ends_with(" FAILED") && trimmed.contains("::") {
        return TestLineKind::Fail;
    }
    // pytest summary: "N passed" or "N passed, M failed"
    if (trimmed.contains(" passed") || trimmed.contains(" failed"))
        && trimmed.starts_with('=')
        && trimmed.ends_with('=')
    {
        return TestLineKind::Summary;
    }

    // --- jest/vitest ---
    // ✓ or ✔ = pass; ✕ or ✗ = fail
    if trimmed.starts_with('✓') || trimmed.starts_with('✔') {
        return TestLineKind::Pass;
    }
    if trimmed.starts_with("✕") || trimmed.starts_with("✗") {
        return TestLineKind::Fail;
    }
    if trimmed.starts_with("Tests:") && (trimmed.contains("passed") || trimmed.contains("failed")) {
        return TestLineKind::Summary;
    }

    // --- go test ---
    if trimmed.starts_with("--- PASS:") {
        return TestLineKind::Pass;
    }
    if trimmed.starts_with("--- FAIL:") {
        return TestLineKind::Fail;
    }
    // go test summary: "ok  pkg  0.123s" or "FAIL  pkg  0.123s"
    if (trimmed.starts_with("ok ") || trimmed.starts_with("FAIL\t") || trimmed.starts_with("FAIL "))
        && trimmed.contains('s')
        && !trimmed.contains("::")
    {
        // Distinguish "FAIL" summary from pytest "FAILED" lines
        if trimmed.starts_with("ok ") {
            return TestLineKind::Summary;
        }
        if trimmed.starts_with("FAIL") && !trimmed.ends_with("FAILED") {
            return TestLineKind::Summary;
        }
    }

    // --- rspec ---
    if trimmed.contains("example")
        && trimmed.contains("failure")
        && trimmed.chars().any(|c| c.is_ascii_digit())
    {
        return TestLineKind::Summary;
    }

    // --- pytest short test summary header ---
    if trimmed.starts_with('=') && trimmed.contains("short test summary") {
        return TestLineKind::Summary;
    }

    // --- FAILED line in pytest summary (e.g., "FAILED tests/...") ---
    if trimmed.starts_with("FAILED ") && trimmed.contains("::") {
        return TestLineKind::Fail;
    }

    TestLineKind::Other
}

/// Intelligently truncate large tool output to save context window tokens.
///
/// Applies compression (ANSI stripping + repetitive line collapsing) first,
/// then when output exceeds `max_chars`, keeps the first ~100 lines and last ~50 lines
/// with a clear `[... truncated N lines ...]` marker in between. This preserves
/// the beginning of output (usually the most informative — headers, first errors)
/// and the end (summary lines, final status).
///
/// Output under the threshold is returned unchanged.
pub fn truncate_tool_output(output: &str, max_chars: usize) -> String {
    // Phase 1: compress (strip ANSI + collapse repetitive lines)
    let compressed = compress_tool_output(output);

    // Under threshold — return compressed output
    if compressed.len() <= max_chars {
        return compressed;
    }

    let lines: Vec<&str> = compressed.lines().collect();
    let total_lines = lines.len();

    // If not enough lines to meaningfully truncate, return as-is
    // (edge case: very long single lines or very few lines)
    if total_lines <= TRUNCATION_HEAD_LINES + TRUNCATION_TAIL_LINES {
        return compressed;
    }

    // Phase 2: If this looks like compiler output, use error-prioritized truncation.
    // The agent needs to see actual errors/warnings to fix code — not just the head
    // (which is often "Compiling..." lines) and tail (which is the summary).
    if is_compiler_output(&lines) {
        if let Some(result) = truncate_compiler_output(&lines, max_chars) {
            return result;
        }
    }

    // Phase 3: Default head/tail truncation for non-compiler output.
    let head = &lines[..TRUNCATION_HEAD_LINES];
    let tail = &lines[total_lines - TRUNCATION_TAIL_LINES..];
    let omitted = total_lines - TRUNCATION_HEAD_LINES - TRUNCATION_TAIL_LINES;

    let mut result = String::with_capacity(max_chars);
    for line in head {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(&format!(
        "\n[... truncated {omitted} {} ...]\n\n",
        pluralize(omitted, "line", "lines")
    ));
    for (i, line) in tail.iter().enumerate() {
        result.push_str(line);
        if i < tail.len() - 1 {
            result.push('\n');
        }
    }

    result
}

/// Check whether the output looks like compiler/linter output by scanning
/// for characteristic error/warning patterns.
fn is_compiler_output(lines: &[&str]) -> bool {
    let mut diagnostic_count = 0u32;
    for line in lines.iter().take(500) {
        if is_diagnostic_header(line) {
            diagnostic_count += 1;
            if diagnostic_count >= 2 {
                return true;
            }
        }
    }
    false
}

/// Returns true if a line looks like the start of a compiler diagnostic.
/// Covers: Rust (`error[E0xxx]:`, `warning:`), TypeScript (`error TS`),
/// GCC/Clang (`file:line:col: error:`), and Go (`file:line:col:`).
fn is_diagnostic_header(line: &str) -> bool {
    let t = line.trim();
    // Rust: "error[E0308]: ...", "error: ...", "warning: ...", "warning[unused]: ..."
    if t.starts_with("error[")
        || t.starts_with("error: ")
        || t.starts_with("warning[")
        || t.starts_with("warning: ")
    {
        return true;
    }
    // GCC/Clang: "src/main.c:10:5: error:" or "src/main.c:10:5: warning:"
    if (t.contains(": error:") || t.contains(": warning:")) && t.contains(':') {
        // Heuristic: must have at least one digit (line number)
        if t.chars().any(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    // TypeScript: "src/index.ts(5,3): error TS2322:"
    if t.contains("error TS") && t.contains(':') {
        return true;
    }
    false
}

/// Maximum number of diagnostic blocks to include in compiler-aware truncation.
const MAX_DIAGNOSTIC_BLOCKS: usize = 20;

/// Extract error/warning blocks from compiler output and build a truncated
/// result that prioritizes diagnostics over noise.
///
/// Returns `None` if the extraction doesn't produce a meaningfully smaller result
/// (in which case the caller falls through to default head/tail truncation).
fn truncate_compiler_output(lines: &[&str], max_chars: usize) -> Option<String> {
    // Identify diagnostic blocks: each block starts at a diagnostic header line
    // and continues until the next diagnostic header or a blank line after code context.
    let mut blocks: Vec<(usize, usize)> = Vec::new(); // (start_idx, end_idx exclusive)
    let mut i = 0;

    while i < lines.len() {
        if is_diagnostic_header(lines[i]) {
            let start = i;
            i += 1;
            // A diagnostic block includes:
            // - The header line
            // - Following context lines (source snippets, arrows, notes)
            // - Stops at next diagnostic header, or after 2+ consecutive blank lines,
            //   or after 30 lines (safety cap for very long diagnostics)
            let mut consecutive_blanks = 0u32;
            let block_line_cap = start + 30;
            while i < lines.len() && i < block_line_cap {
                if is_diagnostic_header(lines[i]) {
                    break;
                }
                if lines[i].trim().is_empty() {
                    consecutive_blanks += 1;
                    if consecutive_blanks >= 2 {
                        break;
                    }
                } else {
                    consecutive_blanks = 0;
                }
                i += 1;
            }
            blocks.push((start, i));
        } else {
            i += 1;
        }
    }

    if blocks.is_empty() {
        return None;
    }

    // Separate errors from warnings — errors take priority
    let mut error_blocks: Vec<(usize, usize)> = Vec::new();
    let mut warning_blocks: Vec<(usize, usize)> = Vec::new();

    for &(start, end) in &blocks {
        let header = lines[start].trim();
        if header.starts_with("error") || header.contains(": error:") || header.contains("error TS")
        {
            error_blocks.push((start, end));
        } else {
            warning_blocks.push((start, end));
        }
    }

    // Build result: errors first, then warnings, capped by MAX_DIAGNOSTIC_BLOCKS
    let mut selected: Vec<(usize, usize)> = Vec::new();
    for block in &error_blocks {
        if selected.len() >= MAX_DIAGNOSTIC_BLOCKS {
            break;
        }
        selected.push(*block);
    }
    for block in &warning_blocks {
        if selected.len() >= MAX_DIAGNOSTIC_BLOCKS {
            break;
        }
        selected.push(*block);
    }

    // Also grab the last few lines (summary — e.g., "error: could not compile", "warning: N warnings emitted")
    let summary_start = lines.len().saturating_sub(5);
    let summary_lines: Vec<&str> = lines[summary_start..].to_vec();

    // Build the output
    let mut result = String::with_capacity(max_chars);

    // Header: how many diagnostics total
    let total_errors = error_blocks.len();
    let total_warnings = warning_blocks.len();
    let shown = selected.len();
    let total_diags = total_errors + total_warnings;
    let omitted_diags = total_diags.saturating_sub(shown);

    result.push_str(&format!(
        "[compiler output: {} {}, {} {} — showing {}",
        total_errors,
        pluralize(total_errors, "error", "errors"),
        total_warnings,
        pluralize(total_warnings, "warning", "warnings"),
        shown,
    ));
    if omitted_diags > 0 {
        result.push_str(&format!(", {} more omitted", omitted_diags,));
    }
    result.push_str("]\n\n");

    // Emit selected diagnostic blocks
    for (idx, &(start, end)) in selected.iter().enumerate() {
        for line in &lines[start..end] {
            result.push_str(line);
            result.push('\n');
        }
        if idx < selected.len() - 1 {
            result.push('\n');
        }
    }

    // Emit summary
    if !summary_lines.is_empty() {
        result.push_str("\n---\n");
        for line in &summary_lines {
            result.push_str(line);
            result.push('\n');
        }
    }

    // If our result is still over max_chars, truncate it with head/tail as fallback
    if result.len() > max_chars {
        let result_lines: Vec<&str> = result.lines().collect();
        let total = result_lines.len();
        if total <= TRUNCATION_HEAD_LINES + TRUNCATION_TAIL_LINES {
            // Already short enough in lines, just return as-is
            return Some(result);
        }
        let head = &result_lines[..TRUNCATION_HEAD_LINES];
        let tail = &result_lines[total - TRUNCATION_TAIL_LINES..];
        let omitted = total - TRUNCATION_HEAD_LINES - TRUNCATION_TAIL_LINES;
        let mut truncated = String::with_capacity(max_chars);
        for line in head {
            truncated.push_str(line);
            truncated.push('\n');
        }
        truncated.push_str(&format!(
            "\n[... truncated {omitted} {} ...]\n\n",
            pluralize(omitted, "line", "lines"),
        ));
        for (i, line) in tail.iter().enumerate() {
            truncated.push_str(line);
            if i < tail.len() - 1 {
                truncated.push('\n');
            }
        }
        return Some(truncated);
    }

    Some(result)
}

/// Format a summary line for a batch of tool executions within a single turn.
///
/// Example output: `  3 tools completed in 1.2s (3 ✓, 0 ✗)`
/// When all succeed: `  3 tools completed in 1.2s (3 ✓)`
/// When some fail: `  3 tools completed in 1.2s (2 ✓, 1 ✗)`
/// Single tool batches return empty (not worth summarizing).
pub fn format_tool_batch_summary(
    total: usize,
    succeeded: usize,
    failed: usize,
    total_duration: std::time::Duration,
) -> String {
    if total <= 1 {
        return String::new();
    }
    let dur = format_duration(total_duration);
    let tool_word = pluralize(total, "tool", "tools");
    let status = if failed == 0 {
        format!("{succeeded} {GREEN}✓{RESET}")
    } else {
        format!("{succeeded} {GREEN}✓{RESET}, {failed} {RED}✗{RESET}")
    };
    format!("{DIM}  {total} {tool_word} completed in {dur}{RESET} ({status})")
}

/// Indent multi-line tool output under its tool header.
///
/// Each line of output gets a `    │ ` prefix for visual nesting.
/// Single-line output is returned as-is with the prefix.
/// Empty input returns empty string.
pub fn indent_tool_output(output: &str) -> String {
    if output.is_empty() {
        return String::new();
    }
    output
        .lines()
        .map(|line| format!("{DIM}    │ {RESET}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Maximum lines to include when auto-truncating a large file for /add.
pub const ADD_MAX_LINES: usize = 500;

/// Truncate file content for context injection (used by /add).
/// Preserves head (40%) and tail (20%) with a clear omission marker
/// showing how many lines were skipped.
/// Returns `(truncated_content, was_truncated, original_line_count)`.
pub fn smart_truncate_for_context(content: &str, max_lines: usize) -> (String, bool, usize) {
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();

    if total <= max_lines {
        return (content.to_string(), false, total);
    }

    // 40% head, 20% tail — gives more context at the top (imports, types, structs)
    let head_count = (max_lines * 2) / 5;
    let tail_count = max_lines / 5;
    // Guard: if max_lines is too small for meaningful head/tail split, just take head
    let tail_count = tail_count.min(total.saturating_sub(head_count));
    let omitted = total - head_count - tail_count;

    let mut result = String::new();
    for line in &lines[..head_count] {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(&format!(
        "\n[... {} lines omitted ({} total) — use /add file:START-END for specific sections ...]\n\n",
        omitted, total
    ));
    if tail_count > 0 {
        for (i, line) in lines[total - tail_count..].iter().enumerate() {
            result.push_str(line);
            if i < tail_count - 1 {
                result.push('\n');
            }
        }
    }

    (result, true, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_truncate_tool_output_under_threshold_unchanged() {
        let short = "hello world\nsecond line\nthird line";
        let result = truncate_tool_output(short, 30_000);
        assert_eq!(result, short);
    }

    #[test]
    fn test_truncate_tool_output_empty_string() {
        let result = truncate_tool_output("", 30_000);
        assert_eq!(result, "");
    }

    #[test]
    fn test_truncate_tool_output_exactly_at_threshold() {
        // Create output exactly at the threshold.
        // Each line starts with a unique first word so compress won't collapse them.
        let lines: Vec<String> = (0..300)
            .map(|i| format!("L{i} {}", "x".repeat(100)))
            .collect();
        let output = lines.join("\n");
        // If it's at or under threshold length, it should be unchanged
        let result = truncate_tool_output(&output, output.len());
        assert_eq!(result, output);
    }

    #[test]
    fn test_truncate_tool_output_over_threshold_has_marker() {
        // Create output with 200 lines, each long enough to exceed 30k chars
        let line = "x".repeat(200);
        let lines: Vec<String> = (0..200).map(|i| format!("line{i}: {line}")).collect();
        let output = lines.join("\n");
        assert!(output.len() > 30_000);

        let result = truncate_tool_output(&output, 30_000);
        assert!(result.contains("[... truncated"));
        assert!(result.contains("lines ...]"));
        // Should contain head lines
        assert!(result.contains("line0:"));
        assert!(result.contains("line99:"));
        // Should contain tail lines
        assert!(result.contains("line199:"));
        assert!(result.contains("line150:"));
        // Should NOT contain middle lines
        assert!(!result.contains("line100:"));
        assert!(!result.contains("line120:"));
    }

    #[test]
    fn test_truncate_tool_output_preserves_head_and_tail_count() {
        // 300 lines, each 200 chars → ~60k chars, well over 30k threshold.
        // Each line starts with a unique first word to avoid compression collapsing.
        let lines: Vec<String> = (0..300).map(|i| format!("U{i} {:>200}", i)).collect();
        let output = lines.join("\n");

        let result = truncate_tool_output(&output, 30_000);
        let _result_lines: Vec<&str> = result.lines().collect();

        // Head: first 100 lines should be present
        for i in 0..100 {
            let expected = format!("U{i} {:>200}", i);
            assert!(result.contains(&expected), "Missing head line {i}");
        }

        // Tail: last 50 lines should be present
        for i in 250..300 {
            let expected = format!("U{i} {:>200}", i);
            assert!(result.contains(&expected), "Missing tail line {i}");
        }

        // Middle should be omitted
        assert!(!result.contains(&format!("U150 {:>200}", 150)));

        // Marker should show correct count
        // 300 - 100 - 50 = 150 omitted lines
        assert!(result.contains("[... truncated 150 lines ...]"));

        // Result should be shorter than original
        assert!(result.len() < output.len());
    }

    #[test]
    fn test_truncate_tool_output_few_long_lines_not_truncated() {
        // Only 140 lines (< head + tail = 150), even if over char threshold
        // Should NOT be truncated because there aren't enough lines.
        // Each line starts with a unique first word to avoid compression collapsing.
        let lines: Vec<String> = (0..140)
            .map(|i| format!("L{i} {}", "x".repeat(500)))
            .collect();
        let output = lines.join("\n");
        assert!(output.len() > 30_000);

        let result = truncate_tool_output(&output, 30_000);
        assert_eq!(
            result, output,
            "Too few lines to truncate, should be unchanged"
        );
    }

    #[test]
    fn test_truncate_tool_output_single_truncated_line_in_marker() {
        // 151 lines → head 100 + tail 50 + 1 omitted → "line" (singular).
        // Each line starts with a unique first word to avoid compression collapsing.
        let lines: Vec<String> = (0..151)
            .map(|i| format!("L{i} {}", "x".repeat(300)))
            .collect();
        let output = lines.join("\n");
        assert!(output.len() > 30_000);

        let result = truncate_tool_output(&output, 30_000);
        assert!(result.contains("[... truncated 1 line ...]"));
    }

    #[test]
    fn test_truncate_tool_output_default_threshold_constant() {
        // Verify the default constant is 30,000
        assert_eq!(TOOL_OUTPUT_MAX_CHARS, 30_000);
    }

    #[test]
    fn test_tool_output_max_chars_piped_smaller() {
        // Piped/CI mode limit should be strictly less than interactive limit
        const _: () = assert!(TOOL_OUTPUT_MAX_CHARS_PIPED < TOOL_OUTPUT_MAX_CHARS);
    }

    #[test]
    fn test_tool_output_max_chars_piped_value() {
        // Piped/CI mode limit should be 15,000
        assert_eq!(TOOL_OUTPUT_MAX_CHARS_PIPED, 15_000);
    }

    #[test]
    fn test_truncate_tool_output_with_custom_limit() {
        // Verify truncation respects a custom (small) limit.
        // Each line starts with a unique first word to avoid compression collapsing.
        let output = (0..200)
            .map(|i| format!("W{i} data"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_tool_output(&output, 100);
        // Output is well over 100 chars and has 200 lines (> head+tail),
        // so it should be truncated
        assert!(
            result.contains("[... truncated"),
            "Should be truncated with 100-char limit, got length {}",
            result.len()
        );
    }

    #[test]
    fn test_truncate_tool_output_respects_limit_parameter() {
        // Same output should NOT be truncated with a large limit but SHOULD be with a small one.
        // Each line starts with a unique first word to avoid compression collapsing.
        let output = (0..200)
            .map(|i| format!("R{i} data"))
            .collect::<Vec<_>>()
            .join("\n");
        let large_limit_result = truncate_tool_output(&output, 1_000_000);
        let small_limit_result = truncate_tool_output(&output, 100);
        assert_eq!(
            large_limit_result, output,
            "Large limit should return output unchanged"
        );
        assert_ne!(
            small_limit_result, output,
            "Small limit should truncate the output"
        );
    }

    // ── decode_html_entities tests ──────────────────────────────────

    #[test]
    fn test_tool_batch_summary_single_tool_returns_empty() {
        let result = format_tool_batch_summary(1, 1, 0, Duration::from_millis(500));
        assert!(
            result.is_empty(),
            "single tool batch should not produce summary"
        );
    }

    #[test]
    fn test_tool_batch_summary_zero_tools_returns_empty() {
        let result = format_tool_batch_summary(0, 0, 0, Duration::from_millis(0));
        assert!(result.is_empty(), "zero tools should not produce summary");
    }

    #[test]
    fn test_tool_batch_summary_all_succeed() {
        let result = format_tool_batch_summary(3, 3, 0, Duration::from_millis(1200));
        assert!(result.contains("3 tools"), "should show tool count");
        assert!(result.contains("1.2s"), "should show duration");
        assert!(result.contains("3"), "should show success count");
        assert!(result.contains("✓"), "should show success marker");
        // When all succeed, no failure count shown
        assert!(
            !result.contains("✗"),
            "should not show failure marker when all succeed"
        );
    }

    #[test]
    fn test_tool_batch_summary_with_failures() {
        let result = format_tool_batch_summary(4, 3, 1, Duration::from_millis(2500));
        assert!(result.contains("4 tools"), "should show total count");
        assert!(result.contains("2.5s"), "should show duration");
        assert!(result.contains("3"), "should show success count");
        assert!(result.contains("✓"), "should show success marker");
        assert!(result.contains("1"), "should show failure count");
        assert!(result.contains("✗"), "should show failure marker");
    }

    #[test]
    fn test_tool_batch_summary_two_tools_plural() {
        let result = format_tool_batch_summary(2, 2, 0, Duration::from_millis(800));
        assert!(result.contains("2 tools"), "should pluralize 'tools'");
        assert!(result.contains("800ms"), "should show ms for sub-second");
    }

    // ── indent tool output tests ──────────────────────────────────

    #[test]
    fn test_indent_tool_output_empty() {
        assert_eq!(indent_tool_output(""), "");
    }

    #[test]
    fn test_indent_tool_output_single_line() {
        let result = indent_tool_output("hello world");
        assert!(result.contains("│"), "should have indent marker");
        assert!(result.contains("hello world"), "should preserve content");
    }

    #[test]
    fn test_indent_tool_output_multiline() {
        let result = indent_tool_output("line 1\nline 2\nline 3");
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3, "should preserve line count");
        for line in &lines {
            assert!(line.contains("│"), "each line should have indent marker");
        }
        assert!(lines[0].contains("line 1"));
        assert!(lines[1].contains("line 2"));
        assert!(lines[2].contains("line 3"));
    }

    // ── filter_noisy_patterns tests ──────────────────────────────────

    #[test]
    fn test_noisy_compiling_lines_collapse() {
        let mut lines = Vec::new();
        for i in 0..20 {
            lines.push(format!("   Compiling crate_{i} v0.{i}.0"));
        }
        let input = lines.join("\n");
        let result = filter_noisy_patterns(&input);
        assert!(
            result.contains("Compiling crate_0 v0.0.0"),
            "should keep first: {result}"
        );
        assert!(
            result.contains("... (18 more)"),
            "should collapse middle: {result}"
        );
        assert!(
            result.contains("Compiling crate_19 v0.19.0"),
            "should keep last: {result}"
        );
        // Should NOT contain middle lines
        assert!(
            !result.contains("crate_5"),
            "should not contain middle lines: {result}"
        );
    }

    #[test]
    fn test_noisy_downloading_lines_collapse() {
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!("   Downloading dep_{i} v1.{i}.0"));
        }
        let input = lines.join("\n");
        let result = filter_noisy_patterns(&input);
        assert!(result.contains("... (8 more)"), "got: {result}");
        assert!(result.contains("dep_0"), "should keep first: {result}");
        assert!(result.contains("dep_9"), "should keep last: {result}");
    }

    #[test]
    fn test_noisy_short_compiling_run_kept() {
        let input = "   Compiling foo v1.0.0\n   Compiling bar v2.0.0";
        let result = filter_noisy_patterns(input);
        assert!(result.contains("foo"), "short run should be kept: {result}");
        assert!(result.contains("bar"), "short run should be kept: {result}");
        assert!(
            !result.contains("more"),
            "no collapse for short run: {result}"
        );
    }

    #[test]
    fn test_noisy_lock_waiting_removed() {
        let input = "   Blocking waiting for file lock on package cache\nreal output here";
        let result = filter_noisy_patterns(input);
        assert!(!result.contains("Blocking"), "lock line should be removed");
        assert!(result.contains("real output here"), "real output kept");
    }

    #[test]
    fn test_noisy_progress_bar_removed() {
        let input = "Building [████████████████████] 95%\nDone.";
        let result = filter_noisy_patterns(input);
        assert!(!result.contains("████"), "progress bar should be removed");
        assert!(result.contains("Done."), "non-progress line kept");
    }

    #[test]
    fn test_noisy_progress_bar_thin_chars_removed() {
        let input = "Progress ━━━━━━━━━━ 50%\nFinished.";
        let result = filter_noisy_patterns(input);
        assert!(!result.contains("━━━"), "thin bar should be removed");
        assert!(result.contains("Finished."), "non-progress line kept");
    }

    #[test]
    fn test_noisy_npm_warn_filtered() {
        let input = [
            "npm warn optional SKIPPING OPTIONAL DEPENDENCY",
            "npm warn deprecated lodash@3.0.0: use lodash@4",
            "npm warn peer missing: react@>=16",
            "npm WARN vulnerability found 2 vulnerabilities",
        ]
        .join("\n");
        let result = filter_noisy_patterns(&input);
        assert!(
            result.contains("deprecated"),
            "should keep deprecated warning: {result}"
        );
        assert!(
            result.contains("vulnerability"),
            "should keep vulnerability warning: {result}"
        );
        assert!(
            !result.contains("SKIPPING"),
            "should remove generic npm warn: {result}"
        );
        assert!(
            !result.contains("peer missing"),
            "should remove peer warn: {result}"
        );
    }

    #[test]
    fn test_noisy_pip_already_satisfied_removed() {
        let input =
            "Requirement already satisfied: requests in /usr/lib/python3\nInstalling collected packages: foo";
        let result = filter_noisy_patterns(input);
        assert!(
            !result.contains("already satisfied"),
            "pip line should be removed"
        );
        assert!(result.contains("Installing"), "other pip output kept");
    }

    #[test]
    fn test_noisy_git_hash_abbreviated() {
        let hash = "a".repeat(40);
        let input = format!("commit {hash}\nAuthor: Test User <test@example.com>");
        let result = filter_noisy_patterns(&input);
        assert!(
            result.contains("commit aaaaaaa..."),
            "should abbreviate hash: {result}"
        );
        assert!(
            !result.contains(&hash),
            "should not contain full hash: {result}"
        );
    }

    #[test]
    fn test_noisy_git_author_date_consolidated() {
        let input = "Author:     Jane   Doe   <jane@example.com>\nDate:       Mon Apr  7 12:00:00 2025 +0000";
        let result = filter_noisy_patterns(input);
        assert!(
            result.contains("Author: Jane Doe <jane@example.com>"),
            "should consolidate whitespace: {result}"
        );
        assert!(
            result.contains("Date: Mon Apr 7 12:00:00 2025 +0000"),
            "should consolidate date whitespace: {result}"
        );
    }

    #[test]
    fn test_noisy_empty_lines_collapsed_to_two() {
        let input = "line1\n\n\n\n\nline2";
        let result = filter_noisy_patterns(input);
        // Count empty lines between line1 and line2
        let parts: Vec<&str> = result.split("line1").collect();
        assert!(parts.len() >= 2, "should have content around line1");
        let between = parts[1].split("line2").next().unwrap_or("");
        let empty_count = between.matches('\n').count();
        // Should be exactly 2 empty lines = 3 newline chars (line1\n\n\nline2)
        assert!(
            empty_count <= 3,
            "should collapse to max 2 empty lines, got {empty_count} newlines between: '{between}'"
        );
        assert!(result.contains("line1"), "should keep line1");
        assert!(result.contains("line2"), "should keep line2");
    }

    #[test]
    fn test_noisy_two_empty_lines_kept() {
        let input = "a\n\n\nb";
        let result = filter_noisy_patterns(input);
        // 2 empty lines should be kept as-is
        assert_eq!(result, "a\n\n\nb", "2 empty lines should be preserved");
    }

    #[test]
    fn test_noisy_passthrough_normal_lines() {
        let input = "error[E0308]: mismatched types\n  --> src/main.rs:42:5\n   |\n42 |     let x: u32 = \"hello\";\n   |                  ^^^^^^^ expected u32";
        let result = filter_noisy_patterns(input);
        assert_eq!(result, input, "normal lines should pass through unchanged");
    }

    #[test]
    fn test_noisy_downloaded_summary_kept() {
        let input = "   Downloading foo v1.0.0\n   Downloading bar v2.0.0\n   Downloading baz v3.0.0\n   Downloading qux v4.0.0\n  Downloaded 4 crates (2.5 MB) in 1.2s";
        let result = filter_noisy_patterns(input);
        assert!(
            result.contains("Downloaded 4 crates"),
            "should keep download summary: {result}"
        );
    }

    #[test]
    fn test_noisy_integration_with_compress() {
        // Verify filter_noisy_patterns works inside compress_tool_output
        let mut lines = Vec::new();
        for i in 0..15 {
            lines.push(format!("   Compiling dep_{i} v0.{i}.0"));
        }
        lines.push(String::from("   Compiling my_project v0.1.0"));
        lines.push(String::from("error[E0308]: mismatched types"));
        let input = lines.join("\n");
        let result = compress_tool_output(&input);
        assert!(
            result.contains("... (14 more)"),
            "compress_tool_output should include noisy filter: {result}"
        );
        assert!(
            result.contains("error[E0308]"),
            "should keep error lines: {result}"
        );
    }

    // ── compress_tool_output tests ────────────────────────────────────

    #[test]
    fn test_compress_strips_ansi_codes() {
        let input = "\x1b[31merror\x1b[0m: something \x1b[1;33mwent\x1b[0m wrong";
        let result = compress_tool_output(input);
        assert_eq!(result, "error: something went wrong");
        assert!(!result.contains("\x1b"));
    }

    #[test]
    fn test_compress_strips_various_ansi_sequences() {
        // SGR, cursor movement, erase
        let input = "\x1b[32mgreen\x1b[0m \x1b[2Kclear \x1b[1Aup \x1b[38;5;196mcolor256\x1b[0m";
        let result = compress_tool_output(input);
        assert!(!result.contains("\x1b"), "still has ANSI: {result}");
        assert!(result.contains("green"));
        assert!(result.contains("color256"));
    }

    #[test]
    fn test_compress_collapses_repetitive_lines() {
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!("   Compiling foo-{i} v1.0.{i}"));
        }
        let input = lines.join("\n");
        let result = compress_tool_output(&input);
        let result_lines: Vec<&str> = result.lines().collect();
        // Should have first line, collapse marker, last line = 3 lines
        assert_eq!(result_lines.len(), 3, "got: {result}");
        assert!(
            result_lines[0].contains("foo-0"),
            "first: {}",
            result_lines[0]
        );
        // Now handled by filter_noisy_patterns with "N more" wording
        assert!(
            result_lines[1].contains("8 more"),
            "marker: {}",
            result_lines[1]
        );
        assert!(
            result_lines[2].contains("foo-9"),
            "last: {}",
            result_lines[2]
        );
    }

    #[test]
    fn test_compress_preserves_non_repetitive_output() {
        let input = "line one\nline two\nline three\nsomething different";
        let result = compress_tool_output(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_compress_short_output_unchanged() {
        // Only 3 similar Compiling lines — filter_noisy_patterns collapses at 3+
        let input = "   Compiling a v1.0\n   Compiling b v1.0\n   Compiling c v1.0";
        let result = compress_tool_output(input);
        // Should collapse: first + "... (1 more)" + last
        assert!(
            result.contains("Compiling a"),
            "should keep first: {result}"
        );
        assert!(result.contains("Compiling c"), "should keep last: {result}");
        assert!(
            result.contains("1 more"),
            "should collapse middle: {result}"
        );
    }

    #[test]
    fn test_compress_mixed_repetitive_blocks() {
        let mut lines = Vec::new();
        for i in 0..5 {
            lines.push(format!("   Compiling crate-{i} v0.1.0"));
        }
        lines.push("warning: unused variable".to_string());
        lines.push("  --> src/main.rs:10:5".to_string());
        for i in 0..6 {
            lines.push(format!("  Downloading dep-{i} v2.0.0"));
        }
        let input = lines.join("\n");
        let result = compress_tool_output(&input);
        // Both repetitive blocks collapsed by filter_noisy_patterns
        assert!(result.contains("3 more"), "compiling block: {result}");
        assert!(result.contains("4 more"), "downloading block: {result}");
        // Non-repetitive lines preserved
        assert!(result.contains("warning: unused variable"));
        assert!(result.contains("--> src/main.rs:10:5"));
    }

    #[test]
    fn test_truncate_uses_compression() {
        // Verify truncate_tool_output strips ANSI codes from output
        let input = "\x1b[32mhello\x1b[0m world";
        let result = truncate_tool_output(input, 100_000);
        assert!(!result.contains("\x1b"), "ANSI not stripped: {result}");
        assert!(result.contains("hello world"));
    }

    #[test]
    fn test_compress_exact_threshold_four_lines() {
        // Exactly 4 Compiling lines — filter_noisy_patterns collapses at 3+
        let input = "   Compiling a v1\n   Compiling b v1\n   Compiling c v1\n   Compiling d v1";
        let result = compress_tool_output(input);
        let result_lines: Vec<&str> = result.lines().collect();
        assert_eq!(result_lines.len(), 3, "got: {result}");
        assert!(
            result_lines[1].contains("2 more"),
            "got: {}",
            result_lines[1]
        );
    }

    #[test]
    fn test_compress_empty_input() {
        assert_eq!(compress_tool_output(""), "");
    }

    #[test]
    fn test_compress_pip_install_pattern() {
        let mut lines = Vec::new();
        for i in 0..8 {
            lines.push(format!("Installing package-{i}==1.0.{i}"));
        }
        let input = lines.join("\n");
        let result = compress_tool_output(&input);
        let result_lines: Vec<&str> = result.lines().collect();
        assert_eq!(result_lines.len(), 3, "got: {result}");
        assert!(result_lines[1].contains("6 more similar"));
    }

    #[test]
    fn test_strip_ansi_preserves_multibyte_utf8() {
        // ✓ is 3 bytes (0xE2 0x9C 0x93), 日本語 has 3-byte chars
        let input = "\x1b[32m✓\x1b[0m passed: 日本語テスト";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "✓ passed: 日本語テスト");
    }

    #[test]
    fn test_strip_ansi_preserves_emoji() {
        // Emoji are 4-byte UTF-8 characters
        let input = "\x1b[1m🦀 Rust\x1b[0m is 🔥";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "🦀 Rust is 🔥");
    }

    #[test]
    fn test_strip_ansi_preserves_accented_chars() {
        // é is 2 bytes (0xC3 0xA9)
        let input = "\x1b[33mcafé\x1b[0m résumé";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "café résumé");
    }

    #[test]
    fn test_compress_multibyte_content() {
        // End-to-end: compress_tool_output should handle multi-byte chars
        let input = "\x1b[32m✓\x1b[0m テスト完了";
        let result = compress_tool_output(input);
        assert_eq!(result, "✓ テスト完了");
    }

    #[test]
    fn test_line_category_multibyte_prefix() {
        // "日本語テストの結" = 8 chars × 3 bytes = 24 bytes, no spaces.
        // first_word_end = 24 (no whitespace found), prefix_len = 24,
        // min(24, CATEGORY_PREFIX_MAX=20) = 20, but byte 20 is inside
        // the 7th character (bytes 18-20). Must not panic.
        let line = "日本語テストの結";
        let _cat = line_category(line); // Should not panic
    }

    #[test]
    fn test_line_category_multibyte_short_word() {
        // "café something" — first word "café" is 5 chars but 6 bytes
        let line = "café something";
        let cat = line_category(line);
        assert_eq!(cat, "café");
    }

    #[test]
    fn test_collapse_repetitive_multibyte_lines() {
        // Lines with multi-byte content that share a category
        let mut lines = Vec::new();
        for i in 0..6 {
            lines.push(format!("コンパイル中 パッケージ-{i} v1.0"));
        }
        let input = lines.join("\n");
        let result = collapse_repetitive_lines(&input);
        let result_lines: Vec<&str> = result.lines().collect();
        assert_eq!(result_lines.len(), 3, "got: {result}");
        assert!(result_lines[1].contains("4 more similar"));
    }

    // ── filter_test_output tests ────────────────────────────────────

    #[test]
    fn test_filter_cargo_test_all_passing() {
        let mut lines = Vec::new();
        for i in 0..20 {
            lines.push(format!("test tests::test_case_{i} ... ok"));
        }
        lines.push(String::new());
        lines.push("test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.50s".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        assert!(
            result.contains("(20 passing tests omitted)"),
            "should omit passing tests, got: {result}"
        );
        assert!(
            result.contains("test result: ok."),
            "should keep summary, got: {result}"
        );
        // Should be much shorter than input
        assert!(
            result.lines().count() < 5,
            "should be very short, got {} lines: {result}",
            result.lines().count()
        );
    }

    #[test]
    fn test_filter_cargo_test_with_failures() {
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!("test tests::test_pass_{i} ... ok"));
        }
        lines.push("test tests::test_broken ... FAILED".to_string());
        for i in 10..15 {
            lines.push(format!("test tests::test_pass_{i} ... ok"));
        }
        lines.push(String::new());
        lines.push("failures:".to_string());
        lines.push(String::new());
        lines.push("---- tests::test_broken stdout ----".to_string());
        lines.push("thread 'tests::test_broken' panicked at 'assertion failed'".to_string());
        lines.push(String::new());
        lines.push("failures:".to_string());
        lines.push("    tests::test_broken".to_string());
        lines.push(String::new());
        lines.push("test result: FAILED. 15 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.0s".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        // Failures must be preserved
        assert!(
            result.contains("test tests::test_broken ... FAILED"),
            "should keep failure line, got: {result}"
        );
        // Failure details must be preserved
        assert!(
            result.contains("assertion failed"),
            "should keep failure details, got: {result}"
        );
        // Summary must be preserved
        assert!(
            result.contains("test result: FAILED."),
            "should keep summary, got: {result}"
        );
        // Passing tests should be omitted
        assert!(
            result.contains("passing tests omitted"),
            "should omit passing tests, got: {result}"
        );
        assert!(
            !result.contains("test_pass_5 ... ok"),
            "should not contain passing test lines, got: {result}"
        );
    }

    #[test]
    fn test_filter_cargo_test_failure_details_preserved() {
        let mut lines = Vec::new();
        for i in 0..5 {
            lines.push(format!("test test_{i} ... ok"));
        }
        lines.push("test test_bad ... FAILED".to_string());
        lines.push(String::new());
        lines.push("failures:".to_string());
        lines.push(String::new());
        lines.push("---- test_bad stdout ----".to_string());
        lines.push("thread 'test_bad' panicked at src/lib.rs:42:".to_string());
        lines.push("assertion `left == right` failed".to_string());
        lines.push("  left: 1".to_string());
        lines.push("  right: 2".to_string());
        lines.push("note: run with `RUST_BACKTRACE=1`".to_string());
        lines.push(String::new());
        lines.push("failures:".to_string());
        lines.push("    test_bad".to_string());
        lines.push(String::new());
        lines.push(
            "test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out"
                .to_string(),
        );
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        // All failure details must be present
        assert!(
            result.contains("thread 'test_bad' panicked"),
            "got: {result}"
        );
        assert!(result.contains("left: 1"), "got: {result}");
        assert!(result.contains("right: 2"), "got: {result}");
        assert!(result.contains("RUST_BACKTRACE"), "got: {result}");
    }

    #[test]
    fn test_filter_pytest_output() {
        let mut lines = Vec::new();
        lines.push(
            "============================= test session starts ============================="
                .to_string(),
        );
        lines.push("collected 15 items".to_string());
        lines.push(String::new());
        for i in 0..12 {
            lines.push(format!("tests/test_app.py::test_case_{i} PASSED"));
        }
        lines.push("tests/test_app.py::test_broken FAILED".to_string());
        lines.push("tests/test_app.py::test_another PASSED".to_string());
        lines.push("tests/test_app.py::test_more PASSED".to_string());
        lines.push(String::new());
        lines.push(
            "=========================== short test summary info ==========================="
                .to_string(),
        );
        lines.push("FAILED tests/test_app.py::test_broken - AssertionError".to_string());
        lines.push(
            "========================= 14 passed, 1 failed =========================".to_string(),
        );
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        assert!(
            result.contains("passing tests omitted"),
            "should omit passing pytest tests, got: {result}"
        );
        assert!(
            result.contains("test_broken FAILED"),
            "should keep failures, got: {result}"
        );
        assert!(
            result.contains("14 passed, 1 failed"),
            "should keep summary, got: {result}"
        );
    }

    #[test]
    fn test_filter_jest_output() {
        let mut lines = Vec::new();
        lines.push("PASS src/app.test.js".to_string());
        lines.push("  App component".to_string());
        for i in 0..10 {
            lines.push(format!("    ✓ should render item {i} (5ms)"));
        }
        lines.push("    ✕ should handle error (10ms)".to_string());
        lines.push(String::new());
        lines.push("Tests:  1 failed, 10 passed, 11 total".to_string());
        lines.push("Time:   2.5s".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        assert!(
            result.contains("passing tests omitted"),
            "should omit passing jest tests, got: {result}"
        );
        assert!(
            result.contains("should handle error"),
            "should keep failure, got: {result}"
        );
        assert!(
            result.contains("Tests:"),
            "should keep summary, got: {result}"
        );
    }

    #[test]
    fn test_filter_go_test_output() {
        let mut lines = Vec::new();
        for i in 0..8 {
            lines.push(format!("--- PASS: TestCase{i} (0.00s)"));
        }
        lines.push("--- FAIL: TestBroken (0.01s)".to_string());
        lines.push("    expected: 1, got: 2".to_string());
        lines.push("FAIL".to_string());
        lines.push("FAIL    github.com/user/repo    0.05s".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        assert!(
            result.contains("passing tests omitted"),
            "should omit passing go tests, got: {result}"
        );
        assert!(
            result.contains("--- FAIL: TestBroken"),
            "should keep failure, got: {result}"
        );
        assert!(
            result.contains("expected: 1, got: 2"),
            "should keep failure details, got: {result}"
        );
    }

    #[test]
    fn test_filter_non_test_output_unchanged() {
        let input = "hello world\nthis is regular output\nnothing to see here\nfoo bar baz";
        let result = filter_test_output(input);
        assert_eq!(
            result, input,
            "non-test output should pass through unchanged"
        );
    }

    #[test]
    fn test_filter_mixed_content() {
        // Compilation output followed by test output
        let mut lines = vec![
            "   Compiling myapp v0.1.0".to_string(),
            "   Compiling dep v1.0.0".to_string(),
            "    Finished test [unoptimized + debuginfo] target(s) in 5.00s".to_string(),
            "     Running unittests src/lib.rs".to_string(),
            String::new(),
            "running 15 tests".to_string(),
        ];
        for i in 0..15 {
            lines.push(format!("test tests::test_case_{i} ... ok"));
        }
        lines.push(String::new());
        lines.push("test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        // Compilation output should be preserved
        assert!(
            result.contains("Compiling myapp"),
            "should keep compilation output, got: {result}"
        );
        // Passing tests should be omitted
        assert!(
            result.contains("passing tests omitted"),
            "should omit passing tests, got: {result}"
        );
        // Summary should be preserved
        assert!(
            result.contains("test result: ok."),
            "should keep test summary, got: {result}"
        );
    }

    #[test]
    fn test_compress_tool_output_integrates_test_filter() {
        // Verify compress_tool_output calls the test filter
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!("\x1b[32mtest test_{i} ... ok\x1b[0m"));
        }
        lines.push(String::new());
        lines.push("\x1b[32mtest result: ok. 10 passed; 0 failed; 0 ignored\x1b[0m".to_string());
        let input = lines.join("\n");
        let result = compress_tool_output(&input);
        // Should have stripped ANSI AND filtered test output
        assert!(!result.contains("\x1b"), "should strip ANSI, got: {result}");
        assert!(
            result.contains("passing tests omitted"),
            "should filter test output, got: {result}"
        );
    }

    #[test]
    fn test_smart_truncate_under_limit() {
        let content = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (result, truncated, total) = smart_truncate_for_context(&content, 500);
        assert!(!truncated);
        assert_eq!(total, 100);
        assert_eq!(result, content);
    }

    #[test]
    fn test_smart_truncate_at_limit() {
        let content = (0..500)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (result, truncated, total) = smart_truncate_for_context(&content, 500);
        assert!(!truncated);
        assert_eq!(total, 500);
        assert_eq!(result, content);
    }

    #[test]
    fn test_smart_truncate_over_limit() {
        let content = (0..1000)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (result, truncated, total) = smart_truncate_for_context(&content, 500);
        assert!(truncated);
        assert_eq!(total, 1000);
        // Head: 200 lines (40% of 500)
        assert!(result.contains("line 0"));
        assert!(result.contains("line 199"));
        // Tail: 100 lines (20% of 500)
        assert!(result.contains("line 900"));
        assert!(result.contains("line 999"));
        // Omission marker
        assert!(result.contains("[... 700 lines omitted (1000 total)"));
        assert!(result.contains("use /add file:START-END"));
        // Middle should be gone
        assert!(!result.contains("line 500"));
    }

    #[test]
    fn test_smart_truncate_omission_counts() {
        let content = (0..600)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (result, truncated, total) = smart_truncate_for_context(&content, 500);
        assert!(truncated);
        assert_eq!(total, 600);
        // Head: 200, Tail: 100, Omitted: 300
        assert!(result.contains("300 lines omitted (600 total)"));
    }

    #[test]
    fn test_smart_truncate_empty_content() {
        let (result, truncated, total) = smart_truncate_for_context("", 500);
        assert!(!truncated);
        assert_eq!(total, 0);
        assert_eq!(result, "");
    }

    #[test]
    fn test_smart_truncate_one_over_limit() {
        let content = (0..501)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (result, truncated, total) = smart_truncate_for_context(&content, 500);
        assert!(truncated);
        assert_eq!(total, 501);
        // Head: 200, Tail: 100, Omitted: 201
        assert!(result.contains("201 lines omitted (501 total)"));
    }

    #[test]
    fn test_smart_truncate_preserves_head_and_tail_content() {
        let mut lines: Vec<String> = Vec::new();
        lines.push("// FILE HEADER".to_string());
        lines.push("use std::io;".to_string());
        for i in 2..998 {
            lines.push(format!("    middle_line_{i}();"));
        }
        lines.push("fn last_function() {}".to_string());
        lines.push("// EOF".to_string());
        let content = lines.join("\n");
        let (result, truncated, _) = smart_truncate_for_context(&content, 500);
        assert!(truncated);
        // Head should have the file header
        assert!(result.contains("// FILE HEADER"));
        assert!(result.contains("use std::io;"));
        // Tail should have the end
        assert!(result.contains("fn last_function() {}"));
        assert!(result.contains("// EOF"));
    }

    // ========================================================================
    // Day 86: Edge-case coverage for compression, truncation, and filtering
    // ========================================================================

    // --- compress_tool_output edge cases ---

    #[test]
    fn test_compress_empty_returns_empty() {
        assert_eq!(compress_tool_output(""), "");
    }

    #[test]
    fn test_compress_short_input_unchanged_content() {
        // Short input with no ANSI, no repetition — should pass through
        let input = "hello\nworld\nfoo";
        let result = compress_tool_output(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_compress_repeated_blank_lines_collapsed() {
        // 5 blank lines should be collapsed to at most 2
        let input = "start\n\n\n\n\n\nend";
        let result = compress_tool_output(input);
        // filter_noisy_patterns collapses 3+ blanks to 2
        let blank_count = result.lines().filter(|l| l.trim().is_empty()).count();
        assert!(
            blank_count <= 2,
            "Expected at most 2 blank lines, got {blank_count} in:\n{result}"
        );
        assert!(result.contains("start"));
        assert!(result.contains("end"));
    }

    #[test]
    fn test_compress_consecutive_duplicate_lines_collapsed() {
        // 6 identical lines should trigger collapse (COLLAPSE_MIN_LINES = 4)
        let lines: Vec<&str> = vec![
            "warning: unused variable",
            "warning: unused variable",
            "warning: unused variable",
            "warning: unused variable",
            "warning: unused variable",
            "warning: unused variable",
        ];
        let input = lines.join("\n");
        let result = compress_tool_output(&input);
        assert!(
            result.contains("more similar lines"),
            "Expected collapse marker in:\n{result}"
        );
        // Should be shorter than input
        assert!(result.lines().count() < 6);
    }

    #[test]
    fn test_compress_very_long_lines_in_output() {
        // A single very long line should still work without panic
        let long_line = "x".repeat(100_000);
        let result = compress_tool_output(&long_line);
        // Should not panic and should contain the content
        assert!(!result.is_empty());
    }

    #[test]
    fn test_compress_mixed_repetitive_and_unique() {
        // Mix of repetitive sections and unique content
        let mut lines = Vec::new();
        lines.push("unique header".to_string());
        // 5 similar lines (same category prefix)
        for i in 0..5 {
            lines.push(format!("warning: item {i} is unused"));
        }
        lines.push("unique middle".to_string());
        // 4 more similar lines
        for i in 0..4 {
            lines.push(format!("error: cannot find {i}"));
        }
        lines.push("unique footer".to_string());
        let input = lines.join("\n");
        let result = compress_tool_output(&input);
        // Unique lines preserved
        assert!(result.contains("unique header"));
        assert!(result.contains("unique middle"));
        assert!(result.contains("unique footer"));
        // Both repetitive sections should be collapsed
        assert!(
            result.contains("more similar lines"),
            "Expected collapse in:\n{result}"
        );
    }

    #[test]
    fn test_compress_ansi_then_collapse() {
        // ANSI codes should be stripped first, then collapse applies
        let input = "\x1b[31mwarning: x\x1b[0m\n\
                      \x1b[31mwarning: y\x1b[0m\n\
                      \x1b[31mwarning: z\x1b[0m\n\
                      \x1b[31mwarning: w\x1b[0m\n\
                      \x1b[31mwarning: v\x1b[0m";
        let result = compress_tool_output(input);
        // ANSI should be gone
        assert!(!result.contains("\x1b["));
        // 5 lines with same category → collapse
        assert!(
            result.contains("more similar lines"),
            "Expected collapse after ANSI strip in:\n{result}"
        );
    }

    // --- filter_test_output edge cases ---

    #[test]
    fn test_filter_test_no_test_markers_unchanged() {
        // Non-test output should pass through unchanged
        let input = "Building project...\nCompilation succeeded\nDone in 2.3s";
        let result = filter_test_output(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_filter_test_only_passing_no_fails() {
        // All passing tests (>= 5) with no failures → compressed
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!("test test_{i} ... ok"));
        }
        lines.push("test result: ok. 10 passed; 0 failed".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        // Pass lines should be replaced with count marker
        assert!(
            result.contains("10 passing tests omitted"),
            "Expected omission marker in:\n{result}"
        );
        // Summary should be preserved
        assert!(result.contains("test result:"));
        // Individual pass lines should be gone
        assert!(!result.contains("test test_0 ... ok"));
    }

    #[test]
    fn test_filter_test_fewer_than_threshold_unchanged() {
        // Only 3 passing tests (< 5 threshold) — should NOT be filtered
        let mut lines = Vec::new();
        for i in 0..3 {
            lines.push(format!("test test_{i} ... ok"));
        }
        lines.push("test result: ok. 3 passed; 0 failed".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        // Should keep all lines since < threshold
        assert!(result.contains("test test_0 ... ok"));
        assert!(result.contains("test test_1 ... ok"));
        assert!(result.contains("test test_2 ... ok"));
    }

    #[test]
    fn test_filter_test_empty_input() {
        assert_eq!(filter_test_output(""), "");
    }

    #[test]
    fn test_filter_test_preserves_failures_among_passes() {
        // Mix of passes and failures — failures must be kept
        let mut lines = Vec::new();
        for i in 0..8 {
            lines.push(format!("test pass_{i} ... ok"));
        }
        lines.push("test failing_test ... FAILED".to_string());
        lines.push("test result: FAILED. 8 passed; 1 failed".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        // Passes should be omitted
        assert!(result.contains("passing tests omitted"));
        // Failure must be preserved
        assert!(result.contains("test failing_test ... FAILED"));
        // Summary preserved
        assert!(result.contains("test result:"));
    }

    // --- smart_truncate_for_context edge cases ---

    #[test]
    fn test_smart_truncate_exactly_at_limit() {
        // Content exactly at limit should NOT be truncated
        let content = (0..500)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (result, truncated, total) = smart_truncate_for_context(&content, 500);
        assert!(!truncated);
        assert_eq!(total, 500);
        assert_eq!(result, content);
    }

    #[test]
    fn test_smart_truncate_single_line_under_limit() {
        let content = "just one line";
        let (result, truncated, total) = smart_truncate_for_context(content, 500);
        assert!(!truncated);
        assert_eq!(total, 1);
        assert_eq!(result, content);
    }

    #[test]
    fn test_smart_truncate_headers_in_truncated_content() {
        // Multi-section content: headers at start should be preserved in head
        let mut lines = Vec::new();
        lines.push("# Section 1".to_string());
        lines.push("## Subsection A".to_string());
        for i in 0..800 {
            lines.push(format!("body line {i}"));
        }
        lines.push("# Final Section".to_string());
        lines.push("final line".to_string());
        let content = lines.join("\n");
        let (result, truncated, total) = smart_truncate_for_context(&content, 500);
        assert!(truncated);
        assert_eq!(total, 804);
        // Head headers preserved
        assert!(result.contains("# Section 1"));
        assert!(result.contains("## Subsection A"));
        // Tail preserved
        assert!(result.contains("# Final Section"));
        assert!(result.contains("final line"));
        // Omission marker present
        assert!(result.contains("lines omitted"));
    }

    // --- truncate_tool_output UTF-8 safety ---

    #[test]
    fn test_truncate_tool_output_multibyte_utf8_no_panic() {
        // Critical: multi-byte UTF-8 chars at truncation boundaries must not panic
        // ✓ is 3 bytes, 日 is 3 bytes, 🦀 is 4 bytes
        let mut lines = Vec::new();
        for i in 0..200 {
            lines.push(format!("U{i} ✓日本語テスト🦀 {}", "あ".repeat(50)));
        }
        let output = lines.join("\n");
        // Use a small limit to force truncation
        let result = truncate_tool_output(&output, 500);
        // Should not panic, and should contain the truncation marker
        assert!(
            result.contains("[... truncated") || result.len() <= 500,
            "Should either be truncated with marker or under limit"
        );
    }

    #[test]
    fn test_truncate_tool_output_emoji_boundary() {
        // Build output with emoji that are 4 bytes each
        let emoji_line = "🦀🐙🎉🚀✨🌟💫⭐".repeat(30);
        let lines: Vec<String> = (0..200).map(|i| format!("E{i} {emoji_line}")).collect();
        let output = lines.join("\n");
        // This must not panic
        let result = truncate_tool_output(&output, 1000);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_compress_multibyte_category_prefix_no_panic() {
        // Lines where the category prefix lands on a multi-byte boundary
        // 日 is 3 bytes; 7 of them = 21 bytes > CATEGORY_PREFIX_MAX (20)
        let mut lines = Vec::new();
        for i in 0..5 {
            lines.push(format!("日本語テスト甲乙 item {i}"));
        }
        let input = lines.join("\n");
        // Must not panic on char boundary issues
        let result = compress_tool_output(&input);
        assert!(!result.is_empty());
    }

    // --- format_tool_batch_summary edge cases ---

    #[test]
    fn test_tool_batch_summary_large_count() {
        let result = format_tool_batch_summary(15, 12, 3, Duration::from_secs(45));
        assert!(result.contains("15 tools"));
        assert!(result.contains("45.0s"));
        assert!(result.contains("12"));
        assert!(result.contains("3"));
    }

    #[test]
    fn test_tool_batch_summary_two_tools_all_fail() {
        let result = format_tool_batch_summary(2, 0, 2, Duration::from_millis(200));
        assert!(result.contains("2 tools"));
        // Should show failure marker
        assert!(result.contains("✗"));
    }

    #[test]
    fn test_tool_batch_summary_succeeds_no_fail_marker() {
        let result = format_tool_batch_summary(3, 3, 0, Duration::from_millis(800));
        // When all succeed, no failure marker
        assert!(!result.contains("✗"));
        assert!(result.contains("✓"));
    }

    // --- collapse_repetitive_lines edge cases ---

    #[test]
    fn test_collapse_three_similar_lines_not_collapsed() {
        // Exactly 3 similar lines (below COLLAPSE_MIN_LINES=4) — should NOT collapse
        let input = "warning: x\nwarning: y\nwarning: z";
        let result = collapse_repetitive_lines(input);
        assert_eq!(result, input, "3 lines should not be collapsed");
    }

    #[test]
    fn test_collapse_four_similar_lines_collapsed() {
        // Exactly 4 similar lines (= COLLAPSE_MIN_LINES) — SHOULD collapse
        let input = "warning: a\nwarning: b\nwarning: c\nwarning: d";
        let result = collapse_repetitive_lines(input);
        assert!(
            result.contains("more similar lines"),
            "4 lines should be collapsed, got:\n{result}"
        );
        // First and last preserved
        assert!(result.contains("warning: a"));
        assert!(result.contains("warning: d"));
    }

    #[test]
    fn test_collapse_empty_lines_not_collapsed_as_similar() {
        // Empty lines have empty category and should NOT be collapsed by this function
        // (that's handled by filter_noisy_patterns instead)
        let input = "\n\n\n\n\n";
        let result = collapse_repetitive_lines(input);
        // Empty category → no collapse
        assert!(!result.contains("more similar lines"));
    }

    // --- indent_tool_output edge cases ---

    #[test]
    fn test_indent_tool_output_preserves_content() {
        let input = "line 1\nline 2\nline 3";
        let result = indent_tool_output(input);
        // Each line should have the prefix
        for line in result.lines() {
            assert!(
                line.contains("│"),
                "Each line should contain the indent bar: {line}"
            );
        }
        // Original content preserved
        assert!(result.contains("line 1"));
        assert!(result.contains("line 2"));
        assert!(result.contains("line 3"));
    }

    // --- strip_ansi_codes edge cases ---

    #[test]
    fn test_strip_ansi_empty() {
        assert_eq!(strip_ansi_codes(""), "");
    }

    #[test]
    fn test_strip_ansi_no_codes() {
        let input = "plain text with no ANSI";
        assert_eq!(strip_ansi_codes(input), input);
    }

    #[test]
    fn test_strip_ansi_nested_codes() {
        // Multiple ANSI codes in sequence
        let input = "\x1b[1m\x1b[31mBold Red\x1b[0m Normal";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "Bold Red Normal");
    }

    #[test]
    fn test_strip_ansi_mid_multibyte_char() {
        // ANSI code followed immediately by multi-byte UTF-8
        let input = "\x1b[32m日本語\x1b[0m";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "日本語");
    }

    #[test]
    fn test_strip_ansi_osc_with_bel() {
        // OSC sequence terminated by BEL (\x07): set window title
        let input = "\x1b]0;My Title\x07some text";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "some text");
    }

    #[test]
    fn test_strip_ansi_osc_with_st() {
        // OSC sequence terminated by ST (ESC \): set window title
        let input = "\x1b]2;Window Title\x1b\\visible";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "visible");
    }

    #[test]
    fn test_strip_ansi_osc_hyperlink() {
        // OSC 8 hyperlink: ESC ]8;;url BEL text ESC ]8;; BEL
        let input = "\x1b]8;;https://example.com\x07click here\x1b]8;;\x07";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "click here");
    }

    #[test]
    fn test_strip_ansi_two_char_escape() {
        // Two-character escapes: ESC M (reverse index), ESC D (index)
        let input = "before\x1bMmiddle\x1bDafter";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "beforemiddleafter");
    }

    #[test]
    fn test_strip_ansi_mixed_csi_osc_two_char() {
        // Mix of CSI, OSC, and two-character escapes in one string
        let input = "\x1b[1mbold\x1b[0m \x1b]0;title\x07 \x1bMtext";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "bold  text");
    }

    #[test]
    fn test_strip_ansi_lone_esc() {
        // Lone ESC at end of string (no following character)
        let input = "text\x1b";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "text");
    }

    // --- Tests for compiler-output-aware truncation ---

    #[test]
    fn test_is_diagnostic_header_rust_error() {
        assert!(is_diagnostic_header("error[E0308]: mismatched types"));
        assert!(is_diagnostic_header("error: cannot find value `x`"));
    }

    #[test]
    fn test_is_diagnostic_header_rust_warning() {
        assert!(is_diagnostic_header("warning: unused variable: `x`"));
        assert!(is_diagnostic_header(
            "warning[dead_code]: function is never used"
        ));
    }

    #[test]
    fn test_is_diagnostic_header_gcc_clang() {
        assert!(is_diagnostic_header("src/main.c:10:5: error: expected ';'"));
        assert!(is_diagnostic_header(
            "lib.cpp:42:1: warning: unused variable"
        ));
    }

    #[test]
    fn test_is_diagnostic_header_typescript() {
        assert!(is_diagnostic_header(
            "src/index.ts(5,3): error TS2322: Type 'string' is not assignable"
        ));
    }

    #[test]
    fn test_is_diagnostic_header_non_diagnostic() {
        assert!(!is_diagnostic_header("Compiling yoyo v0.1.0"));
        assert!(!is_diagnostic_header("    --> src/main.rs:5:10"));
        assert!(!is_diagnostic_header("running 3 tests"));
        assert!(!is_diagnostic_header(""));
    }

    #[test]
    fn test_is_compiler_output_with_multiple_errors() {
        let lines = vec![
            "Compiling yoyo v0.1.0",
            "error[E0308]: mismatched types",
            "  --> src/main.rs:5:10",
            "error[E0425]: cannot find value `x`",
            "  --> src/main.rs:8:5",
        ];
        assert!(is_compiler_output(&lines));
    }

    #[test]
    fn test_is_compiler_output_single_error_not_enough() {
        let lines = vec![
            "Compiling yoyo v0.1.0",
            "error[E0308]: mismatched types",
            "  --> src/main.rs:5:10",
        ];
        // Needs at least 2 diagnostic headers
        assert!(!is_compiler_output(&lines));
    }

    #[test]
    fn test_is_compiler_output_non_compiler() {
        let lines = vec![
            "running 3 tests",
            "test test_foo ... ok",
            "test test_bar ... ok",
            "test test_baz ... FAILED",
        ];
        assert!(!is_compiler_output(&lines));
    }

    #[test]
    fn test_truncate_compiler_output_prioritizes_errors() {
        // Build synthetic compiler output with errors and warnings
        let mut lines_vec: Vec<String> = Vec::new();
        lines_vec.push("Compiling example v0.1.0".to_string());
        // Add some warnings first
        for i in 0..5 {
            lines_vec.push(format!("warning: unused variable `w{i}`"));
            lines_vec.push(format!("  --> src/lib.rs:{i}:5"));
            lines_vec.push(String::new());
        }
        // Add errors
        for i in 0..3 {
            lines_vec.push(format!("error[E030{i}]: some error {i}"));
            lines_vec.push(format!("  --> src/main.rs:{}:10", i + 10));
            lines_vec.push("   |".to_string());
            lines_vec.push(format!("   | let x{i} = bad_code;"));
            lines_vec.push(String::new());
        }
        lines_vec.push("error: aborting due to 3 previous errors".to_string());

        let lines: Vec<&str> = lines_vec.iter().map(|s| s.as_str()).collect();
        let result = truncate_compiler_output(&lines, 50_000).unwrap();

        // Errors should appear before warnings
        let error_pos = result.find("error[E030").unwrap();
        let warning_pos = result.find("warning: unused").unwrap();
        assert!(
            error_pos < warning_pos,
            "Errors should be listed before warnings in output"
        );
    }

    #[test]
    fn test_truncate_compiler_output_returns_none_for_no_diagnostics() {
        let lines = vec!["Compiling example v0.1.0", "Finished dev profile"];
        assert!(truncate_compiler_output(&lines, 50_000).is_none());
    }

    #[test]
    fn test_truncate_compiler_output_includes_summary() {
        let mut lines_vec: Vec<String> = Vec::new();
        for i in 0..3 {
            lines_vec.push(format!("error[E000{i}]: problem {i}"));
            lines_vec.push(format!("  --> src/lib.rs:{i}:1"));
            lines_vec.push(String::new());
        }
        lines_vec.push("error: aborting due to 3 previous errors".to_string());
        lines_vec.push(
            "For more information about this error, try `rustc --explain E0001`.".to_string(),
        );

        let lines: Vec<&str> = lines_vec.iter().map(|s| s.as_str()).collect();
        let result = truncate_compiler_output(&lines, 50_000).unwrap();

        // Should contain the summary section separator
        assert!(result.contains("---"));
        assert!(result.contains("aborting due to 3 previous errors"));
    }

    #[test]
    fn test_truncate_compiler_output_header_shows_counts() {
        let mut lines_vec: Vec<String> = Vec::new();
        for i in 0..2 {
            lines_vec.push(format!("error[E000{i}]: err {i}"));
            lines_vec.push(String::new());
        }
        for i in 0..3 {
            lines_vec.push(format!("warning: warn {i}"));
            lines_vec.push(String::new());
        }

        let lines: Vec<&str> = lines_vec.iter().map(|s| s.as_str()).collect();
        let result = truncate_compiler_output(&lines, 50_000).unwrap();

        // Header should mention error and warning counts
        assert!(result.contains("2 errors"));
        assert!(result.contains("3 warnings"));
    }

    #[test]
    fn test_truncate_tool_output_uses_compiler_aware_for_rust() {
        // Build output that exceeds threshold even after compression.
        // Use unique "Compiling" lines (different crate names) so compression
        // doesn't collapse them, plus many error blocks.
        let mut lines_vec: Vec<String> = Vec::new();
        for i in 0..100 {
            lines_vec.push(format!(
                "   Compiling unique-crate-{i} v{}.{}.0 (/path/to/dep-{i})",
                i / 10,
                i % 10
            ));
        }
        // Add enough error blocks to produce substantial output
        for i in 0..30 {
            lines_vec.push(format!(
                "error[E0{:03}]: type mismatch in function number {i} with a long description",
                300 + i
            ));
            lines_vec.push(format!("  --> src/module_{i}/main.rs:{}:5", i * 10 + 1));
            lines_vec.push("   |".to_string());
            lines_vec.push(format!(
                "{} | fn bad_function_{i}(arg: SomeVeryLongTypeName) -> AnotherLongType {{",
                i * 10 + 1
            ));
            lines_vec.push("   |    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^".to_string());
            lines_vec.push("   |".to_string());
            lines_vec.push(format!("   = note: expected `TypeA` found `TypeB_{i}`"));
            lines_vec.push(String::new());
        }
        lines_vec.push("error: aborting due to 30 previous errors".to_string());

        let output = lines_vec.join("\n");
        // Use a max_chars small enough to require truncation
        let result = truncate_tool_output(&output, 3000);

        // Should use compiler-aware truncation (shows errors, not just head/tail)
        assert!(
            result.contains("error[E0"),
            "Should contain actual error diagnostics, got: {}",
            &result[..result.len().min(500)]
        );
        assert!(
            result.contains("[compiler output:"),
            "Should contain compiler output header, got: {}",
            &result[..result.len().min(500)]
        );
    }
}
