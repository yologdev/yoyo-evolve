//! Syntax highlighting for code blocks (Rust, Python, JS, Go, etc.).

use super::*;

use crate::commands_refactor::{close_open_string, raw_string_open, StringDelim};

fn normalize_lang(lang: &str) -> Option<&'static str> {
    // A fence tag may carry attributes: ```rust,ignore or ```rust no_run. Match on the
    // head only. This is the single seam every caller reaches the language table
    // through, so fixing it here covers every door (markdown renderer, direct calls)
    // instead of one caller's parse.
    let head = lang
        .split([',', ' ', '\t'])
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    match head.as_str() {
        "rust" | "rs" => Some("rust"),
        "python" | "py" => Some("python"),
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" => Some("js"),
        "go" | "golang" => Some("go"),
        "sh" | "bash" | "shell" | "zsh" => Some("shell"),
        "c" | "cpp" | "c++" | "cc" | "cxx" | "h" | "hpp" => Some("c"),
        "json" | "jsonc" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "toml" => Some("toml"),
        _ => None,
    }
}

/// Does the `'` at `chars[i]` open a Rust char literal (rather than a lifetime tick)?
///
/// Delegates to [`crate::commands_refactor::char_literal_len`] — the *same* rule that
/// keeps a brace inside a char literal from counting as structural (#770). The
/// highlighter used to carry its own narrower copy of this discrimination; one scanner
/// with one table test is the point, because two copies is exactly how the lifetime bug
/// survived here after being fixed there.
///
/// Recognised as char literals: `'x'`, one-char escapes (`'\n'`, `'\\'`, `'\''`) and
/// unicode escapes (`'\u{7d}'`). Everything else (`'a`, `'a>`, `'static`, `'_`) is a
/// lifetime and is emitted as plain text, because guessing the other way produced #759.
fn is_rust_char_literal(chars: &[char], i: usize) -> bool {
    chars.get(i) == Some(&'\'') && crate::commands_refactor::char_literal_len(chars, i).is_some()
}

/// Get the keyword list for a normalized language.
fn lang_keywords(lang: &str) -> &'static [&'static str] {
    match lang {
        "rust" => &[
            "fn",
            "let",
            "mut",
            "if",
            "else",
            "for",
            "while",
            "loop",
            "match",
            "return",
            "use",
            "mod",
            "pub",
            "struct",
            "enum",
            "impl",
            "trait",
            "where",
            "async",
            "await",
            "move",
            "self",
            "super",
            "crate",
            "const",
            "static",
            "type",
            "as",
            "in",
            "ref",
            "true",
            "false",
            "Some",
            "None",
            "Ok",
            "Err",
            "unsafe",
            "dyn",
            "macro_rules",
        ],
        "python" => &[
            "def", "class", "if", "elif", "else", "for", "while", "return", "import", "from", "as",
            "with", "try", "except", "finally", "raise", "yield", "lambda", "pass", "break",
            "continue", "and", "or", "not", "in", "is", "None", "True", "False", "self", "async",
            "await", "del", "global", "nonlocal", "assert",
        ],
        "js" => &[
            "function",
            "const",
            "let",
            "var",
            "if",
            "else",
            "for",
            "while",
            "return",
            "import",
            "export",
            "from",
            "class",
            "new",
            "this",
            "async",
            "await",
            "try",
            "catch",
            "finally",
            "throw",
            "typeof",
            "instanceof",
            "true",
            "false",
            "null",
            "undefined",
            "switch",
            "case",
            "default",
            "break",
            "continue",
            "interface",
            "type",
            "enum",
            "of",
            "in",
            "yield",
            "delete",
            "void",
            "super",
            "extends",
            "implements",
            "static",
            "get",
            "set",
        ],
        "go" => &[
            "func",
            "var",
            "const",
            "if",
            "else",
            "for",
            "range",
            "return",
            "import",
            "package",
            "type",
            "struct",
            "interface",
            "map",
            "chan",
            "go",
            "defer",
            "select",
            "case",
            "switch",
            "default",
            "break",
            "continue",
            "nil",
            "true",
            "false",
            "fallthrough",
            "goto",
        ],
        "shell" => &[
            "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac",
            "function", "return", "exit", "echo", "export", "local", "readonly", "set", "unset",
            "in", "true", "false", "source", "alias", "cd", "test",
        ],
        "c" => &[
            "if",
            "else",
            "for",
            "while",
            "do",
            "switch",
            "case",
            "default",
            "break",
            "continue",
            "return",
            "goto",
            "struct",
            "union",
            "enum",
            "typedef",
            "sizeof",
            "static",
            "extern",
            "const",
            "volatile",
            "inline",
            "void",
            "int",
            "char",
            "float",
            "double",
            "long",
            "short",
            "unsigned",
            "signed",
            "auto",
            "register",
            "class",
            "public",
            "private",
            "protected",
            "virtual",
            "template",
            "namespace",
            "using",
            "new",
            "delete",
            "try",
            "catch",
            "throw",
            "nullptr",
            "true",
            "false",
            "bool",
            "include",
            "define",
            "ifdef",
            "ifndef",
            "endif",
            "pragma",
        ],
        "toml" | "yaml" => &["true", "false", "null", "yes", "no", "on", "off"],
        _ => &[],
    }
}

/// Get built-in type names for a normalized language (highlighted in magenta).
fn lang_types(lang: &str) -> &'static [&'static str] {
    match lang {
        "rust" => &[
            "String",
            "Vec",
            "Option",
            "Result",
            "Box",
            "Rc",
            "Arc",
            "HashMap",
            "HashSet",
            "BTreeMap",
            "BTreeSet",
            "VecDeque",
            "LinkedList",
            "BinaryHeap",
            "Cell",
            "RefCell",
            "Mutex",
            "RwLock",
            "Cow",
            "Pin",
            "PhantomData",
            "i8",
            "i16",
            "i32",
            "i64",
            "i128",
            "isize",
            "u8",
            "u16",
            "u32",
            "u64",
            "u128",
            "usize",
            "f32",
            "f64",
            "bool",
            "char",
            "str",
            "Self",
        ],
        "go" => &[
            "int",
            "int8",
            "int16",
            "int32",
            "int64",
            "uint",
            "uint8",
            "uint16",
            "uint32",
            "uint64",
            "uintptr",
            "float32",
            "float64",
            "complex64",
            "complex128",
            "string",
            "bool",
            "byte",
            "rune",
            "error",
        ],
        "c" => &[
            "size_t",
            "ssize_t",
            "ptrdiff_t",
            "intptr_t",
            "uintptr_t",
            "int8_t",
            "int16_t",
            "int32_t",
            "int64_t",
            "uint8_t",
            "uint16_t",
            "uint32_t",
            "uint64_t",
            "FILE",
            "string",
            "vector",
            "map",
            "set",
            "pair",
            "tuple",
            "shared_ptr",
            "unique_ptr",
        ],
        _ => &[],
    }
}

/// Get the line-comment prefix for a normalized language.
fn comment_prefix(lang: &str) -> &'static str {
    match lang {
        "python" | "shell" | "yaml" | "toml" => "#",
        "c" | "rust" | "js" | "go" => "//",
        // json has no comments (jsonc uses //) — treat as //
        _ => "//",
    }
}

/// Does this normalized language use `/* … */` block comments?
///
/// Read off the [`normalize_lang`] result rather than a second language table: the
/// `#` -comment languages (python, shell, yaml, toml) have no block comments at all, and
/// json/yaml/toml take their own dedicated highlighters below.
fn supports_block_comments(norm: &str) -> bool {
    matches!(norm, "rust" | "js" | "go" | "c")
}

/// Do this language's block comments **nest**?
///
/// Rust's do: `/* /* */ */` is one comment and the first `*/` closes only the inner one.
/// The other C-family languages here do not nest, so their depth never climbs past 1 and
/// the first `*/` closes the comment.
fn block_comments_nest(norm: &str) -> bool {
    norm == "rust"
}

/// The facts the highlighter must carry from one line to the next.
///
/// A struct with named fields rather than a bare `usize` so a new carried fact does not
/// churn every call site — the same reasoning that produced
/// `commands_refactor::BraceScanState`, whose rules this mirrors.
///
/// Two facts are carried: the block-comment depth, and the **delimiter** of a string
/// literal left open at the end of the previous line. All three delimiter shapes that
/// really span lines are modelled (#806): a plain `"…"` (Rust — see
/// [`multiline_strings`]), a raw string `r"…"` / `r#"…"#` at any hash count (Rust), and
/// a backtick literal (a JS/TS template literal or a Go raw string — see
/// [`backtick_strings`]).
///
/// Still **not** modelled, stated rather than implied: `${…}` interpolation inside a JS
/// template literal is string content here, not code, so an interpolated expression is
/// not highlighted. This is a highlighter, not a lexer.
#[derive(Debug, Default, Clone)]
pub struct HighlightState {
    /// `/* … */` nesting depth. `0` means "not inside a block comment"; a stray `*/` at
    /// depth 0 is ignored rather than underflowing.
    pub block_comment_depth: usize,
    /// `Some(delim)` when a string literal opened on an earlier line and has not closed.
    ///
    /// The delimiter type is shared with `commands_refactor::significant_braces` rather
    /// than restated here, so "what closes a raw string" has one implementation and one
    /// table test — a second copy of that rule is how #759 outlived #770 by a day.
    pub open_string: Option<crate::commands_refactor::StringDelim>,
}

/// One stretch of a line, classified by the block-comment scanner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentKind {
    /// Ordinary code — handed to the per-line highlighter.
    Code,
    /// Inside a `/* … */` block comment — emitted dim, never highlighted as code.
    Comment,
    /// Inside a `"…"` string literal opened on an earlier line — emitted as a string,
    /// so code-shaped text inside it (keywords, braces) is not highlighted as code.
    Str,
}

/// Does a bare `"…"` literal in this language really span lines?
///
/// Rust: yes. In C/JS/Go a plain double-quoted string does not cross a newline (they use
/// backticks, template literals, or a `\` continuation — see [`backtick_strings`] for the
/// two that do), so those languages keep their byte-identical single-line behavior for
/// this shape rather than inheriting a Rust rule.
fn multiline_strings(norm: &str) -> bool {
    norm == "rust"
}

/// Does a backtick open a literal that may span lines, and does it honour `\` escapes?
///
/// `Some(true)` — JS/TS template literals: `` `…` `` spans lines and a `` \` `` does not
/// close it. `Some(false)` — Go raw strings: same delimiter, spans lines, but no escapes
/// at all, so a `\` before the backtick is literal content. `None` — every other
/// language, where a backtick is an ordinary character (Rust, C, and the languages with
/// no block comments, which never reach the stateful path).
///
/// Go is here rather than deferred because it is the same shape with the same symptom:
/// fixing one and leaving its mirror twin is the split-fix this codebase keeps paying for.
fn backtick_strings(norm: &str) -> Option<bool> {
    match norm {
        "js" => Some(true),
        "go" => Some(false),
        _ => None,
    }
}

/// Split `chars` into code / block-comment stretches, updating the carried depth.
///
/// Returns `(kind, start, end)` char-index ranges in order, covering the whole line.
/// `//` line comments are left inside a `Code` stretch on purpose — the per-line
/// highlighter already dims them, and duplicating that here would be a second rule.
///
/// A `/*` inside a string or char literal does **not** open a comment: the scanner skips
/// literals using the same model the per-line highlighter uses (a `"…"` run with `\`
/// escapes, plus Rust char literals via `char_literal_len`), so the two agree about where
/// code ends. Raw strings (`r#"…"#`) and backtick literals are skipped through the shared
/// `raw_string_open` / `close_open_string` pair, and any of the three left open at end of
/// line is recorded in `state.open_string` for the next line to close (#806).
fn scan_block_comments(
    norm: &str,
    chars: &[char],
    state: &mut HighlightState,
) -> Vec<(SegmentKind, usize, usize)> {
    let len = chars.len();
    let nest = block_comments_nest(norm);
    let mut segments: Vec<(SegmentKind, usize, usize)> = Vec::new();
    let mut seg_start = 0usize;
    let mut i = 0usize;

    // A string an earlier line left open: everything up to its closer is string content,
    // so `/*`, `//` and keywords inside it are inert. The closer rule is the shared one,
    // so a raw string's `"#` and a template literal's backtick end here exactly where
    // `significant_braces` would end them.
    if let Some(delim) = state.open_string {
        match close_open_string(chars, 0, delim) {
            Some(end) => {
                segments.push((SegmentKind::Str, 0, end));
                state.open_string = None;
                seg_start = end;
                i = end;
            }
            None => return vec![(SegmentKind::Str, 0, len)],
        }
    }

    while i < len {
        if state.block_comment_depth > 0 {
            if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                state.block_comment_depth -= 1;
                i += 2;
                if state.block_comment_depth == 0 {
                    segments.push((SegmentKind::Comment, seg_start, i));
                    seg_start = i;
                }
            } else if nest && chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                state.block_comment_depth += 1;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        // A `//` line comment swallows the rest of the line; leave it as code so the
        // per-line highlighter dims it exactly as it does today.
        if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
            break;
        }

        if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            if seg_start < i {
                segments.push((SegmentKind::Code, seg_start, i));
            }
            seg_start = i;
            state.block_comment_depth = 1;
            i += 2;
            continue;
        }

        // Rust: a bare `'` is a lifetime tick, not a literal opener (#759).
        if norm == "rust" && chars[i] == '\'' && !is_rust_char_literal(chars, i) {
            i += 1;
            continue;
        }

        // Rust raw string `r"…"` / `r#"…"#` (any hash count, `b` prefix allowed): no
        // escapes, closed only by `"` + N `#`, and it may span lines (#806).
        if norm == "rust" {
            if let Some((hashes, body)) = raw_string_open(chars, i) {
                let delim = StringDelim::Raw(hashes);
                match close_open_string(chars, body, delim) {
                    Some(end) => i = end,
                    None => {
                        state.open_string = Some(delim);
                        i = len;
                    }
                }
                continue;
            }
        }

        // JS/TS template literal, or a Go raw string — same delimiter, both span lines.
        if chars[i] == '`' {
            if let Some(escapes) = backtick_strings(norm) {
                let delim = StringDelim::Backtick { escapes };
                match close_open_string(chars, i + 1, delim) {
                    Some(end) => i = end,
                    None => {
                        state.open_string = Some(delim);
                        i = len;
                    }
                }
                continue;
            }
        }

        if chars[i] == '"' || chars[i] == '\'' {
            let quote = chars[i];
            i += 1;
            let mut closed = false;
            while i < len {
                let c = chars[i];
                i += 1;
                if c == '\\' && i < len {
                    i += 1;
                } else if c == quote {
                    closed = true;
                    break;
                }
            }
            // Ran off the end still inside a `"…"`: carry it to the next line, but only
            // where such a literal really spans lines.
            if !closed && quote == '"' && multiline_strings(norm) {
                state.open_string = Some(StringDelim::Normal);
            }
            continue;
        }

        i += 1;
    }

    if seg_start < len {
        let kind = if state.block_comment_depth > 0 {
            SegmentKind::Comment
        } else {
            SegmentKind::Code
        };
        segments.push((kind, seg_start, len));
    }

    segments
}

/// Apply syntax-aware ANSI highlighting to a single code line.
///
/// Colorizes keywords (bold cyan), types (magenta), strings (green),
/// comments (dim), and numbers (yellow).
/// JSON keys are highlighted in cyan, YAML keys in bold yellow.
/// Falls back to DIM when language is unrecognized.
///
/// Stateless: each call starts with a fresh [`HighlightState`], so a `/* … */` block
/// comment that spans lines is only coloured correctly on its first line. Callers that
/// render a whole code block line by line should use [`highlight_code_line_with`] and
/// keep one state across the block — [`MarkdownRenderer`](crate::format::MarkdownRenderer)
/// does.
///
/// `#[allow(dead_code)]`: this is the module's stateless public entry point, and the one
/// in-tree consumer (`MarkdownRenderer`) moved to [`highlight_code_line_with`] when the
/// carried state landed. It is kept rather than deleted because it is the byte-identical
/// baseline the regression tests below assert against — the load-bearing promise that
/// adding state changed nothing for single-line callers.
#[allow(dead_code)]
pub fn highlight_code_line(lang: &str, line: &str) -> String {
    let mut state = HighlightState::default();
    highlight_code_line_with(lang, line, &mut state)
}

/// Highlight one line, carrying block-comment state across lines in `state`.
///
/// On entry, a non-zero `state.block_comment_depth` means an earlier line opened a
/// `/* … */` that has not closed: text is emitted comment-coloured up to the closing
/// `*/` and ordinary highlighting resumes just past it on the same line; when the closer
/// is not on this line the whole line is comment-coloured and the depth is carried on.
///
/// Nesting follows the language: for Rust an inner `/*` increments the depth and only
/// depth 0 resumes code (`/* /* */ */` is one comment); the other C-family languages here
/// do not nest, so the first `*/` closes. A stray `*/` at depth 0 is ordinary code, never
/// an underflow. Languages with no block comments (python, shell, yaml, toml, json) reset
/// the depth to 0 and render exactly as the stateless path does.
///
/// A string literal left open at the end of the previous line is carried too, in all
/// three shapes that really span lines (#806): a plain `"…"` where the language allows it
/// (`multiline_strings` — Rust today), a Rust raw string `r"…"` / `r#"…"#` at any hash
/// count, and a backtick literal (a JS/TS template literal or a Go raw string,
/// `backtick_strings`). Its content up to the closer is emitted string-coloured, so `/*`,
/// `//` and keywords inside it are inert, and ordinary highlighting resumes just past the
/// closer on the same line. Every other language resets the field, so their output is
/// byte-identical to the stateless path.
///
/// **Still not handled, deliberately:** `${…}` interpolation inside a template literal is
/// treated as string content, not code. This colours a whole line; it does not lex one.
///
/// Never indexes a `&str` by byte — it walks a `Vec<char>`, so multi-byte input is safe.
pub fn highlight_code_line_with(lang: &str, line: &str, state: &mut HighlightState) -> String {
    let norm = match normalize_lang(lang) {
        Some(n) => n,
        None => {
            state.block_comment_depth = 0;
            state.open_string = None;
            return format!("{DIM}{line}{RESET}");
        }
    };

    if !supports_block_comments(norm) {
        state.block_comment_depth = 0;
        state.open_string = None;
        return highlight_normalized(norm, line);
    }

    let chars: Vec<char> = line.chars().collect();
    let segments = scan_block_comments(norm, &chars, state);

    // No block comment anywhere on this line (and none carried in): one code segment
    // covering everything, so the output is byte-identical to the pre-state behavior.
    if segments.len() == 1 && segments[0] == (SegmentKind::Code, 0, chars.len()) {
        return highlight_normalized(norm, line);
    }

    let mut out = String::with_capacity(line.len() + 64);
    for (kind, start, end) in segments {
        if start >= end {
            continue;
        }
        let text: String = chars[start..end].iter().collect();
        match kind {
            SegmentKind::Comment => out.push_str(&format!("{DIM}{text}{RESET}")),
            SegmentKind::Str => out.push_str(&format!("{GREEN}{text}{RESET}")),
            SegmentKind::Code => out.push_str(&highlight_normalized(norm, &text)),
        }
    }
    out
}

/// Highlight one line of an already-normalized language, with no cross-line state.
///
/// This is the pre-existing per-line highlighter, unchanged; the block-comment-aware
/// entry point above calls it once per code stretch of a line.
fn highlight_normalized(norm: &str, line: &str) -> String {
    let cp = comment_prefix(norm);
    let trimmed = line.trim_start();

    // Full-line comment detection
    if trimmed.starts_with(cp) {
        return format!("{DIM}{line}{RESET}");
    }

    // JSON: highlight keys and string values with simple heuristic
    if norm == "json" {
        return highlight_json_line(line);
    }

    // YAML: highlight keys (word before colon) and values
    if norm == "yaml" {
        return highlight_yaml_line(line);
    }

    // TOML: highlight keys and values
    if norm == "toml" {
        return highlight_toml_line(line);
    }

    let keywords = lang_keywords(norm);
    let types = lang_types(norm);
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(line.len() + 64);
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Check for inline comment: // or # (at current position)
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' && cp == "//" {
            // Rest of line is a comment
            let rest: String = chars[i..].iter().collect();
            result.push_str(&format!("{DIM}{rest}{RESET}"));
            break;
        }
        if ch == '#' && cp == "#" {
            let rest: String = chars[i..].iter().collect();
            result.push_str(&format!("{DIM}{rest}{RESET}"));
            break;
        }

        // Rust: a `'` is far more often a lifetime tick (`&'a str`, `'static`, `'_`)
        // than a char literal. Opening a string on it made the "literal" run to the
        // next `'` or end of line, swallowing real string literals and pairing two
        // lifetimes into one green run (#759). Only the unambiguous char-literal
        // shapes open a literal here; everything else is emitted as plain text.
        if norm == "rust" && ch == '\'' && !is_rust_char_literal(&chars, i) {
            result.push(ch);
            i += 1;
            continue;
        }

        // String literals: "..." or '...'
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let mut s = String::new();
            s.push(ch);
            i += 1;
            while i < len {
                let c = chars[i];
                s.push(c);
                i += 1;
                if c == '\\' && i < len {
                    s.push(chars[i]);
                    i += 1;
                } else if c == quote {
                    break;
                }
            }
            result.push_str(&format!("{GREEN}{s}{RESET}"));
            continue;
        }

        // Numbers: digit sequences (possibly with . for floats)
        if ch.is_ascii_digit()
            && (i == 0 || !chars[i - 1].is_ascii_alphanumeric() && chars[i - 1] != '_')
        {
            let mut num = String::new();
            while i < len && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '_') {
                num.push(chars[i]);
                i += 1;
            }
            // Don't color if followed by an alpha char (it's part of an identifier)
            if i < len && (chars[i].is_ascii_alphabetic() || chars[i] == '_') {
                result.push_str(&num);
            } else {
                result.push_str(&format!("{YELLOW}{num}{RESET}"));
            }
            continue;
        }

        // Word: check for keyword or type
        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut word = String::new();
            let start = i;
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                word.push(chars[i]);
                i += 1;
            }
            // Only highlight if it's a standalone word (not part of a larger identifier)
            let before_ok = start == 0
                || (!chars[start - 1].is_ascii_alphanumeric() && chars[start - 1] != '_');
            let after_ok = i >= len || (!chars[i].is_ascii_alphanumeric() && chars[i] != '_');
            if before_ok && after_ok {
                if keywords.contains(&word.as_str()) {
                    result.push_str(&format!("{BOLD_CYAN}{word}{RESET}"));
                } else if types.contains(&word.as_str()) {
                    result.push_str(&format!("{MAGENTA}{word}{RESET}"));
                } else {
                    result.push_str(&word);
                }
            } else {
                result.push_str(&word);
            }
            continue;
        }

        result.push(ch);
        i += 1;
    }

    result
}

/// Highlight a JSON line: keys in cyan, strings in green, numbers in yellow.
fn highlight_json_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(line.len() + 64);
    let mut i = 0;
    let mut expecting_value = false;

    while i < len {
        let ch = chars[i];

        // String literal
        if ch == '"' {
            let mut s = String::new();
            s.push(ch);
            i += 1;
            while i < len {
                let c = chars[i];
                s.push(c);
                i += 1;
                if c == '\\' && i < len {
                    s.push(chars[i]);
                    i += 1;
                } else if c == '"' {
                    break;
                }
            }
            // Check if this string is followed by a colon (it's a key)
            let rest_trimmed: String = chars[i..].iter().collect();
            if !expecting_value && rest_trimmed.trim_start().starts_with(':') {
                result.push_str(&format!("{CYAN}{s}{RESET}"));
            } else {
                result.push_str(&format!("{GREEN}{s}{RESET}"));
            }
            continue;
        }

        if ch == ':' {
            expecting_value = true;
            result.push(ch);
            i += 1;
            continue;
        }

        if ch == ',' || ch == '{' || ch == '[' {
            expecting_value = false;
            result.push(ch);
            i += 1;
            continue;
        }

        // Numbers
        if ch.is_ascii_digit() || (ch == '-' && i + 1 < len && chars[i + 1].is_ascii_digit()) {
            let mut num = String::new();
            num.push(ch);
            i += 1;
            while i < len
                && (chars[i].is_ascii_digit()
                    || chars[i] == '.'
                    || chars[i] == 'e'
                    || chars[i] == 'E'
                    || chars[i] == '+'
                    || chars[i] == '-')
            {
                num.push(chars[i]);
                i += 1;
            }
            result.push_str(&format!("{YELLOW}{num}{RESET}"));
            continue;
        }

        // true/false/null
        if ch.is_ascii_alphabetic() {
            let mut word = String::new();
            while i < len && chars[i].is_ascii_alphabetic() {
                word.push(chars[i]);
                i += 1;
            }
            match word.as_str() {
                "true" | "false" | "null" => {
                    result.push_str(&format!("{BOLD_CYAN}{word}{RESET}"));
                }
                _ => result.push_str(&word),
            }
            continue;
        }

        result.push(ch);
        i += 1;
    }

    result
}

/// Highlight a YAML line: keys in bold yellow, strings in green, numbers in yellow.
fn highlight_yaml_line(line: &str) -> String {
    let trimmed = line.trim_start();

    // Comment
    if trimmed.starts_with('#') {
        return format!("{DIM}{line}{RESET}");
    }

    // Section header [section]
    if trimmed.starts_with("---") || trimmed.starts_with("...") {
        return format!("{DIM}{line}{RESET}");
    }

    // Key-value pair: look for "key:" pattern
    if let Some(colon_pos) = trimmed.find(':') {
        let key_part = &trimmed[..colon_pos];
        // Only treat as key if it doesn't start with - (list item) and key is simple
        if !key_part.contains(' ') || key_part.starts_with("- ") || key_part.starts_with('-') {
            let indent = &line[..line.len() - trimmed.len()];
            let value_part = &trimmed[colon_pos + 1..];
            let value_highlighted = highlight_yaml_value(value_part);
            return format!("{indent}{BOLD_YELLOW}{key_part}{RESET}:{value_highlighted}");
        }
    }

    // List item
    if let Some(rest) = trimmed.strip_prefix("- ") {
        let indent = &line[..line.len() - trimmed.len()];
        return format!("{indent}- {}", highlight_yaml_value(rest));
    }

    line.to_string()
}

/// Highlight a YAML value (strings, numbers, booleans).
fn highlight_yaml_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return value.to_string();
    }

    // Inline comment
    if let Some(comment_pos) = trimmed.find(" #") {
        let before = &trimmed[..comment_pos];
        let after = &trimmed[comment_pos..];
        return format!(" {}{DIM}{after}{RESET}", highlight_yaml_value_inner(before));
    }

    format!(" {}", highlight_yaml_value_inner(trimmed))
}

fn highlight_yaml_value_inner(value: &str) -> String {
    // Quoted string
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        return format!("{GREEN}{value}{RESET}");
    }

    // Boolean/null keywords
    match value {
        "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "~" => {
            return format!("{BOLD_CYAN}{value}{RESET}");
        }
        _ => {}
    }

    // Number
    if value.parse::<f64>().is_ok() {
        return format!("{YELLOW}{value}{RESET}");
    }

    // Plain string — leave as-is
    value.to_string()
}

/// Highlight a TOML line: section headers in bold, keys in bold yellow.
fn highlight_toml_line(line: &str) -> String {
    let trimmed = line.trim_start();

    // Comment
    if trimmed.starts_with('#') {
        return format!("{DIM}{line}{RESET}");
    }

    // Section header [section] or [[array]]
    if trimmed.starts_with('[') {
        return format!("{BOLD}{CYAN}{line}{RESET}");
    }

    // Key = value
    if let Some(eq_pos) = trimmed.find('=') {
        let key_part = trimmed[..eq_pos].trim();
        let value_part = trimmed[eq_pos + 1..].trim();
        let indent = &line[..line.len() - trimmed.len()];
        let value_highlighted = highlight_toml_value(value_part);
        return format!("{indent}{BOLD_YELLOW}{key_part}{RESET} = {value_highlighted}");
    }

    line.to_string()
}

fn highlight_toml_value(value: &str) -> String {
    // String
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        return format!("{GREEN}{value}{RESET}");
    }

    // Boolean
    match value {
        "true" | "false" => return format!("{BOLD_CYAN}{value}{RESET}"),
        _ => {}
    }

    // Number
    if value.parse::<f64>().is_ok() {
        return format!("{YELLOW}{value}{RESET}");
    }

    // Array or inline table — leave as-is for simplicity
    value.to_string()
}

/// Get pricing rates (per MTok) for a model.
/// Returns (input, cache_write, cache_read, output) or None if model is unknown.
#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: render a full string through the MarkdownRenderer.
    fn render_full(input: &str) -> String {
        let mut r = MarkdownRenderer::new();
        let mut out = r.render_delta(input);
        out.push_str(&r.flush());
        out
    }

    #[test]
    fn test_highlight_rust_keywords() {
        let out = highlight_code_line("rust", "    let mut x = 42;");
        assert!(out.contains(&format!("{BOLD_CYAN}let{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}mut{RESET}")));
        assert!(out.contains(&format!("{YELLOW}42{RESET}")));
    }

    #[test]
    fn test_highlight_rust_fn() {
        let out = highlight_code_line("rust", "fn main() {");
        assert!(out.contains(&format!("{BOLD_CYAN}fn{RESET}")));
        assert!(out.contains("main"));
    }

    #[test]
    fn test_highlight_rust_string() {
        let out = highlight_code_line("rs", r#"let s = "hello world";"#);
        assert!(out.contains(&format!("{GREEN}\"hello world\"{RESET}")));
    }

    #[test]
    fn test_highlight_rust_comment() {
        let out = highlight_code_line("rust", "    // this is a comment");
        assert!(out.contains(&format!("{DIM}")));
        assert!(out.contains("this is a comment"));
    }

    #[test]
    fn test_highlight_rust_full_line_comment() {
        let out = highlight_code_line("rust", "// full line comment");
        assert_eq!(out, format!("{DIM}// full line comment{RESET}"));
    }

    #[test]
    fn test_highlight_python_keywords() {
        let out = highlight_code_line("python", "def hello(self):");
        assert!(out.contains(&format!("{BOLD_CYAN}def{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}self{RESET}")));
    }

    #[test]
    fn test_highlight_python_comment() {
        let out = highlight_code_line("py", "# a comment");
        assert_eq!(out, format!("{DIM}# a comment{RESET}"));
    }

    #[test]
    fn test_highlight_js_keywords() {
        let out = highlight_code_line("javascript", "const x = async () => {");
        assert!(out.contains(&format!("{BOLD_CYAN}const{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}async{RESET}")));
    }

    #[test]
    fn test_highlight_ts_alias() {
        let out = highlight_code_line("ts", "let y = 10;");
        assert!(out.contains(&format!("{BOLD_CYAN}let{RESET}")));
        assert!(out.contains(&format!("{YELLOW}10{RESET}")));
    }

    #[test]
    fn test_highlight_go_keywords() {
        let out = highlight_code_line("go", "func main() {");
        assert!(out.contains(&format!("{BOLD_CYAN}func{RESET}")));
    }

    #[test]
    fn test_highlight_shell_keywords() {
        let out = highlight_code_line("bash", "if [ -f file ]; then");
        assert!(out.contains(&format!("{BOLD_CYAN}if{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}then{RESET}")));
    }

    #[test]
    fn test_highlight_shell_comment() {
        let out = highlight_code_line("sh", "# shell comment");
        assert_eq!(out, format!("{DIM}# shell comment{RESET}"));
    }

    #[test]
    fn test_highlight_unknown_lang_falls_back_to_dim() {
        let out = highlight_code_line("haskell", "main = putStrLn");
        assert_eq!(out, format!("{DIM}main = putStrLn{RESET}"));
    }

    #[test]
    fn test_highlight_empty_line() {
        let out = highlight_code_line("rust", "");
        assert_eq!(out, "");
    }

    #[test]
    fn test_highlight_no_false_keyword_in_identifier() {
        // "letter" contains "let" but should NOT be highlighted
        let out = highlight_code_line("rust", "let letter = 1;");
        assert!(out.contains(&format!("{BOLD_CYAN}let{RESET}")));
        // "letter" should appear plain
        assert!(out.contains("letter"));
        // Make sure "letter" isn't colored as keyword
        let letter_highlighted = format!("{BOLD_CYAN}letter{RESET}");
        assert!(!out.contains(&letter_highlighted));
    }

    #[test]
    fn test_highlight_string_with_escape() {
        let out = highlight_code_line("rust", r#"let s = "he\"llo";"#);
        assert!(out.contains(&format!("{GREEN}")));
        assert!(out.contains(&format!("{BOLD_CYAN}let{RESET}")));
    }

    #[test]
    fn test_highlight_inline_comment_after_code() {
        let out = highlight_code_line("rust", "let x = 1; // comment");
        assert!(out.contains(&format!("{BOLD_CYAN}let{RESET}")));
        assert!(out.contains(&format!("{DIM}// comment{RESET}")));
    }

    #[test]
    fn test_highlight_number_float() {
        let out = highlight_code_line("rust", "let pi = 3.14;");
        assert!(out.contains(&format!("{YELLOW}3.14{RESET}")));
    }

    #[test]
    fn test_normalize_lang_aliases() {
        assert_eq!(normalize_lang("rust"), Some("rust"));
        assert_eq!(normalize_lang("rs"), Some("rust"));
        assert_eq!(normalize_lang("Python"), Some("python"));
        assert_eq!(normalize_lang("JS"), Some("js"));
        assert_eq!(normalize_lang("typescript"), Some("js"));
        assert_eq!(normalize_lang("tsx"), Some("js"));
        assert_eq!(normalize_lang("golang"), Some("go"));
        assert_eq!(normalize_lang("zsh"), Some("shell"));
        assert_eq!(normalize_lang("haskell"), None);
    }

    #[test]
    fn test_highlight_renders_through_markdown() {
        // End-to-end: markdown renderer should use highlighting for tagged blocks
        let input = "```rust\nfn main() {\n    return 42;\n}\n```\n";
        let out = render_full(input);
        assert!(out.contains(&format!("{BOLD_CYAN}fn{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}return{RESET}")));
        assert!(out.contains(&format!("{YELLOW}42{RESET}")));
    }

    // --- Rust highlighting: types ---

    #[test]
    fn test_highlight_rust_types() {
        let out = highlight_code_line("rust", "let v: Vec<String> = Vec::new();");
        assert!(out.contains(&format!("{MAGENTA}Vec{RESET}")));
        assert!(out.contains(&format!("{MAGENTA}String{RESET}")));
    }

    #[test]
    fn test_highlight_rust_option_result() {
        let out = highlight_code_line("rust", "fn foo() -> Option<Result<u32, String>> {");
        assert!(out.contains(&format!("{MAGENTA}Option{RESET}")));
        assert!(out.contains(&format!("{MAGENTA}Result{RESET}")));
        assert!(out.contains(&format!("{MAGENTA}u32{RESET}")));
    }

    #[test]
    fn test_highlight_rust_primitive_types() {
        let out = highlight_code_line("rust", "let x: i32 = 0;");
        assert!(out.contains(&format!("{MAGENTA}i32{RESET}")));
        assert!(out.contains(&format!("{YELLOW}0{RESET}")));
    }

    #[test]
    fn test_highlight_rust_self_type() {
        let out = highlight_code_line("rust", "impl Self {");
        assert!(out.contains(&format!("{MAGENTA}Self{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}impl{RESET}")));
    }

    // --- Python highlighting: comprehensive ---

    #[test]
    fn test_highlight_python_string() {
        let out = highlight_code_line("python", "name = \"hello world\"");
        assert!(out.contains(&format!("{GREEN}\"hello world\"{RESET}")));
    }

    #[test]
    fn test_highlight_python_single_quote_string() {
        let out = highlight_code_line("python", "name = 'hello'");
        assert!(out.contains(&format!("{GREEN}'hello'{RESET}")));
    }

    #[test]
    fn test_highlight_python_inline_comment() {
        let out = highlight_code_line("python", "x = 1  # set x");
        assert!(out.contains(&format!("{YELLOW}1{RESET}")));
        assert!(out.contains(&format!("{DIM}")));
        assert!(out.contains("set x"));
    }

    #[test]
    fn test_highlight_python_class_def() {
        let out = highlight_code_line("python", "class MyClass(Base):");
        assert!(out.contains(&format!("{BOLD_CYAN}class{RESET}")));
        assert!(out.contains("MyClass"));
    }

    #[test]
    fn test_highlight_python_boolean_none() {
        let out = highlight_code_line("python", "if True and not None:");
        assert!(out.contains(&format!("{BOLD_CYAN}True{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}None{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}not{RESET}")));
    }

    #[test]
    fn test_highlight_python_import() {
        let out = highlight_code_line("python", "from os import path");
        assert!(out.contains(&format!("{BOLD_CYAN}from{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}import{RESET}")));
    }

    // --- JavaScript/TypeScript highlighting: comprehensive ---

    #[test]
    fn test_highlight_js_function_declaration() {
        let out = highlight_code_line("js", "function hello() {");
        assert!(out.contains(&format!("{BOLD_CYAN}function{RESET}")));
    }

    #[test]
    fn test_highlight_js_string_template() {
        let out = highlight_code_line("javascript", "const msg = \"hello\";");
        assert!(out.contains(&format!("{BOLD_CYAN}const{RESET}")));
        assert!(out.contains(&format!("{GREEN}\"hello\"{RESET}")));
    }

    #[test]
    fn test_highlight_js_null_undefined() {
        let out = highlight_code_line("js", "if (x === null || y === undefined) {");
        assert!(out.contains(&format!("{BOLD_CYAN}null{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}undefined{RESET}")));
    }

    #[test]
    fn test_highlight_js_comment() {
        let out = highlight_code_line("js", "// this is a JS comment");
        assert_eq!(out, format!("{DIM}// this is a JS comment{RESET}"));
    }

    #[test]
    fn test_highlight_tsx_recognized() {
        let out = highlight_code_line("tsx", "const App = () => {");
        assert!(out.contains(&format!("{BOLD_CYAN}const{RESET}")));
    }

    // --- Shell highlighting: comprehensive ---

    #[test]
    fn test_highlight_shell_for_loop() {
        let out = highlight_code_line("bash", "for f in *.txt; do");
        assert!(out.contains(&format!("{BOLD_CYAN}for{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}in{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}do{RESET}")));
    }

    #[test]
    fn test_highlight_shell_string() {
        let out = highlight_code_line("shell", "echo \"hello world\"");
        assert!(out.contains(&format!("{BOLD_CYAN}echo{RESET}")));
        assert!(out.contains(&format!("{GREEN}\"hello world\"{RESET}")));
    }

    #[test]
    fn test_highlight_shell_export() {
        let out = highlight_code_line("bash", "export PATH=\"/usr/bin\"");
        assert!(out.contains(&format!("{BOLD_CYAN}export{RESET}")));
    }

    #[test]
    fn test_highlight_zsh_recognized() {
        let out = highlight_code_line("zsh", "if [ -f file ]; then");
        assert!(out.contains(&format!("{BOLD_CYAN}if{RESET}")));
    }

    // --- C/C++ highlighting ---

    #[test]
    fn test_highlight_c_keywords() {
        let out = highlight_code_line("c", "int main() {");
        assert!(out.contains(&format!("{BOLD_CYAN}int{RESET}")));
        assert!(out.contains("main"));
    }

    #[test]
    fn test_highlight_cpp_keywords() {
        let out = highlight_code_line("cpp", "class Foo : public Bar {");
        assert!(out.contains(&format!("{BOLD_CYAN}class{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}public{RESET}")));
    }

    #[test]
    fn test_highlight_c_comment() {
        let out = highlight_code_line("c", "// C comment");
        assert_eq!(out, format!("{DIM}// C comment{RESET}"));
    }

    #[test]
    fn test_highlight_c_string() {
        let out = highlight_code_line("c", "char *s = \"hello\";");
        assert!(out.contains(&format!("{GREEN}\"hello\"{RESET}")));
    }

    #[test]
    fn test_highlight_c_types() {
        let out = highlight_code_line("c", "size_t len = strlen(s);");
        assert!(out.contains(&format!("{MAGENTA}size_t{RESET}")));
    }

    #[test]
    fn test_highlight_hpp_recognized() {
        let out = highlight_code_line("hpp", "namespace foo {");
        assert!(out.contains(&format!("{BOLD_CYAN}namespace{RESET}")));
    }

    // --- Go highlighting: types ---

    #[test]
    fn test_highlight_go_types() {
        let out = highlight_code_line("go", "var x int = 42");
        assert!(out.contains(&format!("{BOLD_CYAN}var{RESET}")));
        assert!(out.contains(&format!("{MAGENTA}int{RESET}")));
        assert!(out.contains(&format!("{YELLOW}42{RESET}")));
    }

    #[test]
    fn test_highlight_go_string_type() {
        let out = highlight_code_line("go", "func greet(name string) error {");
        assert!(out.contains(&format!("{BOLD_CYAN}func{RESET}")));
        assert!(out.contains(&format!("{MAGENTA}string{RESET}")));
        assert!(out.contains(&format!("{MAGENTA}error{RESET}")));
    }

    // --- JSON highlighting ---

    #[test]
    fn test_highlight_json_key_value() {
        let out = highlight_code_line("json", r#"  "name": "yoyo","#);
        assert!(out.contains(&format!("{CYAN}\"name\"{RESET}")));
        assert!(out.contains(&format!("{GREEN}\"yoyo\"{RESET}")));
    }

    #[test]
    fn test_highlight_json_number() {
        let out = highlight_code_line("json", r#"  "count": 42,"#);
        assert!(out.contains(&format!("{CYAN}\"count\"{RESET}")));
        assert!(out.contains(&format!("{YELLOW}42{RESET}")));
    }

    #[test]
    fn test_highlight_json_boolean() {
        let out = highlight_code_line("json", r#"  "active": true,"#);
        assert!(out.contains(&format!("{BOLD_CYAN}true{RESET}")));
    }

    #[test]
    fn test_highlight_json_null() {
        let out = highlight_code_line("json", r#"  "value": null"#);
        assert!(out.contains(&format!("{BOLD_CYAN}null{RESET}")));
    }

    #[test]
    fn test_highlight_json_braces() {
        // Braces and brackets should pass through
        let out = highlight_code_line("json", "  {");
        assert!(out.contains('{'));
    }

    #[test]
    fn test_highlight_jsonc_recognized() {
        let out = highlight_code_line("jsonc", r#"  "key": "value""#);
        assert!(out.contains(&format!("{CYAN}\"key\"{RESET}")));
    }

    // --- YAML highlighting ---

    #[test]
    fn test_highlight_yaml_key_value() {
        let out = highlight_code_line("yaml", "name: yoyo");
        assert!(out.contains(&format!("{BOLD_YELLOW}name{RESET}")));
    }

    #[test]
    fn test_highlight_yaml_string_value() {
        let out = highlight_code_line("yaml", "name: \"yoyo\"");
        assert!(out.contains(&format!("{BOLD_YELLOW}name{RESET}")));
        assert!(out.contains(&format!("{GREEN}\"yoyo\"{RESET}")));
    }

    #[test]
    fn test_highlight_yaml_boolean() {
        let out = highlight_code_line("yaml", "enabled: true");
        assert!(out.contains(&format!("{BOLD_CYAN}true{RESET}")));
    }

    #[test]
    fn test_highlight_yaml_number() {
        let out = highlight_code_line("yaml", "port: 8080");
        assert!(out.contains(&format!("{YELLOW}8080{RESET}")));
    }

    #[test]
    fn test_highlight_yaml_comment() {
        let out = highlight_code_line("yml", "# a yaml comment");
        assert_eq!(out, format!("{DIM}# a yaml comment{RESET}"));
    }

    #[test]
    fn test_highlight_yaml_document_separator() {
        let out = highlight_code_line("yaml", "---");
        assert!(out.contains(&format!("{DIM}---{RESET}")));
    }

    #[test]
    fn test_highlight_yml_alias() {
        // "yml" should be recognized as yaml
        assert_eq!(normalize_lang("yml"), Some("yaml"));
    }

    // --- TOML highlighting ---

    #[test]
    fn test_highlight_toml_section() {
        let out = highlight_code_line("toml", "[package]");
        assert!(out.contains(&format!("{BOLD}{CYAN}[package]{RESET}")));
    }

    #[test]
    fn test_highlight_toml_key_string() {
        let out = highlight_code_line("toml", "name = \"yoyo\"");
        assert!(out.contains(&format!("{BOLD_YELLOW}name{RESET}")));
        assert!(out.contains(&format!("{GREEN}\"yoyo\"{RESET}")));
    }

    #[test]
    fn test_highlight_toml_key_number() {
        let out = highlight_code_line("toml", "version = 1");
        assert!(out.contains(&format!("{BOLD_YELLOW}version{RESET}")));
        assert!(out.contains(&format!("{YELLOW}1{RESET}")));
    }

    #[test]
    fn test_highlight_toml_boolean() {
        let out = highlight_code_line("toml", "enabled = true");
        assert!(out.contains(&format!("{BOLD_CYAN}true{RESET}")));
    }

    #[test]
    fn test_highlight_toml_comment() {
        let out = highlight_code_line("toml", "# a toml comment");
        assert_eq!(out, format!("{DIM}# a toml comment{RESET}"));
    }

    #[test]
    fn test_highlight_toml_array_section() {
        let out = highlight_code_line("toml", "[[bin]]");
        assert!(out.contains(&format!("{BOLD}{CYAN}[[bin]]{RESET}")));
    }

    // --- normalize_lang expanded aliases ---

    #[test]
    fn test_normalize_lang_c_family() {
        assert_eq!(normalize_lang("c"), Some("c"));
        assert_eq!(normalize_lang("cpp"), Some("c"));
        assert_eq!(normalize_lang("c++"), Some("c"));
        assert_eq!(normalize_lang("cc"), Some("c"));
        assert_eq!(normalize_lang("h"), Some("c"));
        assert_eq!(normalize_lang("hpp"), Some("c"));
    }

    #[test]
    fn test_normalize_lang_data_formats() {
        assert_eq!(normalize_lang("json"), Some("json"));
        assert_eq!(normalize_lang("jsonc"), Some("json"));
        assert_eq!(normalize_lang("yaml"), Some("yaml"));
        assert_eq!(normalize_lang("yml"), Some("yaml"));
        assert_eq!(normalize_lang("toml"), Some("toml"));
    }

    // --- #758: fence tags with attributes (```rust,ignore / ```rust no_run) ---

    #[test]
    fn test_normalize_lang_ignores_fence_attributes() {
        // A tag with an attribute must resolve exactly like the bare tag.
        assert_eq!(normalize_lang("rust,ignore"), normalize_lang("rust"));
        assert_eq!(normalize_lang("rust,no_run"), normalize_lang("rust"));
        assert_eq!(normalize_lang("rust no_run"), normalize_lang("rust"));
        assert_eq!(normalize_lang("rust\tignore"), normalize_lang("rust"));
        assert_eq!(normalize_lang("Rust,ignore"), normalize_lang("rust"));
        assert_eq!(normalize_lang("js,live"), normalize_lang("js"));
        assert_eq!(normalize_lang("python,skip"), normalize_lang("python"));
        assert_eq!(normalize_lang("rs,should_panic"), Some("rust"));
    }

    #[test]
    fn test_normalize_lang_bare_tags_unchanged() {
        assert_eq!(normalize_lang("rust"), Some("rust"));
        assert_eq!(normalize_lang("rs"), Some("rust"));
        assert_eq!(normalize_lang("yaml"), Some("yaml"));
        // An empty head is not a language.
        assert_eq!(normalize_lang(""), None);
        assert_eq!(normalize_lang(",ignore"), None);
        // An unknown head stays unknown, attribute or not.
        assert_eq!(normalize_lang("nosuchlang"), None);
        assert_eq!(normalize_lang("cobol,x"), None);
    }

    #[test]
    fn test_highlight_code_line_honors_fence_attributes() {
        // Emission point: the rendered string is the promise a user experiences.
        assert_eq!(
            highlight_code_line("rust,ignore", "let x = 1;"),
            highlight_code_line("rust", "let x = 1;")
        );
        assert_eq!(
            highlight_code_line("rust no_run", "let x = 1;"),
            highlight_code_line("rust", "let x = 1;")
        );
        // ...and it is genuinely highlighted, not just consistently dimmed.
        let out = highlight_code_line("rust,ignore", "let x = 1;");
        assert!(
            out.contains(&format!("{BOLD_CYAN}let{RESET}")),
            "got {out:?}"
        );
    }

    // --- #759: Rust lifetimes must not open a string literal ---

    #[test]
    fn test_rust_char_literal_end_table() {
        // Tables the discrimination the highlighter consumes. Since Day 169 the rule
        // lives once, in `commands_refactor::char_literal_len`, so the unicode-escape
        // rows below are new coverage the old local copy could not satisfy.
        let cases: &[(&str, bool)] = &[
            ("'x'", true),
            ("'\\n'", true),
            ("'\\\\'", true),
            ("'\\''", true),
            ("'\\u{7d}'", true),
            ("'\\u{1F600}'", true),
            ("'a", false),
            ("'a>", false),
            ("'static", false),
            ("'_", false),
            ("'", false),
            ("''", false),
        ];
        for (src, want) in cases {
            let chars: Vec<char> = src.chars().collect();
            assert_eq!(is_rust_char_literal(&chars, 0), *want, "input {src:?}");
        }
        // Not a tick at all -> not a char literal.
        let chars: Vec<char> = "x".chars().collect();
        assert!(!is_rust_char_literal(&chars, 0));
    }

    #[test]
    fn test_rust_lifetime_does_not_swallow_string_literal() {
        let out = highlight_code_line("rust", "let x: &'a str = \"hi\";");
        assert!(
            out.contains(&format!("{GREEN}\"hi\"{RESET}")),
            "got {out:?}"
        );
        assert!(!out.contains(&format!("{GREEN}'a")), "got {out:?}");
    }

    #[test]
    fn test_rust_two_lifetimes_do_not_pair_up() {
        let out = highlight_code_line("rust", "fn f<'a>(s: &'a str) {}");
        assert!(!out.contains(&format!("{GREEN}")), "got {out:?}");
    }

    #[test]
    fn test_rust_char_literal_still_highlighted() {
        let out = highlight_code_line("rust", "let c = 'x';");
        assert!(out.contains(&format!("{GREEN}'x'{RESET}")), "got {out:?}");
        let esc = highlight_code_line("rust", "let c = '\\n';");
        assert!(esc.contains(&format!("{GREEN}'\\n'{RESET}")), "got {esc:?}");
        // Unicode escapes: coverage the deleted local copy could not satisfy, asserted
        // at the emission point rather than only in the helper table above.
        let uni = highlight_code_line("rust", "let c = '\\u{1F600}';");
        assert!(
            uni.contains(&format!("{GREEN}'\\u{{1F600}}'{RESET}")),
            "got {uni:?}"
        );
    }

    #[test]
    fn test_rust_apostrophe_in_comment_unchanged() {
        // Near-miss that must pass through: the inline-comment branch still wins.
        let out = highlight_code_line("rust", "let y = 1; // don't");
        assert!(
            out.contains(&format!("{DIM}// don't{RESET}")),
            "got {out:?}"
        );
    }

    #[test]
    fn test_single_quotes_still_strings_in_other_languages() {
        let py = highlight_code_line("python", "s = 'hi'");
        assert!(py.contains(&format!("{GREEN}'hi'{RESET}")), "got {py:?}");
        let js = highlight_code_line("js", "const s = 'hi';");
        assert!(js.contains(&format!("{GREEN}'hi'{RESET}")), "got {js:?}");
        let sh = highlight_code_line("shell", "echo 'hi'");
        assert!(sh.contains(&format!("{GREEN}'hi'{RESET}")), "got {sh:?}");
    }

    // --- End-to-end through MarkdownRenderer ---

    #[test]
    fn test_highlight_json_through_markdown() {
        let input = "```json\n{\"name\": \"yoyo\"}\n```\n";
        let out = render_full(input);
        assert!(out.contains(&format!("{CYAN}\"name\"{RESET}")));
        assert!(out.contains(&format!("{GREEN}\"yoyo\"{RESET}")));
    }

    #[test]
    fn test_highlight_yaml_through_markdown() {
        let input = "```yaml\nname: yoyo\n```\n";
        let out = render_full(input);
        assert!(out.contains(&format!("{BOLD_YELLOW}name{RESET}")));
    }

    #[test]
    fn test_highlight_toml_through_markdown() {
        let input = "```toml\n[package]\nname = \"yoyo\"\n```\n";
        let out = render_full(input);
        assert!(out.contains(&format!("{BOLD}{CYAN}[package]{RESET}")));
        assert!(out.contains(&format!("{GREEN}\"yoyo\"{RESET}")));
    }

    #[test]
    fn test_highlight_c_through_markdown() {
        let input = "```c\nint main() {\n    return 0;\n}\n```\n";
        let out = render_full(input);
        assert!(out.contains(&format!("{BOLD_CYAN}int{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}return{RESET}")));
        assert!(out.contains(&format!("{YELLOW}0{RESET}")));
    }

    // --- Spinner tests ---

    // --- Multi-line string literals (cross-line state) ---

    /// A Rust `"…"` left open at end of line keeps the *next* line inside the string:
    /// the whole continuation line renders as a string, and code-shaped text inside it
    /// (keywords, braces) is not highlighted as code.
    #[test]
    fn rust_string_spanning_lines_keeps_continuation_line_as_string() {
        let mut st = HighlightState::default();

        let l1 = highlight_code_line_with("rust", "let s = \"start {", &mut st);
        assert!(
            l1.contains(&format!("{BOLD_CYAN}let{RESET}")),
            "opening line still highlights code: {l1:?}"
        );
        assert!(st.open_string.is_some(), "line 1 leaves the string open");

        let cont = "fn not really code {";
        let l2 = highlight_code_line_with("rust", cont, &mut st);
        assert_eq!(
            l2,
            format!("{GREEN}{cont}{RESET}"),
            "the whole continuation line is string content"
        );
        assert!(
            !l2.contains(&format!("{BOLD_CYAN}")),
            "`fn` inside a string is not a keyword: {l2:?}"
        );
        assert!(st.open_string.is_some(), "line 2 does not close the string");

        let l3 = highlight_code_line_with("rust", "end\"; let x = 1;", &mut st);
        assert!(
            l3.starts_with(&format!("{GREEN}end\"{RESET}")),
            "the closer ends the string segment: {l3:?}"
        );
        assert!(
            l3.contains(&format!("{BOLD_CYAN}let{RESET}")),
            "code after the closer is highlighted again: {l3:?}"
        );
        assert!(st.open_string.is_none(), "line 3 closes the string");
    }

    /// An escaped quote on a continuation line does not close the string, and a
    /// trailing backslash (escaping the newline) leaves it open.
    #[test]
    fn rust_multiline_string_honours_escapes_on_continuation_lines() {
        let mut st = HighlightState::default();
        highlight_code_line_with("rust", "let s = \"a", &mut st);
        assert!(st.open_string.is_some());

        let cont = "still \\\" inside";
        let out = highlight_code_line_with("rust", cont, &mut st);
        assert_eq!(out, format!("{GREEN}{cont}{RESET}"), "got {out:?}");
        assert!(st.open_string.is_some(), "an escaped quote must not close the string");

        highlight_code_line_with("rust", "done\"", &mut st);
        assert!(st.open_string.is_none());
    }

    /// A string that opens *and* closes on the same line is byte-identical to the
    /// stateless rendering — the common case must not regress.
    #[test]
    fn rust_same_line_string_is_unchanged_by_the_carried_state() {
        let mut st = HighlightState::default();
        let line = "let s = \"hello\"; let n = 1;";
        assert_eq!(
            highlight_code_line_with("rust", line, &mut st),
            highlight_code_line("rust", line)
        );
        assert!(st.open_string.is_none());
    }

    /// Only Rust carries a `"` across lines. In C-family languages a plain string does
    /// not span lines, so those languages stay byte-identical to before.
    #[test]
    fn non_rust_languages_do_not_carry_open_strings_across_lines() {
        for lang in ["js", "go", "c", "python"] {
            let mut st = HighlightState::default();
            highlight_code_line_with(lang, "var s = \"unterminated", &mut st);
            assert!(st.open_string.is_none(), "{lang} must not carry an open string");

            let cont = "return 0;";
            assert_eq!(
                highlight_code_line_with(lang, cont, &mut st),
                highlight_code_line(lang, cont),
                "{lang} continuation line is unchanged"
            );
        }
    }

    /// A `/*` inside an open string is string content, not a block comment.
    #[test]
    fn block_comment_opener_inside_a_multiline_string_is_inert() {
        let mut st = HighlightState::default();
        highlight_code_line_with("rust", "let s = \"a", &mut st);
        highlight_code_line_with("rust", "/* not a comment", &mut st);
        assert_eq!(st.block_comment_depth, 0, "no comment was opened");
        assert!(st.open_string.is_some());
        let out = highlight_code_line_with("rust", "b\"; let x = 1;", &mut st);
        assert!(
            out.contains(&format!("{BOLD_CYAN}let{RESET}")),
            "got {out:?}"
        );
    }

    // ---- #806: raw strings and backtick literals carried across lines ----------------
    //
    // Pinned at the **emission point** — every assertion is on the string a caller
    // receives from `highlight_code_line_with`, not on the carried state one layer below
    // it. The state is checked too, but it is never the whole test.

    /// A Rust raw string opened on line 1 keeps string colour on line 2 (#806). Before
    /// this, the run ended at the line end and line 2 was highlighted as code.
    #[test]
    fn rust_raw_string_spanning_lines_keeps_string_colour_on_line_two() {
        let mut st = HighlightState::default();
        highlight_code_line_with("rust", "let s = r#\"a", &mut st);
        assert!(st.open_string.is_some(), "line 1 leaves the raw string open");

        let line2 = "let x = 1; // not a comment";
        let out = highlight_code_line_with("rust", line2, &mut st);
        assert_eq!(
            out,
            format!("{GREEN}{line2}{RESET}"),
            "line 2 is string content, keyword and `//` inert"
        );

        let out3 = highlight_code_line_with("rust", "b\"#; let y = 2;", &mut st);
        assert!(st.open_string.is_none(), "line 3 closes the raw string");
        assert!(
            out3.contains(&format!("{BOLD_CYAN}let{RESET}")),
            "code resumes just past the closer, got {out3:?}"
        );
    }

    /// The hash count is part of the closer: a `"#` inside an `r##"…"##` does not end it.
    #[test]
    fn rust_raw_string_closer_honours_the_hash_count() {
        let mut st = HighlightState::default();
        highlight_code_line_with("rust", "let s = r##\"a", &mut st);
        let line2 = "still \"# inside";
        assert_eq!(
            highlight_code_line_with("rust", line2, &mut st),
            format!("{GREEN}{line2}{RESET}")
        );
        assert!(st.open_string.is_some(), "one `#` short does not close it");

        highlight_code_line_with("rust", "b\"##;", &mut st);
        assert!(st.open_string.is_none(), "two `#` closes it");
    }

    /// A JS template literal opened on line 1 keeps string colour on line 2 (#806).
    #[test]
    fn js_template_literal_spanning_lines_keeps_string_colour_on_line_two() {
        let mut st = HighlightState::default();
        highlight_code_line_with("js", "const s = `a", &mut st);
        assert!(st.open_string.is_some(), "line 1 leaves the template open");

        let line2 = "const x = 1; /* not a comment */";
        let out = highlight_code_line_with("js", line2, &mut st);
        assert_eq!(out, format!("{GREEN}{line2}{RESET}"));
        assert_eq!(st.block_comment_depth, 0, "no comment was opened");

        let out3 = highlight_code_line_with("js", "b`; const y = 2;", &mut st);
        assert!(st.open_string.is_none(), "line 3 closes the template");
        assert!(
            out3.contains(&format!("{BOLD_CYAN}const{RESET}")),
            "code resumes just past the backtick, got {out3:?}"
        );
    }

    /// Go raw strings are the same shape and are carried too — fixing one and leaving
    /// its mirror twin is the split-fix this codebase keeps paying for.
    #[test]
    fn go_raw_string_spanning_lines_keeps_string_colour_on_line_two() {
        let mut st = HighlightState::default();
        highlight_code_line_with("go", "s := `a", &mut st);
        let line2 = "func not_a_keyword_here() {";
        assert_eq!(
            highlight_code_line_with("go", line2, &mut st),
            format!("{GREEN}{line2}{RESET}")
        );
        highlight_code_line_with("go", "b`", &mut st);
        assert!(st.open_string.is_none());
    }

    /// The escape rule differs between the two backtick languages, and that is the whole
    /// reason `StringDelim::Backtick` carries a flag: JS honours `\``, Go has no escapes.
    #[test]
    fn backtick_escape_rule_differs_between_js_and_go() {
        let mut js = HighlightState::default();
        highlight_code_line_with("js", "const s = `a", &mut js);
        highlight_code_line_with("js", "\\`still open", &mut js);
        assert!(js.open_string.is_some(), "JS: an escaped backtick does not close");

        let mut go = HighlightState::default();
        highlight_code_line_with("go", "s := `a", &mut go);
        highlight_code_line_with("go", "\\` closed", &mut go);
        assert!(go.open_string.is_none(), "Go raw strings have no escapes");
    }

    /// A backtick is an ordinary character in Rust and C — no carry, no colour change.
    #[test]
    fn backtick_is_inert_in_languages_without_backtick_literals() {
        for lang in ["rust", "c"] {
            let mut st = HighlightState::default();
            let line = "let x = 1; // see `foo`";
            assert_eq!(
                highlight_code_line_with(lang, line, &mut st),
                highlight_code_line(lang, line),
                "{lang} backtick line is unchanged"
            );
            assert!(st.open_string.is_none(), "{lang} must not carry a backtick");
        }
    }

    /// The stateless path stays byte-identical for literals that open and close on one
    /// line — the load-bearing promise that adding a carried fact changed nothing.
    #[test]
    fn single_line_raw_and_backtick_literals_are_byte_identical() {
        for (lang, line) in [
            ("rust", "let s = r#\"a }\"#; let n = 1;"),
            ("rust", "let s = r\"plain\"; let n = 1;"),
            ("js", "const s = `a ${b} c`; const n = 1;"),
            ("go", "s := `raw`; n := 1"),
        ] {
            let mut st = HighlightState::default();
            assert_eq!(
                highlight_code_line_with(lang, line, &mut st),
                highlight_code_line(lang, line),
                "{lang}: {line}"
            );
            assert!(st.open_string.is_none(), "{lang}: {line}");
        }
    }
}

#[cfg(test)]
mod scratch_probe {
    use super::*;
    #[test]
    fn probe() {
        let mut st = HighlightState::default();
        for l in ["let a = 1; /* open", "  body ✓ let", "still */ let b = 2;"] {
            println!(
                "{:?} -> {:?} depth={}",
                l,
                highlight_code_line_with("rust", l, &mut st),
                st.block_comment_depth
            );
        }
        println!(
            "stateless same-line: {:?}",
            highlight_code_line("rust", "let a = 1; /* c */ let b = 2;")
        );
        println!(
            "stateless line comment: {:?}",
            highlight_code_line("rust", "let a = 1; // c")
        );
        println!(
            "string with /*: {:?}",
            highlight_code_line("rust", "let s = \"/*\"; let b = 2;")
        );
    }
}
