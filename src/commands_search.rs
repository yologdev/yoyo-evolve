//! Search & navigation command handlers: /find, /grep, /index, /outline.
//! Ast-grep (/ast) lives in `commands_ast_grep.rs`.

#[cfg(test)]
use crate::commands_map::Symbol;
use crate::commands_map::{build_repo_map, FileSymbols, SymbolKind};
use crate::format::*;

// ── shell-like tokenizer ─────────────────────────────────────────────────

/// Split a string into tokens, respecting double-quoted groups.
///
/// Unquoted whitespace separates tokens. A double-quoted span is kept as a
/// single token with the quotes stripped. This is intentionally minimal — no
/// backslash escaping, no single quotes — just enough to round-trip multi-word
/// arguments that `try_dispatch_subcommand` wraps in double quotes.
///
/// ```text
/// tokenize_quoted(r#""fn main" src/"#)  →  ["fn main", "src/"]
/// tokenize_quoted("simple word")        →  ["simple", "word"]
/// tokenize_quoted(r#"-s "fn main""#)    →  ["-s", "fn main"]
/// ```
pub(crate) fn tokenize_quoted(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                // If we just closed quotes, the token will be flushed on next
                // whitespace (or at end). If we just opened quotes on a fresh
                // token, we simply start accumulating.
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            other => {
                current.push(other);
            }
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

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
pub(crate) fn list_project_files() -> Vec<String> {
    // Use git toplevel to avoid CWD-dependency (prevents flaky tests when
    // another test calls set_current_dir during parallel execution).
    if let Ok(toplevel) = crate::git::run_git(&["rev-parse", "--show-toplevel"]) {
        if let Ok(output) = std::process::Command::new("git")
            .args(["-C", &toplevel, "ls-files"])
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                let files: Vec<String> = text
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_string())
                    .collect();
                if !files.is_empty() {
                    return files;
                }
            }
        }
    }
    // Fallback: original CWD-based behavior
    if let Ok(text) = crate::git::run_git(&["ls-files"]) {
        let files: Vec<String> = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
        if !files.is_empty() {
            return files;
        }
    }

    // Last resort: recursive listing of current directory (respecting common ignores).
    // Depth 4 is plenty for a non-git fallback — depth 8 was excessive and caused hangs
    // when run from ~ (see issue #333).
    walk_directory(".", 4)
}

/// Maximum number of files returned by `walk_directory`. Prevents hangs when
/// accidentally walking a huge tree like `~` (see issue #333).
const WALK_DIR_FILE_CAP: usize = 10_000;

/// Non-hidden directory names to skip during fallback directory walks.
/// Hidden directories (starting with `.`) are already excluded by the
/// `name.starts_with('.')` check.
const WALK_DIR_IGNORE: &[&str] = &[
    "node_modules",
    "target",
    "go",
    "Library",
    "__pycache__",
    "venv",
    "vendor",
    "dist",
    "build",
    "coverage",
    "bower_components",
];

/// Simple recursive directory walk (fallback when not in a git repo).
fn walk_directory(dir: &str, max_depth: usize) -> Vec<String> {
    let mut files = Vec::new();
    walk_directory_inner(dir, max_depth, 0, &mut files);
    files
}

fn walk_directory_inner(dir: &str, max_depth: usize, depth: usize, files: &mut Vec<String>) {
    if depth > max_depth || files.len() >= WALK_DIR_FILE_CAP {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if files.len() >= WALK_DIR_FILE_CAP {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden dirs and common ignore patterns
        if name.starts_with('.') || WALK_DIR_IGNORE.iter().any(|&ign| name == ign) {
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

// ---------------------------------------------------------------------------
// /outline — lightweight symbol search across the codebase
// ---------------------------------------------------------------------------

/// Maximum outline results shown by default (use `--all` for unlimited).
const OUTLINE_DEFAULT_LIMIT: usize = 30;

/// A single outline search result.
#[derive(Debug, Clone)]
struct OutlineMatch {
    kind: SymbolKind,
    name: String,
    path: String,
    line: usize,
    score: i32,
}

/// Score a symbol name against a query.
///
/// Returns `None` if the symbol doesn't match at all.
/// Higher scores mean better matches:
///   exact name match  > prefix match > substring match
fn outline_score(name: &str, query: &str) -> Option<i32> {
    let name_lower = name.to_lowercase();
    let query_lower = query.to_lowercase();

    if !name_lower.contains(&query_lower) {
        return None;
    }

    let mut score: i32 = 100;

    // Exact match (case-insensitive)
    if name_lower == query_lower {
        score += 100;
    }
    // Prefix match
    else if name_lower.starts_with(&query_lower) {
        score += 50;
    }

    // Bonus for exact case match (respects original casing)
    if name.contains(query) {
        score += 20;
    }

    // Shorter names are slightly preferred (more specific)
    let len_diff = (name.len() as i32 - query.len() as i32).unsigned_abs() as i32;
    score -= len_diff / 2;

    Some(score)
}

/// Collect outline matches from a set of file symbols, filtered by query.
fn collect_outline_matches(entries: &[FileSymbols], query: &str) -> Vec<OutlineMatch> {
    let mut matches = Vec::new();
    for entry in entries {
        for sym in &entry.symbols {
            if let Some(score) = outline_score(&sym.name, query) {
                matches.push(OutlineMatch {
                    kind: sym.kind.clone(),
                    name: sym.name.clone(),
                    path: entry.path.clone(),
                    line: sym.line,
                    score,
                });
            }
        }
    }
    // Sort by score descending, then by name alphabetically for ties
    matches.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    matches
}

/// Format a single outline match as a colored string.
fn format_outline_match(m: &OutlineMatch) -> String {
    let kind_str = match m.kind {
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
        SymbolKind::Macro => format!("{CYAN}macro{RESET}"),
        SymbolKind::Namespace => format!("{MAGENTA}namespace{RESET}"),
    };
    // Pad kind keyword for alignment (longest is "interface" = 9 chars)
    let kind_plain = match m.kind {
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
        SymbolKind::Macro => "macro",
        SymbolKind::Namespace => "namespace",
    };
    let pad = " ".repeat(9_usize.saturating_sub(kind_plain.len()));
    format!(
        "  {kind_str}{pad} {:<30} {DIM}{}:{}{RESET}",
        m.name, m.path, m.line
    )
}

/// Check if a query looks like a file path (contains `/` or has a known code extension).
fn looks_like_file_path(query: &str) -> bool {
    if query.contains('/') {
        return true;
    }
    // Check for known source file extensions
    let known_extensions = [
        ".rs", ".py", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".go", ".java", ".c", ".cpp", ".cc",
        ".h", ".hpp", ".rb", ".swift", ".kt", ".scala", ".zig", ".lua", ".sh", ".bash", ".zsh",
        ".toml", ".json", ".yaml", ".yml", ".xml", ".html", ".css", ".scss", ".md",
    ];
    let lower = query.to_lowercase();
    known_extensions.iter().any(|ext| lower.ends_with(ext))
}

/// Collect ALL symbols from a specific file path, sorted by line number.
fn collect_file_outline(entries: &[FileSymbols], path: &str) -> Vec<OutlineMatch> {
    let mut matches = Vec::new();
    for entry in entries {
        if entry.path == path || entry.path.ends_with(&format!("/{path}")) {
            for sym in &entry.symbols {
                matches.push(OutlineMatch {
                    kind: sym.kind.clone(),
                    name: sym.name.clone(),
                    path: entry.path.clone(),
                    line: sym.line,
                    score: 0, // not relevant for file-path mode
                });
            }
        }
    }
    matches.sort_by_key(|m| m.line);
    matches
}

/// Handle the `/outline <query|filepath> [--all]` command.
pub fn handle_outline(input: &str) {
    let rest = input.strip_prefix("/outline").unwrap_or(input).trim();

    // Parse --all flag
    let (query, show_all) = if rest.ends_with(" --all") {
        (rest.trim_end_matches(" --all").trim(), true)
    } else if rest == "--all" {
        ("", true)
    } else {
        (rest, false)
    };

    if query.is_empty() {
        println!(
            "{DIM}  Usage: /outline <query|filepath> [--all]{RESET}\n  \
             Search for symbols across the project, or show all symbols in a file.\n\n  \
             Examples:\n    \
             /outline parse            {DIM}# search for symbols matching \"parse\"{RESET}\n    \
             /outline Config            {DIM}# search for symbols matching \"Config\"{RESET}\n    \
             /outline src/main.rs       {DIM}# show all symbols in src/main.rs{RESET}\n    \
             /outline handle --all      {DIM}# show all matches (no limit){RESET}"
        );
        return;
    }

    // Build symbol map (include all symbols, not just public)
    let entries = build_repo_map(None, false);

    // File-path mode: if the query looks like a file path and the file exists,
    // show all symbols from that specific file.
    if looks_like_file_path(query) && std::path::Path::new(query).exists() {
        let file_matches = collect_file_outline(&entries, query);
        if !file_matches.is_empty() {
            println!();
            for m in &file_matches {
                println!("{}", format_outline_match(m));
            }
            println!();
            println!(
                "{DIM}  {} symbol(s) in \"{query}\"{RESET}",
                file_matches.len()
            );
            return;
        }
        // If the file exists but had no symbols extracted, fall through to symbol search.
        // This handles e.g. non-code files like .md that have no parseable symbols.
    }

    // Symbol-name search mode
    let matches = collect_outline_matches(&entries, query);

    if matches.is_empty() {
        println!("{DIM}  No symbols matching \"{query}\" found.{RESET}");
        return;
    }

    let total = matches.len();
    let limit = if show_all {
        total
    } else {
        total.min(OUTLINE_DEFAULT_LIMIT)
    };

    println!();
    for m in &matches[..limit] {
        println!("{}", format_outline_match(m));
    }

    if !show_all && total > OUTLINE_DEFAULT_LIMIT {
        println!(
            "\n{DIM}  ... {} more — use /outline {query} --all to show all{RESET}",
            total - OUTLINE_DEFAULT_LIMIT
        );
    } else {
        println!();
    }
    println!("{DIM}  {} symbol(s) matching \"{query}\"{RESET}", total);
}

/// Maximum matches to display before truncating.
const GREP_MAX_MATCHES: usize = 50;

/// Parsed arguments for the `/grep` command.
#[derive(Debug, Clone, PartialEq)]
pub struct GrepArgs {
    pub pattern: String,
    pub path: String,
    pub case_sensitive: bool,
    /// Lines of context around matches: (before, after).
    /// Set by `-C N` (symmetric), `-B N` (before only), `-A N` (after only).
    /// Flags combine: `-B 2 -A 1` → `Some((2, 1))`.
    pub context_lines: Option<(u32, u32)>,
}

/// Parse `/grep` arguments.
///
/// Syntax: `/grep [-s|--case] [-C N|--context N] [-B N|--before N] [-A N|--after N] <pattern> [path]`
///
/// Supports double-quoted patterns for multi-word searches:
/// `/grep "fn main" src/` → pattern = "fn main", path = "src/"
///
/// Context flags:
/// - `-C N` / `--context N` — show N lines before and after each match
/// - `-B N` / `--before N` — show N lines before each match
/// - `-A N` / `--after N` — show N lines after each match
///
/// Flags combine: `-B 2 -A 1` shows 2 before, 1 after.
///
/// Returns `None` if the pattern is empty.
pub fn parse_grep_args(input: &str) -> Option<GrepArgs> {
    let rest = input.strip_prefix("/grep").unwrap_or(input).trim();

    if rest.is_empty() {
        return None;
    }

    let tokens = tokenize_quoted(rest);

    let mut case_sensitive = false;
    let mut context_before: Option<u32> = None;
    let mut context_after: Option<u32> = None;
    let mut remaining_parts: Vec<String> = Vec::new();

    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        match token.as_str() {
            "-s" | "--case" => {
                case_sensitive = true;
            }
            "-C" | "--context" => {
                if let Some(n) = tokens.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                    context_before = Some(n);
                    context_after = Some(n);
                    i += 1; // consume the number
                }
                // If no valid number follows, flag is silently ignored
            }
            "-B" | "--before" => {
                if let Some(n) = tokens.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                    context_before = Some(n);
                    i += 1;
                }
            }
            "-A" | "--after" => {
                if let Some(n) = tokens.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                    context_after = Some(n);
                    i += 1;
                }
            }
            _ => {
                remaining_parts.push(token.clone());
            }
        }
        i += 1;
    }

    if remaining_parts.is_empty() {
        return None;
    }

    let pattern = remaining_parts[0].clone();
    let path = if remaining_parts.len() > 1 {
        remaining_parts[1..].join(" ")
    } else {
        ".".to_string()
    };

    let context_lines = match (context_before, context_after) {
        (Some(b), Some(a)) => Some((b, a)),
        (Some(b), None) => Some((b, 0)),
        (None, Some(a)) => Some((0, a)),
        (None, None) => None,
    };

    Some(GrepArgs {
        pattern,
        path,
        case_sensitive,
        context_lines,
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

/// Run grep with context lines and return the raw output string.
///
/// When context lines are requested, the output includes match lines,
/// context lines, and `--` group separators. We return the raw string
/// for formatting rather than parsing into `GrepMatch` structs.
fn run_grep_with_context(args: &GrepArgs) -> Result<String, String> {
    let (before, after) = args.context_lines.unwrap_or((0, 0));

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
        if before > 0 {
            cmd.args(["-B", &before.to_string()]);
        }
        if after > 0 {
            cmd.args(["-A", &after.to_string()]);
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
        if before > 0 {
            cmd.args(["-B", &before.to_string()]);
        }
        if after > 0 {
            cmd.args(["-A", &after.to_string()]);
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
        Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
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

/// Format raw grep output that includes context lines.
///
/// `git grep -C N` / `grep -C N` output has three line types:
/// - Match lines: `file:linenum:text` (colon separator after linenum)
/// - Context lines: `file-linenum-text` (dash separator after linenum)
/// - Group separators: `--`
///
/// Match lines get highlighted; context lines are dimmed.
/// Truncation respects `GREP_MAX_MATCHES` counting only match lines.
pub fn format_grep_results_with_context(
    raw_output: &str,
    pattern: &str,
    case_sensitive: bool,
) -> String {
    if raw_output.trim().is_empty() {
        return format!("{DIM}  No matches found.{RESET}\n");
    }

    let mut output = String::new();
    let mut match_count: usize = 0;
    let mut truncated = false;

    for line in raw_output.lines() {
        if line == "--" {
            // Group separator
            if !truncated {
                output.push_str(&format!("  {DIM}--{RESET}\n"));
            }
            continue;
        }

        // Try to parse as match line (file:linenum:text — colon after linenum)
        if let Some(parsed) = parse_context_line(line) {
            if parsed.is_match {
                match_count += 1;
                if match_count > GREP_MAX_MATCHES {
                    truncated = true;
                    continue;
                }
                let highlighted = highlight_grep_match(&parsed.text, pattern, case_sensitive);
                output.push_str(&format!(
                    "  {GREEN}{}{RESET}:{CYAN}{}{RESET}: {}\n",
                    parsed.file, parsed.line_num, highlighted
                ));
            } else if !truncated {
                // Context line — dimmed
                output.push_str(&format!(
                    "  {DIM}{}{RESET}:{DIM}{}{RESET}: {DIM}{}{RESET}\n",
                    parsed.file, parsed.line_num, parsed.text
                ));
            }
        } else if !truncated {
            // Unparseable line — show as-is, dimmed
            output.push_str(&format!("  {DIM}{line}{RESET}\n"));
        }
    }

    if match_count > GREP_MAX_MATCHES {
        output.push_str(&format!(
            "\n{DIM}  ({} more matches, narrow your search){RESET}\n",
            match_count - GREP_MAX_MATCHES
        ));
    } else {
        output.push_str(&format!(
            "\n{DIM}  {} match{}{RESET}\n",
            match_count,
            if match_count == 1 { "" } else { "es" }
        ));
    }

    output
}

/// A parsed line from context-mode grep output.
struct ContextLine {
    file: String,
    line_num: String,
    text: String,
    /// true if this is a match line (colon separator), false if context (dash separator)
    is_match: bool,
}

/// Parse a single line from `grep -C` output.
///
/// Match lines:   `file:linenum:text`  (colon separators)
/// Context lines: `file-linenum-text`  (dash separators)
///
/// Strategy: try match-line format first (file:num:text), then context-line
/// format (file-num-text). For context lines we scan backwards from the end
/// to find `\d+-` and `-\d+-` patterns to locate the linenum, since filenames
/// can contain dashes.
fn parse_context_line(line: &str) -> Option<ContextLine> {
    // Strategy 1: match line — find `:<digits>:` pattern
    // We look for a colon followed by digits followed by another colon.
    if let Some(m) = find_line_num_pattern(line, ':') {
        return Some(ContextLine {
            file: line[..m.file_end].to_string(),
            line_num: line[m.num_start..m.num_end].to_string(),
            text: line[m.text_start..].to_string(),
            is_match: true,
        });
    }

    // Strategy 2: context line — find `-<digits>-` pattern
    // Scan from the right to handle filenames with dashes.
    if let Some(m) = find_line_num_pattern(line, '-') {
        return Some(ContextLine {
            file: line[..m.file_end].to_string(),
            line_num: line[m.num_start..m.num_end].to_string(),
            text: line[m.text_start..].to_string(),
            is_match: false,
        });
    }

    None
}

/// Positions of a `sep<digits>sep` match in a line.
struct LineNumPos {
    file_end: usize,
    num_start: usize,
    num_end: usize,
    text_start: usize,
}

/// Find a `<sep><digits><sep>` pattern in `line` where `sep` is `:` or `-`.
///
/// For `:`, scans left-to-right (first match — shortest filename).
/// For `-`, scans right-to-left (last match — longest filename) because
/// filenames can contain dashes but the linenum is always the rightmost
/// `-<digits>-` before the text.
fn find_line_num_pattern(line: &str, sep: char) -> Option<LineNumPos> {
    let bytes = line.as_bytes();
    let sep_byte = sep as u8;

    if sep == ':' {
        // Scan left-to-right: find first :<digits>:
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == sep_byte {
                // Check if digits follow
                let num_start = i + 1;
                let mut j = num_start;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > num_start && j < bytes.len() && bytes[j] == sep_byte {
                    return Some(LineNumPos {
                        file_end: i,
                        num_start,
                        num_end: j,
                        text_start: j + 1,
                    });
                }
            }
            i += 1;
        }
    } else {
        // Scan right-to-left for -<digits>-: find the rightmost match
        // where the digits portion is valid. This handles filenames with dashes.
        let mut best: Option<LineNumPos> = None;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == sep_byte {
                let num_start = i + 1;
                let mut j = num_start;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > num_start && j < bytes.len() && bytes[j] == sep_byte {
                    best = Some(LineNumPos {
                        file_end: i,
                        num_start,
                        num_end: j,
                        text_start: j + 1,
                    });
                }
            }
            i += 1;
        }
        return best;
    }
    None
}
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
            println!("{DIM}  usage: /grep [-s|--case] [-C N] [-B N] [-A N] <pattern> [path]");
            println!("  Search file contents directly — no AI, no tokens, instant results.");
            println!("  Case-insensitive by default. Use -s or --case for case-sensitive.");
            println!();
            println!("  Context lines:");
            println!("    -C N / --context N  Show N lines before and after each match");
            println!("    -B N / --before N   Show N lines before each match");
            println!("    -A N / --after N    Show N lines after each match");
            println!();
            println!("  Examples:");
            println!("    /grep TODO");
            println!("    /grep \"fn main\" src/");
            println!("    /grep -s MyStruct src/lib.rs");
            println!("    /grep -C 3 \"fn main\" src/");
            println!("    /grep -B 2 -A 1 TODO{RESET}\n");
            return;
        }
    };

    if args.context_lines.is_some() {
        // Context mode: use raw output with context-aware formatting
        match run_grep_with_context(&args) {
            Ok(raw) => {
                let formatted =
                    format_grep_results_with_context(&raw, &args.pattern, args.case_sensitive);
                print!("{formatted}");
            }
            Err(e) => {
                println!("{RED}  Error: {e}{RESET}\n");
            }
        }
    } else {
        // Standard mode: structured parsing
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::KNOWN_COMMANDS;
    use crate::help::help_text;
    use std::fs;
    use tempfile::TempDir;

    // ── tokenize_quoted ────────────────────────────────────────────

    #[test]
    fn tokenize_quoted_simple_words() {
        assert_eq!(tokenize_quoted("hello world"), vec!["hello", "world"]);
    }

    #[test]
    fn tokenize_quoted_double_quoted_group() {
        assert_eq!(
            tokenize_quoted(r#""fn main" src/"#),
            vec!["fn main", "src/"]
        );
    }

    #[test]
    fn tokenize_quoted_mixed() {
        assert_eq!(
            tokenize_quoted(r#"-s "fn main" src/"#),
            vec!["-s", "fn main", "src/"]
        );
    }

    #[test]
    fn tokenize_quoted_empty() {
        let empty: Vec<String> = vec![];
        assert_eq!(tokenize_quoted(""), empty);
        assert_eq!(tokenize_quoted("   "), empty);
    }

    #[test]
    fn tokenize_quoted_no_quotes() {
        assert_eq!(tokenize_quoted("TODO src/"), vec!["TODO", "src/"]);
    }

    #[test]
    fn tokenize_quoted_adjacent_to_text() {
        // Quote directly adjacent to unquoted text gets merged
        assert_eq!(tokenize_quoted(r#"pre"quoted"post"#), vec!["prequotedpost"]);
    }

    #[test]
    fn tokenize_quoted_empty_quotes() {
        // Empty quotes produce an empty token only if adjacent to nothing
        // Actually, "" alone produces nothing since current is empty
        assert_eq!(tokenize_quoted(r#"a "" b"#), vec!["a", "b"]);
    }

    #[test]
    fn tokenize_quoted_multiple_spaces() {
        assert_eq!(tokenize_quoted("  a   b   c  "), vec!["a", "b", "c"]);
    }

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

    #[test]
    fn walk_directory_respects_file_cap() {
        let dir = TempDir::new().unwrap();
        // Create more files than WALK_DIR_FILE_CAP
        let count = WALK_DIR_FILE_CAP + 500;
        for i in 0..count {
            fs::write(dir.path().join(format!("file_{i}.txt")), "").unwrap();
        }
        let files = walk_directory(dir.path().to_str().unwrap(), 3);
        assert!(
            files.len() <= WALK_DIR_FILE_CAP,
            "walk_directory returned {} files, expected at most {}",
            files.len(),
            WALK_DIR_FILE_CAP,
        );
        // Should still return a substantial number of files
        assert!(files.len() >= WALK_DIR_FILE_CAP - 1);
    }

    #[test]
    fn walk_directory_skips_expanded_ignore_dirs() {
        let dir = TempDir::new().unwrap();
        // Create directories that should be ignored
        for ignored in &[
            "go",
            "vendor",
            "__pycache__",
            "venv",
            "build",
            "dist",
            "Library",
        ] {
            fs::create_dir(dir.path().join(ignored)).unwrap();
            fs::write(dir.path().join(format!("{ignored}/should_skip.txt")), "").unwrap();
        }
        fs::write(dir.path().join("keep.txt"), "").unwrap();

        let files = walk_directory(dir.path().to_str().unwrap(), 3);
        assert!(files.iter().any(|f| f.ends_with("keep.txt")));
        assert!(
            !files.iter().any(|f| f.contains("should_skip")),
            "walk_directory should skip expanded ignore dirs, got: {files:?}"
        );
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
    fn parse_grep_args_quoted_pattern() {
        let args = parse_grep_args(r#"/grep "fn main""#).unwrap();
        assert_eq!(args.pattern, "fn main");
        assert_eq!(args.path, ".");
        assert!(!args.case_sensitive);
    }

    #[test]
    fn parse_grep_args_quoted_pattern_with_path() {
        let args = parse_grep_args(r#"/grep "fn main" src/"#).unwrap();
        assert_eq!(args.pattern, "fn main");
        assert_eq!(args.path, "src/");
        assert!(!args.case_sensitive);
    }

    #[test]
    fn parse_grep_args_quoted_pattern_case_sensitive() {
        let args = parse_grep_args(r#"/grep -s "fn main" src/"#).unwrap();
        assert_eq!(args.pattern, "fn main");
        assert_eq!(args.path, "src/");
        assert!(args.case_sensitive);
    }

    #[test]
    fn parse_grep_args_backward_compat_single_word() {
        // Ensure single-word patterns still work without quotes
        let args = parse_grep_args("/grep TODO").unwrap();
        assert_eq!(args.pattern, "TODO");
        assert_eq!(args.path, ".");
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
            context_lines: None,
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

    #[test]
    fn parse_grep_args_context_c_flag() {
        let args = parse_grep_args("/grep -C 3 TODO src/").unwrap();
        assert_eq!(args.pattern, "TODO");
        assert_eq!(args.path, "src/");
        assert_eq!(args.context_lines, Some((3, 3)));
    }

    #[test]
    fn parse_grep_args_context_long_flag() {
        let args = parse_grep_args("/grep --context 5 TODO").unwrap();
        assert_eq!(args.pattern, "TODO");
        assert_eq!(args.context_lines, Some((5, 5)));
    }

    #[test]
    fn parse_grep_args_before_flag() {
        let args = parse_grep_args("/grep -B 2 TODO").unwrap();
        assert_eq!(args.pattern, "TODO");
        assert_eq!(args.context_lines, Some((2, 0)));
    }

    #[test]
    fn parse_grep_args_after_flag() {
        let args = parse_grep_args("/grep -A 4 TODO").unwrap();
        assert_eq!(args.pattern, "TODO");
        assert_eq!(args.context_lines, Some((0, 4)));
    }

    #[test]
    fn parse_grep_args_before_and_after_combined() {
        let args = parse_grep_args("/grep -B 2 -A 1 TODO src/").unwrap();
        assert_eq!(args.pattern, "TODO");
        assert_eq!(args.path, "src/");
        assert_eq!(args.context_lines, Some((2, 1)));
    }

    #[test]
    fn parse_grep_args_context_with_case_flag() {
        let args = parse_grep_args("/grep -s -C 3 MyStruct").unwrap();
        assert_eq!(args.pattern, "MyStruct");
        assert!(args.case_sensitive);
        assert_eq!(args.context_lines, Some((3, 3)));
    }

    #[test]
    fn parse_grep_args_context_without_number_ignored() {
        // -C without a number should be silently ignored, not consume pattern
        let args = parse_grep_args("/grep -C TODO").unwrap();
        assert_eq!(args.pattern, "TODO");
        assert_eq!(args.context_lines, None);
    }

    #[test]
    fn parse_grep_args_no_context_by_default() {
        let args = parse_grep_args("/grep TODO").unwrap();
        assert_eq!(args.context_lines, None);
    }

    #[test]
    fn format_grep_context_empty() {
        let formatted = format_grep_results_with_context("", "pattern", false);
        assert!(formatted.contains("No matches found"));
    }

    #[test]
    fn format_grep_context_match_and_context_lines() {
        // Simulate git grep -C 1 output
        let raw = "\
src/main.rs-1-// comment before
src/main.rs:2:fn main() {
src/main.rs-3-    println!(\"hello\");";
        let formatted = format_grep_results_with_context(raw, "fn main", false);
        // Should contain the match
        assert!(formatted.contains("main.rs"));
        assert!(formatted.contains("2"));
        assert!(formatted.contains("1 match"));
    }

    #[test]
    fn format_grep_context_group_separator() {
        let raw = "\
src/a.rs:10:match one
--
src/b.rs:20:match two";
        let formatted = format_grep_results_with_context(raw, "match", false);
        assert!(formatted.contains("--"));
        assert!(formatted.contains("2 matches"));
    }

    #[test]
    fn parse_context_line_match() {
        let parsed = parse_context_line("src/main.rs:42:fn main() {").unwrap();
        assert_eq!(parsed.file, "src/main.rs");
        assert_eq!(parsed.line_num, "42");
        assert_eq!(parsed.text, "fn main() {");
        assert!(parsed.is_match);
    }

    #[test]
    fn parse_context_line_context() {
        let parsed = parse_context_line("src/main.rs-43-    println!(\"hello\");").unwrap();
        assert_eq!(parsed.file, "src/main.rs");
        assert_eq!(parsed.line_num, "43");
        assert_eq!(parsed.text, "    println!(\"hello\");");
        assert!(!parsed.is_match);
    }

    #[test]
    fn run_grep_with_context_finds_matches() {
        // Test that context mode actually works on the real project
        let args = GrepArgs {
            pattern: "fn main".to_string(),
            path: "src/main.rs".to_string(),
            case_sensitive: true,
            context_lines: Some((1, 1)),
        };
        let result = run_grep_with_context(&args).unwrap();
        assert!(
            !result.is_empty(),
            "Should find 'fn main' with context in src/main.rs"
        );
        assert!(result.contains("fn main"));
    }

    #[test]
    fn list_project_files_returns_known_file() {
        // Verify that list_project_files() returns results including Cargo.toml
        // even if CWD has drifted, thanks to the git-toplevel approach.
        let files = list_project_files();
        assert!(
            !files.is_empty(),
            "list_project_files should return at least some files"
        );
        assert!(
            files.iter().any(|f| f == "Cargo.toml"),
            "list_project_files should include Cargo.toml; got {} files",
            files.len()
        );
    }

    // ── tests moved from commands.rs (Issue #260) ───────────────────

    #[test]
    fn test_find_command_recognized() {
        use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
        assert!(!is_unknown_command("/find"));
        assert!(!is_unknown_command("/find main"));
        assert!(!is_unknown_command("/find .toml"));
        assert!(
            KNOWN_COMMANDS.contains(&"/find"),
            "/find should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_fuzzy_score_basic_match() {
        // Pattern found in path → Some score
        let score = fuzzy_score("src/main.rs", "main");
        assert!(score.is_some(), "should match 'main' in 'src/main.rs'");
        assert!(score.unwrap() > 0, "score should be positive");
    }

    #[test]
    fn test_fuzzy_score_no_match() {
        let score = fuzzy_score("src/main.rs", "zzznotfound");
        assert!(score.is_none(), "should not match 'zzznotfound'");
    }

    #[test]
    fn test_fuzzy_score_case_insensitive() {
        let score_lower = fuzzy_score("src/main.rs", "main");
        let score_upper = fuzzy_score("src/main.rs", "MAIN");
        assert!(score_lower.is_some());
        assert!(score_upper.is_some());
        // Both should match with same score
        assert_eq!(score_lower, score_upper);
    }

    #[test]
    fn test_fuzzy_score_filename_match_higher() {
        // "main" matches in filename for "src/main.rs" but only in dir for "main/other.rs"
        let filename_score = fuzzy_score("src/main.rs", "main");
        let dir_score = fuzzy_score("main_stuff/other.rs", "main");
        assert!(filename_score.is_some());
        assert!(dir_score.is_some());
        // Filename match should score higher because it gets the filename bonus
        assert!(
            filename_score.unwrap() > dir_score.unwrap(),
            "filename match should score higher: {} vs {}",
            filename_score.unwrap(),
            dir_score.unwrap()
        );
    }

    #[test]
    fn test_fuzzy_score_start_of_filename_bonus() {
        // "cli" at start of filename should score higher than "cli" embedded elsewhere
        let start_score = fuzzy_score("src/cli.rs", "cli");
        let mid_score = fuzzy_score("src/public_client.rs", "cli");
        assert!(start_score.is_some());
        assert!(mid_score.is_some());
        assert!(
            start_score.unwrap() > mid_score.unwrap(),
            "start-of-filename match should score higher: {} vs {}",
            start_score.unwrap(),
            mid_score.unwrap()
        );
    }

    #[test]
    fn test_find_files_returns_sorted() {
        // Search for a common pattern in this project
        let matches = find_files("main");
        assert!(!matches.is_empty(), "should find files matching 'main'");
        // Results should be sorted by score descending
        for window in matches.windows(2) {
            assert!(
                window[0].score >= window[1].score,
                "results should be sorted by score descending: {} >= {}",
                window[0].score,
                window[1].score
            );
        }
    }

    #[test]
    fn test_find_files_no_results() {
        let matches = find_files("xyzzy_nonexistent_pattern_12345");
        assert!(
            matches.is_empty(),
            "should find no files for nonsense pattern"
        );
    }

    #[test]
    fn test_find_command_matching() {
        // /find should match exact or with space separator, not /finding
        let find_matches = |s: &str| s == "/find" || s.starts_with("/find ");
        assert!(find_matches("/find"));
        assert!(find_matches("/find main"));
        assert!(find_matches("/find .toml"));
        assert!(!find_matches("/finding"));
        assert!(!find_matches("/findall"));
    }

    #[test]
    fn test_highlight_match_basic() {
        let result = highlight_match("src/main.rs", "main");
        // Should contain the original path text
        assert!(result.contains("main"));
        assert!(result.contains("src/"));
        assert!(result.contains(".rs"));
    }

    #[test]
    fn test_extract_first_meaningful_line_skips_blanks() {
        let content = "\n\n\n//! Module docs here\nfn main() {}";
        let line = extract_first_meaningful_line(content);
        assert_eq!(line, "//! Module docs here");
    }

    #[test]
    fn test_extract_first_meaningful_line_empty() {
        let content = "\n\n\n";
        let line = extract_first_meaningful_line(content);
        assert_eq!(line, "");
    }

    #[test]
    fn test_extract_first_meaningful_line_truncates_long_lines() {
        let content = format!("// {}", "a".repeat(200));
        let line = extract_first_meaningful_line(&content);
        assert!(line.len() <= 83); // 80 chars + "…" (3 bytes)
        assert!(line.ends_with('…'));
    }

    #[test]
    fn test_is_binary_extension() {
        assert!(is_binary_extension("image.png"));
        assert!(is_binary_extension("font.woff2"));
        assert!(is_binary_extension("archive.tar.gz"));
        assert!(!is_binary_extension("main.rs"));
        assert!(!is_binary_extension("Cargo.toml"));
        assert!(!is_binary_extension("README.md"));
    }

    #[test]
    fn test_format_project_index_empty() {
        let entries: Vec<IndexEntry> = vec![];
        let result = format_project_index(&entries);
        assert_eq!(result, "(no indexable files found)");
    }

    #[test]
    fn test_format_project_index_with_entries() {
        let entries = vec![
            IndexEntry {
                path: "src/main.rs".to_string(),
                lines: 100,
                summary: "//! Main module".to_string(),
            },
            IndexEntry {
                path: "src/lib.rs".to_string(),
                lines: 50,
                summary: "//! Library".to_string(),
            },
        ];
        let result = format_project_index(&entries);
        assert!(result.contains("src/main.rs"));
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("//! Main module"));
        assert!(result.contains("//! Library"));
        assert!(result.contains("2 files, 150 total lines"));
    }

    #[test]
    fn test_build_project_index_tempdir() {
        // Create a temp directory with known files and test indexing
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        // Create some test files
        fs::write(dir_path.join("main.rs"), "//! Entry point\nfn main() {}\n").unwrap();
        fs::write(
            dir_path.join("lib.rs"),
            "//! Library code\npub fn hello() {}\n",
        )
        .unwrap();
        fs::write(dir_path.join("image.png"), [0x89, 0x50, 0x4e, 0x47]).unwrap();

        // We can't easily test build_project_index directly since it uses git ls-files
        // or walks cwd, but we CAN test the components
        let content = fs::read_to_string(dir_path.join("main.rs")).unwrap();
        let summary = extract_first_meaningful_line(&content);
        assert_eq!(summary, "//! Entry point");

        // Verify binary filtering
        assert!(is_binary_extension("image.png"));
        assert!(!is_binary_extension("main.rs"));
    }

    #[test]
    fn test_index_entry_construction() {
        let entry = IndexEntry {
            path: "src/commands.rs".to_string(),
            lines: 4000,
            summary: "//! REPL command handlers for yoyo.".to_string(),
        };
        assert_eq!(entry.path, "src/commands.rs");
        assert_eq!(entry.lines, 4000);
        assert_eq!(entry.summary, "//! REPL command handlers for yoyo.");
    }

    #[test]
    fn test_format_project_index_single_file() {
        let entries = vec![IndexEntry {
            path: "README.md".to_string(),
            lines: 1,
            summary: "# Hello".to_string(),
        }];
        let result = format_project_index(&entries);
        assert!(result.contains("1 file, 1 total lines"));
    }

    // ── /outline tests ──────────────────────────────────────────────────

    #[test]
    fn outline_score_exact_match() {
        let score = outline_score("Config", "Config").unwrap();
        assert!(score > 200, "exact match should score high: {score}");
    }

    #[test]
    fn outline_score_prefix_match() {
        let score = outline_score("parse_args", "parse").unwrap();
        assert!(score > 150, "prefix match should score well: {score}");
    }

    #[test]
    fn outline_score_substring_match() {
        let score = outline_score("handle_outline", "outline").unwrap();
        assert!(score >= 100, "substring match should score: {score}");
    }

    #[test]
    fn outline_score_no_match() {
        assert!(outline_score("Config", "zzz").is_none());
    }

    #[test]
    fn outline_score_case_insensitive() {
        assert!(outline_score("Config", "config").is_some());
        assert!(outline_score("config", "Config").is_some());
    }

    #[test]
    fn outline_score_case_bonus() {
        let case_match = outline_score("Config", "Config").unwrap();
        let case_mismatch = outline_score("Config", "config").unwrap();
        assert!(
            case_match > case_mismatch,
            "exact case should score higher: {case_match} vs {case_mismatch}"
        );
    }

    #[test]
    fn outline_score_exact_beats_prefix() {
        let exact = outline_score("parse", "parse").unwrap();
        let prefix = outline_score("parse_args", "parse").unwrap();
        assert!(
            exact > prefix,
            "exact should beat prefix: {exact} vs {prefix}"
        );
    }

    #[test]
    fn outline_collect_matches_filters() {
        let entries = vec![FileSymbols {
            path: "src/main.rs".to_string(),
            lines: 100,
            symbols: vec![
                Symbol {
                    name: "parse_args".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 10,
                },
                Symbol {
                    name: "Config".to_string(),
                    kind: SymbolKind::Struct,
                    is_public: true,
                    line: 20,
                },
                Symbol {
                    name: "run_server".to_string(),
                    kind: SymbolKind::Function,
                    is_public: false,
                    line: 30,
                },
            ],
        }];

        let matches = collect_outline_matches(&entries, "parse");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "parse_args");

        let matches = collect_outline_matches(&entries, "Config");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "Config");

        let matches = collect_outline_matches(&entries, "zzz");
        assert!(matches.is_empty());
    }

    #[test]
    fn outline_collect_matches_sorts_by_score() {
        let entries = vec![FileSymbols {
            path: "src/cli.rs".to_string(),
            lines: 200,
            symbols: vec![
                Symbol {
                    name: "parse_config_file".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 100,
                },
                Symbol {
                    name: "parse".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 50,
                },
                Symbol {
                    name: "parse_args".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 10,
                },
            ],
        }];

        let matches = collect_outline_matches(&entries, "parse");
        // Exact match "parse" should be first, then prefix "parse_args", then longer
        assert_eq!(matches[0].name, "parse");
        assert_eq!(matches[1].name, "parse_args");
        assert_eq!(matches[2].name, "parse_config_file");
    }

    #[test]
    fn outline_format_match_contains_path_and_line() {
        let m = OutlineMatch {
            kind: SymbolKind::Function,
            name: "hello_world".to_string(),
            path: "src/main.rs".to_string(),
            line: 42,
            score: 100,
        };
        let formatted = format_outline_match(&m);
        assert!(formatted.contains("hello_world"));
        assert!(formatted.contains("src/main.rs"));
        assert!(formatted.contains("42"));
    }

    #[test]
    fn outline_result_limit() {
        // With > 30 results, the default should limit to 30
        let symbols: Vec<Symbol> = (0..40)
            .map(|i| Symbol {
                name: format!("parse_{i}"),
                kind: SymbolKind::Function,
                is_public: true,
                line: i + 1,
            })
            .collect();
        let entries = vec![FileSymbols {
            path: "src/test.rs".to_string(),
            lines: 500,
            symbols,
        }];
        let matches = collect_outline_matches(&entries, "parse");
        assert_eq!(matches.len(), 40);
        // The limit is applied in handle_outline, not collect_outline_matches
        let limit = matches.len().min(OUTLINE_DEFAULT_LIMIT);
        assert_eq!(limit, 30);
    }

    #[test]
    fn looks_like_file_path_with_slash() {
        assert!(looks_like_file_path("src/main.rs"));
        assert!(looks_like_file_path("src/format/mod.rs"));
        assert!(looks_like_file_path("lib/utils.py"));
    }

    #[test]
    fn looks_like_file_path_with_known_extension() {
        assert!(looks_like_file_path("main.rs"));
        assert!(looks_like_file_path("utils.py"));
        assert!(looks_like_file_path("index.ts"));
        assert!(looks_like_file_path("app.js"));
        assert!(looks_like_file_path("server.go"));
        assert!(looks_like_file_path("Main.java"));
        assert!(looks_like_file_path("lib.c"));
        assert!(looks_like_file_path("util.cpp"));
        assert!(looks_like_file_path("helper.rb"));
        assert!(looks_like_file_path("config.toml"));
        assert!(looks_like_file_path("data.json"));
        assert!(looks_like_file_path("style.css"));
        assert!(looks_like_file_path("page.html"));
    }

    #[test]
    fn looks_like_file_path_rejects_symbol_names() {
        assert!(!looks_like_file_path("Config"));
        assert!(!looks_like_file_path("parse_args"));
        assert!(!looks_like_file_path("handle"));
        assert!(!looks_like_file_path("run_server"));
    }

    #[test]
    fn collect_file_outline_filters_by_path() {
        let entries = vec![
            FileSymbols {
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
                        line: 20,
                    },
                ],
            },
            FileSymbols {
                path: "src/lib.rs".to_string(),
                lines: 50,
                symbols: vec![Symbol {
                    name: "init".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 5,
                }],
            },
        ];

        let matches = collect_file_outline(&entries, "src/main.rs");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].name, "main");
        assert_eq!(matches[1].name, "Config");

        // Non-existent file returns empty
        let matches = collect_file_outline(&entries, "src/nonexistent.rs");
        assert!(matches.is_empty());
    }

    #[test]
    fn collect_file_outline_sorts_by_line() {
        let entries = vec![FileSymbols {
            path: "src/test.rs".to_string(),
            lines: 100,
            symbols: vec![
                Symbol {
                    name: "gamma".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 30,
                },
                Symbol {
                    name: "alpha".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 10,
                },
                Symbol {
                    name: "beta".to_string(),
                    kind: SymbolKind::Struct,
                    is_public: true,
                    line: 20,
                },
            ],
        }];

        let matches = collect_file_outline(&entries, "src/test.rs");
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].name, "alpha");
        assert_eq!(matches[0].line, 10);
        assert_eq!(matches[1].name, "beta");
        assert_eq!(matches[1].line, 20);
        assert_eq!(matches[2].name, "gamma");
        assert_eq!(matches[2].line, 30);
    }
}
