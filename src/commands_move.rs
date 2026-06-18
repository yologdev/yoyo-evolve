//! Move methods between impl blocks — cross-file method relocation

use crate::format::*;
use crate::git::run_git;

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
                // SAFETY: pos from find() on an ASCII pattern ("impl ") — the match
                // starts at a char boundary, so pos is always a valid char boundary.
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
                // SAFETY: p from find("impl ") — "impl " is ASCII, so the match
                // position is always a valid char boundary.
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
            // SAFETY: fn_pattern is "fn <name>" — all ASCII, so pos (the start of the
            // match) and after (pos + fn_pattern.len()) are always valid char boundaries.
            let after = pos + fn_pattern.len();
            let is_word_char_after = after < trimmed.len()
                && trimmed[after..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            if is_word_char_after {
                continue;
            }
            // Check valid prefix (pub, pub(crate), async, etc.)
            // SAFETY: pos from find(&fn_pattern) where fn_pattern is ASCII ("fn <name>"),
            // so pos is always a valid char boundary.
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
        // SAFETY: indent_len is the count of leading whitespace bytes. Since
        // trim_start() only removes ASCII whitespace (space, tab, etc.), the
        // difference is always a valid char boundary. The is_char_boundary()
        // check below is a belt-and-suspenders guard.
        if sample_line.is_char_boundary(indent_len) {
            &sample_line[..indent_len]
        } else {
            "    "
        }
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
                // SAFETY: min_indent is the minimum leading whitespace length across
                // all non-empty lines. Since Rust's trim_start() removes only ASCII
                // whitespace, min_indent always falls on a char boundary. The
                // is_char_boundary() check is a belt-and-suspenders guard.
                let stripped = if line.len() >= min_indent && line.is_char_boundary(min_indent) {
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
    let file_list = run_git(&["ls-files", "--cached", "--others", "--exclude-standard"]).ok()?;
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
    fn test_parse_move_args_unicode_method_name() {
        // Method and type names can contain multi-byte UTF-8 characters.
        // The "::" splitting must not panic on such names.
        let args = parse_move_args("/move Type::método Target").unwrap();
        assert_eq!(args.source_type, "Type");
        assert_eq!(args.method_name, "método");
        assert_eq!(args.target_type, "Target");
        assert!(args.target_file.is_none());

        // Unicode in both type and method
        let args2 = parse_move_args("/move Données::résumé Cible").unwrap();
        assert_eq!(args2.source_type, "Données");
        assert_eq!(args2.method_name, "résumé");
        assert_eq!(args2.target_type, "Cible");
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

    #[test]
    fn reindent_method_multibyte() {
        let method = "    fn foo() {\n        println!(\"café ✓\");\n    }";
        let result = reindent_method(method, "        ");
        assert!(result.contains("fn foo()"));
        assert!(result.contains("café ✓"));
    }

    #[test]
    fn reindent_method_empty() {
        assert_eq!(reindent_method("", "    "), "");
    }

    #[test]
    fn find_impl_blocks_multibyte_content() {
        let source = r#"
/// A struct with café
impl MyStruct {
    fn method(&self) -> String {
        "résumé ✓".to_string()
    }
}
"#;
        let blocks = find_impl_blocks(source, "MyStruct");
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn find_method_in_impl_multibyte() {
        let impl_text = r#"impl MyStruct {
    /// Returns a café string
    fn get_cafe(&self) -> String {
        "café ✓".to_string()
    }
}"#;
        let result = find_method_in_impl(impl_text, "get_cafe");
        assert!(result.is_some());
    }
}
