//! Code review command handlers: /review, /blame, and non-interactive review.

use crate::commands_session::auto_compact_if_needed;
use crate::format::*;
use crate::git::*;
use crate::prompt::run_prompt;

use yoagent::agent::Agent;
use yoagent::*;

/// Build a review prompt for either staged changes or a specific file.
/// Returns None if there's nothing to review, Some(prompt) otherwise.
pub fn build_review_content(arg: &str) -> Option<(String, String)> {
    let arg = arg.trim();
    if arg.is_empty() {
        // Review staged changes
        match get_staged_diff() {
            None => {
                eprintln!("{RED}  error: not in a git repository{RESET}\n");
                None
            }
            Some(diff) if diff.trim().is_empty() => {
                // Fall back to unstaged diff if nothing staged
                let unstaged = run_git(&["diff"]).unwrap_or_default();
                if unstaged.trim().is_empty() {
                    eprintln!("{DIM}  nothing to review — no staged or unstaged changes{RESET}\n");
                    None
                } else {
                    eprintln!("{DIM}  reviewing unstaged changes...{RESET}");
                    Some(("unstaged changes".to_string(), unstaged))
                }
            }
            Some(diff) => {
                eprintln!("{DIM}  reviewing staged changes...{RESET}");
                Some(("staged changes".to_string(), diff))
            }
        }
    } else if arg.starts_with("--pr") {
        // Review a PR: --pr <number>
        build_review_content_pr(arg)
    } else if arg.contains("..") && !std::path::Path::new(arg).exists() {
        // Review a commit range: HEAD~3..HEAD, abc123..def456, etc.
        // An existing path wins: `../shared/src/lib.rs` is a file, not a range
        // (a commit range never names something that exists on disk).
        build_review_content_range(arg)
    } else {
        // Review a specific file
        let path = std::path::Path::new(arg);
        if !path.exists() {
            eprintln!("{RED}  error: file not found: {arg}{RESET}\n");
            return None;
        }
        match std::fs::read_to_string(path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    eprintln!("{DIM}  file is empty — nothing to review{RESET}\n");
                    None
                } else {
                    eprintln!("{DIM}  reviewing {arg}...{RESET}");
                    Some((arg.to_string(), content))
                }
            }
            Err(e) => {
                eprintln!("{RED}  error reading {arg}: {e}{RESET}\n");
                None
            }
        }
    }
}

/// Build review content from a git commit range (e.g. `HEAD~3..HEAD`).
fn build_review_content_range(arg: &str) -> Option<(String, String)> {
    match run_git(&["diff", arg]) {
        Ok(diff) if !diff.trim().is_empty() => {
            eprintln!("{DIM}  reviewing diff {arg}...{RESET}");
            Some((format!("diff {arg}"), diff))
        }
        Ok(_) => {
            eprintln!("{DIM}  no changes in range {arg}{RESET}\n");
            None
        }
        Err(e) => {
            eprintln!("{RED}  error: git diff {arg} failed: {e}{RESET}\n");
            None
        }
    }
}

/// Build review content from a GitHub PR number (e.g. `--pr 123`).
fn build_review_content_pr(arg: &str) -> Option<(String, String)> {
    let pr_num = arg.strip_prefix("--pr").unwrap_or("").trim();
    if pr_num.is_empty() {
        eprintln!("{RED}  error: --pr requires a PR number (e.g. --pr 123){RESET}\n");
        return None;
    }
    // Validate it's a number
    if pr_num.parse::<u64>().is_err() {
        eprintln!("{RED}  error: invalid PR number: {pr_num}{RESET}\n");
        return None;
    }
    // Use gh CLI to get the PR diff
    match std::process::Command::new("gh")
        .args(["pr", "diff", pr_num])
        .output()
    {
        Ok(output) if output.status.success() => {
            let diff = String::from_utf8_lossy(&output.stdout).to_string();
            if diff.trim().is_empty() {
                eprintln!("{DIM}  PR #{pr_num} has no diff{RESET}\n");
                None
            } else {
                eprintln!("{DIM}  reviewing PR #{pr_num}...{RESET}");
                Some((format!("PR #{pr_num}"), diff))
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("{RED}  error: gh pr diff {pr_num} failed: {stderr}{RESET}\n");
            None
        }
        Err(e) => {
            eprintln!("{RED}  error: failed to run gh CLI: {e}{RESET}");
            eprintln!("{DIM}  install gh: https://cli.github.com/{RESET}\n");
            None
        }
    }
}

/// Run a non-interactive code review and return the review text.
///
/// This is the entry point for `yoyo review` CLI subcommand — it builds
/// the review content, creates a one-shot agent, runs the prompt, and
/// returns the agent's response text. No REPL, no interactive session.
///
/// # Arguments
/// * `arg` — the review target: empty (staged/unstaged), commit range, `--pr N`, or file path
/// * `agent_config` — pre-built agent configuration for creating the side agent
///
/// # Returns
/// * `Ok(review_text)` — the review completed successfully
/// * `Err(message)` — nothing to review or an error occurred
pub async fn run_non_interactive_review(
    arg: &str,
    agent_config: &crate::agent_builder::AgentConfig,
) -> Result<String, String> {
    let (effort, remaining) = parse_review_effort(arg);
    let (label, content) =
        build_review_content(&remaining).ok_or_else(|| "nothing to review".to_string())?;

    let prompt = build_review_prompt(&label, &content, effort);

    // Build a side agent — one-shot, no tools, concise
    let mut side_agent = agent_config.build_side_agent();

    let mut rx = side_agent.prompt(&prompt).await;

    let mut collected = String::new();
    let mut md_renderer = MarkdownRenderer::new();

    loop {
        match rx.recv().await {
            Some(AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            }) => {
                collected.push_str(&delta);
                // Stream to stderr so stdout stays clean for piping
                let rendered = md_renderer.render_delta(&delta);
                if !rendered.is_empty() {
                    eprint!("{rendered}");
                }
            }
            Some(AgentEvent::MessageEnd { .. }) => {
                let tail = md_renderer.flush();
                if !tail.is_empty() {
                    eprint!("{tail}");
                }
            }
            Some(AgentEvent::AgentEnd { .. }) => break,
            None => break,
            _ => {}
        }
    }

    side_agent.finish().await;
    eprintln!(); // newline after streamed output

    // Show cost on stderr
    let messages = side_agent.messages();
    let mut usage = Usage::default();
    for msg in messages {
        if let AgentMessage::Llm(yoagent::types::Message::Assistant { usage: u, .. }) = msg {
            usage.input += u.input;
            usage.output += u.output;
            usage.cache_read += u.cache_read;
            usage.cache_write += u.cache_write;
        }
    }
    let total_tokens = usage.input + usage.output;
    if total_tokens > 0 {
        let cost = estimate_cost(&usage, &agent_config.model);
        if let Some(c) = cost {
            eprintln!("{DIM}  review: {} tokens, ${:.4}{RESET}", total_tokens, c);
        } else {
            eprintln!("{DIM}  review: {} tokens{RESET}", total_tokens);
        }
    }

    if collected.trim().is_empty() {
        Err("agent returned empty response".to_string())
    } else {
        Ok(collected)
    }
}

/// Review effort level — controls depth and focus of the code review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewEffort {
    /// Focus on bugs and security only. Skip style nits. Be terse.
    Quick,
    /// Default: bugs, security, style, performance, suggestions.
    Normal,
    /// Deep review: also check error handling edge cases, API contract
    /// violations, test coverage gaps, documentation accuracy, concurrency safety.
    Thorough,
}

impl ReviewEffort {
    /// Human-readable label for the effort level.
    pub fn label(&self) -> &'static str {
        match self {
            ReviewEffort::Quick => "quick",
            ReviewEffort::Normal => "normal",
            ReviewEffort::Thorough => "thorough",
        }
    }
}

/// Parse effort flags from review input.
///
/// Strips `--quick` or `--thorough` from the input and returns the
/// effort level plus the remaining argument string (file path, range, etc.).
pub fn parse_review_effort(input: &str) -> (ReviewEffort, String) {
    let mut effort = ReviewEffort::Normal;
    let mut remaining_parts: Vec<&str> = Vec::new();

    for part in input.split_whitespace() {
        match part {
            "--quick" => effort = ReviewEffort::Quick,
            "--thorough" => effort = ReviewEffort::Thorough,
            _ => remaining_parts.push(part),
        }
    }

    (effort, remaining_parts.join(" "))
}

/// Build the review prompt to send to the AI.
pub fn build_review_prompt(label: &str, content: &str, effort: ReviewEffort) -> String {
    // Truncate if very large
    let max_chars = 30_000;
    let content_preview = if content.len() > max_chars {
        let truncated = safe_truncate(content, max_chars);
        format!(
            "{truncated}\n\n... (truncated, {} more chars)",
            content.len() - max_chars
        )
    } else {
        content.to_string()
    };

    let criteria = match effort {
        ReviewEffort::Quick => {
            "Focus on critical issues ONLY:\n\n\
             1. **Bugs** — logic errors, crashes, data corruption\n\
             2. **Security** — injection, unsafe operations, credential exposure\n\n\
             Skip style, performance, and minor suggestions. Be terse — one line per finding."
                .to_string()
        }
        ReviewEffort::Normal => {
            "Look for:\n\n\
             1. **Bugs** — logic errors, off-by-one errors, null/None handling, race conditions\n\
             2. **Security** — injection vulnerabilities, unsafe operations, credential exposure\n\
             3. **Style** — naming, idiomatic patterns, unnecessary complexity, dead code\n\
             4. **Performance** — obvious inefficiencies, unnecessary allocations, N+1 patterns\n\
             5. **Suggestions** — improvements, missing error handling, better approaches\n\n\
             Be specific: reference line numbers or code snippets. Be concise — skip things that look fine.\n\
             If the code looks good overall, say so briefly and note any minor suggestions."
                .to_string()
        }
        ReviewEffort::Thorough => {
            "Perform an exhaustive code review. Check ALL of the following:\n\n\
             1. **Bugs** — logic errors, off-by-one errors, null/None handling, race conditions\n\
             2. **Security** — injection vulnerabilities, unsafe operations, credential exposure\n\
             3. **Style** — naming, idiomatic patterns, unnecessary complexity, dead code\n\
             4. **Performance** — obvious inefficiencies, unnecessary allocations, N+1 patterns\n\
             5. **Error Handling** — missing error paths, swallowed errors, unhelpful error messages\n\
             6. **Edge Cases** — boundary conditions, empty inputs, overflow, unicode handling\n\
             7. **API Contracts** — function signatures match usage, invariants maintained\n\
             8. **Test Coverage** — untested paths, missing assertions, fragile test assumptions\n\
             9. **Documentation** — stale comments, missing doc comments, misleading descriptions\n\
             10. **Concurrency** — data races, deadlocks, lock ordering, shared mutable state\n\n\
             Be exhaustive. Reference line numbers. For each finding, explain the risk and suggest a fix.\n\
             Group findings by severity: critical → major → minor → nit."
                .to_string()
        }
    };

    let effort_label = match effort {
        ReviewEffort::Normal => String::new(),
        other => format!(" [{} review]", other.label()),
    };

    format!(
        "Review the following code ({label}){effort_label}. {criteria}\n\n```\n{content_preview}\n```"
    )
}

/// Handle the /review command: review staged changes or a specific file.
/// Returns the review prompt if sent to AI, None otherwise.
pub async fn handle_review(
    input: &str,
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let arg = input.strip_prefix("/review").unwrap_or("").trim();
    let (effort, remaining) = parse_review_effort(arg);

    match build_review_content(&remaining) {
        Some((label, content)) => {
            if effort != ReviewEffort::Normal {
                eprintln!("{DIM}  effort: {}{RESET}", effort.label());
            }
            let prompt = build_review_prompt(&label, &content, effort);
            run_prompt(agent, &prompt, session_total, model).await;
            auto_compact_if_needed(agent);
            Some(prompt)
        }
        None => None,
    }
}

/// Parsed arguments for `/blame`.
#[derive(Debug, PartialEq)]
pub struct BlameArgs {
    pub file: String,
    pub range: Option<(usize, usize)>,
}

/// Parse `/blame <file>` or `/blame <file>:<start>-<end>`.
pub fn parse_blame_args(input: &str) -> Result<BlameArgs, String> {
    let arg = input.strip_prefix("/blame").unwrap_or(input).trim();

    if arg.is_empty() {
        return Err("Usage: /blame <file> or /blame <file>:<start>-<end>".to_string());
    }

    // Check for <file>:<start>-<end> pattern
    if let Some(colon_pos) = arg.rfind(':') {
        // SAFETY: colon_pos from rfind(':') — ':' is ASCII (1 byte), so colon_pos
        // and colon_pos + 1 are always valid char boundaries.
        let file_part = &arg[..colon_pos];
        let range_part = &arg[colon_pos + 1..];

        if let Some(dash_pos) = range_part.find('-') {
            // SAFETY: dash_pos from find('-') — '-' is ASCII (1 byte), so dash_pos
            // and dash_pos + 1 are always valid char boundaries within range_part.
            let start_str = &range_part[..dash_pos];
            let end_str = &range_part[dash_pos + 1..];

            if let (Ok(start), Ok(end)) = (start_str.parse::<usize>(), end_str.parse::<usize>()) {
                if start == 0 || end == 0 {
                    return Err("Line numbers must be >= 1".to_string());
                }
                if start > end {
                    return Err(format!("Invalid range: start ({start}) > end ({end})"));
                }
                if !file_part.is_empty() {
                    return Ok(BlameArgs {
                        file: file_part.to_string(),
                        range: Some((start, end)),
                    });
                }
            }
        }
    }

    // No valid range found — treat entire input as file path
    Ok(BlameArgs {
        file: arg.to_string(),
        range: None,
    })
}

/// Colorize a single line of `git blame` output.
///
/// Typical git blame line format:
/// `abc1234f (Author Name  2024-01-15 10:30:00 +0000  42) line content`
///
/// We colorize:
/// - Commit hash → DIM
/// - Author name → CYAN
/// - Date/time → DIM
/// - Line number → YELLOW
/// - Code content → default
pub fn colorize_blame_line(line: &str) -> String {
    // git blame output: <hash> (<author> <date> <time> <tz> <lineno>) <code>
    // Find the opening paren that starts the author section
    let Some(paren_open) = line.find('(') else {
        return line.to_string();
    };
    let Some(paren_close) = line.find(')') else {
        return line.to_string();
    };
    if paren_close <= paren_open {
        return line.to_string();
    }

    // SAFETY: paren_open from find('('), paren_close from find(')') — both ASCII
    // (1 byte each), so paren_open, paren_open + 1, paren_close, and paren_close + 1
    // are all valid char boundaries.
    let hash = &line[..paren_open];
    let annotation = &line[paren_open + 1..paren_close];
    let code = if paren_close + 1 < line.len() {
        &line[paren_close + 1..]
    } else {
        ""
    };

    // Inside the annotation: "Author Name  2024-01-15 10:30:00 +0000  42"
    // Try to find the date pattern (YYYY-MM-DD) to split author from date+lineno
    let mut author = annotation;
    let mut date_and_lineno = "";

    // Look for a date pattern: 4-digit year followed by -
    for (i, _) in annotation.char_indices() {
        if i + 10 <= annotation.len() {
            // SAFETY: i from char_indices() is always a valid char boundary.
            let slice = &annotation[i..];
            if slice.len() >= 10
                // SAFETY: as_bytes() indexing is safe because slice.len() >= 10
                // and bytes 0-9 are checked individually. The sub-slices [..4],
                // [5..7], [8..10] are safe because all checked bytes are ASCII digits
                // or '-', so every index falls on a char boundary.
                && slice.as_bytes()[4] == b'-'
                && slice.as_bytes()[7] == b'-'
                && slice[..4].chars().all(|c| c.is_ascii_digit())
                && slice[5..7].chars().all(|c| c.is_ascii_digit())
                && slice[8..10].chars().all(|c| c.is_ascii_digit())
            {
                // SAFETY: i from char_indices() — always a valid char boundary.
                author = annotation[..i].trim_end();
                date_and_lineno = &annotation[i..];
                break;
            }
        }
    }

    // Try to split the lineno from date portion
    // The lineno is typically the last whitespace-separated token
    let (date_part, lineno_part) =
        if let Some(last_space) = date_and_lineno.rfind(char::is_whitespace) {
            // SAFETY: rfind(char::is_whitespace) returns the byte index of the
            // start of the matching char — always a valid char boundary.
            let candidate = date_and_lineno[last_space..].trim();
            if candidate.chars().all(|c| c.is_ascii_digit()) && !candidate.is_empty() {
                (&date_and_lineno[..last_space], candidate)
            } else {
                (date_and_lineno, "")
            }
        } else {
            (date_and_lineno, "")
        };

    format!(
        "{DIM}{hash}{RESET}({CYAN}{author}{RESET} {DIM}{date_part}{RESET} {YELLOW}{lineno_part}{RESET}){code}"
    )
}

/// Colorize full `git blame` output (multiple lines).
pub fn colorize_blame(output: &str) -> String {
    output
        .lines()
        .map(colorize_blame_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Handle the `/blame` command.
pub fn handle_blame(input: &str) {
    let args = match parse_blame_args(input) {
        Ok(a) => a,
        Err(e) => {
            println!("  {RED}✗{RESET} {e}");
            return;
        }
    };

    let mut cmd = vec!["blame".to_string()];
    if let Some((start, end)) = args.range {
        cmd.push(format!("-L{start},{end}"));
    }
    cmd.push(args.file.clone());

    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    match run_git(&cmd_refs) {
        Ok(output) => {
            if output.trim().is_empty() {
                println!("  {DIM}(no blame output){RESET}");
            } else {
                println!();
                println!("{}", colorize_blame(&output));
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("no such path") || msg.contains("No such file") {
                println!("  {RED}✗{RESET} File not found: {DIM}{}{RESET}", args.file);
            } else if msg.contains("not a git repository") || msg.contains("fatal: not a git") {
                println!("  {RED}✗{RESET} Not in a git repository");
            } else {
                println!("  {RED}✗{RESET} {msg}");
            }
        }
    }
}

/// A single inline review comment for a PR.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewComment {
    pub path: String,
    pub line: u64,
    pub body: String,
}

/// Parse a structured review JSON string into a list of review comments.
///
/// Expects a JSON array of objects with `path`, `line`, and `body` fields:
/// ```json
/// [
///   {"path": "src/main.rs", "line": 42, "body": "Consider error handling here"},
///   {"path": "src/lib.rs", "line": 10, "body": "This could be simplified"}
/// ]
/// ```
pub fn parse_review_comments(json: &str) -> Result<Vec<ReviewComment>, String> {
    // Find the JSON array in the response — it may be wrapped in markdown code fences
    let trimmed = json.trim();
    let json_str = if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            &trimmed[start..=end]
        } else {
            return Err("No closing ']' found in review JSON".to_string());
        }
    } else {
        return Err("No JSON array found in review output".to_string());
    };

    // Parse using a simple JSON parser (no serde dependency needed)
    let mut comments = Vec::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let mut obj_start = None;

    for (i, ch) in json_str.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '{' => {
                if depth == 1 {
                    obj_start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 1 {
                    if let Some(start) = obj_start {
                        let obj_str = &json_str[start..=i];
                        if let Some(comment) = parse_single_comment(obj_str) {
                            comments.push(comment);
                        }
                    }
                    obj_start = None;
                }
            }
            '[' if depth == 0 => {
                depth = 1;
            }
            ']' if depth == 1 => {
                break;
            }
            _ => {}
        }
    }

    if comments.is_empty() {
        return Err("No valid review comments found in JSON".to_string());
    }
    Ok(comments)
}

/// Parse a single JSON object string into a ReviewComment.
fn parse_single_comment(obj: &str) -> Option<ReviewComment> {
    let path = extract_json_string_field(obj, "path")?;
    let line = extract_json_number_field(obj, "line")?;
    let body = extract_json_string_field(obj, "body")?;
    Some(ReviewComment { path, line, body })
}

/// Extract a string value for a given key from a JSON object string.
fn extract_json_string_field(obj: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let key_pos = obj.find(&pattern)?;
    let after_key = &obj[key_pos + pattern.len()..];
    // Skip whitespace and colon
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    // Expect a quoted string
    if !after_colon.starts_with('"') {
        return None;
    }
    let content = &after_colon[1..];
    let mut result = String::new();
    let mut escape = false;
    for ch in content.chars() {
        if escape {
            match ch {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                '"' => result.push('"'),
                '\\' => result.push('\\'),
                '/' => result.push('/'),
                _ => {
                    result.push('\\');
                    result.push(ch);
                }
            }
            escape = false;
        } else if ch == '\\' {
            escape = true;
        } else if ch == '"' {
            return Some(result);
        } else {
            result.push(ch);
        }
    }
    None // unterminated string
}

/// Extract a numeric value for a given key from a JSON object string.
fn extract_json_number_field(obj: &str, key: &str) -> Option<u64> {
    let pattern = format!("\"{}\"", key);
    let key_pos = obj.find(&pattern)?;
    let after_key = &obj[key_pos + pattern.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    // Read digits
    let num_str: String = after_colon
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num_str.parse().ok()
}

/// Build a review prompt that requests structured JSON output for posting.
///
/// The prompt asks the AI to produce both a human-readable review summary
/// and a JSON array of inline comments that can be posted to GitHub.
pub fn build_review_prompt_structured(pr_number: u32, pr_info: &str, diff: &str) -> String {
    let pr_section = if pr_info.trim().is_empty() {
        String::new()
    } else {
        format!("## PR Description\n\n{}\n\n", pr_info.trim())
    };

    format!(
        "Review this pull request (PR #{pr_number}). Analyze the diff for:\n\
         - Potential bugs or logic errors\n\
         - Code quality issues\n\
         - Missing error handling\n\
         - Performance concerns\n\
         - Suggestions for improvement\n\n\
         Be specific — reference file paths and line numbers from the diff.\n\
         Praise good patterns too. Be constructive.\n\n\
         {pr_section}\
         ## Diff\n\n```diff\n{diff}\n```\n\n\
         ## IMPORTANT: Output Format\n\n\
         After your review analysis, output a JSON code block with the inline comments to post.\n\
         Each comment should reference a file path and line number FROM THE DIFF (the '+' side \
         line number for additions, or the original line number for context about existing code).\n\n\
         Output the JSON block like this:\n\n\
         ```json\n\
         [\n\
         \x20 {{\"path\": \"src/example.rs\", \"line\": 42, \"body\": \"Consider adding error handling here\"}},\n\
         \x20 {{\"path\": \"src/lib.rs\", \"line\": 10, \"body\": \"Nice refactor! Much cleaner.\"}}\n\
         ]\n\
         ```\n\n\
         Rules for the JSON:\n\
         - `path` must be the file path as shown in the diff (e.g., `src/main.rs`)\n\
         - `line` must be a line number from the NEW side of the diff (the '+' lines)\n\
         - `body` should be a constructive review comment\n\
         - Include 3-10 comments covering the most important observations\n\
         - Only include comments where you have substantive feedback"
    )
}

/// Post a PR review with inline comments using `gh api`.
///
/// Takes the PR number and parsed review comments, and posts them as a
/// GitHub pull request review with `COMMENT` event type.
pub fn post_pr_review(pr_number: u32, comments: &[ReviewComment]) -> Result<String, String> {
    if comments.is_empty() {
        return Err("No comments to post".to_string());
    }

    // Build the review body (summary)
    let summary = format!(
        "🐙 **yoyo review** — {} comment{} on PR #{}",
        comments.len(),
        if comments.len() == 1 { "" } else { "s" },
        pr_number
    );

    // Build the comments array for the API
    // Format: [{"path": "...", "line": N, "body": "..."}]
    let comments_json: Vec<String> = comments
        .iter()
        .map(|c| {
            let escaped_body = c
                .body
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n");
            let escaped_path = c.path.replace('\\', "\\\\").replace('"', "\\\"");
            format!(
                "{{\"path\":\"{}\",\"line\":{},\"body\":\"{}\"}}",
                escaped_path, c.line, escaped_body
            )
        })
        .collect();
    let comments_array = format!("[{}]", comments_json.join(","));

    let escaped_summary = summary.replace('\\', "\\\\").replace('"', "\\\"");

    // Build the full review payload
    let payload = format!(
        "{{\"event\":\"COMMENT\",\"body\":\"{}\",\"comments\":{}}}",
        escaped_summary, comments_array
    );

    // Post via gh api
    let output = std::process::Command::new("gh")
        .args([
            "api",
            &format!("repos/{{owner}}/{{repo}}/pulls/{pr_number}/reviews"),
            "--method",
            "POST",
            "--input",
            "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("`gh` CLI not found or failed to start: {e}"))?;

    // Write JSON payload to stdin
    use std::io::Write;
    let mut child = output;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload.as_bytes())
            .map_err(|e| format!("Failed to write to gh stdin: {e}"))?;
    }

    let result = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for gh: {e}"))?;

    if result.status.success() {
        Ok(format!(
            "✓ Posted review with {} inline comment{} to PR #{}",
            comments.len(),
            if comments.len() == 1 { "" } else { "s" },
            pr_number
        ))
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        Err(format!("GitHub API error: {}", stderr.trim()))
    }
}

/// Extract the JSON review block from the AI's response text.
///
/// Looks for a ```json code block and extracts its contents.
pub fn extract_review_json(response: &str) -> Option<String> {
    // Look for ```json block
    let json_start_markers = ["```json\n", "```json\r\n"];
    for marker in &json_start_markers {
        if let Some(start) = response.find(marker) {
            let content_start = start + marker.len();
            if let Some(end) = response[content_start..].find("```") {
                return Some(response[content_start..content_start + end].to_string());
            }
        }
    }
    // Fallback: look for a bare JSON array
    if let Some(start) = response.find('[') {
        if let Some(end) = response.rfind(']') {
            return Some(response[start..=end].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
    use serial_test::serial;

    #[test]
    fn build_review_prompt_contains_label() {
        let prompt = build_review_prompt("staged changes", "fn main() {}", ReviewEffort::Normal);
        assert!(
            prompt.contains("staged changes"),
            "Prompt should include the label"
        );
    }

    #[test]
    fn build_review_prompt_contains_content() {
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let prompt = build_review_prompt("test.rs", code, ReviewEffort::Normal);
        assert!(prompt.contains(code), "Prompt should include the code");
    }

    #[test]
    fn build_review_prompt_contains_review_criteria() {
        let prompt = build_review_prompt("file.rs", "let x = 1;", ReviewEffort::Normal);
        assert!(prompt.contains("Bugs"), "Should mention bugs");
        assert!(prompt.contains("Security"), "Should mention security");
        assert!(prompt.contains("Style"), "Should mention style");
        assert!(prompt.contains("Performance"), "Should mention performance");
        assert!(prompt.contains("Suggestions"), "Should mention suggestions");
    }

    #[test]
    fn build_review_prompt_truncates_large_content() {
        let large_content = "x".repeat(50_000);
        let prompt = build_review_prompt("big.rs", &large_content, ReviewEffort::Normal);
        assert!(
            prompt.contains("truncated"),
            "Large content should be truncated"
        );
        assert!(
            prompt.contains("20000 more chars"),
            "Should show remaining char count"
        );
        // The prompt should be shorter than the original content
        assert!(
            prompt.len() < large_content.len(),
            "Prompt should be shorter than 50k"
        );
    }

    #[test]
    fn build_review_prompt_does_not_truncate_small_content() {
        let small_content = "fn hello() { println!(\"hi\"); }";
        let prompt = build_review_prompt("small.rs", small_content, ReviewEffort::Normal);
        assert!(
            !prompt.contains("truncated"),
            "Small content should not be truncated"
        );
        assert!(
            prompt.contains(small_content),
            "Full content should be present"
        );
    }

    #[test]
    fn build_review_prompt_wraps_in_code_block() {
        let prompt = build_review_prompt("test.rs", "let x = 42;", ReviewEffort::Normal);
        assert!(prompt.contains("```"), "Content should be in a code block");
    }

    #[test]
    fn test_review_command_recognized() {
        assert!(!is_unknown_command("/review"));
        assert!(!is_unknown_command("/review src/main.rs"));
        assert!(
            KNOWN_COMMANDS.contains(&"/review"),
            "/review should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_review_command_matching() {
        // /review should match exact or with space separator, not /reviewing
        let review_matches = |s: &str| s == "/review" || s.starts_with("/review ");
        assert!(review_matches("/review"));
        assert!(review_matches("/review src/main.rs"));
        assert!(review_matches("/review Cargo.toml"));
        assert!(!review_matches("/reviewing"));
        assert!(!review_matches("/reviewer"));
    }

    #[test]
    fn test_build_review_prompt_contains_content() {
        let prompt = build_review_prompt(
            "staged changes",
            "fn main() {\n    println!(\"hello\");\n}",
            ReviewEffort::Normal,
        );
        assert!(
            prompt.contains("staged changes"),
            "Should mention the label"
        );
        assert!(prompt.contains("fn main()"), "Should contain the code");
        assert!(prompt.contains("Bugs"), "Should ask for bug review");
        assert!(
            prompt.contains("Security"),
            "Should ask for security review"
        );
        assert!(prompt.contains("Style"), "Should ask for style review");
        assert!(
            prompt.contains("Performance"),
            "Should ask for performance review"
        );
        assert!(prompt.contains("Suggestions"), "Should ask for suggestions");
    }

    #[test]
    fn test_build_review_prompt_truncates_large_content() {
        let large_content = "x".repeat(40_000);
        let prompt = build_review_prompt("big file", &large_content, ReviewEffort::Normal);
        assert!(
            prompt.contains("truncated"),
            "Large content should be truncated"
        );
        assert!(
            prompt.len() < 40_000,
            "Prompt should be truncated, got {} chars",
            prompt.len()
        );
    }

    #[test]
    fn test_build_review_content_nonexistent_file() {
        let result = build_review_content("nonexistent_file_xyz_12345.rs");
        assert!(result.is_none(), "Nonexistent file should return None");
    }

    #[test]
    #[serial]
    fn test_build_review_content_existing_file() {
        // Use CARGO_MANIFEST_DIR for an absolute path to avoid CWD races
        // with other tests that call set_current_dir
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let cargo_toml = format!("{manifest_dir}/Cargo.toml");
        let result = build_review_content(&cargo_toml);
        assert!(result.is_some(), "Existing file should return Some");
        let (label, content) = result.unwrap();
        assert_eq!(label, cargo_toml);
        assert!(!content.is_empty(), "Content should not be empty");
    }

    #[test]
    fn test_build_review_content_empty_arg_in_git_repo() {
        // Empty arg reviews staged/unstaged changes
        // In CI, this may or may not have changes — just verify it doesn't panic
        let result = build_review_content("");
        // Result depends on git state — either Some or None is valid
        if let Some((label, _content)) = result {
            assert!(
                label.contains("changes"),
                "Label should describe what's being reviewed: {label}"
            );
        }
    }

    #[test]
    fn test_review_help_text_present() {
        // Verify /review appears in the help output by checking the handle_help function output
        // We can't easily capture stdout, but we can verify the command is in KNOWN_COMMANDS
        // and that the help text format is correct
        assert!(KNOWN_COMMANDS.contains(&"/review"));
    }

    #[test]
    fn test_parse_blame_args_file_only() {
        let result = parse_blame_args("/blame src/main.rs").unwrap();
        assert_eq!(result.file, "src/main.rs");
        assert_eq!(result.range, None);
    }

    #[test]
    fn test_parse_blame_args_with_range() {
        let result = parse_blame_args("/blame src/main.rs:10-20").unwrap();
        assert_eq!(result.file, "src/main.rs");
        assert_eq!(result.range, Some((10, 20)));
    }

    #[test]
    fn test_parse_blame_args_single_line_range() {
        let result = parse_blame_args("/blame foo.rs:5-5").unwrap();
        assert_eq!(result.file, "foo.rs");
        assert_eq!(result.range, Some((5, 5)));
    }

    #[test]
    fn test_parse_blame_args_no_args() {
        let result = parse_blame_args("/blame");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Usage"));
    }

    #[test]
    fn test_parse_blame_args_no_args_with_spaces() {
        let result = parse_blame_args("/blame   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_blame_args_invalid_range_reversed() {
        let result = parse_blame_args("/blame foo.rs:20-10");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("start"));
    }

    #[test]
    fn test_parse_blame_args_zero_start() {
        let result = parse_blame_args("/blame foo.rs:0-10");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(">= 1"));
    }

    #[test]
    fn test_parse_blame_args_non_numeric_range_treated_as_file() {
        // If the range part doesn't parse as numbers, treat entire input as filename
        let result = parse_blame_args("/blame some:file:thing").unwrap();
        assert_eq!(result.file, "some:file:thing");
        assert_eq!(result.range, None);
    }

    #[test]
    fn test_parse_blame_args_unicode_file_path() {
        // File paths can contain multi-byte UTF-8 characters (e.g., accented letters).
        // The colon and dash splitting must not panic on such paths.
        let result = parse_blame_args("/blame src/données.rs:10-20").unwrap();
        assert_eq!(result.file, "src/données.rs");
        assert_eq!(result.range, Some((10, 20)));

        // Unicode path without range
        let result2 = parse_blame_args("/blame src/données.rs").unwrap();
        assert_eq!(result2.file, "src/données.rs");
        assert_eq!(result2.range, None);

        // Unicode path with colon but non-numeric range (treated as file)
        let result3 = parse_blame_args("/blame café:résumé").unwrap();
        assert_eq!(result3.file, "café:résumé");
        assert_eq!(result3.range, None);
    }

    #[test]
    fn test_colorize_blame_line_typical() {
        let line = "abc1234f (John Doe  2024-01-15 10:30:00 +0000  42) fn main() {";
        let colored = colorize_blame_line(line);
        // Should contain ANSI codes
        assert!(colored.contains("\x1b["));
        // Should still contain the original content
        assert!(colored.contains("John Doe"));
        assert!(colored.contains("fn main()"));
        assert!(colored.contains("abc1234f"));
    }

    #[test]
    fn test_colorize_blame_line_no_paren() {
        // Lines without parens should be returned unchanged
        let line = "some weird line without parens";
        assert_eq!(colorize_blame_line(line), line);
    }

    #[test]
    fn test_colorize_blame_multiple_lines() {
        let input = "abc123 (Alice 2024-01-15 10:00:00 +0000 1) line1\ndef456 (Bob   2024-01-15 10:00:00 +0000 2) line2";
        let colored = colorize_blame(input);
        let lines: Vec<&str> = colored.lines().collect();
        assert_eq!(lines.len(), 2);
        // Both lines should have ANSI codes
        assert!(lines[0].contains("\x1b["));
        assert!(lines[1].contains("\x1b["));
    }

    #[test]
    fn build_review_content_detects_commit_range() {
        // A range with ".." should be treated as a git diff range
        let arg = "HEAD~3..HEAD";
        // We can't test the actual git command in unit tests, but we can
        // verify the function handles the range format by checking it doesn't
        // try to treat it as a file path.
        let result = build_review_content(arg);
        // In test environment, git may fail — but it should NOT try to
        // read it as a file (which would give "file not found").
        // Either None (git error) or Some (if git works) is fine.
        // The key test: it should not panic.
        let _ = result;
    }

    #[test]
    fn build_review_content_treats_existing_dotdot_path_as_a_file() {
        // `/review <path>` is a documented target ("Review a specific file"), but the
        // commit-range heuristic keys on ".." appearing ANYWHERE in the arg — so a real
        // relative path like `../shared/src/lib.rs` was routed to `git diff <path>` and
        // the user got "no changes in range" for a file they asked to have reviewed.
        // An existing path must win over the range heuristic; a range never exists on disk.
        let dir = tempfile::tempdir().expect("create temp dir");
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).expect("create subdir");
        std::fs::write(dir.path().join("review_target.rs"), "fn main() {}\n").expect("write file");

        // A genuinely existing path that contains "..": <tmp>/sub/../review_target.rs
        let dotdot = sub.join("..").join("review_target.rs");
        let arg = dotdot.to_str().expect("utf-8 temp path");
        assert!(arg.contains(".."), "test path must contain '..'");

        let (label, content) = build_review_content(arg)
            .expect("an existing file whose path contains '..' should be reviewable");
        assert_eq!(label, arg, "label should be the requested path");
        assert!(
            content.contains("fn main()"),
            "should return the file's contents, not a diff: {content:?}"
        );
    }

    #[test]
    fn build_review_content_detects_pr_flag() {
        // --pr without a number should print an error
        let result = build_review_content("--pr");
        assert!(result.is_none(), "bare --pr should return None");
    }

    #[test]
    fn build_review_content_pr_invalid_number() {
        let result = build_review_content("--pr abc");
        assert!(result.is_none(), "non-numeric PR should return None");
    }

    #[test]
    fn build_review_prompt_with_diff_label() {
        let prompt = build_review_prompt(
            "diff HEAD~3..HEAD",
            "diff --git a/foo b/foo\n+bar",
            ReviewEffort::Normal,
        );
        assert!(prompt.contains("diff HEAD~3..HEAD"));
        assert!(prompt.contains("+bar"));
    }

    #[test]
    fn build_review_prompt_with_pr_label() {
        let prompt = build_review_prompt("PR #42", "diff --git a/foo b/foo", ReviewEffort::Normal);
        assert!(prompt.contains("PR #42"));
    }

    #[test]
    fn parse_review_comments_basic() {
        let json = r#"[
            {"path": "src/main.rs", "line": 42, "body": "Add error handling"},
            {"path": "src/lib.rs", "line": 10, "body": "Nice refactor!"}
        ]"#;
        let comments = parse_review_comments(json).unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].path, "src/main.rs");
        assert_eq!(comments[0].line, 42);
        assert_eq!(comments[0].body, "Add error handling");
        assert_eq!(comments[1].path, "src/lib.rs");
        assert_eq!(comments[1].line, 10);
    }

    #[test]
    fn parse_review_comments_with_markdown_fences() {
        let json = "Here is the review:\n\n```json\n[\n  {\"path\": \"foo.rs\", \"line\": 1, \"body\": \"ok\"}\n]\n```\n\nDone.";
        let comments = parse_review_comments(json).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].path, "foo.rs");
    }

    #[test]
    fn parse_review_comments_empty_array() {
        let json = "[]";
        let result = parse_review_comments(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No valid review comments"));
    }

    #[test]
    fn parse_review_comments_no_json() {
        let result = parse_review_comments("no json here at all");
        assert!(result.is_err());
    }

    #[test]
    fn parse_review_comments_escaped_body() {
        let json = r#"[{"path": "a.rs", "line": 5, "body": "Fix the \"bug\" here\nand here"}]"#;
        let comments = parse_review_comments(json).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].body, "Fix the \"bug\" here\nand here");
    }

    #[test]
    fn extract_review_json_from_markdown() {
        let text = "Great PR! Here's my review:\n\n```json\n[{\"path\": \"x.rs\", \"line\": 1, \"body\": \"ok\"}]\n```\n\nAll looks good.";
        let json = extract_review_json(text).unwrap();
        assert!(json.contains("x.rs"));
    }

    #[test]
    fn extract_review_json_bare_array() {
        let text = "review: [{\"path\": \"y.rs\", \"line\": 2, \"body\": \"fix\"}]";
        let json = extract_review_json(text).unwrap();
        assert!(json.contains("y.rs"));
    }

    #[test]
    fn extract_review_json_none() {
        assert!(extract_review_json("no json here").is_none());
    }

    #[test]
    fn build_review_prompt_structured_has_json_format() {
        let prompt = build_review_prompt_structured(42, "Fix bugs", "+fn main() {}");
        assert!(prompt.contains("PR #42"));
        assert!(prompt.contains("JSON"));
        assert!(prompt.contains("path"));
        assert!(prompt.contains("line"));
        assert!(prompt.contains("body"));
        assert!(prompt.contains("Fix bugs"));
    }

    #[test]
    fn build_review_prompt_structured_empty_pr_info() {
        let prompt = build_review_prompt_structured(10, "", "diff content");
        assert!(prompt.contains("PR #10"));
        // Should not have an empty PR description section
        assert!(!prompt.contains("## PR Description"));
    }

    #[test]
    fn review_comment_equality() {
        let a = ReviewComment {
            path: "a.rs".to_string(),
            line: 1,
            body: "ok".to_string(),
        };
        let b = ReviewComment {
            path: "a.rs".to_string(),
            line: 1,
            body: "ok".to_string(),
        };
        assert_eq!(a, b);
    }

    // --- ReviewEffort tests ---

    #[test]
    fn test_parse_review_effort_default() {
        let (effort, remaining) = parse_review_effort("src/main.rs");
        assert_eq!(effort, ReviewEffort::Normal);
        assert_eq!(remaining, "src/main.rs");
    }

    #[test]
    fn test_parse_review_effort_default_empty() {
        let (effort, remaining) = parse_review_effort("");
        assert_eq!(effort, ReviewEffort::Normal);
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_parse_review_effort_quick() {
        let (effort, remaining) = parse_review_effort("--quick src/main.rs");
        assert_eq!(effort, ReviewEffort::Quick);
        assert_eq!(remaining, "src/main.rs");
    }

    #[test]
    fn test_parse_review_effort_quick_no_arg() {
        let (effort, remaining) = parse_review_effort("--quick");
        assert_eq!(effort, ReviewEffort::Quick);
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_parse_review_effort_thorough() {
        let (effort, remaining) = parse_review_effort("--thorough");
        assert_eq!(effort, ReviewEffort::Thorough);
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_parse_review_effort_thorough_with_file() {
        let (effort, remaining) = parse_review_effort("--thorough src/lib.rs");
        assert_eq!(effort, ReviewEffort::Thorough);
        assert_eq!(remaining, "src/lib.rs");
    }

    #[test]
    fn test_parse_review_effort_flag_after_file() {
        // Flag can appear after the file path too
        let (effort, remaining) = parse_review_effort("src/main.rs --quick");
        assert_eq!(effort, ReviewEffort::Quick);
        assert_eq!(remaining, "src/main.rs");
    }

    #[test]
    fn test_build_review_prompt_quick() {
        let prompt = build_review_prompt("test.rs", "let x = 1;", ReviewEffort::Quick);
        assert!(prompt.contains("Bugs"), "Quick review should mention bugs");
        assert!(
            prompt.contains("Security"),
            "Quick review should mention security"
        );
        assert!(
            !prompt.contains("Style"),
            "Quick review should NOT mention style"
        );
        assert!(
            !prompt.contains("Performance"),
            "Quick review should NOT mention performance"
        );
        assert!(
            prompt.contains("quick review"),
            "Should include effort label"
        );
        assert!(
            prompt.contains("terse"),
            "Quick review should ask for terse output"
        );
    }

    #[test]
    fn test_build_review_prompt_thorough() {
        let prompt = build_review_prompt("test.rs", "let x = 1;", ReviewEffort::Thorough);
        assert!(prompt.contains("Bugs"), "Should mention bugs");
        assert!(prompt.contains("Security"), "Should mention security");
        assert!(prompt.contains("Edge Cases"), "Should mention edge cases");
        assert!(
            prompt.contains("API Contracts"),
            "Should mention API contracts"
        );
        assert!(
            prompt.contains("Test Coverage"),
            "Should mention test coverage"
        );
        assert!(prompt.contains("Concurrency"), "Should mention concurrency");
        assert!(
            prompt.contains("Documentation"),
            "Should mention documentation"
        );
        assert!(
            prompt.contains("thorough review"),
            "Should include effort label"
        );
        assert!(
            prompt.contains("exhaustive"),
            "Thorough review should ask for exhaustive review"
        );
    }

    #[test]
    fn test_build_review_prompt_normal_no_effort_label() {
        let prompt = build_review_prompt("test.rs", "let x = 1;", ReviewEffort::Normal);
        // Normal should not have an effort label suffix
        assert!(
            !prompt.contains("[normal review]"),
            "Normal effort should not show effort label"
        );
    }

    #[test]
    fn test_review_effort_label() {
        assert_eq!(ReviewEffort::Quick.label(), "quick");
        assert_eq!(ReviewEffort::Normal.label(), "normal");
        assert_eq!(ReviewEffort::Thorough.label(), "thorough");
    }

    #[test]
    fn test_parse_review_effort_with_pr_flag() {
        let (effort, remaining) = parse_review_effort("--quick --pr 42");
        assert_eq!(effort, ReviewEffort::Quick);
        assert_eq!(remaining, "--pr 42");
    }
}
