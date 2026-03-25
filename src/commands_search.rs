//! Search & navigation command handlers: /find, /grep, /index, /ast.

use crate::format::*;

// ── /find ────────────────────────────────────────────────────────────────

/// Result of a fuzzy file match: (file_path, score, match_ranges).
/// Higher score = better match. match_ranges are byte offsets into the lowercased path.
#[derive(Debug, Clone, PartialEq)]
pub struct FindMatch {
    pub path: String,
    pub score: i32,
}

/// Score a file path against a fuzzy pattern (case-insensitive substring match).
/// Returns None if the pattern doesn't match.
/// Scoring:
///   - Base score for containing the pattern as a substring
///   - Bonus for matching the filename (last component) vs directory
///   - Bonus for exact filename match
///   - Bonus for match at the start of the filename
///   - Shorter paths score higher (less noise)
pub fn fuzzy_score(path: &str, pattern: &str) -> Option<i32> {
    let path_lower = path.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    if !path_lower.contains(&pattern_lower) {
        return None;
    }

    let mut score: i32 = 100; // base score for matching

    // Extract filename (last path component)
    let filename = path.rsplit('/').next().unwrap_or(path);
    let filename_lower = filename.to_lowercase();

    // Big bonus if the pattern matches within the filename itself
    if filename_lower.contains(&pattern_lower) {
        score += 50;

        // Bonus for matching at the start of filename
        if filename_lower.starts_with(&pattern_lower) {
            score += 30;
        }

        // Bonus for exact filename match (without extension)
        let stem = filename_lower.split('.').next().unwrap_or(&filename_lower);
        if stem == pattern_lower {
            score += 20;
        }
    }

    // Shorter paths are slightly preferred (less deeply nested = more relevant)
    let depth = path.matches('/').count();
    score -= depth as i32 * 2;

    Some(score)
}

/// Find files matching a fuzzy pattern. Uses `git ls-files` if in a git repo,
/// otherwise falls back to a recursive directory listing.
pub fn find_files(pattern: &str) -> Vec<FindMatch> {
    let files = list_project_files();
    let mut matches: Vec<FindMatch> = files
        .iter()
        .filter_map(|path| {
            fuzzy_score(path, pattern).map(|score| FindMatch {
                path: path.clone(),
                score,
            })
        })
        .collect();

    // Sort by score descending, then alphabetically for ties
    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.path.cmp(&b.path)));
    matches
}

/// List all project files. Prefers `git ls-files`, falls back to walkdir-style listing.
fn list_project_files() -> Vec<String> {
    if let Ok(text) = crate::git::run_git(&["ls-files"]) {
        return text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
    }

    // Fallback: recursive listing of current directory (respecting common ignores)
    walk_directory(".", 8)
}

/// Simple recursive directory walk (fallback when not in a git repo).
fn walk_directory(dir: &str, max_depth: usize) -> Vec<String> {
    let mut files = Vec::new();
    walk_directory_inner(dir, max_depth, 0, &mut files);
    files
}

fn walk_directory_inner(dir: &str, max_depth: usize, depth: usize, files: &mut Vec<String>) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden dirs and common ignore patterns
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        let path = if dir == "." {
            name.clone()
        } else {
            format!("{dir}/{name}")
        };
        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            walk_directory_inner(&path, max_depth, depth + 1, files);
        } else {
            files.push(path);
        }
    }
}

/// Highlight the matching pattern within a file path for display.
/// Returns the path with ANSI bold/color around the matched portion.
pub fn highlight_match(path: &str, pattern: &str) -> String {
    let path_lower = path.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    if let Some(pos) = path_lower.rfind(&pattern_lower) {
        // Prefer highlighting in the filename portion
        let end = pos + pattern.len();
        format!(
            "{}{BOLD}{GREEN}{}{RESET}{}",
            &path[..pos],
            &path[pos..end],
            &path[end..]
        )
    } else {
        path.to_string()
    }
}

pub fn handle_find(input: &str) {
    let arg = input.strip_prefix("/find").unwrap_or("").trim();
    if arg.is_empty() {
        println!("{DIM}  usage: /find <pattern>");
        println!("  Fuzzy-search project files by name.");
        println!("  Examples: /find main, /find .toml, /find test{RESET}\n");
        return;
    }

    let matches = find_files(arg);
    if matches.is_empty() {
        println!("{DIM}  No files matching '{arg}'.{RESET}\n");
    } else {
        let count = matches.len();
        let shown = matches.iter().take(20);
        println!(
            "{DIM}  {count} file{s} matching '{arg}':",
            s = if count == 1 { "" } else { "s" }
        );
        for m in shown {
            let highlighted = highlight_match(&m.path, arg);
            println!("    {highlighted}");
        }
        if count > 20 {
            println!("    {DIM}... and {} more{RESET}", count - 20);
        }
        println!("{RESET}");
    }
}

// ── /index ───────────────────────────────────────────────────────────────

/// An entry in the project index: path, line count, and first meaningful line.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexEntry {
    pub path: String,
    pub lines: usize,
    pub summary: String,
}

/// Extract the first meaningful line from file content.
/// Skips blank lines, then grabs the first doc comment (`//!`, `///`, `#`),
/// module declaration, or any non-empty line.
pub fn extract_first_meaningful_line(content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Return the first non-empty line, truncated
        return truncate_with_ellipsis(trimmed, 80);
    }
    String::new()
}

/// Build a project index by listing files and extracting metadata.
/// Uses `git ls-files` when available, falls back to directory walk.
/// Only indexes text-like source files (skips binaries, images, etc.).
pub fn build_project_index() -> Vec<IndexEntry> {
    let files = list_project_files();
    let mut entries = Vec::new();

    for path in &files {
        // Skip binary/non-text files based on extension
        if is_binary_extension(path) {
            continue;
        }

        // Read the file — skip if it fails (binary, permission, etc.)
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let line_count = content.lines().count();
        let summary = extract_first_meaningful_line(&content);

        entries.push(IndexEntry {
            path: path.clone(),
            lines: line_count,
            summary,
        });
    }

    entries
}

/// Check if a file extension suggests a binary/non-text file.
pub fn is_binary_extension(path: &str) -> bool {
    let binary_exts = [
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".ico", ".svg", ".woff", ".woff2",
        ".ttf", ".otf", ".eot", ".pdf", ".zip", ".gz", ".tar", ".bz2", ".xz", ".7z", ".rar",
        ".exe", ".dll", ".so", ".dylib", ".o", ".a", ".class", ".pyc", ".pyo", ".wasm", ".lock",
    ];
    let lower = path.to_lowercase();
    binary_exts.iter().any(|ext| lower.ends_with(ext))
}

/// Format the project index as a table string.
pub fn format_project_index(entries: &[IndexEntry]) -> String {
    if entries.is_empty() {
        return "(no indexable files found)".to_string();
    }

    let mut output = String::new();

    // Find max path length for alignment (capped at 50)
    let max_path_len = entries
        .iter()
        .map(|e| e.path.len())
        .max()
        .unwrap_or(0)
        .min(50);

    output.push_str(&format!(
        "  {:<width$}  {:>5}  {}\n",
        "Path",
        "Lines",
        "Summary",
        width = max_path_len
    ));
    output.push_str(&format!(
        "  {:<width$}  {:>5}  {}\n",
        "─".repeat(max_path_len.min(50)),
        "─────",
        "─".repeat(40),
        width = max_path_len
    ));

    for entry in entries {
        let path_display = if entry.path.len() > 50 {
            format!("…{}", &entry.path[entry.path.len() - 49..])
        } else {
            entry.path.clone()
        };
        output.push_str(&format!(
            "  {:<width$}  {:>5}  {}\n",
            path_display,
            entry.lines,
            entry.summary,
            width = max_path_len
        ));
    }

    // Summary line
    let total_files = entries.len();
    let total_lines: usize = entries.iter().map(|e| e.lines).sum();
    output.push_str(&format!(
        "\n  {} file{}, {} total lines\n",
        total_files,
        if total_files == 1 { "" } else { "s" },
        total_lines
    ));

    output
}

/// Handle the /index command: build and display a project file index.
pub fn handle_index() {
    println!("{DIM}  Building project index...{RESET}");
    let entries = build_project_index();
    if entries.is_empty() {
        println!("{DIM}  (no indexable source files found){RESET}\n");
    } else {
        let formatted = format_project_index(&entries);
        println!("{DIM}{formatted}{RESET}");
    }
}

// ── /grep ────────────────────────────────────────────────────────────────

/// Maximum matches to display before truncating.
const GREP_MAX_MATCHES: usize = 50;

/// Parsed arguments for the `/grep` command.
#[derive(Debug, Clone, PartialEq)]
pub struct GrepArgs {
    pub pattern: String,
    pub path: String,
    pub case_sensitive: bool,
}

/// Parse `/grep` arguments.
///
/// Syntax: `/grep [-s|--case] <pattern> [path]`
///
/// Returns `None` if the pattern is empty.
pub fn parse_grep_args(input: &str) -> Option<GrepArgs> {
    let rest = input.strip_prefix("/grep").unwrap_or(input).trim();

    if rest.is_empty() {
        return None;
    }

    let mut case_sensitive = false;
    let mut remaining_parts: Vec<&str> = Vec::new();

    for part in rest.split_whitespace() {
        if part == "-s" || part == "--case" {
            case_sensitive = true;
        } else {
            remaining_parts.push(part);
        }
    }

    if remaining_parts.is_empty() {
        return None;
    }

    let pattern = remaining_parts[0].to_string();
    let path = if remaining_parts.len() > 1 {
        remaining_parts[1..].join(" ")
    } else {
        ".".to_string()
    };

    Some(GrepArgs {
        pattern,
        path,
        case_sensitive,
    })
}

/// A single grep match result.
#[derive(Debug, Clone, PartialEq)]
pub struct GrepMatch {
    pub file: String,
    pub line_num: u32,
    pub text: String,
}

/// Run grep and return structured results.
///
/// Uses `git grep` when inside a git repo (faster, respects .gitignore),
/// falls back to `grep -rn` with common directory exclusions.
pub fn run_grep(args: &GrepArgs) -> Result<Vec<GrepMatch>, String> {
    let in_git_repo = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let output = if in_git_repo {
        let mut cmd = std::process::Command::new("git");
        cmd.args(["grep", "-n", "--color=never"]);
        if !args.case_sensitive {
            cmd.arg("-i");
        }
        cmd.arg("--");
        cmd.arg(&args.pattern);
        if args.path != "." {
            cmd.arg(&args.path);
        }
        cmd.output()
    } else {
        let mut cmd = std::process::Command::new("grep");
        cmd.args(["-rn", "--color=never"]);
        if !args.case_sensitive {
            cmd.arg("-i");
        }
        cmd.args([
            "--exclude-dir=.git",
            "--exclude-dir=target",
            "--exclude-dir=node_modules",
            "--exclude-dir=__pycache__",
            "--exclude-dir=.venv",
        ]);
        cmd.arg(&args.pattern);
        cmd.arg(&args.path);
        cmd.output()
    };

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let matches: Vec<GrepMatch> = stdout
                .lines()
                .filter(|l| !l.is_empty())
                .filter_map(|line| {
                    // Format: file:line_num:text
                    let first_colon = line.find(':')?;
                    let rest = &line[first_colon + 1..];
                    let second_colon = rest.find(':')?;
                    let file = line[..first_colon].to_string();
                    let line_num = rest[..second_colon].parse::<u32>().ok()?;
                    let text = rest[second_colon + 1..].to_string();
                    Some(GrepMatch {
                        file,
                        line_num,
                        text,
                    })
                })
                .collect();
            Ok(matches)
        }
        Err(e) => Err(format!("Failed to run grep: {e}")),
    }
}

/// Format grep results with colors and truncation.
///
/// Returns the formatted string to display.
/// Colors: filenames in green, line numbers in cyan, matches highlighted in bold yellow.
pub fn format_grep_results(matches: &[GrepMatch], pattern: &str, case_sensitive: bool) -> String {
    if matches.is_empty() {
        return format!("{DIM}  No matches found.{RESET}\n");
    }

    let total = matches.len();
    let shown = matches.iter().take(GREP_MAX_MATCHES);
    let mut output = String::new();

    for m in shown {
        // Highlight the matched pattern in the text
        let highlighted_text = highlight_grep_match(&m.text, pattern, case_sensitive);
        output.push_str(&format!(
            "  {GREEN}{}{RESET}:{CYAN}{}{RESET}: {}\n",
            m.file, m.line_num, highlighted_text
        ));
    }

    if total > GREP_MAX_MATCHES {
        output.push_str(&format!(
            "\n{DIM}  ({} more matches, narrow your search){RESET}\n",
            total - GREP_MAX_MATCHES
        ));
    } else {
        output.push_str(&format!(
            "\n{DIM}  {} match{}{RESET}\n",
            total,
            if total == 1 { "" } else { "es" }
        ));
    }

    output
}

/// Highlight occurrences of a pattern in a line of text.
fn highlight_grep_match(text: &str, pattern: &str, case_sensitive: bool) -> String {
    if pattern.is_empty() {
        return text.to_string();
    }

    let mut result = String::new();
    let (search_text, search_pattern) = if case_sensitive {
        (text.to_string(), pattern.to_string())
    } else {
        (text.to_lowercase(), pattern.to_lowercase())
    };

    let mut last_end = 0;
    let mut start = 0;
    while let Some(pos) = search_text[start..].find(&search_pattern) {
        let abs_pos = start + pos;
        // Append text before match
        result.push_str(&text[last_end..abs_pos]);
        // Append highlighted match (use original case from text)
        result.push_str(&format!(
            "{BOLD_YELLOW}{}{RESET}",
            &text[abs_pos..abs_pos + pattern.len()]
        ));
        last_end = abs_pos + pattern.len();
        start = last_end;
    }
    result.push_str(&text[last_end..]);

    result
}

/// Handle the `/grep` command.
pub fn handle_grep(input: &str) {
    let args = match parse_grep_args(input) {
        Some(a) => a,
        None => {
            println!("{DIM}  usage: /grep [-s|--case] <pattern> [path]");
            println!("  Search file contents directly — no AI, no tokens, instant results.");
            println!("  Case-insensitive by default. Use -s or --case for case-sensitive.");
            println!();
            println!("  Examples:");
            println!("    /grep TODO");
            println!("    /grep \"fn main\" src/");
            println!("    /grep -s MyStruct src/lib.rs{RESET}\n");
            return;
        }
    };

    match run_grep(&args) {
        Ok(matches) => {
            let formatted = format_grep_results(&matches, &args.pattern, args.case_sensitive);
            print!("{formatted}");
        }
        Err(e) => {
            println!("{RED}  Error: {e}{RESET}\n");
        }
    }
}

// ── /ast ─────────────────────────────────────────────────────────────────

/// Subcommand completions for `/ast <Tab>`.
pub const AST_GREP_FLAGS: &[&str] = &["--lang", "--in"];

/// Check if ast-grep's `sg` binary is available on PATH.
pub fn is_ast_grep_available() -> bool {
    std::process::Command::new("sg")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run ast-grep structural search.
/// Returns Ok(output) or Err(error message).
pub fn run_ast_grep_search(
    pattern: &str,
    lang: Option<&str>,
    path: Option<&str>,
) -> Result<String, String> {
    if !is_ast_grep_available() {
        return Err(
            "ast-grep (sg) is not installed. Install from: https://ast-grep.github.io/".into(),
        );
    }
    let mut cmd = std::process::Command::new("sg");
    cmd.arg("run").arg("--pattern").arg(pattern);
    if let Some(l) = lang {
        cmd.arg("--lang").arg(l);
    }
    if let Some(p) = path {
        cmd.arg(p);
    }
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            if stdout.trim().is_empty() {
                Ok("No matches found.".into())
            } else {
                Ok(stdout)
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if stderr.trim().is_empty() {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                if stdout.trim().is_empty() {
                    Ok("No matches found.".into())
                } else {
                    Ok(stdout)
                }
            } else {
                Err(format!("ast-grep error: {}", stderr.trim()))
            }
        }
        Err(e) => Err(format!("Failed to run sg: {e}")),
    }
}

/// Parse `/ast` command arguments into (pattern, lang, path).
pub fn parse_ast_grep_args(
    input: &str,
) -> Result<(String, Option<String>, Option<String>), String> {
    let rest = input.strip_prefix("/ast").unwrap_or("").trim();

    if rest.is_empty() {
        return Err("Usage: /ast <pattern> [--lang <lang>] [--in <path>]".into());
    }

    let parts: Vec<&str> = rest.split_whitespace().collect();
    let mut pattern_parts: Vec<&str> = Vec::new();
    let mut lang: Option<String> = None;
    let mut path: Option<String> = None;

    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--lang" => {
                if i + 1 < parts.len() {
                    lang = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    return Err("--lang requires a value (e.g. --lang rust)".into());
                }
            }
            "--in" => {
                if i + 1 < parts.len() {
                    path = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    return Err("--in requires a value (e.g. --in src/)".into());
                }
            }
            other => {
                pattern_parts.push(other);
                i += 1;
            }
        }
    }

    if pattern_parts.is_empty() {
        return Err("Usage: /ast <pattern> [--lang <lang>] [--in <path>]".into());
    }

    Ok((pattern_parts.join(" "), lang, path))
}

/// Handle the `/ast` REPL command.
pub fn handle_ast_grep(input: &str) {
    match parse_ast_grep_args(input) {
        Err(msg) => {
            println!("{YELLOW}  {msg}{RESET}\n");
        }
        Ok((pattern, lang, path)) => {
            if !is_ast_grep_available() {
                println!("{YELLOW}  ast-grep (sg) is not installed.{RESET}");
                println!("{DIM}  Install from: https://ast-grep.github.io/{RESET}");
                println!("{DIM}  Example: npm i -g @ast-grep/cli{RESET}\n");
                return;
            }
            println!("{DIM}  Searching for pattern: {pattern}{RESET}");
            match run_ast_grep_search(&pattern, lang.as_deref(), path.as_deref()) {
                Ok(output) => {
                    println!("{output}");
                }
                Err(e) => {
                    println!("{YELLOW}  {e}{RESET}\n");
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

    // ── fuzzy_score ─────────────────────────────────────────────────

    #[test]
    fn fuzzy_score_no_match() {
        assert!(fuzzy_score("src/main.rs", "xyz").is_none());
    }

    #[test]
    fn fuzzy_score_exact_filename() {
        let score = fuzzy_score("src/main.rs", "main").unwrap();
        assert!(score > 100); // base + filename match + start match + stem match
    }

    #[test]
    fn fuzzy_score_case_insensitive() {
        assert!(fuzzy_score("src/Main.rs", "main").is_some());
        assert!(fuzzy_score("src/MAIN.rs", "main").is_some());
    }

    #[test]
    fn fuzzy_score_directory_match_lower_than_filename() {
        // "src" in path "src/other.rs" matches directory
        let dir_score = fuzzy_score("src/other.rs", "other").unwrap();
        // "main" in "deeply/nested/main.rs" matches filename but deeper
        let file_score = fuzzy_score("deeply/nested/main.rs", "main").unwrap();
        // Both should match, filename match has bonus
        assert!(dir_score > 100);
        assert!(file_score > 100);
    }

    #[test]
    fn fuzzy_score_shorter_path_preferred() {
        let shallow = fuzzy_score("main.rs", "main").unwrap();
        let deep = fuzzy_score("a/b/c/main.rs", "main").unwrap();
        assert!(shallow > deep);
    }

    #[test]
    fn fuzzy_score_extension_match() {
        let score = fuzzy_score("config/settings.toml", ".toml").unwrap();
        assert!(score > 0);
    }

    // ── highlight_match ─────────────────────────────────────────────

    #[test]
    fn highlight_match_contains_pattern() {
        let result = highlight_match("src/main.rs", "main");
        // Should contain ANSI codes around "main"
        assert!(result.contains("main"));
        assert!(result.contains("src/"));
        assert!(result.contains(".rs"));
    }

    #[test]
    fn highlight_match_no_match_returns_plain() {
        let result = highlight_match("src/main.rs", "xyz");
        assert_eq!(result, "src/main.rs");
    }

    #[test]
    fn highlight_match_case_insensitive() {
        let result = highlight_match("src/Main.rs", "main");
        // Should still highlight (rfind on lowercased)
        assert!(result.contains("Main"));
    }

    // ── extract_first_meaningful_line ────────────────────────────────

    #[test]
    fn extract_first_meaningful_line_basic() {
        let result = extract_first_meaningful_line("//! Module docs\nuse std;");
        assert_eq!(result, "//! Module docs");
    }

    #[test]
    fn extract_first_meaningful_line_skips_blanks() {
        let result = extract_first_meaningful_line("\n\n  \n  // comment");
        assert_eq!(result, "// comment");
    }

    #[test]
    fn extract_first_meaningful_line_empty() {
        let result = extract_first_meaningful_line("");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_first_meaningful_line_all_blank() {
        let result = extract_first_meaningful_line("  \n  \n  ");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_first_meaningful_line_truncates_long() {
        let long_line = "x".repeat(200);
        let result = extract_first_meaningful_line(&long_line);
        assert!(result.len() <= 83); // 80 + "..." = 83
    }

    // ── is_binary_extension ─────────────────────────────────────────

    #[test]
    fn is_binary_extension_images() {
        assert!(is_binary_extension("photo.png"));
        assert!(is_binary_extension("icon.jpg"));
        assert!(is_binary_extension("banner.gif"));
        assert!(is_binary_extension("logo.webp"));
    }

    #[test]
    fn is_binary_extension_archives() {
        assert!(is_binary_extension("data.zip"));
        assert!(is_binary_extension("backup.tar"));
        assert!(is_binary_extension("compressed.gz"));
    }

    #[test]
    fn is_binary_extension_source_files() {
        assert!(!is_binary_extension("main.rs"));
        assert!(!is_binary_extension("index.js"));
        assert!(!is_binary_extension("app.py"));
        assert!(!is_binary_extension("README.md"));
        assert!(!is_binary_extension("Cargo.toml"));
    }

    #[test]
    fn is_binary_extension_case_insensitive() {
        assert!(is_binary_extension("PHOTO.PNG"));
        assert!(is_binary_extension("Image.JPG"));
    }

    #[test]
    fn is_binary_extension_lock_files() {
        assert!(is_binary_extension("Cargo.lock"));
        assert!(is_binary_extension("package-lock.lock"));
    }

    #[test]
    fn is_binary_extension_compiled() {
        assert!(is_binary_extension("module.wasm"));
        assert!(is_binary_extension("main.pyc"));
        assert!(is_binary_extension("lib.so"));
        assert!(is_binary_extension("app.exe"));
    }

    // ── IndexEntry & format_project_index ────────────────────────────

    #[test]
    fn format_project_index_empty() {
        let result = format_project_index(&[]);
        assert_eq!(result, "(no indexable files found)");
    }

    #[test]
    fn format_project_index_single_file() {
        let entries = vec![IndexEntry {
            path: "src/main.rs".to_string(),
            lines: 42,
            summary: "//! Main module".to_string(),
        }];
        let output = format_project_index(&entries);
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("42"));
        assert!(output.contains("//! Main module"));
        assert!(output.contains("1 file"));
        assert!(output.contains("42 total lines"));
    }

    #[test]
    fn format_project_index_multiple_files() {
        let entries = vec![
            IndexEntry {
                path: "src/main.rs".to_string(),
                lines: 100,
                summary: "//! Entry point".to_string(),
            },
            IndexEntry {
                path: "src/lib.rs".to_string(),
                lines: 50,
                summary: "//! Library".to_string(),
            },
        ];
        let output = format_project_index(&entries);
        assert!(output.contains("2 files"));
        assert!(output.contains("150 total lines"));
    }

    #[test]
    fn format_project_index_long_path_truncated() {
        let long_path = format!("a/{}", "b/".repeat(25).trim_end_matches('/'));
        let entries = vec![IndexEntry {
            path: long_path,
            lines: 10,
            summary: "long path file".to_string(),
        }];
        let output = format_project_index(&entries);
        // Should contain the truncation marker
        assert!(output.contains('…'));
    }

    // ── FindMatch ────────────────────────────────────────────────────

    #[test]
    fn find_match_equality() {
        let a = FindMatch {
            path: "src/main.rs".to_string(),
            score: 150,
        };
        let b = FindMatch {
            path: "src/main.rs".to_string(),
            score: 150,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn find_match_debug() {
        let m = FindMatch {
            path: "test.rs".to_string(),
            score: 100,
        };
        let debug = format!("{:?}", m);
        assert!(debug.contains("test.rs"));
        assert!(debug.contains("100"));
    }

    // ── walk_directory ──────────────────────────────────────────────

    #[test]
    fn walk_directory_finds_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("hello.txt"), "hi").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/nested.txt"), "there").unwrap();

        let files = walk_directory(dir.path().to_str().unwrap(), 3);
        assert!(files.iter().any(|f| f.ends_with("hello.txt")));
        assert!(files.iter().any(|f| f.ends_with("nested.txt")));
    }

    #[test]
    fn walk_directory_skips_hidden() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".hidden")).unwrap();
        fs::write(dir.path().join(".hidden/secret.txt"), "").unwrap();
        fs::write(dir.path().join("visible.txt"), "").unwrap();

        let files = walk_directory(dir.path().to_str().unwrap(), 3);
        assert!(files.iter().any(|f| f.ends_with("visible.txt")));
        assert!(!files.iter().any(|f| f.contains("secret")));
    }

    #[test]
    fn walk_directory_skips_node_modules() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/dep.js"), "").unwrap();
        fs::write(dir.path().join("app.js"), "").unwrap();

        let files = walk_directory(dir.path().to_str().unwrap(), 3);
        assert!(files.iter().any(|f| f.ends_with("app.js")));
        assert!(!files.iter().any(|f| f.contains("dep.js")));
    }

    #[test]
    fn walk_directory_respects_max_depth() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        fs::write(dir.path().join("a/b/c/deep.txt"), "").unwrap();
        fs::write(dir.path().join("a/shallow.txt"), "").unwrap();

        let files = walk_directory(dir.path().to_str().unwrap(), 1);
        assert!(files.iter().any(|f| f.ends_with("shallow.txt")));
        // At max_depth=1, we go dir->a (depth 1)->files, but a/b is depth 2
        assert!(!files.iter().any(|f| f.ends_with("deep.txt")));
    }

    // ── /grep tests ─────────────────────────────────────────────────────

    #[test]
    fn parse_grep_args_basic_pattern() {
        let args = parse_grep_args("/grep TODO").unwrap();
        assert_eq!(args.pattern, "TODO");
        assert_eq!(args.path, ".");
        assert!(!args.case_sensitive);
    }

    #[test]
    fn parse_grep_args_with_path() {
        let args = parse_grep_args("/grep fn_main src/").unwrap();
        assert_eq!(args.pattern, "fn_main");
        assert_eq!(args.path, "src/");
        assert!(!args.case_sensitive);
    }

    #[test]
    fn parse_grep_args_case_sensitive_flag() {
        let args = parse_grep_args("/grep -s MyStruct src/").unwrap();
        assert_eq!(args.pattern, "MyStruct");
        assert_eq!(args.path, "src/");
        assert!(args.case_sensitive);
    }

    #[test]
    fn parse_grep_args_case_long_flag() {
        let args = parse_grep_args("/grep --case Pattern").unwrap();
        assert_eq!(args.pattern, "Pattern");
        assert!(args.case_sensitive);
    }

    #[test]
    fn parse_grep_args_empty_returns_none() {
        assert!(parse_grep_args("/grep").is_none());
        assert!(parse_grep_args("/grep  ").is_none());
    }

    #[test]
    fn parse_grep_args_only_flag_returns_none() {
        assert!(parse_grep_args("/grep -s").is_none());
        assert!(parse_grep_args("/grep --case").is_none());
    }

    #[test]
    fn format_grep_results_empty() {
        let formatted = format_grep_results(&[], "pattern", false);
        assert!(formatted.contains("No matches found"));
    }

    #[test]
    fn format_grep_results_with_matches() {
        let matches = vec![
            GrepMatch {
                file: "src/main.rs".to_string(),
                line_num: 10,
                text: "fn main() {".to_string(),
            },
            GrepMatch {
                file: "src/lib.rs".to_string(),
                line_num: 5,
                text: "// main entry".to_string(),
            },
        ];
        let formatted = format_grep_results(&matches, "main", false);
        assert!(formatted.contains("src/main.rs"));
        assert!(formatted.contains("10"));
        assert!(formatted.contains("src/lib.rs"));
        assert!(formatted.contains("5"));
        assert!(formatted.contains("2 matches"));
    }

    #[test]
    fn format_grep_results_truncation() {
        let matches: Vec<GrepMatch> = (0..60)
            .map(|i| GrepMatch {
                file: format!("file{i}.rs"),
                line_num: i,
                text: format!("line {i}"),
            })
            .collect();
        let formatted = format_grep_results(&matches, "line", false);
        assert!(formatted.contains("10 more matches, narrow your search"));
        // Should show first 50, not last 10
        assert!(formatted.contains("file0.rs"));
        assert!(formatted.contains("file49.rs"));
    }

    #[test]
    fn format_grep_results_single_match() {
        let matches = vec![GrepMatch {
            file: "test.rs".to_string(),
            line_num: 1,
            text: "hello".to_string(),
        }];
        let formatted = format_grep_results(&matches, "hello", false);
        assert!(formatted.contains("1 match"));
        // Shouldn't say "1 matches"
        assert!(!formatted.contains("1 matches"));
    }

    #[test]
    fn handle_grep_finds_real_matches() {
        // This tests run_grep on the actual project — "fn main" should exist in src/
        let args = GrepArgs {
            pattern: "fn main".to_string(),
            path: "src/".to_string(),
            case_sensitive: true,
        };
        let matches = run_grep(&args).unwrap();
        assert!(
            !matches.is_empty(),
            "Should find 'fn main' in src/ of this project"
        );
        assert!(matches.iter().any(|m| m.file.contains("main.rs")));
    }

    #[test]
    fn grep_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/grep"),
            "/grep should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn grep_in_help_text() {
        let help = help_text();
        assert!(help.contains("/grep"), "/grep should appear in help text");
    }

    // ── /ast tests ──────────────────────────────────────────────────────

    #[test]
    fn test_is_ast_grep_available_no_panic() {
        // Should not panic regardless of whether sg is installed
        let _ = is_ast_grep_available();
    }

    #[test]
    fn test_ast_grep_search_no_sg() {
        // When sg is not installed, should return a helpful error
        if !is_ast_grep_available() {
            let result = run_ast_grep_search("$X.unwrap()", None, None);
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("not installed"));
        }
    }

    #[test]
    fn test_ast_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/ast"),
            "/ast should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_ast_in_help_text() {
        let help = help_text();
        assert!(help.contains("/ast"), "/ast should appear in help text");
    }

    #[test]
    fn test_parse_ast_grep_args_simple_pattern() {
        let result = parse_ast_grep_args("/ast $X.unwrap()");
        assert!(result.is_ok());
        let (pattern, lang, path) = result.unwrap();
        assert_eq!(pattern, "$X.unwrap()");
        assert!(lang.is_none());
        assert!(path.is_none());
    }

    #[test]
    fn test_parse_ast_grep_args_with_lang() {
        let result = parse_ast_grep_args("/ast $X.unwrap() --lang rust");
        assert!(result.is_ok());
        let (pattern, lang, path) = result.unwrap();
        assert_eq!(pattern, "$X.unwrap()");
        assert_eq!(lang.as_deref(), Some("rust"));
        assert!(path.is_none());
    }

    #[test]
    fn test_parse_ast_grep_args_with_lang_and_path() {
        let result = parse_ast_grep_args("/ast $X.unwrap() --lang rust --in src/");
        assert!(result.is_ok());
        let (pattern, lang, path) = result.unwrap();
        assert_eq!(pattern, "$X.unwrap()");
        assert_eq!(lang.as_deref(), Some("rust"));
        assert_eq!(path.as_deref(), Some("src/"));
    }

    #[test]
    fn test_parse_ast_grep_args_flags_before_pattern() {
        let result = parse_ast_grep_args("/ast --lang rust $X.unwrap()");
        assert!(result.is_ok());
        let (pattern, lang, _) = result.unwrap();
        assert_eq!(pattern, "$X.unwrap()");
        assert_eq!(lang.as_deref(), Some("rust"));
    }

    #[test]
    fn test_parse_ast_grep_args_empty() {
        let result = parse_ast_grep_args("/ast");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Usage"));
    }

    #[test]
    fn test_parse_ast_grep_args_missing_lang_value() {
        let result = parse_ast_grep_args("/ast $X --lang");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("--lang requires"));
    }

    #[test]
    fn test_parse_ast_grep_args_missing_in_value() {
        let result = parse_ast_grep_args("/ast $X --in");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("--in requires"));
    }

    #[test]
    fn test_ast_tab_completion() {
        use crate::commands::command_arg_completions;
        let candidates = command_arg_completions("/ast", "");
        assert!(
            candidates.contains(&"--lang".to_string()),
            "Should include '--lang'"
        );
        assert!(
            candidates.contains(&"--in".to_string()),
            "Should include '--in'"
        );
    }

    #[test]
    fn test_ast_tab_completion_filters() {
        use crate::commands::command_arg_completions;
        let candidates = command_arg_completions("/ast", "--l");
        assert!(
            candidates.contains(&"--lang".to_string()),
            "Should include '--lang' for prefix '--l'"
        );
        assert!(
            !candidates.contains(&"--in".to_string()),
            "Should not include '--in' for prefix '--l'"
        );
    }

    #[test]
    fn test_handle_ast_grep_no_panic_empty() {
        // Should not panic on empty input
        handle_ast_grep("/ast");
    }

    #[test]
    fn test_handle_ast_grep_no_panic_with_pattern() {
        // Should not panic even if sg is not installed
        handle_ast_grep("/ast $X.unwrap()");
    }
}
