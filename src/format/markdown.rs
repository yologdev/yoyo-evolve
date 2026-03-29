//! MarkdownRenderer for streaming markdown output with ANSI formatting.

use super::*;

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
    /// - At line start with word boundary (text + trailing space), flushes via
    ///   `flush_on_whitespace()` for word-by-word prose streaming
    /// - Mid-line, renders immediately with inline formatting (bold, inline code)
    /// - Complete lines (ending with `\n`) are always processed immediately
    ///
    /// ## render_latency_budget
    ///
    /// The renderer is designed for minimal token-to-display latency:
    ///
    /// | Path                    | Buffering             | Expected latency |
    /// |-------------------------|-----------------------|------------------|
    /// | Mid-line text           | None (immediate)      | ~0 (no alloc)    |
    /// | Mid-line code block     | None (immediate)      | ~0 (dim wrap)    |
    /// | Line-start, non-special | Flush after 1 char    | ~0               |
    /// | Line-start, word boundary | Flush on whitespace | ~1 token         |
    /// | Line-start, ambiguous   | Buffer 1–4 chars      | 1 token          |
    /// | Line-start, code block  | Buffer until non-`\`` | 1 token          |
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
                let prefix_output = self.try_resolve_block_prefix();
                if !prefix_output.is_empty() {
                    output.push_str(&prefix_output);
                } else {
                    // Still ambiguous from needs_line_buffering(), but if we've
                    // accumulated a word boundary (text + trailing whitespace), the
                    // content can't be a fence/header prefix — flush it now.
                    // This gives word-by-word streaming for prose that starts with
                    // characters that trigger buffering (e.g., digits, dashes).
                    output.push_str(&self.flush_on_whitespace());
                }
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
                // Quick disambiguation: "-" followed by a non-space, non-dash char
                // can't be a list item ("- ") or horizontal rule ("---").
                // "-based", "-flag" → flush immediately. "- item", "--" → keep buffering.
                if trimmed.len() >= 2 {
                    let second = trimmed.as_bytes()[1];
                    if second != b' ' && second != b'-' {
                        return false;
                    }
                }
                trimmed.len() < 2 || trimmed.starts_with("- ") || {
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
                // Quick disambiguation: if we have at least 2 chars and the first
                // non-digit char isn't '.' or ')', it can't be a numbered list —
                // flush immediately. "2nd", "3rd", "100ms" → flush.
                // "1.", "1)", "12" (all digits), "12." → keep buffering.
                if trimmed.len() >= 2 {
                    if let Some(pos) = trimmed.bytes().position(|b| !b.is_ascii_digit()) {
                        let non_digit = trimmed.as_bytes()[pos];
                        if non_digit != b'.' && non_digit != b')' {
                            return false; // Not a numbered list pattern
                        }
                        // We have digits followed by '.' or ')'.
                        // Keep buffering until we see what follows the separator.
                        // "1." "12." "1)" → buffer (next char could be space → list)
                        // "1. " "12. " → buffer (confirmed list pattern, resolve in prefix)
                        // "1.x" "12.x" → flush (not a list — char after dot isn't space)
                        let after_sep = pos + 1;
                        if after_sep >= trimmed.len() {
                            return true; // Haven't seen char after separator yet
                        }
                        let next = trimmed.as_bytes()[after_sep];
                        if next == b' ' {
                            return true; // "12. " pattern — list item, keep buffering
                        }
                        return false; // "12.x" — not a list
                    }
                    // All digits so far, keep buffering
                }
                trimmed.len() < 3
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

    /// Flush the line buffer when it contains a word boundary (whitespace after text).
    ///
    /// This improves perceived streaming performance: when the buffer has accumulated
    /// something like `"The "` or `"Hello world "`, the trailing whitespace proves it
    /// can't be a fence/header prefix (those never have spaces after the control chars
    /// without first being resolved by `try_resolve_block_prefix`). So we flush the
    /// buffer as inline text and switch to the mid-line fast path.
    ///
    /// **Safety:** Does NOT flush when the trimmed buffer starts with `#` or `` ` ``
    /// (potential header/fence), or with block-level markers (`>`, `-`, `*`, `+`,
    /// digits) — those are handled by `needs_line_buffering`/`try_resolve_block_prefix`.
    ///
    /// Returns rendered output if flushed, empty string otherwise.
    pub fn flush_on_whitespace(&mut self) -> String {
        if !self.line_start || self.line_buffer.is_empty() || self.in_code_block {
            return String::new();
        }

        // Check if the buffer ends with whitespace and has non-whitespace content.
        let has_non_ws = self.line_buffer.chars().any(|c| !c.is_whitespace());
        let ends_with_ws = self
            .line_buffer
            .chars()
            .last()
            .map(|c| c.is_whitespace())
            .unwrap_or(false);

        if !has_non_ws || !ends_with_ws {
            return String::new();
        }

        // Don't flush if the content could still be a markdown control sequence.
        // Headers (#), fences (`), block elements (>, -, *, +, digits) need to
        // keep buffering — they're handled by the dedicated resolution paths.
        let trimmed = self.line_buffer.trim_start();
        if !trimmed.is_empty() {
            let first = trimmed.as_bytes()[0];
            match first {
                b'#' | b'`' | b'>' | b'-' | b'*' | b'+' | b'_' | b'|' => return String::new(),
                b'0'..=b'9' => return String::new(),
                _ => {}
            }
        }

        let buf = std::mem::take(&mut self.line_buffer);
        let output = self.render_inline(&buf);
        self.line_start = false;
        output
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

// --- Waiting spinner for AI responses ---

/// Braille spinner frames used for the "thinking" animation.
#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: render a full string through the renderer (not streamed).
    fn render_full(input: &str) -> String {
        let mut r = MarkdownRenderer::new();
        let mut out = r.render_delta(input);
        out.push_str(&r.flush());
        out
    }

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

    // --- flush_on_whitespace tests ---

    #[test]
    fn test_md_flush_on_whitespace_at_line_start() {
        // When buffer accumulates "word " at line start, the trailing space
        // proves it's not a fence/header — flush_on_whitespace should emit it.
        let mut r = MarkdownRenderer::new();
        // Simulate a token that ends with whitespace at line start
        // "1 " could look like the start of an ordered list ("1. "), but
        // the space without a dot means it's just text with a trailing space.
        // However, needs_line_buffering might still hold it. Let's use a
        // clearer case: a digit followed by space that needs_line_buffering holds.
        let out = r.flush_on_whitespace();
        assert_eq!(out, "", "Empty buffer should not flush");
    }

    #[test]
    fn test_md_flush_on_whitespace_with_word_boundary() {
        // Direct test of flush_on_whitespace with a buffer that has
        // non-special content ending in whitespace.
        let mut r = MarkdownRenderer::new();
        r.line_buffer = "Hello ".to_string();
        r.line_start = true;
        let out = r.flush_on_whitespace();
        assert!(
            out.contains("Hello"),
            "Buffer with word boundary should flush, got: '{out}'"
        );
        assert!(!r.line_start, "Should switch to mid-line after flush");
        assert!(
            r.line_buffer.is_empty(),
            "Buffer should be empty after flush"
        );
    }

    #[test]
    fn test_md_flush_on_whitespace_no_trailing_space() {
        let mut r = MarkdownRenderer::new();
        r.line_buffer = "Hello".to_string();
        r.line_start = true;
        let out = r.flush_on_whitespace();
        assert_eq!(
            out, "",
            "Buffer without trailing whitespace should not flush"
        );
    }

    #[test]
    fn test_md_flush_on_whitespace_only_whitespace() {
        let mut r = MarkdownRenderer::new();
        r.line_buffer = "   ".to_string();
        r.line_start = true;
        let out = r.flush_on_whitespace();
        assert_eq!(out, "", "Buffer with only whitespace should not flush");
    }

    #[test]
    fn test_md_flush_on_whitespace_not_at_line_start() {
        let mut r = MarkdownRenderer::new();
        r.line_buffer = "Hello ".to_string();
        r.line_start = false; // mid-line
        let out = r.flush_on_whitespace();
        assert_eq!(out, "", "Should not flush when not at line start");
    }

    #[test]
    fn test_md_flush_on_whitespace_in_code_block() {
        let mut r = MarkdownRenderer::new();
        r.line_buffer = "Hello ".to_string();
        r.line_start = true;
        r.in_code_block = true;
        let out = r.flush_on_whitespace();
        assert_eq!(out, "", "Should not flush inside code blocks");
    }

    #[test]
    fn test_md_streaming_whitespace_flush_integration() {
        // Full streaming simulation: tokens that arrive with trailing whitespace
        // at line start should flush via the whitespace path when the normal
        // needs_line_buffering check would hold them.
        let mut r = MarkdownRenderer::new();

        // "- " at line start triggers needs_line_buffering (could be list).
        // Then "not " arrives. The buffer is now "- not " which has a word
        // boundary. But try_resolve_block_prefix should handle "- not" as a
        // confirmed list item before flush_on_whitespace even fires.
        let out1 = r.render_delta("- ");
        let out2 = r.render_delta("not");
        let total = format!("{out1}{out2}");
        // Should have output — either from prefix resolution or whitespace flush
        assert!(
            total.contains("not") || !out2.is_empty(),
            "Content after list marker should stream, got out1='{out1}' out2='{out2}'"
        );
    }

    #[test]
    fn test_md_streaming_digit_with_space_stays_buffered() {
        // "3 " — starts with digit, needs_line_buffering holds it (could be "3. ").
        // flush_on_whitespace also guards against digits. So it stays buffered
        // until the content resolves. But adding more text ("items") makes
        // needs_line_buffering return false (contains ". " is false, len >= 3,
        // and it's not all digits followed by ". ").
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("3 ");
        // "3 " — buffered (digit start, flush_on_whitespace guards digits)
        // Actually, needs_line_buffering: trimmed="3 ", first byte is digit,
        // trimmed.len() >= 3? "3 " is 2 chars, so < 3, returns true (buffer).
        // Then try_resolve_block_prefix: digit, tries ordered list, no ". " found. Empty.
        // Then flush_on_whitespace: first byte is digit, guarded. Empty.
        // So out1 should be empty.

        let out2 = r.render_delta("items");
        // Buffer is now "3 items". needs_line_buffering: digit start, len >= 3,
        // contains ". "? No. So all(digit) on "3 items"[..?] — find(". ") returns None.
        // The match arm: trimmed.len() < 3 → false. trimmed.contains(". ") is false.
        // So the whole expression: false || false = false. needs_line_buffering returns false!
        // So it flushes as inline text.
        let total = format!("{out1}{out2}");
        assert!(
            total.contains("3") && total.contains("items"),
            "Digit-space-text should eventually produce output, got: '{total}'"
        );
    }

    #[test]
    fn test_md_flush_on_whitespace_each_token_produces_output() {
        // Simulate word-by-word streaming where each word ends with a space.
        // After the first word resolves the line start, subsequent words
        // should produce immediate output via the mid-line fast path.
        let mut r = MarkdownRenderer::new();
        let words = ["The ", "quick ", "brown ", "fox "];
        let mut outputs = Vec::new();
        for word in &words {
            outputs.push(r.render_delta(word));
        }
        // First word should produce output (resolves line start)
        assert!(
            !outputs[0].is_empty(),
            "First word 'The ' should flush immediately (not fence/header)"
        );
        // All subsequent words are mid-line, should produce output
        for (i, out) in outputs.iter().enumerate().skip(1) {
            assert!(
                !out.is_empty(),
                "Word {} should produce mid-line output, got empty",
                i
            );
        }
    }

    #[test]
    fn test_md_flush_on_whitespace_preserves_fence_detection() {
        // Ensure whitespace flush doesn't break fence detection.
        // "``` " could theoretically end with whitespace but should NOT flush
        // as inline text — it needs to be detected as a fence.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("```");
        assert_eq!(out, "", "Fence should buffer, not flush on whitespace");
        // Even with trailing space, the needs_line_buffering check fires first
        let out2 = r.render_delta(" ");
        // ``` + space = "``` " in buffer — needs_line_buffering still true (starts with `)
        // flush_on_whitespace shouldn't fire because needs_line_buffering resolved first
        assert_eq!(
            out2, "",
            "Fence with trailing space should still buffer for language detection"
        );
    }

    #[test]
    fn test_md_flush_on_whitespace_preserves_header_detection() {
        // "# " should not be flushed by whitespace — it's a header marker.
        // flush_on_whitespace guards against first-char '#'.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("# ");
        // The '#' triggers needs_line_buffering, try_resolve_block_prefix
        // doesn't handle headers, and flush_on_whitespace skips '#' content.
        // So "# " stays buffered.
        assert_eq!(
            out, "",
            "'# ' should remain buffered waiting for full header line"
        );

        // Complete the header line — should render with header styling
        let out2 = r.render_delta("Title\n");
        assert!(
            out2.contains("Title"),
            "Header should render when line completes, got: '{out2}'"
        );
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

    // --- Digit-word and dash-word early flush tests (issue #147) ---

    #[test]
    fn test_streaming_digit_nonlist_flushes_early() {
        // "2n" at line start — digit followed by a letter can't be a numbered list.
        // Should flush on the 2nd char since 'n' isn't '.' or ')'.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("2n");
        // "2n" should flush immediately — not a numbered list pattern
        assert!(
            !out1.is_empty(),
            "Digit followed by letter should flush immediately, got empty"
        );
        // Subsequent token is mid-line, should be immediate
        let out2 = r.render_delta("d");
        assert!(
            !out2.is_empty(),
            "Mid-line token after digit-word flush should be immediate, got empty"
        );
    }

    #[test]
    fn test_streaming_dash_nonlist_flushes_early() {
        // "-b" at line start — dash followed by a non-space, non-dash char
        // can't be a list item or horizontal rule. Should flush immediately.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("-b");
        assert!(
            !out1.is_empty(),
            "Dash followed by letter should flush immediately, got empty"
        );
        // Subsequent token is mid-line
        let out2 = r.render_delta("ased");
        assert!(
            !out2.is_empty(),
            "Mid-line token after dash-word flush should be immediate, got empty"
        );
    }

    #[test]
    fn test_streaming_numbered_list_still_buffers() {
        // "1." at line start — could be a numbered list, must keep buffering.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("1.");
        // "1." — digit followed by '.', still ambiguous (could be "1. item")
        assert!(
            out1.is_empty(),
            "Digit-dot should still buffer (potential numbered list), got: '{out1}'"
        );
        // "1. " confirms it's a list — should resolve via try_resolve_block_prefix
        let out2 = r.render_delta(" item");
        assert!(
            !out2.is_empty(),
            "Numbered list '1. item' should eventually produce output, got empty"
        );
    }

    #[test]
    fn test_streaming_dash_list_still_buffers() {
        // "- " at line start is a list item — should buffer correctly.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("- ");
        // "- " is a confirmed unordered list item
        // try_resolve_block_prefix should handle it
        // Whether it's empty or not depends on whether prefix resolves at "- "
        // The key: subsequent content should stream
        let out2 = r.render_delta("item");
        let total = format!("{out1}{out2}");
        assert!(
            total.contains("item"),
            "Dash list '- item' should produce output, got: '{total}'"
        );
    }

    #[test]
    fn test_streaming_dash_hr_still_buffers() {
        // "---" should still buffer as a potential horizontal rule.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("-");
        assert!(
            out1.is_empty(),
            "Single dash should buffer (ambiguous), got: '{out1}'"
        );
        let out2 = r.render_delta("-");
        assert!(
            out2.is_empty(),
            "Double dash should buffer (potential HR), got: '{out2}'"
        );
        let out3 = r.render_delta("-");
        // "---" is a horizontal rule, should still be buffered/handled correctly
        assert!(
            out3.is_empty(),
            "Triple dash should still buffer as HR, got: '{out3}'"
        );
    }

    #[test]
    fn test_streaming_mid_line_always_immediate() {
        // Once line_start is false, ALL tokens should be immediate regardless of content.
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("Hello ");
        assert!(!r.line_start, "Should be mid-line after 'Hello '");

        // Tokens that would trigger buffering at line start should be immediate mid-line
        for token in &["-", "1.", "```", "#", ">", "---"] {
            let out = r.render_delta(token);
            assert!(
                !out.is_empty(),
                "Mid-line token '{token}' should produce immediate output, got empty"
            );
        }
    }

    #[test]
    fn test_streaming_fence_still_buffers() {
        // "```" at line start should still buffer as a code fence.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("`");
        assert!(
            out1.is_empty(),
            "Single backtick should buffer, got: '{out1}'"
        );
        let out2 = r.render_delta("``");
        // Now buffer is "```" — still buffering as potential fence
        assert!(
            out2.is_empty(),
            "Triple backtick without newline should still buffer, got: '{out2}'"
        );
        // A newline confirms the fence
        let out3 = r.render_delta("\n");
        assert!(
            r.in_code_block,
            "Code fence should be detected after newline"
        );
        assert!(
            !out3.is_empty(),
            "Fence line should produce output on newline"
        );
    }

    #[test]
    fn test_streaming_plain_text_immediate() {
        // "Hello" at line start — first char 'H' is not special, should flush immediately.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("H");
        assert!(
            !out.is_empty(),
            "Non-special char 'H' at line start should flush immediately, got empty"
        );
    }

    #[test]
    fn test_streaming_digit_paren_still_buffers() {
        // "1)" at line start — digit followed by ')', could be a numbered list variant.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("1)");
        assert!(
            out.is_empty(),
            "Digit-paren should still buffer (potential list), got: '{out}'"
        );
    }

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

    #[test]
    fn test_streaming_contract_plain_text_no_buffering() {
        // Plain text starting with a non-special character at line start
        // should produce immediate output — no buffering needed.
        let mut r = MarkdownRenderer::new();
        assert!(r.line_start, "Renderer should start at line_start=true");

        // "H" is not a special char (#, `, >, -, *, +, digit, |, _)
        // so needs_line_buffering() returns false → flush as inline text
        let out1 = r.render_delta("H");
        assert!(
            !out1.is_empty(),
            "First token 'H' should produce immediate output (not special char), got empty"
        );
        assert!(
            !r.line_start,
            "After flushing 'H', line_start should be false"
        );
        assert!(
            r.line_buffer.is_empty(),
            "line_buffer should be empty after non-special first char flush"
        );

        // Mid-line tokens produce immediate output (mid-line fast path)
        let out2 = r.render_delta("ello");
        assert!(
            !out2.is_empty(),
            "Mid-line token 'ello' should produce immediate output"
        );
        assert!(
            r.line_buffer.is_empty(),
            "line_buffer should stay empty for mid-line tokens"
        );

        let out3 = r.render_delta(" world");
        assert!(
            !out3.is_empty(),
            "Mid-line token ' world' should produce immediate output"
        );
    }

    #[test]
    fn test_streaming_contract_code_block_passthrough() {
        // Tokens inside a code block should produce immediate output via
        // the mid-line fast path (DIM-wrapped), not the buffered path.
        let mut r = MarkdownRenderer::new();

        // Open a code fence
        let fence_out = r.render_delta("```rust\n");
        assert!(r.in_code_block, "Should be inside code block after fence");
        assert!(
            fence_out.contains(&format!("{DIM}```rust{RESET}")),
            "Fence line should be dim, got: '{fence_out}'"
        );

        // At code block line start, non-fence content resolves immediately.
        // "let x" starts with 'l' (not backtick) → early-resolve as code.
        let out1 = r.render_delta("let x");
        assert!(
            !out1.is_empty(),
            "Code block content 'let x' should produce immediate output, got empty"
        );
        assert!(
            out1.contains(&format!("{DIM}let x{RESET}")),
            "Mid-line code should be DIM-wrapped (fragment styling), got: '{out1}'"
        );

        // Mid-line code token (line_start=false)
        let out2 = r.render_delta(" = 42;");
        assert!(
            !out2.is_empty(),
            "Code block token ' = 42;' should produce immediate output"
        );
        assert!(
            out2.contains(&format!("{DIM} = 42;{RESET}")),
            "Mid-line code token should be DIM-wrapped, got: '{out2}'"
        );
    }

    #[test]
    fn test_streaming_contract_heading_detection() {
        // "#" at line start should buffer. After the line completes with "\n",
        // the heading should render with BOLD+CYAN formatting.
        let mut r = MarkdownRenderer::new();

        let out1 = r.render_delta("#");
        assert_eq!(
            out1, "",
            "'#' at line start should buffer (could be heading)"
        );
        assert!(!r.line_buffer.is_empty(), "line_buffer should contain '#'");

        // Complete the heading line
        let out2 = r.render_delta("# Title\n");
        // line_buffer was "#", now becomes "## Title" after append, then newline processes it
        assert!(
            out2.contains(&format!("{BOLD}{CYAN}")),
            "Heading should have BOLD+CYAN formatting, got: '{out2}'"
        );
        assert!(
            out2.contains("Title"),
            "Heading output should contain 'Title', got: '{out2}'"
        );
        assert!(
            r.line_start,
            "After newline, line_start should be true again"
        );
    }

    #[test]
    fn test_streaming_contract_blockquote_detection() {
        // ">" at line start triggers block-level buffering.
        // Once confirmed as blockquote, renders with DIM│ and ITALIC content.
        let mut r = MarkdownRenderer::new();

        // ">" is a blockquote — try_resolve_block_prefix handles it
        let out1 = r.render_delta("> ");
        // Blockquote prefix should be resolved early by try_resolve_block_prefix
        assert!(
            out1.contains(&format!("{DIM}│{RESET}")),
            "Blockquote should render dim vertical bar, got: '{out1}'"
        );
        assert!(
            r.block_prefix_rendered,
            "block_prefix_rendered should be true after blockquote prefix"
        );
        assert!(
            !r.line_start,
            "line_start should be false after prefix resolution"
        );

        // Subsequent content streams as mid-line inline text
        let out2 = r.render_delta("quoted text");
        assert!(
            !out2.is_empty(),
            "Content after blockquote prefix should stream immediately"
        );
        assert!(
            out2.contains("quoted text"),
            "Should contain the quoted text, got: '{out2}'"
        );

        // Complete the line
        let _out3 = r.render_delta("\n");
        assert!(r.line_start, "After newline, should be at line_start again");
    }

    #[test]
    fn test_streaming_contract_inline_formatting_mid_line() {
        // Mid-line **bold**, *italic*, and `code` formatting should be applied
        // through the render_inline fast path.
        let mut r = MarkdownRenderer::new();

        // Resolve line start with plain text first
        let _ = r.render_delta("This is ");
        assert!(!r.line_start, "Should be mid-line");

        // Bold mid-line
        let out_bold = r.render_delta("**bold**");
        assert!(
            out_bold.contains(&format!("{BOLD}bold{RESET}")),
            "Mid-line **bold** should get BOLD ANSI codes, got: '{out_bold}'"
        );

        // Italic mid-line
        let out_italic = r.render_delta(" and *italic*");
        assert!(
            out_italic.contains(&format!("{ITALIC}italic{RESET}")),
            "Mid-line *italic* should get ITALIC ANSI codes, got: '{out_italic}'"
        );

        // Inline code mid-line
        let out_code = r.render_delta(" and `code`");
        assert!(
            out_code.contains(&format!("{CYAN}code{RESET}")),
            "Mid-line `code` should get CYAN ANSI codes, got: '{out_code}'"
        );
    }

    #[test]
    fn test_streaming_contract_empty_delta() {
        // render_delta("") should return empty string and not corrupt state,
        // at both line_start=true and line_start=false.

        // Test at line_start=true
        let mut r = MarkdownRenderer::new();
        assert!(r.line_start);
        let out1 = r.render_delta("");
        assert_eq!(out1, "", "Empty delta at line_start should return empty");
        assert!(
            r.line_start,
            "line_start should remain true after empty delta"
        );
        assert!(
            r.line_buffer.is_empty(),
            "line_buffer should remain empty after empty delta"
        );
        assert!(
            !r.in_code_block,
            "in_code_block should remain false after empty delta"
        );

        // Test at line_start=false (mid-line)
        let _ = r.render_delta("Hello");
        assert!(!r.line_start, "Should be mid-line after 'Hello'");
        let out2 = r.render_delta("");
        assert_eq!(out2, "", "Empty delta at mid-line should return empty");
        assert!(
            !r.line_start,
            "line_start should remain false after empty mid-line delta"
        );
    }

    #[test]
    fn test_streaming_contract_newline_resets_line_start() {
        // After rendering mid-line content, a "\n" should set line_start=true.
        let mut r = MarkdownRenderer::new();

        // Get into mid-line state
        let _ = r.render_delta("Hello world");
        assert!(!r.line_start, "Should be mid-line after 'Hello world'");

        // Newline should reset to line_start
        let out = r.render_delta("\n");
        assert!(
            !out.is_empty() || out.contains('\n'),
            "Newline delta should produce output containing newline"
        );
        assert!(r.line_start, "line_start should be true after newline");
        assert!(
            !r.block_prefix_rendered,
            "block_prefix_rendered should be false after newline reset"
        );
    }

    #[test]
    fn test_streaming_contract_consecutive_code_blocks() {
        // Open fence → content → close fence → open another fence.
        // State should correctly track in_code_block across transitions.
        let mut r = MarkdownRenderer::new();

        // First code block
        let _ = r.render_delta("```\n");
        assert!(r.in_code_block, "Should be in code block after first fence");
        assert!(
            r.code_lang.is_none(),
            "No language specified for first fence"
        );

        let _ = r.render_delta("first block\n");
        assert!(r.in_code_block, "Should still be in code block");

        let _ = r.render_delta("```\n");
        assert!(
            !r.in_code_block,
            "Should exit code block after closing fence"
        );
        assert!(
            r.code_lang.is_none(),
            "code_lang should be None after closing"
        );

        // Normal text between code blocks
        let out_normal = r.render_delta("between blocks\n");
        assert!(
            !r.in_code_block,
            "Should not be in code block for normal text"
        );
        assert!(
            out_normal.contains("between blocks"),
            "Normal text should render, got: '{out_normal}'"
        );

        // Second code block with language
        let _ = r.render_delta("```python\n");
        assert!(
            r.in_code_block,
            "Should be in code block after second fence"
        );
        assert_eq!(
            r.code_lang.as_deref(),
            Some("python"),
            "Should capture language 'python'"
        );

        let _ = r.render_delta("second block\n");
        assert!(r.in_code_block, "Should still be in second code block");

        let _ = r.render_delta("```\n");
        assert!(
            !r.in_code_block,
            "Should exit second code block after closing fence"
        );
        assert!(
            r.code_lang.is_none(),
            "code_lang should be None after second close"
        );
    }

    #[test]
    fn test_streaming_contract_flush_final() {
        // After feeding partial content without a trailing newline,
        // flush() should emit whatever's in the line buffer.
        let mut r = MarkdownRenderer::new();

        // Feed content that stays buffered (# is ambiguous at line start)
        let out1 = r.render_delta("# Partial heading");
        // "# Partial heading" — starts with '#', needs_line_buffering=true.
        // flush_on_whitespace won't fire for '#'.
        // So it stays in the buffer.
        assert!(
            !r.line_buffer.is_empty() || !out1.is_empty(),
            "Content should be either buffered or already output"
        );

        // Flush should emit the remaining content
        let flushed = r.flush();
        assert!(!flushed.is_empty(), "flush() should emit buffered content");
        assert!(
            flushed.contains("Partial heading"),
            "flushed output should contain the text, got: '{flushed}'"
        );
        assert!(
            r.line_buffer.is_empty(),
            "line_buffer should be empty after flush"
        );

        // Also test flush with non-special content that was already emitted
        let mut r2 = MarkdownRenderer::new();
        let _ = r2.render_delta("Already emitted");
        // "Already emitted" starts with 'A' — non-special → flushed immediately
        let flushed2 = r2.flush();
        // Nothing should be in buffer since it was already emitted
        assert!(
            r2.line_buffer.is_empty(),
            "line_buffer should be empty after non-special text was already flushed"
        );
        // flushed2 might be empty (content already emitted) or contain RESET
        // The key contract: no panic, no corruption
        let _ = flushed2;
    }

    #[test]
    fn test_streaming_contract_nested_formatting_in_list() {
        // "- **bold item**\n" should get both list bullet formatting and bold.
        let mut r = MarkdownRenderer::new();

        let out = r.render_delta("- **bold item**\n");
        // This is a complete line, processed by render_line.
        // strip_unordered_list_marker finds "- " and returns "**bold item**".
        // render_inline processes the bold markers.
        assert!(
            out.contains(&format!("{CYAN}•{RESET}")),
            "Should have colored bullet, got: '{out}'"
        );
        assert!(
            out.contains(&format!("{BOLD}bold item{RESET}")),
            "Should have bold formatting inside list item, got: '{out}'"
        );

        // Also test streamed version where prefix resolves early
        let mut r2 = MarkdownRenderer::new();
        let out1 = r2.render_delta("- ");
        // "- " — try_resolve_block_prefix tries unordered list.
        // try_confirm_unordered_list: "- " has empty rest → returns Some("").
        // So prefix renders with bullet.
        let out2 = r2.render_delta("**bold item**");
        let out3 = r2.render_delta("\n");
        let total = format!("{out1}{out2}{out3}");
        assert!(
            total.contains(&format!("{CYAN}•{RESET}")),
            "Streamed list should have colored bullet, got: '{total}'"
        );
        assert!(
            total.contains("bold item"),
            "Streamed list should contain bold item text, got: '{total}'"
        );
    }

    #[test]
    fn test_streaming_contract_digit_word_flushes() {
        // Issue #147: digit-word patterns like "2nd" should flush early.
        // "2" at line start buffers (could be numbered list "2. ").
        // "2n" → second char is not '.' or ')' → needs_line_buffering() returns false → flush.
        let mut r = MarkdownRenderer::new();

        let out1 = r.render_delta("2");
        // "2" alone — a digit at line start with len < 2, needs_line_buffering returns true
        assert!(
            r.line_start,
            "After single digit '2', should still be at line_start (buffering)"
        );

        let out2 = r.render_delta("n");
        // line_buffer is now "2n". needs_line_buffering sees '2' then 'n' (not '.' or ')').
        // Returns false → buffer flushes as inline text.
        let combined = format!("{out1}{out2}");
        assert!(
            !combined.is_empty(),
            "After '2n', digit-word should have flushed, got empty"
        );
        assert!(
            combined.contains('2'),
            "Flushed output should contain '2', got: '{combined}'"
        );
        assert!(
            !r.line_start,
            "After digit-word flush, line_start should be false"
        );

        // Subsequent tokens stream immediately via mid-line fast path
        let out3 = r.render_delta("d");
        assert!(
            !out3.is_empty(),
            "Mid-line token 'd' should produce immediate output"
        );
    }

    #[test]
    fn test_streaming_contract_dash_word_flushes() {
        // Issue #147: dash-word patterns like "-based" should flush early.
        // "-" at line start buffers (could be list "- " or horizontal rule "---").
        // "-b" → second char is not space or dash → needs_line_buffering() returns false → flush.
        let mut r = MarkdownRenderer::new();

        let out1 = r.render_delta("-");
        // "-" alone — needs_line_buffering: trimmed.len() < 2 → true
        assert!(
            r.line_start,
            "After single dash '-', should still be at line_start (buffering)"
        );

        let out2 = r.render_delta("b");
        // line_buffer is now "-b". needs_line_buffering: second char 'b' != ' ' && != '-'
        // → returns false → flush as inline text.
        let combined = format!("{out1}{out2}");
        assert!(
            !combined.is_empty(),
            "After '-b', dash-word should have flushed, got empty"
        );
        assert!(
            !r.line_start,
            "After dash-word flush, line_start should be false"
        );

        // Subsequent tokens stream immediately
        let out3 = r.render_delta("ased");
        assert!(
            !out3.is_empty(),
            "Mid-line token 'ased' should produce immediate output"
        );
    }

    #[test]
    fn test_streaming_contract_numbered_list_buffers() {
        // "1." at line start should keep buffering (could be numbered list "1. item").
        // needs_line_buffering: digit followed by '.' → keeps buffering.
        // Once "1. item" arrives (via newline), it resolves as ordered list.
        let mut r = MarkdownRenderer::new();

        let out1 = r.render_delta("1");
        assert!(r.line_start, "After '1', should still buffer at line_start");

        let out2 = r.render_delta(".");
        // line_buffer is "1." — needs_line_buffering: digit then '.', trimmed.len() < 3 → true
        assert!(
            r.line_start,
            "After '1.', should still buffer (could be numbered list)"
        );

        let out3 = r.render_delta(" ");
        // line_buffer is "1. " — needs_line_buffering checks for ". " pattern.
        // try_resolve_block_prefix tries ordered list: "1. " with empty content → returns None.
        // flush_on_whitespace: starts with digit → returns empty.
        // So still buffering.
        let pre_content = format!("{out1}{out2}{out3}");

        let out4 = r.render_delta("item");
        // line_buffer is "1. item" — needs_line_buffering: contains ". " and digits before it → true.
        // try_resolve_block_prefix → try_confirm_ordered_list: "1. item" → Some(("1", "item")).
        // Renders prefix and sets line_start=false.
        let all = format!("{pre_content}{out4}");
        assert!(
            all.contains(&format!("{CYAN}1.{RESET}")),
            "Numbered list should render with CYAN number, got: '{all}'"
        );
        assert!(
            all.contains("item"),
            "Should contain list item content, got: '{all}'"
        );
        assert!(
            !r.line_start,
            "After ordered list prefix resolves, line_start should be false"
        );
    }

    #[test]
    fn test_streaming_contract_multi_digit_numbered_list_buffers() {
        // "12." at line start should keep buffering (could be "12. item").
        // The early disambiguation should NOT flush "12." as inline text —
        // digits followed by '.' is a valid numbered-list prefix pattern.
        let mut r = MarkdownRenderer::new();

        let out1 = r.render_delta("1");
        assert!(r.line_start, "After '1', should still buffer");

        let out2 = r.render_delta("2");
        // "12" — all digits, len < 3, needs_line_buffering → true
        assert!(r.line_start, "After '12', should still buffer (all digits)");

        let out3 = r.render_delta(".");
        // "12." — digits followed by '.', should keep buffering
        // (could become "12. item" — a numbered list)
        assert!(
            r.line_start,
            "After '12.', should still buffer (could be numbered list like '12. item')"
        );

        let out4 = r.render_delta(" ");
        // "12. " — has ". " pattern with digits before it
        let out5 = r.render_delta("item");
        // "12. item" — should resolve as ordered list
        let all = format!("{out1}{out2}{out3}{out4}{out5}");
        assert!(
            all.contains(&format!("{CYAN}12.{RESET}")),
            "Multi-digit numbered list should render with CYAN number, got: '{all}'"
        );
        assert!(
            all.contains("item"),
            "Should contain list item content, got: '{all}'"
        );
        assert!(
            !r.line_start,
            "After ordered list prefix resolves, line_start should be false"
        );
    }

    #[test]
    fn test_streaming_contract_digit_dot_non_space_flushes() {
        // "12.x" at line start: digits + '.' + non-space → not a numbered list.
        // Should flush as inline text once the non-space char after '.' is seen.
        let mut r = MarkdownRenderer::new();

        let out1 = r.render_delta("1");
        assert!(r.line_start, "After '1', should buffer");

        let out2 = r.render_delta("2");
        assert!(r.line_start, "After '12', should buffer");

        let out3 = r.render_delta(".");
        // "12." — digits + '.', could be list, still buffering
        assert!(r.line_start, "After '12.', should still buffer");

        let out4 = r.render_delta("x");
        // "12.x" — char after dot is 'x', not space → not a list → flush
        let combined = format!("{out1}{out2}{out3}{out4}");
        assert!(
            !combined.is_empty(),
            "After '12.x' (not a list), should flush as inline text"
        );
        assert!(
            !r.line_start,
            "After flushing '12.x', line_start should be false"
        );
    }

    #[test]
    fn test_streaming_contract_unordered_list_buffers() {
        // "- " at line start triggers list detection but doesn't resolve until
        // non-dash, non-space content arrives (to rule out horizontal rule "- - -").
        // After "- item", try_resolve_block_prefix confirms it as a list.
        let mut r = MarkdownRenderer::new();

        let out1 = r.render_delta("- ");
        // "- " alone: try_confirm_unordered_list returns None (could be "- - -" HR).
        // Still buffering.
        assert!(
            r.line_start,
            "After '- ', should still be at line_start (not yet confirmed as list)"
        );

        let out2 = r.render_delta("item");
        // line_buffer is "- item" — try_confirm_unordered_list: rest="item", has non-dash char → Some.
        // Prefix renders with bullet, line_start=false.
        let combined = format!("{out1}{out2}");
        assert!(
            combined.contains(&format!("{CYAN}•{RESET}")),
            "Unordered list should render with CYAN bullet after '- item', got: '{combined}'"
        );
        assert!(
            !r.line_start,
            "After list prefix resolves, line_start should be false"
        );
        assert!(
            r.block_prefix_rendered,
            "block_prefix_rendered should be true after list prefix"
        );
        assert!(
            combined.contains("item"),
            "Output should contain 'item', got: '{combined}'"
        );
    }

    #[test]
    fn test_streaming_contract_code_fence_buffers() {
        // Code fence "```" should buffer until fully resolved.
        // No output should leak before the fence is complete.
        let mut r = MarkdownRenderer::new();

        let out1 = r.render_delta("`");
        assert_eq!(
            out1, "",
            "Single '`' at line start should buffer (could be fence)"
        );
        assert!(r.line_start, "Should still be at line_start after '`'");

        let out2 = r.render_delta("`");
        assert_eq!(
            out2, "",
            "Two backticks '``' should still buffer (could be fence)"
        );
        assert!(r.line_start, "Should still be at line_start after '``'");

        let out3 = r.render_delta("`");
        assert_eq!(
            out3, "",
            "Three backticks '```' should still buffer (fence, awaiting newline)"
        );

        let out4 = r.render_delta("rust\n");
        // Now the fence line "```rust\n" is complete — should produce output
        let all = format!("{out1}{out2}{out3}{out4}");
        assert!(
            !all.is_empty(),
            "Complete fence line should produce output, got empty"
        );
        assert!(
            r.in_code_block,
            "Should be inside code block after fence resolves"
        );
    }

    #[test]
    fn test_streaming_contract_mid_line_immediate() {
        // After line_start is set to false (by flushing initial content),
        // subsequent tokens should produce immediate output via mid-line fast path.
        let mut r = MarkdownRenderer::new();

        // "Hello" starts with 'H' — not a special char, flushes immediately
        let out1 = r.render_delta("Hello");
        assert!(
            !out1.is_empty(),
            "'Hello' should flush immediately (non-special first char)"
        );
        assert!(!r.line_start, "After flushing 'Hello', should be mid-line");

        // Now feed mid-line content
        let out2 = r.render_delta(" world");
        assert!(
            !out2.is_empty(),
            "Mid-line ' world' should produce immediate output"
        );
        assert!(
            out2.contains("world"),
            "Mid-line output should contain 'world', got: '{out2}'"
        );
    }

    #[test]
    fn test_streaming_contract_plain_text_immediate_flush() {
        // Text starting with a non-special character ('H', 'T', 'A', etc.)
        // should flush immediately — no buffering needed.
        let mut r = MarkdownRenderer::new();
        assert!(r.line_start, "Fresh renderer starts at line_start=true");

        let out = r.render_delta("Hello");
        assert!(
            !out.is_empty(),
            "'Hello' at line start should produce immediate output (not a special char)"
        );
        assert!(
            out.contains("Hello"),
            "Output should contain 'Hello', got: '{out}'"
        );
        assert!(
            !r.line_start,
            "After flushing plain text, line_start should be false"
        );
        assert!(
            r.line_buffer.is_empty(),
            "line_buffer should be empty after immediate flush"
        );
    }

    #[test]
    fn test_streaming_contract_heading_buffers_then_resolves() {
        // "#" at line start should buffer. "# Title\n" resolves as heading.
        let mut r = MarkdownRenderer::new();

        let out1 = r.render_delta("#");
        assert_eq!(
            out1, "",
            "'#' at line start should buffer (could be heading)"
        );
        assert!(r.line_start, "Should still be at line_start after '#'");
        assert!(!r.line_buffer.is_empty(), "line_buffer should contain '#'");

        let out2 = r.render_delta(" ");
        // line_buffer is "# " — still needs buffering (heading confirmed but no content yet)
        let out3 = r.render_delta("Title");
        let out4 = r.render_delta("\n");
        let all = format!("{out1}{out2}{out3}{out4}");

        // After newline, the complete heading "# Title" should render with formatting
        assert!(
            all.contains(&format!("{BOLD}{CYAN}")),
            "Heading should have BOLD+CYAN formatting, got: '{all}'"
        );
        assert!(
            all.contains("Title"),
            "Heading output should contain 'Title', got: '{all}'"
        );
        assert!(r.line_start, "After newline, should be at line_start again");
    }

    #[test]
    fn test_color_struct_display_consistency() {
        // All color constants should be the same type and format without panic
        let result = format!("{BOLD}{DIM}{GREEN}{YELLOW}{CYAN}{RED}{RESET}");
        // Should either have all codes or be empty (if NO_COLOR is set)
        assert!(result.contains('\x1b') || result.is_empty());
    }

    // --- MarkdownRenderer tests ---

    #[test]
    fn test_streaming_multi_digit_nonlist_flushes() {
        // "100m" — multi-digit number followed by letter, not a list.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("10");
        // "10" — all digits, could still be "10. " — should buffer
        assert!(
            out1.is_empty(),
            "All-digit '10' should buffer (could be list number), got: '{out1}'"
        );
        let out2 = r.render_delta("0m");
        // "100m" — the 'm' disambiguates: not a list number
        assert!(
            !out2.is_empty(),
            "'100m' should flush — letter after digits means not a list, got empty"
        );
    }
}
