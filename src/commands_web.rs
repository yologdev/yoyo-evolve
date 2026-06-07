//! Web content fetching (/web) and clipboard integration (/copy).

use crate::format::*;

/// Maximum characters to display from a fetched web page.
pub(crate) const WEB_MAX_CHARS: usize = 5000;

/// Case-insensitive search for an ASCII-only pattern in a UTF-8 string.
///
/// Returns the byte offset in `haystack` where `needle` starts.
/// `needle` must be ASCII lowercase.
fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let needle_bytes = needle.as_bytes();
    let hay_bytes = haystack.as_bytes();
    if needle_bytes.is_empty() || needle_bytes.len() > hay_bytes.len() {
        return None;
    }
    'outer: for start in 0..=(hay_bytes.len() - needle_bytes.len()) {
        for (k, &nb) in needle_bytes.iter().enumerate() {
            if hay_bytes[start + k].to_ascii_lowercase() != nb {
                continue 'outer;
            }
        }
        return Some(start);
    }
    None
}

/// Check if `haystack` starts with ASCII lowercase `needle` (case-insensitive).
fn starts_with_ascii_ci(haystack: &str, needle: &str) -> bool {
    let hay_bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();
    if hay_bytes.len() < needle_bytes.len() {
        return false;
    }
    for (k, &nb) in needle_bytes.iter().enumerate() {
        if hay_bytes[k].to_ascii_lowercase() != nb {
            return false;
        }
    }
    true
}

/// Strip HTML tags and extract readable text content.
///
/// This function:
/// - Removes `<script>`, `<style>`, `<nav>`, `<footer>`, `<header>`, `<svg>` blocks entirely
/// - Converts `<br>`, `<p>`, `<div>`, `<li>`, `<h1>`–`<h6>`, `<tr>` to newlines
/// - Converts `<li>` items to bullet points
/// - Strips all remaining HTML tags
/// - Decodes common HTML entities
/// - Collapses excessive whitespace
/// - Truncates to `max_chars`
pub fn strip_html_tags(html: &str, max_chars: usize) -> String {
    // First pass: remove blocks we want to skip entirely (script, style, etc.)
    // Uses find_ascii_ci for case-insensitive tag matching without pre-lowering
    // the entire string (which would break byte-position correspondence for
    // non-ASCII chars whose lowercase has a different byte length).
    let mut cleaned = String::with_capacity(html.len());
    let skip_tags = ["script", "style", "nav", "footer", "header", "svg"];

    let mut i = 0;
    let bytes = html.as_bytes();

    while i < bytes.len() {
        // '<' is ASCII (0x3C) — never appears as a UTF-8 continuation byte
        if bytes[i] == b'<' {
            let rest = &html[i..];
            let mut found_skip = false;
            for tag in &skip_tags {
                let open = format!("<{}", tag);
                if starts_with_ascii_ci(rest, &open) {
                    // Check delimiter after tag name (open is ASCII, so len is byte-safe)
                    let after = &rest[open.len()..];
                    if after.is_empty()
                        || after.starts_with(' ')
                        || after.starts_with('>')
                        || after.starts_with('\t')
                        || after.starts_with('\n')
                    {
                        // Find the closing tag (case-insensitive)
                        let close = format!("</{}>", tag);
                        if let Some(end_pos) = find_ascii_ci(rest, &close) {
                            i += end_pos + close.len();
                            found_skip = true;
                            break;
                        }
                    }
                }
            }
            if !found_skip {
                cleaned.push('<');
                i += 1; // '<' is 1 byte
            }
        } else {
            // Copy one full UTF-8 character. i is always at a char boundary
            // because we only advance by char len or past single-byte ASCII '<'.
            if let Some(c) = html[i..].chars().next() {
                cleaned.push(c);
                i += c.len_utf8();
            } else {
                break;
            }
        }
    }

    // Second pass: convert meaningful tags to formatting, strip the rest.
    // Tag delimiters '<' and '>' are ASCII, so byte-scanning for them is safe
    // in UTF-8. Non-tag text is copied char-by-char to preserve multi-byte chars.
    let mut result = String::with_capacity(cleaned.len());
    let cbytes = cleaned.as_bytes();
    let mut j = 0;

    while j < cbytes.len() {
        if cbytes[j] == b'<' {
            let tag_start = j;
            let mut tag_end = j + 1;
            // '>' is ASCII — safe to scan byte-by-byte
            while tag_end < cbytes.len() && cbytes[tag_end] != b'>' {
                tag_end += 1;
            }
            if tag_end < cbytes.len() {
                tag_end += 1; // include '>'
            }

            let tag_content = &cleaned[tag_start..tag_end.min(cbytes.len())];

            if starts_with_ascii_ci(tag_content, "<br") {
                result.push('\n');
            } else if starts_with_ascii_ci(tag_content, "<li") {
                result.push_str("\n• ");
            } else if starts_with_ascii_ci(tag_content, "<h1")
                || starts_with_ascii_ci(tag_content, "<h2")
                || starts_with_ascii_ci(tag_content, "<h3")
                || starts_with_ascii_ci(tag_content, "<h4")
                || starts_with_ascii_ci(tag_content, "<h5")
                || starts_with_ascii_ci(tag_content, "<h6")
            {
                result.push_str("\n\n");
            } else if starts_with_ascii_ci(tag_content, "</h")
                || starts_with_ascii_ci(tag_content, "<p")
                || starts_with_ascii_ci(tag_content, "</p")
                || starts_with_ascii_ci(tag_content, "<div")
                || starts_with_ascii_ci(tag_content, "</div")
                || starts_with_ascii_ci(tag_content, "<tr")
                || starts_with_ascii_ci(tag_content, "</tr")
                || starts_with_ascii_ci(tag_content, "<blockquote")
                || starts_with_ascii_ci(tag_content, "</blockquote")
                || starts_with_ascii_ci(tag_content, "<section")
                || starts_with_ascii_ci(tag_content, "</section")
                || starts_with_ascii_ci(tag_content, "<article")
                || starts_with_ascii_ci(tag_content, "</article")
            {
                result.push('\n');
            }
            // All other tags: skip (emit nothing)

            j = tag_end;
        } else {
            // Copy one full UTF-8 character
            if let Some(c) = cleaned[j..].chars().next() {
                result.push(c);
                j += c.len_utf8();
            } else {
                break;
            }
        }
    }

    // Decode HTML entities (shared utility)
    let decoded = crate::format::decode_html_entities(&result);

    // Collapse whitespace: multiple blank lines → two newlines, multiple spaces → one
    let mut final_text = String::with_capacity(decoded.len());
    let mut prev_newlines = 0u32;
    let mut prev_space = false;

    for c in decoded.chars() {
        if c == '\n' {
            prev_newlines += 1;
            prev_space = false;
            if prev_newlines <= 2 {
                final_text.push('\n');
            }
        } else if c == ' ' || c == '\t' {
            if prev_newlines > 0 {
                // Skip spaces right after newlines (trim line starts)
            } else if !prev_space {
                final_text.push(' ');
                prev_space = true;
            }
        } else {
            prev_newlines = 0;
            prev_space = false;
            final_text.push(c);
        }
    }

    // Trim each line and rejoin
    let final_text: String = final_text
        .lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n");

    let final_text = final_text.trim().to_string();

    // Truncate to max_chars
    if final_text.len() > max_chars {
        let truncated = &final_text[..final_text.floor_char_boundary(max_chars)];
        format!("{truncated}\n\n[… truncated at {max_chars} chars]")
    } else {
        final_text
    }
}

/// Validate that a string looks like a URL.
pub fn is_valid_url(url: &str) -> bool {
    (url.starts_with("http://") || url.starts_with("https://"))
        && url.len() > 10
        && url.contains('.')
}

/// Fetch a URL using curl and return the HTML content.
pub(crate) fn fetch_url(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args([
            "-sL", // silent, follow redirects
            "--max-time",
            "15", // timeout
            "-A",
            "Mozilla/5.0 (compatible; yoyo-agent/0.1)", // user agent
            url,
        ])
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "curl failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    let body = String::from_utf8_lossy(&output.stdout).to_string();
    if body.is_empty() {
        return Err("empty response".to_string());
    }

    Ok(body)
}

/// Handle the /web command — fetch a URL and display readable text.
/// Subcommands for `/web` tab-completion.
pub const WEB_SUBCOMMANDS: &[&str] = &["search"];

pub fn handle_web(input: &str) {
    let arg = input.trim_start_matches("/web").trim();

    if arg.is_empty() {
        println!("{DIM}  usage: /web <url>          — fetch a web page");
        println!("         /web search <query> — search the web{RESET}\n");
        return;
    }

    if arg.starts_with("search ") || arg.starts_with("search\t") || arg == "search" {
        let query = arg.strip_prefix("search").unwrap().trim();
        handle_web_search(query);
        return;
    }

    // Existing URL fetch logic
    let url = arg;

    // Auto-prepend https:// if missing
    let url = if !url.starts_with("http://") && !url.starts_with("https://") {
        format!("https://{url}")
    } else {
        url.to_string()
    };

    if !is_valid_url(&url) {
        println!("{RED}  Invalid URL: {url}{RESET}\n");
        return;
    }

    println!("{DIM}  Fetching {url}...{RESET}");

    match fetch_url(&url) {
        Ok(html) => {
            let text = strip_html_tags(&html, WEB_MAX_CHARS);
            if text.is_empty() {
                println!("{DIM}  (no readable text content found){RESET}\n");
            } else {
                let line_count = text.lines().count();
                let char_count = text.len();
                println!();
                println!("{text}");
                println!();
                println!("{DIM}  ── {line_count} lines, {char_count} chars from {url}{RESET}\n");
            }
        }
        Err(e) => {
            println!("{RED}  Failed to fetch: {e}{RESET}\n");
        }
    }
}

fn handle_web_search(query: &str) {
    if query.is_empty() {
        println!("{DIM}  usage: /web search <query>{RESET}\n");
        return;
    }

    println!("{DIM}  Searching for: {query}...{RESET}");

    match web_search(query, 8) {
        Ok(results) if results.is_empty() => {
            println!("  {DIM}(no results found){RESET}\n");
        }
        Ok(results) => {
            println!();
            for (i, r) in results.iter().enumerate() {
                println!("  {}. {BOLD}{}{RESET}", i + 1, r.title);
                println!("     {CYAN}{}{RESET}", r.url);
                if !r.snippet.is_empty() {
                    println!("     {DIM}{}{RESET}", r.snippet);
                }
                println!();
            }
            println!("  {DIM}Tip: use /web <url> to read any result{RESET}\n");
        }
        Err(e) => {
            println!("  {RED}Search failed: {e}{RESET}\n");
        }
    }
}

pub const COPY_SUBCOMMANDS: &[&str] = &["last", "code"];

/// Extract the text content of the last assistant message from the message
/// list.  Returns `None` if there are no assistant messages.
pub(crate) fn extract_last_assistant_text(messages: &[yoagent::AgentMessage]) -> Option<String> {
    for msg in messages.iter().rev() {
        if let yoagent::AgentMessage::Llm(yoagent::Message::Assistant { content, .. }) = msg {
            let text: String = content
                .iter()
                .filter_map(|c| match c {
                    yoagent::Content::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// Extract the last fenced code block (` ```...``` `) from a markdown string.
/// Returns the code content without the fence markers.
pub(crate) fn extract_last_code_block(text: &str) -> Option<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut in_block = false;
    let mut current_block = String::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_block {
                // Closing fence
                blocks.push(current_block.clone());
                current_block.clear();
                in_block = false;
            } else {
                // Opening fence — skip the language tag
                in_block = true;
                current_block.clear();
            }
        } else if in_block {
            if !current_block.is_empty() {
                current_block.push('\n');
            }
            current_block.push_str(line);
        }
    }

    blocks.last().cloned()
}

/// Return the platform-appropriate clipboard command name.
///
/// Returns `(command, args)` for the clipboard tool that should receive text
/// on stdin.  Returns `None` if no known tool is available.
fn clipboard_command() -> Option<(&'static str, Vec<&'static str>)> {
    if cfg!(target_os = "macos") {
        Some(("pbcopy", vec![]))
    } else if cfg!(target_os = "windows") {
        Some(("clip.exe", vec![]))
    } else {
        // Linux: try Wayland first, then X11 tools
        if command_exists("wl-copy") {
            Some(("wl-copy", vec![]))
        } else if command_exists("xclip") {
            Some(("xclip", vec!["-selection", "clipboard"]))
        } else if command_exists("xsel") {
            Some(("xsel", vec!["--clipboard", "--input"]))
        } else {
            None
        }
    }
}

/// Check if a command exists on the system PATH.
fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Copy text to the system clipboard.  Returns `Ok(())` on success.
fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let (cmd, args) = clipboard_command().ok_or_else(|| {
        if cfg!(target_os = "linux") {
            "No clipboard tool found. Install one of: wl-copy (Wayland), xclip, or xsel".to_string()
        } else {
            "No clipboard tool found".to_string()
        }
    })?;

    let mut child = std::process::Command::new(cmd)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run {cmd}: {e}"))?;

    if let Some(ref mut stdin) = child.stdin {
        use std::io::Write;
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("Failed to write to {cmd}: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for {cmd}: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("{cmd} failed: {}", stderr.trim()))
    }
}

/// Handle the `/copy` slash command.
///
/// - `/copy` or `/copy last` — copy the last assistant message text
/// - `/copy code` — copy the last code block from the last assistant message
/// - `/copy <text>` — copy the literal text argument
pub fn handle_copy(input: &str, messages: &[yoagent::AgentMessage]) {
    let args = input.strip_prefix("/copy").unwrap_or("").trim().to_string();

    let text = if args.is_empty() || args == "last" {
        // Copy last assistant message text
        match extract_last_assistant_text(messages) {
            Some(t) => t,
            None => {
                eprintln!("{YELLOW}  No assistant messages to copy{RESET}");
                return;
            }
        }
    } else if args == "code" {
        // Copy last code block from last assistant message
        let assistant_text = match extract_last_assistant_text(messages) {
            Some(t) => t,
            None => {
                eprintln!("{YELLOW}  No assistant messages to copy{RESET}");
                return;
            }
        };
        match extract_last_code_block(&assistant_text) {
            Some(code) => code,
            None => {
                eprintln!("{YELLOW}  No code blocks found in the last response{RESET}");
                return;
            }
        }
    } else {
        // Copy the literal text
        args
    };

    let char_count = text.len();
    match copy_to_clipboard(&text) {
        Ok(()) => {
            eprintln!("{GREEN}  ✓ Copied {char_count} chars to clipboard{RESET}");
        }
        Err(e) => {
            eprintln!("{RED}  ✗ {e}{RESET}");
        }
    }
}

// ---------------------------------------------------------------------------
// Web search via DuckDuckGo HTML endpoint
// ---------------------------------------------------------------------------

/// A single web search result.
pub(crate) struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Simple percent-encoding for URL query parameters.
/// Encodes everything except unreserved characters (A-Z, a-z, 0-9, `-`, `_`, `.`, `~`).
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

/// Decode a percent-encoded string (e.g. from a URL query parameter).
fn url_decode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&String::from_utf8_lossy(&bytes[i + 1..i + 3]), 16)
            {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Extract the actual URL from a DuckDuckGo redirect href.
///
/// DDG wraps result links like:
/// `//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage&rut=...`
///
/// We extract the `uddg=` value and percent-decode it. If the href doesn't
/// contain `uddg=`, we return it as-is (stripping a leading `//` if present).
pub(crate) fn extract_ddg_url(href: &str) -> String {
    // Look for uddg= parameter
    if let Some(pos) = href.find("uddg=") {
        let value_start = pos + 5; // len("uddg=")
        let rest = &href[value_start..];
        // Value ends at next '&' or end of string
        let value_end = rest.find('&').unwrap_or(rest.len());
        let encoded = &rest[..value_end];
        return url_decode(encoded);
    }
    // No uddg parameter — return href cleaned up
    let trimmed = href.trim();
    if trimmed.starts_with("//") {
        format!("https:{trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Extract the value of an HTML attribute from a tag string.
///
/// Given `<a class="result__a" href="/l/?uddg=...">`, calling
/// `extract_attr(tag, "href")` returns the value inside the quotes.
fn extract_attr(tag: &str, attr_name: &str) -> Option<String> {
    // Search for attr_name= (case-insensitive for the attr name)
    let needle = format!("{attr_name}=");
    let pos = find_ascii_ci(tag, &needle.to_ascii_lowercase())?;
    let after = &tag[pos + needle.len()..];
    // Attribute value may be quoted with " or '
    let (quote, rest) = if let Some(stripped) = after.strip_prefix('"') {
        ('"', stripped)
    } else if let Some(stripped) = after.strip_prefix('\'') {
        ('\'', stripped)
    } else {
        // Unquoted — value ends at whitespace or >
        let end = after
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(after.len());
        return Some(after[..end].to_string());
    };
    let end = rest.find(quote).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// Extract text content between an opening tag at position 0 and its closing tag.
/// Returns the inner text with HTML tags stripped.
fn extract_inner_text(html: &str) -> String {
    // Strip all tags from the fragment
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    let decoded = crate::format::decode_html_entities(&out);
    // Collapse whitespace
    let mut result = String::with_capacity(decoded.len());
    let mut prev_space = false;
    for c in decoded.chars() {
        if c.is_whitespace() {
            if !prev_space {
                result.push(' ');
                prev_space = true;
            }
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    result.trim().to_string()
}

/// Parse DuckDuckGo HTML search results into structured data.
///
/// The HTML structure we look for:
/// ```html
/// <div class="result results_links results_links_deep web-result">
///   <a class="result__a" href="//duckduckgo.com/l/?uddg=...">Title</a>
///   <a class="result__snippet" href="...">Snippet text</a>
/// </div>
/// ```
pub(crate) fn parse_ddg_results(html: &str, max_results: usize) -> Vec<WebSearchResult> {
    let max_results = max_results.min(20);
    let mut results = Vec::new();

    // Strategy: find each `class="result__a"` anchor — that's the title+url.
    // Then find the next `result__snippet` after it for the snippet.
    let mut search_from = 0;

    while results.len() < max_results {
        // Find the next result__a anchor
        let anchor_class = "class=\"result__a\"";
        let alt_anchor_class = "class='result__a'";
        let anchor_pos = find_ascii_ci(&html[search_from..], anchor_class)
            .or_else(|| find_ascii_ci(&html[search_from..], alt_anchor_class));
        let anchor_pos = match anchor_pos {
            Some(p) => search_from + p,
            None => break,
        };

        // Walk backwards to find the opening '<a' of this tag
        let tag_start = html[..anchor_pos].rfind('<').unwrap_or(anchor_pos);
        // Find the end of this opening tag
        let tag_end = match html[tag_start..].find('>') {
            Some(p) => tag_start + p + 1,
            None => {
                search_from = anchor_pos + anchor_class.len();
                continue;
            }
        };

        let tag_str = &html[tag_start..tag_end];

        // Extract href
        let href = match extract_attr(tag_str, "href") {
            Some(h) => h,
            None => {
                search_from = tag_end;
                continue;
            }
        };

        let url = extract_ddg_url(&href);

        // Extract title: text between <a ...> and </a>
        let close_a = match find_ascii_ci(&html[tag_end..], "</a>") {
            Some(p) => tag_end + p,
            None => {
                search_from = tag_end;
                continue;
            }
        };
        let title_html = &html[tag_end..close_a];
        let title = extract_inner_text(title_html);

        // Now look for the snippet in the vicinity (within next ~2000 chars)
        let snippet_search_end = (close_a + 2000).min(html.len());
        let snippet_region = &html[close_a..snippet_search_end];

        let snippet = if let Some(snip_pos) = find_ascii_ci(snippet_region, "result__snippet") {
            // Find the tag start
            let snip_abs = close_a + snip_pos;
            let snip_tag_start = html[..snip_abs].rfind('<').unwrap_or(snip_abs);
            // Find end of opening tag
            if let Some(snip_tag_end_rel) = html[snip_tag_start..].find('>') {
                let snip_tag_end = snip_tag_start + snip_tag_end_rel + 1;
                // Determine if it's an <a> or <td> tag
                let tag_name_region = &html[snip_tag_start..snip_tag_end].to_ascii_lowercase();
                let close_tag = if tag_name_region.starts_with("<td") {
                    "</td>"
                } else {
                    "</a>"
                };
                if let Some(snip_close_rel) = find_ascii_ci(&html[snip_tag_end..], close_tag) {
                    let snip_content = &html[snip_tag_end..snip_tag_end + snip_close_rel];
                    extract_inner_text(snip_content)
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Only add if we have at least a title and URL
        if !title.is_empty() && !url.is_empty() {
            results.push(WebSearchResult {
                title,
                url,
                snippet,
            });
        }

        search_from = close_a + 4; // past </a>
    }

    results
}

/// Format search results as a clean numbered text block.
pub(crate) fn format_search_results(results: &[WebSearchResult]) -> String {
    if results.is_empty() {
        return "No results found.".to_string();
    }
    let mut out = String::new();
    for (i, r) in results.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!("{}. {}\n", i + 1, r.title));
        out.push_str(&format!("   {}\n", r.url));
        if !r.snippet.is_empty() {
            out.push_str(&format!("   {}\n", r.snippet));
        }
    }
    out
}

/// Perform a web search via DuckDuckGo and return formatted results.
///
/// Uses `curl` to hit the DuckDuckGo HTML endpoint, parses results, and
/// returns them as a formatted string. On failure returns an error message
/// (never `Result` — designed for tool consumption).
/// Perform a web search via DuckDuckGo and return structured results.
///
/// Uses `curl` to hit the DuckDuckGo HTML endpoint and parses the response.
pub(crate) fn web_search(query: &str, max_results: usize) -> Result<Vec<WebSearchResult>, String> {
    let max_results = if max_results == 0 {
        8
    } else {
        max_results.min(20)
    };
    let encoded_query = url_encode(query);
    let url = format!("https://html.duckduckgo.com/html/?q={encoded_query}");

    let output = std::process::Command::new("curl")
        .args([
            "-sL",
            "--max-time",
            "15",
            "-A",
            "Mozilla/5.0 (compatible; yoyo-agent/0.1)",
            &url,
        ])
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "search request failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    let html = String::from_utf8_lossy(&output.stdout).to_string();
    if html.is_empty() {
        return Err("empty response from search".to_string());
    }

    let results = parse_ddg_results(&html, max_results);
    if results.is_empty() {
        return Err(
            "no search results found (DuckDuckGo returned no parseable results)".to_string(),
        );
    }

    Ok(results)
}

/// Convenience: run a web search and return formatted text.
///
/// Returns a human-/agent-readable string on both success and failure.
pub(crate) fn web_search_and_read(query: &str, max_results: usize) -> String {
    match web_search(query, max_results) {
        Ok(results) => format_search_results(&results),
        Err(e) => format!("Web search failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── strip_html_tags ──────────────────────────────────────────────

    #[test]
    fn strip_html_basic_paragraph() {
        let html = "<p>Hello, world!</p>";
        let text = strip_html_tags(html, 5000);
        assert_eq!(text, "Hello, world!");
    }

    #[test]
    fn strip_html_removes_script_and_style() {
        let html =
            "<p>Before</p><script>alert('xss');</script><style>.x{color:red}</style><p>After</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("color:red"));
    }

    #[test]
    fn strip_html_removes_nav_footer_header() {
        let html = "<header>Nav stuff</header><p>Content</p><footer>Footer stuff</footer>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Content"));
        assert!(!text.contains("Nav stuff"));
        assert!(!text.contains("Footer stuff"));
    }

    #[test]
    fn strip_html_converts_br_to_newline() {
        let html = "Line 1<br>Line 2<br/>Line 3";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Line 1\nLine 2\nLine 3"));
    }

    #[test]
    fn strip_html_converts_li_to_bullets() {
        let html = "<ul><li>First</li><li>Second</li><li>Third</li></ul>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("• First"));
        assert!(text.contains("• Second"));
        assert!(text.contains("• Third"));
    }

    #[test]
    fn strip_html_headings() {
        let html = "<h1>Title</h1><p>Content</p><h2>Subtitle</h2>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Title"));
        assert!(text.contains("Content"));
        assert!(text.contains("Subtitle"));
    }

    #[test]
    fn strip_html_decodes_entities() {
        let html = "<p>5 &gt; 3 &amp; 2 &lt; 4</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("5 > 3 & 2 < 4"));
    }

    #[test]
    fn strip_html_decodes_numeric_entities() {
        let html = "<p>&#65;&#66;&#67;</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("ABC"));
    }

    #[test]
    fn strip_html_decodes_quotes_and_apostrophes() {
        let html = "<p>&quot;hello&quot; &amp; &apos;world&apos;</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("\"hello\" & 'world'"));
    }

    #[test]
    fn strip_html_collapses_whitespace() {
        let html = "<p>Hello</p>   \n\n\n\n\n   <p>World</p>";
        let text = strip_html_tags(html, 5000);
        // Should not have more than 2 consecutive newlines
        assert!(!text.contains("\n\n\n"));
    }

    #[test]
    fn strip_html_truncates_long_content() {
        let html = "<p>".to_string() + &"x".repeat(6000) + "</p>";
        let text = strip_html_tags(&html, 100);
        assert!(text.len() < 200); // truncated text + suffix
        assert!(text.contains("[… truncated at 100 chars]"));
    }

    #[test]
    fn strip_html_empty_input() {
        let text = strip_html_tags("", 5000);
        assert_eq!(text, "");
    }

    #[test]
    fn strip_html_no_tags() {
        let text = strip_html_tags("Just plain text", 5000);
        assert_eq!(text, "Just plain text");
    }

    #[test]
    fn strip_html_nested_tags() {
        let html = "<div><p>Inside <strong>bold</strong> and <em>italic</em></p></div>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Inside bold and italic"));
    }

    #[test]
    fn strip_html_case_insensitive_tags() {
        let html = "<SCRIPT>bad</SCRIPT><P>Good</P>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Good"));
        assert!(!text.contains("bad"));
    }

    #[test]
    fn strip_html_nbsp() {
        let html = "<p>word&nbsp;word</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("word word"));
    }

    #[test]
    fn strip_html_non_ascii_content() {
        // Common non-ASCII characters: middle dot, em dash, accented letters
        let html = "<p>Price · $10 — café résumé</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("·"), "Should preserve middle dot");
        assert!(text.contains("—"), "Should preserve em dash");
        assert!(text.contains("café"), "Should preserve accented chars");
        assert!(text.contains("résumé"), "Should preserve accented chars");
    }

    #[test]
    fn strip_html_non_ascii_in_skip_tag() {
        // Non-ASCII inside script tags should not panic
        let html = "<p>Before</p><script>alert('café — naïve')</script><p>After</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        assert!(!text.contains("café"));
    }

    #[test]
    fn strip_html_chinese_japanese() {
        let html = "<p>中文测试</p><div>日本語テスト</div>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("中文测试"), "Should preserve Chinese");
        assert!(text.contains("日本語テスト"), "Should preserve Japanese");
    }

    #[test]
    fn strip_html_mixed_multibyte() {
        // Mix of ASCII and multi-byte throughout, including emoji
        let html = "<h1>Hello 🌍 World</h1><p>naïve · recipe — Pro™</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Hello 🌍 World"), "Should preserve emoji");
        assert!(text.contains("naïve"), "Should preserve accented chars");
        assert!(text.contains("·"), "Should preserve middle dot");
        assert!(text.contains("—"), "Should preserve em dash");
        assert!(text.contains("Pro™"), "Should preserve trademark");
    }

    #[test]
    fn strip_html_emoji_in_tags() {
        let html = "<li>🎉 Party</li><li>🚀 Launch</li>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("🎉 Party"));
        assert!(text.contains("🚀 Launch"));
    }

    #[test]
    fn strip_html_non_ascii_truncation() {
        // Ensure truncation with non-ASCII doesn't panic
        let html = "<p>".to_string() + &"café ".repeat(1000) + "</p>";
        let text = strip_html_tags(&html, 100);
        assert!(text.contains("[… truncated at 100 chars]"));
    }

    #[test]
    fn valid_urls() {
        assert!(is_valid_url("https://example.com"));
        assert!(is_valid_url("http://docs.rs/yoagent"));
        assert!(is_valid_url(
            "https://doc.rust-lang.org/book/ch01-01-installation.html"
        ));
    }

    #[test]
    fn invalid_urls() {
        assert!(!is_valid_url("not-a-url"));
        assert!(!is_valid_url("ftp://files.com"));
        assert!(!is_valid_url("https://"));
        assert!(!is_valid_url("http://x"));
        assert!(!is_valid_url(""));
    }

    #[test]
    fn test_extract_last_code_block_single() {
        let md = "Here is some code:\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n";
        let block = extract_last_code_block(md).unwrap();
        assert!(block.contains("fn main()"));
        assert!(block.contains("println!"));
        assert!(!block.contains("```"));
    }

    #[test]
    fn test_extract_last_code_block_multiple() {
        let md = "First:\n```\nfirst block\n```\nSecond:\n```python\nprint('hi')\n```\n";
        let block = extract_last_code_block(md).unwrap();
        assert_eq!(block, "print('hi')");
    }

    #[test]
    fn test_extract_last_code_block_none() {
        let md = "No code blocks here, just text.";
        assert!(extract_last_code_block(md).is_none());
    }

    #[test]
    fn test_extract_last_code_block_unclosed() {
        // Unclosed fence should not produce a block
        let md = "```rust\nfn foo() {}\n";
        assert!(extract_last_code_block(md).is_none());
    }

    #[test]

    fn test_extract_last_assistant_text_finds_last() {
        use yoagent::*;
        let messages = vec![
            AgentMessage::Llm(Message::User {
                content: vec![Content::Text {
                    text: "hello".into(),
                }],
                timestamp: 0,
            }),
            AgentMessage::Llm(Message::Assistant {
                content: vec![Content::Text {
                    text: "first response".into(),
                }],
                stop_reason: StopReason::Stop,
                model: "test".into(),
                provider: "test".into(),
                usage: Usage::default(),
                timestamp: 1,
                error_message: None,
            }),
            AgentMessage::Llm(Message::User {
                content: vec![Content::Text {
                    text: "followup".into(),
                }],
                timestamp: 2,
            }),
            AgentMessage::Llm(Message::Assistant {
                content: vec![Content::Text {
                    text: "second response".into(),
                }],
                stop_reason: StopReason::Stop,
                model: "test".into(),
                provider: "test".into(),
                usage: Usage::default(),
                timestamp: 3,
                error_message: None,
            }),
        ];
        let text = extract_last_assistant_text(&messages).unwrap();
        assert_eq!(text, "second response");
    }

    #[test]
    fn test_extract_last_assistant_text_empty() {
        let messages: Vec<yoagent::AgentMessage> = vec![];
        assert!(extract_last_assistant_text(&messages).is_none());
    }

    #[test]
    fn test_extract_last_assistant_text_no_assistant() {
        use yoagent::*;
        let messages = vec![AgentMessage::Llm(Message::User {
            content: vec![Content::Text {
                text: "hello".into(),
            }],
            timestamp: 0,
        })];
        assert!(extract_last_assistant_text(&messages).is_none());
    }

    #[test]

    fn test_clipboard_command_platform() {
        // We can't test what clipboard_command returns deterministically
        // across CI environments, but we can verify it doesn't panic
        // and returns a valid shape.
        let result = super::clipboard_command();
        // On CI (Linux without display), it may return None — that's fine.
        if let Some((cmd, args)) = result {
            assert!(!cmd.is_empty());
            // args can be empty (pbcopy, clip.exe) or non-empty (xclip)
            let _ = args;
        }
    }

    #[test]
    fn test_is_valid_url_detection() {
        // URLs should be detected
        assert!(is_valid_url("https://docs.rs/some-crate"));
        assert!(is_valid_url("http://example.com"));
        assert!(is_valid_url(
            "https://doc.rust-lang.org/book/ch01-01-installation.html"
        ));

        // Regular file paths should NOT be detected as URLs
        assert!(!is_valid_url("src/main.rs"));
        assert!(!is_valid_url("Cargo.toml"));
        assert!(!is_valid_url("src/*.rs"));
        assert!(!is_valid_url("./relative/path.txt"));
        assert!(!is_valid_url(""));
        assert!(!is_valid_url("http://x")); // too short, no dot
    }

    // -----------------------------------------------------------------------
    // Web search tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_web_search_result_construction() {
        let result = WebSearchResult {
            title: "Rust Programming Language".to_string(),
            url: "https://www.rust-lang.org/".to_string(),
            snippet: "A language empowering everyone to build reliable software.".to_string(),
        };
        assert_eq!(result.title, "Rust Programming Language");
        assert_eq!(result.url, "https://www.rust-lang.org/");
        assert_eq!(
            result.snippet,
            "A language empowering everyone to build reliable software."
        );
    }

    #[test]
    fn test_url_encode_basic() {
        assert_eq!(url_encode("hello world"), "hello+world");
        assert_eq!(url_encode("rust lang"), "rust+lang");
        assert_eq!(url_encode("simple"), "simple");
    }

    #[test]
    fn test_url_encode_special_chars() {
        assert_eq!(url_encode("a&b=c"), "a%26b%3Dc");
        assert_eq!(url_encode("foo bar?baz"), "foo+bar%3Fbaz");
        assert_eq!(url_encode("100%"), "100%25");
    }

    #[test]
    fn test_url_encode_preserves_unreserved() {
        assert_eq!(url_encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(url_encode("ABC123"), "ABC123");
    }

    #[test]
    fn test_url_decode_basic() {
        assert_eq!(url_decode("hello+world"), "hello world");
        assert_eq!(url_decode("hello%20world"), "hello world");
        assert_eq!(url_decode("simple"), "simple");
    }

    #[test]
    fn test_url_decode_special_chars() {
        assert_eq!(url_decode("a%26b%3Dc"), "a&b=c");
        assert_eq!(
            url_decode("https%3A%2F%2Fexample.com%2Fpage"),
            "https://example.com/page"
        );
    }

    #[test]
    fn test_extract_ddg_url_with_uddg() {
        let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F&rut=abc123";
        assert_eq!(extract_ddg_url(href), "https://www.rust-lang.org/");
    }

    #[test]
    fn test_extract_ddg_url_with_uddg_no_other_params() {
        let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpath";
        assert_eq!(extract_ddg_url(href), "https://example.com/path");
    }

    #[test]
    fn test_extract_ddg_url_plain_url() {
        // If no uddg parameter, return as-is (with // → https:)
        let href = "//example.com/direct";
        assert_eq!(extract_ddg_url(href), "https://example.com/direct");
    }

    #[test]
    fn test_extract_ddg_url_absolute() {
        let href = "https://example.com/page";
        assert_eq!(extract_ddg_url(href), "https://example.com/page");
    }

    #[test]
    fn test_extract_attr_double_quote() {
        let tag = r#"<a class="result__a" href="/l/?uddg=test">"#;
        assert_eq!(extract_attr(tag, "href"), Some("/l/?uddg=test".to_string()));
        assert_eq!(extract_attr(tag, "class"), Some("result__a".to_string()));
    }

    #[test]
    fn test_extract_attr_single_quote() {
        let tag = "<a class='result__a' href='/l/?uddg=test'>";
        assert_eq!(extract_attr(tag, "href"), Some("/l/?uddg=test".to_string()));
    }

    #[test]
    fn test_extract_attr_missing() {
        let tag = "<a class=\"foo\">";
        assert_eq!(extract_attr(tag, "href"), None);
    }

    #[test]
    fn test_extract_inner_text_strips_tags() {
        assert_eq!(
            extract_inner_text("<b>Hello</b> <i>world</i>"),
            "Hello world"
        );
    }

    #[test]
    fn test_extract_inner_text_decodes_entities() {
        assert_eq!(
            extract_inner_text("foo &amp; bar &lt;baz&gt;"),
            "foo & bar <baz>"
        );
    }

    #[test]
    fn test_parse_ddg_results_realistic() {
        // Realistic DuckDuckGo HTML fixture
        let html = r#"
<div class="serp__results">
  <div class="result results_links results_links_deep web-result">
    <div class="result__body links_main links_deep">
      <h2 class="result__title">
        <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F&amp;rut=abc">Rust Programming Language</a>
      </h2>
      <a class="result__snippet" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F&amp;rut=abc">A language empowering everyone to build <b>reliable</b> and efficient software.</a>
    </div>
  </div>
  <div class="result results_links results_links_deep web-result">
    <div class="result__body links_main links_deep">
      <h2 class="result__title">
        <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fdoc.rust-lang.org%2Fbook%2F&amp;rut=def">The Rust Programming Language - Book</a>
      </h2>
      <a class="result__snippet" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fdoc.rust-lang.org%2Fbook%2F&amp;rut=def">An introductory book about Rust.</a>
    </div>
  </div>
  <div class="result results_links results_links_deep web-result">
    <div class="result__body links_main links_deep">
      <h2 class="result__title">
        <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fcrates.io%2F&amp;rut=ghi">crates.io: Rust Package Registry</a>
      </h2>
      <td class="result__snippet">The Rust community&apos;s crate registry.</td>
    </div>
  </div>
</div>
"#;

        let results = parse_ddg_results(html, 10);
        assert_eq!(results.len(), 3);

        assert_eq!(results[0].title, "Rust Programming Language");
        assert_eq!(results[0].url, "https://www.rust-lang.org/");
        assert!(results[0].snippet.contains("reliable"));
        assert!(results[0].snippet.contains("efficient software"));

        assert_eq!(results[1].title, "The Rust Programming Language - Book");
        assert_eq!(results[1].url, "https://doc.rust-lang.org/book/");
        assert_eq!(results[1].snippet, "An introductory book about Rust.");

        assert_eq!(results[2].title, "crates.io: Rust Package Registry");
        assert_eq!(results[2].url, "https://crates.io/");
        // Snippet from <td> tag with entity decoding
        assert!(results[2].snippet.contains("Rust community"));
        assert!(results[2].snippet.contains("crate registry"));
    }

    #[test]
    fn test_parse_ddg_results_max_results_cap() {
        // Build HTML with many results
        let mut html = String::from("<div>");
        for i in 0..25 {
            html.push_str(&format!(
                "<a class=\"result__a\" href=\"//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2F{i}\">Result {i}</a>\n\
                 <a class=\"result__snippet\" href=\"x\">Snippet {i}</a>\n"
            ));
        }
        html.push_str("</div>");

        // Request 5
        let results = parse_ddg_results(&html, 5);
        assert_eq!(results.len(), 5);

        // Request 30 — should cap at 20
        let results = parse_ddg_results(&html, 30);
        assert_eq!(results.len(), 20);
    }

    #[test]
    fn test_parse_ddg_results_empty_html() {
        let html = "<html><body><p>No results</p></body></html>";
        let results = parse_ddg_results(html, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_ddg_results_no_snippet() {
        let html = r#"
<div class="result">
  <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com">Example Title</a>
</div>
"#;
        let results = parse_ddg_results(html, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example Title");
        assert_eq!(results[0].url, "https://example.com");
        assert!(results[0].snippet.is_empty());
    }

    #[test]
    fn test_format_search_results_basic() {
        let results = vec![
            WebSearchResult {
                title: "Rust Lang".to_string(),
                url: "https://www.rust-lang.org/".to_string(),
                snippet: "A systems programming language.".to_string(),
            },
            WebSearchResult {
                title: "Crates.io".to_string(),
                url: "https://crates.io/".to_string(),
                snippet: "The Rust package registry.".to_string(),
            },
        ];

        let formatted = format_search_results(&results);
        assert!(formatted.contains("1. Rust Lang"));
        assert!(formatted.contains("   https://www.rust-lang.org/"));
        assert!(formatted.contains("   A systems programming language."));
        assert!(formatted.contains("2. Crates.io"));
        assert!(formatted.contains("   https://crates.io/"));
        assert!(formatted.contains("   The Rust package registry."));
    }

    #[test]
    fn test_format_search_results_empty() {
        let results: Vec<WebSearchResult> = vec![];
        let formatted = format_search_results(&results);
        assert_eq!(formatted, "No results found.");
    }

    #[test]
    fn test_format_search_results_no_snippet() {
        let results = vec![WebSearchResult {
            title: "Example".to_string(),
            url: "https://example.com".to_string(),
            snippet: String::new(),
        }];
        let formatted = format_search_results(&results);
        assert!(formatted.contains("1. Example"));
        assert!(formatted.contains("   https://example.com"));
        // Should not have a third indented line for empty snippet
        let lines: Vec<&str> = formatted.lines().collect();
        assert_eq!(lines.len(), 2); // title + url only
    }

    #[test]
    fn test_url_encode_roundtrip() {
        let original = "rust programming language + guide";
        let encoded = url_encode(original);
        let decoded = url_decode(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_parse_ddg_results_with_bold_in_snippet() {
        // DDG often wraps search terms in <b> tags within snippets
        let html = r##"
<a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com">Test Page</a>
<a class="result__snippet" href="#">This is a <b>test</b> snippet with <b>bold</b> words.</a>
"##;
        let results = parse_ddg_results(html, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].snippet,
            "This is a test snippet with bold words."
        );
    }
}
