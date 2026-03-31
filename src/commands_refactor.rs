//! Refactoring command handlers: /extract, /rename, /move, /refactor.

use crate::commands_search::is_binary_extension;
use crate::format::*;

// ── /extract ─────────────────────────────────────────────────────────────

/// Parse `/extract <symbol> <source_file> <target_file>` arguments.
pub fn parse_extract_args(input: &str) -> Option<(String, String, String)> {
    let rest = input.strip_prefix("/extract").unwrap_or(input).trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() == 3 {
        Some((
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
        ))
    } else {
        None
    }
}

/// Find a top-level symbol block (fn, struct, enum, impl, trait, type, const, static) in source text.
/// Returns `(start_line_0indexed, end_line_0indexed, block_text)` where the range
/// is inclusive on both ends.
///
/// Uses brace-depth tracking: finds the line where the symbol keyword + name appear,
/// then scans backwards to collect any `#[...]` attributes or `///` doc comments
/// immediately above, then scans forward counting `{` and `}` until depth returns to 0.
pub fn find_symbol_block(source: &str, symbol: &str) -> Option<(usize, usize, String)> {
    let lines: Vec<&str> = source.lines().collect();

    // Build patterns to match: fn symbol, pub fn symbol, struct symbol, enum symbol,
    // impl symbol, trait symbol, type symbol, const symbol, static symbol, etc.
    let keyword_patterns: Vec<String> = vec![
        format!("fn {symbol}"),
        format!("struct {symbol}"),
        format!("enum {symbol}"),
        format!("impl {symbol}"),
        format!("trait {symbol}"),
        format!("type {symbol}"),
        format!("const {symbol}"),
        format!("static mut {symbol}"),
        format!("static {symbol}"),
    ];

    // Find the line containing the symbol declaration
    let mut decl_line = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        // Skip lines inside comments
        if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }
        for pat in &keyword_patterns {
            // Check if this line contains the pattern at a word boundary
            if let Some(pos) = trimmed.find(pat.as_str()) {
                // Make sure the character after the symbol name is a word boundary
                let after = pos + pat.len();
                if after >= trimmed.len()
                    || !trimmed.as_bytes()[after].is_ascii_alphanumeric()
                        && trimmed.as_bytes()[after] != b'_'
                {
                    // Also verify the keyword is at line start (possibly after pub/pub(crate)/etc.)
                    let before = &trimmed[..pos];
                    let is_valid_prefix = before.is_empty()
                        || before.trim_end().is_empty()
                        || before.trim_end() == "pub"
                        || before.trim_end().starts_with("pub(")
                        || before.trim_end() == "async"
                        || before.trim_end() == "pub async"
                        || before.trim_end() == "unsafe"
                        || before.trim_end() == "pub unsafe";
                    if is_valid_prefix {
                        decl_line = Some(i);
                        break;
                    }
                }
            }
        }
        if decl_line.is_some() {
            break;
        }
    }

    let decl_line = decl_line?;

    // Scan backwards to collect doc comments and attributes
    let mut start_line = decl_line;
    while start_line > 0 {
        let prev = lines[start_line - 1].trim();
        if prev.starts_with("///")
            || prev.starts_with("#[")
            || prev.starts_with("#![")
            || prev.starts_with("//!")
        {
            start_line -= 1;
        } else {
            break;
        }
    }

    // Check if the declaration line is semicolon-terminated (unit struct, etc.)
    // before doing brace scanning, to avoid picking up braces from later code.
    let decl_trimmed = lines[decl_line].trim();
    if decl_trimmed.ends_with(';') {
        let block: String = lines[start_line..=decl_line].join("\n");
        return Some((start_line, decl_line, block));
    }

    // Scan forward with brace-depth tracking
    let mut depth: i32 = 0;
    let mut found_open = false;
    let mut end_line = decl_line;

    for (i, line) in lines.iter().enumerate().skip(decl_line) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                found_open = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        end_line = i;
        if found_open && depth == 0 {
            break;
        }
    }

    // If we never found an opening brace, the item might span multiple lines
    // ending with a semicolon (e.g., type aliases)
    if !found_open {
        // Check if there's a semicolon somewhere in the range
        let has_semi = lines[decl_line..=end_line].iter().any(|l| l.contains(';'));
        if !has_semi {
            return None;
        }
        // End at the line with the semicolon
        for (idx, line) in lines.iter().enumerate().take(end_line + 1).skip(decl_line) {
            if line.contains(';') {
                end_line = idx;
                break;
            }
        }
    }

    let block: String = lines[start_line..=end_line].join("\n");
    Some((start_line, end_line, block))
}

/// Extract a symbol from source_path to target_path.
/// Returns a summary message on success, or an error description.
pub fn extract_symbol(
    source_path: &str,
    target_path: &str,
    symbol: &str,
) -> Result<String, String> {
    // Read source file
    let source_content = std::fs::read_to_string(source_path)
        .map_err(|e| format!("Cannot read source file '{source_path}': {e}"))?;

    // Find the symbol block
    let (start_line, end_line, block_text) = find_symbol_block(&source_content, symbol)
        .ok_or_else(|| format!("Symbol '{symbol}' not found in '{source_path}'"))?;

    // Read target file (create if doesn't exist)
    let target_content = std::fs::read_to_string(target_path).unwrap_or_default();

    // Check if the symbol is pub — if so, we'll add a use statement
    let is_pub = block_text.trim_start().starts_with("pub ")
        || block_text.trim_start().starts_with("/// ")
            && block_text.contains(&format!("pub fn {symbol}"))
        || block_text.trim_start().starts_with("#[")
            && block_text.contains(&format!("pub fn {symbol}"))
        || block_text.trim_start().starts_with("pub(")
        || block_text.contains(&format!("pub struct {symbol}"))
        || block_text.contains(&format!("pub enum {symbol}"))
        || block_text.contains(&format!("pub trait {symbol}"))
        || block_text.contains(&format!("pub type {symbol}"))
        || block_text.contains(&format!("pub const {symbol}"))
        || block_text.contains(&format!("pub static {symbol}"));

    // Remove the block from source
    let source_lines: Vec<&str> = source_content.lines().collect();
    let mut new_source_lines: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < source_lines.len() {
        if i >= start_line && i <= end_line {
            i += 1;
            continue;
        }
        new_source_lines.push(source_lines[i]);
        i += 1;
    }

    // Clean up consecutive blank lines at the removal site
    let mut new_source = new_source_lines.join("\n");
    // Ensure file ends with newline
    if !new_source.ends_with('\n') {
        new_source.push('\n');
    }

    // Append block to target
    let mut new_target = target_content.clone();
    if !new_target.is_empty() && !new_target.ends_with('\n') {
        new_target.push('\n');
    }
    if !new_target.is_empty() {
        new_target.push('\n');
    }
    new_target.push_str(&block_text);
    new_target.push('\n');

    // Write both files
    std::fs::write(source_path, &new_source)
        .map_err(|e| format!("Failed to write source file '{source_path}': {e}"))?;
    std::fs::write(target_path, &new_target)
        .map_err(|e| format!("Failed to write target file '{target_path}': {e}"))?;

    let line_count = end_line - start_line + 1;
    let line_word = crate::format::pluralize(line_count, "line", "lines");
    let pub_note = if is_pub {
        format!(
            "\n  {DIM}Note: '{symbol}' is public — you may need to add a `use` import in '{source_path}'.{RESET}"
        )
    } else {
        String::new()
    };

    Ok(format!(
        "Moved '{symbol}' ({line_count} {line_word}) from '{source_path}' to '{target_path}'.{pub_note}"
    ))
}

/// Handle the `/extract` command: find symbol, preview, confirm, move.
pub fn handle_extract(input: &str) {
    let (symbol, source, target) = match parse_extract_args(input) {
        Some(args) => args,
        None => {
            println!("{DIM}  usage: /extract <symbol> <source_file> <target_file>");
            println!("  Move a function, struct, enum, impl, trait, type alias, const, or static from one file to another.");
            println!("  Shows a preview of the block to be moved and asks for confirmation.");
            println!();
            println!("  Examples:");
            println!("    /extract my_func src/lib.rs src/utils.rs");
            println!("    /extract MyStruct src/main.rs src/types.rs");
            println!("    /extract MyTrait src/old.rs src/new.rs");
            println!("    /extract MyResult src/lib.rs src/errors.rs");
            println!("    /extract MAX_SIZE src/config.rs src/constants.rs{RESET}\n");
            return;
        }
    };

    // Read source
    let source_content = match std::fs::read_to_string(&source) {
        Ok(c) => c,
        Err(e) => {
            println!("{RED}  Cannot read '{source}': {e}{RESET}\n");
            return;
        }
    };

    // Find the symbol
    let (start_line, end_line, block_text) = match find_symbol_block(&source_content, &symbol) {
        Some(found) => found,
        None => {
            println!("{DIM}  Symbol '{symbol}' not found in '{source}'.{RESET}\n");
            return;
        }
    };

    let line_count = end_line - start_line + 1;
    let line_word = crate::format::pluralize(line_count, "line", "lines");

    // Preview
    println!();
    println!("  {BOLD}Extract preview:{RESET}");
    println!(
        "  Move {CYAN}{symbol}{RESET} ({line_count} {line_word}) from {RED}{source}{RESET} → {GREEN}{target}{RESET}"
    );
    println!();

    // Show truncated preview of the block
    let preview_lines: Vec<&str> = block_text.lines().collect();
    let max_preview = 15;
    for (i, line) in preview_lines.iter().take(max_preview).enumerate() {
        println!("    {CYAN}{:>4}{RESET}: {line}", start_line + i + 1);
    }
    if preview_lines.len() > max_preview {
        println!(
            "    {DIM}... ({} more lines){RESET}",
            preview_lines.len() - max_preview
        );
    }
    println!();

    // Ask for confirmation
    print!("  {BOLD}Move this symbol? (y/n): {RESET}");
    use std::io::Write;
    std::io::stdout().flush().ok();

    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        println!("{RED}  Failed to read input.{RESET}\n");
        return;
    }

    let answer = answer.trim().to_lowercase();
    if answer != "y" && answer != "yes" {
        println!("{DIM}  Extract cancelled.{RESET}\n");
        return;
    }

    match extract_symbol(&source, &target, &symbol) {
        Ok(msg) => println!("{GREEN}  ✓ {msg}{RESET}\n"),
        Err(e) => println!("{RED}  ✗ {e}{RESET}\n"),
    }
}

// ── /refactor ─────────────────────────────────────────────────────────────

/// Handle the `/refactor` umbrella command.
///
/// With no arguments, displays a summary of all available refactoring commands.
/// With a subcommand (`rename`, `extract`, `move`), dispatches to the corresponding handler.
pub fn handle_refactor(input: &str) {
    let rest = input.strip_prefix("/refactor").unwrap_or(input).trim();

    if rest.is_empty() {
        println!("{DIM}  Refactoring Tools:");
        println!("    /rename <old> <new>              Rename a symbol across all project files");
        println!(
            "    /extract <item> <src> <dst>      Move a function, struct, or type to another file"
        );
        println!("    /move <Type>::<method> <Target>   Relocate a method between impl blocks");
        println!();
        println!("  Examples:");
        println!("    /rename MyOldStruct MyNewStruct");
        println!("    /extract parse_config src/lib.rs src/config.rs");
        println!("    /move Parser::validate Validator");
        println!();
        println!(
            "  These operate on source text (not ASTs), so they work with any language.{RESET}"
        );
        println!();
        return;
    }

    // Dispatch subcommands: /refactor rename ... → /rename ...
    let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
    let subcmd = parts[0];
    let sub_args = if parts.len() > 1 { parts[1].trim() } else { "" };

    match subcmd {
        "rename" => {
            let forwarded = if sub_args.is_empty() {
                "/rename".to_string()
            } else {
                format!("/rename {sub_args}")
            };
            handle_rename(&forwarded);
        }
        "extract" => {
            let forwarded = if sub_args.is_empty() {
                "/extract".to_string()
            } else {
                format!("/extract {sub_args}")
            };
            handle_extract(&forwarded);
        }
        "move" => {
            let forwarded = if sub_args.is_empty() {
                "/move".to_string()
            } else {
                format!("/move {sub_args}")
            };
            handle_move(&forwarded);
        }
        other => {
            println!("{RED}  Unknown refactoring subcommand: {other}{RESET}");
            println!("{DIM}  Available: rename, extract, move");
            println!("  Run /refactor with no arguments to see all options.{RESET}\n");
        }
    }
}

// ── /rename ──────────────────────────────────────────────────────────────

/// Check if a character is a word boundary character (not alphanumeric or underscore).
fn is_word_boundary_char(c: char) -> bool {
    !c.is_alphanumeric() && c != '_'
}

/// Check if position `pos` in `text` is at a word boundary start.
/// A word boundary exists at the start of the string or when the preceding char
/// is not a word character.
fn is_word_start(text: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    text[..pos].chars().last().is_none_or(is_word_boundary_char)
}

/// Check if position `pos` in `text` is at a word boundary end.
/// A word boundary exists at the end of the string or when the following char
/// is not a word character.
fn is_word_end(text: &str, pos: usize) -> bool {
    if pos >= text.len() {
        return true;
    }
    text[pos..].chars().next().is_none_or(is_word_boundary_char)
}

/// A single rename match with context.
#[derive(Debug, Clone, PartialEq)]
pub struct RenameMatch {
    pub file: String,
    pub line_num: usize,
    pub line_text: String,
    pub column: usize,
}

/// Result of a rename-in-project operation.
#[derive(Debug, Clone, PartialEq)]
pub struct RenameResult {
    pub files_changed: Vec<String>,
    pub total_replacements: usize,
    pub preview: String,
}

/// Perform a word-boundary-aware rename across git-tracked files.
///
/// If `scope` is `Some(path)`, only files under that path are considered.
/// Returns a `RenameResult` with details of what changed, or an error message.
pub fn rename_in_project(
    old_name: &str,
    new_name: &str,
    scope: Option<&str>,
) -> Result<RenameResult, String> {
    if old_name.is_empty() {
        return Err("old_name must not be empty".to_string());
    }
    if new_name.is_empty() {
        return Err("new_name must not be empty".to_string());
    }
    if old_name == new_name {
        return Err("old_name and new_name are identical — nothing to do".to_string());
    }

    let mut matches = find_rename_matches(old_name);

    // Filter by scope if provided
    if let Some(scope_path) = scope {
        matches.retain(|m| m.file.starts_with(scope_path));
    }

    if matches.is_empty() {
        let scope_msg = scope
            .map(|s| format!(" (scoped to '{s}')"))
            .unwrap_or_default();
        return Err(format!(
            "No word-boundary matches found for '{old_name}'{scope_msg}."
        ));
    }

    let preview = format_rename_preview(&matches, old_name, new_name);

    // Collect unique files that will be changed
    let mut files_changed: Vec<String> = matches.iter().map(|m| m.file.clone()).collect();
    files_changed.sort();
    files_changed.dedup();

    let total_replacements = apply_rename(&matches, old_name, new_name);

    Ok(RenameResult {
        files_changed,
        total_replacements,
        preview,
    })
}

/// Find all word-boundary matches of `old_name` across files tracked by git.
/// Skips binary files. Returns matches sorted by file then line number.
pub fn find_rename_matches(old_name: &str) -> Vec<RenameMatch> {
    if old_name.is_empty() {
        return Vec::new();
    }

    let files = list_git_files();
    let mut matches = Vec::new();

    for file_path in &files {
        if is_binary_extension(file_path) {
            continue;
        }

        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (line_idx, line) in content.lines().enumerate() {
            let line_matches = find_word_boundary_matches(line, old_name);
            for col in line_matches {
                matches.push(RenameMatch {
                    file: file_path.clone(),
                    line_num: line_idx + 1,
                    line_text: line.to_string(),
                    column: col,
                });
            }
        }
    }

    matches
}

/// Find all positions in `text` where `pattern` occurs at word boundaries.
pub fn find_word_boundary_matches(text: &str, pattern: &str) -> Vec<usize> {
    if pattern.is_empty() || text.is_empty() {
        return Vec::new();
    }

    let mut positions = Vec::new();
    let mut start = 0;
    let pat_len = pattern.len();

    while start + pat_len <= text.len() {
        if let Some(pos) = text[start..].find(pattern) {
            let abs_pos = start + pos;
            let end_pos = abs_pos + pat_len;

            if is_word_start(text, abs_pos) && is_word_end(text, end_pos) {
                positions.push(abs_pos);
            }

            start = abs_pos + 1;
        } else {
            break;
        }
    }

    positions
}

/// List files tracked by git (via `git ls-files`).
/// Falls back to walking the current directory if not in a git repo.
fn list_git_files() -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["ls-files"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Format a rename preview showing all matches with context.
pub fn format_rename_preview(matches: &[RenameMatch], old_name: &str, new_name: &str) -> String {
    if matches.is_empty() {
        return format!("{DIM}  No matches found for '{old_name}'.{RESET}\n");
    }

    let mut output = String::new();

    // Group by file
    let mut current_file = String::new();
    let mut file_count = 0usize;

    for m in matches {
        if m.file != current_file {
            current_file = m.file.clone();
            file_count += 1;
            output.push_str(&format!("\n  {GREEN}{}{RESET}\n", m.file));
        }

        // Highlight the old name in the line
        let highlighted = m.line_text.replace(
            old_name,
            &format!("{RED}{old_name}{RESET}→{GREEN}{new_name}{RESET}"),
        );
        output.push_str(&format!(
            "    {CYAN}{:>4}{RESET}: {}\n",
            m.line_num, highlighted
        ));
    }

    let match_word = crate::format::pluralize(matches.len(), "match", "matches");
    let file_word = crate::format::pluralize(file_count, "file", "files");
    output.push_str(&format!(
        "\n  {BOLD}{} {match_word}{RESET} across {BOLD}{file_count} {file_word}{RESET}\n",
        matches.len()
    ));
    output.push_str(&format!(
        "  Rename {RED}{old_name}{RESET} → {GREEN}{new_name}{RESET}\n"
    ));

    output
}

/// Apply the rename across all files, replacing word-boundary matches of `old_name`
/// with `new_name`. Returns the number of replacements made.
pub fn apply_rename(matches: &[RenameMatch], old_name: &str, new_name: &str) -> usize {
    if matches.is_empty() {
        return 0;
    }

    // Group matches by file
    let mut files_to_update: std::collections::HashMap<&str, Vec<&RenameMatch>> =
        std::collections::HashMap::new();
    for m in matches {
        files_to_update.entry(m.file.as_str()).or_default().push(m);
    }

    let mut total_replacements = 0usize;

    for file_path in files_to_update.keys() {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut new_content = String::new();
        for line in content.lines() {
            let replaced = replace_word_boundary(line, old_name, new_name);
            // Count how many replacements happened in this line
            let orig_count = find_word_boundary_matches(line, old_name).len();
            total_replacements += orig_count;
            new_content.push_str(&replaced);
            new_content.push('\n');
        }

        // Preserve trailing newline state
        if !content.ends_with('\n') && new_content.ends_with('\n') {
            new_content.pop();
        }

        if let Err(e) = std::fs::write(file_path, &new_content) {
            println!("{RED}  Failed to write {file_path}: {e}{RESET}");
        }
    }

    total_replacements
}

/// Replace all word-boundary occurrences of `old` with `new` in a single line.
pub fn replace_word_boundary(text: &str, old: &str, new: &str) -> String {
    if old.is_empty() {
        return text.to_string();
    }

    let positions = find_word_boundary_matches(text, old);
    if positions.is_empty() {
        return text.to_string();
    }

    let mut result = String::new();
    let mut last_end = 0;

    for pos in positions {
        result.push_str(&text[last_end..pos]);
        result.push_str(new);
        last_end = pos + old.len();
    }
    result.push_str(&text[last_end..]);

    result
}

/// Parse `/rename old_name new_name` arguments.
pub fn parse_rename_args(input: &str) -> Option<(String, String)> {
    let rest = input.strip_prefix("/rename").unwrap_or(input).trim();

    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

/// Handle the `/rename` command: find matches, preview, confirm, apply.
pub fn handle_rename(input: &str) {
    let (old_name, new_name) = match parse_rename_args(input) {
        Some(args) => args,
        None => {
            println!("{DIM}  usage: /rename <old_name> <new_name>");
            println!("  Cross-file symbol renaming with word-boundary matching.");
            println!("  Shows a preview of all changes and asks for confirmation.");
            println!();
            println!("  Examples:");
            println!("    /rename my_func new_func");
            println!("    /rename OldStruct NewStruct");
            println!("    /rename CONFIG_KEY NEW_KEY{RESET}\n");
            return;
        }
    };

    if old_name == new_name {
        println!("{DIM}  (old and new names are the same — nothing to do){RESET}\n");
        return;
    }

    println!("{DIM}  searching for '{old_name}'...{RESET}");

    let matches = find_rename_matches(&old_name);

    if matches.is_empty() {
        println!("{DIM}  No word-boundary matches found for '{old_name}'.{RESET}\n");
        return;
    }

    let preview = format_rename_preview(&matches, &old_name, &new_name);
    print!("{preview}");

    // Ask for confirmation
    print!("\n  {BOLD}Apply rename? (y/n): {RESET}");
    use std::io::Write;
    std::io::stdout().flush().ok();

    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        println!("{RED}  Failed to read input.{RESET}\n");
        return;
    }

    let answer = answer.trim().to_lowercase();
    if answer != "y" && answer != "yes" {
        println!("{DIM}  Rename cancelled.{RESET}\n");
        return;
    }

    let count = apply_rename(&matches, &old_name, &new_name);
    let repl_word = crate::format::pluralize(count, "replacement", "replacements");
    println!("{GREEN}  ✓ Applied {count} {repl_word}.{RESET}\n");
}

// ── /move ─────────────────────────────────────────────────────────────

/// Parsed `/move` command arguments.
pub struct MoveArgs {
    pub source_type: String,
    pub method_name: String,
    pub target_file: Option<String>,
    pub target_type: String,
}

/// Parse `/move SourceType::method_name [file::]TargetType` arguments.
pub fn parse_move_args(input: &str) -> Option<MoveArgs> {
    let rest = input.strip_prefix("/move").unwrap_or(input).trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }

    // Parse source: SourceType::method_name
    let source_parts: Vec<&str> = parts[0].splitn(2, "::").collect();
    if source_parts.len() != 2 {
        return None;
    }
    let source_type = source_parts[0].to_string();
    let method_name = source_parts[1].to_string();

    if source_type.is_empty() || method_name.is_empty() {
        return None;
    }

    // Parse target: [file::]TargetType
    let target = parts[1];
    let (target_file, target_type) = if target.contains("::") {
        let tparts: Vec<&str> = target.splitn(2, "::").collect();
        (Some(tparts[0].to_string()), tparts[1].to_string())
    } else {
        (None, target.to_string())
    };

    if target_type.is_empty() {
        return None;
    }

    Some(MoveArgs {
        source_type,
        method_name,
        target_file,
        target_type,
    })
}

/// Find all `impl TypeName` blocks in source text.
/// Returns a vec of `(start_line, end_line, block_text)` (0-indexed, inclusive).
pub fn find_impl_blocks(source: &str, type_name: &str) -> Vec<(usize, usize, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut results = Vec::new();

    // Patterns to match impl blocks for this type
    let patterns = [
        format!("impl {type_name} "),
        format!("impl {type_name} {{"),
        format!("impl {type_name}{{"),
    ];

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            i += 1;
            continue;
        }

        let mut found = false;
        for pat in &patterns {
            if let Some(pos) = trimmed.find(pat.as_str()) {
                let before = &trimmed[..pos];
                let is_valid_prefix = before.is_empty()
                    || before.trim_end().is_empty()
                    || before.trim_end() == "pub"
                    || before.trim_end().starts_with("pub(");
                if is_valid_prefix {
                    found = true;
                    break;
                }
            }
        }

        // Also match `impl TypeName\n{` (type name at end of line)
        if !found {
            let ends_with_type = trimmed.ends_with(&format!("impl {type_name}"))
                || trimmed.ends_with(&format!("impl {type_name} {{"));
            if ends_with_type {
                let before_impl = trimmed
                    .find("impl ")
                    .map(|p| trimmed[..p].trim_end())
                    .unwrap_or("");
                if before_impl.is_empty() || before_impl == "pub" || before_impl.starts_with("pub(")
                {
                    found = true;
                }
            }
        }

        if found {
            // Scan backwards for attributes/doc comments
            let mut start = i;
            while start > 0 {
                let prev = lines[start - 1].trim();
                if prev.starts_with("///")
                    || prev.starts_with("#[")
                    || prev.starts_with("#![")
                    || prev.starts_with("//!")
                {
                    start -= 1;
                } else {
                    break;
                }
            }

            // Brace-depth tracking
            let mut depth: i32 = 0;
            let mut found_open = false;
            let mut end = i;
            for (j, line) in lines.iter().enumerate().skip(i) {
                for ch in line.chars() {
                    if ch == '{' {
                        depth += 1;
                        found_open = true;
                    } else if ch == '}' {
                        depth -= 1;
                    }
                }
                end = j;
                if found_open && depth == 0 {
                    break;
                }
            }

            let block: String = lines[start..=end].join("\n");
            results.push((start, end, block));
            i = end + 1;
        } else {
            i += 1;
        }
    }

    results
}

/// Find a method within an impl block's text.
/// Returns `(method_start_offset, method_end_offset, method_text, has_self_ref)`
/// where offsets are line numbers relative to the impl block start.
pub fn find_method_in_impl(
    impl_text: &str,
    method_name: &str,
) -> Option<(usize, usize, String, bool)> {
    let lines: Vec<&str> = impl_text.lines().collect();
    let fn_pattern = format!("fn {method_name}");

    let mut decl_line = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }
        if let Some(pos) = trimmed.find(&fn_pattern) {
            // Check word boundary after method name
            let after = pos + fn_pattern.len();
            if after < trimmed.len() {
                let next_char = trimmed.as_bytes()[after];
                if next_char.is_ascii_alphanumeric() || next_char == b'_' {
                    continue;
                }
            }
            // Check valid prefix (pub, pub(crate), async, etc.)
            let before = &trimmed[..pos];
            let is_valid = before.is_empty()
                || before.trim_end().is_empty()
                || before.trim_end() == "pub"
                || before.trim_end().starts_with("pub(")
                || before.trim_end() == "async"
                || before.trim_end() == "pub async"
                || before.trim_end() == "unsafe"
                || before.trim_end() == "pub unsafe"
                || before.trim_end() == "pub async unsafe"
                || before.trim_end() == "async unsafe";
            if is_valid {
                decl_line = Some(i);
                break;
            }
        }
    }

    let decl_line = decl_line?;

    // Scan backwards for doc comments and attributes
    let mut start = decl_line;
    while start > 0 {
        let prev = lines[start - 1].trim();
        if prev.starts_with("///") || prev.starts_with("#[") || prev.starts_with("//!") {
            start -= 1;
        } else {
            break;
        }
    }

    // Brace-depth tracking forward
    let mut depth: i32 = 0;
    let mut found_open = false;
    let mut end = decl_line;
    for (j, line) in lines.iter().enumerate().skip(decl_line) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                found_open = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        end = j;
        if found_open && depth == 0 {
            break;
        }
    }

    let method_text: String = lines[start..=end].join("\n");

    // Check for self references
    let has_self_ref = method_text.contains("self.");

    Some((start, end, method_text, has_self_ref))
}

/// Move a method between impl blocks.
///
/// If `target_file` is `None`, source and target are the same file.
/// Returns `(summary, warning)` on success — the warning is set if `self.` references were found.
pub fn move_method(
    source_file: &str,
    source_type: &str,
    method_name: &str,
    target_file: Option<&str>,
    target_type: &str,
) -> Result<(String, Option<String>), String> {
    let source_content = std::fs::read_to_string(source_file)
        .map_err(|e| format!("Cannot read source file '{source_file}': {e}"))?;

    // Find impl blocks for the source type
    let source_impls = find_impl_blocks(&source_content, source_type);
    if source_impls.is_empty() {
        return Err(format!(
            "No `impl {source_type}` block found in '{source_file}'"
        ));
    }

    // Find the method in one of the source impl blocks
    let mut found = None;
    for (impl_start, impl_end, impl_text) in &source_impls {
        if let Some((m_start, m_end, m_text, has_self)) =
            find_method_in_impl(impl_text, method_name)
        {
            found = Some((*impl_start, *impl_end, m_start, m_end, m_text, has_self));
            break;
        }
    }

    let (impl_start, _impl_end, method_offset_start, method_offset_end, method_text, has_self_ref) =
        found.ok_or_else(|| {
            format!("Method '{method_name}' not found in any `impl {source_type}` block in '{source_file}'")
        })?;

    // Absolute line numbers in source file for the method
    let abs_method_start = impl_start + method_offset_start;
    let abs_method_end = impl_start + method_offset_end;

    // Determine target file content
    let same_file = target_file.is_none() || target_file == Some(source_file);
    let actual_target = target_file.unwrap_or(source_file);

    let target_content = if same_file {
        source_content.clone()
    } else {
        std::fs::read_to_string(actual_target)
            .map_err(|e| format!("Cannot read target file '{actual_target}': {e}"))?
    };

    // Find target impl block
    let target_impls = find_impl_blocks(&target_content, target_type);
    if target_impls.is_empty() {
        return Err(format!(
            "No `impl {target_type}` block found in '{actual_target}'"
        ));
    }

    let (target_impl_start, target_impl_end, _target_impl_text) = &target_impls[0];

    // --- Apply changes ---
    // We need to:
    // 1. Remove the method from the source impl block
    // 2. Insert the method into the target impl block (before the closing `}`)

    let source_lines: Vec<&str> = source_content.lines().collect();
    let target_lines: Vec<&str> = target_content.lines().collect();

    // Determine indentation for the target
    // Look at the first line inside the target impl for indentation
    let target_indent = if *target_impl_end > *target_impl_start + 1 {
        let sample_line = target_lines[target_impl_start + 1];
        let indent_len = sample_line.len() - sample_line.trim_start().len();
        &sample_line[..indent_len]
    } else {
        "    "
    };

    // Re-indent the method text to match target
    let re_indented = reindent_method(&method_text, target_indent);

    if same_file {
        // Same-file move: iterate original lines, skip method, insert before target's `}`
        let mut new_lines: Vec<String> = Vec::new();

        for (i, line) in source_lines.iter().enumerate() {
            // Skip the method lines (they'll be re-inserted at the target)
            if i >= abs_method_start && i <= abs_method_end {
                continue;
            }

            // When we reach the closing `}` of the target impl, insert the method first
            if i == *target_impl_end {
                new_lines.push(String::new());
                new_lines.push(re_indented.clone());
            }

            new_lines.push(line.to_string());
        }

        // Clean up consecutive blank lines
        let mut result = new_lines.join("\n");
        // Remove runs of 3+ blank lines
        while result.contains("\n\n\n\n") {
            result = result.replace("\n\n\n\n", "\n\n\n");
        }
        if !result.ends_with('\n') {
            result.push('\n');
        }

        std::fs::write(source_file, &result)
            .map_err(|e| format!("Failed to write '{source_file}': {e}"))?;
    } else {
        // Cross-file move
        // 1. Remove method from source
        let mut new_source_lines: Vec<&str> = Vec::new();
        for (i, line) in source_lines.iter().enumerate() {
            if i >= abs_method_start && i <= abs_method_end {
                continue;
            }
            new_source_lines.push(line);
        }
        let mut new_source = new_source_lines.join("\n");
        while new_source.contains("\n\n\n\n") {
            new_source = new_source.replace("\n\n\n\n", "\n\n\n");
        }
        if !new_source.ends_with('\n') {
            new_source.push('\n');
        }

        // 2. Insert method into target (before closing `}` of first impl block)
        let mut new_target_lines: Vec<String> = Vec::new();
        for (i, line) in target_lines.iter().enumerate() {
            if i == *target_impl_end {
                new_target_lines.push(String::new());
                new_target_lines.push(re_indented.clone());
            }
            new_target_lines.push(line.to_string());
        }
        let mut new_target = new_target_lines.join("\n");
        if !new_target.ends_with('\n') {
            new_target.push('\n');
        }

        std::fs::write(source_file, &new_source)
            .map_err(|e| format!("Failed to write source '{source_file}': {e}"))?;
        std::fs::write(actual_target, &new_target)
            .map_err(|e| format!("Failed to write target '{actual_target}': {e}"))?;
    }

    let line_count = abs_method_end - abs_method_start + 1;
    let line_word = crate::format::pluralize(line_count, "line", "lines");
    let target_desc = if same_file {
        format!("`impl {target_type}` in '{source_file}'")
    } else {
        format!("`impl {target_type}` in '{actual_target}'")
    };

    let summary = format!(
        "Moved '{source_type}::{method_name}' ({line_count} {line_word}) to {target_desc}."
    );

    let warning = if has_self_ref {
        Some(format!(
            "Method uses `self.` — verify field/method references are valid on `{target_type}`."
        ))
    } else {
        None
    };

    Ok((summary, warning))
}

/// Re-indent a method block to the given indentation.
fn reindent_method(method_text: &str, target_indent: &str) -> String {
    let lines: Vec<&str> = method_text.lines().collect();
    if lines.is_empty() {
        return String::new();
    }

    // Find the minimum indentation of non-empty lines
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                let stripped = if line.len() >= min_indent {
                    &line[min_indent..]
                } else {
                    line.trim_start()
                };
                format!("{target_indent}{stripped}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Handle the `/move` command: parse, preview, confirm, apply.
pub fn handle_move(input: &str) {
    let args = match parse_move_args(input) {
        Some(a) => a,
        None => {
            println!("{DIM}  usage: /move <SourceType>::<method> [file::]<TargetType>");
            println!("  Relocate a method from one impl block to another.");
            println!();
            println!("  Examples:");
            println!("    /move MyStruct::process TargetStruct          (same file)");
            println!("    /move MyStruct::process other.rs::TargetStruct  (cross-file)");
            println!();
            println!("  Shows a preview and asks for confirmation before applying.");
            println!("  Warns if the method uses `self.` references.{RESET}\n");
            return;
        }
    };

    // Determine source file: look for impl block in current directory
    let source_file = find_file_with_impl(&args.source_type);
    let source_file = match source_file {
        Some(f) => f,
        None => {
            println!(
                "{RED}  Could not find a file containing `impl {}`.{RESET}\n",
                args.source_type
            );
            println!("{DIM}  Tip: run /move from the project root directory.{RESET}\n");
            return;
        }
    };

    let target_file = args.target_file.as_deref();

    // Read source to show preview
    let source_content = match std::fs::read_to_string(&source_file) {
        Ok(c) => c,
        Err(e) => {
            println!("{RED}  Cannot read '{source_file}': {e}{RESET}\n");
            return;
        }
    };

    // Find the method for preview
    let impls = find_impl_blocks(&source_content, &args.source_type);
    let mut method_preview = None;
    for (_impl_start, _impl_end, impl_text) in &impls {
        if let Some((_ms, _me, m_text, has_self)) =
            find_method_in_impl(impl_text, &args.method_name)
        {
            method_preview = Some((m_text, has_self));
            break;
        }
    }

    let (method_text, has_self) = match method_preview {
        Some(p) => p,
        None => {
            println!(
                "{DIM}  Method '{}' not found in any `impl {}` block.{RESET}\n",
                args.method_name, args.source_type
            );
            return;
        }
    };

    let actual_target = target_file.unwrap_or(&source_file);
    let line_count = method_text.lines().count();
    let line_word = crate::format::pluralize(line_count, "line", "lines");

    // Preview
    println!();
    println!("  {BOLD}Move preview:{RESET}");
    println!(
        "  Move {CYAN}{}::{}{RESET} ({line_count} {line_word})",
        args.source_type, args.method_name
    );
    println!(
        "  from {RED}impl {}{RESET} in '{source_file}'",
        args.source_type
    );
    println!(
        "  to   {GREEN}impl {}{RESET} in '{actual_target}'",
        args.target_type
    );
    println!();

    // Show method preview
    let preview_lines: Vec<&str> = method_text.lines().collect();
    let max_preview = 15;
    for line in preview_lines.iter().take(max_preview) {
        println!("    {CYAN}│{RESET} {line}");
    }
    if preview_lines.len() > max_preview {
        println!(
            "    {DIM}... ({} more lines){RESET}",
            preview_lines.len() - max_preview
        );
    }
    println!();

    if has_self {
        println!(
            "  {YELLOW}⚠ Method uses `self.` — verify references are valid on `{}`.{RESET}",
            args.target_type
        );
        println!();
    }

    // Confirm
    print!("  {BOLD}Move this method? (y/n): {RESET}");
    use std::io::Write;
    std::io::stdout().flush().ok();

    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        println!("{RED}  Failed to read input.{RESET}\n");
        return;
    }

    let answer = answer.trim().to_lowercase();
    if answer != "y" && answer != "yes" {
        println!("{DIM}  Move cancelled.{RESET}\n");
        return;
    }

    match move_method(
        &source_file,
        &args.source_type,
        &args.method_name,
        args.target_file.as_deref(),
        &args.target_type,
    ) {
        Ok((summary, warning)) => {
            println!("{GREEN}  ✓ {summary}{RESET}");
            if let Some(w) = warning {
                println!("  {YELLOW}⚠ {w}{RESET}");
            }
            println!();
        }
        Err(e) => println!("{RED}  ✗ {e}{RESET}\n"),
    }
}

/// Search project files for one containing `impl TypeName`.
fn find_file_with_impl(type_name: &str) -> Option<String> {
    let pattern = format!("impl {type_name}");

    // Check git-tracked files first
    let output = std::process::Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output()
        .ok()?;

    let file_list = String::from_utf8_lossy(&output.stdout);
    for file in file_list.lines() {
        if !file.ends_with(".rs") {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(file) {
            if content.contains(&pattern) {
                return Some(file.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::KNOWN_COMMANDS;
    use crate::help::help_text;
    use std::fs;
    use tempfile::TempDir;

    // ── rename: word boundary matching ──────────────────────────────

    #[test]
    fn find_word_boundary_simple_match() {
        let matches = find_word_boundary_matches("let foo = 42;", "foo");
        assert_eq!(matches, vec![4]);
    }

    #[test]
    fn find_word_boundary_no_match_substring() {
        // "foo" should NOT match inside "foobar"
        let matches = find_word_boundary_matches("let foobar = 42;", "foo");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_word_boundary_no_match_prefix() {
        // "foo" should NOT match inside "barfoo"... wait, "barfoo" — "foo" is at end
        // but "bar" precedes it without boundary. Let's test "afoo"
        let matches = find_word_boundary_matches("let afoo = 42;", "foo");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_word_boundary_at_start_of_line() {
        let matches = find_word_boundary_matches("foo = 42;", "foo");
        assert_eq!(matches, vec![0]);
    }

    #[test]
    fn find_word_boundary_at_end_of_line() {
        let matches = find_word_boundary_matches("let x = foo", "foo");
        assert_eq!(matches, vec![8]);
    }

    #[test]
    fn find_word_boundary_multiple_matches() {
        let matches = find_word_boundary_matches("foo + foo * foo", "foo");
        assert_eq!(matches, vec![0, 6, 12]);
    }

    #[test]
    fn find_word_boundary_with_underscore() {
        // Underscore is a word character, so "my_func" should not match "my"
        let matches = find_word_boundary_matches("call my_func()", "my");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_word_boundary_dots_are_boundaries() {
        // Dots are word boundaries, so "foo" should match in "self.foo"
        let matches = find_word_boundary_matches("self.foo.bar", "foo");
        assert_eq!(matches, vec![5]);
    }

    #[test]
    fn find_word_boundary_empty_pattern() {
        let matches = find_word_boundary_matches("hello", "");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_word_boundary_empty_text() {
        let matches = find_word_boundary_matches("", "foo");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_word_boundary_exact_match() {
        let matches = find_word_boundary_matches("foo", "foo");
        assert_eq!(matches, vec![0]);
    }

    #[test]
    fn find_word_boundary_parens_are_boundaries() {
        let matches = find_word_boundary_matches("call(foo)", "foo");
        assert_eq!(matches, vec![5]);
    }

    // ── rename: replace_word_boundary ───────────────────────────────

    #[test]
    fn replace_word_boundary_simple() {
        let result = replace_word_boundary("let foo = 42;", "foo", "bar");
        assert_eq!(result, "let bar = 42;");
    }

    #[test]
    fn replace_word_boundary_no_partial() {
        let result = replace_word_boundary("let foobar = 42;", "foo", "bar");
        assert_eq!(result, "let foobar = 42;"); // unchanged
    }

    #[test]
    fn replace_word_boundary_multiple() {
        let result = replace_word_boundary("foo + foo", "foo", "bar");
        assert_eq!(result, "bar + bar");
    }

    #[test]
    fn replace_word_boundary_empty_pattern() {
        let result = replace_word_boundary("hello", "", "bar");
        assert_eq!(result, "hello");
    }

    #[test]
    fn replace_word_boundary_no_matches() {
        let result = replace_word_boundary("nothing here", "foo", "bar");
        assert_eq!(result, "nothing here");
    }

    #[test]
    fn replace_word_boundary_with_longer_replacement() {
        let result = replace_word_boundary("fn f(x: T) -> T", "T", "MyType");
        assert_eq!(result, "fn f(x: MyType) -> MyType");
    }

    #[test]
    fn replace_word_boundary_with_shorter_replacement() {
        let result =
            replace_word_boundary("let my_variable = my_variable + 1;", "my_variable", "x");
        assert_eq!(result, "let x = x + 1;");
    }

    // ── rename: parse_rename_args ───────────────────────────────────

    #[test]
    fn parse_rename_args_valid() {
        let result = parse_rename_args("/rename foo bar");
        assert_eq!(result, Some(("foo".to_string(), "bar".to_string())));
    }

    #[test]
    fn parse_rename_args_no_args() {
        let result = parse_rename_args("/rename");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_rename_args_one_arg() {
        let result = parse_rename_args("/rename foo");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_rename_args_too_many_args() {
        let result = parse_rename_args("/rename foo bar baz");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_rename_args_extra_whitespace() {
        let result = parse_rename_args("/rename  foo   bar");
        assert_eq!(result, Some(("foo".to_string(), "bar".to_string())));
    }

    // ── rename: format_rename_preview ───────────────────────────────

    #[test]
    fn format_rename_preview_no_matches() {
        let preview = format_rename_preview(&[], "foo", "bar");
        assert!(preview.contains("No matches found"));
    }

    #[test]
    fn format_rename_preview_shows_file_and_line() {
        let matches = vec![RenameMatch {
            file: "src/main.rs".to_string(),
            line_num: 10,
            line_text: "let foo = 42;".to_string(),
            column: 4,
        }];
        let preview = format_rename_preview(&matches, "foo", "bar");
        assert!(preview.contains("src/main.rs"));
        assert!(preview.contains("10"));
        assert!(preview.contains("1 match"));
        assert!(preview.contains("1 file"));
    }

    #[test]
    fn format_rename_preview_multiple_files() {
        let matches = vec![
            RenameMatch {
                file: "a.rs".to_string(),
                line_num: 1,
                line_text: "use foo;".to_string(),
                column: 4,
            },
            RenameMatch {
                file: "b.rs".to_string(),
                line_num: 5,
                line_text: "foo()".to_string(),
                column: 0,
            },
        ];
        let preview = format_rename_preview(&matches, "foo", "bar");
        assert!(preview.contains("a.rs"));
        assert!(preview.contains("b.rs"));
        assert!(preview.contains("2 matches"));
        assert!(preview.contains("2 files"));
    }

    // ── rename: apply_rename with temp files ────────────────────────

    #[test]
    fn apply_rename_modifies_files() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "let foo = 1;\nlet bar = foo;\n").unwrap();

        let matches = vec![
            RenameMatch {
                file: file_path.to_str().unwrap().to_string(),
                line_num: 1,
                line_text: "let foo = 1;".to_string(),
                column: 4,
            },
            RenameMatch {
                file: file_path.to_str().unwrap().to_string(),
                line_num: 2,
                line_text: "let bar = foo;".to_string(),
                column: 10,
            },
        ];

        let count = apply_rename(&matches, "foo", "baz");
        assert_eq!(count, 2);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("let baz = 1;"));
        assert!(content.contains("let bar = baz;"));
        assert!(!content.contains("foo"));
    }

    #[test]
    fn apply_rename_preserves_non_matching_lines() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "// comment\nlet foo = 1;\n// end\n").unwrap();

        let matches = vec![RenameMatch {
            file: file_path.to_str().unwrap().to_string(),
            line_num: 2,
            line_text: "let foo = 1;".to_string(),
            column: 4,
        }];

        apply_rename(&matches, "foo", "bar");

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("// comment"));
        assert!(content.contains("let bar = 1;"));
        assert!(content.contains("// end"));
    }

    #[test]
    fn apply_rename_no_partial_replace() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "let foobar = foo;\n").unwrap();

        // Only match the standalone "foo", not "foobar"
        let matches = vec![RenameMatch {
            file: file_path.to_str().unwrap().to_string(),
            line_num: 1,
            line_text: "let foobar = foo;".to_string(),
            column: 13,
        }];

        apply_rename(&matches, "foo", "baz");

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("foobar")); // foobar unchanged
        assert!(content.contains("= baz;")); // standalone foo replaced
    }

    #[test]
    fn apply_rename_empty_matches() {
        let count = apply_rename(&[], "foo", "bar");
        assert_eq!(count, 0);
    }

    // ── /extract: parse_extract_args ─────────────────────────────────

    #[test]
    fn parse_extract_args_valid() {
        let result = parse_extract_args("/extract my_func src/lib.rs src/utils.rs");
        assert_eq!(
            result,
            Some((
                "my_func".to_string(),
                "src/lib.rs".to_string(),
                "src/utils.rs".to_string()
            ))
        );
    }

    #[test]
    fn parse_extract_args_missing_target() {
        assert_eq!(parse_extract_args("/extract my_func src/lib.rs"), None);
    }

    #[test]
    fn parse_extract_args_too_many() {
        assert_eq!(parse_extract_args("/extract a b c d"), None);
    }

    #[test]
    fn parse_extract_args_empty() {
        assert_eq!(parse_extract_args("/extract"), None);
    }

    // ── /extract: find_symbol_block ──────────────────────────────────

    #[test]
    fn find_symbol_block_simple_fn() {
        let source = "fn hello() {\n    println!(\"hi\");\n}\n";
        let result = find_symbol_block(source, "hello");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 2);
        assert!(block.contains("fn hello()"));
        assert!(block.contains("println!"));
    }

    #[test]
    fn find_symbol_block_pub_fn() {
        let source = "pub fn greet(name: &str) -> String {\n    format!(\"Hello {name}\")\n}\n";
        let result = find_symbol_block(source, "greet");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 2);
        assert!(block.contains("pub fn greet"));
    }

    #[test]
    fn find_symbol_block_struct() {
        let source = "pub struct MyPoint {\n    pub x: f64,\n    pub y: f64,\n}\n";
        let result = find_symbol_block(source, "MyPoint");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("pub struct MyPoint"));
        assert!(block.contains("pub x: f64"));
    }

    #[test]
    fn find_symbol_block_enum() {
        let source = "enum Color {\n    Red,\n    Green,\n    Blue,\n}\n";
        let result = find_symbol_block(source, "Color");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("enum Color"));
        assert!(block.contains("Blue"));
    }

    #[test]
    fn find_symbol_block_impl() {
        let source = "struct Foo;\n\nimpl Foo {\n    fn bar(&self) {}\n}\n";
        let result = find_symbol_block(source, "Foo");
        // Should find `struct Foo;` first (it's a unit struct)
        assert!(result.is_some());
        let (start, _end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert!(block.contains("struct Foo"));
    }

    #[test]
    fn find_symbol_block_with_doc_comments() {
        let source = "/// A helper function.\n/// Does something.\nfn helper() {\n    // body\n}\n";
        let result = find_symbol_block(source, "helper");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0); // doc comments included
        assert_eq!(end, 4);
        assert!(block.contains("/// A helper function."));
        assert!(block.contains("fn helper()"));
    }

    #[test]
    fn find_symbol_block_with_attributes() {
        let source = "#[derive(Debug)]\npub struct Config {\n    pub name: String,\n}\n";
        let result = find_symbol_block(source, "Config");
        assert!(result.is_some());
        let (start, _, block) = result.unwrap();
        assert_eq!(start, 0); // attribute included
        assert!(block.contains("#[derive(Debug)]"));
        assert!(block.contains("pub struct Config"));
    }

    #[test]
    fn find_symbol_block_not_found() {
        let source = "fn other() {\n}\n";
        assert!(find_symbol_block(source, "missing").is_none());
    }

    #[test]
    fn find_symbol_block_nested_braces() {
        let source = "fn complex() {\n    if true {\n        for i in 0..10 {\n            println!(\"{i}\");\n        }\n    }\n}\n";
        let result = find_symbol_block(source, "complex");
        assert!(result.is_some());
        let (start, end, _block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 6);
    }

    #[test]
    fn find_symbol_block_among_multiple() {
        let source = "fn first() {\n}\n\nfn second() {\n    let x = 1;\n}\n\nfn third() {\n}\n";
        let result = find_symbol_block(source, "second");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 3);
        assert_eq!(end, 5);
        assert!(block.contains("fn second()"));
        assert!(block.contains("let x = 1"));
    }

    #[test]
    fn find_symbol_block_unit_struct() {
        let source = "pub struct Unit;\n\nfn other() {}\n";
        let result = find_symbol_block(source, "Unit");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 0);
        assert!(block.contains("pub struct Unit;"));
    }

    #[test]
    fn find_symbol_block_trait() {
        let source = "pub trait Drawable {\n    fn draw(&self);\n}\n";
        let result = find_symbol_block(source, "Drawable");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("pub trait Drawable"));
        assert!(block.contains("fn draw"));
    }

    #[test]
    fn find_symbol_block_async_fn() {
        let source = "pub async fn fetch_data() {\n    // async body\n}\n";
        let result = find_symbol_block(source, "fetch_data");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("pub async fn fetch_data"));
    }

    #[test]
    fn find_symbol_block_no_partial_match() {
        let source = "fn my_func_extended() {\n}\n\nfn my_func() {\n    // target\n}\n";
        let result = find_symbol_block(source, "my_func");
        assert!(result.is_some());
        let (start, _, block) = result.unwrap();
        // Should match my_func, not my_func_extended
        assert_eq!(start, 3);
        assert!(block.contains("// target"));
    }

    // ── /extract: extract_symbol (integration) ──────────────────────

    #[test]
    fn extract_symbol_moves_function() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(
            &source,
            "fn keep_me() {\n    // stays\n}\n\npub fn move_me() {\n    // goes\n}\n\nfn also_stays() {\n}\n",
        )
        .unwrap();
        fs::write(&target, "// existing content\n").unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "move_me",
        );
        assert!(result.is_ok());

        let source_after = fs::read_to_string(&source).unwrap();
        assert!(source_after.contains("fn keep_me()"));
        assert!(source_after.contains("fn also_stays()"));
        assert!(!source_after.contains("fn move_me()"));

        let target_after = fs::read_to_string(&target).unwrap();
        assert!(target_after.contains("// existing content"));
        assert!(target_after.contains("pub fn move_me()"));
        assert!(target_after.contains("// goes"));
    }

    #[test]
    fn extract_symbol_creates_target_if_missing() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("new_file.rs");

        fs::write(&source, "fn movable() {\n    let x = 1;\n}\n").unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "movable",
        );
        assert!(result.is_ok());
        assert!(target.exists());

        let target_content = fs::read_to_string(&target).unwrap();
        assert!(target_content.contains("fn movable()"));
    }

    #[test]
    fn extract_symbol_not_found() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(&source, "fn other() {}\n").unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "missing",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn extract_symbol_source_not_found() {
        let dir = TempDir::new().unwrap();
        let result = extract_symbol(
            dir.path().join("nope.rs").to_str().unwrap(),
            dir.path().join("target.rs").to_str().unwrap(),
            "foo",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot read"));
    }

    #[test]
    fn extract_symbol_with_doc_comments_moves_docs() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(
            &source,
            "/// Important docs.\n/// More docs.\npub fn documented() {\n    // body\n}\n",
        )
        .unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "documented",
        );
        assert!(result.is_ok());

        let target_content = fs::read_to_string(&target).unwrap();
        assert!(target_content.contains("/// Important docs."));
        assert!(target_content.contains("/// More docs."));
        assert!(target_content.contains("pub fn documented()"));
    }

    #[test]
    fn extract_command_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/extract"),
            "/extract should be in KNOWN_COMMANDS"
        );
    }

    // ── /extract: find_symbol_block — type alias, const, static ─────

    #[test]
    fn find_symbol_block_type_alias() {
        let source = "pub type Result<T> = std::result::Result<T, MyError>;\n\nfn other() {}\n";
        let result = find_symbol_block(source, "Result");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 0);
        assert!(block.contains("pub type Result<T>"));
    }

    #[test]
    fn find_symbol_block_type_alias_simple() {
        let source = "type Callback = fn(u32) -> bool;\n";
        let result = find_symbol_block(source, "Callback");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 0);
        assert!(block.contains("type Callback"));
    }

    #[test]
    fn find_symbol_block_const() {
        let source = "pub const MAX_SIZE: usize = 1024;\n\nfn other() {}\n";
        let result = find_symbol_block(source, "MAX_SIZE");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 0);
        assert!(block.contains("pub const MAX_SIZE"));
    }

    #[test]
    fn find_symbol_block_const_with_doc() {
        let source = "/// The maximum buffer size.\nconst BUFFER_SIZE: usize = 512;\n";
        let result = find_symbol_block(source, "BUFFER_SIZE");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0); // doc comment included
        assert_eq!(end, 1);
        assert!(block.contains("/// The maximum buffer size."));
        assert!(block.contains("const BUFFER_SIZE"));
    }

    #[test]
    fn find_symbol_block_static() {
        let source = "static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);\n";
        let result = find_symbol_block(source, "COUNTER");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("static COUNTER"));
    }

    #[test]
    fn find_symbol_block_static_mut() {
        let source = "static mut GLOBAL: u32 = 0;\n\nfn other() {}\n";
        let result = find_symbol_block(source, "GLOBAL");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("static mut GLOBAL"));
    }

    #[test]
    fn find_symbol_block_pub_const_crate() {
        let source = "pub(crate) const INTERNAL_LIMIT: u32 = 100;\n";
        let result = find_symbol_block(source, "INTERNAL_LIMIT");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("pub(crate) const INTERNAL_LIMIT"));
    }

    #[test]
    fn find_symbol_block_const_multiline() {
        let source = "const ITEMS: &[&str] = &[\n    \"alpha\",\n    \"beta\",\n];\n";
        let result = find_symbol_block(source, "ITEMS");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 3);
        assert!(block.contains("const ITEMS"));
        assert!(block.contains("\"beta\""));
    }

    // ── /extract: extract_symbol with new types ─────────────────────

    #[test]
    fn extract_symbol_moves_type_alias() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(
            &source,
            "pub type MyResult<T> = Result<T, MyError>;\n\nfn keep() {}\n",
        )
        .unwrap();
        fs::write(&target, "// types\n").unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "MyResult",
        );
        assert!(result.is_ok());

        let source_after = fs::read_to_string(&source).unwrap();
        assert!(!source_after.contains("type MyResult"));
        assert!(source_after.contains("fn keep()"));

        let target_after = fs::read_to_string(&target).unwrap();
        assert!(target_after.contains("pub type MyResult<T>"));
    }

    #[test]
    fn extract_symbol_moves_const() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(&source, "pub const LIMIT: usize = 42;\n\nfn keep() {}\n").unwrap();
        fs::write(&target, "").unwrap();

        let result = extract_symbol(source.to_str().unwrap(), target.to_str().unwrap(), "LIMIT");
        assert!(result.is_ok());

        let source_after = fs::read_to_string(&source).unwrap();
        assert!(!source_after.contains("const LIMIT"));

        let target_after = fs::read_to_string(&target).unwrap();
        assert!(target_after.contains("pub const LIMIT: usize = 42;"));
    }

    #[test]
    fn extract_symbol_moves_static() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(
            &source,
            "pub static INSTANCE: &str = \"hello\";\n\nfn keep() {}\n",
        )
        .unwrap();
        fs::write(&target, "").unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "INSTANCE",
        );
        assert!(result.is_ok());

        let source_after = fs::read_to_string(&source).unwrap();
        assert!(!source_after.contains("static INSTANCE"));

        let target_after = fs::read_to_string(&target).unwrap();
        assert!(target_after.contains("pub static INSTANCE"));
    }

    // ── /move tests ──────────────────────────────────────────────────

    #[test]
    fn test_parse_move_args_basic() {
        let args = parse_move_args("/move MyStruct::process TargetStruct").unwrap();
        assert_eq!(args.source_type, "MyStruct");
        assert_eq!(args.method_name, "process");
        assert_eq!(args.target_type, "TargetStruct");
        assert!(args.target_file.is_none());
    }

    #[test]
    fn test_parse_move_args_cross_file() {
        let args = parse_move_args("/move Parser::parse_expr other.rs::Lexer").unwrap();
        assert_eq!(args.source_type, "Parser");
        assert_eq!(args.method_name, "parse_expr");
        assert_eq!(args.target_file.as_deref(), Some("other.rs"));
        assert_eq!(args.target_type, "Lexer");
    }

    #[test]
    fn test_parse_move_args_missing_method() {
        assert!(parse_move_args("/move MyStruct TargetStruct").is_none());
    }

    #[test]
    fn test_parse_move_args_empty() {
        assert!(parse_move_args("/move").is_none());
    }

    #[test]
    fn test_parse_move_args_too_many() {
        assert!(parse_move_args("/move A::b C D").is_none());
    }

    #[test]
    fn test_find_impl_blocks_single() {
        let src = "struct Foo;\n\nimpl Foo {\n    fn bar(&self) {}\n}\n";
        let blocks = find_impl_blocks(src, "Foo");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].2.contains("fn bar"));
    }

    #[test]
    fn test_find_impl_blocks_multiple() {
        let src = "\
struct Foo;

impl Foo {
    fn one(&self) {}
}

impl Foo {
    fn two(&self) {}
}
";
        let blocks = find_impl_blocks(src, "Foo");
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].2.contains("fn one"));
        assert!(blocks[1].2.contains("fn two"));
    }

    #[test]
    fn test_find_impl_blocks_not_found() {
        let src = "struct Foo;\nimpl Bar {\n    fn baz() {}\n}\n";
        let blocks = find_impl_blocks(src, "Foo");
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_find_method_in_impl_basic() {
        let impl_text = "impl Foo {\n    fn bar(&self) -> i32 {\n        42\n    }\n}";
        let result = find_method_in_impl(impl_text, "bar").unwrap();
        assert!(result.2.contains("fn bar"));
        assert!(result.2.contains("42"));
        // has_self_ref should be false (no self. usage, just &self param)
        assert!(!result.3);
    }

    #[test]
    fn test_find_method_in_impl_with_self_ref() {
        let impl_text = "impl Foo {\n    fn bar(&self) -> i32 {\n        self.value + 1\n    }\n}";
        let result = find_method_in_impl(impl_text, "bar").unwrap();
        assert!(result.3); // has_self_ref = true
    }

    #[test]
    fn test_find_method_in_impl_not_found() {
        let impl_text = "impl Foo {\n    fn bar(&self) {}\n}";
        assert!(find_method_in_impl(impl_text, "baz").is_none());
    }

    #[test]
    fn test_find_method_with_doc_comments() {
        let impl_text = "impl Foo {\n    /// Does something.\n    /// Multi-line doc.\n    fn documented(&self) {\n        // body\n    }\n}";
        let result = find_method_in_impl(impl_text, "documented").unwrap();
        assert!(result.2.contains("/// Does something."));
        assert!(result.2.contains("/// Multi-line doc."));
        assert!(result.2.contains("fn documented"));
    }

    #[test]
    fn test_find_method_with_attributes() {
        let impl_text =
            "impl Foo {\n    #[inline]\n    pub fn fast(&self) -> u32 {\n        0\n    }\n}";
        let result = find_method_in_impl(impl_text, "fast").unwrap();
        assert!(result.2.contains("#[inline]"));
        assert!(result.2.contains("pub fn fast"));
    }

    #[test]
    fn test_move_method_same_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("lib.rs");
        fs::write(
            &file,
            "\
struct Alpha;
struct Beta;

impl Alpha {
    fn greet(&self) -> &str {
        \"hello\"
    }

    fn farewell(&self) -> &str {
        \"bye\"
    }
}

impl Beta {
    fn existing(&self) {}
}
",
        )
        .unwrap();

        let result = move_method(file.to_str().unwrap(), "Alpha", "greet", None, "Beta");
        assert!(result.is_ok());
        let (summary, warning) = result.unwrap();
        assert!(summary.contains("greet"));
        assert!(summary.contains("Alpha"));
        assert!(summary.contains("Beta"));
        assert!(warning.is_none());

        let content = fs::read_to_string(&file).unwrap();
        // Method should be gone from Alpha
        assert!(!impl_block_contains(&content, "Alpha", "fn greet"));
        // farewell should still be in Alpha
        assert!(impl_block_contains(&content, "Alpha", "fn farewell"));
        // Method should be in Beta
        assert!(impl_block_contains(&content, "Beta", "fn greet"));
        // existing should still be in Beta
        assert!(impl_block_contains(&content, "Beta", "fn existing"));
    }

    #[test]
    fn test_move_method_cross_file() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(
            &source,
            "\
struct Src;

impl Src {
    fn compute(&self) -> i32 {
        42
    }
}
",
        )
        .unwrap();

        fs::write(
            &target,
            "\
struct Dst;

impl Dst {
    fn other(&self) {}
}
",
        )
        .unwrap();

        let result = move_method(
            source.to_str().unwrap(),
            "Src",
            "compute",
            Some(target.to_str().unwrap()),
            "Dst",
        );
        assert!(result.is_ok());

        let src_content = fs::read_to_string(&source).unwrap();
        assert!(!src_content.contains("fn compute"));

        let tgt_content = fs::read_to_string(&target).unwrap();
        assert!(tgt_content.contains("fn compute"));
        assert!(tgt_content.contains("42"));
        assert!(tgt_content.contains("fn other"));
    }

    #[test]
    fn test_move_method_with_doc_comments() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("lib.rs");
        fs::write(
            &file,
            "\
struct A;
struct B;

impl A {
    /// Important method.
    /// Does important things.
    fn important(&self) {
        // body
    }
}

impl B {
    fn placeholder(&self) {}
}
",
        )
        .unwrap();

        let result = move_method(file.to_str().unwrap(), "A", "important", None, "B");
        assert!(result.is_ok());

        let content = fs::read_to_string(&file).unwrap();
        // Doc comments should move with the method
        let b_block = extract_impl_block(&content, "B");
        assert!(b_block.contains("/// Important method."));
        assert!(b_block.contains("/// Does important things."));
        assert!(b_block.contains("fn important"));
    }

    #[test]
    fn test_move_method_not_found() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("lib.rs");
        fs::write(
            &file,
            "struct A;\nimpl A {\n    fn existing(&self) {}\n}\nstruct B;\nimpl B {}\n",
        )
        .unwrap();

        let result = move_method(file.to_str().unwrap(), "A", "nonexistent", None, "B");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_move_method_target_impl_not_found() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("lib.rs");
        fs::write(&file, "struct A;\nimpl A {\n    fn method(&self) {}\n}\n").unwrap();

        let result = move_method(file.to_str().unwrap(), "A", "method", None, "NonExistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No `impl NonExistent`"));
    }

    #[test]
    fn test_move_method_self_reference_warning() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("lib.rs");
        fs::write(
            &file,
            "\
struct A { value: i32 }
struct B;

impl A {
    fn get_value(&self) -> i32 {
        self.value
    }
}

impl B {
    fn other(&self) {}
}
",
        )
        .unwrap();

        let result = move_method(file.to_str().unwrap(), "A", "get_value", None, "B");
        assert!(result.is_ok());
        let (_summary, warning) = result.unwrap();
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("self."));
    }

    #[test]
    fn test_move_source_impl_not_found() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("lib.rs");
        fs::write(&file, "struct B;\nimpl B {\n    fn x(&self) {}\n}\n").unwrap();

        let result = move_method(file.to_str().unwrap(), "NonExistent", "method", None, "B");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No `impl NonExistent`"));
    }

    #[test]
    fn test_move_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/move"),
            "/move should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_move_in_help_text() {
        let text = help_text();
        assert!(text.contains("/move"), "/move should appear in help text");
    }

    #[test]
    fn test_reindent_method() {
        let method = "    fn foo(&self) {\n        42\n    }";
        let result = reindent_method(method, "        ");
        assert!(result.starts_with("        fn foo"));
        assert!(result.contains("            42"));
    }

    // Helper: check if an impl block for `type_name` contains `needle`
    fn impl_block_contains(source: &str, type_name: &str, needle: &str) -> bool {
        let blocks = find_impl_blocks(source, type_name);
        blocks.iter().any(|(_, _, text)| text.contains(needle))
    }

    // Helper: extract the text of the first impl block for a type
    fn extract_impl_block(source: &str, type_name: &str) -> String {
        let blocks = find_impl_blocks(source, type_name);
        if blocks.is_empty() {
            String::new()
        } else {
            blocks[0].2.clone()
        }
    }

    // ── rename_in_project ─────────────────────────────────────────────

    #[test]
    fn test_rename_in_project_empty_old_name() {
        let result = rename_in_project("", "Bar", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("old_name must not be empty"));
    }

    #[test]
    fn test_rename_in_project_empty_new_name() {
        let result = rename_in_project("Foo", "", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("new_name must not be empty"));
    }

    #[test]
    fn test_rename_in_project_same_name() {
        let result = rename_in_project("Foo", "Foo", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("identical"));
    }

    #[test]
    fn test_rename_result_fields() {
        let r = RenameResult {
            files_changed: vec!["a.rs".to_string()],
            total_replacements: 3,
            preview: "preview".to_string(),
        };
        assert_eq!(r.files_changed, vec!["a.rs"]);
        assert_eq!(r.total_replacements, 3);
        assert_eq!(r.preview, "preview");
    }

    #[test]
    fn test_rename_in_project_scoped_no_match() {
        // Scope to a nonexistent directory — should find no matches
        let result = rename_in_project("RenameMatch", "RM", Some("nonexistent_dir_xyz/"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No word-boundary matches"));
    }

    // ── /refactor tests ──────────────────────────────────────────────────

    #[test]
    fn test_refactor_no_args_shows_help() {
        // Calling handle_refactor with no args should not panic
        // and should print the refactoring tools summary
        handle_refactor("/refactor");
    }

    #[test]
    fn test_refactor_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/refactor"),
            "/refactor should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_refactor_help_exists() {
        use crate::help::command_help;
        assert!(
            command_help("refactor").is_some(),
            "/refactor should have a help entry"
        );
    }

    #[test]
    fn test_refactor_tab_completion() {
        use crate::commands::command_arg_completions;
        let candidates = command_arg_completions("/refactor", "");
        assert!(
            candidates.contains(&"rename".to_string()),
            "Should include 'rename'"
        );
        assert!(
            candidates.contains(&"extract".to_string()),
            "Should include 'extract'"
        );
        assert!(
            candidates.contains(&"move".to_string()),
            "Should include 'move'"
        );
    }

    #[test]
    fn test_refactor_tab_completion_filters() {
        use crate::commands::command_arg_completions;
        let candidates = command_arg_completions("/refactor", "re");
        assert!(
            candidates.contains(&"rename".to_string()),
            "Should include 'rename' for prefix 're'"
        );
        assert!(
            !candidates.contains(&"extract".to_string()),
            "Should not include 'extract' for prefix 're'"
        );
        assert!(
            !candidates.contains(&"move".to_string()),
            "Should not include 'move' for prefix 're'"
        );
    }

    #[test]
    fn test_refactor_unknown_subcommand() {
        // Should not panic on unknown subcommand
        handle_refactor("/refactor foobar");
    }

    #[test]
    fn test_refactor_in_help_text() {
        let help = help_text();
        assert!(
            help.contains("/refactor"),
            "/refactor should appear in help text"
        );
    }
}
