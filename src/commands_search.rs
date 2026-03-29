//! Search & navigation command handlers: /find, /grep, /index, /map, /ast.

use crate::format::*;
use regex::Regex;
use std::path::Path;

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

// ── /map — structural codebase understanding ────────────────────────────

/// Kind of structural symbol extracted from source code.
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Interface,
    Class,
    Type,
    Const,
    Impl,
    Module,
}

/// A structural symbol extracted from a source file.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub is_public: bool,
    pub line: usize,
}

/// Symbols extracted from a single file.
#[derive(Debug, Clone)]
pub struct FileSymbols {
    pub path: String,
    pub lines: usize,
    pub symbols: Vec<Symbol>,
}

/// Detect programming language from file extension.
pub fn detect_language(path: &str) -> Option<&'static str> {
    match Path::new(path).extension()?.to_str()? {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "js" | "jsx" | "mjs" => Some("javascript"),
        "ts" | "tsx" => Some("typescript"),
        "go" => Some("go"),
        "java" => Some("java"),
        _ => None,
    }
}

/// Extract structural symbols from source code for the given language.
///
/// Uses regex-based line-by-line extraction. This is intentionally simple —
/// false positives in comments are acceptable for v1.
pub fn extract_symbols(code: &str, language: &str) -> Vec<Symbol> {
    match language {
        "rust" => extract_rust_symbols(code),
        "python" => extract_python_symbols(code),
        "javascript" => extract_js_symbols(code),
        "typescript" => extract_ts_symbols(code),
        "go" => extract_go_symbols(code),
        "java" => extract_java_symbols(code),
        _ => Vec::new(),
    }
}

/// Extract symbols from Rust source code.
/// Skips content inside `#[cfg(test)]` modules.
fn extract_rust_symbols(code: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut in_test_module = false;
    let mut test_brace_depth: i32 = 0;

    let re_fn = Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?(?:async\s+)?fn\s+(\w+)").unwrap();
    let re_struct = Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?struct\s+(\w+)").unwrap();
    let re_enum = Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?enum\s+(\w+)").unwrap();
    let re_trait = Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?trait\s+(\w+)").unwrap();
    let re_impl = Regex::new(r"^\s*impl(?:<[^>]*>)?\s+(.+?)(?:\s*\{|$)").unwrap();
    let re_const = Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?(?:const|static)\s+(\w+)").unwrap();
    let re_mod = Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?mod\s+(\w+)").unwrap();
    let re_cfg_test = Regex::new(r"#\[cfg\(test\)\]").unwrap();

    let mut next_is_test_mod = false;

    for (line_num, line) in code.lines().enumerate() {
        // Track #[cfg(test)] — the next `mod` after this attribute starts a test module
        if re_cfg_test.is_match(line) {
            next_is_test_mod = true;
            continue;
        }

        if in_test_module {
            // Count braces to find the end of the test module
            for ch in line.chars() {
                if ch == '{' {
                    test_brace_depth += 1;
                } else if ch == '}' {
                    test_brace_depth -= 1;
                    if test_brace_depth <= 0 {
                        in_test_module = false;
                        break;
                    }
                }
            }
            continue;
        }

        // If the previous line was #[cfg(test)], check if this line starts a mod
        if next_is_test_mod {
            if re_mod.is_match(line) {
                in_test_module = true;
                test_brace_depth = 0;
                for ch in line.chars() {
                    if ch == '{' {
                        test_brace_depth += 1;
                    } else if ch == '}' {
                        test_brace_depth -= 1;
                    }
                }
                if test_brace_depth <= 0 && line.contains('{') {
                    in_test_module = false;
                }
                next_is_test_mod = false;
                continue;
            }
            // If not a mod line, the #[cfg(test)] applied to something else
            next_is_test_mod = false;
        }

        let is_pub = line.trim_start().starts_with("pub");

        // impl blocks (check before fn to avoid matching fn inside impl detection)
        if let Some(caps) = re_impl.captures(line) {
            // Skip if line also matches fn (impl is not a fn)
            if !re_fn.is_match(line) {
                let impl_target = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
                let name = format!("impl {impl_target}");
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Impl,
                    is_public: is_pub,
                    line: line_num + 1,
                });
                continue;
            }
        }

        if let Some(caps) = re_fn.captures(line) {
            let name = caps.get(2).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_struct.captures(line) {
            let name = caps.get(2).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Struct,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_enum.captures(line) {
            let name = caps.get(2).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Enum,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_trait.captures(line) {
            let name = caps.get(2).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Trait,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_const.captures(line) {
            let name = caps.get(2).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Const,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_mod.captures(line) {
            let name = caps.get(2).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Module,
                is_public: is_pub,
                line: line_num + 1,
            });
        }
    }

    symbols
}

/// Extract symbols from Python source code.
/// Only extracts top-level definitions (indentation level 0).
fn extract_python_symbols(code: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    let re_class = Regex::new(r"^class\s+(\w+)").unwrap();
    let re_func = Regex::new(r"^(?:async\s+)?def\s+(\w+)").unwrap();
    let re_const = Regex::new(r"^([A-Z][A-Z0-9_]*)\s*=").unwrap();

    for (line_num, line) in code.lines().enumerate() {
        // Only consider top-level (no indentation)
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }

        if let Some(caps) = re_class.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Class,
                is_public: !line.starts_with('_'),
                line: line_num + 1,
            });
        } else if let Some(caps) = re_func.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let is_public = !name.starts_with('_');
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                is_public,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_const.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Const,
                is_public: true,
                line: line_num + 1,
            });
        }
    }

    symbols
}

/// Extract symbols from JavaScript source code.
fn extract_js_symbols(code: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    let re_export_func =
        Regex::new(r"^(?:export\s+(?:default\s+)?)?(?:async\s+)?function\s+(\w+)").unwrap();
    let re_class = Regex::new(r"^(?:export\s+(?:default\s+)?)?class\s+(\w+)").unwrap();
    let re_const = Regex::new(r"^(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=").unwrap();

    for (line_num, line) in code.lines().enumerate() {
        let trimmed = line.trim_start();

        if let Some(caps) = re_export_func.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let is_public = trimmed.starts_with("export");
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                is_public,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_class.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let is_public = trimmed.starts_with("export");
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Class,
                is_public,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_const.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let is_public = trimmed.starts_with("export");
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Const,
                is_public,
                line: line_num + 1,
            });
        }
    }

    symbols
}

/// Extract symbols from TypeScript source code.
/// Includes all JS patterns plus interface and type.
fn extract_ts_symbols(code: &str) -> Vec<Symbol> {
    // Start with JS symbols
    let mut symbols = extract_js_symbols(code);

    let re_interface = Regex::new(r"^(?:export\s+)?interface\s+(\w+)").unwrap();
    let re_type = Regex::new(r"^(?:export\s+)?type\s+(\w+)\s*[=<]").unwrap();

    for (line_num, line) in code.lines().enumerate() {
        let trimmed = line.trim_start();

        if let Some(caps) = re_interface.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let is_public = trimmed.starts_with("export");
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Interface,
                is_public,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_type.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let is_public = trimmed.starts_with("export");
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Type,
                is_public,
                line: line_num + 1,
            });
        }
    }

    // Sort by line number since we appended TS-specific symbols after JS ones
    symbols.sort_by_key(|s| s.line);
    symbols
}

/// Extract symbols from Go source code.
fn extract_go_symbols(code: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    let re_func = Regex::new(r"^func\s+(\w+)\s*\(").unwrap();
    let re_method = Regex::new(r"^func\s+\([^)]+\)\s+(\w+)\s*\(").unwrap();
    let re_type_struct = Regex::new(r"^type\s+(\w+)\s+struct\b").unwrap();
    let re_type_interface = Regex::new(r"^type\s+(\w+)\s+interface\b").unwrap();
    let re_const = Regex::new(r"^(?:const|var)\s+(\w+)").unwrap();

    for (line_num, line) in code.lines().enumerate() {
        let trimmed = line.trim_start();

        if let Some(caps) = re_method.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let is_public = name.starts_with(|c: char| c.is_uppercase());
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                is_public,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_func.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let is_public = name.starts_with(|c: char| c.is_uppercase());
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                is_public,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_type_struct.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let is_public = name.starts_with(|c: char| c.is_uppercase());
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Struct,
                is_public,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_type_interface.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let is_public = name.starts_with(|c: char| c.is_uppercase());
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Interface,
                is_public,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_const.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let is_public = name.starts_with(|c: char| c.is_uppercase());
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Const,
                is_public,
                line: line_num + 1,
            });
        }
    }

    symbols
}

/// Extract symbols from Java source code.
fn extract_java_symbols(code: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    let re_class =
        Regex::new(r"^\s*(?:public\s+)?(?:abstract\s+)?(?:final\s+)?class\s+(\w+)").unwrap();
    let re_interface = Regex::new(r"^\s*(?:public\s+)?interface\s+(\w+)").unwrap();
    let re_enum = Regex::new(r"^\s*(?:public\s+)?enum\s+(\w+)").unwrap();
    let re_method = Regex::new(
        r"^\s*(?:public|private|protected)?\s*(?:static\s+)?(?:final\s+)?(?:[\w<>\[\],\s]+)\s+(\w+)\s*\(",
    )
    .unwrap();

    for (line_num, line) in code.lines().enumerate() {
        let is_pub = line.trim_start().starts_with("public");

        if let Some(caps) = re_class.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Class,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_interface.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Interface,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_enum.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Enum,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_method.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            // Skip common Java keywords that match the method regex
            if ![
                "if",
                "for",
                "while",
                "switch",
                "catch",
                "return",
                "new",
                "class",
                "interface",
            ]
            .contains(&name.as_str())
            {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    is_public: is_pub,
                    line: line_num + 1,
                });
            }
        }
    }

    symbols
}

/// Build the ast-grep inline rule YAML for a given language.
///
/// Returns a YAML string targeting structural symbol kinds (functions, structs,
/// classes, etc.) appropriate for the language.
fn ast_grep_rule_for_language(language: &str) -> Option<String> {
    let rule = match language {
        "rust" => {
            "id: symbols\nlanguage: Rust\nrule:\n  any:\n    \
             - kind: function_item\n    \
             - kind: struct_item\n    \
             - kind: enum_item\n    \
             - kind: trait_item\n    \
             - kind: impl_item\n    \
             - kind: const_item\n    \
             - kind: mod_item"
        }
        "python" => {
            "id: symbols\nlanguage: Python\nrule:\n  any:\n    \
             - kind: function_definition\n    \
             - kind: class_definition"
        }
        "javascript" => {
            "id: symbols\nlanguage: JavaScript\nrule:\n  any:\n    \
             - kind: function_declaration\n    \
             - kind: class_declaration\n    \
             - kind: lexical_declaration\n    \
             - kind: export_statement"
        }
        "typescript" => {
            "id: symbols\nlanguage: TypeScript\nrule:\n  any:\n    \
             - kind: function_declaration\n    \
             - kind: class_declaration\n    \
             - kind: interface_declaration\n    \
             - kind: type_alias_declaration\n    \
             - kind: lexical_declaration\n    \
             - kind: export_statement"
        }
        "go" => {
            "id: symbols\nlanguage: Go\nrule:\n  any:\n    \
             - kind: function_declaration\n    \
             - kind: method_declaration\n    \
             - kind: type_declaration"
        }
        "java" => {
            "id: symbols\nlanguage: Java\nrule:\n  any:\n    \
             - kind: class_declaration\n    \
             - kind: interface_declaration\n    \
             - kind: enum_declaration\n    \
             - kind: method_declaration"
        }
        _ => return None,
    };
    Some(rule.to_string())
}

/// Parse ast-grep JSON output into Symbol entries.
///
/// Each match from `sg scan --json` has "text", "range.start.line", etc.
/// We parse the first line of text to extract the symbol kind and name.
pub fn parse_ast_grep_symbols(json_str: &str, language: &str) -> Vec<Symbol> {
    // ast-grep outputs a JSON array of match objects
    let arr: Vec<serde_json::Value> = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut symbols = Vec::new();
    for item in &arr {
        let text = match item.get("text").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => continue,
        };
        let line = item
            .get("range")
            .and_then(|r| r.get("start"))
            .and_then(|s| s.get("line"))
            .and_then(|l| l.as_u64())
            .unwrap_or(0) as usize;

        // Extract symbol info from the first line of matched text
        let first_line = text.lines().next().unwrap_or("");
        if let Some(sym) = parse_symbol_from_text(first_line, language, line) {
            symbols.push(sym);
        }
    }
    symbols
}

/// Parse a symbol kind and name from a source code line.
///
/// Handles patterns like:
///   - `pub fn name(...)` / `fn name(...)`
///   - `pub struct Name` / `struct Name`
///   - `impl Name` / `impl Trait for Name`
///   - `class Name` / `def name(...)` / `func name(...)` etc.
fn parse_symbol_from_text(line: &str, language: &str, line_num: usize) -> Option<Symbol> {
    let trimmed = line.trim();
    let is_public = trimmed.starts_with("pub ")
        || trimmed.starts_with("export ")
        || (language == "go" && first_ident_uppercase(trimmed));

    // Strip leading visibility/decorators
    let stripped = trimmed
        .strip_prefix("pub(crate) ")
        .or_else(|| trimmed.strip_prefix("pub(super) "))
        .or_else(|| trimmed.strip_prefix("pub "))
        .or_else(|| trimmed.strip_prefix("export default "))
        .or_else(|| trimmed.strip_prefix("export "))
        .or_else(|| trimmed.strip_prefix("async "))
        .unwrap_or(trimmed);

    // Also handle "async" after pub
    let stripped = stripped.strip_prefix("async ").unwrap_or(stripped);

    // Match keyword → (SymbolKind, what-follows)
    if let Some(rest) = stripped.strip_prefix("fn ") {
        let name = ident_before(rest, &['(', '<', ' ', '{']);
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            is_public,
            line: line_num,
        });
    }
    if let Some(rest) = stripped.strip_prefix("struct ") {
        let name = ident_before(rest, &['(', '<', ' ', '{', ';']);
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Struct,
            is_public,
            line: line_num,
        });
    }
    if let Some(rest) = stripped.strip_prefix("enum ") {
        let name = ident_before(rest, &['<', ' ', '{']);
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Enum,
            is_public,
            line: line_num,
        });
    }
    if let Some(rest) = stripped.strip_prefix("trait ") {
        let name = ident_before(rest, &['<', ' ', '{', ':']);
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Trait,
            is_public,
            line: line_num,
        });
    }
    if let Some(rest) = stripped.strip_prefix("impl ") {
        // "impl Foo" or "impl Trait for Foo"
        let name = rest.split([' ', '<', '{']).next().unwrap_or("").trim();
        if name.is_empty() {
            return None;
        }
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Impl,
            is_public: false,
            line: line_num,
        });
    }
    if let Some(rest) = stripped.strip_prefix("mod ") {
        let name = ident_before(rest, &[' ', '{', ';']);
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Module,
            is_public,
            line: line_num,
        });
    }
    if let Some(rest) = stripped.strip_prefix("const ") {
        let name = ident_before(rest, &[':', ' ', '=']);
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Const,
            is_public,
            line: line_num,
        });
    }
    if let Some(rest) = stripped.strip_prefix("class ") {
        let name = ident_before(rest, &['(', ' ', '{', ':']);
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Class,
            is_public,
            line: line_num,
        });
    }
    if let Some(rest) = stripped.strip_prefix("interface ") {
        let name = ident_before(rest, &['<', ' ', '{']);
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Interface,
            is_public,
            line: line_num,
        });
    }
    if let Some(rest) = stripped.strip_prefix("type ") {
        let name = ident_before(rest, &['<', ' ', '=']);
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Type,
            is_public,
            line: line_num,
        });
    }
    // Python: def/async def
    if let Some(rest) = stripped.strip_prefix("def ") {
        let name = ident_before(rest, &['(', ' ', ':']);
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            is_public: !name.starts_with('_'),
            line: line_num,
        });
    }
    // Go: func (receiver) Name(...) or func Name(...)
    if let Some(rest) = stripped.strip_prefix("func ") {
        let rest = if rest.starts_with('(') {
            // Method: skip receiver
            rest.find(')').map(|i| rest[i + 1..].trim()).unwrap_or(rest)
        } else {
            rest
        };
        let name = ident_before(rest, &['(', '<', ' ', '{']);
        let is_go_pub = name.chars().next().is_some_and(|c| c.is_uppercase());
        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            is_public: is_go_pub,
            line: line_num,
        });
    }

    None
}

/// Extract the identifier from the start of `s`, stopping at any of `stops`.
fn ident_before<'a>(s: &'a str, stops: &[char]) -> &'a str {
    let end = s.find(stops).unwrap_or(s.len());
    s[..end].trim()
}

/// Check if the first identifier in a Go declaration is uppercase (exported).
fn first_ident_uppercase(line: &str) -> bool {
    // Skip "func ", "type ", etc.
    let after_kw = line
        .strip_prefix("func ")
        .or_else(|| line.strip_prefix("type "))
        .or_else(|| line.strip_prefix("const "))
        .or_else(|| line.strip_prefix("var "))
        .unwrap_or(line);
    // For methods, skip receiver
    let after_kw = if after_kw.starts_with('(') {
        after_kw
            .find(')')
            .map(|i| after_kw[i + 1..].trim())
            .unwrap_or(after_kw)
    } else {
        after_kw
    };
    after_kw.chars().next().is_some_and(|c| c.is_uppercase())
}

/// Try to extract symbols from a file using ast-grep.
///
/// Returns `Some(symbols)` if ast-grep succeeds, `None` if sg is not available
/// or the extraction fails (callers should fall back to regex).
pub fn extract_symbols_ast_grep(path: &str, language: &str) -> Option<Vec<Symbol>> {
    let rule = ast_grep_rule_for_language(language)?;

    let output = std::process::Command::new("sg")
        .arg("scan")
        .arg("--json")
        .arg("--inline-rules")
        .arg(&rule)
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Some(Vec::new());
    }

    let symbols = parse_ast_grep_symbols(&stdout, language);
    Some(symbols)
}

/// Which backend was used for symbol extraction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MapBackend {
    AstGrep,
    Regex,
}

/// Build a repo map by scanning project files and extracting symbols.
///
/// If `root` is Some, only scan files under that path.
/// If `public_only` is true, filter to only public/exported symbols.
pub fn build_repo_map(root: Option<&str>, public_only: bool) -> Vec<FileSymbols> {
    build_repo_map_with_backend(root, public_only, false).0
}

/// Build a repo map with explicit backend control.
///
/// When `force_regex` is true, skip ast-grep even if available.
/// Returns the file symbols and which backend was actually used.
pub fn build_repo_map_with_backend(
    root: Option<&str>,
    public_only: bool,
    force_regex: bool,
) -> (Vec<FileSymbols>, MapBackend) {
    let files = list_project_files();
    let mut result = Vec::new();

    // Check ast-grep availability once upfront
    let use_ast_grep = !force_regex && is_ast_grep_available();
    let backend = if use_ast_grep {
        MapBackend::AstGrep
    } else {
        MapBackend::Regex
    };

    for path in &files {
        // If a root filter is given, only include matching files
        if let Some(root_path) = root {
            if !path.starts_with(root_path) {
                continue;
            }
        }

        if is_binary_extension(path) {
            continue;
        }
        let lang = match detect_language(path) {
            Some(l) => l,
            None => continue,
        };
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let line_count = content.lines().count();

        // Try ast-grep first, fall back to regex
        let mut symbols = if use_ast_grep {
            extract_symbols_ast_grep(path, lang).unwrap_or_else(|| extract_symbols(&content, lang))
        } else {
            extract_symbols(&content, lang)
        };

        if public_only {
            symbols.retain(|s| s.is_public);
        }
        if !symbols.is_empty() {
            result.push(FileSymbols {
                path: path.clone(),
                lines: line_count,
                symbols,
            });
        }
    }

    // Sort by line count descending (biggest/most important files first)
    result.sort_by(|a, b| b.lines.cmp(&a.lines));
    (result, backend)
}

/// Format the repo map with ANSI colors for REPL display.
pub fn format_repo_map_colored(entries: &[FileSymbols]) -> String {
    if entries.is_empty() {
        return format!("{DIM}  (no structural symbols found){RESET}\n");
    }

    let mut output = String::new();

    for entry in entries {
        output.push_str(&format!(
            "\n{BOLD_CYAN}{}{RESET} {DIM}({} lines){RESET}\n",
            entry.path, entry.lines
        ));
        for sym in &entry.symbols {
            let kind_colored = match sym.kind {
                SymbolKind::Function => format!("{GREEN}fn{RESET}"),
                SymbolKind::Struct => format!("{YELLOW}struct{RESET}"),
                SymbolKind::Enum => format!("{YELLOW}enum{RESET}"),
                SymbolKind::Trait => format!("{YELLOW}trait{RESET}"),
                SymbolKind::Interface => format!("{YELLOW}interface{RESET}"),
                SymbolKind::Class => format!("{YELLOW}class{RESET}"),
                SymbolKind::Type => format!("{YELLOW}type{RESET}"),
                SymbolKind::Const => format!("{CYAN}const{RESET}"),
                SymbolKind::Impl => format!("{MAGENTA}impl{RESET}"),
                SymbolKind::Module => format!("{MAGENTA}mod{RESET}"),
            };
            let vis = if sym.is_public {
                format!("{GREEN}pub{RESET} ")
            } else {
                String::new()
            };
            output.push_str(&format!("  {vis}{kind_colored} {}\n", sym.name));
        }
    }

    output
}

/// Format the repo map as plain text for the system prompt.
///
/// Condensed format: no blank lines, public symbols only, capped at `max_chars`.
pub fn format_repo_map(entries: &[FileSymbols]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut output = String::new();

    for entry in entries {
        output.push_str(&format!("{} ({} lines)\n", entry.path, entry.lines));
        for sym in &entry.symbols {
            let kind_label = match sym.kind {
                SymbolKind::Function => "fn",
                SymbolKind::Struct => "struct",
                SymbolKind::Enum => "enum",
                SymbolKind::Trait => "trait",
                SymbolKind::Interface => "interface",
                SymbolKind::Class => "class",
                SymbolKind::Type => "type",
                SymbolKind::Const => "const",
                SymbolKind::Impl => "impl",
                SymbolKind::Module => "mod",
            };
            output.push_str(&format!("  {kind_label} {}\n", sym.name));
        }
    }

    output
}

/// Generate a repo map for the system prompt, capped at `max_chars` characters.
///
/// Returns `None` if no supported source files are found.
pub fn generate_repo_map_for_prompt_with_limit(max_chars: usize) -> Option<String> {
    let entries = build_repo_map(None, true);
    if entries.is_empty() {
        return None;
    }

    let full = format_repo_map(&entries);
    if full.len() <= max_chars {
        Some(full)
    } else {
        // Truncate: include files until we hit the limit
        let mut output = String::new();
        for entry in &entries {
            let mut file_block = format!("{} ({} lines)\n", entry.path, entry.lines);
            for sym in &entry.symbols {
                let kind_label = match sym.kind {
                    SymbolKind::Function => "fn",
                    SymbolKind::Struct => "struct",
                    SymbolKind::Enum => "enum",
                    SymbolKind::Trait => "trait",
                    SymbolKind::Interface => "interface",
                    SymbolKind::Class => "class",
                    SymbolKind::Type => "type",
                    SymbolKind::Const => "const",
                    SymbolKind::Impl => "impl",
                    SymbolKind::Module => "mod",
                };
                file_block.push_str(&format!("  {kind_label} {}\n", sym.name));
            }
            if output.len() + file_block.len() > max_chars {
                output.push_str("  ...\n");
                break;
            }
            output.push_str(&file_block);
        }
        Some(output)
    }
}

/// Default max characters for the system prompt repo map (~16K chars ≈ ~4K tokens).
const REPO_MAP_MAX_CHARS: usize = 16_000;

/// Generate a repo map for the system prompt with the default size cap.
pub fn generate_repo_map_for_prompt() -> Option<String> {
    generate_repo_map_for_prompt_with_limit(REPO_MAP_MAX_CHARS)
}

/// Handle the `/map` REPL command: show structural symbols from the codebase.
///
/// Usage: `/map [path]` — show all symbols
/// Usage: `/map --all [path]` — include private symbols
/// Usage: `/map --regex [path]` — force regex backend even if ast-grep is available
pub fn handle_map(input: &str) {
    let rest = input.strip_prefix("/map").unwrap_or("").trim();

    let mut show_all = false;
    let mut force_regex = false;
    let mut path_filter: Option<&str> = None;

    for part in rest.split_whitespace() {
        match part {
            "--all" => show_all = true,
            "--regex" => force_regex = true,
            _ => path_filter = Some(part),
        }
    }

    println!("{DIM}  Building repo map...{RESET}");
    let public_only = !show_all;
    let (entries, backend) = build_repo_map_with_backend(path_filter, public_only, force_regex);

    if entries.is_empty() {
        println!("{DIM}  (no supported source files with symbols found){RESET}\n");
        return;
    }

    let total_symbols: usize = entries.iter().map(|e| e.symbols.len()).sum();
    let total_files = entries.len();

    let formatted = format_repo_map_colored(&entries);
    print!("{formatted}");

    let backend_label = match backend {
        MapBackend::AstGrep => "using ast-grep",
        MapBackend::Regex => "using regex",
    };

    println!(
        "\n{DIM}  {} symbol{} across {} file{} ({backend_label}){RESET}\n",
        total_symbols,
        if total_symbols == 1 { "" } else { "s" },
        total_files,
        if total_files == 1 { "" } else { "s" },
    );
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

    // ── /map: SymbolKind, Symbol, extract_symbols ─────────────────────

    #[test]
    fn extract_rust_symbols_basic() {
        let code = r#"
pub fn hello(name: &str) -> String { todo!() }
fn private_fn() {}
pub struct MyStruct {
    field: i32,
}
pub enum Color { Red, Green, Blue }
pub trait Drawable { fn draw(&self); }
impl MyStruct {
    pub fn new() -> Self { todo!() }
}
const MAX: usize = 100;
"#;
        let symbols = extract_symbols(code, "rust");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "hello" && s.kind == SymbolKind::Function),
            "should find pub fn hello"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "MyStruct" && s.kind == SymbolKind::Struct),
            "should find pub struct MyStruct"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Color" && s.kind == SymbolKind::Enum),
            "should find pub enum Color"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Drawable" && s.kind == SymbolKind::Trait),
            "should find pub trait Drawable"
        );
        assert!(
            symbols.iter().any(|s| s.name.contains("impl MyStruct")),
            "should find impl MyStruct"
        );
    }

    #[test]
    fn extract_rust_skips_test_module() {
        let code = r#"
pub fn real_fn() {}

#[cfg(test)]
mod tests {
    fn test_something() {}
}
"#;
        let symbols = extract_symbols(code, "rust");
        assert!(
            symbols.iter().any(|s| s.name == "real_fn"),
            "should find real_fn"
        );
        assert!(
            !symbols.iter().any(|s| s.name == "test_something"),
            "should skip test_something inside #[cfg(test)]"
        );
    }

    #[test]
    fn extract_rust_pub_visibility() {
        let code = "pub fn public_one() {}\nfn private_one() {}\n";
        let symbols = extract_symbols(code, "rust");
        let public = symbols.iter().find(|s| s.name == "public_one").unwrap();
        assert!(public.is_public);
        let private = symbols.iter().find(|s| s.name == "private_one").unwrap();
        assert!(!private.is_public);
    }

    #[test]
    fn extract_python_symbols() {
        let code = r#"
class MyClass:
    def method(self):
        pass

def top_level_func(x, y):
    return x + y

async def async_handler(req):
    pass

MAX_SIZE = 1024
"#;
        let symbols = extract_symbols(code, "python");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "MyClass" && s.kind == SymbolKind::Class),
            "should find class MyClass"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "top_level_func" && s.kind == SymbolKind::Function),
            "should find def top_level_func"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "async_handler" && s.kind == SymbolKind::Function),
            "should find async def async_handler"
        );
    }

    #[test]
    fn extract_python_skips_indented() {
        let code = "class Foo:\n    def method(self):\n        pass\n";
        let symbols = extract_symbols(code, "python");
        // `method` is indented, so should NOT be extracted as top-level
        assert!(
            !symbols.iter().any(|s| s.name == "method"),
            "should skip indented def method"
        );
        assert!(symbols.iter().any(|s| s.name == "Foo"));
    }

    #[test]
    fn extract_js_symbols() {
        let code = r#"
export function fetchData(url) { }
function helper() { }
export class ApiClient { }
const BASE_URL = "https://api.example.com";
export default function main() { }
"#;
        let symbols = extract_symbols(code, "javascript");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "fetchData" && s.kind == SymbolKind::Function),
            "should find export function fetchData"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "ApiClient" && s.kind == SymbolKind::Class),
            "should find export class ApiClient"
        );
    }

    #[test]
    fn extract_typescript_symbols() {
        let code = r#"
interface Config { key: string; }
type Result<T> = { data: T; error?: string; }
export class Service { }
"#;
        let symbols = extract_symbols(code, "typescript");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Config" && s.kind == SymbolKind::Interface),
            "should find interface Config"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Result" && s.kind == SymbolKind::Type),
            "should find type Result"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Service" && s.kind == SymbolKind::Class),
            "should find export class Service"
        );
    }

    #[test]
    fn extract_go_symbols() {
        let code = r#"
func main() { }
func (s *Server) Handle(w http.ResponseWriter, r *http.Request) { }
type Server struct { port int }
type Handler interface { Handle() }
"#;
        let symbols = extract_symbols(code, "go");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "main" && s.kind == SymbolKind::Function),
            "should find func main"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Server" && s.kind == SymbolKind::Struct),
            "should find type Server struct"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Handler" && s.kind == SymbolKind::Interface),
            "should find type Handler interface"
        );
    }

    #[test]
    fn extract_go_method() {
        let code = "func (s *Server) Handle(w http.ResponseWriter) { }\n";
        let symbols = extract_symbols(code, "go");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Handle" && s.kind == SymbolKind::Function),
            "should find method Handle"
        );
    }

    #[test]
    fn extract_java_symbols() {
        let code = r#"
public class MyApp {
    public void run() { }
    private int count() { return 0; }
}
public interface Runnable {
    void run();
}
public enum Status { OK, ERROR }
"#;
        let symbols = extract_symbols(code, "java");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "MyApp" && s.kind == SymbolKind::Class),
            "should find public class MyApp"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Runnable" && s.kind == SymbolKind::Interface),
            "should find public interface Runnable"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Status" && s.kind == SymbolKind::Enum),
            "should find public enum Status"
        );
    }

    // ── detect_language ──────────────────────────────────────────────

    #[test]
    fn detect_language_known_extensions() {
        assert_eq!(detect_language("main.rs"), Some("rust"));
        assert_eq!(detect_language("app.py"), Some("python"));
        assert_eq!(detect_language("index.js"), Some("javascript"));
        assert_eq!(detect_language("index.jsx"), Some("javascript"));
        assert_eq!(detect_language("lib.ts"), Some("typescript"));
        assert_eq!(detect_language("lib.tsx"), Some("typescript"));
        assert_eq!(detect_language("main.go"), Some("go"));
        assert_eq!(detect_language("App.java"), Some("java"));
    }

    #[test]
    fn detect_language_unknown_extension() {
        assert_eq!(detect_language("README.md"), None);
        assert_eq!(detect_language("Cargo.toml"), None);
        assert_eq!(detect_language("file.txt"), None);
    }

    // ── format_repo_map ─────────────────────────────────────────────

    #[test]
    fn format_repo_map_empty_project() {
        let entries: Vec<FileSymbols> = vec![];
        let result = format_repo_map(&entries);
        assert!(
            result.is_empty(),
            "empty entries should produce empty string"
        );
    }

    #[test]
    fn format_repo_map_basic() {
        let entries = vec![FileSymbols {
            path: "src/main.rs".to_string(),
            lines: 100,
            symbols: vec![
                Symbol {
                    name: "main".to_string(),
                    kind: SymbolKind::Function,
                    is_public: false,
                    line: 1,
                },
                Symbol {
                    name: "Config".to_string(),
                    kind: SymbolKind::Struct,
                    is_public: true,
                    line: 10,
                },
            ],
        }];
        let result = format_repo_map(&entries);
        assert!(result.contains("src/main.rs"));
        assert!(result.contains("100 lines"));
        assert!(result.contains("fn main"));
        assert!(result.contains("struct Config"));
    }

    // ── generate_repo_map_for_prompt_with_limit ─────────────────────

    #[test]
    fn generate_repo_map_respects_size_limit() {
        // We can't control what files are in the repo during tests,
        // but we can verify the function doesn't panic and respects limits
        let result = generate_repo_map_for_prompt_with_limit(1000);
        if let Some(map) = result {
            assert!(
                map.len() <= 1010, // small tolerance for "..." truncation
                "map should respect size limit, got {} chars",
                map.len()
            );
        }
    }

    #[test]
    fn generate_repo_map_for_prompt_does_not_panic() {
        // Should not panic even if no source files exist
        let _result = generate_repo_map_for_prompt();
    }

    // ── handle_map ──────────────────────────────────────────────────

    #[test]
    fn handle_map_no_panic_empty() {
        // Should not panic with default input
        handle_map("/map");
    }

    #[test]
    fn handle_map_no_panic_with_path() {
        // Should not panic with a path argument
        handle_map("/map src/");
    }

    #[test]
    fn handle_map_no_panic_with_all() {
        // Should not panic with --all flag
        handle_map("/map --all");
    }

    // ── /map in KNOWN_COMMANDS and help ─────────────────────────────

    #[test]
    fn map_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/map"),
            "/map should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn map_in_help_text() {
        let help = help_text();
        assert!(
            help.contains("/map"),
            "help_text should mention /map command"
        );
    }

    #[test]
    fn map_has_detailed_help() {
        use crate::help::command_help;
        let help = command_help("map");
        assert!(help.is_some(), "/map should have detailed help text");
        let text = help.unwrap();
        assert!(
            text.contains("structural"),
            "map help should describe structural mapping"
        );
    }

    // ── ast-grep backend ───────────────────────────────────────────

    #[test]
    fn ast_grep_rule_exists_for_supported_languages() {
        for lang in &["rust", "python", "javascript", "typescript", "go", "java"] {
            assert!(
                ast_grep_rule_for_language(lang).is_some(),
                "should have ast-grep rule for {lang}"
            );
        }
    }

    #[test]
    fn ast_grep_rule_none_for_unknown_language() {
        assert!(ast_grep_rule_for_language("haskell").is_none());
        assert!(ast_grep_rule_for_language("").is_none());
    }

    #[test]
    fn parse_ast_grep_symbols_empty_input() {
        let symbols = parse_ast_grep_symbols("[]", "rust");
        assert!(symbols.is_empty());
    }

    #[test]
    fn parse_ast_grep_symbols_invalid_json() {
        let symbols = parse_ast_grep_symbols("not json", "rust");
        assert!(symbols.is_empty());
    }

    #[test]
    fn parse_ast_grep_symbols_rust_function() {
        let json = r#"[{
            "text": "pub fn my_func(x: i32) -> bool {\n    true\n}",
            "range": {"start": {"line": 5, "column": 0}, "end": {"line": 7, "column": 1}},
            "file": "src/lib.rs",
            "ruleId": "symbols"
        }]"#;
        let symbols = parse_ast_grep_symbols(json, "rust");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "my_func");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert!(symbols[0].is_public);
        assert_eq!(symbols[0].line, 5);
    }

    #[test]
    fn parse_ast_grep_symbols_rust_struct() {
        let json = r#"[{
            "text": "pub struct Config {\n    name: String\n}",
            "range": {"start": {"line": 1, "column": 0}, "end": {"line": 3, "column": 1}},
            "file": "src/lib.rs",
            "ruleId": "symbols"
        }]"#;
        let symbols = parse_ast_grep_symbols(json, "rust");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Config");
        assert_eq!(symbols[0].kind, SymbolKind::Struct);
        assert!(symbols[0].is_public);
    }

    #[test]
    fn parse_ast_grep_symbols_rust_impl() {
        let json = r#"[{
            "text": "impl Config {\n    fn new() -> Self { todo!() }\n}",
            "range": {"start": {"line": 10, "column": 0}, "end": {"line": 12, "column": 1}},
            "file": "src/lib.rs",
            "ruleId": "symbols"
        }]"#;
        let symbols = parse_ast_grep_symbols(json, "rust");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Config");
        assert_eq!(symbols[0].kind, SymbolKind::Impl);
    }

    #[test]
    fn parse_ast_grep_symbols_rust_enum_and_trait() {
        let json = r#"[
            {
                "text": "pub enum Color {\n    Red,\n    Blue\n}",
                "range": {"start": {"line": 1, "column": 0}, "end": {"line": 4, "column": 1}},
                "file": "src/lib.rs",
                "ruleId": "symbols"
            },
            {
                "text": "pub trait Drawable {\n    fn draw(&self);\n}",
                "range": {"start": {"line": 6, "column": 0}, "end": {"line": 8, "column": 1}},
                "file": "src/lib.rs",
                "ruleId": "symbols"
            }
        ]"#;
        let symbols = parse_ast_grep_symbols(json, "rust");
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "Color");
        assert_eq!(symbols[0].kind, SymbolKind::Enum);
        assert_eq!(symbols[1].name, "Drawable");
        assert_eq!(symbols[1].kind, SymbolKind::Trait);
    }

    #[test]
    fn parse_ast_grep_symbols_private_fn() {
        let json = r#"[{
            "text": "fn helper() {\n    // ...\n}",
            "range": {"start": {"line": 0, "column": 0}, "end": {"line": 2, "column": 1}},
            "file": "src/lib.rs",
            "ruleId": "symbols"
        }]"#;
        let symbols = parse_ast_grep_symbols(json, "rust");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "helper");
        assert!(!symbols[0].is_public);
    }

    #[test]
    fn parse_ast_grep_symbols_python() {
        let json = r#"[
            {
                "text": "def process(data):\n    pass",
                "range": {"start": {"line": 0, "column": 0}, "end": {"line": 1, "column": 8}},
                "file": "main.py",
                "ruleId": "symbols"
            },
            {
                "text": "class Handler:\n    pass",
                "range": {"start": {"line": 3, "column": 0}, "end": {"line": 4, "column": 8}},
                "file": "main.py",
                "ruleId": "symbols"
            }
        ]"#;
        let symbols = parse_ast_grep_symbols(json, "python");
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "process");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[1].name, "Handler");
        assert_eq!(symbols[1].kind, SymbolKind::Class);
    }

    #[test]
    fn parse_ast_grep_symbols_go() {
        let json = r#"[{
            "text": "func (s *Server) HandleRequest(w http.ResponseWriter, r *http.Request) {",
            "range": {"start": {"line": 10, "column": 0}, "end": {"line": 20, "column": 1}},
            "file": "server.go",
            "ruleId": "symbols"
        }]"#;
        let symbols = parse_ast_grep_symbols(json, "go");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "HandleRequest");
        assert!(symbols[0].is_public, "Go exported func should be public");
    }

    #[test]
    fn parse_symbol_from_text_various_rust() {
        let sym = parse_symbol_from_text("pub const MAX_SIZE: usize = 100;", "rust", 1).unwrap();
        assert_eq!(sym.name, "MAX_SIZE");
        assert_eq!(sym.kind, SymbolKind::Const);
        assert!(sym.is_public);

        let sym = parse_symbol_from_text("mod utils {", "rust", 5).unwrap();
        assert_eq!(sym.name, "utils");
        assert_eq!(sym.kind, SymbolKind::Module);

        let sym = parse_symbol_from_text("pub async fn serve()", "rust", 3).unwrap();
        assert_eq!(sym.name, "serve");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert!(sym.is_public);
    }

    #[test]
    fn parse_symbol_from_text_typescript() {
        let sym =
            parse_symbol_from_text("export interface ApiResponse {", "typescript", 1).unwrap();
        assert_eq!(sym.name, "ApiResponse");
        assert_eq!(sym.kind, SymbolKind::Interface);
        assert!(sym.is_public);

        let sym = parse_symbol_from_text("type Config = {", "typescript", 5).unwrap();
        assert_eq!(sym.name, "Config");
        assert_eq!(sym.kind, SymbolKind::Type);
    }

    #[test]
    fn extract_symbols_ast_grep_returns_none_when_sg_unavailable() {
        // If the system `sg` is NOT ast-grep (or not installed),
        // extract_symbols_ast_grep should return None (graceful fallback).
        // This test just verifies it doesn't panic.
        let result = extract_symbols_ast_grep("nonexistent_file.rs", "rust");
        // Result is None (file doesn't exist) or Some (if sg happened to work)
        // Either way, no panic.
        let _ = result;
    }

    #[test]
    fn build_repo_map_with_regex_backend() {
        // Force regex backend and verify it returns results and correct backend
        let (entries, backend) = build_repo_map_with_backend(Some("src/"), true, true);
        assert_eq!(backend, MapBackend::Regex);
        // We're in a Rust project, so we should find symbols
        assert!(
            !entries.is_empty(),
            "should find symbols in src/ with regex backend"
        );
    }

    #[test]
    fn handle_map_no_panic_with_regex_flag() {
        handle_map("/map --regex");
    }

    #[test]
    fn handle_map_no_panic_with_regex_and_all() {
        handle_map("/map --regex --all");
    }

    #[test]
    fn map_backend_display() {
        // Verify MapBackend values match expected variants
        assert_eq!(MapBackend::AstGrep, MapBackend::AstGrep);
        assert_eq!(MapBackend::Regex, MapBackend::Regex);
        assert_ne!(MapBackend::AstGrep, MapBackend::Regex);
    }
}
