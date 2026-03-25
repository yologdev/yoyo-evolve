//! File operation command handlers: /add, /apply, /web, @file mentions.

use crate::format::*;

use std::io::IsTerminal;

// ── /web ─────────────────────────────────────────────────────────────────

/// Maximum characters to display from a fetched web page.
const WEB_MAX_CHARS: usize = 5000;

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
fn fetch_url(url: &str) -> Result<String, String> {
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
pub fn handle_web(input: &str) {
    let url = input.trim_start_matches("/web").trim();

    if url.is_empty() {
        println!("{DIM}  usage: /web <url>");
        println!("  Fetch a web page and display readable text content.");
        println!(
            "  Example: /web https://doc.rust-lang.org/book/ch01-01-installation.html{RESET}\n"
        );
        return;
    }

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

// ── /add ─────────────────────────────────────────────────────────────────

/// Parse an `/add` argument into a file path and optional line range.
///
/// Supports:
///   - `path/to/file.rs` → ("path/to/file.rs", None)
///   - `path/to/file.rs:10-20` → ("path/to/file.rs", Some((10, 20)))
///
/// Only recognizes `:<digits>-<digits>` at the end as a line range.
pub fn parse_add_arg(arg: &str) -> (&str, Option<(usize, usize)>) {
    // Look for the last colon that's followed by digits-digits
    if let Some(colon_pos) = arg.rfind(':') {
        let after = &arg[colon_pos + 1..];
        if let Some(dash_pos) = after.find('-') {
            let start_str = &after[..dash_pos];
            let end_str = &after[dash_pos + 1..];
            if let (Ok(start), Ok(end)) = (start_str.parse::<usize>(), end_str.parse::<usize>()) {
                if start > 0 && end >= start {
                    return (&arg[..colon_pos], Some((start, end)));
                }
            }
        }
    }
    (arg, None)
}

/// Expand a path argument that may contain glob patterns.
/// Returns the original path as-is if it has no glob characters.
pub fn expand_add_paths(pattern: &str) -> Vec<String> {
    if !pattern.contains('*') && !pattern.contains('?') && !pattern.contains('[') {
        return vec![pattern.to_string()];
    }
    match glob::glob(pattern) {
        Ok(paths) => {
            let mut result: Vec<String> = paths
                .filter_map(|p| p.ok())
                .filter(|p| p.is_file())
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            result.sort();
            result
        }
        Err(_) => Vec::new(),
    }
}

/// Read a file (optionally a line range) for the /add command.
/// Returns the file content and line count.
pub fn read_file_for_add(
    path: &str,
    range: Option<(usize, usize)>,
) -> Result<(String, usize), String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("could not read {path}: {e}"))?;

    match range {
        Some((start, end)) => {
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();
            if start > total {
                return Err(format!(
                    "start line {start} is past end of file ({total} lines)"
                ));
            }
            let end = end.min(total);
            let selected: Vec<&str> = lines[start - 1..end].to_vec();
            let count = selected.len();
            Ok((selected.join("\n"), count))
        }
        None => {
            let count = content.lines().count();
            Ok((content, count))
        }
    }
}

/// Format file content for injection into the conversation.
/// Wraps it in a markdown code block with the filename as header.
pub fn format_add_content(path: &str, content: &str) -> String {
    // Detect language extension for syntax highlighting
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let lang = match ext {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "rb" => "ruby",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" | "cxx" => "cpp",
        "sh" | "bash" => "bash",
        "yml" | "yaml" => "yaml",
        "json" => "json",
        "toml" => "toml",
        "md" => "markdown",
        "html" | "htm" => "html",
        "css" => "css",
        "sql" => "sql",
        "xml" => "xml",
        _ => "",
    };
    format!("**{path}**\n```{lang}\n{content}\n```")
}

// ── Image support helpers ─────────────────────────────────────────────

/// Check if a file path has an image extension.
pub fn is_image_extension(path: &str) -> bool {
    let lower = path.to_lowercase();
    matches!(
        lower.rsplit('.').next(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
    )
}

/// Map a file extension to a MIME type string.
/// Returns `"application/octet-stream"` for unknown extensions.
pub fn mime_type_for_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// Result type for `/add` that distinguishes text files from image files.
#[derive(Debug, Clone, PartialEq)]
pub enum AddResult {
    /// A text file: summary line + formatted content to inject.
    Text { summary: String, content: String },
    /// An image file: summary line + base64-encoded data + MIME type.
    Image {
        summary: String,
        data: String,
        mime_type: String,
    },
}

/// Read an image file from disk and return base64-encoded data and MIME type.
pub fn read_image_for_add(path: &str) -> Result<(String, String), String> {
    use base64::Engine;
    let bytes = std::fs::read(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    let ext = path.rsplit('.').next().unwrap_or("");
    let mime = mime_type_for_extension(ext).to_string();
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok((data, mime))
}

/// Handle the `/add` command: read file(s) and return the formatted content
/// to be injected as a user message.
///
/// Returns a Vec of `AddResult` — either text or image — for each file.
pub fn handle_add(input: &str) -> Vec<AddResult> {
    let args = input.strip_prefix("/add").unwrap_or("").trim();

    if args.is_empty() {
        println!("{DIM}  usage: /add <path> — inject file contents into conversation");
        println!("         /add <path>:<start>-<end> — inject specific line range");
        println!("         /add src/*.rs — inject multiple files via glob{RESET}\n");
        return Vec::new();
    }

    let mut results = Vec::new();

    // Split on whitespace to support multiple paths: /add foo.rs bar.rs
    for arg in args.split_whitespace() {
        let (raw_path, range) = parse_add_arg(arg);
        let paths = expand_add_paths(raw_path);

        if paths.is_empty() {
            println!("{RED}  no files matched: {raw_path}{RESET}");
            continue;
        }

        for path in &paths {
            // Check if this is an image file
            if is_image_extension(path) {
                // Line ranges don't apply to images
                if range.is_some() {
                    println!("{RED}  ✗ line ranges not supported for images: {path}{RESET}");
                    continue;
                }
                match read_image_for_add(path) {
                    Ok((data, mime_type)) => {
                        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                        let size_str = if size >= 1_048_576 {
                            format!("{:.1} MB", size as f64 / 1_048_576.0)
                        } else {
                            format!("{:.0} KB", size as f64 / 1024.0)
                        };
                        let summary = format!(
                            "{GREEN}  ✓ added image {path} ({size_str}, {mime_type}){RESET}"
                        );
                        results.push(AddResult::Image {
                            summary,
                            data,
                            mime_type,
                        });
                    }
                    Err(e) => {
                        println!("{RED}  ✗ {e}{RESET}");
                    }
                }
                continue;
            }

            match read_file_for_add(path, range) {
                Ok((content, line_count)) => {
                    let formatted = format_add_content(path, &content);
                    let word = crate::format::pluralize(line_count, "line", "lines");
                    let range_info = if let Some((s, e)) = range {
                        format!(" (lines {s}-{e})")
                    } else {
                        String::new()
                    };
                    let summary =
                        format!("{GREEN}  ✓ added {path}{range_info} ({line_count} {word}){RESET}");
                    results.push(AddResult::Text {
                        summary,
                        content: formatted,
                    });
                }
                Err(e) => {
                    println!("{RED}  ✗ {e}{RESET}");
                }
            }
        }
    }

    results
}

// ── @file mention expansion ──────────────────────────────────────────

/// Scan user input for `@path` mentions (e.g. `@src/main.rs` or
/// `@src/cli.rs:50-100`) and resolve them to file contents.
///
/// Returns:
/// - The cleaned prompt text (with resolved `@path` replaced by just the filename)
/// - A vec of `AddResult` items for every file that was successfully read
///
/// Mentions that don't resolve to an existing file are left unchanged
/// (they might be usernames or other references). Email-like patterns
/// (`word@domain`) are skipped.
pub fn expand_file_mentions(input: &str) -> (String, Vec<AddResult>) {
    let mut results = Vec::new();
    let mut output = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] != '@' {
            output.push(chars[i]);
            i += 1;
            continue;
        }

        // Found an '@'. Check if it's email-like (preceded by an alphanumeric char).
        if i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '.' || chars[i - 1] == '_') {
            // Email-like: word@domain — leave it alone
            output.push('@');
            i += 1;
            continue;
        }

        // Collect the path after '@': alphanumeric, '/', '.', '-', '_', ':'
        let start = i + 1;
        let mut j = start;
        while j < len
            && (chars[j].is_alphanumeric() || matches!(chars[j], '/' | '.' | '-' | '_' | ':'))
        {
            j += 1;
        }

        // Nothing after '@' (just @ at end, or @ followed by space)
        if j == start {
            output.push('@');
            i += 1;
            continue;
        }

        let mention = &input[byte_offset(&chars, start)..byte_offset(&chars, j)];

        // Parse path and optional line range using existing helper
        let (raw_path, range) = parse_add_arg(mention);

        // Check if the file exists
        let path = std::path::Path::new(raw_path);
        if !path.is_file() {
            // Not a file — leave the mention unchanged
            output.push('@');
            output.push_str(mention);
            i = j;
            continue;
        }

        // It's a real file — read it
        if is_image_extension(raw_path) {
            if range.is_some() {
                // Line ranges don't apply to images — leave unchanged
                output.push('@');
                output.push_str(mention);
                i = j;
                continue;
            }
            match read_image_for_add(raw_path) {
                Ok((data, mime_type)) => {
                    let size = std::fs::metadata(raw_path).map(|m| m.len()).unwrap_or(0);
                    let size_str = if size >= 1_048_576 {
                        format!("{:.1} MB", size as f64 / 1_048_576.0)
                    } else {
                        format!("{:.0} KB", size as f64 / 1024.0)
                    };
                    let summary = format!(
                        "{GREEN}  ✓ added image {raw_path} ({size_str}, {mime_type}){RESET}"
                    );
                    results.push(AddResult::Image {
                        summary,
                        data,
                        mime_type,
                    });
                    // Replace @path with just the filename in output
                    let filename = path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| raw_path.to_string());
                    output.push_str(&filename);
                }
                Err(_) => {
                    // Read failed — leave unchanged
                    output.push('@');
                    output.push_str(mention);
                }
            }
        } else {
            match read_file_for_add(raw_path, range) {
                Ok((content, line_count)) => {
                    let formatted = format_add_content(raw_path, &content);
                    let word = crate::format::pluralize(line_count, "line", "lines");
                    let range_info = if let Some((s, e)) = range {
                        format!(" (lines {s}-{e})")
                    } else {
                        String::new()
                    };
                    let summary = format!(
                        "{GREEN}  ✓ added {raw_path}{range_info} ({line_count} {word}){RESET}"
                    );
                    results.push(AddResult::Text {
                        summary,
                        content: formatted,
                    });
                    // Replace @path with just the filename in output
                    let filename = path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| raw_path.to_string());
                    if let Some((s, e)) = range {
                        output.push_str(&format!("{filename}:{s}-{e}"));
                    } else {
                        output.push_str(&filename);
                    }
                }
                Err(_) => {
                    // Read failed — leave unchanged
                    output.push('@');
                    output.push_str(mention);
                }
            }
        }

        i = j;
    }

    (output, results)
}

/// Helper: get the byte offset corresponding to a char index.
fn byte_offset(chars: &[char], char_idx: usize) -> usize {
    chars[..char_idx].iter().map(|c| c.len_utf8()).sum()
}

// ── /apply ──────────────────────────────────────────────────────────────

/// Tab-completion flags for `/apply`.
pub const APPLY_FLAGS: &[&str] = &["--check"];

/// Parsed arguments for the `/apply` command.
#[derive(Debug, PartialEq)]
pub struct ApplyArgs {
    /// Path to the patch file (None if reading from stdin).
    pub file: Option<String>,
    /// Dry-run mode: show what would change without applying.
    pub check_only: bool,
}

/// Parse `/apply` arguments.
///
/// Accepted forms:
///   /apply                     — no file (read from stdin or show usage)
///   /apply patch.diff          — apply the given patch file
///   /apply --check patch.diff  — dry-run
///   /apply patch.diff --check  — dry-run (flag can be before or after file)
pub fn parse_apply_args(input: &str) -> ApplyArgs {
    let rest = input.strip_prefix("/apply").unwrap_or("").trim();

    if rest.is_empty() {
        return ApplyArgs {
            file: None,
            check_only: false,
        };
    }

    let parts: Vec<&str> = rest.split_whitespace().collect();
    let mut check_only = false;
    let mut file: Option<String> = None;

    for part in &parts {
        if *part == "--check" {
            check_only = true;
        } else if file.is_none() {
            file = Some(part.to_string());
        }
    }

    ApplyArgs { file, check_only }
}

/// Apply a patch file using `git apply`. Returns `(success, output_message)`.
pub fn apply_patch(path: &str, check_only: bool) -> (bool, String) {
    use std::process::Command;

    // Verify file exists
    if !std::path::Path::new(path).exists() {
        return (false, format!("Patch file not found: {path}"));
    }

    // First get stat output to show a summary
    let stat_result = Command::new("git").args(["apply", "--stat", path]).output();

    let stat_text = match &stat_result {
        Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
        Err(_) => String::new(),
    };

    // Run the actual apply (or check)
    let mut args = vec!["apply"];
    if check_only {
        args.push("--check");
    }
    args.push(path);

    match Command::new("git").args(&args).output() {
        Ok(output) => {
            if output.status.success() {
                let mut msg = String::new();
                if check_only {
                    msg.push_str("Dry-run OK — patch can be applied cleanly.\n");
                } else {
                    msg.push_str("Patch applied successfully.\n");
                }
                if !stat_text.is_empty() {
                    msg.push_str("\nFiles affected:\n");
                    msg.push_str(&stat_text);
                }
                (true, msg)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let mut msg = String::new();
                if check_only {
                    msg.push_str("Dry-run FAILED — patch cannot be applied cleanly.\n");
                } else {
                    msg.push_str("Failed to apply patch.\n");
                }
                if !stderr.is_empty() {
                    msg.push_str(&stderr);
                }
                (false, msg)
            }
        }
        Err(e) => (false, format!("Failed to run git apply: {e}")),
    }
}

/// Apply a patch from string content. Writes to a temp file, applies, then cleans up.
/// Returns `(success, output_message)`.
pub fn apply_patch_from_string(patch: &str, check_only: bool) -> (bool, String) {
    if patch.trim().is_empty() {
        return (false, "Empty patch content — nothing to apply.".to_string());
    }

    // Write to a temp file
    let tmp_dir = std::env::temp_dir();
    let tmp_path = tmp_dir.join("yoyo_apply_patch.tmp");
    let tmp_str = tmp_path.to_string_lossy().to_string();

    if let Err(e) = std::fs::write(&tmp_path, patch) {
        return (false, format!("Failed to write temp patch file: {e}"));
    }

    let result = apply_patch(&tmp_str, check_only);

    // Clean up temp file
    let _ = std::fs::remove_file(&tmp_path);

    result
}

/// Handle the `/apply` REPL command.
pub fn handle_apply(input: &str) {
    let args = parse_apply_args(input);

    match args.file {
        Some(path) => {
            let mode = if args.check_only {
                "Checking"
            } else {
                "Applying"
            };
            println!("{DIM}  {mode} patch: {path}{RESET}");

            let (ok, msg) = apply_patch(&path, args.check_only);
            if ok {
                println!("{GREEN}  {msg}{RESET}");
            } else {
                println!("{YELLOW}  {msg}{RESET}");
            }
        }
        None => {
            // No file provided — check if stdin is piped
            if std::io::stdin().is_terminal() {
                // Interactive mode: show usage
                println!("{DIM}  Usage: /apply <file>        Apply a patch file");
                println!("         /apply --check <file>  Dry-run (show what would change)");
                println!("         cat patch.diff | yoyo  Pipe patch via stdin (non-interactive){RESET}\n");
            } else {
                // Piped mode: read patch from stdin
                use std::io::Read;
                let mut patch = String::new();
                match std::io::stdin().read_to_string(&mut patch) {
                    Ok(_) => {
                        let (ok, msg) = apply_patch_from_string(&patch, args.check_only);
                        if ok {
                            println!("{GREEN}  {msg}{RESET}");
                        } else {
                            println!("{YELLOW}  {msg}{RESET}");
                        }
                    }
                    Err(e) => {
                        println!("{YELLOW}  Failed to read patch from stdin: {e}{RESET}\n");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::KNOWN_COMMANDS;
    use crate::help::help_text;
    use std::fs;
    use tempfile::TempDir;

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

    // ── is_valid_url ────────────────────────────────────────────────

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

    // ── /add command tests ────────────────────────────────────────────

    #[test]
    fn parse_add_arg_simple_path() {
        let (path, range) = parse_add_arg("src/main.rs");
        assert_eq!(path, "src/main.rs");
        assert!(range.is_none());
    }

    #[test]
    fn parse_add_arg_with_line_range() {
        let (path, range) = parse_add_arg("src/main.rs:10-20");
        assert_eq!(path, "src/main.rs");
        assert_eq!(range, Some((10, 20)));
    }

    #[test]
    fn parse_add_arg_with_single_line() {
        let (path, range) = parse_add_arg("src/main.rs:42-42");
        assert_eq!(path, "src/main.rs");
        assert_eq!(range, Some((42, 42)));
    }

    #[test]
    fn parse_add_arg_with_colon_in_path_no_range() {
        // A colon followed by non-numeric text should not be treated as a range
        let (path, range) = parse_add_arg("C:/Users/test.rs");
        assert_eq!(path, "C:/Users/test.rs");
        assert!(range.is_none());
    }

    #[test]
    fn parse_add_arg_windows_path_with_range() {
        // Windows-style: C:/foo/bar.rs:5-10 — colon after drive letter
        let (path, range) = parse_add_arg("foo/bar.rs:5-10");
        assert_eq!(path, "foo/bar.rs");
        assert_eq!(range, Some((5, 10)));
    }

    #[test]
    fn format_add_content_basic() {
        let content = format_add_content("hello.txt", "hello world\n");
        assert!(content.contains("hello.txt"));
        assert!(content.contains("```"));
        assert!(content.contains("hello world"));
    }

    #[test]
    fn format_add_content_wraps_in_code_block() {
        let content = format_add_content("test.rs", "fn main() {}\n");
        // Should have opening and closing code fences
        let fences: Vec<&str> = content.lines().filter(|l| l.starts_with("```")).collect();
        assert_eq!(fences.len(), 2, "Should have exactly 2 code fences");
    }

    #[test]
    fn expand_add_globs_no_glob() {
        let paths = expand_add_paths("src/main.rs");
        assert_eq!(paths, vec!["src/main.rs".to_string()]);
    }

    #[test]
    fn expand_add_globs_with_glob() {
        // This tests with a real glob pattern against the project
        let paths = expand_add_paths("src/*.rs");
        assert!(!paths.is_empty(), "Should match at least one .rs file");
        for p in &paths {
            assert!(p.ends_with(".rs"), "All matches should be .rs files: {p}");
            assert!(p.starts_with("src/"), "All matches should be in src/: {p}");
        }
    }

    #[test]
    fn expand_add_globs_no_matches() {
        let paths = expand_add_paths("nonexistent_dir_xyz/*.zzz");
        assert!(paths.is_empty(), "Non-matching glob should return empty");
    }

    #[test]
    fn add_read_file_with_range() {
        // Read our own source with a line range
        let result = read_file_for_add("src/commands_project.rs", Some((1, 3)));
        assert!(result.is_ok());
        let (content, count) = result.unwrap();
        assert_eq!(count, 3);
        assert!(!content.is_empty());
    }

    #[test]
    fn add_read_file_full() {
        let result = read_file_for_add("Cargo.toml", None);
        assert!(result.is_ok());
        let (content, count) = result.unwrap();
        assert!(count > 0);
        assert!(content.contains("[package]"));
    }

    #[test]
    fn add_read_file_not_found() {
        let result = read_file_for_add("definitely_not_a_real_file.xyz", None);
        assert!(result.is_err());
    }

    // ── is_image_extension ────────────────────────────────────────────

    #[test]
    fn is_image_extension_supported_formats() {
        assert!(is_image_extension("photo.png"));
        assert!(is_image_extension("photo.jpg"));
        assert!(is_image_extension("photo.jpeg"));
        assert!(is_image_extension("photo.gif"));
        assert!(is_image_extension("photo.webp"));
        assert!(is_image_extension("photo.bmp"));
    }

    #[test]
    fn is_image_extension_case_insensitive() {
        assert!(is_image_extension("photo.PNG"));
        assert!(is_image_extension("image.Jpg"));
        assert!(is_image_extension("banner.JPEG"));
        assert!(is_image_extension("icon.GIF"));
        assert!(is_image_extension("pic.WeBp"));
        assert!(is_image_extension("scan.BMP"));
    }

    #[test]
    fn is_image_extension_non_image_files() {
        assert!(!is_image_extension("main.rs"));
        assert!(!is_image_extension("notes.txt"));
        assert!(!is_image_extension("README.md"));
        assert!(!is_image_extension("config.json"));
        assert!(!is_image_extension("Cargo.toml"));
        assert!(!is_image_extension("archive.zip"));
    }

    #[test]
    fn is_image_extension_no_extension() {
        assert!(!is_image_extension("Makefile"));
        assert!(!is_image_extension(""));
    }

    #[test]
    fn is_image_extension_with_full_paths() {
        assert!(is_image_extension("src/assets/logo.png"));
        assert!(is_image_extension("/home/user/photos/vacation.jpg"));
        assert!(is_image_extension("../../images/banner.webp"));
        assert!(!is_image_extension("src/main.rs"));
    }

    // ── mime_type_for_extension ───────────────────────────────────────

    #[test]
    fn mime_type_png() {
        assert_eq!(mime_type_for_extension("png"), "image/png");
    }

    #[test]
    fn mime_type_jpg_and_jpeg() {
        assert_eq!(mime_type_for_extension("jpg"), "image/jpeg");
        assert_eq!(mime_type_for_extension("jpeg"), "image/jpeg");
    }

    #[test]
    fn mime_type_gif() {
        assert_eq!(mime_type_for_extension("gif"), "image/gif");
    }

    #[test]
    fn mime_type_webp() {
        assert_eq!(mime_type_for_extension("webp"), "image/webp");
    }

    #[test]
    fn mime_type_bmp() {
        assert_eq!(mime_type_for_extension("bmp"), "image/bmp");
    }

    #[test]
    fn mime_type_unknown_extension() {
        assert_eq!(mime_type_for_extension("zip"), "application/octet-stream");
        assert_eq!(mime_type_for_extension("rs"), "application/octet-stream");
        assert_eq!(mime_type_for_extension(""), "application/octet-stream");
    }

    #[test]
    fn mime_type_case_insensitive() {
        assert_eq!(mime_type_for_extension("PNG"), "image/png");
        assert_eq!(mime_type_for_extension("Jpg"), "image/jpeg");
        assert_eq!(mime_type_for_extension("GIF"), "image/gif");
    }

    // ── AddResult ─────────────────────────────────────────────────────

    #[test]
    fn add_result_text_fields_accessible() {
        let result = AddResult::Text {
            summary: "added foo.rs".to_string(),
            content: "fn main() {}".to_string(),
        };
        match &result {
            AddResult::Text { summary, content } => {
                assert_eq!(summary, "added foo.rs");
                assert_eq!(content, "fn main() {}");
            }
            _ => panic!("expected Text variant"),
        }
    }

    #[test]
    fn add_result_image_fields_accessible() {
        let result = AddResult::Image {
            summary: "added logo.png".to_string(),
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        };
        match &result {
            AddResult::Image {
                summary,
                data,
                mime_type,
            } => {
                assert_eq!(summary, "added logo.png");
                assert_eq!(data, "base64data");
                assert_eq!(mime_type, "image/png");
            }
            _ => panic!("expected Image variant"),
        }
    }

    #[test]
    fn add_result_partial_eq() {
        let a = AddResult::Text {
            summary: "s".to_string(),
            content: "c".to_string(),
        };
        let b = AddResult::Text {
            summary: "s".to_string(),
            content: "c".to_string(),
        };
        let c = AddResult::Text {
            summary: "different".to_string(),
            content: "c".to_string(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);

        let img1 = AddResult::Image {
            summary: "s".to_string(),
            data: "d".to_string(),
            mime_type: "image/png".to_string(),
        };
        let img2 = AddResult::Image {
            summary: "s".to_string(),
            data: "d".to_string(),
            mime_type: "image/png".to_string(),
        };
        assert_eq!(img1, img2);

        // Text != Image even with same summary
        assert_ne!(a, img1);
    }

    // ── read_image_for_add ────────────────────────────────────────────

    #[test]
    fn read_image_for_add_valid_png() {
        let dir = TempDir::new().unwrap();
        let png_path = dir.path().join("test.png");

        // Minimal valid PNG: 8-byte signature + IHDR chunk (25 bytes) + IEND chunk (12 bytes)
        #[rustfmt::skip]
        let png_bytes: Vec<u8> = vec![
            // PNG signature
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
            // IHDR chunk: length=13
            0x00, 0x00, 0x00, 0x0D,
            // "IHDR"
            0x49, 0x48, 0x44, 0x52,
            // width=1, height=1
            0x00, 0x00, 0x00, 0x01,
            0x00, 0x00, 0x00, 0x01,
            // bit depth=8, color type=2 (RGB), compression=0, filter=0, interlace=0
            0x08, 0x02, 0x00, 0x00, 0x00,
            // IHDR CRC (precalculated for this exact IHDR)
            0x90, 0x77, 0x53, 0xDE,
            // IEND chunk: length=0
            0x00, 0x00, 0x00, 0x00,
            // "IEND"
            0x49, 0x45, 0x4E, 0x44,
            // IEND CRC
            0xAE, 0x42, 0x60, 0x82,
        ];
        fs::write(&png_path, &png_bytes).unwrap();

        let path_str = png_path.to_str().unwrap();
        let result = read_image_for_add(path_str);
        assert!(result.is_ok(), "should succeed reading a valid PNG file");

        let (data, mime_type) = result.unwrap();
        assert!(!data.is_empty(), "base64 data should be non-empty");
        assert_eq!(mime_type, "image/png");

        // Verify the base64 decodes back to the original bytes
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&data)
            .expect("should be valid base64");
        assert_eq!(decoded, png_bytes);
    }

    #[test]
    fn read_image_for_add_nonexistent_file() {
        let result = read_image_for_add("/tmp/definitely_does_not_exist_yoyo_test.png");
        assert!(result.is_err(), "should fail for nonexistent file");
        let err = result.unwrap_err();
        assert!(
            err.contains("failed to read"),
            "error should mention failure: {err}"
        );
    }

    #[test]
    fn read_image_for_add_jpg_mime_type() {
        let dir = TempDir::new().unwrap();
        let jpg_path = dir.path().join("photo.jpg");
        // Just some bytes — we're testing MIME detection, not image validity
        fs::write(&jpg_path, b"fake jpg content").unwrap();

        let (data, mime_type) = read_image_for_add(jpg_path.to_str().unwrap()).unwrap();
        assert!(!data.is_empty());
        assert_eq!(mime_type, "image/jpeg");
    }

    #[test]
    fn read_image_for_add_webp_mime_type() {
        let dir = TempDir::new().unwrap();
        let webp_path = dir.path().join("image.webp");
        fs::write(&webp_path, b"fake webp content").unwrap();

        let (_, mime_type) = read_image_for_add(webp_path.to_str().unwrap()).unwrap();
        assert_eq!(mime_type, "image/webp");
    }

    // ── expand_file_mentions tests ───────────────────────────────────

    #[test]
    fn expand_file_mentions_no_mentions() {
        let (text, results) = expand_file_mentions("hello world, no mentions here");
        assert_eq!(text, "hello world, no mentions here");
        assert!(results.is_empty());
    }

    #[test]
    fn expand_file_mentions_resolves_real_file() {
        // Cargo.toml should exist at the project root
        let (text, results) = expand_file_mentions("explain @Cargo.toml");
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], AddResult::Text { summary, .. } if summary.contains("Cargo.toml"))
        );
        assert_eq!(text, "explain Cargo.toml");
    }

    #[test]
    fn expand_file_mentions_nonexistent_file_unchanged() {
        let (text, results) = expand_file_mentions("look at @nonexistent_xyz_file.rs");
        assert!(results.is_empty());
        assert_eq!(text, "look at @nonexistent_xyz_file.rs");
    }

    #[test]
    fn expand_file_mentions_with_line_range() {
        let (text, results) = expand_file_mentions("review @Cargo.toml:1-3");
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], AddResult::Text { summary, .. } if summary.contains("lines 1-3"))
        );
        assert_eq!(text, "review Cargo.toml:1-3");
    }

    #[test]
    fn expand_file_mentions_multiple_mentions() {
        let (text, results) = expand_file_mentions("compare @Cargo.toml and @LICENSE");
        assert_eq!(results.len(), 2);
        assert_eq!(text, "compare Cargo.toml and LICENSE");
    }

    #[test]
    fn expand_file_mentions_at_end_of_string_no_path() {
        let (text, results) = expand_file_mentions("trailing @");
        assert!(results.is_empty());
        assert_eq!(text, "trailing @");
    }

    #[test]
    fn expand_file_mentions_at_followed_by_space() {
        let (text, results) = expand_file_mentions("hello @ world");
        assert!(results.is_empty());
        assert_eq!(text, "hello @ world");
    }

    #[test]
    fn expand_file_mentions_skips_email_like() {
        let (text, results) = expand_file_mentions("email user@example.com please");
        assert!(results.is_empty());
        assert_eq!(text, "email user@example.com please");
    }

    #[test]
    fn expand_file_mentions_path_with_dirs() {
        // src/main.rs should exist
        let (text, results) = expand_file_mentions("look at @src/main.rs");
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], AddResult::Text { summary, .. } if summary.contains("src/main.rs"))
        );
        assert_eq!(text, "look at main.rs");
    }

    #[test]
    fn expand_file_mentions_mixed_real_and_fake() {
        let (text, results) = expand_file_mentions("@Cargo.toml is real but @fake_abc.rs is not");
        assert_eq!(results.len(), 1);
        assert!(text.contains("Cargo.toml"));
        assert!(text.contains("@fake_abc.rs"));
    }

    // ── /apply tests ────────────────────────────────────────────────────

    #[test]
    fn test_apply_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/apply"),
            "/apply should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_apply_in_help_text() {
        let help = help_text();
        assert!(help.contains("/apply"), "/apply should appear in help text");
    }

    #[test]
    fn test_apply_parse_args_file() {
        let args = parse_apply_args("/apply patch.diff");
        assert_eq!(args.file, Some("patch.diff".to_string()));
        assert!(!args.check_only);
    }

    #[test]
    fn test_apply_parse_args_check() {
        let args = parse_apply_args("/apply --check patch.diff");
        assert_eq!(args.file, Some("patch.diff".to_string()));
        assert!(args.check_only);
    }

    #[test]
    fn test_apply_parse_args_check_after_file() {
        let args = parse_apply_args("/apply patch.diff --check");
        assert_eq!(args.file, Some("patch.diff".to_string()));
        assert!(args.check_only);
    }

    #[test]
    fn test_apply_parse_args_empty() {
        let args = parse_apply_args("/apply");
        assert_eq!(args.file, None);
        assert!(!args.check_only);
    }

    #[test]
    fn test_apply_parse_args_empty_with_spaces() {
        let args = parse_apply_args("/apply   ");
        assert_eq!(args.file, None);
        assert!(!args.check_only);
    }

    #[test]
    fn test_apply_patch_nonexistent_file() {
        let (ok, msg) = apply_patch("nonexistent_patch_file_12345.diff", false);
        assert!(!ok);
        assert!(
            msg.contains("not found"),
            "Expected 'not found', got: {msg}"
        );
    }

    #[test]
    fn test_apply_patch_from_string_empty() {
        let (ok, msg) = apply_patch_from_string("", false);
        assert!(!ok);
        assert!(
            msg.contains("Empty"),
            "Expected 'Empty' in message, got: {msg}"
        );
    }

    #[test]
    fn test_apply_help_text_exists() {
        use crate::help::command_help;
        assert!(
            command_help("apply").is_some(),
            "/apply should have detailed help"
        );
    }

    #[test]
    fn test_apply_tab_completion() {
        use crate::commands::command_arg_completions;
        let candidates = command_arg_completions("/apply", "");
        assert!(
            candidates.contains(&"--check".to_string()),
            "Should include '--check'"
        );
    }

    #[test]
    fn test_apply_tab_completion_filters() {
        use crate::commands::command_arg_completions;
        let candidates = command_arg_completions("/apply", "--c");
        assert!(
            candidates.contains(&"--check".to_string()),
            "Should include '--check' for prefix '--c'"
        );
    }

    #[test]
    fn test_apply_patch_from_string_valid_in_git_repo() {
        // Create a temp dir with a git repo and test applying a real patch
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("hello.txt");
        fs::write(&file_path, "hello\n").unwrap();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create a patch
        let patch = "--- a/hello.txt\n+++ b/hello.txt\n@@ -1 +1 @@\n-hello\n+hello world\n";
        let patch_path = dir.path().join("test.patch");
        fs::write(&patch_path, patch).unwrap();

        // Apply with --check first
        let patch_str = patch_path.to_string_lossy().to_string();
        let old_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let (ok, msg) = apply_patch(&patch_str, true);
        assert!(ok, "Check should succeed: {msg}");

        // Apply for real
        let (ok, msg) = apply_patch(&patch_str, false);
        assert!(ok, "Apply should succeed: {msg}");

        // Verify file changed
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world\n");

        std::env::set_current_dir(old_dir).unwrap();
    }
}
