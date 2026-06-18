//! Rename symbol across project files — word-boundary-aware find-and-replace.

use crate::commands_search::is_binary_extension;
use crate::format::*;
use crate::git::run_git;

/// Check if a character is a word boundary character (not alphanumeric or underscore).
fn is_word_boundary_char(c: char) -> bool {
    !c.is_alphanumeric() && c != '_'
}

/// Check if position `pos` in `text` is at a word boundary start.
/// A word boundary exists at the start of the string or when the preceding char
/// is not a word character. Returns `false` if `pos` is not on a char boundary.
fn is_word_start(text: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    if !text.is_char_boundary(pos) {
        return false;
    }
    text[..pos].chars().last().is_none_or(is_word_boundary_char)
}

/// Check if position `pos` in `text` is at a word boundary end.
/// A word boundary exists at the end of the string or when the following char
/// is not a word character. Returns `false` if `pos` is not on a char boundary.
fn is_word_end(text: &str, pos: usize) -> bool {
    if pos >= text.len() {
        return true;
    }
    if !text.is_char_boundary(pos) {
        return false;
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

            // Advance past the match start — but ensure we land on a char boundary
            // to avoid panicking on text[start..] with multi-byte characters.
            start = abs_pos + 1;
            while start < text.len() && !text.is_char_boundary(start) {
                start += 1;
            }
        } else {
            break;
        }
    }

    positions
}

/// List files tracked by git (via `git ls-files`).
/// Falls back to walking the current directory if not in a git repo.
fn list_git_files() -> Vec<String> {
    match run_git(&["ls-files"]) {
        Ok(stdout) => stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect(),
        Err(_) => Vec::new(),
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
        // Safety: positions come from find() which returns char-boundary offsets,
        // and last_end = pos + old.len() is always at the end of a valid UTF-8 match.
        // Defensive check anyway to avoid panics on corrupted positions.
        if !text.is_char_boundary(pos) || !text.is_char_boundary(last_end) {
            continue;
        }
        result.push_str(&text[last_end..pos]);
        result.push_str(new);
        last_end = pos + old.len();
    }
    if text.is_char_boundary(last_end) {
        result.push_str(&text[last_end..]);
    }

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

#[cfg(test)]
mod tests {
    use super::*;
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

    // --- Multi-byte / Unicode safety tests ---

    #[test]
    fn find_word_boundary_with_multibyte_context() {
        // Pattern surrounded by multi-byte chars (✓ is 3 bytes)
        let text = "let ✓ foo ✓ bar";
        let matches = find_word_boundary_matches(text, "foo");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn find_word_boundary_multibyte_no_panic() {
        // Ensure no panic when text has multi-byte chars throughout
        let text = "café résumé naïve";
        let matches = find_word_boundary_matches(text, "résumé");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn find_word_boundary_multibyte_pattern_repeated() {
        // Pattern starting with multi-byte char, appearing twice at word boundaries.
        // Regression: start = abs_pos + 1 could land mid-char and panic.
        let text = "x é_thing y é_thing z";
        let matches = find_word_boundary_matches(text, "é_thing");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn find_word_boundary_multibyte_pattern_no_boundary() {
        // Multi-byte pattern NOT at word boundary — no match expected
        let text = "aé_thing bé_thing";
        let matches = find_word_boundary_matches(text, "é_thing");
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn find_word_boundary_empty_inputs() {
        assert!(find_word_boundary_matches("", "foo").is_empty());
        assert!(find_word_boundary_matches("foo", "").is_empty());
        assert!(find_word_boundary_matches("", "").is_empty());
    }

    #[test]
    fn replace_word_boundary_multibyte() {
        let text = "let ✓ foo ✓ bar";
        let result = replace_word_boundary(text, "foo", "baz");
        assert_eq!(result, "let ✓ baz ✓ bar");
    }

    #[test]
    fn replace_word_boundary_multibyte_pattern() {
        // Pattern itself contains multi-byte chars
        let text = "use café in code";
        let result = replace_word_boundary(text, "café", "coffee");
        assert_eq!(result, "use coffee in code");
    }

    #[test]
    fn is_word_start_end_at_boundaries() {
        // These functions should not panic on valid char boundary positions
        let text = "hello ✓ world";
        // Position 0 is always word start
        assert!(is_word_start(text, 0));
        // Position at text.len() is always word end
        assert!(is_word_end(text, text.len()));
    }
}
