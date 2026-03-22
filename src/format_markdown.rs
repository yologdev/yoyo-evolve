//! Markdown rendering for streamed LLM output.
//!
//! Provides `MarkdownRenderer` which incrementally processes text deltas
//! and applies ANSI formatting for code blocks, inline code, bold, italic,
//! headers, lists, blockquotes, and horizontal rules.

use crate::format::{BOLD, BOLD_CYAN, BOLD_ITALIC, CYAN, DIM, ITALIC, RESET};
use crate::format_syntax::highlight_code_line;

/// Incremental markdown renderer for streamed text output.
/// Tracks state across partial deltas to apply ANSI formatting for
/// code blocks, inline code, bold text, and headers.
///
/// Designed for LLM streaming: mid-line tokens are rendered immediately
/// with inline formatting. Only line boundaries buffer briefly to detect
/// code fences (`` ``` ``) and headers (`#`).
pub struct MarkdownRenderer {
    in_code_block: bool,
    code_lang: Option<String>,
    line_buffer: String,
    /// Whether we're at the start of a new line (need to detect fence/header).
    line_start: bool,
    /// When a block element prefix (list marker, blockquote `>`) has been rendered
    /// early for streaming, this tracks the prefix so we don't re-render on newline.
    /// Once set, subsequent tokens stream as inline text until the newline arrives.
    block_prefix_rendered: bool,
}

impl MarkdownRenderer {
    /// Create a new renderer with empty state.
    pub fn new() -> Self {
        Self {
            in_code_block: false,
            code_lang: None,
            line_buffer: String::new(),
            line_start: true,
            block_prefix_rendered: false,
        }
    }

    /// Process a delta chunk and return ANSI-formatted output.
    ///
    /// **Streaming behavior:**
    /// - At line start, buffers briefly to detect code fences/headers (typically 1–4 chars)
    /// - Mid-line, renders immediately with inline formatting (bold, inline code)
    /// - Complete lines (ending with `\n`) are always processed immediately
    ///
    /// ## render_latency_budget
    ///
    /// The renderer is designed for minimal token-to-display latency:
    ///
    /// | Path                  | Buffering             | Expected latency |
    /// |-----------------------|-----------------------|------------------|
    /// | Mid-line text         | None (immediate)      | ~0 (no alloc)    |
    /// | Mid-line code block   | None (immediate)      | ~0 (dim wrap)    |
    /// | Line-start, non-special | Flush after 1 char  | ~0               |
    /// | Line-start, ambiguous | Buffer 1–4 chars      | 1 token          |
    /// | Line-start, code block| Buffer until non-`\`` | 1 token          |
    ///
    /// **Flush contract:** Every call to `render_delta()` that produces output
    /// expects the caller to call `io::stdout().flush()` immediately after
    /// printing. This ensures tokens appear on screen without stdio batching.
    /// The caller in `prompt.rs::handle_events()` does this after every delta.
    ///
    /// **Do not regress:** Adding new buffering paths (e.g., for tables or
    /// footnotes) must preserve the mid-line fast path. Any change that causes
    /// mid-line tokens to return empty strings is a latency regression.
    pub fn render_delta(&mut self, delta: &str) -> String {
        let mut output = String::new();

        // Mid-line fast paths: render tokens immediately without buffering.
        // Code fences and headers only matter at line start, so mid-line is safe.
        if !self.line_start {
            if self.in_code_block {
                // Mid-line inside a code block: emit tokens immediately with
                // appropriate styling (dim or syntax-highlighted) instead of
                // buffering until a complete line. This gives token-by-token
                // streaming for code blocks (issue #147).
                if let Some(newline_pos) = delta.find('\n') {
                    let mid_line_part = &delta[..newline_pos];
                    if !mid_line_part.is_empty() {
                        output.push_str(&self.render_code_inline(mid_line_part));
                    }
                    output.push('\n');
                    self.line_start = true;
                    self.block_prefix_rendered = false;

                    // Process the rest (after the first \n) via buffered path
                    // because we're now at line start and need fence detection.
                    let rest = &delta[newline_pos + 1..];
                    if !rest.is_empty() {
                        output.push_str(&self.render_delta_buffered(rest));
                    }
                } else {
                    // No newline — pure mid-line code content, render immediately
                    output.push_str(&self.render_code_inline(delta));
                }
                return output;
            }

            // Mid-line outside a code block: render with inline formatting
            if let Some(newline_pos) = delta.find('\n') {
                // Render the mid-line portion immediately
                let mid_line_part = &delta[..newline_pos];
                if !mid_line_part.is_empty() {
                    output.push_str(&self.render_inline(mid_line_part));
                }
                output.push('\n');
                self.line_start = true;
                self.block_prefix_rendered = false;

                // Process the rest (after the first \n) by buffering
                let rest = &delta[newline_pos + 1..];
                if !rest.is_empty() {
                    output.push_str(&self.render_delta_buffered(rest));
                }
            } else {
                // No newline — pure mid-line content, render immediately
                output.push_str(&self.render_inline(delta));
            }
            return output;
        }

        // We're at line start — use buffered approach (needed to detect fences, headers)
        output.push_str(&self.render_delta_buffered(delta));
        output
    }

    /// Render a code block fragment with dim styling for immediate streaming.
    /// Used for mid-line token-by-token output inside code blocks.
    /// Full syntax highlighting is applied to complete lines (at newline boundaries);
    /// fragments get dim styling for responsiveness.
    fn render_code_inline(&self, text: &str) -> String {
        format!("{DIM}{text}{RESET}")
    }

    /// Buffered rendering: adds delta to line_buffer, processes complete lines,
    /// and attempts early flush of line-start content when safe.
    ///
    /// render_latency_budget: This path is only entered at line start. The buffer
    /// holds at most 1–4 characters before resolving. The `needs_line_buffering()`
    /// check and `try_resolve_block_prefix()` aim to flush as early as possible,
    /// switching to the mid-line fast path for subsequent tokens.
    fn render_delta_buffered(&mut self, delta: &str) -> String {
        let mut output = String::new();
        self.line_buffer.push_str(delta);

        // Process all complete lines (those ending with \n)
        while let Some(newline_pos) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..newline_pos].to_string();
            self.line_buffer = self.line_buffer[newline_pos + 1..].to_string();

            if self.block_prefix_rendered {
                // The prefix (bullet, quote marker, etc.) was already rendered.
                // Just render remaining content as inline text.
                output.push_str(&self.render_inline(&line));
            } else {
                output.push_str(&self.render_line(&line));
            }
            output.push('\n');
            self.line_start = true;
            self.block_prefix_rendered = false;
        }

        // Try to resolve the line-start buffer early:
        // If we have enough characters to determine it's NOT a fence, header,
        // or other block-level construct (list, blockquote, hr), flush as inline text.
        if self.line_start && !self.line_buffer.is_empty() && !self.in_code_block {
            if !self.needs_line_buffering() {
                // Definitely not a fence, header, or block element — flush as inline text
                let buf = std::mem::take(&mut self.line_buffer);
                output.push_str(&self.render_inline(&buf));
                self.line_start = false;
            } else {
                // Check if we can confirm a block element and render its prefix early,
                // switching to mid-line streaming for subsequent tokens.
                output.push_str(&self.try_resolve_block_prefix());
            }
        }

        // Inside a code block at line start: early-resolve when content can't be a
        // closing fence. Only ``` matters here (no headers, lists, etc.). Once we
        // know it's not a fence, flush as code content and set line_start=false so
        // subsequent tokens stream immediately via the mid-line fast path (issue #147).
        //
        // render_latency_budget: In CommonMark, a closing fence can have 0–3 spaces
        // of indentation. Content with >3 leading spaces or any non-backtick first
        // non-space char is guaranteed not to be a fence and resolves immediately.
        if self.line_start && !self.line_buffer.is_empty() && self.in_code_block {
            let leading_spaces = self.line_buffer.len() - self.line_buffer.trim_start().len();
            let trimmed = self.line_buffer.trim_start();

            let could_be_fence = if leading_spaces > 3 {
                // >3 spaces of indentation — can't be a closing fence per CommonMark
                false
            } else {
                trimmed.is_empty() || trimmed.starts_with('`') || "`".starts_with(trimmed)
            };

            if !could_be_fence {
                // Definitely not a closing fence — flush as code content immediately
                let buf = std::mem::take(&mut self.line_buffer);
                output.push_str(&self.render_code_inline(&buf));
                self.line_start = false;
            }
        }

        output
    }

    /// Check if the current line_buffer content at line start still needs buffering
    /// because it could be a markdown control sequence (fence, header, block element).
    /// Returns false when the content is definitely plain text and can be flushed.
    fn needs_line_buffering(&self) -> bool {
        let trimmed = self.line_buffer.trim_start();
        if trimmed.is_empty() {
            return true;
        }

        let could_be_fence = trimmed.starts_with('`') || "`".starts_with(trimmed);
        let could_be_header = trimmed.starts_with('#') || "#".starts_with(trimmed);

        if could_be_fence || could_be_header {
            return true;
        }

        // Check for block-level constructs
        let first = trimmed.as_bytes()[0];
        match first {
            b'>' => true, // blockquote — always a block element
            b'+' => trimmed.len() < 2 || trimmed.starts_with("+ "),
            b'-' => {
                trimmed.len() < 3 || trimmed.starts_with("- ") || {
                    let no_sp: String = trimmed.chars().filter(|c| *c != ' ').collect();
                    !no_sp.is_empty() && no_sp.chars().all(|c| c == '-')
                }
            }
            b'*' => {
                trimmed.len() < 2 || trimmed.starts_with("* ") || {
                    let no_sp: String = trimmed.chars().filter(|c| *c != ' ').collect();
                    !no_sp.is_empty() && no_sp.chars().all(|c| c == '*')
                }
            }
            b'_' => {
                trimmed.len() < 3 || {
                    let no_sp: String = trimmed.chars().filter(|c| *c != ' ').collect();
                    !no_sp.is_empty() && no_sp.chars().all(|c| c == '_')
                }
            }
            b'0'..=b'9' => {
                trimmed.len() < 3
                    || trimmed.contains(". ")
                        && trimmed[..trimmed.find(". ").unwrap_or(0)]
                            .chars()
                            .all(|c| c.is_ascii_digit())
            }
            b'|' => true, // table row
            _ => false,
        }
    }

    /// Try to resolve a confirmed block element prefix and render it immediately.
    /// When successful, renders the prefix (bullet, quote marker, etc.) and sets
    /// `line_start = false` so subsequent tokens stream via the mid-line fast path.
    /// Returns any rendered output.
    fn try_resolve_block_prefix(&mut self) -> String {
        let trimmed = self.line_buffer.trim_start();
        if trimmed.is_empty() {
            return String::new();
        }

        let first = trimmed.as_bytes()[0];

        // Blockquote: ">" or "> " confirmed — render prefix, stream rest
        if first == b'>' {
            let rest = trimmed.strip_prefix('>').unwrap_or("");
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            let prefix_output = format!("{DIM}│{RESET} {ITALIC}");
            let rest_output = if !rest.is_empty() {
                self.render_inline(rest)
            } else {
                String::new()
            };
            self.line_buffer.clear();
            self.line_start = false;
            self.block_prefix_rendered = true;
            return format!("{prefix_output}{rest_output}");
        }

        // Unordered list: confirmed when we see "- X", "* X", "+ X"
        // where X is NOT a continuation of a horizontal rule
        if let Some(content) = self.try_confirm_unordered_list(trimmed) {
            let indent = Self::leading_whitespace(&self.line_buffer);
            let content_output = if !content.is_empty() {
                self.render_inline(content)
            } else {
                String::new()
            };
            let prefix_output = format!("{indent}{CYAN}•{RESET} {content_output}");
            self.line_buffer.clear();
            self.line_start = false;
            self.block_prefix_rendered = true;
            return prefix_output;
        }

        // Ordered list: confirmed when we see "N. " with content
        if let Some((num, content)) = self.try_confirm_ordered_list(trimmed) {
            let indent = Self::leading_whitespace(&self.line_buffer);
            let content_output = if !content.is_empty() {
                self.render_inline(content)
            } else {
                String::new()
            };
            let prefix_output = format!("{indent}{CYAN}{num}.{RESET} {content_output}");
            self.line_buffer.clear();
            self.line_start = false;
            self.block_prefix_rendered = true;
            return prefix_output;
        }

        String::new()
    }

    /// Try to confirm an unordered list item and return the content after the marker.
    /// Only confirms when we have enough content to rule out a horizontal rule.
    /// For "- ", confirms when a non-dash non-space character follows.
    /// For "* ", confirms when a non-star non-space character follows.
    /// For "+ ", always a list item (no ambiguity with HR).
    fn try_confirm_unordered_list<'a>(&self, trimmed: &'a str) -> Option<&'a str> {
        // "+ X" — always a list item
        if let Some(rest) = trimmed.strip_prefix("+ ") {
            if !rest.is_empty() {
                return Some(rest);
            }
            // "+ " alone: still ambiguous (could get more dashes), but "+ " is a list
            return Some(rest);
        }

        // "- X" — list item if X contains a non-dash, non-space char
        if let Some(rest) = trimmed.strip_prefix("- ") {
            if !rest.is_empty() && rest.chars().any(|c| c != '-' && c != ' ') {
                return Some(rest);
            }
            return None; // Could still be "- - -" horizontal rule
        }

        // "* X" — list item if X contains a non-star, non-space char
        if let Some(rest) = trimmed.strip_prefix("* ") {
            if !rest.is_empty() && rest.chars().any(|c| c != '*' && c != ' ') {
                return Some(rest);
            }
            return None; // Could still be "* * *" horizontal rule
        }

        None
    }

    /// Try to confirm an ordered list item and return (number, content).
    /// Confirms when we see "N. " followed by actual content.
    fn try_confirm_ordered_list<'a>(&self, trimmed: &'a str) -> Option<(&'a str, &'a str)> {
        let dot_space = trimmed.find(". ")?;
        let num_part = &trimmed[..dot_space];
        if num_part.is_empty() || !num_part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let content = &trimmed[dot_space + 2..];
        if content.is_empty() {
            return None; // Haven't seen content yet
        }
        Some((num_part, content))
    }

    /// Flush any remaining buffered content (call after stream ends).
    pub fn flush(&mut self) -> String {
        if self.line_buffer.is_empty() {
            if self.block_prefix_rendered {
                // Close any open italic from blockquote prefix
                self.block_prefix_rendered = false;
                return format!("{RESET}");
            }
            return String::new();
        }
        let line = std::mem::take(&mut self.line_buffer);
        self.line_start = true;
        if self.block_prefix_rendered {
            self.block_prefix_rendered = false;
            // Prefix already rendered — just render remaining inline content
            let formatted = self.render_inline(&line);
            return format!("{formatted}{RESET}");
        }
        self.render_line(&line)
    }

    /// Render a single complete line, updating state for code fences.
    fn render_line(&mut self, line: &str) -> String {
        let trimmed = line.trim();
        // After rendering a complete line, next content will be at line start
        self.line_start = true;
        self.block_prefix_rendered = false;

        // Check for code fence (``` with optional language)
        if let Some(after_fence) = trimmed.strip_prefix("```") {
            if self.in_code_block {
                // Closing fence
                self.in_code_block = false;
                self.code_lang = None;
                return format!("{DIM}{line}{RESET}");
            } else {
                // Opening fence — capture language if present
                self.in_code_block = true;
                let lang = after_fence.trim();
                self.code_lang = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };
                return format!("{DIM}{line}{RESET}");
            }
        }

        if self.in_code_block {
            // Code block content: syntax highlight if language is known, else dim
            return if let Some(ref lang) = self.code_lang {
                highlight_code_line(lang, line)
            } else {
                format!("{DIM}{line}{RESET}")
            };
        }

        // Header: # at line start → BOLD+CYAN
        if trimmed.starts_with('#') {
            return format!("{BOLD}{CYAN}{line}{RESET}");
        }

        // Horizontal rule: ---, ***, ___ (3+ of the same char, possibly with spaces)
        if Self::is_horizontal_rule(trimmed) {
            let width = 40;
            return format!("{DIM}{}{RESET}", "─".repeat(width));
        }

        // Blockquote: > at line start
        if let Some(rest) = trimmed.strip_prefix('>') {
            let content = rest.strip_prefix(' ').unwrap_or(rest);
            let formatted = self.render_inline(content);
            return format!("{DIM}│{RESET} {ITALIC}{formatted}{RESET}");
        }

        // Unordered list: lines starting with - , * , or +  (with optional leading whitespace)
        if let Some(content) = Self::strip_unordered_list_marker(trimmed) {
            let indent = Self::leading_whitespace(line);
            let formatted = self.render_inline(content);
            return format!("{indent}{CYAN}•{RESET} {formatted}");
        }

        // Ordered list: lines matching N. text
        if let Some((num, content)) = Self::strip_ordered_list_marker(trimmed) {
            let indent = Self::leading_whitespace(line);
            let formatted = self.render_inline(content);
            return format!("{indent}{CYAN}{num}.{RESET} {formatted}");
        }

        // Apply inline formatting for normal text
        self.render_inline(line)
    }

    /// Check if a trimmed line is a horizontal rule (---, ***, ___, 3+ chars).
    fn is_horizontal_rule(trimmed: &str) -> bool {
        if trimmed.len() < 3 {
            return false;
        }
        let no_spaces: String = trimmed.chars().filter(|c| *c != ' ').collect();
        if no_spaces.len() < 3 {
            return false;
        }
        let first = no_spaces.chars().next().unwrap();
        (first == '-' || first == '*' || first == '_') && no_spaces.chars().all(|c| c == first)
    }

    /// Strip an unordered list marker (- , * , + ) and return the content after it.
    fn strip_unordered_list_marker(trimmed: &str) -> Option<&str> {
        // Must be "- text", "* text", or "+ text"
        // Be careful: "---" is a horizontal rule, not a list item
        // "* " alone at start needs to not conflict with bold/italic markers at line level
        for marker in &["- ", "* ", "+ "] {
            if let Some(rest) = trimmed.strip_prefix(marker) {
                return Some(rest);
            }
        }
        None
    }

    /// Strip an ordered list marker (N. ) and return (number_str, content).
    fn strip_ordered_list_marker(trimmed: &str) -> Option<(&str, &str)> {
        // Match pattern: one or more digits, then '. ', then content
        let dot_pos = trimmed.find(". ")?;
        let num_part = &trimmed[..dot_pos];
        if !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit()) {
            Some((num_part, &trimmed[dot_pos + 2..]))
        } else {
            None
        }
    }

    /// Extract leading whitespace from a line.
    fn leading_whitespace(line: &str) -> &str {
        let trimmed_len = line.trim_start().len();
        &line[..line.len() - trimmed_len]
    }

    /// Apply inline formatting (bold, italic, inline code) to a line of normal text.
    fn render_inline(&self, line: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            // Check for bold italic: ***text***
            if i + 2 < len && chars[i] == '*' && chars[i + 1] == '*' && chars[i + 2] == '*' {
                if let Some(close) = Self::find_triple_star(&chars, i + 3) {
                    let inner: String = chars[i + 3..close].iter().collect();
                    result.push_str(&format!("{BOLD_ITALIC}{inner}{RESET}"));
                    i = close + 3;
                    continue;
                }
            }

            // Check for bold: **text**
            if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
                // Find closing **
                if let Some(close) = Self::find_double_star(&chars, i + 2) {
                    let inner: String = chars[i + 2..close].iter().collect();
                    result.push_str(&format!("{BOLD}{inner}{RESET}"));
                    i = close + 2;
                    continue;
                }
            }

            // Check for italic: *text* (single star, not followed by another star)
            if chars[i] == '*' && (i + 1 >= len || chars[i + 1] != '*') {
                if let Some(close) = Self::find_single_star(&chars, i + 1) {
                    // Must have at least one char between markers
                    if close > i + 1 {
                        let inner: String = chars[i + 1..close].iter().collect();
                        result.push_str(&format!("{ITALIC}{inner}{RESET}"));
                        i = close + 1;
                        continue;
                    }
                }
            }

            // Check for inline code: `text`
            if chars[i] == '`' {
                // Find closing backtick (not another opening fence)
                if let Some(close) = Self::find_backtick(&chars, i + 1) {
                    let inner: String = chars[i + 1..close].iter().collect();
                    result.push_str(&format!("{CYAN}{inner}{RESET}"));
                    i = close + 1;
                    continue;
                }
            }

            result.push(chars[i]);
            i += 1;
        }

        result
    }

    /// Find closing *** starting from position `from` in char slice.
    fn find_triple_star(chars: &[char], from: usize) -> Option<usize> {
        let len = chars.len();
        let mut j = from;
        while j + 2 < len {
            if chars[j] == '*' && chars[j + 1] == '*' && chars[j + 2] == '*' {
                return Some(j);
            }
            j += 1;
        }
        None
    }

    /// Find closing ** starting from position `from` in char slice.
    fn find_double_star(chars: &[char], from: usize) -> Option<usize> {
        let len = chars.len();
        let mut j = from;
        while j + 1 < len {
            if chars[j] == '*' && chars[j + 1] == '*' {
                return Some(j);
            }
            j += 1;
        }
        None
    }

    /// Find closing single * starting from position `from` in char slice.
    /// The closing * must NOT be followed by another * (to avoid matching inside **).
    fn find_single_star(chars: &[char], from: usize) -> Option<usize> {
        let len = chars.len();
        for j in from..len {
            if chars[j] == '*' {
                // Make sure it's not part of a ** sequence
                if j + 1 < len && chars[j + 1] == '*' {
                    continue;
                }
                // Also make sure the preceding char isn't * (closing side of **)
                if j > from && chars[j - 1] == '*' {
                    continue;
                }
                return Some(j);
            }
        }
        None
    }

    /// Find closing backtick starting from position `from` in char slice.
    fn find_backtick(chars: &[char], from: usize) -> Option<usize> {
        (from..chars.len()).find(|&j| chars[j] == '`')
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md_code_block_detection() {
        let input = "before\n```\ncode line\n```\nafter\n";
        let out = render_full(input);
        // "code line" should be wrapped in DIM
        assert!(out.contains(&format!("{DIM}code line{RESET}")));
        // "before" and "after" should NOT be dim
        assert!(out.contains("before"));
        assert!(out.contains("after"));
    }

    #[test]
    fn test_md_code_block_with_language() {
        let input = "```rust\nlet x = 1;\n```\n";
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta(input);
        let flushed = r.flush();
        let full = format!("{out}{flushed}");
        // Language should be captured and fence dimmed
        assert!(full.contains(&format!("{DIM}```rust{RESET}")));
        // "let" should be keyword-highlighted, not just DIM
        assert!(full.contains(&format!("{BOLD_CYAN}let{RESET}")));
        // Number should be yellow
        assert!(full.contains(&format!("{YELLOW}1{RESET}")));
    }

    #[test]
    fn test_md_inline_code() {
        let out = render_full("use `Option<T>` here\n");
        assert!(out.contains(&format!("{CYAN}Option<T>{RESET}")));
    }

    #[test]
    fn test_md_bold_text() {
        let out = render_full("this is **important** stuff\n");
        assert!(out.contains(&format!("{BOLD}important{RESET}")));
    }

    #[test]
    fn test_md_header_rendering() {
        let out = render_full("# Hello World\n");
        assert!(out.contains(&format!("{BOLD}{CYAN}# Hello World{RESET}")));
    }

    #[test]
    fn test_md_header_h2() {
        let out = render_full("## Section Two\n");
        assert!(out.contains(&format!("{BOLD}{CYAN}## Section Two{RESET}")));
    }

    #[test]
    fn test_md_partial_delta_fence() {
        // Fence marker split across multiple deltas
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("``");
        // Nothing emitted yet — still buffered (no newline)
        assert_eq!(out1, "");
        let out2 = r.render_delta("`\n");
        // Now the fence line is complete
        assert!(out2.contains(&format!("{DIM}```{RESET}")));
        let out3 = r.render_delta("code here\n");
        assert!(out3.contains(&format!("{DIM}code here{RESET}")));
        let out4 = r.render_delta("```\n");
        assert!(out4.contains(&format!("{DIM}```{RESET}")));
        // After closing, normal text again
        let out5 = r.render_delta("normal\n");
        assert!(out5.contains("normal"));
        assert!(!out5.contains(&format!("{DIM}")));
    }

    #[test]
    fn test_md_empty_delta() {
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("");
        assert_eq!(out, "");
        let flushed = r.flush();
        assert_eq!(flushed, "");
    }

    #[test]
    fn test_md_multiple_code_blocks() {
        let input = "text\n```\nblock1\n```\nmiddle\n```python\nblock2\n```\nend\n";
        let out = render_full(input);
        // Untagged code block → DIM fallback
        assert!(out.contains(&format!("{DIM}block1{RESET}")));
        assert!(out.contains("middle"));
        // Python-tagged code block → syntax highlighted (no keyword match, plain output)
        assert!(out.contains("block2"));
        assert!(out.contains("end"));
    }

    #[test]
    fn test_md_inline_code_inside_bold() {
        // Inline code backticks inside bold — bold wraps, code is separate
        let out = render_full("**bold** and `code`\n");
        assert!(out.contains(&format!("{BOLD}bold{RESET}")));
        assert!(out.contains(&format!("{CYAN}code{RESET}")));
    }

    #[test]
    fn test_md_unmatched_backtick() {
        // Single backtick without closing — should pass through literally
        let out = render_full("it's a `partial\n");
        assert!(out.contains('`'));
        assert!(out.contains("partial"));
    }

    #[test]
    fn test_md_unmatched_bold() {
        // Unmatched ** should pass through literally
        let out = render_full("star **power\n");
        assert!(out.contains("**"));
        assert!(out.contains("power"));
    }

    #[test]
    fn test_md_flush_partial_line() {
        let mut r = MarkdownRenderer::new();
        // "no" at line start — can't be fence/header, resolves immediately
        let out = r.render_delta("no");
        assert!(
            out.contains("no"),
            "Short non-fence/non-header text resolves immediately"
        );
        // Continue adding text — mid-line now, immediate output
        let out2 = r.render_delta(" newline here");
        assert!(out2.contains(" newline here"));
    }

    #[test]
    fn test_md_flush_with_inline_formatting() {
        let mut r = MarkdownRenderer::new();
        // "hello **world**" — resolves as non-fence at line start, then renders inline
        let out = r.render_delta("hello **world**");
        let flushed = r.flush();
        let total = format!("{out}{flushed}");
        assert!(total.contains(&format!("{BOLD}world{RESET}")));
    }

    #[test]
    fn test_md_default_trait() {
        let r = MarkdownRenderer::default();
        assert!(!r.in_code_block);
        assert!(r.code_lang.is_none());
        assert!(r.line_buffer.is_empty());
        assert!(r.line_start);
        assert!(!r.block_prefix_rendered);
    }

    // --- Streaming output tests (mid-line tokens should render immediately) ---

    #[test]
    fn test_md_streaming_mid_line_immediate_output() {
        // Simulate LLM streaming: first token starts a line, subsequent tokens mid-line
        let mut r = MarkdownRenderer::new();
        // First token: "Hello " — at line start, long enough to resolve as normal text
        let out1 = r.render_delta("Hello ");
        // Should produce output (6 chars, clearly not a fence or header)
        assert!(
            out1.contains("Hello "),
            "Expected immediate output for non-fence/non-header text, got: '{out1}'"
        );

        // Second token: "world" — mid-line, should be immediate
        let out2 = r.render_delta("world");
        assert!(
            out2.contains("world"),
            "Mid-line delta should produce immediate output, got: '{out2}'"
        );

        // Third token: " how" — still mid-line
        let out3 = r.render_delta(" how");
        assert!(
            out3.contains(" how"),
            "Mid-line delta should produce immediate output, got: '{out3}'"
        );
    }

    #[test]
    fn test_md_streaming_newline_resets_to_line_start() {
        let mut r = MarkdownRenderer::new();
        // Start with text that resolves line start
        let _ = r.render_delta("Hello world");
        // Now a newline — next delta should be at line start again
        let _ = r.render_delta("\n");
        // Short text at start of new line — should buffer briefly
        let out = r.render_delta("``");
        // Two backticks could be start of a fence — should buffer
        assert_eq!(
            out, "",
            "Short ambiguous text at line start should be buffered"
        );
    }

    #[test]
    fn test_md_streaming_code_fence_detected_at_line_start() {
        let mut r = MarkdownRenderer::new();
        // Send a code fence at line start
        let out1 = r.render_delta("```\n");
        assert!(out1.contains(&format!("{DIM}```{RESET}")));
        assert!(r.in_code_block);

        // Content inside code block
        let out2 = r.render_delta("some code\n");
        assert!(out2.contains(&format!("{DIM}some code{RESET}")));

        // Closing fence
        let out3 = r.render_delta("```\n");
        assert!(out3.contains(&format!("{DIM}```{RESET}")));
        assert!(!r.in_code_block);
    }

    #[test]
    fn test_md_streaming_header_detected_at_line_start() {
        let mut r = MarkdownRenderer::new();
        // Header at line start
        let out = r.render_delta("# My Header\n");
        assert!(out.contains(&format!("{BOLD}{CYAN}# My Header{RESET}")));
    }

    #[test]
    fn test_md_streaming_bold_mid_line() {
        let mut r = MarkdownRenderer::new();
        // Start a line with enough text to resolve
        let out1 = r.render_delta("This is ");
        assert!(out1.contains("This is "));
        // Now bold text mid-line
        let out2 = r.render_delta("**important**");
        assert!(
            out2.contains(&format!("{BOLD}important{RESET}")),
            "Bold formatting should work in mid-line streaming, got: '{out2}'"
        );
    }

    #[test]
    fn test_md_streaming_inline_code_mid_line() {
        let mut r = MarkdownRenderer::new();
        // Start a line
        let out1 = r.render_delta("Use the ");
        assert!(out1.contains("Use the "));
        // Inline code mid-line
        let out2 = r.render_delta("`Option`");
        assert!(
            out2.contains(&format!("{CYAN}Option{RESET}")),
            "Inline code should work in mid-line streaming, got: '{out2}'"
        );
    }

    #[test]
    fn test_md_streaming_word_by_word_paragraph() {
        // Simulate typical LLM streaming: word by word
        let mut r = MarkdownRenderer::new();
        let words = ["The ", "quick ", "brown ", "fox ", "jumps"];
        let mut got_output = false;
        for word in &words[..] {
            let out = r.render_delta(word);
            if !out.is_empty() {
                got_output = true;
            }
        }
        // We should have gotten SOME output before the line ends
        assert!(
            got_output,
            "Word-by-word streaming should produce output before newline"
        );

        // Flush remainder
        let _flushed = r.flush();
        // Total output should contain all words
        let mut total = String::new();
        let mut r2 = MarkdownRenderer::new();
        for word in &words[..] {
            total.push_str(&r2.render_delta(word));
        }
        total.push_str(&r2.flush());
        assert!(total.contains("The "));
        assert!(total.contains("fox "));
    }

    #[test]
    fn test_md_streaming_line_start_buffer_short_text() {
        // At line start, very short text (1-3 chars) that could be start of fence/header
        // should be buffered
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("#");
        // Single '#' could be a header — should buffer
        assert_eq!(out, "", "Single '#' at line start should be buffered");

        // Now add more to reveal it's a header
        let out2 = r.render_delta(" Title\n");
        assert!(out2.contains(&format!("{BOLD}{CYAN}# Title{RESET}")));
    }

    #[test]
    fn test_md_streaming_line_start_resolves_normal() {
        // At line start, text that quickly resolves as not a fence/header
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("Normal text");
        // "Normal" is 11 chars, clearly not a fence or header — should output
        assert!(
            out.contains("Normal text"),
            "Non-fence/non-header text should be output once resolved, got: '{out}'"
        );
    }

    #[test]
    fn test_md_streaming_existing_tests_still_pass() {
        // Ensure the full-line render_full helper still works exactly as before
        let out = render_full("Hello **world** and `code`\n");
        assert!(out.contains("Hello "));
        assert!(out.contains(&format!("{BOLD}world{RESET}")));
        assert!(out.contains(&format!("{CYAN}code{RESET}")));
    }

    #[test]
    fn test_md_streaming_in_code_block_immediate() {
        // Inside a code block, tokens should stream immediately once fence is ruled out.
        // "let x" can't be a closing fence (doesn't start with `), so it should
        // be early-resolved and emitted without needing flush().
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```rust\n");
        assert!(r.in_code_block);
        // Send code token — not a fence, should be emitted immediately
        let out = r.render_delta("let x");
        assert!(
            !out.is_empty(),
            "Code block content that can't be a fence should emit immediately, got empty"
        );
        assert!(
            out.contains("let"),
            "Code block content should contain the text, got: '{out}'"
        );
    }

    #[test]
    fn test_md_code_block_mid_line_emitted_immediately() {
        // Issue #147: Mid-line code block content should be emitted token-by-token,
        // not buffered until a newline arrives.
        let mut r = MarkdownRenderer::new();
        // Open a code block
        let _ = r.render_delta("```\n");
        assert!(r.in_code_block);

        // Send a line start token that gets buffered (could be closing fence)
        // Then a complete line to move past line_start
        let _ = r.render_delta("let x = 1;\n");

        // Now send a mid-line token — should be emitted immediately, not empty
        let out = r.render_delta("println");
        assert!(
            !out.is_empty(),
            "Mid-line code block token should be emitted immediately, got empty string"
        );
        assert!(
            out.contains("println"),
            "Mid-line code block token should contain the text, got: '{out}'"
        );
    }

    #[test]
    fn test_md_code_block_mid_line_with_newline() {
        // When a newline arrives mid-line in a code block, it should transition to line_start
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```\n");
        let _ = r.render_delta("first line\n");

        // Send mid-line token followed by newline
        let out = r.render_delta("hello\n");
        assert!(
            out.contains("hello"),
            "Code block content before newline should be rendered, got: '{out}'"
        );
        // After the newline, we should be at line_start again
        assert!(
            r.line_start,
            "After newline in code block, should be at line_start"
        );
    }

    #[test]
    fn test_md_code_block_fence_detection_still_works() {
        // Closing fence detection must still work even with the mid-line fast path
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```rust\n");
        assert!(r.in_code_block);

        let _ = r.render_delta("let x = 42;\n");
        assert!(r.in_code_block);

        // Closing fence at line start — must be detected (not short-circuited)
        let _ = r.render_delta("```\n");
        assert!(
            !r.in_code_block,
            "Closing fence should still be detected and end the code block"
        );
    }

    #[test]
    fn test_md_code_block_mid_line_multiple_tokens() {
        // Multiple mid-line tokens in a code block should each produce output
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```\n");
        let _ = r.render_delta("start\n");

        let out1 = r.render_delta("foo");
        assert!(
            !out1.is_empty(),
            "First mid-line token should emit, got empty"
        );

        let out2 = r.render_delta("bar");
        assert!(
            !out2.is_empty(),
            "Second mid-line token should emit, got empty"
        );

        let out3 = r.render_delta(" baz");
        assert!(
            !out3.is_empty(),
            "Third mid-line token should emit, got empty"
        );
    }

    #[test]
    fn test_md_streaming_single_token_produces_output() {
        // Issue #137: Common single-token inputs should produce non-empty output
        // when used mid-line. At line start, short tokens that can't be fences/headers
        // should also flush immediately.
        let test_cases = vec![
            // (token, description)
            ("Hello", "common greeting"),
            ("I", "single letter word"),
            (" will", "space-prefixed verb"),
            ("The", "article"),
            ("Sure", "affirmative"),
            ("Let", "common start word"),
            ("Yes", "short response"),
            ("To", "preposition"),
        ];

        for (token, desc) in &test_cases {
            // Test mid-line: should always produce output immediately
            let mut r = MarkdownRenderer::new();
            // First, get past line-start by sending a resolved line-start token
            let _ = r.render_delta("Start ");
            let out = r.render_delta(token);
            assert!(
                !out.is_empty(),
                "Mid-line token '{token}' ({desc}) should produce non-empty output, got empty"
            );
        }

        // Test at line start: tokens that can't be fences (``) or headers (#)
        // should flush immediately even if short
        let line_start_cases = vec![
            ("Hello", "common greeting"),
            ("I", "single letter I"),
            ("Sure", "affirmative"),
            ("The", "article"),
            ("Yes", "short response"),
        ];

        for (token, desc) in &line_start_cases {
            let mut r = MarkdownRenderer::new();
            let out = r.render_delta(token);
            assert!(
                !out.is_empty(),
                "Line-start token '{token}' ({desc}) that can't be fence/header should produce output, got empty"
            );
        }
    }

    #[test]
    fn test_md_streaming_single_char_non_special_at_line_start() {
        // Single characters that are NOT '#' or '`' should flush immediately
        // at line start, since they can't possibly be fences or headers
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("I");
        assert!(
            !out.is_empty(),
            "'I' at line start cannot be fence or header, should flush immediately"
        );
    }

    #[test]
    fn test_md_streaming_space_prefixed_token_at_line_start() {
        // " will" — space-prefixed, trimmed = "will" (4 chars), not fence/header
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta(" will");
        assert!(
            !out.is_empty(),
            "' will' at line start should resolve — trimmed 'will' is 4 chars, not fence/header"
        );
    }

    // --- Streaming latency: block elements should flush content after prefix ---

    #[test]
    fn test_md_streaming_list_item_content_not_buffered() {
        // List items should NOT buffer all content until newline.
        // Once we see "- " we know it's a list item — subsequent tokens
        // should stream immediately.
        let mut r = MarkdownRenderer::new();
        // Send list marker
        let out1 = r.render_delta("- ");
        // The marker itself may or may not produce output yet (prefix detection)
        // but let's accumulate
        let mut total = out1;

        // Send content token — should produce output immediately
        let out2 = r.render_delta("Hello");
        total.push_str(&out2);
        assert!(
            !out2.is_empty(),
            "List item content after '- ' should stream immediately, got empty"
        );

        // Another content token
        let out3 = r.render_delta(" world");
        total.push_str(&out3);
        assert!(
            !out3.is_empty(),
            "Additional list item tokens should stream immediately, got empty"
        );
    }

    #[test]
    fn test_md_streaming_blockquote_content_not_buffered() {
        // Blockquote content after "> " should stream immediately.
        let mut r = MarkdownRenderer::new();
        let _out1 = r.render_delta("> ");

        let out2 = r.render_delta("Some quoted");
        assert!(
            !out2.is_empty(),
            "Blockquote content after '> ' should stream immediately, got empty"
        );

        let out3 = r.render_delta(" text");
        assert!(
            !out3.is_empty(),
            "Additional blockquote tokens should stream immediately, got empty"
        );
    }

    #[test]
    fn test_md_streaming_header_content_still_buffers() {
        // Headers need to buffer until newline because the entire line
        // gets BOLD+CYAN styling. But "#" alone should buffer.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("#");
        assert_eq!(out, "", "Single '#' should buffer (could be header)");
    }

    #[test]
    fn test_md_streaming_code_fence_opener_still_buffers() {
        // Code fence openers must buffer until complete so we detect the fence.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("``");
        assert_eq!(out, "", "Partial fence '``' should buffer");

        let out2 = r.render_delta("`");
        // Still buffering (no newline yet, could be ```lang)
        // The fence might be detected only on \n
        assert_eq!(
            out2, "",
            "Complete fence '```' without newline should buffer"
        );
    }

    #[test]
    fn test_md_streaming_inline_formatting_on_partial_lines() {
        // Bold/italic/code formatting should work on partial lines (flushed mid-line)
        let mut r = MarkdownRenderer::new();
        // Start with resolved text
        let _ = r.render_delta("Check ");
        // Send bold text mid-line
        let out = r.render_delta("**this**");
        assert!(
            out.contains(&format!("{BOLD}this{RESET}")),
            "Bold formatting should work on mid-line partial text, got: '{out}'"
        );
    }

    #[test]
    fn test_md_streaming_list_renders_correctly_on_newline() {
        // Even with early flushing, the full list item should render correctly
        // when the newline arrives.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("- ");
        let out2 = r.render_delta("item text");
        let out3 = r.render_delta("\n");
        let flushed = r.flush();
        let total = format!("{out1}{out2}{out3}{flushed}");
        // Should contain the bullet character from list rendering
        assert!(
            total.contains("item text"),
            "List item text should appear in output, got: '{total}'"
        );
    }

    #[test]
    fn test_md_streaming_ordered_list_content_not_buffered() {
        // Ordered list: "1. " detected, subsequent content should stream
        let mut r = MarkdownRenderer::new();
        let _out1 = r.render_delta("1. ");

        let out2 = r.render_delta("First item");
        assert!(
            !out2.is_empty(),
            "Ordered list content after '1. ' should stream immediately, got empty"
        );
    }

    #[test]
    fn test_md_streaming_no_regression_full_render() {
        // Full render should still produce correct output for all line types
        let out = render_full("- list item\n> quoted\n1. ordered\n# header\nplain\n");
        assert!(
            out.contains("list item"),
            "List item missing from full render"
        );
        assert!(
            out.contains("quoted"),
            "Blockquote missing from full render"
        );
        assert!(
            out.contains("ordered"),
            "Ordered list missing from full render"
        );
        assert!(out.contains("header"), "Header missing from full render");
        assert!(out.contains("plain"), "Plain text missing from full render");
    }

    #[test]
    fn test_md_plain_text_unchanged() {
        let out = render_full("just plain text\n");
        assert!(out.contains("just plain text"));
    }

    #[test]
    fn test_md_multiple_inline_codes_one_line() {
        let out = render_full("use `foo` and `bar` here\n");
        assert!(out.contains(&format!("{CYAN}foo{RESET}")));
        assert!(out.contains(&format!("{CYAN}bar{RESET}")));
    }

    #[test]
    fn test_md_code_block_preserves_content() {
        let input = "```\nfn main() {\n    println!(\"hello\");\n}\n```\n";
        let out = render_full(input);
        assert!(out.contains("fn main()"));
        assert!(out.contains("println!"));
    }

    // --- Markdown rendering: italic, lists, horizontal rules, blockquotes ---

    #[test]
    fn test_md_italic_text() {
        let out = render_full("this is *italic* text\n");
        assert!(
            out.contains(&format!("{ITALIC}italic{RESET}")),
            "Expected italic ANSI for *italic*, got: '{out}'"
        );
    }

    #[test]
    fn test_md_bold_still_works() {
        // Regression: bold must not break after adding italic support
        let out = render_full("this is **bold** text\n");
        assert!(
            out.contains(&format!("{BOLD}bold{RESET}")),
            "Expected bold ANSI for **bold**, got: '{out}'"
        );
    }

    #[test]
    fn test_md_bold_italic_text() {
        let out = render_full("this is ***both*** here\n");
        assert!(
            out.contains(&format!("{BOLD_ITALIC}both{RESET}")),
            "Expected bold+italic ANSI for ***both***, got: '{out}'"
        );
    }

    #[test]
    fn test_md_mixed_inline_formatting() {
        let out = render_full("**bold** and *italic* and `code`\n");
        assert!(
            out.contains(&format!("{BOLD}bold{RESET}")),
            "Missing bold in mixed line, got: '{out}'"
        );
        assert!(
            out.contains(&format!("{ITALIC}italic{RESET}")),
            "Missing italic in mixed line, got: '{out}'"
        );
        assert!(
            out.contains(&format!("{CYAN}code{RESET}")),
            "Missing code in mixed line, got: '{out}'"
        );
    }

    #[test]
    fn test_md_unclosed_italic_no_format() {
        // A single * at end of line without closing should NOT italicize
        let out = render_full("star *power\n");
        assert!(
            out.contains('*'),
            "Unclosed italic marker should pass through literally, got: '{out}'"
        );
        assert!(out.contains("power"));
    }

    #[test]
    fn test_md_unordered_list_dash() {
        let out = render_full("- first item\n");
        assert!(
            out.contains(&format!("{CYAN}•{RESET}")),
            "Expected colored bullet for '- item', got: '{out}'"
        );
        assert!(out.contains("first item"));
    }

    #[test]
    fn test_md_unordered_list_star() {
        let out = render_full("* second item\n");
        assert!(
            out.contains(&format!("{CYAN}•{RESET}")),
            "Expected colored bullet for '* item', got: '{out}'"
        );
        assert!(out.contains("second item"));
    }

    #[test]
    fn test_md_unordered_list_plus() {
        let out = render_full("+ third item\n");
        assert!(
            out.contains(&format!("{CYAN}•{RESET}")),
            "Expected colored bullet for '+ item', got: '{out}'"
        );
        assert!(out.contains("third item"));
    }

    #[test]
    fn test_md_unordered_list_with_inline_formatting() {
        let out = render_full("- a **bold** list item\n");
        assert!(out.contains(&format!("{CYAN}•{RESET}")));
        assert!(
            out.contains(&format!("{BOLD}bold{RESET}")),
            "List item content should get inline formatting, got: '{out}'"
        );
    }

    #[test]
    fn test_md_ordered_list() {
        let out = render_full("1. first\n");
        assert!(
            out.contains(&format!("{CYAN}1.{RESET}")),
            "Expected colored number for '1. first', got: '{out}'"
        );
        assert!(out.contains("first"));
    }

    #[test]
    fn test_md_ordered_list_larger_number() {
        let out = render_full("42. the answer\n");
        assert!(
            out.contains(&format!("{CYAN}42.{RESET}")),
            "Expected colored number for '42. item', got: '{out}'"
        );
        assert!(out.contains("the answer"));
    }

    #[test]
    fn test_md_horizontal_rule_dashes() {
        let out = render_full("---\n");
        assert!(
            out.contains("─"),
            "Expected horizontal rule rendering for '---', got: '{out}'"
        );
        assert!(
            out.contains(&format!("{DIM}")),
            "Horizontal rule should be dim, got: '{out}'"
        );
    }

    #[test]
    fn test_md_horizontal_rule_stars() {
        let out = render_full("***\n");
        assert!(
            out.contains("─"),
            "Expected horizontal rule rendering for '***', got: '{out}'"
        );
    }

    #[test]
    fn test_md_horizontal_rule_underscores() {
        let out = render_full("___\n");
        assert!(
            out.contains("─"),
            "Expected horizontal rule rendering for '___', got: '{out}'"
        );
    }

    #[test]
    fn test_md_horizontal_rule_long() {
        let out = render_full("----------\n");
        assert!(
            out.contains("─"),
            "Expected horizontal rule for long dashes, got: '{out}'"
        );
    }

    #[test]
    fn test_md_blockquote() {
        let out = render_full("> quoted text\n");
        assert!(
            out.contains(&format!("{DIM}│{RESET}")),
            "Expected dim vertical bar for blockquote, got: '{out}'"
        );
        assert!(
            out.contains(&format!("{ITALIC}quoted text{RESET}")),
            "Blockquote content should be italic, got: '{out}'"
        );
    }

    #[test]
    fn test_md_blockquote_with_inline_formatting() {
        let out = render_full("> a **bold** quote\n");
        assert!(out.contains(&format!("{DIM}│{RESET}")));
        // The content goes through render_inline, which processes bold inside the italic context
        assert!(out.contains("bold"));
    }

    #[test]
    fn test_md_indented_list_item() {
        let out = render_full("  - nested item\n");
        assert!(
            out.contains(&format!("{CYAN}•{RESET}")),
            "Indented list item should still get bullet, got: '{out}'"
        );
        assert!(out.contains("nested item"));
    }

    #[test]
    fn test_md_not_a_list_in_code_block() {
        // Inside code blocks, list markers should NOT be rendered as bullets
        let out = render_full("```\n- not a list\n```\n");
        assert!(
            !out.contains(&format!("{CYAN}•{RESET}")),
            "List markers inside code blocks should not get bullets, got: '{out}'"
        );
    }

    // --- Syntax highlighting tests ---

    #[test]
    fn test_md_code_block_indented_line_resolves_immediately() {
        // Indented code lines like "    let x = 1;" should resolve at line start
        // without waiting for more tokens — a closing fence never has leading spaces
        // before the backticks (in CommonMark, ≤3 spaces are allowed, but the first
        // non-space char must be `\``). Content starting with spaces followed by a
        // non-backtick char should early-resolve.
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```rust\n");
        assert!(r.in_code_block);

        // Indented code at line start — should resolve immediately
        let out = r.render_delta("    let x");
        assert!(
            !out.is_empty(),
            "Indented code block content should resolve immediately at line start, got empty"
        );
        assert!(
            out.contains("let x"),
            "Should contain the code text, got: '{out}'"
        );
    }

    #[test]
    fn test_md_code_block_space_only_token_buffers() {
        // A token that is only whitespace at code block line start should buffer
        // because we don't yet know what follows
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```\n");
        assert!(r.in_code_block);

        // Just spaces — ambiguous, should buffer
        let out = r.render_delta("  ");
        // This may or may not emit — it's okay either way as long as
        // subsequent non-fence content resolves quickly
        let _ = out; // don't assert on whitespace-only

        // Follow-up with non-fence content should resolve
        let out2 = r.render_delta("code");
        assert!(
            !out2.is_empty(),
            "Content after whitespace should resolve, got empty"
        );
    }

    #[test]
    fn test_md_render_delta_every_call_produces_or_buffers_minimally() {
        // Simulate a realistic streaming sequence and verify tokens aren't
        // held longer than necessary. Each non-ambiguous mid-line token should
        // produce output on the same call.
        let mut r = MarkdownRenderer::new();
        // First token resolves line start
        let out1 = r.render_delta("Here is ");
        assert!(!out1.is_empty(), "First token should resolve");

        // Each subsequent mid-line token must produce output immediately
        let tokens = ["a ", "sentence ", "with ", "multiple ", "tokens."];
        for token in &tokens {
            let out = r.render_delta(token);
            assert!(
                !out.is_empty(),
                "Mid-line token '{token}' should produce immediate output"
            );
        }
    }

    #[test]
    fn test_md_flush_produces_output_for_buffered_content() {
        // flush() should emit any content still in the line buffer
        let mut r = MarkdownRenderer::new();
        // Send a partial line that gets buffered at line start
        let out = r.render_delta("#");
        assert_eq!(out, "", "# should buffer at line start");

        // flush() should emit the buffered content
        let flushed = r.flush();
        assert!(
            !flushed.is_empty(),
            "flush() should emit buffered '#' content"
        );
    }

    #[test]
    fn test_md_code_block_backtick_start_buffers_correctly() {
        // A token starting with ` at code block line start must buffer
        // (could be closing fence ```)
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```\n");
        let _ = r.render_delta("content\n");

        // Backtick at line start — could be closing fence
        let out = r.render_delta("`");
        assert_eq!(
            out, "",
            "Single backtick at code block line start should buffer"
        );

        // Complete the closing fence
        let out2 = r.render_delta("``\n");
        assert!(!r.in_code_block, "Should have closed the code block");
        assert!(!out2.is_empty(), "Closing fence should produce output");
    }

    // --- render_latency_budget: document the expected flush behavior ---
    //
    // The streaming pipeline has the following latency budget per text delta:
    //
    // 1. Spinner stop (first token only): ~0.1ms
    //    - Synchronous eprint!("\r\x1b[K") + stderr flush
    //    - Sends cancel signal to async spinner task
    //    - Aborts the spawned task handle
    //
    // 2. MarkdownRenderer::render_delta(): ~0 allocation for mid-line tokens
    //    - Mid-line fast path: no buffering, immediate String return
    //    - Line-start: buffers 1-4 chars for fence/header detection
    //    - Code block line-start: buffers until first non-backtick char
    //
    // 3. print!() + io::stdout().flush(): system call, ~0.01ms
    //    - Called after every render_delta that produces output
    //    - Ensures tokens are visible immediately, not batched by stdio
    //
    // Total per-token latency: <0.2ms for first token, <0.05ms for subsequent
    // The bottleneck is always the network/API, not the renderer.

    #[test]
    fn test_md_render_delta_latency_budget_mid_line() {
        // Verify the mid-line fast path produces output without allocating
        // a line buffer — this is the hot path for streaming latency.
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("Start ");
        assert!(!r.line_start, "Should be mid-line after first token");

        // Mid-line token should not touch line_buffer
        let out = r.render_delta("word");
        assert!(!out.is_empty(), "Mid-line should produce output");
        assert!(
            r.line_buffer.is_empty(),
            "Mid-line fast path should not use line_buffer"
        );
    }

    // --- Live tool progress formatting tests ---
}
