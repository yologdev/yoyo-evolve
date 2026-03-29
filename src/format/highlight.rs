//! Syntax highlighting for code blocks (Rust, Python, JS, Go, etc.).

use super::*;

fn normalize_lang(lang: &str) -> Option<&'static str> {
    match lang.to_lowercase().as_str() {
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

/// Apply syntax-aware ANSI highlighting to a single code line.
///
/// Colorizes keywords (bold cyan), types (magenta), strings (green),
/// comments (dim), and numbers (yellow).
/// JSON keys are highlighted in cyan, YAML keys in bold yellow.
/// Falls back to DIM when language is unrecognized.
pub fn highlight_code_line(lang: &str, line: &str) -> String {
    let norm = match normalize_lang(lang) {
        Some(n) => n,
        None => return format!("{DIM}{line}{RESET}"),
    };

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
}
