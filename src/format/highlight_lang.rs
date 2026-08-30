//! Per-language tables and predicates for the syntax highlighter.
//!
//! Split verbatim out of `highlight.rs` on Day 174 to clear `MAX_MODULE_LINES`
//! (2048 lines, 48 over a cap that is fatal at +51). Pure move: no body was
//! rewritten and no table was "improved" on the way across, so the existing
//! highlighter tests — which all still live in `highlight.rs` — keep proving it.
//!
//! Everything here answers one question: *what is true of language X?* Nothing
//! here touches `HighlightState`, a `&mut` scanner, or an output string; that is
//! the seam the split was taken at.

pub(super) fn normalize_lang(lang: &str) -> Option<&'static str> {
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

/// Get the keyword list for a normalized language.
pub(super) fn lang_keywords(lang: &str) -> &'static [&'static str] {
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
pub(super) fn lang_types(lang: &str) -> &'static [&'static str] {
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
pub(super) fn comment_prefix(lang: &str) -> &'static str {
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
pub(super) fn supports_block_comments(norm: &str) -> bool {
    matches!(norm, "rust" | "js" | "go" | "c")
}

/// Do this language's block comments **nest**?
///
/// Rust's do: `/* /* */ */` is one comment and the first `*/` closes only the inner one.
/// The other C-family languages here do not nest, so their depth never climbs past 1 and
/// the first `*/` closes the comment.
pub(super) fn block_comments_nest(norm: &str) -> bool {
    norm == "rust"
}

/// Does a bare `"…"` literal in this language really span lines?
///
/// Rust: yes. In C/JS/Go a plain double-quoted string does not cross a newline (they use
/// backticks, template literals, or a `\` continuation — see [`backtick_strings`] for the
/// two that do), so those languages keep their byte-identical single-line behavior for
/// this shape rather than inheriting a Rust rule.
pub(super) fn multiline_strings(norm: &str) -> bool {
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
pub(super) fn backtick_strings(norm: &str) -> Option<bool> {
    match norm {
        "js" => Some(true),
        "go" => Some(false),
        _ => None,
    }
}

/// Does this language have **triple-quoted** literals (`"""…"""` / `'''…'''`) that span
/// lines?
///
/// Deliberately separate from [`multiline_strings`] rather than folded into it, because
/// the two ask different questions and the older one is *correct* for Python: a bare
/// `"…"` really does not span lines there. #865 was not a wrong row — it was a table with
/// no **vocabulary** for triple quotes at all, so a docstring body rendered as executable
/// code (a `#` opened an inline comment, `return` highlighted as a keyword).
///
/// Python only, on purpose. A row nothing exercises is a claim no test can grade, so
/// widening this to another language belongs in the task that brings that language's
/// fixtures with it.
pub(super) fn triple_quote_strings(norm: &str) -> bool {
    norm == "python"
}
