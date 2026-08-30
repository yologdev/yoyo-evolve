//! Refactoring command handlers: /extract, /refactor routing hub.

use crate::commands_move::handle_move;
use crate::commands_rename::handle_rename;
use crate::format::*;
use crate::session::SessionChanges;

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

/// True for characters that can continue a Rust identifier.
fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// If `chars[start]` is a `'` that opens a *closed* char literal, return how many
/// chars the literal occupies (including both quotes). Returns `None` for a lone `'`
/// that is really a lifetime (`&'a str`), which must be treated as an ordinary char.
///
/// Recognised: `'x'`, `'\n'`-style one-char escapes, and `'\u{7d}'` unicode escapes.
///
/// Two consumers, deliberately sharing one scanner: [`significant_braces`] here (so a
/// brace inside a char literal is not structural, #770) and
/// `format::highlight::highlight_code_line` (so a lifetime tick does not open a string
/// literal and swallow the rest of the line, #759). A second copy of this rule is how
/// #759 outlived #770 by a day — keep it one implementation with one table test.
pub(crate) fn char_literal_len(chars: &[char], start: usize) -> Option<usize> {
    let mut j = start + 1;
    let first = *chars.get(j)?;
    if first == '\'' {
        // `''` is not a char literal.
        return None;
    }
    if first == '\\' {
        j += 1;
        let esc = *chars.get(j)?;
        if esc == 'u' && chars.get(j + 1) == Some(&'{') {
            j += 2;
            while j < chars.len() && chars[j] != '}' {
                j += 1;
            }
            chars.get(j)?; // unterminated `\u{`
            j += 1;
        } else {
            j += 1;
        }
    } else {
        j += 1;
    }
    if chars.get(j) == Some(&'\'') {
        Some(j + 1 - start)
    } else {
        None
    }
}

/// If a raw-string literal opens at `chars[start]` (an `r`, optionally preceded by a
/// `b` byte-string prefix), return `(hash_count, index_just_after_the_opening_quote)`.
///
/// `pub(crate)` because it has a second consumer: `format::highlight::scan_block_comments`
/// needs the same opener rule to carry a raw string across lines (#806). A second copy of
/// this rule is exactly how #759 outlived #770 by a day — one implementation, one table.
pub(crate) fn raw_string_open(chars: &[char], start: usize) -> Option<(usize, usize)> {
    if chars.get(start) != Some(&'r') {
        return None;
    }
    // The `r` must begin a token: either it is the first char, or the char before it
    // is not an identifier char, or it is a `b` prefix that itself begins a token.
    let prev_ok = match start.checked_sub(1) {
        None => true,
        Some(p) => {
            !is_ident_char(chars[p])
                || (chars[p] == 'b' && p.checked_sub(1).is_none_or(|q| !is_ident_char(chars[q])))
        }
    };
    if !prev_ok {
        return None;
    }
    let mut j = start + 1;
    let mut hashes = 0usize;
    while chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    if chars.get(j) == Some(&'"') {
        Some((hashes, j + 1))
    } else {
        None
    }
}

/// The delimiter that closes a string literal which was still open at end of line.
///
/// The `b` byte prefix (`b"…"`, `br#"…"#`) does not change the closer, so it needs no
/// variant of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringDelim {
    /// A plain `"…"` string: closed by a `"` that is not backslash-escaped.
    Normal,
    /// A raw string `r"…"` / `r#"…"#`: closed by a `"` followed by exactly N `#`.
    /// Raw strings have no escapes, so a `\` before the quote does not protect it.
    Raw(usize),
    /// A backtick-delimited literal that may span lines: a JS/TS template literal
    /// (`` `…` ``) or a Go raw string. Closed by the next `` ` ``. `escapes` is true for
    /// JS (a `` \` `` does not close) and false for Go, whose raw strings have no escapes
    /// at all — the one place the two shapes actually differ.
    ///
    /// Produced only by `format::highlight` (#806). [`significant_braces`] is a Rust-only
    /// scanner and never sets it; it lives here because the *closer* rule belongs beside
    /// its two siblings rather than in a second scanner.
    Backtick { escapes: bool },
    /// A Python triple-quoted literal (`"""…"""` or `'''…'''`), which spans lines and
    /// honours `\` escapes, so a `\"""` does not close it. `quote` carries which of the
    /// two delimiters opened it, because `'''` must not be closed by `"""`.
    ///
    /// Produced only by `format::highlight` (#865), for the same reason as [`Backtick`]:
    /// Rust has no triple-quoted literal, so [`significant_braces`] can never open one.
    /// The *closer* rule lives here beside its three siblings rather than in a second
    /// scanner — two copies of a delimiter rule is how #759 outlived #770 by a day.
    ///
    /// [`Backtick`]: StringDelim::Backtick
    TripleQuote { quote: char },
}

/// The facts [`significant_braces`] must carry from one line to the next.
///
/// Both fields exist because Rust lets the two constructs span lines: `/* … */` block
/// comments (#771 item 1) and string literals (#771 item 2). They are one struct rather
/// than two out-parameters so a new carried fact does not churn every call site again.
#[derive(Debug, Default, Clone)]
pub(crate) struct BraceScanState {
    /// `/* … */` nesting depth. `0` means "not in a comment"; a stray `*/` at depth 0 is
    /// ignored rather than underflowing.
    pub(crate) block_comment_depth: usize,
    /// `Some(delim)` when a string literal opened on an earlier line and has not closed.
    pub(crate) open_string: Option<StringDelim>,
}

/// Scan for the closer of an open string starting at `from`, returning the index just
/// past it, or `None` when the closer is not on this line.
///
/// `pub(crate)` for the same reason as [`raw_string_open`]: `format::highlight` carries
/// the same three delimiters across lines (#806) and must close them by the same rule.
///
/// Never indexes a `&str` by byte — the caller hands over a `Vec<char>` slice.
pub(crate) fn close_open_string(chars: &[char], from: usize, delim: StringDelim) -> Option<usize> {
    let mut j = from;
    match delim {
        StringDelim::Normal => {
            while j < chars.len() {
                if chars[j] == '\\' {
                    // Escape: skip the escaped char. A trailing `\` escapes the newline,
                    // which just means the string is still open — j runs off the end.
                    j += 2;
                } else if chars[j] == '"' {
                    return Some(j + 1);
                } else {
                    j += 1;
                }
            }
            None
        }
        StringDelim::Raw(hashes) => {
            while j < chars.len() {
                if chars[j] == '"' && (1..=hashes).all(|k| chars.get(j + k) == Some(&'#')) {
                    return Some(j + 1 + hashes);
                }
                j += 1;
            }
            None
        }
        StringDelim::Backtick { escapes } => {
            while j < chars.len() {
                if escapes && chars[j] == '\\' {
                    // JS: `\` escapes the next char, so a `\`` does not close.
                    j += 2;
                } else if chars[j] == '`' {
                    return Some(j + 1);
                } else {
                    j += 1;
                }
            }
            None
        }
        StringDelim::TripleQuote { quote } => {
            while j < chars.len() {
                if chars[j] == '\\' {
                    // Python honours escapes inside a triple-quoted literal, so a
                    // `\"""` does not close it. A trailing `\` escapes the newline,
                    // which just means the literal is still open — j runs off the end.
                    j += 2;
                } else if chars[j] == quote
                    && chars.get(j + 1) == Some(&quote)
                    && chars.get(j + 2) == Some(&quote)
                {
                    return Some(j + 3);
                } else {
                    j += 1;
                }
            }
            None
        }
    }
}

/// Return the structurally significant `{` / `}` characters of `line`, in order,
/// skipping any that appear inside string literals, char literals, or comments.
///
/// `state` carries the two facts that can span lines and is updated in place: the
/// `/* … */` nesting **depth** (a depth, not a flag, because Rust block comments nest:
/// `/* /* */ */` is one comment and the first `*/` closes only the innermost `/*`,
/// #771 item 1) and the delimiter of a string literal left open at end of line
/// (#771 item 2). Braces are returned **in order** rather than as an open/close count,
/// because a line like `} fn other() {` both closes a block and opens one and the
/// caller's depth machine must see those in sequence.
///
/// Handles: `"…"` strings with `\` escapes, raw strings `r"…"` / `r#"…"#` (any hash
/// count, plus the `b` byte prefix), `'x'` char literals *without* mistaking a lifetime
/// `&'a str` for one, `//` line comments, `/* … */` block comments including multi-line
/// and nested ones (#771 item 1), and — since #771 item 2 — string literals that **span
/// lines**, both plain and raw-with-N-hashes: every brace between the opening delimiter
/// and its closer is inert however many lines apart they are, and ordinary scanning
/// resumes on the same line just past the closer.
///
/// Still **not** handled, stated rather than papered over: this is a brace scanner, not a
/// Rust lexer. It does not know about `#[cfg]`-disabled code, macro token trees with
/// unbalanced braces, or nested `{}` inside a format-string's `{…}` argument capture —
/// all of those still count as ordinary code.
///
/// Never indexes a `&str` by byte — it walks a `Vec<char>`, so multi-byte input is safe.
///
/// `pub(crate)` because it has a second consumer: `find_impl_blocks` and
/// `find_method_in_impl` in `commands_move.rs` used to count braces with no
/// string/comment state at all, which is the same data-corruption class through
/// `/move` that #770 fixed here (#771 item 3). One scanner, one set of table tests.
pub(crate) fn significant_braces(line: &str, state: &mut BraceScanState) -> Vec<char> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;

    // A string opened on an earlier line: find its closer before anything else, because
    // until then every character on this line — braces included — is string content.
    if let Some(delim) = state.open_string {
        match close_open_string(&chars, 0, delim) {
            Some(after) => {
                state.open_string = None;
                i = after;
            }
            None => return out,
        }
    }

    while i < chars.len() {
        let c = chars[i];

        if state.block_comment_depth > 0 {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                state.block_comment_depth -= 1;
                i += 2;
            } else if c == '/' && chars.get(i + 1) == Some(&'*') {
                // Rust block comments nest — this opens an inner one (#771 item 1).
                state.block_comment_depth += 1;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        if c == '/' && chars.get(i + 1) == Some(&'/') {
            // Line comment: nothing after this matters on this line.
            break;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            state.block_comment_depth = 1;
            i += 2;
            continue;
        }

        if let Some((hashes, j)) = raw_string_open(&chars, i) {
            match close_open_string(&chars, j, StringDelim::Raw(hashes)) {
                Some(after) => {
                    i = after;
                    continue;
                }
                None => {
                    // Spans lines (#771 item 2): remember the closer we owe.
                    state.open_string = Some(StringDelim::Raw(hashes));
                    break;
                }
            }
        }

        if c == '"' {
            match close_open_string(&chars, i + 1, StringDelim::Normal) {
                Some(after) => {
                    i = after;
                    continue;
                }
                None => {
                    state.open_string = Some(StringDelim::Normal);
                    break;
                }
            }
        }

        if c == '\'' {
            if let Some(len) = char_literal_len(&chars, i) {
                i += len;
            } else {
                // A lifetime (`&'a str`) — ordinary character, keep going.
                i += 1;
            }
            continue;
        }

        if c == '{' || c == '}' {
            out.push(c);
        }
        i += 1;
    }

    out
}

/// Find a top-level symbol block (fn, struct, enum, impl, trait, type, const, static) in source text.
/// Returns `(start_line_0indexed, end_line_0indexed, block_text)` where the range
/// is inclusive on both ends.
///
/// Uses brace-depth tracking: finds the line where the symbol keyword + name appear,
/// then scans backwards to collect any `#[...]` attributes or `///` doc comments
/// immediately above, then scans forward counting the *structurally significant* `{`
/// and `}` reported by [`significant_braces`] (so braces inside strings, char literals
/// and comments are ignored) until depth returns to 0.
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
                    || trimmed[after..]
                        .chars()
                        .next()
                        .is_some_and(|c| !c.is_ascii_alphanumeric() && c != '_')
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

    // Scan forward with brace-depth tracking. Braces inside string/char literals and
    // comments are not structural — see `significant_braces` (#770: a body containing
    // `println!("}")` used to end the block early and `/extract` then deleted a
    // truncated range from the source, corrupting both files).
    let mut depth: i32 = 0;
    let mut found_open = false;
    let mut end_line = decl_line;
    // Per scan, not per line: a `/* … */` or a string literal may span lines.
    let mut scan = BraceScanState::default();

    for (i, line) in lines.iter().enumerate().skip(decl_line) {
        for ch in significant_braces(line, &mut scan) {
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

    // Create the target's parent directory if it is missing — and do it BEFORE
    // touching the source, because the source write below happens first: a bad
    // target path used to delete the symbol from the source and write it nowhere.
    if let Some(parent) = std::path::Path::new(target_path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!("Cannot create target directory '{}': {e}", parent.display())
            })?;
        }
    }

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
pub fn handle_refactor(input: &str, changes: &SessionChanges) {
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
            handle_rename(&forwarded, changes);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::KNOWN_COMMANDS;
    use crate::help::help_text;
    use std::fs;
    use tempfile::TempDir;

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

    /// Round 55 (h3): `/extract`'s help promises "Creates the target file if it
    /// doesn't exist", but the write path only called `fs::write`, which does not
    /// create missing *parent directories*. Asserted at the emission point — the
    /// `Result` a caller actually receives — plus the two files' final state,
    /// because the pre-fix failure was not merely an error: the source write ran
    /// first, so the symbol was deleted from the source and then written nowhere.
    #[test]
    fn extract_symbol_creates_missing_target_parent_dir() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        // Two levels deep, neither existing — the pre-fix path failed here.
        let target = dir
            .path()
            .join("newdir")
            .join("nested")
            .join("constants.rs");

        fs::write(&source, "pub const MAX: usize = 7;\n\nfn keep() {}\n").unwrap();
        assert!(!target.parent().unwrap().exists());

        let result = extract_symbol(source.to_str().unwrap(), target.to_str().unwrap(), "MAX");

        // Emission point: the caller gets Ok, not a raw OS "No such file or
        // directory" error arriving after it already confirmed the move.
        assert!(result.is_ok(), "expected Ok, got {result:?}");

        // The symbol actually landed in the target...
        let target_after = fs::read_to_string(&target).unwrap();
        assert!(target_after.contains("pub const MAX: usize = 7;"));

        // ...and the source is not left mutilated (symbol removed, written nowhere).
        let source_after = fs::read_to_string(&source).unwrap();
        assert!(!source_after.contains("const MAX"));
        assert!(source_after.contains("fn keep()"));
    }

    /// A target in the current directory has an empty parent path; the guard must
    /// pass it through rather than trying to create "".
    #[test]
    fn extract_symbol_bare_target_filename_still_works() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(&source, "pub const ONE: usize = 1;\n\nfn keep() {}\n").unwrap();

        let result = extract_symbol(source.to_str().unwrap(), target.to_str().unwrap(), "ONE");
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert!(fs::read_to_string(&target)
            .unwrap()
            .contains("pub const ONE: usize = 1;"));
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

    // ── /refactor tests ──────────────────────────────────────────────────

    #[test]
    fn test_refactor_no_args_shows_help() {
        // Calling handle_refactor with no args should not panic
        // and should print the refactoring tools summary
        handle_refactor("/refactor", &SessionChanges::new());
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
        handle_refactor("/refactor foobar", &SessionChanges::new());
    }

    #[test]
    fn test_refactor_in_help_text() {
        let help = help_text();
        assert!(
            help.contains("/refactor"),
            "/refactor should appear in help text"
        );
    }

    #[test]
    fn find_symbol_block_multibyte_comments() {
        // Source with multi-byte chars in comments shouldn't panic
        let source = r#"
/// Process café data — résumé handler
fn process_data() {
    println!("✓ done");
}
"#;
        let result = find_symbol_block(source, "process_data");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("fn process_data"));
    }

    // --- #770: braces inside strings / chars / comments are not structural ---

    #[test]
    fn significant_braces_table() {
        // (line, incoming block-comment depth) -> (expected braces, outgoing depth)
        let cases: Vec<(&str, usize, Vec<char>, usize)> = vec![
            // plain control flow is untouched
            ("if x { y } else {", 0, vec!['{', '}', '{'], 0),
            // braces in a string literal
            (r#"    println!("}");"#, 0, vec![], 0),
            (r#"    println!("{");"#, 0, vec![], 0),
            // escaped quote does not end the string, so the `}` stays inside it
            (r#"    let s = "\"}";"#, 0, vec![], 0),
            // trailing line comment
            ("    let x = 1; // }", 0, vec![], 0),
            ("    } // { not real", 0, vec!['}'], 0),
            // block comment opening and closing on one line
            ("    /* } */ let y = 2; {", 0, vec!['{'], 0),
            // block comment spanning lines
            ("    /* start {", 0, vec![], 1),
            ("       still } inside", 1, vec![], 1),
            ("       end */ }", 1, vec!['}'], 0),
            // #771 item 1: Rust block comments NEST. The first `*/` closes only the
            // innermost one, so the trailing `}` is still commented out.
            ("    /* /* } */ } */ {", 0, vec!['{'], 0),
            ("    /* outer /* inner */ still } inside", 0, vec![], 1),
            ("    /* a /* b", 0, vec![], 2),
            ("       still } commented */ }", 2, vec![], 1),
            ("       out */ }", 1, vec!['}'], 0),
            // an unbalanced `*/` must not underflow the depth
            ("    */ }", 0, vec!['}'], 0),
            // a `/*` inside a string does not open a comment
            (r#"    let s = "/*"; {"#, 0, vec!['{'], 0),
            // lifetimes must NOT be read as char literals (#759's trap)
            ("fn f<'a>(x: &'a str) -> &'a str {", 0, vec!['{'], 0),
            // real char literals are skipped
            ("    let c = '{';", 0, vec![], 0),
            (r"    let c = '\'';", 0, vec![], 0),
            (r"    let c = '\u{7d}'; {", 0, vec!['{'], 0),
            // raw strings, with and without hashes
            (r##"    let s = r#"}"#; {"##, 0, vec!['{'], 0),
            (r#"    let s = r"}";"#, 0, vec![], 0),
            (r##"    let s = br#"{"#;"##, 0, vec![], 0),
            // `r` that is merely the tail of an identifier is not a raw string
            (r#"    let ptr = "x"; {"#, 0, vec!['{'], 0),
            // multi-byte input must not panic
            ("    // café — résumé }", 0, vec![], 0),
            ("    let s = \"✓}\"; {", 0, vec!['{'], 0),
        ];

        for (line, mut depth, expected, expected_state) in cases {
            let mut state = BraceScanState {
                block_comment_depth: depth,
                open_string: None,
            };
            let got = significant_braces(line, &mut state);
            assert_eq!(got, expected, "braces for line: {line:?}");
            depth = state.block_comment_depth;
            assert_eq!(depth, expected_state, "block-comment state after: {line:?}");
            assert!(
                state.open_string.is_none(),
                "no string should still be open after: {line:?}"
            );
        }
    }

    // --- #771 item 2: string literals that span lines ---

    #[test]
    fn significant_braces_multiline_string_table() {
        // Each case is a *sequence* of lines run through one carried state, because the
        // defect only exists across lines: (label, lines, expected braces per line).
        type Case = (&'static str, Vec<&'static str>, Vec<Vec<char>>);
        let cases: Vec<Case> = vec![
            (
                "plain multi-line string: braces inert until the closer",
                vec![
                    r#"    let s = "open {"#,
                    "    middle }",
                    r#"    end";"#,
                    "    fn after() {",
                ],
                vec![vec![], vec![], vec![], vec!['{']],
            ),
            (
                "raw multi-line string with one hash",
                vec![
                    r##"    let s = r#"{ } ""##,
                    "    still inside",
                    r##"    "#;"##,
                ],
                vec![vec![], vec![], vec![]],
            ),
            (
                "raw multi-line string with two hashes: a lone `\"#` does not close it",
                vec![
                    r###"    let s = r##"}"###,
                    r###"    "# still inside {"###,
                    r###"    "##; }"###,
                ],
                vec![vec![], vec![], vec!['}']],
            ),
            (
                "regression: a string closing on the same line behaves exactly as before",
                vec![r#"    let s = "}"; {"#, "    }"],
                vec![vec!['{'], vec!['}']],
            ),
            (
                "resume guard: a `}` after the closer on the closing line is emitted",
                vec![r#"    let s = "open"#, r#"    end"; }"#],
                vec![vec![], vec!['}']],
            ),
            (
                "an escaped quote on a continuation line does not close the string",
                vec![
                    r#"    let s = "open"#,
                    r#"    still \" inside {"#,
                    r#"    end"; }"#,
                ],
                vec![vec![], vec![], vec!['}']],
            ),
            (
                "a trailing backslash escapes the newline: still open",
                vec![
                    r#"    let s = "open \"#,
                    "    } still inside",
                    r#"    end";"#,
                ],
                vec![vec![], vec![], vec![]],
            ),
            (
                "a brace after a multi-line string on the opening line's own tail",
                vec![r#"    let s = "a"; if x { let t = "open"#, r#"    end"; }"#],
                vec![vec!['{'], vec!['}']],
            ),
        ];

        for (label, lines, expected) in cases {
            let mut state = BraceScanState::default();
            let got: Vec<Vec<char>> = lines
                .iter()
                .map(|l| significant_braces(l, &mut state))
                .collect();
            assert_eq!(got, expected, "{label}: lines {lines:?}");
        }
    }

    #[test]
    fn find_symbol_block_ignores_brace_in_multiline_string() {
        // The `}` on its own line is string content, not the end of the fn.
        let source = "fn tricky4() {\n    let s = \"open\n}\nend\";\n    let x = 1;\n}\n";
        let (start, end, block) = find_symbol_block(source, "tricky4").unwrap();
        assert_eq!((start, end), (0, 5), "block should span the whole fn");
        assert!(block.contains("let x = 1;"), "block: {block:?}");
    }

    #[test]
    fn find_symbol_block_open_brace_in_multiline_string_does_not_swallow_next_fn() {
        let source =
            "fn tricky5() {\n    let s = \"open\n{\nend\";\n    let x = 1;\n}\n\nfn after() {}\n";
        let (start, end, block) = find_symbol_block(source, "tricky5").unwrap();
        assert_eq!((start, end), (0, 5));
        assert!(
            !block.contains("fn after"),
            "must not swallow the following item: {block:?}"
        );
    }

    #[test]
    fn find_symbol_block_ignores_brace_in_string() {
        // Previously returned Some((0, 1, "fn tricky() {")) — two lines short.
        let source = "fn tricky() {\n    println!(\"}\");\n    let x = 1;\n}\n";
        let (start, end, block) = find_symbol_block(source, "tricky").unwrap();
        assert_eq!((start, end), (0, 3), "block should span the whole fn");
        assert!(
            block.ends_with('}'),
            "block should end with the closing brace"
        );
    }

    #[test]
    fn find_symbol_block_ignores_brace_in_line_comment() {
        let source = "fn tricky3() {\n    let x = 1; // }\n}\n";
        let (start, end, _) = find_symbol_block(source, "tricky3").unwrap();
        assert_eq!((start, end), (0, 2));
    }

    #[test]
    fn extract_symbol_moves_whole_symbol_with_brace_in_multiline_string() {
        // Emission-point test for #771 item 2: the user receives two files, so assert on
        // both. Before the carried string state, the bare `}` line inside the literal
        // ended the block and `/extract` wrote a truncated fn to the target while
        // leaving the tail behind in the source — both files wrong.
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src.rs");
        let dst = dir.path().join("dst.rs");
        std::fs::write(
            &src,
            "fn keep() {}\n\nfn tricky4() {\n    let s = \"open\n}\nend\";\n    let x = 1;\n}\n",
        )
        .unwrap();

        let res = extract_symbol(src.to_str().unwrap(), dst.to_str().unwrap(), "tricky4");
        assert!(res.is_ok(), "extract failed: {res:?}");

        let target = std::fs::read_to_string(&dst).unwrap();
        assert!(target.contains("fn tricky4()"), "target: {target:?}");
        assert!(target.contains("let x = 1;"), "target: {target:?}");
        assert!(
            target.trim_end().ends_with('}'),
            "target must include the closing brace: {target:?}"
        );

        let remaining = std::fs::read_to_string(&src).unwrap();
        assert!(
            !remaining.contains("fn tricky4"),
            "source still has the symbol: {remaining:?}"
        );
        assert!(
            !remaining.contains("let x = 1;"),
            "source kept part of the moved fn: {remaining:?}"
        );
        assert!(remaining.contains("fn keep()"), "source: {remaining:?}");
        assert_eq!(
            remaining.matches('{').count(),
            remaining.matches('}').count(),
            "source left with unbalanced braces: {remaining:?}"
        );
    }

    #[test]
    fn extract_symbol_open_brace_in_multiline_string_leaves_following_fn_in_source() {
        // The over-long direction: a `{` inside a multi-line string must not make the
        // block run on and swallow the next item into the target.
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src.rs");
        let dst = dir.path().join("dst.rs");
        std::fs::write(
            &src,
            "fn tricky5() {\n    let s = \"open\n{\nend\";\n    let x = 1;\n}\n\nfn after() {}\n",
        )
        .unwrap();

        let res = extract_symbol(src.to_str().unwrap(), dst.to_str().unwrap(), "tricky5");
        assert!(res.is_ok(), "extract failed: {res:?}");

        let remaining = std::fs::read_to_string(&src).unwrap();
        assert!(
            remaining.contains("fn after() {}"),
            "following item was swallowed out of the source: {remaining:?}"
        );
        let target = std::fs::read_to_string(&dst).unwrap();
        assert!(
            !target.contains("fn after"),
            "target swallowed the following item: {target:?}"
        );
        assert!(target.contains("let x = 1;"), "target: {target:?}");
    }

    #[test]
    fn find_symbol_block_open_brace_in_string_does_not_swallow_next_fn() {
        // Previously returned Some((0, 5, …)) — swallowed `fn after()`.
        let source = "fn tricky2() {\n    println!(\"{\");\n    let x = 1;\n}\n\nfn after() {}\n";
        let (start, end, block) = find_symbol_block(source, "tricky2").unwrap();
        assert_eq!((start, end), (0, 3));
        assert!(
            !block.contains("fn after"),
            "must not swallow the following item: {block:?}"
        );
    }

    #[test]
    fn extract_symbol_moves_whole_symbol_with_brace_in_string() {
        // Emission-point test: the user receives two files, so assert on both files.
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src.rs");
        let dst = dir.path().join("dst.rs");
        std::fs::write(
            &src,
            "fn keep() {}\n\nfn tricky() {\n    println!(\"}\");\n    let x = 1;\n}\n",
        )
        .unwrap();

        let res = extract_symbol(src.to_str().unwrap(), dst.to_str().unwrap(), "tricky");
        assert!(res.is_ok(), "extract failed: {res:?}");

        let target = std::fs::read_to_string(&dst).unwrap();
        assert!(target.contains("fn tricky()"), "target: {target:?}");
        assert!(target.contains("let x = 1;"), "target: {target:?}");
        assert!(
            target.trim_end().ends_with('}'),
            "target must include the closing brace: {target:?}"
        );

        let remaining = std::fs::read_to_string(&src).unwrap();
        assert!(
            !remaining.contains("fn tricky"),
            "source still has the symbol: {remaining:?}"
        );
        assert!(remaining.contains("fn keep()"), "source: {remaining:?}");
        assert_eq!(
            remaining.matches('{').count(),
            remaining.matches('}').count(),
            "source left with unbalanced braces: {remaining:?}"
        );
    }

    #[test]
    fn extract_symbol_open_brace_in_string_leaves_following_fn_in_source() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src.rs");
        let dst = dir.path().join("dst.rs");
        std::fs::write(
            &src,
            "fn tricky2() {\n    println!(\"{\");\n    let x = 1;\n}\n\nfn after() {}\n",
        )
        .unwrap();

        let res = extract_symbol(src.to_str().unwrap(), dst.to_str().unwrap(), "tricky2");
        assert!(res.is_ok(), "extract failed: {res:?}");

        let remaining = std::fs::read_to_string(&src).unwrap();
        assert!(
            remaining.contains("fn after() {}"),
            "following item was swallowed out of the source: {remaining:?}"
        );
        let target = std::fs::read_to_string(&dst).unwrap();
        assert!(
            !target.contains("fn after"),
            "target must not carry the following item: {target:?}"
        );
    }
}
