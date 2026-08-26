//! Diff rendering: LCS-based line diff and colored unified diff output.

use super::{DIM, GREEN, RED, RESET};

/// Maximum number of diff lines to display before truncating.
const MAX_DIFF_LINES: usize = 20;

/// Number of context lines to show around each change hunk.
const DIFF_CONTEXT_LINES: usize = 3;

/// Maximum bytes of a *single* diff line to render before truncating it.
///
/// This is a **judgment threshold, not a measurement** — nothing measured says
/// 500 is right; it is the point past which a diff line stops being readable
/// and starts being a terminal hazard. `MAX_DIFF_LINES` bounds how many lines
/// render; this bounds how wide one of them may be, which is a different
/// property: a diff touching a base64 blob, a minified JS bundle, a one-line
/// JSON or a lockfile hash is *one* line and so passes the line-count bound
/// untouched, then renders whole through ANSI colouring into the terminal.
///
/// Measured in bytes rather than display columns, matching `CappedCapture` in
/// `commands_run.rs`. For the shapes this exists for — base64, hashes, minified
/// source — those are the same number.
const MAX_DIFF_LINE_WIDTH: usize = 500;

/// Bound one diff line's width, marking the cut in band.
///
/// `None` means the line is at or under budget and the caller must emit it
/// **verbatim** — that is every line of every hand-sized diff, i.e. the whole
/// regression surface. `Some(_)` is the kept prefix plus a marker naming what
/// was dropped, in the vocabulary `CappedCapture` and `cap_ast_output` already
/// use; a silent elision is the bug this closes.
///
/// Never indexes by a raw byte offset — the kept prefix walks back to a
/// `is_char_boundary` first, so a multi-byte character straddling the cut is
/// dropped whole rather than split (CLAUDE.md rule #250; a base64 blob is
/// exactly where a multi-byte neighbour sits).
fn truncate_diff_line(line: &str, max: usize) -> Option<String> {
    if line.len() <= max {
        return None;
    }
    let mut b = max;
    while b > 0 && !line.is_char_boundary(b) {
        b -= 1;
    }
    let dropped = line.len() - b;
    Some(format!(
        "{}… [yoyo: {dropped} more bytes elided — diff lines are capped at {max} bytes]",
        &line[..b]
    ))
}

/// Operations produced by the LCS diff algorithm.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffOp<'a> {
    Keep(&'a str),
    Delete(&'a str),
    Insert(&'a str),
}

/// Compute a line-level diff between two texts using LCS (Longest Common Subsequence).
///
/// Returns a sequence of `DiffOp`s representing keeps, deletions, and insertions.
fn compute_line_diff<'a>(old_lines: &[&'a str], new_lines: &[&'a str]) -> Vec<DiffOp<'a>> {
    let m = old_lines.len();
    let n = new_lines.len();

    // Build LCS table
    // dp[i][j] = length of LCS of old_lines[..i] and new_lines[..j]
    let mut dp = vec![vec![0u32; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if old_lines[i - 1] == new_lines[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack to produce diff ops
    let mut ops = Vec::new();
    let mut i = m;
    let mut j = n;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old_lines[i - 1] == new_lines[j - 1] {
            ops.push(DiffOp::Keep(old_lines[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            ops.push(DiffOp::Insert(new_lines[j - 1]));
            j -= 1;
        } else {
            ops.push(DiffOp::Delete(old_lines[i - 1]));
            i -= 1;
        }
    }

    ops.reverse();
    ops
}

/// Format a colored unified diff between old_text and new_text.
///
/// Uses LCS-based line diffing to produce proper unified-style output with context lines.
/// Context lines (unchanged) are shown dimmed, removed lines in red with `- ` prefix,
/// added lines in green with `+ ` prefix. Non-adjacent hunks are separated by `···`.
/// If the diff exceeds `MAX_DIFF_LINES`, it is truncated with an ellipsis note.
pub fn format_edit_diff(old_text: &str, new_text: &str) -> String {
    // Handle both-empty case
    if old_text.is_empty() && new_text.is_empty() {
        return String::new();
    }

    let old_lines: Vec<&str> = if old_text.is_empty() {
        Vec::new()
    } else {
        old_text.lines().collect()
    };
    let new_lines: Vec<&str> = if new_text.is_empty() {
        Vec::new()
    } else {
        new_text.lines().collect()
    };

    let ops = compute_line_diff(&old_lines, &new_lines);

    // If everything is Keep, texts are identical
    if ops.iter().all(|op| matches!(op, DiffOp::Keep(_))) {
        return String::new();
    }

    // Assign indices and mark which ops are changes (Delete or Insert)
    let is_change: Vec<bool> = ops
        .iter()
        .map(|op| !matches!(op, DiffOp::Keep(_)))
        .collect();

    // For each op, determine if it should be shown (is a change, or within
    // DIFF_CONTEXT_LINES of a change)
    let len = ops.len();
    let mut visible = vec![false; len];
    for (idx, &changed) in is_change.iter().enumerate() {
        if changed {
            // Mark the change itself and surrounding context
            let start = idx.saturating_sub(DIFF_CONTEXT_LINES);
            let end = (idx + DIFF_CONTEXT_LINES + 1).min(len);
            for v in &mut visible[start..end] {
                *v = true;
            }
        }
    }

    // Build output lines, inserting hunk separators where there are gaps
    let mut output: Vec<String> = Vec::new();
    let mut last_visible: Option<usize> = None;

    for (idx, op) in ops.iter().enumerate() {
        if !visible[idx] {
            continue;
        }

        // Insert hunk separator if there's a gap
        if let Some(prev) = last_visible {
            if idx > prev + 1 {
                output.push(format!("{DIM}  ···{RESET}"));
            }
        }
        last_visible = Some(idx);

        match op {
            DiffOp::Keep(line) | DiffOp::Delete(line) | DiffOp::Insert(line) => {
                // Bound the RAW text, then colour it. The ordering is
                // load-bearing, not stylistic: truncating a string that already
                // carries ANSI escapes can cut *inside* an escape sequence and
                // leave the user's terminal stuck in a colour, and it would drop
                // the trailing RESET entirely. Bounding the content first keeps
                // every escape this function emits balanced.
                let bounded = truncate_diff_line(line, MAX_DIFF_LINE_WIDTH);
                let line = bounded.as_deref().unwrap_or(line);
                match op {
                    DiffOp::Keep(_) => output.push(format!("{DIM}    {line}{RESET}")),
                    DiffOp::Delete(_) => output.push(format!("{RED}  - {line}{RESET}")),
                    DiffOp::Insert(_) => output.push(format!("{GREEN}  + {line}{RESET}")),
                }
            }
        }
    }

    if output.is_empty() {
        return String::new();
    }

    // Truncate if too many lines
    if output.len() > MAX_DIFF_LINES {
        let remaining = output.len() - MAX_DIFF_LINES;
        output.truncate(MAX_DIFF_LINES);
        output.push(format!("{DIM}  ... ({remaining} more lines){RESET}"));
    }

    output.join("\n")
}

/// Truncate a pre-formatted diff preview to at most `max_lines` lines.
///
/// If the diff has more lines, the output is trimmed to `max_lines - 2`
/// lines followed by a dimmed summary showing how many lines were omitted.
/// Empty input is returned unchanged.
pub fn truncate_diff_preview(diff: &str, max_lines: usize) -> String {
    if diff.is_empty() {
        return String::new();
    }

    let lines: Vec<&str> = diff.lines().collect();
    if lines.len() <= max_lines {
        return diff.to_string();
    }

    // Keep first (max_lines - 2) lines, then add a summary line
    let keep = max_lines.saturating_sub(2);
    let remaining = lines.len() - keep;
    let mut result = lines[..keep].join("\n");
    result.push('\n');
    result.push_str(&format!("{DIM}  ... ({remaining} more lines){RESET}"));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_edit_diff_single_line_change() {
        let diff = format_edit_diff("old line", "new line");
        assert!(diff.contains("- old line"));
        assert!(diff.contains("+ new line"));
        // Should have red for removed, green for added
        assert!(diff.contains(&format!("{RED}")));
        assert!(diff.contains(&format!("{GREEN}")));
    }

    #[test]
    fn test_format_edit_diff_multi_line_change() {
        let old = "line 1\nline 2\nline 3";
        let new = "line A\nline B";
        let diff = format_edit_diff(old, new);
        assert!(diff.contains("- line 1"));
        assert!(diff.contains("- line 2"));
        assert!(diff.contains("- line 3"));
        assert!(diff.contains("+ line A"));
        assert!(diff.contains("+ line B"));
    }

    #[test]
    fn test_format_edit_diff_addition_only() {
        let diff = format_edit_diff("", "new content\nmore content");
        // No removed lines
        assert!(!diff.contains("- "));
        // Added lines present
        assert!(diff.contains("+ new content"));
        assert!(diff.contains("+ more content"));
    }

    #[test]
    fn test_format_edit_diff_deletion_only() {
        let diff = format_edit_diff("old content\nmore old", "");
        // Removed lines present
        assert!(diff.contains("- old content"));
        assert!(diff.contains("- more old"));
        // No added lines
        assert!(!diff.contains("+ "));
    }

    #[test]
    fn test_format_edit_diff_long_diff_truncation() {
        // Generate a diff with more than MAX_DIFF_LINES lines
        let old_lines: Vec<&str> = (0..15).map(|_| "old").collect();
        let new_lines: Vec<&str> = (0..15).map(|_| "new").collect();
        let old = old_lines.join("\n");
        let new = new_lines.join("\n");
        let diff = format_edit_diff(&old, &new);
        // Should be truncated — total would be 30 lines, max is 20
        assert!(diff.contains("more lines)"));
    }

    #[test]
    fn test_format_edit_diff_empty_both() {
        let diff = format_edit_diff("", "");
        assert!(diff.is_empty());
    }

    #[test]
    fn test_format_edit_diff_empty_old_text_new_file_section() {
        // Simulates adding new content to a file (old_text is empty)
        let diff = format_edit_diff("", "fn new_function() {\n    println!(\"hello\");\n}");
        assert!(!diff.contains("- "));
        assert!(diff.contains("+ fn new_function()"));
        assert!(diff.contains("+ }"));
    }

    #[test]
    fn test_format_edit_diff_short_diff_not_truncated() {
        let diff = format_edit_diff("a", "b");
        assert!(!diff.contains("more lines"));
    }

    #[test]
    fn test_format_edit_diff_context_lines_around_change() {
        // Change one line in the middle of a block — context lines should appear
        let old = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9";
        let new = "line 1\nline 2\nline 3\nline 4\nLINE FIVE\nline 6\nline 7\nline 8\nline 9";
        let diff = format_edit_diff(old, new);
        // The changed lines should be present
        assert!(diff.contains("- line 5"));
        assert!(diff.contains("+ LINE FIVE"));
        // Context lines around the change should be present (dimmed)
        assert!(diff.contains("line 3") || diff.contains("line 4"));
        assert!(diff.contains("line 6") || diff.contains("line 7"));
        // Lines far from the change should NOT appear
        assert!(!diff.contains("line 1"));
        assert!(!diff.contains("line 9"));
    }

    #[test]
    fn test_format_edit_diff_adjacent_changes_grouped() {
        // Two consecutive changed lines should appear in one hunk without separator
        let old = "keep 1\nold A\nold B\nkeep 2";
        let new = "keep 1\nnew A\nnew B\nkeep 2";
        let diff = format_edit_diff(old, new);
        assert!(diff.contains("- old A"));
        assert!(diff.contains("- old B"));
        assert!(diff.contains("+ new A"));
        assert!(diff.contains("+ new B"));
        // No hunk separator between adjacent changes
        assert!(!diff.contains("···"));
    }

    #[test]
    fn test_format_edit_diff_nonadjacent_changes_get_separator() {
        // Two changes separated by many unchanged lines should get a hunk separator
        let old = "line 1\nold A\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nold B\nline 12";
        let new = "line 1\nnew A\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nnew B\nline 12";
        let diff = format_edit_diff(old, new);
        assert!(diff.contains("- old A"));
        assert!(diff.contains("+ new A"));
        assert!(diff.contains("- old B"));
        assert!(diff.contains("+ new B"));
        // Should have a hunk separator between the two distant changes
        assert!(diff.contains("···"));
    }

    #[test]
    fn test_format_edit_diff_single_line_change_with_context() {
        // A single line changed, surrounded by context
        let old = "before\ntarget\nafter";
        let new = "before\nreplacement\nafter";
        let diff = format_edit_diff(old, new);
        assert!(diff.contains("- target"));
        assert!(diff.contains("+ replacement"));
        // Context should include surrounding lines
        assert!(diff.contains("before"));
        assert!(diff.contains("after"));
    }

    #[test]
    fn test_format_edit_diff_identical_texts() {
        let diff = format_edit_diff("same\ncontent\nhere", "same\ncontent\nhere");
        assert!(diff.is_empty());
    }

    // --- per-line width bound -------------------------------------------------
    //
    // The decision half is asserted directly, and every consequence is asserted
    // at the emission point — the string a caller of `format_edit_diff`
    // receives, never the helper one layer below it.

    #[test]
    fn test_truncate_diff_line_under_budget_returns_none() {
        // `None` means "emit the line verbatim". This is essentially every line
        // of every hand-sized diff, and so the whole regression surface.
        assert_eq!(truncate_diff_line("short line", MAX_DIFF_LINE_WIDTH), None);
        assert_eq!(truncate_diff_line("", MAX_DIFF_LINE_WIDTH), None);
    }

    #[test]
    fn test_truncate_diff_line_boundary_in_both_directions() {
        // Exactly at the cap is NOT truncated; one byte past is. A discriminator
        // tested only on the side that fires is vacuous green.
        let at_cap = "a".repeat(MAX_DIFF_LINE_WIDTH);
        assert_eq!(truncate_diff_line(&at_cap, MAX_DIFF_LINE_WIDTH), None);

        let one_past = "a".repeat(MAX_DIFF_LINE_WIDTH + 1);
        let cut = truncate_diff_line(&one_past, MAX_DIFF_LINE_WIDTH)
            .expect("one byte past the cap must truncate");
        assert!(cut.starts_with(&at_cap), "kept prefix must be the budget");
        assert!(
            cut.contains("1 more bytes elided"),
            "marker names the cut: {cut}"
        );
    }

    #[test]
    fn test_truncate_diff_line_never_splits_a_multibyte_char() {
        // 499 ASCII bytes then a 3-byte char: the cut at 500 lands *inside* the
        // char, so it must walk back to the boundary and drop the char whole.
        // A raw byte index here would panic (CLAUDE.md rule #250).
        let line = format!("{}✓", "a".repeat(MAX_DIFF_LINE_WIDTH - 1));
        let cut = truncate_diff_line(&line, MAX_DIFF_LINE_WIDTH)
            .expect("502 bytes must truncate at a 500-byte cap");
        assert!(
            cut.starts_with(&"a".repeat(MAX_DIFF_LINE_WIDTH - 1)),
            "prefix preserved"
        );
        assert!(
            !cut.contains('✓'),
            "the straddling char is dropped, not split"
        );
        assert!(
            cut.contains("3 more bytes elided"),
            "all three bytes of the dropped char are counted: {cut}"
        );
    }

    #[test]
    fn test_format_edit_diff_bounds_one_enormous_line() {
        // The Claude-Code-v2.1.246 case: a base64-shaped blob on one line.
        let blob = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo".repeat(200);
        assert!(blob.len() > 5000, "fixture must exceed the cap by a lot");
        let diff = format_edit_diff("small", &blob);

        assert!(
            diff.contains("more bytes elided"),
            "the cut is marked in band"
        );
        assert!(
            !diff.contains(&blob),
            "the whole blob must not reach the terminal"
        );
        // Bounded by the cap plus prefix/marker/escapes — not by the input size.
        assert!(
            diff.len() < MAX_DIFF_LINE_WIDTH * 2,
            "rendered diff stayed bounded, got {} bytes",
            diff.len()
        );
    }

    #[test]
    fn test_format_edit_diff_truncated_line_still_resets_color() {
        // Truncation happens on the RAW text, before colouring — so the closing
        // RESET is still emitted and the terminal is not left stuck in a colour.
        let diff = format_edit_diff("small", &"z".repeat(5000));
        for line in diff.lines() {
            assert!(
                line.ends_with(RESET.0),
                "every rendered line ends with a reset: {line:?}"
            );
        }
        assert!(diff.contains("more bytes elided"));
    }

    #[test]
    fn test_format_edit_diff_ordinary_widths_are_byte_identical() {
        // NEAR-MISS GUARD. The exact string an ordinary diff renders, asserted
        // whole rather than by `contains` — this is what proves the width bound
        // did not change any normal diff.
        let diff = format_edit_diff("old line", "new line");
        assert_eq!(
            diff,
            format!("{RED}  - old line{RESET}\n{GREEN}  + new line{RESET}")
        );
        assert!(!diff.contains("elided"));
    }

    #[test]
    fn test_format_edit_diff_context_lines_are_also_bounded() {
        // A long *unchanged* line rendered as context is on the same display
        // path as a +/- line, so it takes the same bound.
        let long = "c".repeat(5000);
        let old = format!("{long}\ntarget");
        let new = format!("{long}\nreplacement");
        let diff = format_edit_diff(&old, &new);
        assert!(diff.contains("- target"));
        assert!(
            !diff.contains(&long),
            "the long context line must not render whole"
        );
        assert!(diff.contains("more bytes elided"));
    }

    #[test]
    fn test_truncate_diff_preview_empty() {
        assert!(truncate_diff_preview("", 30).is_empty());
    }

    #[test]
    fn test_truncate_diff_preview_short_passes_through() {
        let diff = "line 1\nline 2\nline 3";
        let result = truncate_diff_preview(diff, 30);
        assert_eq!(result, diff);
    }

    #[test]
    fn test_truncate_diff_preview_exact_limit() {
        let lines: Vec<&str> = (0..5).map(|_| "line").collect();
        let diff = lines.join("\n");
        let result = truncate_diff_preview(&diff, 5);
        assert_eq!(result, diff);
    }

    #[test]
    fn test_truncate_diff_preview_truncates_long_diff() {
        let lines: Vec<String> = (0..20).map(|i| format!("line {i}")).collect();
        let diff = lines.join("\n");
        let result = truncate_diff_preview(&diff, 10);
        // Should have 8 kept lines + 1 summary line = 9 lines
        let result_lines: Vec<&str> = result.lines().collect();
        assert_eq!(result_lines.len(), 9);
        // First line preserved
        assert!(result_lines[0].contains("line 0"));
        // Last line is the summary
        assert!(result_lines[8].contains("12 more lines"));
    }
}
