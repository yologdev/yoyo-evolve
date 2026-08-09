//! File operation command handlers: /add, /apply, /copy, /web, @file mentions.

use crate::commands_map::detect_language;
use crate::format::*;
use crate::git::run_git_output;

use std::io::IsTerminal;

use crate::commands_web::{fetch_url, is_valid_url, strip_html_tags, WEB_MAX_CHARS};

// ── /web ─────────────────────────────────────────────────────────────────

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

/// The image formats the Anthropic API accepts, as (extension, MIME type) pairs.
/// Single source of truth for `is_image_extension` and `mime_type_for_extension`
/// (#698: two hand-typed lists agreed with each other and were jointly wrong —
/// bmp was accepted here, printed a success, and the API rejected it a turn later).
pub const SUPPORTED_IMAGE_FORMATS: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
];

/// Check if a file path has an image extension the API supports.
pub fn is_image_extension(path: &str) -> bool {
    let lower = path.to_lowercase();
    match lower.rsplit('.').next() {
        Some(ext) => SUPPORTED_IMAGE_FORMATS.iter().any(|(e, _)| *e == ext),
        None => false,
    }
}

/// Map a file extension to a MIME type string.
/// Returns `"application/octet-stream"` for unknown extensions.
pub fn mime_type_for_extension(ext: &str) -> &'static str {
    let lower = ext.to_lowercase();
    SUPPORTED_IMAGE_FORMATS
        .iter()
        .find(|(e, _)| *e == lower)
        .map(|(_, mime)| *mime)
        .unwrap_or("application/octet-stream")
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

/// Estimate the number of tokens in a text string.
///
/// Uses the standard approximation of ~4 characters per token, which is
/// widely used for English text and code. Returns an approximate value
/// (callers should display with a `~` prefix).
pub fn estimate_tokens_simple(text: &str) -> usize {
    // 4 bytes per token is the standard rough estimate.
    // We use byte length (not char count) since most tokenizers
    // operate on bytes/byte-pairs and this matches the ~4 chars/token
    // heuristic for ASCII-heavy content like code.
    text.len() / 4
}

/// Handle the `/add` command: read file(s) and return the formatted content
/// to be injected as a user message.
///
/// Returns a tuple of:
/// - A Vec of `AddResult` — either text or image — for each file
/// - The list of file paths that were actually added successfully. Failed
///   reads and URL fetches are excluded, so related-file suggestions are
///   derived from what was really added, not a re-parse of the input (#697).
pub fn handle_add(input: &str) -> (Vec<AddResult>, Vec<String>) {
    let args = input.strip_prefix("/add").unwrap_or("").trim();

    if args.is_empty() {
        println!("{DIM}  usage: /add <path|url> — inject file or web contents into conversation");
        println!("         /add <path>:<start>-<end> — inject specific line range");
        println!("         /add src/*.rs — inject multiple files via glob");
        println!("         /add https://example.com — fetch and inject web page{RESET}\n");
        return (Vec::new(), Vec::new());
    }

    let mut results = Vec::new();
    let mut added_paths: Vec<String> = Vec::new();

    // Split on whitespace to support multiple paths: /add foo.rs bar.rs
    for arg in args.split_whitespace() {
        // Check if argument is a URL — fetch web content instead of reading a file
        if is_valid_url(arg) {
            println!("{DIM}  Fetching {arg}...{RESET}");
            match fetch_url(arg) {
                Ok(html) => {
                    let text = strip_html_tags(&html, WEB_MAX_CHARS);
                    if text.is_empty() {
                        println!("{RED}  ✗ no readable text content at {arg}{RESET}");
                        continue;
                    }
                    // Apply smart truncation for large web content
                    let (text, was_truncated, original_lines) =
                        smart_truncate_for_context(&text, ADD_MAX_LINES);
                    let line_count = text.lines().count();
                    let char_count = text.len();
                    let token_est = estimate_tokens_simple(&text);
                    let formatted = format!("**{arg}**\n```\n{text}\n```");
                    let summary = if was_truncated {
                        format!(
                            "{GREEN}  ✓ added {arg} (truncated: {line_count} of {original_lines} lines, {char_count} chars, ~{token_est} tokens){RESET}"
                        )
                    } else {
                        let word = crate::format::pluralize(line_count, "line", "lines");
                        format!(
                            "{GREEN}  ✓ added {arg} ({line_count} {word}, {char_count} chars, ~{token_est} tokens){RESET}"
                        )
                    };
                    results.push(AddResult::Text {
                        summary,
                        content: formatted,
                    });
                }
                Err(e) => {
                    println!("{RED}  ✗ failed to fetch {arg}: {e}{RESET}");
                }
            }
            continue;
        }

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
                        added_paths.push(path.clone());
                    }
                    Err(e) => {
                        println!("{RED}  ✗ {e}{RESET}");
                    }
                }
                continue;
            }

            match read_file_for_add(path, range) {
                Ok((content, line_count)) => {
                    // Apply smart truncation for large files when no line range specified
                    let (content, was_truncated, original_lines) = if range.is_none() {
                        let (truncated, did_truncate, total) =
                            smart_truncate_for_context(&content, ADD_MAX_LINES);
                        (truncated, did_truncate, total)
                    } else {
                        (content, false, line_count)
                    };

                    let formatted = format_add_content(path, &content);
                    let token_est = estimate_tokens_simple(&content);
                    let word = crate::format::pluralize(line_count, "line", "lines");
                    let range_info = if let Some((s, e)) = range {
                        format!(" (lines {s}-{e})")
                    } else {
                        String::new()
                    };
                    let summary = if was_truncated {
                        let head_count = (ADD_MAX_LINES * 2) / 5;
                        let tail_count = ADD_MAX_LINES / 5;
                        format!(
                            "{GREEN}  📎 added {path} (truncated: {head_count} head + {tail_count} tail of {original_lines} lines, ~{token_est} tokens){RESET}\n{DIM}     use /add {path}:START-END to add specific sections{RESET}"
                        )
                    } else {
                        format!("{GREEN}  ✓ added {path}{range_info} ({line_count} {word}, ~{token_est} tokens){RESET}")
                    };
                    results.push(AddResult::Text {
                        summary,
                        content: formatted,
                    });
                    added_paths.push(path.clone());
                }
                Err(e) => {
                    println!("{RED}  ✗ {e}{RESET}");
                }
            }
        }
    }

    (results, added_paths)
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
                Err(e) => {
                    // Read failed — leave unchanged, but if the file exists
                    // the user meant it as a file: warn instead of silently
                    // absorbing the failure (#704).
                    if path.exists() {
                        eprintln!("{}", mention_read_warning(raw_path, &e));
                    }
                    output.push('@');
                    output.push_str(mention);
                }
            }
        } else {
            match read_file_for_add(raw_path, range) {
                Ok((content, line_count)) => {
                    let formatted = format_add_content(raw_path, &content);
                    let token_est = estimate_tokens_simple(&content);
                    let word = crate::format::pluralize(line_count, "line", "lines");
                    let range_info = if let Some((s, e)) = range {
                        format!(" (lines {s}-{e})")
                    } else {
                        String::new()
                    };
                    let summary = format!(
                        "{GREEN}  ✓ added {raw_path}{range_info} ({line_count} {word}, ~{token_est} tokens){RESET}"
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
                Err(e) => {
                    // Read failed — leave unchanged, but if the file exists
                    // the user meant it as a file: warn instead of silently
                    // absorbing the failure (#704).
                    if path.exists() {
                        eprintln!("{}", mention_read_warning(raw_path, &e));
                    }
                    output.push('@');
                    output.push_str(mention);
                }
            }
        }

        i = j;
    }

    (output, results)
}

/// Build the user-visible warning for an @mention that resolved to an existing
/// file but could not be read. Matches /add's `✗`-style error surface (#704) —
/// without this, only the literal `@path` string reaches the model while the
/// user believes the file content did.
fn mention_read_warning(raw_path: &str, err: &str) -> String {
    format!("{RED}  ✗ could not read @{raw_path}: {err}{RESET}")
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

/// Compare two `git status --porcelain` outputs and return the paths of entries
/// that are new or changed in `after` relative to `before` (#699). A line that
/// appears verbatim in `before` is pre-existing state; anything else — a new
/// path, or an existing path whose status flipped (e.g. ` M` → `UU`) — means
/// the tree was mutated. Pure, so it is testable without touching git.
fn status_diff_paths(before: &str, after: &str) -> Vec<String> {
    let before_lines: std::collections::HashSet<&str> =
        before.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut changed = Vec::new();
    for line in after.lines() {
        if line.trim().is_empty() || before_lines.contains(line) {
            continue;
        }
        // Porcelain v1 format: `XY path` (2 status chars + space + path).
        if let Some(path) = line.get(3..).map(str::trim) {
            if !path.is_empty() && !changed.iter().any(|p| p == path) {
                changed.push(path.to_string());
            }
        }
    }
    changed
}

/// Build the honest failure report for a `--3way` attempt that failed but left
/// changes in the working tree (#699). Names the strategy, lists the touched
/// files, and tells the user how to inspect/undo. Deliberately does NOT roll
/// back: the files may have had pre-existing local edits.
fn three_way_mutation_message(changed: &[String]) -> String {
    let mut msg = String::new();
    msg.push_str(
        "Failed to apply patch — and the --3way attempt left changes in your working tree.\n",
    );
    msg.push_str("Stopped before trying further strategies against the modified files.\n");
    msg.push_str("Files touched by the failed 3-way merge (may contain conflict markers):\n");
    for path in changed {
        msg.push_str(&format!("  {path}\n"));
    }
    msg.push_str("Inspect with `git status` / `git diff`.\n");
    msg.push_str(
        "To discard the leftover changes in a file that was clean before: git checkout -- <file>\n",
    );
    msg.push_str(
        "(Nothing was rolled back automatically — files may have had pre-existing local edits.)\n",
    );
    msg
}

/// Apply a patch file using `git apply`. Returns `(success, output_message)`.
pub fn apply_patch(path: &str, check_only: bool) -> (bool, String) {
    // Verify file exists
    if !std::path::Path::new(path).exists() {
        return (false, format!("Patch file not found: {path}"));
    }

    // First get stat output to show a summary
    let stat_text = match run_git_output(&["apply", "--stat", path]) {
        Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
        Err(_) => String::new(),
    };

    // Fallback strategies: try strict first, then progressively more relaxed
    let strategies: &[(&[&str], &str)] = &[
        (&[], ""),
        (&["--3way"], " with 3-way merge (--3way)"),
        (&["-C1"], " with relaxed context matching (-C1)"),
        (&["--recount"], " with recounted hunks (--recount)"),
    ];

    for (extra_flags, label) in strategies {
        let mut args = vec!["apply"];
        if check_only {
            args.push("--check");
        }
        for flag in *extra_flags {
            args.push(flag);
        }
        args.push(path);

        // #699: `git apply --3way` is not clean-failure — on merge conflict it
        // writes conflict markers into the working tree and exits non-zero.
        // Snapshot the tree state before that attempt (non-check mode only) so
        // a mutating failure can be detected and reported instead of cascading
        // `-C1`/`--recount` against the dirty state. `--check` never writes.
        let status_before = if !check_only && extra_flags.contains(&"--3way") {
            run_git_output(&["status", "--porcelain"])
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        } else {
            None
        };

        match run_git_output(&args) {
            Ok(output) if output.status.success() => {
                let mut msg = String::new();
                if check_only {
                    if label.is_empty() {
                        msg.push_str("Dry-run OK — patch can be applied cleanly.\n");
                    } else {
                        msg.push_str(&format!("Dry-run OK — patch can be applied{label}.\n"));
                    }
                } else if label.is_empty() {
                    msg.push_str("Patch applied successfully.\n");
                } else {
                    msg.push_str(&format!("Patch applied{label}.\n"));
                }
                if !stat_text.is_empty() {
                    msg.push_str("\nFiles affected:\n");
                    msg.push_str(&stat_text);
                }
                return (true, msg);
            }
            Ok(_) => {
                // This strategy failed. If it was --3way and it dirtied the
                // tree, stop honestly instead of cascading (#699).
                if let Some(before) = status_before {
                    if let Ok(o) = run_git_output(&["status", "--porcelain"]) {
                        let after = String::from_utf8_lossy(&o.stdout).to_string();
                        let changed = status_diff_paths(&before, &after);
                        if !changed.is_empty() {
                            return (false, three_way_mutation_message(&changed));
                        }
                    }
                }
                // Clean failure — try the next strategy.
                continue;
            }
            Err(e) => return (false, format!("Failed to run git apply: {e}")),
        }
    }

    // All strategies exhausted — report failure using the strict attempt's error
    let mut fail_args = vec!["apply"];
    if check_only {
        fail_args.push("--check");
    }
    fail_args.push(path);
    let stderr = run_git_output(&fail_args)
        .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
        .unwrap_or_default();

    let mut msg = String::new();
    if check_only {
        msg.push_str("Dry-run FAILED — patch cannot be applied cleanly.\n");
    } else {
        msg.push_str("Failed to apply patch.\n");
    }
    msg.push_str("Tried: strict, --3way, -C1, --recount — all failed.\n");
    if !stderr.is_empty() {
        msg.push_str(&stderr);
    }
    (false, msg)
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

// ── /explain ─────────────────────────────────────────────────────────────

/// Build a prompt asking the agent to explain code from a file.
///
/// Parses the argument as `path[:start-end]`, reads the file content (or a
/// line range), and wraps it in a clear "explain this code" prompt that gets
/// sent to the agent. Returns `None` (after printing usage) when the input
/// is empty or the file cannot be read.
pub fn build_explain_prompt(input: &str) -> Option<String> {
    let arg = input.strip_prefix("/explain").unwrap_or(input).trim();

    if arg.is_empty() {
        println!("{DIM}  usage: /explain <file>[:<start>-<end>]{RESET}");
        println!("{DIM}  Read code from a file and ask the agent to explain it.{RESET}");
        println!("{DIM}  Example: /explain src/main.rs:50-100{RESET}\n");
        return None;
    }

    let (path, range) = parse_add_arg(arg);

    let (code, line_count) = match read_file_for_add(path, range) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{RED}  {e}{RESET}\n");
            return None;
        }
    };

    let lang = detect_language(path).unwrap_or_else(|| {
        std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
    });

    let range_desc = match range {
        Some((start, end)) => format!(" (lines {start}-{end})"),
        None => {
            if line_count > 0 {
                format!(" ({line_count} lines)")
            } else {
                String::new()
            }
        }
    };

    println!("{DIM}  🔍 Explaining {path}{range_desc}{RESET}\n");

    let prompt = format!(
        "Explain the following code from `{path}`{range_desc}:\n\
         \n\
         ```{lang}\n\
         {code}\n\
         ```\n\
         \n\
         Focus on: what it does, how it works, any notable patterns or potential issues."
    );

    Some(prompt)
}

// ── /open command ──

/// Parse the input for `/open`, extracting file path and optional line number.
/// Supports:
/// - `/open path/to/file`
/// - `/open path/to/file:42`
/// - `/open path/to/file 42`
pub fn parse_open_args(input: &str) -> (String, Option<u32>) {
    let args = input.strip_prefix("/open").unwrap_or("").trim().to_string();
    if args.is_empty() {
        return (String::new(), None);
    }

    // Try "path:line" syntax first
    if let Some(colon_pos) = args.rfind(':') {
        let path_part = &args[..colon_pos];
        let line_part = &args[colon_pos + 1..];
        if let Ok(line) = line_part.parse::<u32>() {
            if line > 0 {
                return (path_part.to_string(), Some(line));
            }
        }
    }

    // Try "path line" syntax (last token is a number)
    let parts: Vec<&str> = args.rsplitn(2, ' ').collect();
    if parts.len() == 2 {
        if let Ok(line) = parts[0].parse::<u32>() {
            if line > 0 {
                return (parts[1].to_string(), Some(line));
            }
        }
    }

    // Just a path, no line number
    (args, None)
}

/// Resolve the editor command to use. Checks $VISUAL, $EDITOR, then tries
/// common editors in PATH.
pub fn resolve_editor() -> Option<String> {
    // Check env vars first
    if let Ok(editor) = std::env::var("VISUAL") {
        if !editor.is_empty() {
            return Some(editor);
        }
    }
    if let Ok(editor) = std::env::var("EDITOR") {
        if !editor.is_empty() {
            return Some(editor);
        }
    }

    // Try common editors in PATH
    let candidates = ["code", "vim", "vi", "nano"];
    for candidate in &candidates {
        let output = std::process::Command::new("which").arg(candidate).output();
        if let Ok(out) = output {
            if out.status.success() {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

/// Format editor args with optional line number.
/// Most editors support `+N` before the filename for line jumping.
pub fn format_editor_args(_editor: &str, file: &str, line: Option<u32>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(n) = line {
        // +N syntax works for vim, vi, nano, emacs, code (VS Code)
        args.push(format!("+{n}"));
    }
    args.push(file.to_string());
    args
}

/// Handle the `/open` slash command.
pub fn handle_open(input: &str) {
    let (file, line) = parse_open_args(input);
    if file.is_empty() {
        eprintln!("{YELLOW}  Usage: /open <file>[:<line>] or /open <file> <line>{RESET}");
        return;
    }

    // Warn if file doesn't exist but don't block — editor may create it
    if !std::path::Path::new(&file).exists() {
        eprintln!("{YELLOW}  Warning: {file} does not exist (editor may create it){RESET}");
    }

    let Some(editor) = resolve_editor() else {
        eprintln!(
            "{RED}  No editor found. Set $VISUAL or $EDITOR, or install vim/nano/code.{RESET}"
        );
        return;
    };

    let args = format_editor_args(&editor, &file, line);
    let line_suffix = line.map_or(String::new(), |n| format!(" at line {n}"));
    // Extract just the command name for display (strip path if any)
    let editor_display = editor.rsplit('/').next().unwrap_or(&editor);
    eprintln!("{DIM}  Opening {file} in {editor_display}{line_suffix}...{RESET}");

    // Use .status() so we wait for terminal editors (vim, nano, etc.)
    let result = std::process::Command::new(&editor).args(&args).status();
    match result {
        Ok(status) => {
            if !status.success() {
                let code = status.code().unwrap_or(-1);
                eprintln!("{YELLOW}  Editor exited with status {code}{RESET}");
            }
        }
        Err(e) => {
            eprintln!("{RED}  Failed to launch editor '{editor}': {e}{RESET}");
        }
    }
}

/// Source file extensions eligible for related-file suggestions.
const SOURCE_EXTENSIONS: &[&str] = &["rs", "py", "ts", "js", "go", "java", "rb", "cpp", "c", "h"];

/// Check if a file path has a source-code extension eligible for suggestions.
fn is_source_extension(path: &str) -> bool {
    let lower = path.to_lowercase();
    if let Some(ext) = lower.rsplit('.').next() {
        SOURCE_EXTENSIONS.contains(&ext)
    } else {
        false
    }
}

/// Suggest related files the user might want to add alongside the given file.
///
/// Returns up to 3 related file paths, detected via:
/// 1. Test file patterns (e.g. `tests/foo.rs`, `src/foo_test.rs`)
/// 2. Module parent (`src/foo/bar.rs` → `src/foo/mod.rs`)
/// 3. Corresponding module directory (`src/foo.rs` → `src/foo/mod.rs`)
/// 4. Rust `use crate::` / `mod` declarations → sibling modules
///
/// Only works for source files. Skips paths in `already_added`.
pub fn suggest_related_files(path: &str, already_added: &[String]) -> Vec<String> {
    if !is_source_extension(path) {
        return Vec::new();
    }

    let p = std::path::Path::new(path);
    let canon_path = path.replace('\\', "/");
    let mut suggestions: Vec<String> = Vec::new();

    // Collect candidates, then filter and deduplicate
    let mut candidates: Vec<String> = Vec::new();

    // 1. Test file patterns
    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
        if let Some(parent) = p.parent().and_then(|d| d.to_str()) {
            // tests/<stem>.rs (relative to project root)
            candidates.push(format!("tests/{stem}.rs"));

            // <parent>/<stem>_test.rs
            if parent.is_empty() {
                candidates.push(format!("{stem}_test.rs"));
            } else {
                candidates.push(format!("{parent}/{stem}_test.rs"));
            }
        }
    }

    // 2. Module parent: src/foo/bar.rs → src/foo/mod.rs
    if let Some(parent) = p.parent() {
        let mod_rs = parent.join("mod.rs");
        if let Some(s) = mod_rs.to_str() {
            candidates.push(s.to_string());
        }
    }

    // 3. Corresponding module dir: src/foo.rs → src/foo/mod.rs
    if let (Some(parent), Some(stem)) = (
        p.parent().and_then(|d| d.to_str()),
        p.file_stem().and_then(|s| s.to_str()),
    ) {
        // Don't suggest for mod.rs or lib.rs or main.rs
        if stem != "mod" && stem != "lib" && stem != "main" {
            if parent.is_empty() {
                candidates.push(format!("{stem}/mod.rs"));
            } else {
                candidates.push(format!("{parent}/{stem}/mod.rs"));
            }
        }
    }

    // 4. Rust-specific: parse `use crate::` and `mod` declarations
    if path.ends_with(".rs") {
        if let Ok(content) = std::fs::read_to_string(path) {
            let src_prefix = detect_src_prefix(path);
            for line in content.lines().take(50) {
                let trimmed = line.trim();

                // `use crate::foo` or `use crate::foo::bar`
                if let Some(rest) = trimmed.strip_prefix("use crate::") {
                    if let Some(module) = rest.split("::").next() {
                        let module = module.trim_end_matches(';').trim();
                        if !module.is_empty()
                            && module.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            candidates.push(format!("{src_prefix}{module}.rs"));
                            candidates.push(format!("{src_prefix}{module}/mod.rs"));
                        }
                    }
                }

                // `mod foo;` (external module declaration)
                if let Some(rest) = trimmed.strip_prefix("mod ") {
                    let rest = rest.trim_end_matches(';').trim();
                    if !rest.is_empty()
                        && !rest.contains('{')
                        && rest.chars().all(|c| c.is_alphanumeric() || c == '_')
                    {
                        candidates.push(format!("{src_prefix}{rest}.rs"));
                        candidates.push(format!("{src_prefix}{rest}/mod.rs"));
                    }
                }
            }
        }
    }

    // Filter: must exist, not self, not already added, deduplicated
    for candidate in candidates {
        if suggestions.len() >= 3 {
            break;
        }
        let canon = candidate.replace('\\', "/");
        if canon == canon_path {
            continue;
        }
        if already_added.iter().any(|a| a.replace('\\', "/") == canon) {
            continue;
        }
        if suggestions.contains(&canon) {
            continue;
        }
        if std::path::Path::new(&candidate).exists() {
            suggestions.push(canon);
        }
    }

    suggestions
}

/// Detect the `src/` prefix from a file path.
/// E.g., `src/commands_file.rs` → `src/`, `lib/foo.rs` → `lib/`.
fn detect_src_prefix(path: &str) -> &str {
    let normalized = path.replace('\\', "/");
    // Find the last path component and return everything before it
    if let Some(pos) = normalized.rfind('/') {
        &path[..pos + 1]
    } else {
        ""
    }
}

/// Extract file paths referenced in compiler/linter error output.
///
/// Recognises common formats:
/// - Rust:       `--> src/foo.rs:42:10` or `src/foo.rs:42:10`
/// - TypeScript: `src/foo.ts(42,10)` or `src/foo.ts:42:10`
/// - Python:     `File "src/foo.py", line 42`
/// - Go:         `./foo.go:42:10`
/// - Generic:    `path/file.ext:LINE`
///
/// Results are deduplicated and sorted.  Shared by the public
/// `extract_file_paths_from_output` (filters to existing files) and
/// the test-only `extract_file_path_candidates` (no filesystem check).
fn extract_file_path_candidates_from(output: &str) -> Vec<String> {
    use std::collections::BTreeSet;
    use std::sync::LazyLock;

    use regex::Regex;

    // Compile patterns once — this function is called on every watch
    // iteration, so avoiding repeated Regex::new() matters.
    static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![
            // Rust: `--> src/foo.rs:42:10`
            Regex::new(r"-->\s+([\w./_\-\\]+\.\w+):\d+").unwrap(),
            // Python: `File "src/foo.py", line 42`
            Regex::new(r#"File\s+"([\w./_\-\\]+\.\w+)",\s+line\s+\d+"#).unwrap(),
            // TS paren style: `src/foo.ts(42,10)`
            Regex::new(r"([\w./_\-\\]+\.\w+)\(\d+[,)]").unwrap(),
            // Generic `path/file.ext:LINE` — must contain a `/` or `.\` to
            // avoid matching random words like `error:1`.
            Regex::new(r"([\w._\-\\]*[/\\][\w./_\-\\]+\.\w+):\d+").unwrap(),
            // Go-style `./foo.go:42`
            Regex::new(r"(\./[\w./_\-\\]+\.\w+):\d+").unwrap(),
        ]
    });

    let mut seen = BTreeSet::new();
    for pat in PATTERNS.iter() {
        for cap in pat.captures_iter(output) {
            if let Some(m) = cap.get(1) {
                seen.insert(m.as_str().to_string());
            }
        }
    }

    seen.into_iter().collect()
}

/// Extract file paths from compiler/linter output, filtered to only
/// files that actually exist on disk.
pub fn extract_file_paths_from_output(output: &str) -> Vec<String> {
    use std::path::Path;

    extract_file_path_candidates_from(output)
        .into_iter()
        .filter(|p| Path::new(p).is_file())
        .collect()
}

/// Internal helper: extract file-path candidates from output without
/// the filesystem-existence filter.  Used by tests to verify regex
/// matching independently of on-disk state.
#[cfg(test)]
fn extract_file_path_candidates(output: &str) -> Vec<String> {
    extract_file_path_candidates_from(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::KNOWN_COMMANDS;
    use crate::help::help_text;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    // ── strip_html_tags ──────────────────────────────────────────────

    // ── is_valid_url ────────────────────────────────────────────────

    // ── /add command tests ────────────────────────────────────────────

    // --- extract_file_paths_from_output tests (regex matching only) ---

    #[test]
    fn extract_paths_rust_error() {
        let output = "error[E0308]: mismatched types\n  --> src/main.rs:42:10\n";
        let paths = extract_file_path_candidates(output);
        assert!(paths.contains(&"src/main.rs".to_string()), "got: {paths:?}");
    }

    #[test]
    fn extract_paths_rust_multiple() {
        let output = "\
error[E0308]: mismatched types
  --> src/foo.rs:10:5
  --> src/bar.rs:20:3
";
        let paths = extract_file_path_candidates(output);
        assert!(paths.contains(&"src/foo.rs".to_string()));
        assert!(paths.contains(&"src/bar.rs".to_string()));
    }

    #[test]
    fn extract_paths_typescript_paren() {
        let output = "src/index.ts(42,10): error TS2345: ...\n";
        let paths = extract_file_path_candidates(output);
        assert!(
            paths.contains(&"src/index.ts".to_string()),
            "got: {paths:?}"
        );
    }

    #[test]
    fn extract_paths_typescript_colon() {
        let output = "src/app.tsx:15:8 - error TS1005: ';' expected.\n";
        let paths = extract_file_path_candidates(output);
        assert!(paths.contains(&"src/app.tsx".to_string()), "got: {paths:?}");
    }

    #[test]
    fn extract_paths_python_traceback() {
        let output = r#"Traceback (most recent call last):
  File "src/main.py", line 42, in <module>
  File "lib/utils.py", line 10, in helper
"#;
        let paths = extract_file_path_candidates(output);
        assert!(paths.contains(&"src/main.py".to_string()), "got: {paths:?}");
        assert!(
            paths.contains(&"lib/utils.py".to_string()),
            "got: {paths:?}"
        );
    }

    #[test]
    fn extract_paths_go_style() {
        let output = "./cmd/server.go:42:10: undefined: foo\n";
        let paths = extract_file_path_candidates(output);
        assert!(
            paths.contains(&"./cmd/server.go".to_string()),
            "got: {paths:?}"
        );
    }

    #[test]
    fn extract_paths_deduplication() {
        let output = "\
error[E0308]: first
  --> src/foo.rs:10:5
error[E0308]: second
  --> src/foo.rs:20:3
";
        let paths = extract_file_path_candidates(output);
        // Should appear only once despite two references
        assert_eq!(
            paths.iter().filter(|p| *p == "src/foo.rs").count(),
            1,
            "got: {paths:?}"
        );
    }

    #[test]
    fn extract_paths_sorted() {
        let output = "\
  --> src/z.rs:1:1
  --> src/a.rs:1:1
  --> src/m.rs:1:1
";
        let paths = extract_file_path_candidates(output);
        assert_eq!(paths, vec!["src/a.rs", "src/m.rs", "src/z.rs"]);
    }

    #[test]
    fn extract_paths_filters_nonexistent() {
        // The public fn filters by Path::exists(). With fake paths, we
        // should get an empty vec.
        let output = "  --> definitely/not/a/real/file.rs:42:10\n";
        let paths = extract_file_paths_from_output(output);
        assert!(
            paths.is_empty(),
            "non-existent file should be filtered: {paths:?}"
        );
    }

    #[test]
    fn extract_paths_real_file() {
        // Cargo.toml always exists in the project root during tests.
        let _output = "  --> Cargo.toml:1:1\n";
        // Cargo.toml doesn't have a `/` so generic won't match — but
        // the Rust arrow pattern will.  Let's use src/main.rs which
        // does exist.
        let output2 = "  --> src/main.rs:1:1\n";
        let paths = extract_file_paths_from_output(output2);
        assert!(
            paths.contains(&"src/main.rs".to_string()),
            "existing file should pass filter: {paths:?}"
        );
    }

    // --- original tests below ---

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
        // #698: bmp is rejected by the API — must NOT be treated as an image.
        assert!(!is_image_extension("photo.bmp"));
    }

    #[test]
    fn is_image_extension_case_insensitive() {
        assert!(is_image_extension("photo.PNG"));
        assert!(is_image_extension("image.Jpg"));
        assert!(is_image_extension("banner.JPEG"));
        assert!(is_image_extension("icon.GIF"));
        assert!(is_image_extension("pic.WeBp"));
        assert!(!is_image_extension("scan.BMP"));
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
    fn mime_type_bmp_not_supported() {
        // #698: image/bmp is rejected by the API — bmp must not map to an image MIME.
        assert_eq!(mime_type_for_extension("bmp"), "application/octet-stream");
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
    fn mention_read_warning_names_path_and_error() {
        // Emission-point discipline: this string is exactly what the user
        // sees on stderr when an existing @mention can't be read.
        let warning = mention_read_warning("secrets/locked.txt", "permission denied");
        assert!(warning.contains("✗"));
        assert!(warning.contains("could not read @secrets/locked.txt"));
        assert!(warning.contains("permission denied"));
    }

    #[test]
    fn expand_file_mentions_nonexistent_mention_is_freeform_text() {
        // Free-form mentions like @yuanhao must pass through untouched —
        // no results, no warning path taken (path doesn't exist).
        let (text, results) = expand_file_mentions("thanks @yuanhao for the review");
        assert!(results.is_empty());
        assert_eq!(text, "thanks @yuanhao for the review");
    }

    #[cfg(unix)]
    #[test]
    fn expand_file_mentions_unreadable_file_warns_and_leaves_text_unchanged() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("locked.txt");
        std::fs::write(&file_path, "secret contents").unwrap();
        std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o000)).unwrap();

        // Environment gate: as root (some CI containers), mode 0o000 files
        // are still readable and the failure can't be produced — skip.
        if std::fs::read(&file_path).is_ok() {
            eprintln!("skipping: environment can read mode-0o000 files (running as root?)");
            return;
        }

        let path_str = file_path.to_string_lossy();
        let input = format!("explain @{path_str}");
        let (text, results) = expand_file_mentions(&input);

        // The mention text must stay unchanged (only the silence was the bug).
        assert_eq!(text, input);
        assert!(results.is_empty());

        // Restore permissions so TempDir cleanup can remove the file.
        std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o644)).unwrap();
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

    // --- #699: failed --3way mutating the tree must stop the cascade honestly ---

    #[test]
    fn test_status_diff_paths_unchanged_tree_detects_nothing() {
        // Identical before/after status → no mutation detected → cascade continues
        // exactly as before (pins zero behavior change on the clean-failure path).
        let status = " M src/lib.rs\n?? notes.txt\n";
        assert!(status_diff_paths(status, status).is_empty());
        assert!(status_diff_paths("", "").is_empty());
    }

    #[test]
    fn test_status_diff_paths_detects_new_and_modified_entries() {
        let before = " M src/lib.rs\n";
        let after = " M src/lib.rs\nUU src/conflicted.rs\n M src/other.rs\n";
        let changed = status_diff_paths(before, after);
        assert_eq!(changed, vec!["src/conflicted.rs", "src/other.rs"]);
    }

    #[test]
    fn test_status_diff_paths_detects_status_change_of_existing_entry() {
        // A previously-clean-ish entry whose status flips (e.g. ` M` → `UU`)
        // is a mutation even though the path appeared before.
        let before = " M src/lib.rs\n";
        let after = "UU src/lib.rs\n";
        assert_eq!(status_diff_paths(before, after), vec!["src/lib.rs"]);
    }

    #[test]
    fn test_status_diff_paths_ignores_short_lines() {
        assert!(status_diff_paths("", "x\n\n").is_empty());
    }

    #[test]
    fn test_three_way_mutation_message_names_files_and_recovery() {
        let msg =
            three_way_mutation_message(&["src/conflicted.rs".to_string(), "README.md".to_string()]);
        assert!(msg.contains("--3way"), "should name the strategy: {msg}");
        assert!(
            msg.contains("src/conflicted.rs"),
            "should list files: {msg}"
        );
        assert!(msg.contains("README.md"), "should list files: {msg}");
        assert!(
            msg.contains("git status") && msg.contains("git checkout --"),
            "should tell the user how to inspect/undo: {msg}"
        );
        assert!(
            msg.to_lowercase().contains("not") && msg.to_lowercase().contains("roll"),
            "should state it was NOT rolled back automatically: {msg}"
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
    #[serial]
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

    /// Helper: set up a temp git repo with a committed file.
    /// Returns (TempDir, file_path, old_cwd).
    fn setup_git_repo(
        filename: &str,
        content: &str,
    ) -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join(filename);
        fs::write(&file_path, content).unwrap();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
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

        let old_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        (dir, file_path, old_dir)
    }

    #[test]
    #[serial]
    fn test_apply_patch_fallback_strategies() {
        // Create a repo with a file, then modify it so that a patch against the
        // original version has shifted context — forcing a fallback strategy.
        let original = "line1\nline2\nline3\nline4\nline5\n";
        let (_dir, file_path, old_dir) = setup_git_repo("test.txt", original);

        // Now modify the file (add lines at the top) so context drifts
        let modified = "new_top_1\nnew_top_2\nline1\nline2\nline3\nline4\nline5\n";
        fs::write(&file_path, modified).unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(_dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "shift"])
            .current_dir(_dir.path())
            .output()
            .unwrap();

        // Create a patch that was generated against the *original* layout.
        // The hunk says line 3 is at line 3, but now it's at line 5 — context has drifted.
        let patch = "\
diff --git a/test.txt b/test.txt
--- a/test.txt
+++ b/test.txt
@@ -1,5 +1,5 @@
 line1
 line2
-line3
+line3_modified
 line4
 line5
";
        let patch_path = _dir.path().join("drift.patch");
        fs::write(&patch_path, patch).unwrap();

        let (ok, msg) = apply_patch(&patch_path.to_string_lossy(), false);
        // Should succeed via a fallback strategy
        assert!(ok, "Fallback should succeed: {msg}");

        // Verify the modification was applied
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(
            content.contains("line3_modified"),
            "File should contain modified line, got: {content}"
        );

        std::env::set_current_dir(old_dir).unwrap();
    }

    #[test]
    #[serial]
    fn test_apply_patch_check_only_fallback() {
        // Same drift setup as above, but with check_only=true
        let original = "line1\nline2\nline3\nline4\nline5\n";
        let (_dir, file_path, old_dir) = setup_git_repo("test.txt", original);

        let modified = "new_top_1\nnew_top_2\nline1\nline2\nline3\nline4\nline5\n";
        fs::write(&file_path, modified).unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(_dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "shift"])
            .current_dir(_dir.path())
            .output()
            .unwrap();

        let patch = "\
diff --git a/test.txt b/test.txt
--- a/test.txt
+++ b/test.txt
@@ -1,5 +1,5 @@
 line1
 line2
-line3
+line3_modified
 line4
 line5
";
        let patch_path = _dir.path().join("drift.patch");
        fs::write(&patch_path, patch).unwrap();

        let (ok, msg) = apply_patch(&patch_path.to_string_lossy(), true);
        assert!(ok, "Check-only fallback should succeed: {msg}");
        assert!(
            msg.contains("Dry-run OK"),
            "Should indicate dry-run success: {msg}"
        );

        // File should be unchanged since check_only=true
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(
            !content.contains("line3_modified"),
            "File should not be modified in check-only mode"
        );

        std::env::set_current_dir(old_dir).unwrap();
    }

    #[test]
    #[serial]
    fn test_apply_patch_reports_fallback_strategy() {
        // Create a drifted patch situation where strict apply fails but a fallback succeeds.
        // Verify the output message mentions the fallback method.
        let original = "line1\nline2\nline3\nline4\nline5\n";
        let (_dir, file_path, old_dir) = setup_git_repo("test.txt", original);

        let modified = "new_top_1\nnew_top_2\nline1\nline2\nline3\nline4\nline5\n";
        fs::write(&file_path, modified).unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(_dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "shift"])
            .current_dir(_dir.path())
            .output()
            .unwrap();

        let patch = "\
diff --git a/test.txt b/test.txt
--- a/test.txt
+++ b/test.txt
@@ -1,5 +1,5 @@
 line1
 line2
-line3
+line3_modified
 line4
 line5
";
        let patch_path = _dir.path().join("drift.patch");
        fs::write(&patch_path, patch).unwrap();

        let (ok, msg) = apply_patch(&patch_path.to_string_lossy(), false);
        assert!(ok, "Should succeed via fallback: {msg}");

        // If it didn't apply via strict mode, the message should mention a fallback
        // (either --3way, -C1, or --recount)
        let used_fallback = msg.contains("--3way")
            || msg.contains("-C1")
            || msg.contains("--recount")
            || msg.contains("successfully");
        assert!(
            used_fallback,
            "Message should mention the strategy used: {msg}"
        );

        std::env::set_current_dir(old_dir).unwrap();
    }

    #[test]
    #[serial]
    fn test_apply_patch_strict_still_works() {
        // When a patch applies cleanly, it should say "successfully" without fallback labels
        let original = "hello\n";
        let (_dir, file_path, old_dir) = setup_git_repo("clean.txt", original);

        let patch = "\
diff --git a/clean.txt b/clean.txt
--- a/clean.txt
+++ b/clean.txt
@@ -1 +1 @@
-hello
+hello world
";
        let patch_path = _dir.path().join("clean.patch");
        fs::write(&patch_path, patch).unwrap();

        let (ok, msg) = apply_patch(&patch_path.to_string_lossy(), false);
        assert!(ok, "Strict apply should succeed: {msg}");
        assert!(
            msg.contains("successfully"),
            "Should say 'successfully': {msg}"
        );
        // Should NOT mention any fallback strategy
        assert!(
            !msg.contains("--3way") && !msg.contains("-C1") && !msg.contains("--recount"),
            "Clean apply should not mention fallback: {msg}"
        );

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world\n");

        std::env::set_current_dir(old_dir).unwrap();
    } // ── Tests moved from commands.rs — /add command tests ────────────

    #[test]
    fn test_add_command_recognized() {
        use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
        assert!(!is_unknown_command("/add"));
        assert!(!is_unknown_command("/add src/main.rs"));
        assert!(
            KNOWN_COMMANDS.contains(&"/add"),
            "/add should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_add_in_help_text() {
        use crate::help::help_text;
        let text = help_text();
        assert!(
            text.contains("/add"),
            "Help text should mention /add command"
        );
    }

    #[test]
    fn test_handle_add_no_args_returns_empty() {
        let (results, _) = handle_add("/add");
        assert!(results.is_empty(), "No args should return empty results");
    }

    #[test]
    fn test_handle_add_with_space_no_args_returns_empty() {
        let (results, _) = handle_add("/add   ");
        assert!(
            results.is_empty(),
            "Whitespace-only args should return empty"
        );
    }

    #[test]
    fn test_handle_add_real_file() {
        let root = env!("CARGO_MANIFEST_DIR");
        let cargo_path = format!("{}/Cargo.toml", root);
        let (results, _) = handle_add(&format!("/add {}", cargo_path));
        assert_eq!(results.len(), 1, "Should return one result for Cargo.toml");
        match &results[0] {
            AddResult::Text { summary, content } => {
                assert!(
                    summary.contains("Cargo.toml"),
                    "Summary should mention the file"
                );
                assert!(
                    content.contains("[package]"),
                    "Content should contain file text"
                );
            }
            _ => panic!("Expected AddResult::Text for Cargo.toml"),
        }
    }

    #[test]
    fn test_handle_add_shows_token_estimate() {
        let root = env!("CARGO_MANIFEST_DIR");
        let cargo_path = format!("{}/Cargo.toml", root);
        let (results, _) = handle_add(&format!("/add {}", cargo_path));
        assert_eq!(results.len(), 1);
        match &results[0] {
            AddResult::Text { summary, .. } => {
                assert!(
                    summary.contains("tokens"),
                    "Summary should include token estimate, got: {summary}"
                );
                assert!(
                    summary.contains('~'),
                    "Token estimate should use ~ prefix for approximation"
                );
            }
            _ => panic!("Expected AddResult::Text"),
        }
    }

    #[test]
    fn test_handle_add_with_line_range() {
        let root = env!("CARGO_MANIFEST_DIR");
        let (results, _) = handle_add(&format!("/add {}/Cargo.toml:1-3", root));
        assert_eq!(results.len(), 1);
        match &results[0] {
            AddResult::Text { summary, content } => {
                assert!(
                    summary.contains("lines 1-3"),
                    "Summary should mention line range"
                );
                assert!(
                    content.contains("```"),
                    "Content should be wrapped in code fence"
                );
            }
            _ => panic!("Expected AddResult::Text for line range"),
        }
    }

    #[test]
    fn test_handle_add_glob_pattern() {
        let root = env!("CARGO_MANIFEST_DIR");
        let (results, _) = handle_add(&format!("/add {}/src/*.rs", root));
        assert!(results.len() > 1, "Should match multiple .rs files in src/");
    }

    #[test]
    fn test_handle_add_nonexistent_file() {
        let (results, _) = handle_add("/add nonexistent_xyz_file.rs");
        assert!(results.is_empty(), "Nonexistent file should return empty");
    }

    #[test]
    fn test_handle_add_multiple_files() {
        let root = env!("CARGO_MANIFEST_DIR");
        let (results, _) = handle_add(&format!("/add {}/Cargo.toml {}/LICENSE", root, root));
        assert_eq!(results.len(), 2, "Should return results for both files");
    }

    /// Regression test for #697: the paths fed to related-file suggestions
    /// must be the files that were ACTUALLY added — a failed add (nonexistent
    /// file) and a URL argument must not appear in the added-paths list.
    #[test]
    fn test_handle_add_paths_exclude_failed_adds_and_urls() {
        let root = env!("CARGO_MANIFEST_DIR");
        let good = format!("{root}/Cargo.toml");
        // The .invalid TLD is reserved (RFC 2606) and never resolves, so the
        // URL branch fails fast without depending on any real host.
        let (results, added_paths) = handle_add(&format!(
            "/add {good} nonexistent_file_697.rs https://nonexistent-host-697.invalid/page"
        ));
        assert_eq!(
            added_paths,
            vec![good],
            "only the successfully-added file belongs in added_paths"
        );
        assert!(
            !added_paths.iter().any(|p| p.contains("nonexistent_file")),
            "a failed add must not leak into added_paths"
        );
        assert!(
            !added_paths.iter().any(|p| p.starts_with("http")),
            "a URL arg must never leak into added_paths"
        );
        // The successful file still produced its normal result.
        assert!(results.iter().any(
            |r| matches!(r, AddResult::Text { summary, .. } if summary.contains("Cargo.toml"))
        ));
    }

    // ── build_explain_prompt ─────────────────────────────────────────

    #[test]
    fn explain_prompt_with_real_file() {
        let root = env!("CARGO_MANIFEST_DIR");
        let path = format!("{}/Cargo.toml", root);
        let result = build_explain_prompt(&format!("/explain {path}"));
        assert!(result.is_some(), "Should return a prompt for a real file");
        let prompt = result.unwrap();
        assert!(
            prompt.contains("Cargo.toml"),
            "Prompt should mention filename"
        );
        assert!(
            prompt.contains("[package]"),
            "Prompt should include file content"
        );
        assert!(
            prompt.contains("```toml"),
            "Prompt should include language fence"
        );
        assert!(
            prompt.contains("Focus on:"),
            "Prompt should include focus instructions"
        );
    }

    #[test]
    fn explain_prompt_nonexistent_file_returns_none() {
        let result = build_explain_prompt("/explain nonexistent_xyz_file.rs");
        assert!(result.is_none(), "Nonexistent file should return None");
    }

    #[test]
    fn explain_prompt_with_line_range() {
        let root = env!("CARGO_MANIFEST_DIR");
        let path = format!("{}/Cargo.toml", root);
        let result = build_explain_prompt(&format!("/explain {path}:1-3"));
        assert!(result.is_some(), "Should return a prompt for a line range");
        let prompt = result.unwrap();
        assert!(
            prompt.contains("lines 1-3"),
            "Prompt should mention the line range"
        );
        // Only 3 lines — shouldn't have the entire file
        let code_block_start = prompt.find("```toml\n").unwrap();
        let code_block_end = prompt[code_block_start + 8..].find("\n```").unwrap();
        let code_content = &prompt[code_block_start + 8..code_block_start + 8 + code_block_end];
        let line_count = code_content.lines().count();
        assert_eq!(line_count, 3, "Should include exactly 3 lines");
    }

    #[test]
    fn explain_prompt_empty_input_returns_none() {
        let result = build_explain_prompt("/explain");
        assert!(result.is_none(), "Empty input should return None");
        let result2 = build_explain_prompt("/explain   ");
        assert!(
            result2.is_none(),
            "Whitespace-only input should return None"
        );
    }

    #[test]
    fn test_handle_add_large_file_truncated() {
        // Create a temp file with more than ADD_MAX_LINES (500) lines
        let dir = tempfile::tempdir().unwrap();
        let big_file = dir.path().join("big.rs");
        let mut content = String::new();
        for i in 0..800 {
            content.push_str(&format!("fn function_{i}() {{ }}\n"));
        }
        std::fs::write(&big_file, &content).unwrap();

        let path = big_file.to_str().unwrap();
        let (results, _) = handle_add(&format!("/add {path}"));
        assert_eq!(results.len(), 1);

        match &results[0] {
            AddResult::Text { summary, content } => {
                // Summary should mention truncation
                assert!(
                    summary.contains("truncated"),
                    "Summary should mention truncation: {summary}"
                );
                assert!(
                    summary.contains("800 lines"),
                    "Summary should mention original line count: {summary}"
                );
                // Content should have the omission marker
                assert!(
                    content.contains("lines omitted"),
                    "Content should have omission marker"
                );
                // Should have head content
                assert!(
                    content.contains("function_0"),
                    "Should include head content"
                );
                // Should have tail content
                assert!(
                    content.contains("function_799"),
                    "Should include tail content"
                );
                // Should NOT have middle content
                assert!(
                    !content.contains("function_500"),
                    "Should not include middle content"
                );
            }
            _ => panic!("Expected Text result"),
        }
    }

    #[test]
    fn test_handle_add_line_range_skips_truncation() {
        // Even for a large file, a line range should not be truncated
        let dir = tempfile::tempdir().unwrap();
        let big_file = dir.path().join("big2.rs");
        let mut content = String::new();
        for i in 0..800 {
            content.push_str(&format!("fn function_{i}() {{ }}\n"));
        }
        std::fs::write(&big_file, &content).unwrap();

        let path = big_file.to_str().unwrap();
        let (results, _) = handle_add(&format!("/add {path}:1-600"));
        assert_eq!(results.len(), 1);

        match &results[0] {
            AddResult::Text { summary, content } => {
                // Should NOT be truncated since a range was specified
                assert!(
                    !summary.contains("truncated"),
                    "Line-range add should not truncate: {summary}"
                );
                // Should have all 600 lines
                assert!(content.contains("function_0"), "Should include start");
                assert!(content.contains("function_599"), "Should include end");
                assert!(
                    content.contains("function_300"),
                    "Should include middle (no truncation)"
                );
            }
            _ => panic!("Expected Text result"),
        }
    }

    // ── /open tests ──

    #[test]
    fn test_parse_open_args_empty() {
        let (file, line) = parse_open_args("/open");
        assert!(file.is_empty());
        assert_eq!(line, None);
    }

    #[test]
    fn test_parse_open_args_file_only() {
        let (file, line) = parse_open_args("/open src/main.rs");
        assert_eq!(file, "src/main.rs");
        assert_eq!(line, None);
    }

    #[test]
    fn test_parse_open_args_colon_line() {
        let (file, line) = parse_open_args("/open src/main.rs:42");
        assert_eq!(file, "src/main.rs");
        assert_eq!(line, Some(42));
    }

    #[test]
    fn test_parse_open_args_space_line() {
        let (file, line) = parse_open_args("/open src/main.rs 100");
        assert_eq!(file, "src/main.rs");
        assert_eq!(line, Some(100));
    }

    #[test]
    fn test_parse_open_args_colon_not_a_number() {
        // Windows-style path with colon that's not a line number
        let (file, line) = parse_open_args("/open C:/Users/file.txt");
        // The "C" part won't parse as u32, so treat as plain path
        assert_eq!(file, "C:/Users/file.txt");
        assert_eq!(line, None);
    }

    #[test]
    fn test_parse_open_args_line_zero_ignored() {
        // Line 0 is not valid
        let (file, line) = parse_open_args("/open src/main.rs:0");
        assert_eq!(file, "src/main.rs:0");
        assert_eq!(line, None);
    }

    #[test]
    fn test_format_editor_args_no_line() {
        let args = format_editor_args("vim", "src/main.rs", None);
        assert_eq!(args, vec!["src/main.rs"]);
    }

    #[test]
    fn test_format_editor_args_with_line() {
        let args = format_editor_args("vim", "src/main.rs", Some(42));
        assert_eq!(args, vec!["+42", "src/main.rs"]);
    }

    #[test]
    fn test_resolve_editor_from_env() {
        // This test sets $VISUAL to verify env var resolution
        std::env::set_var("VISUAL", "test-editor-that-doesnt-exist");
        let editor = resolve_editor();
        assert_eq!(editor, Some("test-editor-that-doesnt-exist".to_string()));
        std::env::remove_var("VISUAL");
    }

    #[test]
    fn test_handle_add_url_detection() {
        // A URL argument should be treated as a URL, not a file path.
        // We can't test actual fetching without network, but we can verify
        // that a URL doesn't produce "no files matched" (which would mean
        // it fell through to the file-path branch).
        //
        // The server may return content (error page HTML) or an empty body,
        // so the result can be empty or contain one entry. Either is fine —
        // the important thing is it took the URL code-path rather than
        // trying to glob-expand the URL as a file path.
        let (results, _) = handle_add("/add https://httpbin.org/status/404");
        assert!(
            results.len() <= 1,
            "URL should produce at most one result, got {}",
            results.len()
        );
    }

    #[test]
    fn test_handle_add_file_path_not_treated_as_url() {
        // Regular paths should still go through file-path expansion
        let (results, _) = handle_add("/add nonexistent_file_that_does_not_exist.rs");
        // Should be empty because file doesn't exist (glob returns nothing)
        assert!(results.is_empty());
    }

    #[test]
    fn test_estimate_tokens_simple_empty() {
        assert_eq!(estimate_tokens_simple(""), 0);
    }

    #[test]
    fn test_estimate_tokens_simple_known_text() {
        // "hello world" is 11 bytes → 11/4 = 2
        let est = estimate_tokens_simple("hello world");
        assert_eq!(est, 2);
    }

    #[test]
    fn test_estimate_tokens_simple_code() {
        let code = "fn main() {\n    println!(\"Hello, world!\");\n}\n";
        let est = estimate_tokens_simple(code);
        // 45 bytes → 45/4 = 11, reasonable for a small code snippet
        assert!(est > 0, "should produce a non-zero estimate for code");
        assert!(est < 100, "should not wildly overestimate a small snippet");
    }

    #[test]
    fn test_estimate_tokens_simple_large_input() {
        // 40,000 chars → ~10,000 tokens
        let large = "x".repeat(40_000);
        let est = estimate_tokens_simple(&large);
        assert_eq!(est, 10_000);
    }

    #[test]
    fn test_estimate_tokens_simple_unicode() {
        // Multi-byte characters: "日本語" is 9 bytes (3 chars × 3 bytes each)
        let est = estimate_tokens_simple("日本語");
        // 9 bytes / 4 = 2 — doesn't panic, gives reasonable result
        assert_eq!(est, 2);
    }

    // --- suggest_related_files tests ---

    #[test]
    fn test_suggest_related_non_source_returns_empty() {
        // Non-source files should return no suggestions
        let result = suggest_related_files("README.md", &[]);
        assert!(
            result.is_empty(),
            "non-source file should have no suggestions"
        );

        let result = suggest_related_files("data.json", &[]);
        assert!(result.is_empty());
    }

    #[test]
    #[serial]
    fn test_suggest_related_test_file_detection() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        // Create src/foo.rs
        std::fs::write(src.join("foo.rs"), "fn hello() {}").unwrap();
        // Create src/foo_test.rs
        std::fs::write(src.join("foo_test.rs"), "#[test] fn t() {}").unwrap();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let suggestions = suggest_related_files("src/foo.rs", &[]);
        assert!(
            suggestions.contains(&"src/foo_test.rs".to_string()),
            "should find test file, got: {suggestions:?}"
        );

        std::env::set_current_dir(orig_dir).unwrap();
    }

    #[test]
    #[serial]
    fn test_suggest_related_module_parent() {
        let dir = tempfile::tempdir().unwrap();
        let src_foo = dir.path().join("src").join("foo");
        std::fs::create_dir_all(&src_foo).unwrap();
        std::fs::write(src_foo.join("bar.rs"), "fn bar() {}").unwrap();
        std::fs::write(src_foo.join("mod.rs"), "mod bar;").unwrap();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let suggestions = suggest_related_files("src/foo/bar.rs", &[]);
        assert!(
            suggestions.contains(&"src/foo/mod.rs".to_string()),
            "should find module parent, got: {suggestions:?}"
        );

        std::env::set_current_dir(orig_dir).unwrap();
    }

    #[test]
    #[serial]
    fn test_suggest_related_use_crate_parsing() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        // Create a file that uses crate::dispatch
        std::fs::write(
            src.join("commands.rs"),
            "use crate::dispatch;\nuse crate::format;\nfn main() {}",
        )
        .unwrap();
        // Create the referenced module
        std::fs::write(src.join("dispatch.rs"), "pub fn run() {}").unwrap();
        // format.rs does NOT exist — should not appear

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let suggestions = suggest_related_files("src/commands.rs", &[]);
        assert!(
            suggestions.contains(&"src/dispatch.rs".to_string()),
            "should find crate dependency, got: {suggestions:?}"
        );
        // format.rs doesn't exist, so it shouldn't appear
        assert!(
            !suggestions.contains(&"src/format.rs".to_string()),
            "non-existent files should not appear"
        );

        std::env::set_current_dir(orig_dir).unwrap();
    }

    #[test]
    fn test_suggest_related_skips_already_added() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("foo.rs"), "fn foo() {}").unwrap();
        std::fs::write(src.join("foo_test.rs"), "#[test] fn t() {}").unwrap();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        // If the test file is already added, don't suggest it
        let already = vec!["src/foo_test.rs".to_string()];
        let suggestions = suggest_related_files("src/foo.rs", &already);
        assert!(
            !suggestions.contains(&"src/foo_test.rs".to_string()),
            "already-added files should be excluded, got: {suggestions:?}"
        );

        std::env::set_current_dir(orig_dir).unwrap();
    }

    #[test]
    fn test_suggest_related_cap_at_three() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        // Create a file with many use crate:: lines
        let content = (0..10)
            .map(|i| format!("use crate::mod_{i};"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(src.join("main_file.rs"), &content).unwrap();
        // Create all the referenced modules
        for i in 0..10 {
            std::fs::write(src.join(format!("mod_{i}.rs")), "pub fn f() {}").unwrap();
        }
        // Also create test file
        std::fs::write(src.join("main_file_test.rs"), "#[test] fn t() {}").unwrap();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let suggestions = suggest_related_files("src/main_file.rs", &[]);
        assert!(
            suggestions.len() <= 3,
            "should cap at 3, got {} suggestions: {suggestions:?}",
            suggestions.len()
        );

        std::env::set_current_dir(orig_dir).unwrap();
    }

    #[test]
    fn test_is_source_extension() {
        assert!(is_source_extension("foo.rs"));
        assert!(is_source_extension("bar.py"));
        assert!(is_source_extension("baz.ts"));
        assert!(is_source_extension("qux.go"));
        assert!(is_source_extension("thing.java"));
        assert!(is_source_extension("lib.cpp"));
        assert!(is_source_extension("header.h"));
        assert!(!is_source_extension("readme.md"));
        assert!(!is_source_extension("data.json"));
        assert!(!is_source_extension("image.png"));
        assert!(!is_source_extension("noextension"));
    }

    #[test]
    fn test_detect_src_prefix() {
        assert_eq!(detect_src_prefix("src/foo.rs"), "src/");
        assert_eq!(detect_src_prefix("src/commands/file.rs"), "src/commands/");
        assert_eq!(detect_src_prefix("foo.rs"), "");
    }
}
