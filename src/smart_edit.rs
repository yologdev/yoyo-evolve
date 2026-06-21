//! SmartEditTool — augments edit_file "not found" errors with line-number context
//!
//! Extracted from `tool_wrappers.rs`. This wrapper intercepts edit_file failures,
//! searches for the nearest fuzzy match, and either auto-fixes whitespace-only
//! mismatches or augments the error with location hints.

use yoagent::types::AgentTool;

/// Maximum file size (bytes) we'll read for nearest-match searching.
const SMART_EDIT_MAX_FILE_SIZE: u64 = 100_000;

/// Number of context lines to show around the nearest match.
const SMART_EDIT_CONTEXT_LINES: usize = 5;

/// Minimum per-line similarity (0.0–1.0) required for a fuzzy match to be
/// reported. Below this threshold the match is considered noise.
const FUZZY_MIN_SIMILARITY: f64 = 0.6;

/// Edit distance between two strings (Levenshtein). Used for fuzzy matching
/// when exact trimmed matches fail.
///
/// Uses only O(min(n,m)) memory instead of the naive O(n*m) 2-D table —
/// we only keep two rows (previous and current) and swap them each iteration.
/// For large edit blocks this avoids multi-megabyte allocations.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    // Ensure `b` is the shorter side so the rows are as small as possible.
    let (a, b) = if a.len() < b.len() { (b, a) } else { (a, b) };

    let b_len = b.len();
    let mut prev = (0..=b_len).collect::<Vec<usize>>();
    let mut curr = vec![0usize; b_len + 1];

    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b_len {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_len]
}

/// Compute similarity between two strings as a ratio in 0.0..=1.0.
/// Returns 1.0 for identical strings, 0.0 for completely different ones.
fn line_similarity(a: &str, b: &str) -> f64 {
    let a_trimmed = a.trim();
    let b_trimmed = b.trim();
    if a_trimmed.is_empty() && b_trimmed.is_empty() {
        return 1.0;
    }
    // Use char count, not byte length — edit_distance operates on chars,
    // so the denominator must be in the same units to avoid inflated
    // similarity scores on multibyte UTF-8 content (CJK, emoji, etc.).
    let max_len = a_trimmed.chars().count().max(b_trimmed.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let dist = edit_distance(a_trimmed, b_trimmed);
    1.0 - (dist as f64 / max_len as f64)
}

/// A wrapper tool specifically for `edit_file` that intercepts "not found"
/// failures and augments the error message with:
/// - The line number of the nearest match (first-line matching with whitespace normalization)
/// - A snippet of actual content at that location
/// - A hint when the mismatch is purely whitespace/indentation
pub(crate) struct SmartEditTool {
    inner: Box<dyn AgentTool>,
}

/// Result of a nearest-match search.
struct NearestMatch {
    /// 1-indexed line number in the file.
    line_num: usize,
    /// Whether the mismatch is whitespace-only (auto-fixable).
    is_whitespace_only: bool,
    /// Whether this was a fuzzy (non-exact) match.
    is_fuzzy: bool,
    /// Whether multiple equally-good matches exist (ambiguous — unsafe to auto-fix).
    is_ambiguous: bool,
    /// Average similarity score (1.0 = exact match, 0.0 = completely different).
    similarity: f64,
    /// Snippet of the file content around the match.
    snippet: String,
}

/// Build a snippet of context lines starting at `match_line_idx` (0-indexed).
fn build_snippet(file_lines: &[&str], match_line_idx: usize) -> String {
    let snippet_start = match_line_idx;
    let snippet_end = (match_line_idx + SMART_EDIT_CONTEXT_LINES).min(file_lines.len());
    file_lines[snippet_start..snippet_end]
        .iter()
        .enumerate()
        .map(|(j, line)| format!("{:>4} │ {}", snippet_start + j + 1, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check if the match at `match_line_idx` (0-indexed) is a whitespace-only diff.
fn check_whitespace_only(file_lines: &[&str], old_lines: &[&str], match_line_idx: usize) -> bool {
    let mut all_match_trimmed = true;
    let mut any_exact_mismatch = false;
    for (j, old_line) in old_lines.iter().enumerate() {
        let file_idx = match_line_idx + j;
        if file_idx < file_lines.len() {
            if file_lines[file_idx].trim() == old_line.trim() {
                if file_lines[file_idx] != *old_line {
                    any_exact_mismatch = true;
                }
            } else {
                all_match_trimmed = false;
                break;
            }
        } else {
            all_match_trimmed = false;
            break;
        }
    }
    all_match_trimmed && any_exact_mismatch
}

/// Try to find an exact trimmed-line match for `old_text` in `file_lines`.
/// Returns `(start_line_0indexed, match_count, is_ambiguous)` for the best match.
/// `is_ambiguous` is true when multiple positions tie for the best match count,
/// meaning auto-fix cannot safely choose which block to edit.
fn find_exact_trimmed_match(
    file_lines: &[&str],
    old_lines: &[&str],
) -> Option<(usize, usize, bool)> {
    let anchor = old_lines.iter().find(|l| !l.trim().is_empty())?;
    let anchor_trimmed = anchor.trim();

    let mut best_match: Option<(usize, usize)> = None;
    let mut best_count_hits: usize = 0; // how many positions share the best match_count

    for (i, line) in file_lines.iter().enumerate() {
        if line.trim() == anchor_trimmed {
            let mut match_count = 1;
            let anchor_offset = old_lines
                .iter()
                .position(|l| !l.trim().is_empty())
                .unwrap_or(0);

            for j in 1..(old_lines.len() - anchor_offset) {
                let old_idx = anchor_offset + j;
                let file_idx = i + j;
                if file_idx < file_lines.len()
                    && old_idx < old_lines.len()
                    && file_lines[file_idx].trim() == old_lines[old_idx].trim()
                {
                    match_count += 1;
                } else {
                    break;
                }
            }

            match best_match {
                Some((_, prev_count)) if match_count > prev_count => {
                    // Strictly better — reset
                    let start_line = if anchor_offset > 0 && i >= anchor_offset {
                        i - anchor_offset
                    } else {
                        i
                    };
                    best_match = Some((start_line, match_count));
                    best_count_hits = 1;
                }
                Some((_, prev_count)) if match_count == prev_count => {
                    // Tied — mark ambiguous
                    best_count_hits += 1;
                }
                None => {
                    let start_line = if anchor_offset > 0 && i >= anchor_offset {
                        i - anchor_offset
                    } else {
                        i
                    };
                    best_match = Some((start_line, match_count));
                    best_count_hits = 1;
                }
                _ => {} // match_count < prev_count — ignore
            }
        }
    }

    best_match.map(|(line, count)| (line, count, best_count_hits > 1))
}

/// Compute the average similarity of `old_lines` against `file_lines` starting
/// at position `start` (0-indexed). Only considers non-empty old lines.
fn block_similarity(file_lines: &[&str], old_lines: &[&str], start: usize) -> f64 {
    let mut total_sim = 0.0;
    let mut count = 0;
    for (j, old_line) in old_lines.iter().enumerate() {
        // Skip empty lines — they don't carry signal for matching
        if old_line.trim().is_empty() {
            continue;
        }
        let file_idx = start + j;
        if file_idx < file_lines.len() {
            total_sim += line_similarity(old_line, file_lines[file_idx]);
            count += 1;
        } else {
            // File ran out of lines — penalize
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    total_sim / count as f64
}

/// Try to find a fuzzy match for `old_text` in `file_lines` when exact
/// trimmed matching found nothing. Scores every candidate position by
/// average per-line similarity and returns the best one above the threshold.
fn find_fuzzy_match(file_lines: &[&str], old_lines: &[&str]) -> Option<(usize, f64)> {
    let non_empty_old: Vec<&str> = old_lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .copied()
        .collect();
    if non_empty_old.is_empty() {
        return None;
    }

    // Use the first non-empty old line as a pre-filter: only consider positions
    // where the anchor has at least moderate similarity to avoid O(n*m) distance
    // computation on every file line.
    let anchor = non_empty_old[0];

    let mut best: Option<(usize, f64)> = None;

    for i in 0..file_lines.len() {
        // Quick pre-filter: anchor similarity must pass threshold
        let anchor_sim = line_similarity(anchor, file_lines[i]);
        if anchor_sim < FUZZY_MIN_SIMILARITY {
            continue;
        }

        // Compute block-level similarity at this position
        let sim = block_similarity(file_lines, old_lines, i);
        if sim < FUZZY_MIN_SIMILARITY {
            continue;
        }

        if best.is_none_or(|(_, prev_sim)| sim > prev_sim) {
            best = Some((i, sim));
        }
    }

    best
}

/// Search a file's content for the best match of `old_text`, returning
/// match details including location, similarity, and a context snippet.
///
/// Strategy:
/// 1. Try exact trimmed-line matching (fast, handles whitespace-only diffs)
/// 2. Fall back to fuzzy matching with edit-distance similarity scoring
fn find_nearest_match(file_content: &str, old_text: &str) -> Option<NearestMatch> {
    let old_lines: Vec<&str> = old_text.lines().collect();
    if old_lines.is_empty() {
        return None;
    }

    let file_lines: Vec<&str> = file_content.lines().collect();

    // Phase 1: exact trimmed match
    if let Some((match_line_idx, _match_count, is_ambiguous)) =
        find_exact_trimmed_match(&file_lines, &old_lines)
    {
        let is_ws_only = check_whitespace_only(&file_lines, &old_lines, match_line_idx);
        let snippet = build_snippet(&file_lines, match_line_idx);
        return Some(NearestMatch {
            line_num: match_line_idx + 1,
            is_whitespace_only: is_ws_only,
            is_fuzzy: false,
            is_ambiguous,
            similarity: 1.0,
            snippet,
        });
    }

    // Phase 2: fuzzy match
    if let Some((match_line_idx, similarity)) = find_fuzzy_match(&file_lines, &old_lines) {
        let snippet = build_snippet(&file_lines, match_line_idx);
        return Some(NearestMatch {
            line_num: match_line_idx + 1,
            is_whitespace_only: false,
            is_fuzzy: true,
            is_ambiguous: false,
            similarity,
            snippet,
        });
    }

    None
}

/// Extract exact text from file content starting at `match_line_0indexed` for `line_count` lines.
/// Returns the joined text (with newlines between lines). If the file doesn't have enough lines,
/// returns as many as are available.
fn extract_matched_text(
    file_content: &str,
    match_line_0indexed: usize,
    line_count: usize,
) -> String {
    let file_lines: Vec<&str> = file_content.lines().collect();
    let end = (match_line_0indexed + line_count).min(file_lines.len());
    if match_line_0indexed >= file_lines.len() {
        return String::new();
    }
    file_lines[match_line_0indexed..end].join("\n")
}

/// Wrap an edit_file tool with smart error augmentation.
pub(crate) fn with_smart_edit(tool: Box<dyn AgentTool>) -> Box<dyn AgentTool> {
    Box::new(SmartEditTool { inner: tool })
}

#[async_trait::async_trait]
impl AgentTool for SmartEditTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        match self.inner.execute(params.clone(), ctx.clone()).await {
            Ok(result) => Ok(result),
            Err(yoagent::types::ToolError::Failed(msg)) if msg.contains("not found") => {
                // Try whitespace auto-fix before falling back to augmented error
                if let Some(retry_result) = self.try_whitespace_autofix(&msg, &params, &ctx).await {
                    return retry_result;
                }
                // No auto-fix possible — augment the error with location info
                let augmented = self.augment_not_found_error(&msg, &params);
                Err(yoagent::types::ToolError::Failed(augmented))
            }
            Err(other) => Err(other),
        }
    }
}

impl SmartEditTool {
    /// Attempt to auto-fix a whitespace-only mismatch by extracting the actual text
    /// from the file and retrying with corrected `old_text`.
    ///
    /// Returns `Some(Ok(..))` if the retry succeeded, `Some(Err(..))` if the retry
    /// also failed (falls through to normal augmented error), or `None` if the mismatch
    /// is not whitespace-only (so caller should use the normal augmentation path).
    async fn try_whitespace_autofix(
        &self,
        _original_msg: &str,
        params: &serde_json::Value,
        ctx: &yoagent::types::ToolContext,
    ) -> Option<Result<yoagent::types::ToolResult, yoagent::types::ToolError>> {
        let path = params.get("path").and_then(|v| v.as_str())?;
        let old_text = params.get("old_text").and_then(|v| v.as_str())?;

        // Check file size
        let metadata = std::fs::metadata(path).ok()?;
        if metadata.len() > SMART_EDIT_MAX_FILE_SIZE {
            return None;
        }

        let content = std::fs::read_to_string(path).ok()?;

        // find_nearest_match returns structured match info
        let m = find_nearest_match(&content, old_text)?;

        if !m.is_whitespace_only {
            return None; // Not a whitespace-only diff — let caller handle it
        }

        // When multiple equally-good matches exist, auto-fix could silently edit
        // the wrong block. Refuse and let the augmented error show the location
        // so the agent can read the file and choose the right one.
        if m.is_ambiguous {
            return None;
        }

        // Extract the actual text from the file at the match position
        let old_line_count = old_text.lines().count().max(1);
        let match_line_0indexed = m.line_num - 1;
        let actual_text = extract_matched_text(&content, match_line_0indexed, old_line_count);

        // Build corrected params with the file's actual whitespace
        let mut corrected_params = params.clone();
        corrected_params["old_text"] = serde_json::Value::String(actual_text);

        // Retry with corrected old_text
        match self.inner.execute(corrected_params, ctx.clone()).await {
            Ok(mut result) => {
                // Append auto-fix note to the result
                let note = format!("\n⚡ Auto-fixed whitespace mismatch at line {}", m.line_num);
                result.content.push(yoagent::Content::Text { text: note });
                Some(Ok(result))
            }
            Err(_) => {
                // Retry also failed — return None to fall through to augmented error
                None
            }
        }
    }

    fn augment_not_found_error(&self, original_msg: &str, params: &serde_json::Value) -> String {
        // Extract path and old_text from params
        let path = match params.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return original_msg.to_string(),
        };
        let old_text = match params.get("old_text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return original_msg.to_string(),
        };

        // Check file size — skip for huge files
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return original_msg.to_string(),
        };
        if metadata.len() > SMART_EDIT_MAX_FILE_SIZE {
            return original_msg.to_string();
        }

        // Read the file
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return original_msg.to_string(),
        };

        // Search for nearest match
        match find_nearest_match(&content, old_text) {
            Some(m) => {
                let mut augmented = original_msg.to_string();
                if m.is_fuzzy {
                    let pct = (m.similarity * 100.0) as u32;
                    augmented.push_str(&format!(
                        "\n\n📍 Nearest fuzzy match at line {} ({pct}% similar):\n```\n{}\n```",
                        m.line_num, m.snippet
                    ));
                    augmented.push_str(
                        "\n\n⚠ The content differs (not just whitespace). \
                         Read the file at that line to see the actual text.",
                    );
                } else {
                    augmented.push_str(&format!(
                        "\n\n📍 Nearest match at line {}:\n```\n{}\n```",
                        m.line_num, m.snippet
                    ));
                    if m.is_ambiguous {
                        augmented.push_str(
                            "\n\n⚠️ Multiple identical blocks found — auto-fix skipped to avoid \
                             editing the wrong one. Use read_file to verify which block you want, \
                             then include more surrounding context in old_text to disambiguate.",
                        );
                    } else if m.is_whitespace_only {
                        augmented.push_str(
                            "\n\n⚠️ Hint: the text exists but indentation/whitespace differs. \
                             Use read_file to see the exact whitespace.",
                        );
                    }
                }
                augmented
            }
            None => original_msg.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tool_context() -> yoagent::types::ToolContext {
        yoagent::types::ToolContext {
            tool_call_id: "test".to_string(),
            tool_name: "test".to_string(),
            cancel: tokio_util::sync::CancellationToken::new(),
            on_update: None,
            on_progress: None,
        }
    }

    #[test]
    fn test_find_nearest_match_exact_line() {
        let file_content = "line one\nline two\nfn hello() {\n    world()\n}\nline six\n";
        let old_text = "fn hello() {\n    world()\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some(), "Should find a match");
        let m = result.unwrap();
        let (line, is_ws, snippet) = (m.line_num, m.is_whitespace_only, m.snippet);
        assert_eq!(line, 3, "Match should be at line 3");
        assert!(!is_ws, "Should not be whitespace-only diff");
        assert!(
            snippet.contains("fn hello()"),
            "Snippet should contain the match"
        );
    }

    #[test]
    fn test_find_nearest_match_whitespace_only_diff() {
        // File has 4-space indent, old_text has 2-space indent
        let file_content = "fn main() {\n    let x = 1;\n    let y = 2;\n}\n";
        let old_text = "fn main() {\n  let x = 1;\n  let y = 2;\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some(), "Should find a match");
        let m = result.unwrap();
        let (line, is_ws) = (m.line_num, m.is_whitespace_only);
        assert_eq!(line, 1, "Match should be at line 1");
        assert!(is_ws, "Should detect whitespace-only diff");
    }

    #[test]
    fn test_find_nearest_match_no_match() {
        let file_content = "fn main() {\n    println!(\"hello\");\n}\n";
        let old_text = "fn totally_different() {\n    nothing();\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_none(), "Should not find a match");
    }

    #[test]
    fn test_find_nearest_match_empty_old_text() {
        let file_content = "fn main() {}\n";
        let result = find_nearest_match(file_content, "");
        assert!(result.is_none(), "Empty old_text should return None");
    }

    #[test]
    fn test_find_nearest_match_only_whitespace_lines() {
        let file_content = "fn main() {}\n";
        let result = find_nearest_match(file_content, "   \n   \n");
        assert!(
            result.is_none(),
            "All-whitespace old_text should return None"
        );
    }

    #[test]
    fn test_find_nearest_match_snippet_limited_to_5_lines() {
        let file_content = (1..=20)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let old_text = "line 5";
        let result = find_nearest_match(&file_content, old_text);
        assert!(result.is_some());
        let m = result.unwrap();
        let (line, snippet) = (m.line_num, m.snippet);
        assert_eq!(line, 5);
        // Should show exactly 5 lines of context
        let snippet_lines: Vec<&str> = snippet.lines().collect();
        assert_eq!(
            snippet_lines.len(),
            5,
            "Snippet should be 5 lines: {:?}",
            snippet_lines
        );
    }

    /// A mock tool for SmartEditTool tests — returns a configurable error or success.
    struct SmartEditMockTool {
        fail_msg: Option<String>,
        result_text: Option<String>,
    }

    #[async_trait::async_trait]
    impl AgentTool for SmartEditMockTool {
        fn name(&self) -> &str {
            "edit_file"
        }
        fn label(&self) -> &str {
            "edit_file"
        }
        fn description(&self) -> &str {
            "mock edit_file"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: yoagent::types::ToolContext,
        ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
            if let Some(ref msg) = self.fail_msg {
                Err(yoagent::types::ToolError::Failed(msg.clone()))
            } else {
                Ok(yoagent::types::ToolResult {
                    content: vec![yoagent::Content::Text {
                        text: self.result_text.clone().unwrap_or_else(|| "ok".into()),
                    }],
                    details: serde_json::Value::Null,
                })
            }
        }
    }

    #[tokio::test]
    async fn test_smart_edit_passes_through_success() {
        let tool = with_smart_edit(Box::new(SmartEditMockTool {
            fail_msg: None,
            result_text: Some("edited successfully".into()),
        }));

        let params = serde_json::json!({
            "path": "src/main.rs",
            "old_text": "fn main()",
            "new_text": "fn main2()"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_ok(), "Success should pass through");
    }

    #[tokio::test]
    async fn test_smart_edit_passes_through_non_not_found_error() {
        let tool = with_smart_edit(Box::new(SmartEditMockTool {
            fail_msg: Some("permission denied".into()),
            result_text: None,
        }));

        let params = serde_json::json!({
            "path": "src/main.rs",
            "old_text": "fn main()",
            "new_text": "fn main2()"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert_eq!(
            err, "permission denied",
            "Non-'not found' errors pass through unchanged"
        );
    }

    #[tokio::test]
    async fn test_smart_edit_augments_not_found_with_line_number() {
        // Create a temp file with known content
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(
            &file_path,
            "line one\nline two\nfn hello() {\n    world()\n}\nline six\n",
        )
        .unwrap();

        let tool = with_smart_edit(Box::new(SmartEditMockTool {
            fail_msg: Some("old_text not found in file".into()),
            result_text: None,
        }));

        let params = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "fn hello() {\n  world()\n}",
            "new_text": "fn goodbye()"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("line 3"), "Should mention line number: {err}");
        assert!(
            err.contains("fn hello()"),
            "Should show snippet with actual content: {err}"
        );
        assert!(
            err.contains("📍 Nearest match"),
            "Should have nearest match marker: {err}"
        );
    }

    #[tokio::test]
    async fn test_smart_edit_detects_whitespace_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("ws.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n    let x = 1;\n    let y = 2;\n}\n",
        )
        .unwrap();

        let tool = with_smart_edit(Box::new(SmartEditMockTool {
            fail_msg: Some("old_text not found in file".into()),
            result_text: None,
        }));

        // old_text with 2-space indent instead of 4-space
        let params = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "fn main() {\n  let x = 1;\n  let y = 2;\n}",
            "new_text": "fn main() {\n  let x = 42;\n}"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("indentation") || err.contains("whitespace"),
            "Should hint about whitespace difference: {err}"
        );
        assert!(err.contains("line 1"), "Should report line number: {err}");
    }

    #[tokio::test]
    async fn test_smart_edit_handles_missing_file_gracefully() {
        let tool = with_smart_edit(Box::new(SmartEditMockTool {
            fail_msg: Some("old_text not found in file".into()),
            result_text: None,
        }));

        let params = serde_json::json!({
            "path": "/nonexistent/file.rs",
            "old_text": "fn hello()",
            "new_text": "fn goodbye()"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // Should gracefully fall back to original message without panic
        assert!(
            err.contains("old_text not found"),
            "Should contain original error: {err}"
        );
    }

    // === extract_matched_text tests ===

    #[test]
    fn test_extract_matched_text_basic() {
        let content = "line 0\nline 1\nline 2\nline 3\nline 4\n";
        let result = extract_matched_text(content, 1, 2);
        assert_eq!(result, "line 1\nline 2");
    }

    #[test]
    fn test_extract_matched_text_from_start() {
        let content = "fn main() {\n    hello();\n}\n";
        let result = extract_matched_text(content, 0, 3);
        assert_eq!(result, "fn main() {\n    hello();\n}");
    }

    #[test]
    fn test_extract_matched_text_beyond_end() {
        let content = "line 0\nline 1\n";
        // Request more lines than available
        let result = extract_matched_text(content, 1, 5);
        assert_eq!(result, "line 1");
    }

    #[test]
    fn test_extract_matched_text_out_of_bounds() {
        let content = "line 0\n";
        let result = extract_matched_text(content, 10, 2);
        assert_eq!(result, "");
    }

    // === SmartEditTool whitespace auto-fix tests ===

    /// A stateful mock tool that fails on first call and succeeds on second.
    /// Used to simulate auto-fix retry behavior.
    struct SmartEditRetryMockTool {
        call_count: std::sync::atomic::AtomicUsize,
        /// If set, first call fails with this message.
        first_fail_msg: String,
    }

    #[async_trait::async_trait]
    impl AgentTool for SmartEditRetryMockTool {
        fn name(&self) -> &str {
            "edit_file"
        }
        fn label(&self) -> &str {
            "edit_file"
        }
        fn description(&self) -> &str {
            "mock edit_file (retry-aware)"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: yoagent::types::ToolContext,
        ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
            let call = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if call == 0 {
                Err(yoagent::types::ToolError::Failed(
                    self.first_fail_msg.clone(),
                ))
            } else {
                Ok(yoagent::types::ToolResult {
                    content: vec![yoagent::Content::Text {
                        text: "edit applied".into(),
                    }],
                    details: serde_json::Value::Null,
                })
            }
        }
    }

    /// A stateful mock that always fails (used to test retry-failure fallback).
    struct SmartEditAlwaysFailMockTool {
        fail_msg: String,
    }

    #[async_trait::async_trait]
    impl AgentTool for SmartEditAlwaysFailMockTool {
        fn name(&self) -> &str {
            "edit_file"
        }
        fn label(&self) -> &str {
            "edit_file"
        }
        fn description(&self) -> &str {
            "mock edit_file (always fails)"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: yoagent::types::ToolContext,
        ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
            Err(yoagent::types::ToolError::Failed(self.fail_msg.clone()))
        }
    }

    #[tokio::test]
    async fn test_smart_edit_autofix_whitespace_mismatch() {
        // Create a temp file with 4-space indentation
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("ws_fix.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n    let x = 1;\n    let y = 2;\n}\n",
        )
        .unwrap();

        // The mock fails on first call (wrong whitespace), succeeds on retry
        let tool = with_smart_edit(Box::new(SmartEditRetryMockTool {
            call_count: std::sync::atomic::AtomicUsize::new(0),
            first_fail_msg: "old_text not found in file".into(),
        }));

        // old_text with 2-space indent (wrong), new_text is the intended replacement
        let params = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "fn main() {\n  let x = 1;\n  let y = 2;\n}",
            "new_text": "fn main() {\n    let x = 42;\n}"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_ok(), "Auto-fix should succeed: {:?}", result);
        let result = result.unwrap();
        // Check that the auto-fix note is appended
        let texts: Vec<String> = result
            .content
            .iter()
            .filter_map(|c| match c {
                yoagent::Content::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect();
        let combined = texts.join(" ");
        assert!(
            combined.contains("Auto-fixed whitespace mismatch"),
            "Should contain auto-fix note: {combined}"
        );
        assert!(
            combined.contains("line 1"),
            "Should mention the line number: {combined}"
        );
    }

    #[tokio::test]
    async fn test_smart_edit_no_autofix_for_non_whitespace_mismatch() {
        // Create a temp file
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("no_fix.rs");
        std::fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

        let tool = with_smart_edit(Box::new(SmartEditMockTool {
            fail_msg: Some("old_text not found in file".into()),
            result_text: None,
        }));

        // old_text differs in content, not just whitespace
        let params = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "fn main() {\n    println!(\"world\");\n}",
            "new_text": "fn main() {\n    println!(\"goodbye\");\n}"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_err(), "Non-whitespace mismatch should still fail");
        let err = result.unwrap_err().to_string();
        // Should have the augmented error with nearest match, NOT auto-fix
        assert!(
            err.contains("not found"),
            "Should contain original error: {err}"
        );
        assert!(
            !err.contains("Auto-fixed"),
            "Should NOT contain auto-fix note: {err}"
        );
    }

    #[tokio::test]
    async fn test_smart_edit_autofix_retry_failure_falls_through() {
        // Create a temp file with 4-space indentation
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("fail_retry.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n    let x = 1;\n    let y = 2;\n}\n",
        )
        .unwrap();

        // The mock always fails — even the retry
        let tool = with_smart_edit(Box::new(SmartEditAlwaysFailMockTool {
            fail_msg: "old_text not found in file".into(),
        }));

        // old_text with whitespace mismatch (will trigger auto-fix attempt, but retry also fails)
        let params = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "fn main() {\n  let x = 1;\n  let y = 2;\n}",
            "new_text": "fn main() {\n    let x = 42;\n}"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(
            result.is_err(),
            "Should fall through to augmented error when retry fails"
        );
        let err = result.unwrap_err().to_string();
        // Should have the augmented error (from augment_not_found_error), including the hint
        assert!(
            err.contains("Nearest match"),
            "Should contain nearest match info: {err}"
        );
        assert!(
            err.contains("whitespace"),
            "Should contain whitespace hint: {err}"
        );
    }

    // === find_nearest_match edge case tests ===

    #[test]
    fn test_find_nearest_match_extra_blank_lines_in_old_text() {
        // File has no blank lines between statements; old_text has an extra blank line
        let file_content = "fn foo() {\n    let a = 1;\n    let b = 2;\n}\n";
        let old_text = "fn foo() {\n    let a = 1;\n\n    let b = 2;\n}";
        let result = find_nearest_match(file_content, old_text);
        // Should still find a match anchored on "fn foo()" even with the blank line mismatch
        assert!(
            result.is_some(),
            "Should find a match despite extra blank line"
        );
        let m = result.unwrap();
        let line = m.line_num;
        assert_eq!(line, 1, "Match should be at line 1");
    }

    #[test]
    fn test_find_nearest_match_fewer_blank_lines_in_old_text() {
        // File has a blank line; old_text omits it
        let file_content = "fn bar() {\n    let a = 1;\n\n    let b = 2;\n}\n";
        let old_text = "fn bar() {\n    let a = 1;\n    let b = 2;\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(
            result.is_some(),
            "Should find a match despite fewer blank lines"
        );
        let m = result.unwrap();
        let line = m.line_num;
        assert_eq!(line, 1, "Match should be at line 1");
    }

    #[test]
    fn test_find_nearest_match_at_start_of_file() {
        // Match is at the very first line
        let file_content = "fn first() {\n    body();\n}\nfn second() {}\n";
        let old_text = "fn first() {\n    body();\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some(), "Should find match at start");
        let m = result.unwrap();
        let (line, is_ws) = (m.line_num, m.is_whitespace_only);
        assert_eq!(line, 1, "Match should be at line 1 (very start)");
        assert!(!is_ws, "Should be an exact match, not whitespace-only");
    }

    #[test]
    fn test_find_nearest_match_at_end_of_file_no_trailing_newline() {
        // Match at the very end, file has no trailing newline
        let file_content = "fn first() {}\nfn last() {\n    done();\n}";
        let old_text = "fn last() {\n    done();\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some(), "Should find match at end of file");
        let m = result.unwrap();
        let (line, is_ws) = (m.line_num, m.is_whitespace_only);
        assert_eq!(line, 2, "Match should be at line 2");
        assert!(!is_ws, "Should be exact match");
    }

    #[test]
    fn test_find_nearest_match_at_end_snippet_truncated() {
        // When match is near the end, snippet should not go past file end
        let file_content = "a\nb\nc\nlast_line";
        let old_text = "last_line";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some());
        let m = result.unwrap();
        let (line, snippet) = (m.line_num, m.snippet);
        assert_eq!(line, 4, "Match at line 4 (last line)");
        // Snippet should only have 1 line since there's nothing after
        let snippet_lines: Vec<&str> = snippet.lines().collect();
        assert_eq!(snippet_lines.len(), 1, "Snippet limited to remaining lines");
        assert!(snippet.contains("last_line"));
    }

    #[test]
    fn test_find_nearest_match_very_short_old_text_single_char() {
        // A single character should still match if it exists as a whole line
        let file_content = "a\nb\nc\n";
        let old_text = "b";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some(), "Single char should match a whole line");
        let m = result.unwrap();
        let (line, is_ws) = (m.line_num, m.is_whitespace_only);
        assert_eq!(line, 2);
        assert!(!is_ws);
    }

    #[test]
    fn test_find_nearest_match_very_short_old_text_no_line_match() {
        // Short old_text that doesn't match any whole line (only substring)
        let file_content = "hello world\nfoo bar\nbaz qux\n";
        let old_text = "oo"; // substring of "foo" but not a whole line
        let result = find_nearest_match(file_content, old_text);
        assert!(
            result.is_none(),
            "Partial substring should not match (trimmed comparison is exact)"
        );
    }

    #[test]
    fn test_find_nearest_match_tabs_vs_spaces() {
        // File uses tabs, old_text uses spaces
        let file_content = "fn main() {\n\tlet x = 1;\n\tlet y = 2;\n}\n";
        let old_text = "fn main() {\n    let x = 1;\n    let y = 2;\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(
            result.is_some(),
            "Should find match with tab/space mismatch"
        );
        let m = result.unwrap();
        let (line, is_ws) = (m.line_num, m.is_whitespace_only);
        assert_eq!(line, 1);
        assert!(is_ws, "Tab vs space difference should be whitespace-only");
    }

    #[test]
    fn test_find_nearest_match_spaces_vs_tabs() {
        // Reverse: file uses spaces, old_text uses tabs
        let file_content = "fn main() {\n    let x = 1;\n}\n";
        let old_text = "fn main() {\n\tlet x = 1;\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(
            result.is_some(),
            "Should find match with space/tab mismatch"
        );
        let m = result.unwrap();
        let is_ws = m.is_whitespace_only;
        assert!(is_ws, "Space vs tab difference should be whitespace-only");
    }

    #[test]
    fn test_find_nearest_match_multiple_partial_matches_picks_best() {
        // Two functions with the same opening but different bodies.
        // The old_text matches the second one better (more lines match).
        let file_content = "fn do_thing() {\n    alpha();\n}\n\nfn do_thing() {\n    alpha();\n    beta();\n    gamma();\n}\n";
        let old_text = "fn do_thing() {\n    alpha();\n    beta();\n    gamma();\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some(), "Should find the best match");
        let m = result.unwrap();
        let (line, is_ws) = (m.line_num, m.is_whitespace_only);
        // The second fn do_thing() starts at line 5 and matches 5 lines
        assert_eq!(line, 5, "Should pick the better (longer) match at line 5");
        assert!(!is_ws);
    }

    #[test]
    fn test_find_nearest_match_multiple_matches_first_if_equal() {
        // Two identical matches — should pick the one with the higher match count
        // (in practice, if equal, the later one wins because of > comparison)
        let file_content = "let x = 1;\nlet y = 2;\nlet x = 1;\nlet y = 2;\n";
        let old_text = "let x = 1;\nlet y = 2;";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some());
        let m = result.unwrap();
        let line = m.line_num;
        // Both matches have count=2, so the second one wins (> not >=)
        // Actually let's check: is_none_or with match_count > prev_count means
        // equal count does NOT replace, so first match wins
        assert_eq!(line, 1, "Equal matches: first one wins");
    }

    #[test]
    fn test_find_nearest_match_unicode_content() {
        let file_content =
            "fn greet() {\n    println!(\"こんにちは\");\n    println!(\"世界\");\n}\n";
        let old_text = "fn greet() {\n    println!(\"こんにちは\");\n    println!(\"世界\");\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some(), "Should match Unicode content");
        let m = result.unwrap();
        let (line, is_ws, snippet) = (m.line_num, m.is_whitespace_only, m.snippet);
        assert_eq!(line, 1);
        assert!(!is_ws);
        assert!(
            snippet.contains("こんにちは"),
            "Snippet should contain Unicode"
        );
    }

    #[test]
    fn test_find_nearest_match_unicode_with_whitespace_diff() {
        // Unicode content with indentation mismatch
        let file_content = "fn emoji() {\n    let msg = \"🎉✓\";\n}\n";
        let old_text = "fn emoji() {\n  let msg = \"🎉✓\";\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some());
        let m = result.unwrap();
        let is_ws = m.is_whitespace_only;
        assert!(
            is_ws,
            "Unicode content with only whitespace diff should be detected"
        );
    }

    #[test]
    fn test_find_nearest_match_empty_file() {
        let result = find_nearest_match("", "fn hello()");
        assert!(result.is_none(), "Empty file should return None");
    }

    #[test]
    fn test_find_nearest_match_empty_file_empty_old_text() {
        let result = find_nearest_match("", "");
        assert!(result.is_none(), "Both empty should return None");
    }

    #[test]
    fn test_find_nearest_match_old_text_with_leading_empty_lines() {
        // old_text starts with empty lines, the anchor is on a later line
        let file_content = "fn alpha() {}\n\nfn beta() {\n    body();\n}\n";
        let old_text = "\n\nfn beta() {\n    body();\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(
            result.is_some(),
            "Should find match even with leading empty lines in old_text"
        );
        let m = result.unwrap();
        let line = m.line_num;
        // The anchor "fn beta() {" is at file line 3 (1-indexed),
        // and anchor_offset is 2 (two leading empty lines), so start_line adjusts back
        assert!(line <= 3, "Line should account for leading empty lines");
    }

    #[test]
    fn test_find_nearest_match_single_line_file() {
        let file_content = "only_line";
        let old_text = "only_line";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some());
        let m = result.unwrap();
        let (line, is_ws) = (m.line_num, m.is_whitespace_only);
        assert_eq!(line, 1);
        assert!(!is_ws);
    }

    #[test]
    fn test_find_nearest_match_trailing_whitespace_diff() {
        // File lines have trailing spaces, old_text doesn't
        let file_content = "fn main() {  \n    let x = 1;  \n}\n";
        let old_text = "fn main() {\n    let x = 1;\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some());
        let m = result.unwrap();
        let (line, is_ws) = (m.line_num, m.is_whitespace_only);
        assert_eq!(line, 1);
        assert!(
            is_ws,
            "Trailing whitespace difference should be whitespace-only"
        );
    }

    // === augment_not_found_error edge case tests ===

    #[test]
    fn test_augment_not_found_error_missing_path() {
        let tool = SmartEditTool {
            inner: Box::new(SmartEditMockTool {
                fail_msg: None,
                result_text: None,
            }),
        };
        let params = serde_json::json!({
            "old_text": "fn hello()",
            "new_text": "fn goodbye()"
        });
        let result = tool.augment_not_found_error("old_text not found in file", &params);
        assert_eq!(
            result, "old_text not found in file",
            "Missing path should return original msg"
        );
    }

    #[test]
    fn test_augment_not_found_error_missing_old_text() {
        let tool = SmartEditTool {
            inner: Box::new(SmartEditMockTool {
                fail_msg: None,
                result_text: None,
            }),
        };
        let params = serde_json::json!({
            "path": "/some/file.rs",
            "new_text": "fn goodbye()"
        });
        let result = tool.augment_not_found_error("old_text not found in file", &params);
        assert_eq!(
            result, "old_text not found in file",
            "Missing old_text should return original msg"
        );
    }

    #[test]
    fn test_augment_not_found_error_nonexistent_file() {
        let tool = SmartEditTool {
            inner: Box::new(SmartEditMockTool {
                fail_msg: None,
                result_text: None,
            }),
        };
        let params = serde_json::json!({
            "path": "/definitely/does/not/exist/file.rs",
            "old_text": "fn hello()",
            "new_text": "fn goodbye()"
        });
        let result = tool.augment_not_found_error("old_text not found in file", &params);
        assert_eq!(
            result, "old_text not found in file",
            "Nonexistent file should return original msg"
        );
    }

    #[test]
    fn test_augment_not_found_error_line_number_accuracy() {
        // Create a temp file with known content, verify exact line number in output
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("lines.rs");
        std::fs::write(
            &file_path,
            "line 1\nline 2\nline 3\nfn target() {\n    body();\n}\nline 7\n",
        )
        .unwrap();

        let tool = SmartEditTool {
            inner: Box::new(SmartEditMockTool {
                fail_msg: None,
                result_text: None,
            }),
        };
        let params = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "fn target() {\n  body();\n}",
            "new_text": "fn replacement()"
        });
        let result = tool.augment_not_found_error("old_text not found", &params);
        assert!(
            result.contains("line 4"),
            "Should report line 4 for fn target(): {result}"
        );
        assert!(
            result.contains("fn target()"),
            "Should show the actual content"
        );
        assert!(
            result.contains("whitespace"),
            "Should hint about whitespace diff"
        );
    }

    #[test]
    fn test_augment_not_found_error_no_match_in_file() {
        // File exists but old_text has no match at all
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("nomatch.rs");
        std::fs::write(&file_path, "fn alpha() {}\nfn beta() {}\n").unwrap();

        let tool = SmartEditTool {
            inner: Box::new(SmartEditMockTool {
                fail_msg: None,
                result_text: None,
            }),
        };
        let params = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "fn completely_unrelated()",
            "new_text": "fn replacement()"
        });
        let result = tool.augment_not_found_error("old_text not found", &params);
        // No match found, should return original message unaugmented
        assert_eq!(
            result, "old_text not found",
            "No match should return original: {result}"
        );
    }

    // === extract_matched_text additional tests ===

    #[test]
    fn test_extract_matched_text_empty_content() {
        let result = extract_matched_text("", 0, 3);
        assert_eq!(result, "", "Empty content should return empty string");
    }

    #[test]
    fn test_extract_matched_text_single_line() {
        let result = extract_matched_text("only_line", 0, 1);
        assert_eq!(result, "only_line");
    }

    #[test]
    fn test_extract_matched_text_exact_range() {
        let content = "a\nb\nc\nd\n";
        let result = extract_matched_text(content, 0, 4);
        assert_eq!(result, "a\nb\nc\nd");
    }

    // === line_similarity tests ===

    #[test]
    fn test_line_similarity_identical() {
        assert!((line_similarity("fn hello()", "fn hello()") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_line_similarity_whitespace_only_diff() {
        // Whitespace is trimmed, so these are identical
        assert!((line_similarity("  fn hello()", "    fn hello()") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_line_similarity_empty_strings() {
        assert!((line_similarity("", "") - 1.0).abs() < f64::EPSILON);
        assert!((line_similarity("   ", "  ") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_line_similarity_small_typo() {
        // "println" vs "prinltn" — 1 transposition in 7 chars = high similarity
        let sim = line_similarity("println!(\"hello\")", "prinltn!(\"hello\")");
        assert!(sim > 0.8, "Small typo should have high similarity: {sim}");
    }

    #[test]
    fn test_line_similarity_completely_different() {
        let sim = line_similarity("fn hello() {}", "use std::io;");
        assert!(
            sim < 0.5,
            "Completely different should have low similarity: {sim}"
        );
    }

    // === edit_distance tests ===

    #[test]
    fn test_edit_distance_identical() {
        assert_eq!(edit_distance("hello", "hello"), 0);
    }

    #[test]
    fn test_edit_distance_empty() {
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("abc", ""), 3);
        assert_eq!(edit_distance("", ""), 0);
    }

    #[test]
    fn test_edit_distance_single_change() {
        assert_eq!(edit_distance("cat", "bat"), 1);
        assert_eq!(edit_distance("cat", "cats"), 1);
        assert_eq!(edit_distance("cat", "at"), 1);
    }

    // === fuzzy matching tests ===

    #[test]
    fn test_fuzzy_match_single_char_typo() {
        let file_content = "fn main() {\n    println!(\"hello world\");\n}\n";
        // Typo: "helo" instead of "hello"
        let old_text = "    println!(\"helo world\");";
        let result = find_nearest_match(file_content, old_text);
        assert!(
            result.is_some(),
            "Should find fuzzy match for single-char typo"
        );
        let m = result.unwrap();
        assert_eq!(m.line_num, 2);
        assert!(m.is_fuzzy, "Should be marked as fuzzy");
        assert!(
            m.similarity > 0.8,
            "Similarity should be high for single-char typo: {}",
            m.similarity
        );
    }

    #[test]
    fn test_fuzzy_match_minor_hallucination() {
        let file_content =
            "fn process_data(input: &str) -> Result<String, Error> {\n    let result = input.trim();\n    Ok(result.to_string())\n}\n";
        // LLM hallucinated slightly different function signature
        let old_text = "fn process_data(input: &str) -> Result<String, MyError> {";
        let result = find_nearest_match(file_content, old_text);
        assert!(
            result.is_some(),
            "Should find fuzzy match for minor hallucination"
        );
        let m = result.unwrap();
        assert_eq!(m.line_num, 1);
        assert!(m.is_fuzzy);
    }

    #[test]
    fn test_fuzzy_match_multi_line_block() {
        let file_content = "fn alpha() {}\nfn beta(x: i32) {\n    let y = x + 1;\n    println!(\"{}\", y);\n}\nfn gamma() {}\n";
        // Typo in function name and argument
        let old_text = "fn bata(x: i32) {\n    let y = x + 1;\n    println!(\"{}\", y);\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(
            result.is_some(),
            "Should find fuzzy match for multi-line block with typo"
        );
        let m = result.unwrap();
        assert_eq!(m.line_num, 2, "Should match at line 2 (fn beta)");
        assert!(m.is_fuzzy);
    }

    #[test]
    fn test_fuzzy_match_not_triggered_for_exact() {
        // When there's an exact trimmed match, fuzzy should NOT be used
        let file_content = "fn hello() {\n    println!(\"hi\");\n}\n";
        let old_text = "  fn hello() {"; // whitespace diff only
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some());
        let m = result.unwrap();
        assert!(
            !m.is_fuzzy,
            "Exact trimmed match should not be marked fuzzy"
        );
    }

    #[test]
    fn test_fuzzy_match_too_dissimilar() {
        let file_content = "fn hello() {\n    println!(\"hi\");\n}\n";
        let old_text = "struct CompletelyDifferent { field: u32 }";
        let result = find_nearest_match(file_content, old_text);
        // Should return None because nothing is similar enough
        assert!(
            result.is_none(),
            "Completely different content should not match"
        );
    }

    #[test]
    fn test_fuzzy_match_augmented_error_message() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("fuzzy.rs");
        std::fs::write(&file_path, "fn process(x: i32) -> i32 {\n    x + 1\n}\n").unwrap();

        let tool = SmartEditTool {
            inner: Box::new(SmartEditMockTool {
                fail_msg: None,
                result_text: None,
            }),
        };
        // Typo in function name
        let params = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "fn procss(x: i32) -> i32 {",
            "new_text": "fn process(x: i64) -> i64 {"
        });
        let result = tool.augment_not_found_error("old_text not found", &params);
        assert!(
            result.contains("fuzzy match"),
            "Should mention fuzzy match: {result}"
        );
        assert!(
            result.contains("similar"),
            "Should mention similarity: {result}"
        );
        assert!(
            result.contains("line 1"),
            "Should indicate line number: {result}"
        );
    }

    #[test]
    fn test_block_similarity_perfect() {
        let file_lines = vec!["fn hello() {", "    println!(\"hi\");", "}"];
        let old_lines = vec!["fn hello() {", "    println!(\"hi\");", "}"];
        let sim = block_similarity(&file_lines, &old_lines, 0);
        assert!(
            (sim - 1.0).abs() < f64::EPSILON,
            "Identical blocks should have similarity 1.0: {sim}"
        );
    }

    #[test]
    fn test_block_similarity_partial() {
        let file_lines = vec!["fn hello() {", "    println!(\"hi\");", "}"];
        let old_lines = vec!["fn helo() {", "    println!(\"hi\");", "}"];
        let sim = block_similarity(&file_lines, &old_lines, 0);
        assert!(
            sim > 0.9,
            "One-char typo in one of three lines should be high similarity: {sim}"
        );
        assert!(sim < 1.0, "Should not be perfect: {sim}");
    }

    // === Ambiguity detection tests ===

    #[test]
    fn test_find_exact_trimmed_match_ambiguous_duplicate_blocks() {
        // Two identical blocks at different indentation levels
        let file_lines = vec![
            "fn alpha() {",      // 0
            "  if x > 0 {",      // 1
            "    do_thing();",   // 2
            "  }",               // 3
            "}",                 // 4
            "",                  // 5
            "fn beta() {",       // 6
            "    if x > 0 {",    // 7
            "      do_thing();", // 8
            "    }",             // 9
            "}",                 // 10
        ];
        let old_lines = vec!["if x > 0 {", "  do_thing();", "}"];

        let result = find_exact_trimmed_match(&file_lines, &old_lines);
        assert!(result.is_some(), "Should find a match");
        let (_line, _count, is_ambiguous) = result.unwrap();
        assert!(
            is_ambiguous,
            "Should detect ambiguity when two blocks match equally well"
        );
    }

    #[test]
    fn test_find_exact_trimmed_match_unambiguous_single_block() {
        let file_lines = vec![
            "fn alpha() {",
            "    let x = 1;",
            "    let y = 2;",
            "}",
            "",
            "fn beta() {",
            "    let z = 3;",
            "}",
        ];
        let old_lines = vec!["fn alpha() {", "    let x = 1;", "    let y = 2;", "}"];

        let result = find_exact_trimmed_match(&file_lines, &old_lines);
        assert!(result.is_some());
        let (line, _count, is_ambiguous) = result.unwrap();
        assert_eq!(line, 0, "Should match at line 0");
        assert!(!is_ambiguous, "Single match should not be ambiguous");
    }

    #[test]
    fn test_find_nearest_match_ambiguous_blocks_no_autofix() {
        // File with two identical if-blocks at different indent levels
        let file_content = "\
fn alpha() {
  if x > 0 {
    do_thing();
  }
}

fn beta() {
    if x > 0 {
      do_thing();
    }
}";
        let old_text = "if x > 0 {\n  do_thing();\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some());
        let m = result.unwrap();
        assert!(
            m.is_whitespace_only,
            "Trimmed lines match — whitespace-only diff"
        );
        assert!(
            m.is_ambiguous,
            "Two equally-good matches should be flagged as ambiguous"
        );
    }

    #[test]
    fn test_find_nearest_match_unique_block_not_ambiguous() {
        let file_content = "\
fn alpha() {
    let x = 1;
    let y = 2;
}

fn beta() {
    let z = 3;
}";
        let old_text = "fn alpha() {\n  let x = 1;\n  let y = 2;\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some());
        let m = result.unwrap();
        assert!(!m.is_ambiguous, "Unique block should not be ambiguous");
    }

    #[test]
    fn test_line_similarity_char_based_not_bytes() {
        // CJK characters are 3 bytes each. With byte-based length the similarity
        // would be inflated because max_len (bytes) >> edit_distance (chars).
        // "你好" vs "再见" — 2 chars each, both completely different → similarity should be 0.0.
        let sim = line_similarity("你好", "再见");
        assert!(
            sim < 0.01,
            "Completely different CJK strings should have ~0.0 similarity, got {sim}"
        );
    }

    #[test]
    fn test_line_similarity_ascii_unchanged() {
        // Sanity check: ASCII behavior should be unaffected by the char-based fix
        let sim = line_similarity("hello world", "hello world");
        assert!((sim - 1.0).abs() < f64::EPSILON, "Identical strings: {sim}");

        let sim2 = line_similarity("hello", "helo");
        // 1 deletion out of 5 chars → distance=1, max_len=5, sim=0.8
        assert!(
            (sim2 - 0.8).abs() < 0.01,
            "One-char deletion should be 0.8 similarity: {sim2}"
        );
    }

    #[tokio::test]
    async fn test_smart_edit_refuses_autofix_on_ambiguous_match() {
        // Create a file with two identical blocks at different indentation
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("ambiguous.rs");
        std::fs::write(
            &file_path,
            "fn alpha() {\n  if x > 0 {\n    do_thing();\n  }\n}\n\n\
             fn beta() {\n    if x > 0 {\n      do_thing();\n    }\n}\n",
        )
        .unwrap();

        let tool = SmartEditTool {
            inner: Box::new(SmartEditMockTool {
                fail_msg: Some("old_text not found in file".into()),
                result_text: None,
            }),
        };

        // old_text with wrong indent that trimmed-matches both blocks
        let params = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "if x > 0 {\n  do_thing();\n}",
            "new_text": "if x > 0 {\n  do_other();\n}"
        });

        let result = tool
            .try_whitespace_autofix("old_text not found", &params, &test_tool_context())
            .await;
        assert!(
            result.is_none(),
            "Should refuse auto-fix when match is ambiguous"
        );

        // The augmented error should mention the ambiguity
        let augmented = tool.augment_not_found_error("old_text not found", &params);
        assert!(
            augmented.contains("Multiple identical blocks"),
            "Augmented error should warn about ambiguity: {augmented}"
        );
    }
}
