//! Formatting helpers: ANSI colors, cost, duration, tokens, context bar, truncation.

use std::io::{self, Write};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use yoagent::types::{Content, ToolResult};

// --- Color support with NO_COLOR and --no-color ---

/// Whether color output has been disabled (via NO_COLOR env or --no-color flag).
static COLOR_DISABLED: OnceLock<bool> = OnceLock::new();

/// Disable color output. Call before any formatting happens (e.g., from CLI arg parsing).
pub fn disable_color() {
    let _ = COLOR_DISABLED.set(true);
}

/// Check if color output is enabled. Cached after first call.
/// Respects the NO_COLOR environment variable (https://no-color.org/).
fn color_enabled() -> bool {
    !*COLOR_DISABLED.get_or_init(|| std::env::var("NO_COLOR").is_ok())
}

/// A color code that respects the NO_COLOR convention.
/// When color is disabled, formats as an empty string.
pub struct Color(pub &'static str);

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if color_enabled() {
            f.write_str(self.0)
        } else {
            Ok(())
        }
    }
}

// ANSI color helpers — respect NO_COLOR env var and --no-color flag
pub static RESET: Color = Color("\x1b[0m");
pub static BOLD: Color = Color("\x1b[1m");
pub static DIM: Color = Color("\x1b[2m");
pub static GREEN: Color = Color("\x1b[32m");
pub static YELLOW: Color = Color("\x1b[33m");
pub static CYAN: Color = Color("\x1b[36m");
pub static RED: Color = Color("\x1b[31m");
pub static MAGENTA: Color = Color("\x1b[35m");
pub static ITALIC: Color = Color("\x1b[3m");
pub static BOLD_ITALIC: Color = Color("\x1b[1;3m");
pub static BOLD_CYAN: Color = Color("\x1b[1;36m");
pub static BOLD_YELLOW: Color = Color("\x1b[1;33m");

// --- Syntax highlighting for code blocks ---

/// Languages recognized for syntax highlighting.
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
fn model_pricing(model: &str) -> Option<(f64, f64, f64, f64)> {
    // Returns (input_per_MTok, cache_write_per_MTok, cache_read_per_MTok, output_per_MTok)
    // For providers without caching, cache_write and cache_read are set to 0.0.

    // Strip common OpenRouter prefixes (e.g. "anthropic/claude-sonnet-4-20250514")
    let model = model
        .strip_prefix("anthropic/")
        .or_else(|| model.strip_prefix("openai/"))
        .or_else(|| model.strip_prefix("google/"))
        .or_else(|| model.strip_prefix("deepseek/"))
        .or_else(|| model.strip_prefix("mistralai/"))
        .or_else(|| model.strip_prefix("x-ai/"))
        .or_else(|| model.strip_prefix("meta-llama/"))
        .unwrap_or(model);

    // ── Anthropic ─────────────────────────────────────────────────────
    // https://docs.anthropic.com/en/about-claude/pricing
    if model.contains("opus") {
        if model.contains("4-6")
            || model.contains("4-5")
            || model.contains("4.6")
            || model.contains("4.5")
        {
            return Some((5.0, 6.25, 0.50, 25.0));
        } else {
            return Some((15.0, 18.75, 1.50, 75.0));
        }
    }
    if model.contains("sonnet") {
        return Some((3.0, 3.75, 0.30, 15.0));
    }
    if model.contains("haiku") {
        if model.contains("4-5") || model.contains("4.5") {
            return Some((1.0, 1.25, 0.10, 5.0));
        } else {
            return Some((0.80, 1.0, 0.08, 4.0));
        }
    }

    // ── OpenAI ────────────────────────────────────────────────────────
    // https://platform.openai.com/docs/pricing
    if model.starts_with("gpt-4.1") {
        if model.contains("mini") {
            return Some((0.40, 0.0, 0.0, 1.60)); // gpt-4.1-mini
        } else if model.contains("nano") {
            return Some((0.10, 0.0, 0.0, 0.40)); // gpt-4.1-nano
        } else {
            return Some((2.00, 0.0, 0.0, 8.00)); // gpt-4.1
        }
    }
    if model.starts_with("gpt-4o") {
        if model.contains("mini") {
            return Some((0.15, 0.0, 0.0, 0.60)); // gpt-4o-mini
        } else {
            return Some((2.50, 0.0, 0.0, 10.00)); // gpt-4o
        }
    }
    if model.starts_with("o4-mini") {
        return Some((1.10, 0.0, 0.0, 4.40));
    }
    if model.starts_with("o3-mini") {
        return Some((1.10, 0.0, 0.0, 4.40));
    }
    if model == "o3" {
        return Some((2.00, 0.0, 0.0, 8.00));
    }

    // ── Google Gemini ─────────────────────────────────────────────────
    // https://ai.google.dev/pricing
    if model.contains("gemini-2.5-pro") {
        return Some((1.25, 0.0, 0.0, 10.00));
    }
    if model.contains("gemini-2.5-flash") {
        return Some((0.15, 0.0, 0.0, 0.60));
    }
    if model.contains("gemini-2.0-flash") {
        return Some((0.10, 0.0, 0.0, 0.40));
    }

    // ── DeepSeek ──────────────────────────────────────────────────────
    // https://platform.deepseek.com/api-docs/pricing/
    if model.contains("deepseek-chat") || model.contains("deepseek-v3") {
        return Some((0.27, 0.0, 0.0, 1.10));
    }
    if model.contains("deepseek-reasoner") || model.contains("deepseek-r1") {
        return Some((0.55, 0.0, 0.0, 2.19));
    }

    // ── Mistral ───────────────────────────────────────────────────────
    // https://mistral.ai/products#pricing
    if model.contains("mistral-large") {
        return Some((2.00, 0.0, 0.0, 6.00));
    }
    if model.contains("mistral-small") || model.contains("mistral-latest") {
        return Some((0.10, 0.0, 0.0, 0.30));
    }
    if model.contains("codestral") {
        return Some((0.30, 0.0, 0.0, 0.90));
    }

    // ── xAI (Grok) ───────────────────────────────────────────────────
    // https://docs.x.ai/docs/models#models-and-pricing
    if model.contains("grok-3") {
        if model.contains("mini") {
            return Some((0.30, 0.0, 0.0, 0.50));
        } else {
            return Some((3.00, 0.0, 0.0, 15.00));
        }
    }
    if model.contains("grok-2") {
        return Some((2.00, 0.0, 0.0, 10.00));
    }

    // ── ZAI (Zhipu AI / z.ai) ────────────────────────────────────────
    // https://open.bigmodel.cn/pricing — prices converted from CNY to USD
    if model.contains("glm-4-plus") || model.contains("glm-4.7") {
        return Some((0.70, 0.0, 0.0, 0.70));
    }
    if model.contains("glm-4-air") || model.contains("glm-4.5-air") {
        return Some((0.07, 0.0, 0.0, 0.07));
    }
    if model.contains("glm-4-flash") || model.contains("glm-4.5-flash") {
        return Some((0.01, 0.0, 0.0, 0.01));
    }
    if model.contains("glm-4-long") {
        return Some((0.14, 0.0, 0.0, 0.14));
    }
    if model.contains("glm-5") {
        return Some((0.70, 0.0, 0.0, 0.70));
    }

    // ── Groq (hosted models) ─────────────────────────────────────────
    // https://groq.com/pricing/
    if model.contains("llama-3.3-70b") || model.contains("llama3-70b") {
        return Some((0.59, 0.0, 0.0, 0.79));
    }
    if model.contains("llama-3.1-8b") || model.contains("llama3-8b") {
        return Some((0.05, 0.0, 0.0, 0.08));
    }
    if model.contains("mixtral-8x7b") {
        return Some((0.24, 0.0, 0.0, 0.24));
    }
    if model.contains("gemma2-9b") {
        return Some((0.20, 0.0, 0.0, 0.20));
    }

    None
}

/// Estimate cost in USD for a given usage and model.
/// Returns None if the model pricing is unknown.
pub fn estimate_cost(usage: &yoagent::Usage, model: &str) -> Option<f64> {
    let (input_cost, cw_cost, cr_cost, output_cost) = cost_breakdown(usage, model)?;
    Some(input_cost + cw_cost + cr_cost + output_cost)
}

/// Get individual cost components for a usage and model.
/// Returns (input_cost, cache_write_cost, cache_read_cost, output_cost) or None if model unknown.
pub fn cost_breakdown(usage: &yoagent::Usage, model: &str) -> Option<(f64, f64, f64, f64)> {
    let (input_per_m, cache_write_per_m, cache_read_per_m, output_per_m) = model_pricing(model)?;

    let input_cost = usage.input as f64 * input_per_m / 1_000_000.0;
    let cache_write_cost = usage.cache_write as f64 * cache_write_per_m / 1_000_000.0;
    let cache_read_cost = usage.cache_read as f64 * cache_read_per_m / 1_000_000.0;
    let output_cost = usage.output as f64 * output_per_m / 1_000_000.0;

    Some((input_cost, cache_write_cost, cache_read_cost, output_cost))
}

/// Format a cost in USD for display (e.g., "$0.0042", "$1.23").
pub fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else if cost < 1.0 {
        format!("${:.3}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

/// Format a duration for display (e.g., "1.2s", "350ms", "2m 15s").
pub fn format_duration(d: std::time::Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1000;
        format!("{mins}m {secs}s")
    }
}

/// Format a token count for display (e.g., 1500 -> "1.5k", 1000000 -> "1.0M").
pub fn format_token_count(count: u64) -> String {
    if count < 1000 {
        format!("{count}")
    } else if count < 1_000_000 {
        format!("{:.1}k", count as f64 / 1000.0)
    } else {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    }
}

/// Build a context usage bar (e.g., "████████░░░░░░░░░░░░ 40%").
pub fn context_bar(used: u64, max: u64) -> String {
    let pct = if max == 0 {
        0.0
    } else {
        (used as f64 / max as f64).min(1.0)
    };
    let width = 20;
    let filled = (pct * width as f64).round() as usize;
    let empty = width - filled;
    let bar: String = "█".repeat(filled) + &"░".repeat(empty);
    format!("{bar} {:.0}%", pct * 100.0)
}

/// Truncate a string with an ellipsis if it exceeds `max` characters.
/// Return the correct singular or plural form of a word based on count.
///
/// `pluralize(1, "line", "lines")` → `"line"`
/// `pluralize(3, "line", "lines")` → `"lines"`
pub fn pluralize<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

pub fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

/// Decode HTML entities in a string.
///
/// Handles named entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, `&#39;`,
/// `&nbsp;`, `&#x27;`, `&mdash;`, `&ndash;`, `&hellip;`, `&copy;`, `&reg;`)
/// and numeric entities (decimal `&#NNN;` and hex `&#xHH;`).
pub fn decode_html_entities(s: &str) -> String {
    // First pass: named entities
    let s = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&copy;", "©")
        .replace("&reg;", "®");

    // Second pass: remaining numeric entities (&#NNN; and &#xHH;)
    let mut decoded = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' && chars.peek() == Some(&'#') {
            let mut entity = String::from("&#");
            chars.next(); // consume '#'
            while let Some(&nc) = chars.peek() {
                if nc == ';' {
                    chars.next();
                    break;
                }
                entity.push(nc);
                chars.next();
            }
            let num_str = &entity[2..];
            let parsed = if let Some(hex) = num_str.strip_prefix('x').or(num_str.strip_prefix('X'))
            {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num_str.parse::<u32>().ok()
            };
            if let Some(ch) = parsed.and_then(char::from_u32) {
                decoded.push(ch);
            } else {
                // Failed to decode — emit original
                decoded.push_str(&entity);
                decoded.push(';');
            }
        } else {
            decoded.push(c);
        }
    }

    decoded
}

/// Default character threshold for tool output truncation.
/// Outputs longer than this get the head/tail treatment.
pub const TOOL_OUTPUT_MAX_CHARS: usize = 30_000;

/// Number of lines to keep from the start of truncated output.
const TRUNCATION_HEAD_LINES: usize = 100;

/// Number of lines to keep from the end of truncated output.
const TRUNCATION_TAIL_LINES: usize = 50;

/// Intelligently truncate large tool output to save context window tokens.
///
/// When output exceeds `max_chars`, keeps the first ~100 lines and last ~50 lines
/// with a clear `[... truncated N lines ...]` marker in between. This preserves
/// the beginning of output (usually the most informative — headers, first errors)
/// and the end (summary lines, final status).
///
/// Output under the threshold is returned unchanged.
pub fn truncate_tool_output(output: &str, max_chars: usize) -> String {
    // Under threshold — return unchanged
    if output.len() <= max_chars {
        return output.to_string();
    }

    let lines: Vec<&str> = output.lines().collect();
    let total_lines = lines.len();

    // If not enough lines to meaningfully truncate, return as-is
    // (edge case: very long single lines or very few lines)
    if total_lines <= TRUNCATION_HEAD_LINES + TRUNCATION_TAIL_LINES {
        return output.to_string();
    }

    let head = &lines[..TRUNCATION_HEAD_LINES];
    let tail = &lines[total_lines - TRUNCATION_TAIL_LINES..];
    let omitted = total_lines - TRUNCATION_HEAD_LINES - TRUNCATION_TAIL_LINES;

    let mut result = String::with_capacity(max_chars);
    for line in head {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(&format!(
        "\n[... truncated {omitted} {} ...]\n\n",
        pluralize(omitted, "line", "lines")
    ));
    for (i, line) in tail.iter().enumerate() {
        result.push_str(line);
        if i < tail.len() - 1 {
            result.push('\n');
        }
    }

    result
}

// --- Section headers and dividers for visual hierarchy ---

/// Get the terminal width from the COLUMNS environment variable, falling back to 80.
fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(80)
}

/// Format a summary line for a batch of tool executions within a single turn.
///
/// Example output: `  3 tools completed in 1.2s (3 ✓, 0 ✗)`
/// When all succeed: `  3 tools completed in 1.2s (3 ✓)`
/// When some fail: `  3 tools completed in 1.2s (2 ✓, 1 ✗)`
/// Single tool batches return empty (not worth summarizing).
pub fn format_tool_batch_summary(
    total: usize,
    succeeded: usize,
    failed: usize,
    total_duration: std::time::Duration,
) -> String {
    if total <= 1 {
        return String::new();
    }
    let dur = format_duration(total_duration);
    let tool_word = pluralize(total, "tool", "tools");
    let status = if failed == 0 {
        format!("{succeeded} {GREEN}✓{RESET}")
    } else {
        format!("{succeeded} {GREEN}✓{RESET}, {failed} {RED}✗{RESET}")
    };
    format!("{DIM}  {total} {tool_word} completed in {dur}{RESET} ({status})")
}

/// Indent multi-line tool output under its tool header.
///
/// Each line of output gets a `    │ ` prefix for visual nesting.
/// Single-line output is returned as-is with the prefix.
/// Empty input returns empty string.
pub fn indent_tool_output(output: &str) -> String {
    if output.is_empty() {
        return String::new();
    }
    output
        .lines()
        .map(|line| format!("{DIM}    │ {RESET}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render a turn boundary marker between agent turns.
///
/// Shows a subtle visual separator so users can distinguish
/// when the agent starts a new reasoning/action cycle.
/// Example: `  ╭─ Turn 3 ──────────────────────────╮`
pub fn turn_boundary(turn_number: usize) -> String {
    let width = terminal_width();
    let label = format!(" Turn {turn_number} ");
    let prefix = "  ╭─";
    let suffix = "╮";
    let used = prefix.len() + label.len() + suffix.len();
    let fill = width.saturating_sub(used);
    let trail = "─".repeat(fill);
    format!("{DIM}{prefix}{label}{trail}{suffix}{RESET}")
}

/// Render a labeled section header, e.g. `── Thinking ──────────────────────────`
/// Uses DIM style and thin box-drawing characters (─).
/// The label is centered between two runs of ─ characters.
pub fn section_header(label: &str) -> String {
    let width = terminal_width();
    if label.is_empty() {
        return section_divider();
    }
    // Format: "── Label ─────────..."
    let prefix = "── ";
    let separator = " ";
    let used = prefix.len() + label.len() + separator.len();
    let remaining = width.saturating_sub(used);
    let trail = "─".repeat(remaining);
    format!("{DIM}{prefix}{label}{separator}{trail}{RESET}")
}

/// Render a plain thin divider line: `──────────────────────────────────────`
/// Uses DIM style and thin box-drawing characters (─).
pub fn section_divider() -> String {
    let width = terminal_width();
    format!("{DIM}{}{RESET}", "─".repeat(width))
}

/// Truncate a string to `max` characters (no ellipsis).
#[cfg(test)]
pub fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Maximum number of diff lines to display before truncating.
const MAX_DIFF_LINES: usize = 20;

/// Format a colored unified diff between old_text and new_text.
/// Removed lines are shown in red with `- ` prefix, added lines in green with `+ ` prefix.
/// If the diff exceeds `MAX_DIFF_LINES`, it is truncated with an ellipsis note.
pub fn format_edit_diff(old_text: &str, new_text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Show removed lines (old_text)
    if !old_text.is_empty() {
        for line in old_text.lines() {
            lines.push(format!("{RED}  - {line}{RESET}"));
        }
    }

    // Show added lines (new_text)
    if !new_text.is_empty() {
        for line in new_text.lines() {
            lines.push(format!("{GREEN}  + {line}{RESET}"));
        }
    }

    if lines.is_empty() {
        return String::new();
    }

    // Truncate if too many lines
    if lines.len() > MAX_DIFF_LINES {
        let remaining = lines.len() - MAX_DIFF_LINES;
        lines.truncate(MAX_DIFF_LINES);
        lines.push(format!("{DIM}  ... ({remaining} more lines){RESET}"));
    }

    lines.join("\n")
}

/// Format a human-readable summary for a tool execution.
///
/// Each tool gets a concise one-line description showing the key parameters:
/// - `bash` — `$ <command>` (first line + line count for multi-line scripts)
/// - `read_file` — `read <path>` with optional `:offset..end` or `(N lines)` range
/// - `write_file` — `write <path> (N lines)`
/// - `edit_file` — `edit <path> (old → new lines)`
/// - `list_files` — `ls <path> (pattern)`
/// - `search` — `search 'pattern' in <path> (include)`
pub fn format_tool_summary(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "bash" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            let line_count = cmd.lines().count();
            let first_line = cmd.lines().next().unwrap_or("...");
            if line_count > 1 {
                format!(
                    "$ {} ({line_count} lines)",
                    truncate_with_ellipsis(first_line, 60)
                )
            } else {
                format!("$ {}", truncate_with_ellipsis(cmd, 80))
            }
        }
        "read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let offset = args.get("offset").and_then(|v| v.as_u64());
            let limit = args.get("limit").and_then(|v| v.as_u64());
            match (offset, limit) {
                (Some(off), Some(lim)) => {
                    format!("read {path}:{off}..{}", off + lim)
                }
                (Some(off), None) => {
                    format!("read {path}:{off}..")
                }
                (None, Some(lim)) => {
                    let word = pluralize(lim as usize, "line", "lines");
                    format!("read {path} ({lim} {word})")
                }
                (None, None) => {
                    format!("read {path}")
                }
            }
        }
        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let line_info = args
                .get("content")
                .and_then(|v| v.as_str())
                .map(|c| {
                    let count = c.lines().count();
                    let word = pluralize(count, "line", "lines");
                    format!(" ({count} {word})")
                })
                .unwrap_or_default();
            format!("write {path}{line_info}")
        }
        "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let old_text = args.get("old_text").and_then(|v| v.as_str());
            let new_text = args.get("new_text").and_then(|v| v.as_str());
            match (old_text, new_text) {
                (Some(old), Some(new)) => {
                    let old_lines = old.lines().count();
                    let new_lines = new.lines().count();
                    format!("edit {path} ({old_lines} → {new_lines} lines)")
                }
                _ => format!("edit {path}"),
            }
        }
        "list_files" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let pattern = args.get("pattern").and_then(|v| v.as_str());
            match pattern {
                Some(pat) => format!("ls {path} ({pat})"),
                None => format!("ls {path}"),
            }
        }
        "search" => {
            let pat = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            let search_path = args.get("path").and_then(|v| v.as_str());
            let include = args.get("include").and_then(|v| v.as_str());
            let mut summary = format!("search '{}'", truncate_with_ellipsis(pat, 60));
            if let Some(p) = search_path {
                summary.push_str(&format!(" in {p}"));
            }
            if let Some(inc) = include {
                summary.push_str(&format!(" ({inc})"));
            }
            summary
        }
        _ => tool_name.to_string(),
    }
}

/// Print usage stats after a prompt response.
pub fn print_usage(
    usage: &yoagent::Usage,
    total: &yoagent::Usage,
    model: &str,
    elapsed: std::time::Duration,
) {
    if usage.input > 0 || usage.output > 0 {
        let cache_info = if usage.cache_read > 0 || usage.cache_write > 0 {
            format!(
                "  [cache: {} read, {} write]",
                usage.cache_read, usage.cache_write
            )
        } else {
            String::new()
        };
        let cost_info = estimate_cost(usage, model)
            .map(|c| format!("  cost: {}", format_cost(c)))
            .unwrap_or_default();
        let total_cost_info = estimate_cost(total, model)
            .map(|c| format!("  total: {}", format_cost(c)))
            .unwrap_or_default();
        let elapsed_str = format_duration(elapsed);
        println!(
            "\n{DIM}  tokens: {} in / {} out{cache_info}  (session: {} in / {} out){cost_info}{total_cost_info}  ⏱ {elapsed_str}{RESET}",
            usage.input, usage.output, total.input, total.output
        );
    }
}

/// Incremental markdown renderer for streamed text output.
/// Tracks state across partial deltas to apply ANSI formatting for
/// code blocks, inline code, bold text, and headers.
///
/// Designed for LLM streaming: mid-line tokens are rendered immediately
/// with inline formatting. Only line boundaries buffer briefly to detect
/// code fences (`` ``` ``) and headers (`#`).
pub struct MarkdownRenderer {
    in_code_block: bool,
    code_lang: Option<String>,
    line_buffer: String,
    /// Whether we're at the start of a new line (need to detect fence/header).
    line_start: bool,
    /// When a block element prefix (list marker, blockquote `>`) has been rendered
    /// early for streaming, this tracks the prefix so we don't re-render on newline.
    /// Once set, subsequent tokens stream as inline text until the newline arrives.
    block_prefix_rendered: bool,
}

impl MarkdownRenderer {
    /// Create a new renderer with empty state.
    pub fn new() -> Self {
        Self {
            in_code_block: false,
            code_lang: None,
            line_buffer: String::new(),
            line_start: true,
            block_prefix_rendered: false,
        }
    }

    /// Process a delta chunk and return ANSI-formatted output.
    ///
    /// **Streaming behavior:**
    /// - At line start, buffers briefly to detect code fences/headers (typically 1–4 chars)
    /// - At line start with word boundary (text + trailing space), flushes via
    ///   `flush_on_whitespace()` for word-by-word prose streaming
    /// - Mid-line, renders immediately with inline formatting (bold, inline code)
    /// - Complete lines (ending with `\n`) are always processed immediately
    ///
    /// ## render_latency_budget
    ///
    /// The renderer is designed for minimal token-to-display latency:
    ///
    /// | Path                    | Buffering             | Expected latency |
    /// |-------------------------|-----------------------|------------------|
    /// | Mid-line text           | None (immediate)      | ~0 (no alloc)    |
    /// | Mid-line code block     | None (immediate)      | ~0 (dim wrap)    |
    /// | Line-start, non-special | Flush after 1 char    | ~0               |
    /// | Line-start, word boundary | Flush on whitespace | ~1 token         |
    /// | Line-start, ambiguous   | Buffer 1–4 chars      | 1 token          |
    /// | Line-start, code block  | Buffer until non-`\`` | 1 token          |
    ///
    /// **Flush contract:** Every call to `render_delta()` that produces output
    /// expects the caller to call `io::stdout().flush()` immediately after
    /// printing. This ensures tokens appear on screen without stdio batching.
    /// The caller in `prompt.rs::handle_events()` does this after every delta.
    ///
    /// **Do not regress:** Adding new buffering paths (e.g., for tables or
    /// footnotes) must preserve the mid-line fast path. Any change that causes
    /// mid-line tokens to return empty strings is a latency regression.
    pub fn render_delta(&mut self, delta: &str) -> String {
        let mut output = String::new();

        // Mid-line fast paths: render tokens immediately without buffering.
        // Code fences and headers only matter at line start, so mid-line is safe.
        if !self.line_start {
            if self.in_code_block {
                // Mid-line inside a code block: emit tokens immediately with
                // appropriate styling (dim or syntax-highlighted) instead of
                // buffering until a complete line. This gives token-by-token
                // streaming for code blocks (issue #147).
                if let Some(newline_pos) = delta.find('\n') {
                    let mid_line_part = &delta[..newline_pos];
                    if !mid_line_part.is_empty() {
                        output.push_str(&self.render_code_inline(mid_line_part));
                    }
                    output.push('\n');
                    self.line_start = true;
                    self.block_prefix_rendered = false;

                    // Process the rest (after the first \n) via buffered path
                    // because we're now at line start and need fence detection.
                    let rest = &delta[newline_pos + 1..];
                    if !rest.is_empty() {
                        output.push_str(&self.render_delta_buffered(rest));
                    }
                } else {
                    // No newline — pure mid-line code content, render immediately
                    output.push_str(&self.render_code_inline(delta));
                }
                return output;
            }

            // Mid-line outside a code block: render with inline formatting
            if let Some(newline_pos) = delta.find('\n') {
                // Render the mid-line portion immediately
                let mid_line_part = &delta[..newline_pos];
                if !mid_line_part.is_empty() {
                    output.push_str(&self.render_inline(mid_line_part));
                }
                output.push('\n');
                self.line_start = true;
                self.block_prefix_rendered = false;

                // Process the rest (after the first \n) by buffering
                let rest = &delta[newline_pos + 1..];
                if !rest.is_empty() {
                    output.push_str(&self.render_delta_buffered(rest));
                }
            } else {
                // No newline — pure mid-line content, render immediately
                output.push_str(&self.render_inline(delta));
            }
            return output;
        }

        // We're at line start — use buffered approach (needed to detect fences, headers)
        output.push_str(&self.render_delta_buffered(delta));
        output
    }

    /// Render a code block fragment with dim styling for immediate streaming.
    /// Used for mid-line token-by-token output inside code blocks.
    /// Full syntax highlighting is applied to complete lines (at newline boundaries);
    /// fragments get dim styling for responsiveness.
    fn render_code_inline(&self, text: &str) -> String {
        format!("{DIM}{text}{RESET}")
    }

    /// Buffered rendering: adds delta to line_buffer, processes complete lines,
    /// and attempts early flush of line-start content when safe.
    ///
    /// render_latency_budget: This path is only entered at line start. The buffer
    /// holds at most 1–4 characters before resolving. The `needs_line_buffering()`
    /// check and `try_resolve_block_prefix()` aim to flush as early as possible,
    /// switching to the mid-line fast path for subsequent tokens.
    fn render_delta_buffered(&mut self, delta: &str) -> String {
        let mut output = String::new();
        self.line_buffer.push_str(delta);

        // Process all complete lines (those ending with \n)
        while let Some(newline_pos) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..newline_pos].to_string();
            self.line_buffer = self.line_buffer[newline_pos + 1..].to_string();

            if self.block_prefix_rendered {
                // The prefix (bullet, quote marker, etc.) was already rendered.
                // Just render remaining content as inline text.
                output.push_str(&self.render_inline(&line));
            } else {
                output.push_str(&self.render_line(&line));
            }
            output.push('\n');
            self.line_start = true;
            self.block_prefix_rendered = false;
        }

        // Try to resolve the line-start buffer early:
        // If we have enough characters to determine it's NOT a fence, header,
        // or other block-level construct (list, blockquote, hr), flush as inline text.
        if self.line_start && !self.line_buffer.is_empty() && !self.in_code_block {
            if !self.needs_line_buffering() {
                // Definitely not a fence, header, or block element — flush as inline text
                let buf = std::mem::take(&mut self.line_buffer);
                output.push_str(&self.render_inline(&buf));
                self.line_start = false;
            } else {
                // Check if we can confirm a block element and render its prefix early,
                // switching to mid-line streaming for subsequent tokens.
                let prefix_output = self.try_resolve_block_prefix();
                if !prefix_output.is_empty() {
                    output.push_str(&prefix_output);
                } else {
                    // Still ambiguous from needs_line_buffering(), but if we've
                    // accumulated a word boundary (text + trailing whitespace), the
                    // content can't be a fence/header prefix — flush it now.
                    // This gives word-by-word streaming for prose that starts with
                    // characters that trigger buffering (e.g., digits, dashes).
                    output.push_str(&self.flush_on_whitespace());
                }
            }
        }

        // Inside a code block at line start: early-resolve when content can't be a
        // closing fence. Only ``` matters here (no headers, lists, etc.). Once we
        // know it's not a fence, flush as code content and set line_start=false so
        // subsequent tokens stream immediately via the mid-line fast path (issue #147).
        //
        // render_latency_budget: In CommonMark, a closing fence can have 0–3 spaces
        // of indentation. Content with >3 leading spaces or any non-backtick first
        // non-space char is guaranteed not to be a fence and resolves immediately.
        if self.line_start && !self.line_buffer.is_empty() && self.in_code_block {
            let leading_spaces = self.line_buffer.len() - self.line_buffer.trim_start().len();
            let trimmed = self.line_buffer.trim_start();

            let could_be_fence = if leading_spaces > 3 {
                // >3 spaces of indentation — can't be a closing fence per CommonMark
                false
            } else {
                trimmed.is_empty() || trimmed.starts_with('`') || "`".starts_with(trimmed)
            };

            if !could_be_fence {
                // Definitely not a closing fence — flush as code content immediately
                let buf = std::mem::take(&mut self.line_buffer);
                output.push_str(&self.render_code_inline(&buf));
                self.line_start = false;
            }
        }

        output
    }

    /// Check if the current line_buffer content at line start still needs buffering
    /// because it could be a markdown control sequence (fence, header, block element).
    /// Returns false when the content is definitely plain text and can be flushed.
    fn needs_line_buffering(&self) -> bool {
        let trimmed = self.line_buffer.trim_start();
        if trimmed.is_empty() {
            return true;
        }

        let could_be_fence = trimmed.starts_with('`') || "`".starts_with(trimmed);
        let could_be_header = trimmed.starts_with('#') || "#".starts_with(trimmed);

        if could_be_fence || could_be_header {
            return true;
        }

        // Check for block-level constructs
        let first = trimmed.as_bytes()[0];
        match first {
            b'>' => true, // blockquote — always a block element
            b'+' => trimmed.len() < 2 || trimmed.starts_with("+ "),
            b'-' => {
                // Quick disambiguation: "-" followed by a non-space, non-dash char
                // can't be a list item ("- ") or horizontal rule ("---").
                // "-based", "-flag" → flush immediately. "- item", "--" → keep buffering.
                if trimmed.len() >= 2 {
                    let second = trimmed.as_bytes()[1];
                    if second != b' ' && second != b'-' {
                        return false;
                    }
                }
                trimmed.len() < 2 || trimmed.starts_with("- ") || {
                    let no_sp: String = trimmed.chars().filter(|c| *c != ' ').collect();
                    !no_sp.is_empty() && no_sp.chars().all(|c| c == '-')
                }
            }
            b'*' => {
                trimmed.len() < 2 || trimmed.starts_with("* ") || {
                    let no_sp: String = trimmed.chars().filter(|c| *c != ' ').collect();
                    !no_sp.is_empty() && no_sp.chars().all(|c| c == '*')
                }
            }
            b'_' => {
                trimmed.len() < 3 || {
                    let no_sp: String = trimmed.chars().filter(|c| *c != ' ').collect();
                    !no_sp.is_empty() && no_sp.chars().all(|c| c == '_')
                }
            }
            b'0'..=b'9' => {
                // Quick disambiguation: if we have at least 2 chars and the first
                // non-digit char isn't '.' or ')', it can't be a numbered list —
                // flush immediately. "2nd", "3rd", "100ms" → flush.
                // "1.", "1)", "12" (all digits) → keep buffering.
                if trimmed.len() >= 2 {
                    if let Some(pos) = trimmed.bytes().position(|b| !b.is_ascii_digit()) {
                        let non_digit = trimmed.as_bytes()[pos];
                        if non_digit != b'.' && non_digit != b')' {
                            return false; // Not a numbered list pattern
                        }
                    }
                    // All digits so far, or digit(s) followed by ./), keep buffering
                }
                trimmed.len() < 3
                    || trimmed.contains(". ")
                        && trimmed[..trimmed.find(". ").unwrap_or(0)]
                            .chars()
                            .all(|c| c.is_ascii_digit())
            }
            b'|' => true, // table row
            _ => false,
        }
    }

    /// Try to resolve a confirmed block element prefix and render it immediately.
    /// When successful, renders the prefix (bullet, quote marker, etc.) and sets
    /// `line_start = false` so subsequent tokens stream via the mid-line fast path.
    /// Returns any rendered output.
    fn try_resolve_block_prefix(&mut self) -> String {
        let trimmed = self.line_buffer.trim_start();
        if trimmed.is_empty() {
            return String::new();
        }

        let first = trimmed.as_bytes()[0];

        // Blockquote: ">" or "> " confirmed — render prefix, stream rest
        if first == b'>' {
            let rest = trimmed.strip_prefix('>').unwrap_or("");
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            let prefix_output = format!("{DIM}│{RESET} {ITALIC}");
            let rest_output = if !rest.is_empty() {
                self.render_inline(rest)
            } else {
                String::new()
            };
            self.line_buffer.clear();
            self.line_start = false;
            self.block_prefix_rendered = true;
            return format!("{prefix_output}{rest_output}");
        }

        // Unordered list: confirmed when we see "- X", "* X", "+ X"
        // where X is NOT a continuation of a horizontal rule
        if let Some(content) = self.try_confirm_unordered_list(trimmed) {
            let indent = Self::leading_whitespace(&self.line_buffer);
            let content_output = if !content.is_empty() {
                self.render_inline(content)
            } else {
                String::new()
            };
            let prefix_output = format!("{indent}{CYAN}•{RESET} {content_output}");
            self.line_buffer.clear();
            self.line_start = false;
            self.block_prefix_rendered = true;
            return prefix_output;
        }

        // Ordered list: confirmed when we see "N. " with content
        if let Some((num, content)) = self.try_confirm_ordered_list(trimmed) {
            let indent = Self::leading_whitespace(&self.line_buffer);
            let content_output = if !content.is_empty() {
                self.render_inline(content)
            } else {
                String::new()
            };
            let prefix_output = format!("{indent}{CYAN}{num}.{RESET} {content_output}");
            self.line_buffer.clear();
            self.line_start = false;
            self.block_prefix_rendered = true;
            return prefix_output;
        }

        String::new()
    }

    /// Try to confirm an unordered list item and return the content after the marker.
    /// Only confirms when we have enough content to rule out a horizontal rule.
    /// For "- ", confirms when a non-dash non-space character follows.
    /// For "* ", confirms when a non-star non-space character follows.
    /// For "+ ", always a list item (no ambiguity with HR).
    fn try_confirm_unordered_list<'a>(&self, trimmed: &'a str) -> Option<&'a str> {
        // "+ X" — always a list item
        if let Some(rest) = trimmed.strip_prefix("+ ") {
            if !rest.is_empty() {
                return Some(rest);
            }
            // "+ " alone: still ambiguous (could get more dashes), but "+ " is a list
            return Some(rest);
        }

        // "- X" — list item if X contains a non-dash, non-space char
        if let Some(rest) = trimmed.strip_prefix("- ") {
            if !rest.is_empty() && rest.chars().any(|c| c != '-' && c != ' ') {
                return Some(rest);
            }
            return None; // Could still be "- - -" horizontal rule
        }

        // "* X" — list item if X contains a non-star, non-space char
        if let Some(rest) = trimmed.strip_prefix("* ") {
            if !rest.is_empty() && rest.chars().any(|c| c != '*' && c != ' ') {
                return Some(rest);
            }
            return None; // Could still be "* * *" horizontal rule
        }

        None
    }

    /// Try to confirm an ordered list item and return (number, content).
    /// Confirms when we see "N. " followed by actual content.
    fn try_confirm_ordered_list<'a>(&self, trimmed: &'a str) -> Option<(&'a str, &'a str)> {
        let dot_space = trimmed.find(". ")?;
        let num_part = &trimmed[..dot_space];
        if num_part.is_empty() || !num_part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let content = &trimmed[dot_space + 2..];
        if content.is_empty() {
            return None; // Haven't seen content yet
        }
        Some((num_part, content))
    }

    /// Flush the line buffer when it contains a word boundary (whitespace after text).
    ///
    /// This improves perceived streaming performance: when the buffer has accumulated
    /// something like `"The "` or `"Hello world "`, the trailing whitespace proves it
    /// can't be a fence/header prefix (those never have spaces after the control chars
    /// without first being resolved by `try_resolve_block_prefix`). So we flush the
    /// buffer as inline text and switch to the mid-line fast path.
    ///
    /// **Safety:** Does NOT flush when the trimmed buffer starts with `#` or `` ` ``
    /// (potential header/fence), or with block-level markers (`>`, `-`, `*`, `+`,
    /// digits) — those are handled by `needs_line_buffering`/`try_resolve_block_prefix`.
    ///
    /// Returns rendered output if flushed, empty string otherwise.
    pub fn flush_on_whitespace(&mut self) -> String {
        if !self.line_start || self.line_buffer.is_empty() || self.in_code_block {
            return String::new();
        }

        // Check if the buffer ends with whitespace and has non-whitespace content.
        let has_non_ws = self.line_buffer.chars().any(|c| !c.is_whitespace());
        let ends_with_ws = self
            .line_buffer
            .chars()
            .last()
            .map(|c| c.is_whitespace())
            .unwrap_or(false);

        if !has_non_ws || !ends_with_ws {
            return String::new();
        }

        // Don't flush if the content could still be a markdown control sequence.
        // Headers (#), fences (`), block elements (>, -, *, +, digits) need to
        // keep buffering — they're handled by the dedicated resolution paths.
        let trimmed = self.line_buffer.trim_start();
        if !trimmed.is_empty() {
            let first = trimmed.as_bytes()[0];
            match first {
                b'#' | b'`' | b'>' | b'-' | b'*' | b'+' | b'_' | b'|' => return String::new(),
                b'0'..=b'9' => return String::new(),
                _ => {}
            }
        }

        let buf = std::mem::take(&mut self.line_buffer);
        let output = self.render_inline(&buf);
        self.line_start = false;
        output
    }

    /// Flush any remaining buffered content (call after stream ends).
    pub fn flush(&mut self) -> String {
        if self.line_buffer.is_empty() {
            if self.block_prefix_rendered {
                // Close any open italic from blockquote prefix
                self.block_prefix_rendered = false;
                return format!("{RESET}");
            }
            return String::new();
        }
        let line = std::mem::take(&mut self.line_buffer);
        self.line_start = true;
        if self.block_prefix_rendered {
            self.block_prefix_rendered = false;
            // Prefix already rendered — just render remaining inline content
            let formatted = self.render_inline(&line);
            return format!("{formatted}{RESET}");
        }
        self.render_line(&line)
    }

    /// Render a single complete line, updating state for code fences.
    fn render_line(&mut self, line: &str) -> String {
        let trimmed = line.trim();
        // After rendering a complete line, next content will be at line start
        self.line_start = true;
        self.block_prefix_rendered = false;

        // Check for code fence (``` with optional language)
        if let Some(after_fence) = trimmed.strip_prefix("```") {
            if self.in_code_block {
                // Closing fence
                self.in_code_block = false;
                self.code_lang = None;
                return format!("{DIM}{line}{RESET}");
            } else {
                // Opening fence — capture language if present
                self.in_code_block = true;
                let lang = after_fence.trim();
                self.code_lang = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };
                return format!("{DIM}{line}{RESET}");
            }
        }

        if self.in_code_block {
            // Code block content: syntax highlight if language is known, else dim
            return if let Some(ref lang) = self.code_lang {
                highlight_code_line(lang, line)
            } else {
                format!("{DIM}{line}{RESET}")
            };
        }

        // Header: # at line start → BOLD+CYAN
        if trimmed.starts_with('#') {
            return format!("{BOLD}{CYAN}{line}{RESET}");
        }

        // Horizontal rule: ---, ***, ___ (3+ of the same char, possibly with spaces)
        if Self::is_horizontal_rule(trimmed) {
            let width = 40;
            return format!("{DIM}{}{RESET}", "─".repeat(width));
        }

        // Blockquote: > at line start
        if let Some(rest) = trimmed.strip_prefix('>') {
            let content = rest.strip_prefix(' ').unwrap_or(rest);
            let formatted = self.render_inline(content);
            return format!("{DIM}│{RESET} {ITALIC}{formatted}{RESET}");
        }

        // Unordered list: lines starting with - , * , or +  (with optional leading whitespace)
        if let Some(content) = Self::strip_unordered_list_marker(trimmed) {
            let indent = Self::leading_whitespace(line);
            let formatted = self.render_inline(content);
            return format!("{indent}{CYAN}•{RESET} {formatted}");
        }

        // Ordered list: lines matching N. text
        if let Some((num, content)) = Self::strip_ordered_list_marker(trimmed) {
            let indent = Self::leading_whitespace(line);
            let formatted = self.render_inline(content);
            return format!("{indent}{CYAN}{num}.{RESET} {formatted}");
        }

        // Apply inline formatting for normal text
        self.render_inline(line)
    }

    /// Check if a trimmed line is a horizontal rule (---, ***, ___, 3+ chars).
    fn is_horizontal_rule(trimmed: &str) -> bool {
        if trimmed.len() < 3 {
            return false;
        }
        let no_spaces: String = trimmed.chars().filter(|c| *c != ' ').collect();
        if no_spaces.len() < 3 {
            return false;
        }
        let first = no_spaces.chars().next().unwrap();
        (first == '-' || first == '*' || first == '_') && no_spaces.chars().all(|c| c == first)
    }

    /// Strip an unordered list marker (- , * , + ) and return the content after it.
    fn strip_unordered_list_marker(trimmed: &str) -> Option<&str> {
        // Must be "- text", "* text", or "+ text"
        // Be careful: "---" is a horizontal rule, not a list item
        // "* " alone at start needs to not conflict with bold/italic markers at line level
        for marker in &["- ", "* ", "+ "] {
            if let Some(rest) = trimmed.strip_prefix(marker) {
                return Some(rest);
            }
        }
        None
    }

    /// Strip an ordered list marker (N. ) and return (number_str, content).
    fn strip_ordered_list_marker(trimmed: &str) -> Option<(&str, &str)> {
        // Match pattern: one or more digits, then '. ', then content
        let dot_pos = trimmed.find(". ")?;
        let num_part = &trimmed[..dot_pos];
        if !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit()) {
            Some((num_part, &trimmed[dot_pos + 2..]))
        } else {
            None
        }
    }

    /// Extract leading whitespace from a line.
    fn leading_whitespace(line: &str) -> &str {
        let trimmed_len = line.trim_start().len();
        &line[..line.len() - trimmed_len]
    }

    /// Apply inline formatting (bold, italic, inline code) to a line of normal text.
    fn render_inline(&self, line: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            // Check for bold italic: ***text***
            if i + 2 < len && chars[i] == '*' && chars[i + 1] == '*' && chars[i + 2] == '*' {
                if let Some(close) = Self::find_triple_star(&chars, i + 3) {
                    let inner: String = chars[i + 3..close].iter().collect();
                    result.push_str(&format!("{BOLD_ITALIC}{inner}{RESET}"));
                    i = close + 3;
                    continue;
                }
            }

            // Check for bold: **text**
            if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
                // Find closing **
                if let Some(close) = Self::find_double_star(&chars, i + 2) {
                    let inner: String = chars[i + 2..close].iter().collect();
                    result.push_str(&format!("{BOLD}{inner}{RESET}"));
                    i = close + 2;
                    continue;
                }
            }

            // Check for italic: *text* (single star, not followed by another star)
            if chars[i] == '*' && (i + 1 >= len || chars[i + 1] != '*') {
                if let Some(close) = Self::find_single_star(&chars, i + 1) {
                    // Must have at least one char between markers
                    if close > i + 1 {
                        let inner: String = chars[i + 1..close].iter().collect();
                        result.push_str(&format!("{ITALIC}{inner}{RESET}"));
                        i = close + 1;
                        continue;
                    }
                }
            }

            // Check for inline code: `text`
            if chars[i] == '`' {
                // Find closing backtick (not another opening fence)
                if let Some(close) = Self::find_backtick(&chars, i + 1) {
                    let inner: String = chars[i + 1..close].iter().collect();
                    result.push_str(&format!("{CYAN}{inner}{RESET}"));
                    i = close + 1;
                    continue;
                }
            }

            result.push(chars[i]);
            i += 1;
        }

        result
    }

    /// Find closing *** starting from position `from` in char slice.
    fn find_triple_star(chars: &[char], from: usize) -> Option<usize> {
        let len = chars.len();
        let mut j = from;
        while j + 2 < len {
            if chars[j] == '*' && chars[j + 1] == '*' && chars[j + 2] == '*' {
                return Some(j);
            }
            j += 1;
        }
        None
    }

    /// Find closing ** starting from position `from` in char slice.
    fn find_double_star(chars: &[char], from: usize) -> Option<usize> {
        let len = chars.len();
        let mut j = from;
        while j + 1 < len {
            if chars[j] == '*' && chars[j + 1] == '*' {
                return Some(j);
            }
            j += 1;
        }
        None
    }

    /// Find closing single * starting from position `from` in char slice.
    /// The closing * must NOT be followed by another * (to avoid matching inside **).
    fn find_single_star(chars: &[char], from: usize) -> Option<usize> {
        let len = chars.len();
        for j in from..len {
            if chars[j] == '*' {
                // Make sure it's not part of a ** sequence
                if j + 1 < len && chars[j + 1] == '*' {
                    continue;
                }
                // Also make sure the preceding char isn't * (closing side of **)
                if j > from && chars[j - 1] == '*' {
                    continue;
                }
                return Some(j);
            }
        }
        None
    }

    /// Find closing backtick starting from position `from` in char slice.
    fn find_backtick(chars: &[char], from: usize) -> Option<usize> {
        (from..chars.len()).find(|&j| chars[j] == '`')
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// --- Waiting spinner for AI responses ---

/// Braille spinner frames used for the "thinking" animation.
pub const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Get the spinner frame for a given tick index (wraps around).
pub fn spinner_frame(tick: usize) -> char {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
}

/// A handle to a running spinner task. Dropping or calling `stop()` cancels it.
pub struct Spinner {
    cancel: tokio::sync::watch::Sender<bool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Spinner {
    /// Start a spinner that prints frames to stderr every 100ms.
    /// The spinner shows `⠋ thinking...` cycling through braille characters.
    pub fn start() -> Self {
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(async move {
            let mut tick: usize = 0;
            loop {
                // Check cancellation before printing
                if *cancel_rx.borrow() {
                    // Clear the spinner line
                    eprint!("\r\x1b[K");
                    break;
                }
                let frame = spinner_frame(tick);
                eprint!("\r{DIM}  {frame} thinking...{RESET}");
                tick = tick.wrapping_add(1);

                // Wait 100ms or until cancelled
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                    _ = cancel_rx.changed() => {
                        // Clear the spinner line
                        eprint!("\r\x1b[K");
                        break;
                    }
                }
            }
        });
        Self {
            cancel: cancel_tx,
            handle: Some(handle),
        }
    }

    /// Stop the spinner and clear its output.
    /// Clears the spinner line directly (don't rely on the async task to clear,
    /// since abort() can race with the clear sequence).
    ///
    /// render_latency_budget: This is the first-token cost (~0.1ms).
    /// The synchronous eprint + flush ensures the spinner line is cleared
    /// before any stdout text appears. The async handle abort is deferred
    /// to Drop to minimize latency on the critical path.
    pub fn stop(self) {
        let _ = self.cancel.send(true);
        // Clear the spinner line from the calling thread — this is synchronous
        // and guaranteed to complete before any subsequent stdout writes.
        eprint!("\r\x1b[K");
        let _ = io::stderr().flush();
        // Defer handle.abort() to Drop — it interacts with the tokio runtime
        // and doesn't need to complete before the first text token is printed.
        // The cancel signal already ensures the spinner task won't write again.
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        // Clear the spinner line synchronously on drop too
        eprint!("\r\x1b[K");
        let _ = io::stderr().flush();
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

// --- Live tool progress display ---

/// Format a live progress line for a running tool.
///
/// Shows spinner frame, tool name, elapsed time, and optional line count.
/// Example: `  ⠹ bash ⏱ 12s` or `  ⠹ bash ⏱ 1m 5s (142 lines)`
pub fn format_tool_progress(
    tool_name: &str,
    elapsed: Duration,
    tick: usize,
    line_count: Option<usize>,
) -> String {
    let frame = spinner_frame(tick);
    let time_str = format_duration_live(elapsed);
    let lines_str = match line_count {
        Some(n) if n > 0 => {
            let word = pluralize(n, "line", "lines");
            format!(" ({n} {word})")
        }
        _ => String::new(),
    };
    format!("{DIM}  {frame} {tool_name} ⏱ {time_str}{lines_str}{RESET}")
}

/// Format elapsed duration for live display (compact, human-friendly).
///
/// - Under 60s: `5s`
/// - 60s+: `1m 5s`
/// - 60m+: `1h 2m`
pub fn format_duration_live(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{m}m")
        } else {
            format!("{m}m {s}s")
        }
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h {m}m")
        }
    }
}

/// Format the last N lines of partial output for live display.
///
/// Returns dimmed, indented lines showing the tail of tool output.
/// Used to give users a preview of what a running command is producing.
/// Empty input returns empty string.
pub fn format_partial_tail(output: &str, max_lines: usize) -> String {
    if output.is_empty() || max_lines == 0 {
        return String::new();
    }
    let lines: Vec<&str> = output.lines().collect();
    let total = lines.len();
    let start = total.saturating_sub(max_lines);
    let tail: Vec<&str> = lines[start..].to_vec();

    let mut result = String::new();
    if start > 0 {
        let skipped = start;
        let word = pluralize(skipped, "line", "lines");
        result.push_str(&format!("{DIM}    ┆ ... {skipped} {word} above{RESET}\n"));
    }
    for line in tail {
        let truncated = truncate_with_ellipsis(line, 120);
        result.push_str(&format!("{DIM}    ┆ {truncated}{RESET}\n"));
    }
    // Remove trailing newline
    if result.ends_with('\n') {
        result.pop();
    }
    result
}

/// Count the number of lines in a tool result's text content.
pub fn count_result_lines(result: &ToolResult) -> usize {
    result
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(text.lines().count()),
            _ => None,
        })
        .sum()
}

/// Extract all text content from a ToolResult as a single string.
pub fn extract_result_text(result: &ToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// State tracker for a currently-running tool, used by the event loop
/// to display live progress updates.
#[allow(dead_code)]
pub struct ActiveToolState {
    pub tool_name: String,
    pub start: Instant,
    pub line_count: usize,
    pub last_output: String,
}

impl ActiveToolState {
    /// Create a new state tracker for a tool.
    #[allow(dead_code)]
    pub fn new(tool_name: String) -> Self {
        Self {
            tool_name,
            start: Instant::now(),
            line_count: 0,
            last_output: String::new(),
        }
    }

    /// Update with partial output from a ToolExecutionUpdate event.
    #[allow(dead_code)]
    pub fn update_partial(&mut self, text: &str) {
        self.line_count = text.lines().count();
        self.last_output = text.to_string();
    }
}

/// A handle to a running tool-progress timer task.
/// Shows `  ⠹ bash ⏱ 12s` on stderr, updating every second.
/// Dropping or calling `stop()` cancels it and clears the line.
pub struct ToolProgressTimer {
    cancel: tokio::sync::watch::Sender<bool>,
    line_count: Arc<std::sync::atomic::AtomicUsize>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl ToolProgressTimer {
    /// Start a timer that shows elapsed time for a tool on stderr.
    /// Updates every second with the current line count.
    pub fn start(tool_name: String) -> Self {
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let line_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let line_count_clone = Arc::clone(&line_count);
        let handle = tokio::spawn(async move {
            let start = Instant::now();
            let mut tick: usize = 0;
            // Wait 2 seconds before showing the timer — short commands
            // finish fast and don't need a progress display.
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                _ = cancel_rx.changed() => {
                    return;
                }
            }
            loop {
                if *cancel_rx.borrow() {
                    eprint!("\r\x1b[K");
                    let _ = io::stderr().flush();
                    break;
                }
                let elapsed = start.elapsed();
                let lc = line_count_clone.load(std::sync::atomic::Ordering::Relaxed);
                let lc_opt = if lc > 0 { Some(lc) } else { None };
                let progress = format_tool_progress(&tool_name, elapsed, tick, lc_opt);
                eprint!("\r\x1b[K{progress}");
                let _ = io::stderr().flush();
                tick = tick.wrapping_add(1);

                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                    _ = cancel_rx.changed() => {
                        eprint!("\r\x1b[K");
                        let _ = io::stderr().flush();
                        break;
                    }
                }
            }
        });
        Self {
            cancel: cancel_tx,
            line_count,
            handle: Some(handle),
        }
    }

    /// Update the line count shown in the timer display.
    pub fn set_line_count(&self, count: usize) {
        self.line_count
            .store(count, std::sync::atomic::Ordering::Relaxed);
    }

    /// Stop the timer and clear its output.
    pub fn stop(self) {
        let _ = self.cancel.send(true);
        eprint!("\r\x1b[K");
        let _ = io::stderr().flush();
    }
}

impl Drop for ToolProgressTimer {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        eprint!("\r\x1b[K");
        let _ = io::stderr().flush();
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_unicode() {
        assert_eq!(truncate("héllo wörld", 5), "héllo");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate("", 5), "");
    }

    #[test]
    fn test_truncate_adds_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello…");
        assert_eq!(truncate_with_ellipsis("hi", 5), "hi");
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn test_format_token_count() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(999), "999");
        assert_eq!(format_token_count(1000), "1.0k");
        assert_eq!(format_token_count(1500), "1.5k");
        assert_eq!(format_token_count(10000), "10.0k");
        assert_eq!(format_token_count(150000), "150.0k");
        assert_eq!(format_token_count(1000000), "1.0M");
        assert_eq!(format_token_count(2500000), "2.5M");
    }

    #[test]
    fn test_context_bar() {
        let bar = context_bar(50000, 200000);
        assert!(bar.contains('█'));
        assert!(bar.contains("25%"));

        let bar_empty = context_bar(0, 200000);
        assert!(bar_empty.contains("0%"));

        let bar_full = context_bar(200000, 200000);
        assert!(bar_full.contains("100%"));
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.0001), "$0.0001");
        assert_eq!(format_cost(0.0042), "$0.0042");
        assert_eq!(format_cost(0.05), "$0.050");
        assert_eq!(format_cost(0.123), "$0.123");
        assert_eq!(format_cost(1.5), "$1.50");
        assert_eq!(format_cost(12.345), "$12.35");
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(50)),
            "50ms"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(999)),
            "999ms"
        );
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(1000)),
            "1.0s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(1500)),
            "1.5s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(30000)),
            "30.0s"
        );
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(60000)),
            "1m 0s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(90000)),
            "1m 30s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(125000)),
            "2m 5s"
        );
    }

    #[test]
    fn test_estimate_cost_opus() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-opus-4-6").unwrap();
        assert!((cost - 7.5).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_sonnet() {
        let usage = yoagent::Usage {
            input: 500_000,
            output: 50_000,
            cache_read: 200_000,
            cache_write: 100_000,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-sonnet-4-6").unwrap();
        assert!((cost - 2.685).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_haiku() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 500_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-haiku-4-5").unwrap();
        assert!((cost - 3.5).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_unknown_model() {
        let usage = yoagent::Usage {
            input: 1000,
            output: 1000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // A truly unknown model should return None
        assert!(estimate_cost(&usage, "unknown-model-xyz").is_none());
    }

    #[test]
    fn test_cost_breakdown_opus() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 500_000,
            cache_write: 200_000,
            total_tokens: 0,
        };
        let (input, cw, cr, output) = cost_breakdown(&usage, "claude-opus-4-6").unwrap();
        // input: 1M * 5/M = 5.0
        assert!((input - 5.0).abs() < 0.001);
        // output: 100k * 25/M = 2.5
        assert!((output - 2.5).abs() < 0.001);
        // cache_read: 500k * 0.50/M = 0.25
        assert!((cr - 0.25).abs() < 0.001);
        // cache_write: 200k * 6.25/M = 1.25
        assert!((cw - 1.25).abs() < 0.001);
        // Total should match estimate_cost
        let total = input + cw + cr + output;
        let expected = estimate_cost(&usage, "claude-opus-4-6").unwrap();
        assert!((total - expected).abs() < 0.001);
    }

    #[test]
    fn test_cost_breakdown_unknown_model() {
        let usage = yoagent::Usage {
            input: 1000,
            output: 1000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        assert!(cost_breakdown(&usage, "unknown-model-xyz").is_none());
    }

    // ── OpenAI model pricing tests ───────────────────────────────────

    #[test]
    fn test_estimate_cost_gpt4o() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-4o: $2.50/MTok input, $10.00/MTok output
        let cost = estimate_cost(&usage, "gpt-4o").unwrap();
        assert!((cost - 3.5).abs() < 0.001, "gpt-4o cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gpt4o_mini() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-4o-mini: $0.15/MTok input, $0.60/MTok output
        let cost = estimate_cost(&usage, "gpt-4o-mini").unwrap();
        assert!((cost - 0.75).abs() < 0.001, "gpt-4o-mini cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gpt41() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-4.1: $2.00/MTok input, $8.00/MTok output
        let cost = estimate_cost(&usage, "gpt-4.1").unwrap();
        assert!((cost - 2.8).abs() < 0.001, "gpt-4.1 cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gpt41_mini() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-4.1-mini: $0.40/MTok input, $1.60/MTok output
        let cost = estimate_cost(&usage, "gpt-4.1-mini").unwrap();
        assert!((cost - 2.0).abs() < 0.001, "gpt-4.1-mini cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_o3() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // o3: $2.00/MTok input, $8.00/MTok output
        let cost = estimate_cost(&usage, "o3").unwrap();
        assert!((cost - 2.8).abs() < 0.001, "o3 cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_o4_mini() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // o4-mini: $1.10/MTok input, $4.40/MTok output
        let cost = estimate_cost(&usage, "o4-mini").unwrap();
        assert!((cost - 1.54).abs() < 0.001, "o4-mini cost: {cost}");
    }

    // ── Google Gemini pricing tests ──────────────────────────────────

    #[test]
    fn test_estimate_cost_gemini_25_pro() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gemini-2.5-pro: $1.25/MTok input, $10.00/MTok output
        let cost = estimate_cost(&usage, "gemini-2.5-pro").unwrap();
        assert!((cost - 2.25).abs() < 0.001, "gemini-2.5-pro cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gemini_25_flash() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gemini-2.5-flash: $0.15/MTok input, $0.60/MTok output
        let cost = estimate_cost(&usage, "gemini-2.5-flash").unwrap();
        assert!((cost - 0.75).abs() < 0.001, "gemini-2.5-flash cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gemini_20_flash() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gemini-2.0-flash: $0.10/MTok input, $0.40/MTok output
        let cost = estimate_cost(&usage, "gemini-2.0-flash").unwrap();
        assert!((cost - 0.50).abs() < 0.001, "gemini-2.0-flash cost: {cost}");
    }

    // ── DeepSeek pricing tests ───────────────────────────────────────

    #[test]
    fn test_estimate_cost_deepseek_chat() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // deepseek-chat: $0.27/MTok input, $1.10/MTok output
        let cost = estimate_cost(&usage, "deepseek-chat").unwrap();
        assert!((cost - 1.37).abs() < 0.001, "deepseek-chat cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_deepseek_reasoner() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // deepseek-reasoner: $0.55/MTok input, $2.19/MTok output
        let cost = estimate_cost(&usage, "deepseek-reasoner").unwrap();
        assert!(
            (cost - 2.74).abs() < 0.001,
            "deepseek-reasoner cost: {cost}"
        );
    }

    // ── Mistral pricing tests ────────────────────────────────────────

    #[test]
    fn test_estimate_cost_mistral_large() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // mistral-large: $2.00/MTok input, $6.00/MTok output
        let cost = estimate_cost(&usage, "mistral-large-latest").unwrap();
        assert!((cost - 2.6).abs() < 0.001, "mistral-large cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_mistral_small() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // mistral-small: $0.10/MTok input, $0.30/MTok output
        let cost = estimate_cost(&usage, "mistral-small-latest").unwrap();
        assert!((cost - 0.40).abs() < 0.001, "mistral-small cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_codestral() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // codestral: $0.30/MTok input, $0.90/MTok output
        let cost = estimate_cost(&usage, "codestral-latest").unwrap();
        assert!((cost - 1.20).abs() < 0.001, "codestral cost: {cost}");
    }

    // ── xAI (Grok) pricing tests ─────────────────────────────────────

    #[test]
    fn test_estimate_cost_grok3() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // grok-3: $3.00/MTok input, $15.00/MTok output
        let cost = estimate_cost(&usage, "grok-3").unwrap();
        assert!((cost - 4.5).abs() < 0.001, "grok-3 cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_grok3_mini() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // grok-3-mini: $0.30/MTok input, $0.50/MTok output
        let cost = estimate_cost(&usage, "grok-3-mini").unwrap();
        assert!((cost - 0.80).abs() < 0.001, "grok-3-mini cost: {cost}");
    }

    // ── Groq pricing tests ───────────────────────────────────────────

    #[test]
    fn test_estimate_cost_groq_llama70b() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // llama-3.3-70b on Groq: $0.59/MTok input, $0.79/MTok output
        let cost = estimate_cost(&usage, "llama-3.3-70b-versatile").unwrap();
        assert!((cost - 1.38).abs() < 0.001, "llama-3.3-70b cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_groq_llama8b() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // llama-3.1-8b on Groq: $0.05/MTok input, $0.08/MTok output
        let cost = estimate_cost(&usage, "llama-3.1-8b-instant").unwrap();
        assert!((cost - 0.13).abs() < 0.001, "llama-3.1-8b cost: {cost}");
    }

    // ── ZAI (Zhipu AI) pricing tests ─────────────────────────────────

    #[test]
    fn test_estimate_cost_glm4_plus() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // glm-4-plus: $0.70/MTok input, $0.70/MTok output
        let cost = estimate_cost(&usage, "glm-4-plus").unwrap();
        assert!((cost - 1.40).abs() < 0.001, "glm-4-plus cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_glm4_air() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // glm-4-air: $0.07/MTok input, $0.07/MTok output
        let cost = estimate_cost(&usage, "glm-4-air").unwrap();
        assert!((cost - 0.14).abs() < 0.001, "glm-4-air cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_glm4_flash() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // glm-4-flash: $0.01/MTok input, $0.01/MTok output
        let cost = estimate_cost(&usage, "glm-4-flash").unwrap();
        assert!((cost - 0.02).abs() < 0.001, "glm-4-flash cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_glm5() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // glm-5: $0.70/MTok input, $0.70/MTok output
        let cost = estimate_cost(&usage, "glm-5").unwrap();
        assert!((cost - 1.40).abs() < 0.001, "glm-5 cost: {cost}");
    }

    // ── OpenRouter prefix stripping tests ────────────────────────────

    #[test]
    fn test_estimate_cost_openrouter_anthropic_prefix() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // OpenRouter uses "anthropic/claude-sonnet-4-20250514" format
        let cost = estimate_cost(&usage, "anthropic/claude-sonnet-4-20250514").unwrap();
        let direct_cost = estimate_cost(&usage, "claude-sonnet-4-20250514").unwrap();
        assert!(
            (cost - direct_cost).abs() < 0.001,
            "OpenRouter prefix should resolve to same pricing"
        );
    }

    #[test]
    fn test_estimate_cost_openrouter_openai_prefix() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "openai/gpt-4o").unwrap();
        let direct_cost = estimate_cost(&usage, "gpt-4o").unwrap();
        assert!(
            (cost - direct_cost).abs() < 0.001,
            "OpenRouter openai/ prefix should resolve to same pricing"
        );
    }

    #[test]
    fn test_estimate_cost_openrouter_google_prefix() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "google/gemini-2.0-flash").unwrap();
        let direct_cost = estimate_cost(&usage, "gemini-2.0-flash").unwrap();
        assert!(
            (cost - direct_cost).abs() < 0.001,
            "OpenRouter google/ prefix should resolve to same pricing"
        );
    }

    // ── Non-caching provider zero cache costs ────────────────────────

    #[test]
    fn test_non_anthropic_providers_zero_cache_costs() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 500_000,
            cache_write: 200_000,
            total_tokens: 0,
        };
        // For non-Anthropic models, cache_write and cache_read rates are 0
        // so even with cache_read/cache_write tokens, those don't add to cost
        let (_, cw, cr, _) = cost_breakdown(&usage, "gpt-4o").unwrap();
        assert!(
            cw.abs() < 0.001 && cr.abs() < 0.001,
            "Non-Anthropic models should have zero cache costs: cw={cw}, cr={cr}"
        );
    }

    #[test]
    fn test_format_tool_summary_bash() {
        let args = serde_json::json!({"command": "echo hello"});
        assert_eq!(format_tool_summary("bash", &args), "$ echo hello");
    }

    #[test]
    fn test_format_tool_summary_bash_long_command() {
        let long_cmd = "a".repeat(100);
        let args = serde_json::json!({"command": long_cmd});
        let result = format_tool_summary("bash", &args);
        assert!(result.starts_with("$ "));
        assert!(result.ends_with('…'));
        assert!(result.len() < 100);
    }

    #[test]
    fn test_format_tool_summary_read_file() {
        let args = serde_json::json!({"path": "src/main.rs"});
        assert_eq!(format_tool_summary("read_file", &args), "read src/main.rs");
    }

    #[test]
    fn test_format_tool_summary_write_file() {
        let args = serde_json::json!({"path": "out.txt"});
        assert_eq!(format_tool_summary("write_file", &args), "write out.txt");
    }

    #[test]
    fn test_format_tool_summary_edit_file() {
        let args = serde_json::json!({"path": "foo.rs"});
        assert_eq!(format_tool_summary("edit_file", &args), "edit foo.rs");
    }

    #[test]
    fn test_format_tool_summary_list_files() {
        let args = serde_json::json!({"path": "src/"});
        assert_eq!(format_tool_summary("list_files", &args), "ls src/");
    }

    #[test]
    fn test_format_tool_summary_list_files_no_path() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("list_files", &args), "ls .");
    }

    #[test]
    fn test_format_tool_summary_search() {
        let args = serde_json::json!({"pattern": "TODO"});
        assert_eq!(format_tool_summary("search", &args), "search 'TODO'");
    }

    #[test]
    fn test_format_tool_summary_unknown_tool() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("custom_tool", &args), "custom_tool");
    }

    #[test]
    fn test_color_struct_display_outputs_ansi() {
        // Color struct should produce the ANSI code when color is enabled
        let c = Color("\x1b[1m");
        let formatted = format!("{c}");
        // We can't guarantee NO_COLOR isn't set in the test environment,
        // but the type itself should compile and format correctly.
        assert!(formatted == "\x1b[1m" || formatted.is_empty());
    }

    #[test]
    fn test_color_struct_display_consistency() {
        // All color constants should be the same type and format without panic
        let result = format!("{BOLD}{DIM}{GREEN}{YELLOW}{CYAN}{RED}{RESET}");
        // Should either have all codes or be empty (if NO_COLOR is set)
        assert!(result.contains('\x1b') || result.is_empty());
    }

    // --- MarkdownRenderer tests ---

    /// Helper: render a full string through the renderer (not streamed).
    fn render_full(input: &str) -> String {
        let mut r = MarkdownRenderer::new();
        let mut out = r.render_delta(input);
        out.push_str(&r.flush());
        out
    }

    #[test]
    fn test_md_code_block_detection() {
        let input = "before\n```\ncode line\n```\nafter\n";
        let out = render_full(input);
        // "code line" should be wrapped in DIM
        assert!(out.contains(&format!("{DIM}code line{RESET}")));
        // "before" and "after" should NOT be dim
        assert!(out.contains("before"));
        assert!(out.contains("after"));
    }

    #[test]
    fn test_md_code_block_with_language() {
        let input = "```rust\nlet x = 1;\n```\n";
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta(input);
        let flushed = r.flush();
        let full = format!("{out}{flushed}");
        // Language should be captured and fence dimmed
        assert!(full.contains(&format!("{DIM}```rust{RESET}")));
        // "let" should be keyword-highlighted, not just DIM
        assert!(full.contains(&format!("{BOLD_CYAN}let{RESET}")));
        // Number should be yellow
        assert!(full.contains(&format!("{YELLOW}1{RESET}")));
    }

    #[test]
    fn test_md_inline_code() {
        let out = render_full("use `Option<T>` here\n");
        assert!(out.contains(&format!("{CYAN}Option<T>{RESET}")));
    }

    #[test]
    fn test_md_bold_text() {
        let out = render_full("this is **important** stuff\n");
        assert!(out.contains(&format!("{BOLD}important{RESET}")));
    }

    #[test]
    fn test_md_header_rendering() {
        let out = render_full("# Hello World\n");
        assert!(out.contains(&format!("{BOLD}{CYAN}# Hello World{RESET}")));
    }

    #[test]
    fn test_md_header_h2() {
        let out = render_full("## Section Two\n");
        assert!(out.contains(&format!("{BOLD}{CYAN}## Section Two{RESET}")));
    }

    #[test]
    fn test_md_partial_delta_fence() {
        // Fence marker split across multiple deltas
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("``");
        // Nothing emitted yet — still buffered (no newline)
        assert_eq!(out1, "");
        let out2 = r.render_delta("`\n");
        // Now the fence line is complete
        assert!(out2.contains(&format!("{DIM}```{RESET}")));
        let out3 = r.render_delta("code here\n");
        assert!(out3.contains(&format!("{DIM}code here{RESET}")));
        let out4 = r.render_delta("```\n");
        assert!(out4.contains(&format!("{DIM}```{RESET}")));
        // After closing, normal text again
        let out5 = r.render_delta("normal\n");
        assert!(out5.contains("normal"));
        assert!(!out5.contains(&format!("{DIM}")));
    }

    #[test]
    fn test_md_empty_delta() {
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("");
        assert_eq!(out, "");
        let flushed = r.flush();
        assert_eq!(flushed, "");
    }

    #[test]
    fn test_md_multiple_code_blocks() {
        let input = "text\n```\nblock1\n```\nmiddle\n```python\nblock2\n```\nend\n";
        let out = render_full(input);
        // Untagged code block → DIM fallback
        assert!(out.contains(&format!("{DIM}block1{RESET}")));
        assert!(out.contains("middle"));
        // Python-tagged code block → syntax highlighted (no keyword match, plain output)
        assert!(out.contains("block2"));
        assert!(out.contains("end"));
    }

    #[test]
    fn test_md_inline_code_inside_bold() {
        // Inline code backticks inside bold — bold wraps, code is separate
        let out = render_full("**bold** and `code`\n");
        assert!(out.contains(&format!("{BOLD}bold{RESET}")));
        assert!(out.contains(&format!("{CYAN}code{RESET}")));
    }

    #[test]
    fn test_md_unmatched_backtick() {
        // Single backtick without closing — should pass through literally
        let out = render_full("it's a `partial\n");
        assert!(out.contains('`'));
        assert!(out.contains("partial"));
    }

    #[test]
    fn test_md_unmatched_bold() {
        // Unmatched ** should pass through literally
        let out = render_full("star **power\n");
        assert!(out.contains("**"));
        assert!(out.contains("power"));
    }

    #[test]
    fn test_md_flush_partial_line() {
        let mut r = MarkdownRenderer::new();
        // "no" at line start — can't be fence/header, resolves immediately
        let out = r.render_delta("no");
        assert!(
            out.contains("no"),
            "Short non-fence/non-header text resolves immediately"
        );
        // Continue adding text — mid-line now, immediate output
        let out2 = r.render_delta(" newline here");
        assert!(out2.contains(" newline here"));
    }

    #[test]
    fn test_md_flush_with_inline_formatting() {
        let mut r = MarkdownRenderer::new();
        // "hello **world**" — resolves as non-fence at line start, then renders inline
        let out = r.render_delta("hello **world**");
        let flushed = r.flush();
        let total = format!("{out}{flushed}");
        assert!(total.contains(&format!("{BOLD}world{RESET}")));
    }

    #[test]
    fn test_md_default_trait() {
        let r = MarkdownRenderer::default();
        assert!(!r.in_code_block);
        assert!(r.code_lang.is_none());
        assert!(r.line_buffer.is_empty());
        assert!(r.line_start);
        assert!(!r.block_prefix_rendered);
    }

    // --- Streaming output tests (mid-line tokens should render immediately) ---

    #[test]
    fn test_md_streaming_mid_line_immediate_output() {
        // Simulate LLM streaming: first token starts a line, subsequent tokens mid-line
        let mut r = MarkdownRenderer::new();
        // First token: "Hello " — at line start, long enough to resolve as normal text
        let out1 = r.render_delta("Hello ");
        // Should produce output (6 chars, clearly not a fence or header)
        assert!(
            out1.contains("Hello "),
            "Expected immediate output for non-fence/non-header text, got: '{out1}'"
        );

        // Second token: "world" — mid-line, should be immediate
        let out2 = r.render_delta("world");
        assert!(
            out2.contains("world"),
            "Mid-line delta should produce immediate output, got: '{out2}'"
        );

        // Third token: " how" — still mid-line
        let out3 = r.render_delta(" how");
        assert!(
            out3.contains(" how"),
            "Mid-line delta should produce immediate output, got: '{out3}'"
        );
    }

    #[test]
    fn test_md_streaming_newline_resets_to_line_start() {
        let mut r = MarkdownRenderer::new();
        // Start with text that resolves line start
        let _ = r.render_delta("Hello world");
        // Now a newline — next delta should be at line start again
        let _ = r.render_delta("\n");
        // Short text at start of new line — should buffer briefly
        let out = r.render_delta("``");
        // Two backticks could be start of a fence — should buffer
        assert_eq!(
            out, "",
            "Short ambiguous text at line start should be buffered"
        );
    }

    #[test]
    fn test_md_streaming_code_fence_detected_at_line_start() {
        let mut r = MarkdownRenderer::new();
        // Send a code fence at line start
        let out1 = r.render_delta("```\n");
        assert!(out1.contains(&format!("{DIM}```{RESET}")));
        assert!(r.in_code_block);

        // Content inside code block
        let out2 = r.render_delta("some code\n");
        assert!(out2.contains(&format!("{DIM}some code{RESET}")));

        // Closing fence
        let out3 = r.render_delta("```\n");
        assert!(out3.contains(&format!("{DIM}```{RESET}")));
        assert!(!r.in_code_block);
    }

    #[test]
    fn test_md_streaming_header_detected_at_line_start() {
        let mut r = MarkdownRenderer::new();
        // Header at line start
        let out = r.render_delta("# My Header\n");
        assert!(out.contains(&format!("{BOLD}{CYAN}# My Header{RESET}")));
    }

    #[test]
    fn test_md_streaming_bold_mid_line() {
        let mut r = MarkdownRenderer::new();
        // Start a line with enough text to resolve
        let out1 = r.render_delta("This is ");
        assert!(out1.contains("This is "));
        // Now bold text mid-line
        let out2 = r.render_delta("**important**");
        assert!(
            out2.contains(&format!("{BOLD}important{RESET}")),
            "Bold formatting should work in mid-line streaming, got: '{out2}'"
        );
    }

    #[test]
    fn test_md_streaming_inline_code_mid_line() {
        let mut r = MarkdownRenderer::new();
        // Start a line
        let out1 = r.render_delta("Use the ");
        assert!(out1.contains("Use the "));
        // Inline code mid-line
        let out2 = r.render_delta("`Option`");
        assert!(
            out2.contains(&format!("{CYAN}Option{RESET}")),
            "Inline code should work in mid-line streaming, got: '{out2}'"
        );
    }

    #[test]
    fn test_md_streaming_word_by_word_paragraph() {
        // Simulate typical LLM streaming: word by word
        let mut r = MarkdownRenderer::new();
        let words = ["The ", "quick ", "brown ", "fox ", "jumps"];
        let mut got_output = false;
        for word in &words[..] {
            let out = r.render_delta(word);
            if !out.is_empty() {
                got_output = true;
            }
        }
        // We should have gotten SOME output before the line ends
        assert!(
            got_output,
            "Word-by-word streaming should produce output before newline"
        );

        // Flush remainder
        let _flushed = r.flush();
        // Total output should contain all words
        let mut total = String::new();
        let mut r2 = MarkdownRenderer::new();
        for word in &words[..] {
            total.push_str(&r2.render_delta(word));
        }
        total.push_str(&r2.flush());
        assert!(total.contains("The "));
        assert!(total.contains("fox "));
    }

    #[test]
    fn test_md_streaming_line_start_buffer_short_text() {
        // At line start, very short text (1-3 chars) that could be start of fence/header
        // should be buffered
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("#");
        // Single '#' could be a header — should buffer
        assert_eq!(out, "", "Single '#' at line start should be buffered");

        // Now add more to reveal it's a header
        let out2 = r.render_delta(" Title\n");
        assert!(out2.contains(&format!("{BOLD}{CYAN}# Title{RESET}")));
    }

    #[test]
    fn test_md_streaming_line_start_resolves_normal() {
        // At line start, text that quickly resolves as not a fence/header
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("Normal text");
        // "Normal" is 11 chars, clearly not a fence or header — should output
        assert!(
            out.contains("Normal text"),
            "Non-fence/non-header text should be output once resolved, got: '{out}'"
        );
    }

    #[test]
    fn test_md_streaming_existing_tests_still_pass() {
        // Ensure the full-line render_full helper still works exactly as before
        let out = render_full("Hello **world** and `code`\n");
        assert!(out.contains("Hello "));
        assert!(out.contains(&format!("{BOLD}world{RESET}")));
        assert!(out.contains(&format!("{CYAN}code{RESET}")));
    }

    #[test]
    fn test_md_streaming_in_code_block_immediate() {
        // Inside a code block, tokens should stream immediately once fence is ruled out.
        // "let x" can't be a closing fence (doesn't start with `), so it should
        // be early-resolved and emitted without needing flush().
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```rust\n");
        assert!(r.in_code_block);
        // Send code token — not a fence, should be emitted immediately
        let out = r.render_delta("let x");
        assert!(
            !out.is_empty(),
            "Code block content that can't be a fence should emit immediately, got empty"
        );
        assert!(
            out.contains("let"),
            "Code block content should contain the text, got: '{out}'"
        );
    }

    #[test]
    fn test_md_code_block_mid_line_emitted_immediately() {
        // Issue #147: Mid-line code block content should be emitted token-by-token,
        // not buffered until a newline arrives.
        let mut r = MarkdownRenderer::new();
        // Open a code block
        let _ = r.render_delta("```\n");
        assert!(r.in_code_block);

        // Send a line start token that gets buffered (could be closing fence)
        // Then a complete line to move past line_start
        let _ = r.render_delta("let x = 1;\n");

        // Now send a mid-line token — should be emitted immediately, not empty
        let out = r.render_delta("println");
        assert!(
            !out.is_empty(),
            "Mid-line code block token should be emitted immediately, got empty string"
        );
        assert!(
            out.contains("println"),
            "Mid-line code block token should contain the text, got: '{out}'"
        );
    }

    #[test]
    fn test_md_code_block_mid_line_with_newline() {
        // When a newline arrives mid-line in a code block, it should transition to line_start
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```\n");
        let _ = r.render_delta("first line\n");

        // Send mid-line token followed by newline
        let out = r.render_delta("hello\n");
        assert!(
            out.contains("hello"),
            "Code block content before newline should be rendered, got: '{out}'"
        );
        // After the newline, we should be at line_start again
        assert!(
            r.line_start,
            "After newline in code block, should be at line_start"
        );
    }

    #[test]
    fn test_md_code_block_fence_detection_still_works() {
        // Closing fence detection must still work even with the mid-line fast path
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```rust\n");
        assert!(r.in_code_block);

        let _ = r.render_delta("let x = 42;\n");
        assert!(r.in_code_block);

        // Closing fence at line start — must be detected (not short-circuited)
        let _ = r.render_delta("```\n");
        assert!(
            !r.in_code_block,
            "Closing fence should still be detected and end the code block"
        );
    }

    #[test]
    fn test_md_code_block_mid_line_multiple_tokens() {
        // Multiple mid-line tokens in a code block should each produce output
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```\n");
        let _ = r.render_delta("start\n");

        let out1 = r.render_delta("foo");
        assert!(
            !out1.is_empty(),
            "First mid-line token should emit, got empty"
        );

        let out2 = r.render_delta("bar");
        assert!(
            !out2.is_empty(),
            "Second mid-line token should emit, got empty"
        );

        let out3 = r.render_delta(" baz");
        assert!(
            !out3.is_empty(),
            "Third mid-line token should emit, got empty"
        );
    }

    #[test]
    fn test_md_streaming_single_token_produces_output() {
        // Issue #137: Common single-token inputs should produce non-empty output
        // when used mid-line. At line start, short tokens that can't be fences/headers
        // should also flush immediately.
        let test_cases = vec![
            // (token, description)
            ("Hello", "common greeting"),
            ("I", "single letter word"),
            (" will", "space-prefixed verb"),
            ("The", "article"),
            ("Sure", "affirmative"),
            ("Let", "common start word"),
            ("Yes", "short response"),
            ("To", "preposition"),
        ];

        for (token, desc) in &test_cases {
            // Test mid-line: should always produce output immediately
            let mut r = MarkdownRenderer::new();
            // First, get past line-start by sending a resolved line-start token
            let _ = r.render_delta("Start ");
            let out = r.render_delta(token);
            assert!(
                !out.is_empty(),
                "Mid-line token '{token}' ({desc}) should produce non-empty output, got empty"
            );
        }

        // Test at line start: tokens that can't be fences (``) or headers (#)
        // should flush immediately even if short
        let line_start_cases = vec![
            ("Hello", "common greeting"),
            ("I", "single letter I"),
            ("Sure", "affirmative"),
            ("The", "article"),
            ("Yes", "short response"),
        ];

        for (token, desc) in &line_start_cases {
            let mut r = MarkdownRenderer::new();
            let out = r.render_delta(token);
            assert!(
                !out.is_empty(),
                "Line-start token '{token}' ({desc}) that can't be fence/header should produce output, got empty"
            );
        }
    }

    #[test]
    fn test_md_streaming_single_char_non_special_at_line_start() {
        // Single characters that are NOT '#' or '`' should flush immediately
        // at line start, since they can't possibly be fences or headers
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("I");
        assert!(
            !out.is_empty(),
            "'I' at line start cannot be fence or header, should flush immediately"
        );
    }

    #[test]
    fn test_md_streaming_space_prefixed_token_at_line_start() {
        // " will" — space-prefixed, trimmed = "will" (4 chars), not fence/header
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta(" will");
        assert!(
            !out.is_empty(),
            "' will' at line start should resolve — trimmed 'will' is 4 chars, not fence/header"
        );
    }

    // --- Streaming latency: block elements should flush content after prefix ---

    #[test]
    fn test_md_streaming_list_item_content_not_buffered() {
        // List items should NOT buffer all content until newline.
        // Once we see "- " we know it's a list item — subsequent tokens
        // should stream immediately.
        let mut r = MarkdownRenderer::new();
        // Send list marker
        let out1 = r.render_delta("- ");
        // The marker itself may or may not produce output yet (prefix detection)
        // but let's accumulate
        let mut total = out1;

        // Send content token — should produce output immediately
        let out2 = r.render_delta("Hello");
        total.push_str(&out2);
        assert!(
            !out2.is_empty(),
            "List item content after '- ' should stream immediately, got empty"
        );

        // Another content token
        let out3 = r.render_delta(" world");
        total.push_str(&out3);
        assert!(
            !out3.is_empty(),
            "Additional list item tokens should stream immediately, got empty"
        );
    }

    #[test]
    fn test_md_streaming_blockquote_content_not_buffered() {
        // Blockquote content after "> " should stream immediately.
        let mut r = MarkdownRenderer::new();
        let _out1 = r.render_delta("> ");

        let out2 = r.render_delta("Some quoted");
        assert!(
            !out2.is_empty(),
            "Blockquote content after '> ' should stream immediately, got empty"
        );

        let out3 = r.render_delta(" text");
        assert!(
            !out3.is_empty(),
            "Additional blockquote tokens should stream immediately, got empty"
        );
    }

    #[test]
    fn test_md_streaming_header_content_still_buffers() {
        // Headers need to buffer until newline because the entire line
        // gets BOLD+CYAN styling. But "#" alone should buffer.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("#");
        assert_eq!(out, "", "Single '#' should buffer (could be header)");
    }

    #[test]
    fn test_md_streaming_code_fence_opener_still_buffers() {
        // Code fence openers must buffer until complete so we detect the fence.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("``");
        assert_eq!(out, "", "Partial fence '``' should buffer");

        let out2 = r.render_delta("`");
        // Still buffering (no newline yet, could be ```lang)
        // The fence might be detected only on \n
        assert_eq!(
            out2, "",
            "Complete fence '```' without newline should buffer"
        );
    }

    #[test]
    fn test_md_streaming_inline_formatting_on_partial_lines() {
        // Bold/italic/code formatting should work on partial lines (flushed mid-line)
        let mut r = MarkdownRenderer::new();
        // Start with resolved text
        let _ = r.render_delta("Check ");
        // Send bold text mid-line
        let out = r.render_delta("**this**");
        assert!(
            out.contains(&format!("{BOLD}this{RESET}")),
            "Bold formatting should work on mid-line partial text, got: '{out}'"
        );
    }

    #[test]
    fn test_md_streaming_list_renders_correctly_on_newline() {
        // Even with early flushing, the full list item should render correctly
        // when the newline arrives.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("- ");
        let out2 = r.render_delta("item text");
        let out3 = r.render_delta("\n");
        let flushed = r.flush();
        let total = format!("{out1}{out2}{out3}{flushed}");
        // Should contain the bullet character from list rendering
        assert!(
            total.contains("item text"),
            "List item text should appear in output, got: '{total}'"
        );
    }

    #[test]
    fn test_md_streaming_ordered_list_content_not_buffered() {
        // Ordered list: "1. " detected, subsequent content should stream
        let mut r = MarkdownRenderer::new();
        let _out1 = r.render_delta("1. ");

        let out2 = r.render_delta("First item");
        assert!(
            !out2.is_empty(),
            "Ordered list content after '1. ' should stream immediately, got empty"
        );
    }

    #[test]
    fn test_md_streaming_no_regression_full_render() {
        // Full render should still produce correct output for all line types
        let out = render_full("- list item\n> quoted\n1. ordered\n# header\nplain\n");
        assert!(
            out.contains("list item"),
            "List item missing from full render"
        );
        assert!(
            out.contains("quoted"),
            "Blockquote missing from full render"
        );
        assert!(
            out.contains("ordered"),
            "Ordered list missing from full render"
        );
        assert!(out.contains("header"), "Header missing from full render");
        assert!(out.contains("plain"), "Plain text missing from full render");
    }

    // --- flush_on_whitespace tests ---

    #[test]
    fn test_md_flush_on_whitespace_at_line_start() {
        // When buffer accumulates "word " at line start, the trailing space
        // proves it's not a fence/header — flush_on_whitespace should emit it.
        let mut r = MarkdownRenderer::new();
        // Simulate a token that ends with whitespace at line start
        // "1 " could look like the start of an ordered list ("1. "), but
        // the space without a dot means it's just text with a trailing space.
        // However, needs_line_buffering might still hold it. Let's use a
        // clearer case: a digit followed by space that needs_line_buffering holds.
        let out = r.flush_on_whitespace();
        assert_eq!(out, "", "Empty buffer should not flush");
    }

    #[test]
    fn test_md_flush_on_whitespace_with_word_boundary() {
        // Direct test of flush_on_whitespace with a buffer that has
        // non-special content ending in whitespace.
        let mut r = MarkdownRenderer::new();
        r.line_buffer = "Hello ".to_string();
        r.line_start = true;
        let out = r.flush_on_whitespace();
        assert!(
            out.contains("Hello"),
            "Buffer with word boundary should flush, got: '{out}'"
        );
        assert!(!r.line_start, "Should switch to mid-line after flush");
        assert!(
            r.line_buffer.is_empty(),
            "Buffer should be empty after flush"
        );
    }

    #[test]
    fn test_md_flush_on_whitespace_no_trailing_space() {
        let mut r = MarkdownRenderer::new();
        r.line_buffer = "Hello".to_string();
        r.line_start = true;
        let out = r.flush_on_whitespace();
        assert_eq!(
            out, "",
            "Buffer without trailing whitespace should not flush"
        );
    }

    #[test]
    fn test_md_flush_on_whitespace_only_whitespace() {
        let mut r = MarkdownRenderer::new();
        r.line_buffer = "   ".to_string();
        r.line_start = true;
        let out = r.flush_on_whitespace();
        assert_eq!(out, "", "Buffer with only whitespace should not flush");
    }

    #[test]
    fn test_md_flush_on_whitespace_not_at_line_start() {
        let mut r = MarkdownRenderer::new();
        r.line_buffer = "Hello ".to_string();
        r.line_start = false; // mid-line
        let out = r.flush_on_whitespace();
        assert_eq!(out, "", "Should not flush when not at line start");
    }

    #[test]
    fn test_md_flush_on_whitespace_in_code_block() {
        let mut r = MarkdownRenderer::new();
        r.line_buffer = "Hello ".to_string();
        r.line_start = true;
        r.in_code_block = true;
        let out = r.flush_on_whitespace();
        assert_eq!(out, "", "Should not flush inside code blocks");
    }

    #[test]
    fn test_md_streaming_whitespace_flush_integration() {
        // Full streaming simulation: tokens that arrive with trailing whitespace
        // at line start should flush via the whitespace path when the normal
        // needs_line_buffering check would hold them.
        let mut r = MarkdownRenderer::new();

        // "- " at line start triggers needs_line_buffering (could be list).
        // Then "not " arrives. The buffer is now "- not " which has a word
        // boundary. But try_resolve_block_prefix should handle "- not" as a
        // confirmed list item before flush_on_whitespace even fires.
        let out1 = r.render_delta("- ");
        let out2 = r.render_delta("not");
        let total = format!("{out1}{out2}");
        // Should have output — either from prefix resolution or whitespace flush
        assert!(
            total.contains("not") || !out2.is_empty(),
            "Content after list marker should stream, got out1='{out1}' out2='{out2}'"
        );
    }

    #[test]
    fn test_md_streaming_digit_with_space_stays_buffered() {
        // "3 " — starts with digit, needs_line_buffering holds it (could be "3. ").
        // flush_on_whitespace also guards against digits. So it stays buffered
        // until the content resolves. But adding more text ("items") makes
        // needs_line_buffering return false (contains ". " is false, len >= 3,
        // and it's not all digits followed by ". ").
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("3 ");
        // "3 " — buffered (digit start, flush_on_whitespace guards digits)
        // Actually, needs_line_buffering: trimmed="3 ", first byte is digit,
        // trimmed.len() >= 3? "3 " is 2 chars, so < 3, returns true (buffer).
        // Then try_resolve_block_prefix: digit, tries ordered list, no ". " found. Empty.
        // Then flush_on_whitespace: first byte is digit, guarded. Empty.
        // So out1 should be empty.

        let out2 = r.render_delta("items");
        // Buffer is now "3 items". needs_line_buffering: digit start, len >= 3,
        // contains ". "? No. So all(digit) on "3 items"[..?] — find(". ") returns None.
        // The match arm: trimmed.len() < 3 → false. trimmed.contains(". ") is false.
        // So the whole expression: false || false = false. needs_line_buffering returns false!
        // So it flushes as inline text.
        let total = format!("{out1}{out2}");
        assert!(
            total.contains("3") && total.contains("items"),
            "Digit-space-text should eventually produce output, got: '{total}'"
        );
    }

    #[test]
    fn test_md_flush_on_whitespace_each_token_produces_output() {
        // Simulate word-by-word streaming where each word ends with a space.
        // After the first word resolves the line start, subsequent words
        // should produce immediate output via the mid-line fast path.
        let mut r = MarkdownRenderer::new();
        let words = ["The ", "quick ", "brown ", "fox "];
        let mut outputs = Vec::new();
        for word in &words {
            outputs.push(r.render_delta(word));
        }
        // First word should produce output (resolves line start)
        assert!(
            !outputs[0].is_empty(),
            "First word 'The ' should flush immediately (not fence/header)"
        );
        // All subsequent words are mid-line, should produce output
        for (i, out) in outputs.iter().enumerate().skip(1) {
            assert!(
                !out.is_empty(),
                "Word {} should produce mid-line output, got empty",
                i
            );
        }
    }

    #[test]
    fn test_md_flush_on_whitespace_preserves_fence_detection() {
        // Ensure whitespace flush doesn't break fence detection.
        // "``` " could theoretically end with whitespace but should NOT flush
        // as inline text — it needs to be detected as a fence.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("```");
        assert_eq!(out, "", "Fence should buffer, not flush on whitespace");
        // Even with trailing space, the needs_line_buffering check fires first
        let out2 = r.render_delta(" ");
        // ``` + space = "``` " in buffer — needs_line_buffering still true (starts with `)
        // flush_on_whitespace shouldn't fire because needs_line_buffering resolved first
        assert_eq!(
            out2, "",
            "Fence with trailing space should still buffer for language detection"
        );
    }

    #[test]
    fn test_md_flush_on_whitespace_preserves_header_detection() {
        // "# " should not be flushed by whitespace — it's a header marker.
        // flush_on_whitespace guards against first-char '#'.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("# ");
        // The '#' triggers needs_line_buffering, try_resolve_block_prefix
        // doesn't handle headers, and flush_on_whitespace skips '#' content.
        // So "# " stays buffered.
        assert_eq!(
            out, "",
            "'# ' should remain buffered waiting for full header line"
        );

        // Complete the header line — should render with header styling
        let out2 = r.render_delta("Title\n");
        assert!(
            out2.contains("Title"),
            "Header should render when line completes, got: '{out2}'"
        );
    }

    #[test]
    fn test_md_plain_text_unchanged() {
        let out = render_full("just plain text\n");
        assert!(out.contains("just plain text"));
    }

    #[test]
    fn test_md_multiple_inline_codes_one_line() {
        let out = render_full("use `foo` and `bar` here\n");
        assert!(out.contains(&format!("{CYAN}foo{RESET}")));
        assert!(out.contains(&format!("{CYAN}bar{RESET}")));
    }

    #[test]
    fn test_md_code_block_preserves_content() {
        let input = "```\nfn main() {\n    println!(\"hello\");\n}\n```\n";
        let out = render_full(input);
        assert!(out.contains("fn main()"));
        assert!(out.contains("println!"));
    }

    // --- Markdown rendering: italic, lists, horizontal rules, blockquotes ---

    #[test]
    fn test_md_italic_text() {
        let out = render_full("this is *italic* text\n");
        assert!(
            out.contains(&format!("{ITALIC}italic{RESET}")),
            "Expected italic ANSI for *italic*, got: '{out}'"
        );
    }

    #[test]
    fn test_md_bold_still_works() {
        // Regression: bold must not break after adding italic support
        let out = render_full("this is **bold** text\n");
        assert!(
            out.contains(&format!("{BOLD}bold{RESET}")),
            "Expected bold ANSI for **bold**, got: '{out}'"
        );
    }

    #[test]
    fn test_md_bold_italic_text() {
        let out = render_full("this is ***both*** here\n");
        assert!(
            out.contains(&format!("{BOLD_ITALIC}both{RESET}")),
            "Expected bold+italic ANSI for ***both***, got: '{out}'"
        );
    }

    #[test]
    fn test_md_mixed_inline_formatting() {
        let out = render_full("**bold** and *italic* and `code`\n");
        assert!(
            out.contains(&format!("{BOLD}bold{RESET}")),
            "Missing bold in mixed line, got: '{out}'"
        );
        assert!(
            out.contains(&format!("{ITALIC}italic{RESET}")),
            "Missing italic in mixed line, got: '{out}'"
        );
        assert!(
            out.contains(&format!("{CYAN}code{RESET}")),
            "Missing code in mixed line, got: '{out}'"
        );
    }

    #[test]
    fn test_md_unclosed_italic_no_format() {
        // A single * at end of line without closing should NOT italicize
        let out = render_full("star *power\n");
        assert!(
            out.contains('*'),
            "Unclosed italic marker should pass through literally, got: '{out}'"
        );
        assert!(out.contains("power"));
    }

    #[test]
    fn test_md_unordered_list_dash() {
        let out = render_full("- first item\n");
        assert!(
            out.contains(&format!("{CYAN}•{RESET}")),
            "Expected colored bullet for '- item', got: '{out}'"
        );
        assert!(out.contains("first item"));
    }

    #[test]
    fn test_md_unordered_list_star() {
        let out = render_full("* second item\n");
        assert!(
            out.contains(&format!("{CYAN}•{RESET}")),
            "Expected colored bullet for '* item', got: '{out}'"
        );
        assert!(out.contains("second item"));
    }

    #[test]
    fn test_md_unordered_list_plus() {
        let out = render_full("+ third item\n");
        assert!(
            out.contains(&format!("{CYAN}•{RESET}")),
            "Expected colored bullet for '+ item', got: '{out}'"
        );
        assert!(out.contains("third item"));
    }

    #[test]
    fn test_md_unordered_list_with_inline_formatting() {
        let out = render_full("- a **bold** list item\n");
        assert!(out.contains(&format!("{CYAN}•{RESET}")));
        assert!(
            out.contains(&format!("{BOLD}bold{RESET}")),
            "List item content should get inline formatting, got: '{out}'"
        );
    }

    #[test]
    fn test_md_ordered_list() {
        let out = render_full("1. first\n");
        assert!(
            out.contains(&format!("{CYAN}1.{RESET}")),
            "Expected colored number for '1. first', got: '{out}'"
        );
        assert!(out.contains("first"));
    }

    #[test]
    fn test_md_ordered_list_larger_number() {
        let out = render_full("42. the answer\n");
        assert!(
            out.contains(&format!("{CYAN}42.{RESET}")),
            "Expected colored number for '42. item', got: '{out}'"
        );
        assert!(out.contains("the answer"));
    }

    #[test]
    fn test_md_horizontal_rule_dashes() {
        let out = render_full("---\n");
        assert!(
            out.contains("─"),
            "Expected horizontal rule rendering for '---', got: '{out}'"
        );
        assert!(
            out.contains(&format!("{DIM}")),
            "Horizontal rule should be dim, got: '{out}'"
        );
    }

    #[test]
    fn test_md_horizontal_rule_stars() {
        let out = render_full("***\n");
        assert!(
            out.contains("─"),
            "Expected horizontal rule rendering for '***', got: '{out}'"
        );
    }

    #[test]
    fn test_md_horizontal_rule_underscores() {
        let out = render_full("___\n");
        assert!(
            out.contains("─"),
            "Expected horizontal rule rendering for '___', got: '{out}'"
        );
    }

    #[test]
    fn test_md_horizontal_rule_long() {
        let out = render_full("----------\n");
        assert!(
            out.contains("─"),
            "Expected horizontal rule for long dashes, got: '{out}'"
        );
    }

    #[test]
    fn test_md_blockquote() {
        let out = render_full("> quoted text\n");
        assert!(
            out.contains(&format!("{DIM}│{RESET}")),
            "Expected dim vertical bar for blockquote, got: '{out}'"
        );
        assert!(
            out.contains(&format!("{ITALIC}quoted text{RESET}")),
            "Blockquote content should be italic, got: '{out}'"
        );
    }

    #[test]
    fn test_md_blockquote_with_inline_formatting() {
        let out = render_full("> a **bold** quote\n");
        assert!(out.contains(&format!("{DIM}│{RESET}")));
        // The content goes through render_inline, which processes bold inside the italic context
        assert!(out.contains("bold"));
    }

    #[test]
    fn test_md_indented_list_item() {
        let out = render_full("  - nested item\n");
        assert!(
            out.contains(&format!("{CYAN}•{RESET}")),
            "Indented list item should still get bullet, got: '{out}'"
        );
        assert!(out.contains("nested item"));
    }

    #[test]
    fn test_md_not_a_list_in_code_block() {
        // Inside code blocks, list markers should NOT be rendered as bullets
        let out = render_full("```\n- not a list\n```\n");
        assert!(
            !out.contains(&format!("{CYAN}•{RESET}")),
            "List markers inside code blocks should not get bullets, got: '{out}'"
        );
    }

    // --- Syntax highlighting tests ---

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

    #[test]
    fn test_spinner_frames_not_empty() {
        assert!(!SPINNER_FRAMES.is_empty());
    }

    #[test]
    fn test_spinner_frames_are_braille() {
        // All braille characters are in the Unicode range U+2800..U+28FF
        for &frame in SPINNER_FRAMES {
            assert!(
                ('\u{2800}'..='\u{28FF}').contains(&frame),
                "Expected braille character, got {:?}",
                frame
            );
        }
    }

    #[test]
    fn test_spinner_frame_cycling() {
        // First 10 frames should match SPINNER_FRAMES exactly
        for (i, &expected) in SPINNER_FRAMES.iter().enumerate() {
            assert_eq!(spinner_frame(i), expected);
        }
    }

    #[test]
    fn test_spinner_frame_wraps_around() {
        let len = SPINNER_FRAMES.len();
        // After one full cycle, it should repeat
        assert_eq!(spinner_frame(0), spinner_frame(len));
        assert_eq!(spinner_frame(1), spinner_frame(len + 1));
        assert_eq!(spinner_frame(2), spinner_frame(len + 2));
    }

    #[test]
    fn test_spinner_frame_large_index() {
        // Should not panic even with very large indices
        let frame = spinner_frame(999_999);
        assert!(SPINNER_FRAMES.contains(&frame));
    }

    #[test]
    fn test_spinner_frames_all_unique() {
        // Each frame in the animation should be distinct
        let mut seen = std::collections::HashSet::new();
        for &frame in SPINNER_FRAMES {
            assert!(seen.insert(frame), "Duplicate spinner frame: {:?}", frame);
        }
    }

    // --- format_edit_diff tests ---

    #[test]
    fn test_format_edit_diff_single_line_change() {
        let diff = format_edit_diff("old line", "new line");
        assert!(diff.contains("- old line"));
        assert!(diff.contains("+ new line"));
        // Should have red for removed, green for added
        assert!(diff.contains(&format!("{RED}")));
        assert!(diff.contains(&format!("{GREEN}")));
    }

    #[test]
    fn test_format_edit_diff_multi_line_change() {
        let old = "line 1\nline 2\nline 3";
        let new = "line A\nline B";
        let diff = format_edit_diff(old, new);
        assert!(diff.contains("- line 1"));
        assert!(diff.contains("- line 2"));
        assert!(diff.contains("- line 3"));
        assert!(diff.contains("+ line A"));
        assert!(diff.contains("+ line B"));
    }

    #[test]
    fn test_format_edit_diff_addition_only() {
        let diff = format_edit_diff("", "new content\nmore content");
        // No removed lines
        assert!(!diff.contains("- "));
        // Added lines present
        assert!(diff.contains("+ new content"));
        assert!(diff.contains("+ more content"));
    }

    #[test]
    fn test_format_edit_diff_deletion_only() {
        let diff = format_edit_diff("old content\nmore old", "");
        // Removed lines present
        assert!(diff.contains("- old content"));
        assert!(diff.contains("- more old"));
        // No added lines
        assert!(!diff.contains("+ "));
    }

    #[test]
    fn test_format_edit_diff_long_diff_truncation() {
        // Generate a diff with more than MAX_DIFF_LINES lines
        let old_lines: Vec<&str> = (0..15).map(|_| "old").collect();
        let new_lines: Vec<&str> = (0..15).map(|_| "new").collect();
        let old = old_lines.join("\n");
        let new = new_lines.join("\n");
        let diff = format_edit_diff(&old, &new);
        // Should be truncated — total would be 30 lines, max is 20
        assert!(diff.contains("more lines)"));
    }

    #[test]
    fn test_format_edit_diff_empty_both() {
        let diff = format_edit_diff("", "");
        assert!(diff.is_empty());
    }

    #[test]
    fn test_format_edit_diff_empty_old_text_new_file_section() {
        // Simulates adding new content to a file (old_text is empty)
        let diff = format_edit_diff("", "fn new_function() {\n    println!(\"hello\");\n}");
        assert!(!diff.contains("- "));
        assert!(diff.contains("+ fn new_function()"));
        assert!(diff.contains("+ }"));
    }

    #[test]
    fn test_format_edit_diff_short_diff_not_truncated() {
        let diff = format_edit_diff("a", "b");
        assert!(!diff.contains("more lines"));
    }

    // --- format_tool_summary write_file with line count ---

    #[test]
    fn test_format_tool_summary_write_file_with_content() {
        let args = serde_json::json!({"path": "out.txt", "content": "line1\nline2\nline3"});
        let result = format_tool_summary("write_file", &args);
        assert_eq!(result, "write out.txt (3 lines)");
    }

    #[test]
    fn test_format_tool_summary_write_file_single_line() {
        let args = serde_json::json!({"path": "out.txt", "content": "hello"});
        let result = format_tool_summary("write_file", &args);
        assert_eq!(result, "write out.txt (1 line)");
    }

    #[test]
    fn test_format_tool_summary_write_file_no_content() {
        let args = serde_json::json!({"path": "out.txt"});
        let result = format_tool_summary("write_file", &args);
        assert_eq!(result, "write out.txt");
    }

    // --- format_tool_summary enriched details ---

    #[test]
    fn test_format_tool_summary_read_file_with_offset_and_limit() {
        let args = serde_json::json!({"path": "src/main.rs", "offset": 10, "limit": 50});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs:10..60");
    }

    #[test]
    fn test_format_tool_summary_read_file_with_offset_only() {
        let args = serde_json::json!({"path": "src/main.rs", "offset": 100});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs:100..");
    }

    #[test]
    fn test_format_tool_summary_read_file_with_limit_only() {
        let args = serde_json::json!({"path": "src/main.rs", "limit": 25});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs (25 lines)");
    }

    #[test]
    fn test_format_tool_summary_read_file_no_extras() {
        let args = serde_json::json!({"path": "src/main.rs"});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs");
    }

    #[test]
    fn test_format_tool_summary_edit_file_with_text() {
        let args = serde_json::json!({
            "path": "foo.rs",
            "old_text": "fn old() {\n}\n",
            "new_text": "fn new() {\n    // improved\n    do_stuff();\n}\n"
        });
        let result = format_tool_summary("edit_file", &args);
        assert_eq!(result, "edit foo.rs (2 → 4 lines)");
    }

    #[test]
    fn test_format_tool_summary_edit_file_no_text() {
        let args = serde_json::json!({"path": "foo.rs"});
        let result = format_tool_summary("edit_file", &args);
        assert_eq!(result, "edit foo.rs");
    }

    #[test]
    fn test_format_tool_summary_edit_file_same_lines() {
        let args = serde_json::json!({
            "path": "foo.rs",
            "old_text": "let x = 1;",
            "new_text": "let x = 2;"
        });
        let result = format_tool_summary("edit_file", &args);
        assert_eq!(result, "edit foo.rs (1 → 1 lines)");
    }

    #[test]
    fn test_format_tool_summary_search_with_path() {
        let args = serde_json::json!({"pattern": "TODO", "path": "src/"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'TODO' in src/");
    }

    #[test]
    fn test_format_tool_summary_search_with_include() {
        let args = serde_json::json!({"pattern": "fn main", "include": "*.rs"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'fn main' (*.rs)");
    }

    #[test]
    fn test_format_tool_summary_search_with_path_and_include() {
        let args = serde_json::json!({"pattern": "test", "path": "src/", "include": "*.rs"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'test' in src/ (*.rs)");
    }

    #[test]
    fn test_format_tool_summary_search_pattern_only() {
        let args = serde_json::json!({"pattern": "TODO"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'TODO'");
    }

    #[test]
    fn test_format_tool_summary_list_files_with_pattern() {
        let args = serde_json::json!({"path": "src/", "pattern": "*.rs"});
        let result = format_tool_summary("list_files", &args);
        assert_eq!(result, "ls src/ (*.rs)");
    }

    #[test]
    fn test_format_tool_summary_list_files_pattern_no_path() {
        let args = serde_json::json!({"pattern": "*.toml"});
        let result = format_tool_summary("list_files", &args);
        assert_eq!(result, "ls . (*.toml)");
    }

    #[test]
    fn test_format_tool_summary_bash_multiline_shows_first_line() {
        let args = serde_json::json!({"command": "cd src\ngrep -r 'test' ."});
        let result = format_tool_summary("bash", &args);
        assert!(
            result.starts_with("$ cd src"),
            "Should show first line: {result}"
        );
        assert!(
            result.contains("(2 lines)"),
            "Should indicate line count: {result}"
        );
    }

    // --- pluralize ---

    #[test]
    fn test_pluralize_singular() {
        assert_eq!(pluralize(1, "line", "lines"), "line");
        assert_eq!(pluralize(1, "file", "files"), "file");
    }

    #[test]
    fn test_pluralize_plural() {
        assert_eq!(pluralize(0, "line", "lines"), "lines");
        assert_eq!(pluralize(2, "line", "lines"), "lines");
        assert_eq!(pluralize(100, "file", "files"), "files");
    }

    // --- truncate_tool_output tests ---

    #[test]
    fn test_truncate_tool_output_under_threshold_unchanged() {
        let short = "hello world\nsecond line\nthird line";
        let result = truncate_tool_output(short, 30_000);
        assert_eq!(result, short);
    }

    #[test]
    fn test_truncate_tool_output_empty_string() {
        let result = truncate_tool_output("", 30_000);
        assert_eq!(result, "");
    }

    #[test]
    fn test_truncate_tool_output_exactly_at_threshold() {
        // Create output exactly at the threshold
        let line = "x".repeat(100);
        let lines: Vec<String> = (0..300).map(|_| line.clone()).collect();
        let output = lines.join("\n");
        // If it's at or under threshold length, it should be unchanged
        let result = truncate_tool_output(&output, output.len());
        assert_eq!(result, output);
    }

    #[test]
    fn test_truncate_tool_output_over_threshold_has_marker() {
        // Create output with 200 lines, each long enough to exceed 30k chars
        let line = "x".repeat(200);
        let lines: Vec<String> = (0..200).map(|i| format!("line{i}: {line}")).collect();
        let output = lines.join("\n");
        assert!(output.len() > 30_000);

        let result = truncate_tool_output(&output, 30_000);
        assert!(result.contains("[... truncated"));
        assert!(result.contains("lines ...]"));
        // Should contain head lines
        assert!(result.contains("line0:"));
        assert!(result.contains("line99:"));
        // Should contain tail lines
        assert!(result.contains("line199:"));
        assert!(result.contains("line150:"));
        // Should NOT contain middle lines
        assert!(!result.contains("line100:"));
        assert!(!result.contains("line120:"));
    }

    #[test]
    fn test_truncate_tool_output_preserves_head_and_tail_count() {
        // 300 lines, each 200 chars → ~60k chars, well over 30k threshold
        let lines: Vec<String> = (0..300).map(|i| format!("{:>200}", i)).collect();
        let output = lines.join("\n");

        let result = truncate_tool_output(&output, 30_000);
        let _result_lines: Vec<&str> = result.lines().collect();

        // Head: first 100 lines should be present
        for i in 0..100 {
            let expected = format!("{:>200}", i);
            assert!(result.contains(&expected), "Missing head line {i}");
        }

        // Tail: last 50 lines should be present
        for i in 250..300 {
            let expected = format!("{:>200}", i);
            assert!(result.contains(&expected), "Missing tail line {i}");
        }

        // Middle should be omitted
        assert!(!result.contains(&format!("{:>200}", 150)));

        // Marker should show correct count
        // 300 - 100 - 50 = 150 omitted lines
        assert!(result.contains("[... truncated 150 lines ...]"));

        // Result should be shorter than original
        assert!(result.len() < output.len());
    }

    #[test]
    fn test_truncate_tool_output_few_long_lines_not_truncated() {
        // Only 140 lines (< head + tail = 150), even if over char threshold
        // Should NOT be truncated because there aren't enough lines
        let line = "x".repeat(500);
        let lines: Vec<String> = (0..140).map(|_| line.clone()).collect();
        let output = lines.join("\n");
        assert!(output.len() > 30_000);

        let result = truncate_tool_output(&output, 30_000);
        assert_eq!(
            result, output,
            "Too few lines to truncate, should be unchanged"
        );
    }

    #[test]
    fn test_truncate_tool_output_single_truncated_line_in_marker() {
        // 152 lines → head 100 + tail 50 + 2 omitted
        // But 2 omitted uses "lines" (plural)
        // 151 lines → 1 omitted → "line" (singular)
        let line = "x".repeat(300);
        let lines: Vec<String> = (0..151).map(|_| line.clone()).collect();
        let output = lines.join("\n");
        assert!(output.len() > 30_000);

        let result = truncate_tool_output(&output, 30_000);
        assert!(result.contains("[... truncated 1 line ...]"));
    }

    #[test]
    fn test_truncate_tool_output_default_threshold_constant() {
        // Verify the default constant is 30,000
        assert_eq!(TOOL_OUTPUT_MAX_CHARS, 30_000);
    }

    // ── decode_html_entities tests ──────────────────────────────────

    #[test]
    fn test_decode_html_entities_named() {
        assert_eq!(decode_html_entities("&amp;"), "&");
        assert_eq!(decode_html_entities("&lt;"), "<");
        assert_eq!(decode_html_entities("&gt;"), ">");
        assert_eq!(decode_html_entities("&quot;"), "\"");
        assert_eq!(decode_html_entities("&apos;"), "'");
        assert_eq!(decode_html_entities("&#39;"), "'");
        assert_eq!(decode_html_entities("&nbsp;"), " ");
        assert_eq!(decode_html_entities("&#x27;"), "'");
        assert_eq!(decode_html_entities("&mdash;"), "—");
        assert_eq!(decode_html_entities("&ndash;"), "–");
        assert_eq!(decode_html_entities("&hellip;"), "…");
        assert_eq!(decode_html_entities("&copy;"), "©");
        assert_eq!(decode_html_entities("&reg;"), "®");
    }

    #[test]
    fn test_decode_html_entities_numeric_decimal() {
        // &#65; = 'A'
        assert_eq!(decode_html_entities("&#65;"), "A");
        // &#8212; = '—' (em dash)
        assert_eq!(decode_html_entities("&#8212;"), "—");
    }

    #[test]
    fn test_decode_html_entities_numeric_hex() {
        // &#x41; = 'A'
        assert_eq!(decode_html_entities("&#x41;"), "A");
        // &#x2014; = '—' (em dash)
        assert_eq!(decode_html_entities("&#x2014;"), "—");
    }

    #[test]
    fn test_decode_html_entities_mixed() {
        assert_eq!(
            decode_html_entities("hello &amp; world &lt;3 &#8212; done"),
            "hello & world <3 — done"
        );
    }

    #[test]
    fn test_decode_html_entities_no_entities() {
        assert_eq!(decode_html_entities("plain text"), "plain text");
    }

    #[test]
    fn test_decode_html_entities_invalid_numeric() {
        // Invalid numeric entity — should be preserved as-is
        assert_eq!(decode_html_entities("&#xZZZZ;"), "&#xZZZZ;");
        assert_eq!(decode_html_entities("&#abc;"), "&#abc;");
    }

    #[test]
    fn test_decode_html_entities_incomplete() {
        // Ampersand not part of an entity
        assert_eq!(decode_html_entities("a & b"), "a & b");
    }

    // --- Section header and divider tests ---

    #[test]
    fn test_section_header_contains_label_and_line_chars() {
        let header = section_header("Thinking");
        assert!(
            header.contains("Thinking"),
            "header should contain the label"
        );
        assert!(
            header.contains("─"),
            "header should contain box-drawing chars"
        );
    }

    #[test]
    fn test_section_header_empty_label_produces_divider() {
        let header = section_header("");
        // Empty label should produce the same as section_divider
        let divider = section_divider();
        assert_eq!(header, divider);
    }

    #[test]
    fn test_section_divider_nonempty_with_line_chars() {
        let divider = section_divider();
        assert!(!divider.is_empty(), "divider should not be empty");
        assert!(
            divider.contains("─"),
            "divider should contain box-drawing chars"
        );
    }

    #[test]
    fn test_section_header_no_color() {
        // When NO_COLOR is set, the output still contains the label and line chars
        // (Color codes render as empty strings, but the structural content remains)
        let header = section_header("Tools");
        assert!(header.contains("Tools"));
        assert!(header.contains("─"));
    }

    #[test]
    fn test_section_divider_no_color() {
        let divider = section_divider();
        assert!(divider.contains("─"));
    }

    #[test]
    fn test_terminal_width_default() {
        // terminal_width should return a reasonable default (80) when COLUMNS is not set
        // or it should return the value of COLUMNS if set
        let width = terminal_width();
        assert!(width > 0, "terminal width should be positive");
    }

    #[test]
    fn test_section_header_with_various_labels() {
        // Test with different labels to ensure formatting works
        for label in &[
            "Thinking",
            "Response",
            "A",
            "Very Long Section Label For Testing",
        ] {
            let header = section_header(label);
            assert!(header.contains(label), "header should contain '{}'", label);
            assert!(header.contains("──"), "header should have line prefix");
        }
    }

    // ── tool batch summary tests ──────────────────────────────────

    #[test]
    fn test_tool_batch_summary_single_tool_returns_empty() {
        let result = format_tool_batch_summary(1, 1, 0, Duration::from_millis(500));
        assert!(
            result.is_empty(),
            "single tool batch should not produce summary"
        );
    }

    #[test]
    fn test_tool_batch_summary_zero_tools_returns_empty() {
        let result = format_tool_batch_summary(0, 0, 0, Duration::from_millis(0));
        assert!(result.is_empty(), "zero tools should not produce summary");
    }

    #[test]
    fn test_tool_batch_summary_all_succeed() {
        let result = format_tool_batch_summary(3, 3, 0, Duration::from_millis(1200));
        assert!(result.contains("3 tools"), "should show tool count");
        assert!(result.contains("1.2s"), "should show duration");
        assert!(result.contains("3"), "should show success count");
        assert!(result.contains("✓"), "should show success marker");
        // When all succeed, no failure count shown
        assert!(
            !result.contains("✗"),
            "should not show failure marker when all succeed"
        );
    }

    #[test]
    fn test_tool_batch_summary_with_failures() {
        let result = format_tool_batch_summary(4, 3, 1, Duration::from_millis(2500));
        assert!(result.contains("4 tools"), "should show total count");
        assert!(result.contains("2.5s"), "should show duration");
        assert!(result.contains("3"), "should show success count");
        assert!(result.contains("✓"), "should show success marker");
        assert!(result.contains("1"), "should show failure count");
        assert!(result.contains("✗"), "should show failure marker");
    }

    #[test]
    fn test_tool_batch_summary_two_tools_plural() {
        let result = format_tool_batch_summary(2, 2, 0, Duration::from_millis(800));
        assert!(result.contains("2 tools"), "should pluralize 'tools'");
        assert!(result.contains("800ms"), "should show ms for sub-second");
    }

    // ── indent tool output tests ──────────────────────────────────

    #[test]
    fn test_indent_tool_output_empty() {
        assert_eq!(indent_tool_output(""), "");
    }

    #[test]
    fn test_indent_tool_output_single_line() {
        let result = indent_tool_output("hello world");
        assert!(result.contains("│"), "should have indent marker");
        assert!(result.contains("hello world"), "should preserve content");
    }

    #[test]
    fn test_indent_tool_output_multiline() {
        let result = indent_tool_output("line 1\nline 2\nline 3");
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3, "should preserve line count");
        for line in &lines {
            assert!(line.contains("│"), "each line should have indent marker");
        }
        assert!(lines[0].contains("line 1"));
        assert!(lines[1].contains("line 2"));
        assert!(lines[2].contains("line 3"));
    }

    // ── turn boundary tests ──────────────────────────────────

    #[test]
    fn test_turn_boundary_contains_number() {
        let result = turn_boundary(1);
        assert!(result.contains("Turn 1"), "should show turn number");
        assert!(result.contains("╭"), "should have box-drawing start");
        assert!(result.contains("╮"), "should have box-drawing end");
    }

    #[test]
    fn test_turn_boundary_different_numbers() {
        for n in [1, 5, 10, 99] {
            let result = turn_boundary(n);
            assert!(
                result.contains(&format!("Turn {n}")),
                "should contain Turn {n}"
            );
        }
    }

    #[test]
    fn test_turn_boundary_has_fill_characters() {
        let result = turn_boundary(1);
        assert!(result.contains("─"), "should have fill characters");
    }

    // --- Streaming latency tests (issue #147) ---

    #[test]
    fn test_md_code_block_indented_line_resolves_immediately() {
        // Indented code lines like "    let x = 1;" should resolve at line start
        // without waiting for more tokens — a closing fence never has leading spaces
        // before the backticks (in CommonMark, ≤3 spaces are allowed, but the first
        // non-space char must be `\``). Content starting with spaces followed by a
        // non-backtick char should early-resolve.
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```rust\n");
        assert!(r.in_code_block);

        // Indented code at line start — should resolve immediately
        let out = r.render_delta("    let x");
        assert!(
            !out.is_empty(),
            "Indented code block content should resolve immediately at line start, got empty"
        );
        assert!(
            out.contains("let x"),
            "Should contain the code text, got: '{out}'"
        );
    }

    #[test]
    fn test_md_code_block_space_only_token_buffers() {
        // A token that is only whitespace at code block line start should buffer
        // because we don't yet know what follows
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```\n");
        assert!(r.in_code_block);

        // Just spaces — ambiguous, should buffer
        let out = r.render_delta("  ");
        // This may or may not emit — it's okay either way as long as
        // subsequent non-fence content resolves quickly
        let _ = out; // don't assert on whitespace-only

        // Follow-up with non-fence content should resolve
        let out2 = r.render_delta("code");
        assert!(
            !out2.is_empty(),
            "Content after whitespace should resolve, got empty"
        );
    }

    #[test]
    fn test_md_render_delta_every_call_produces_or_buffers_minimally() {
        // Simulate a realistic streaming sequence and verify tokens aren't
        // held longer than necessary. Each non-ambiguous mid-line token should
        // produce output on the same call.
        let mut r = MarkdownRenderer::new();
        // First token resolves line start
        let out1 = r.render_delta("Here is ");
        assert!(!out1.is_empty(), "First token should resolve");

        // Each subsequent mid-line token must produce output immediately
        let tokens = ["a ", "sentence ", "with ", "multiple ", "tokens."];
        for token in &tokens {
            let out = r.render_delta(token);
            assert!(
                !out.is_empty(),
                "Mid-line token '{token}' should produce immediate output"
            );
        }
    }

    #[test]
    fn test_md_flush_produces_output_for_buffered_content() {
        // flush() should emit any content still in the line buffer
        let mut r = MarkdownRenderer::new();
        // Send a partial line that gets buffered at line start
        let out = r.render_delta("#");
        assert_eq!(out, "", "# should buffer at line start");

        // flush() should emit the buffered content
        let flushed = r.flush();
        assert!(
            !flushed.is_empty(),
            "flush() should emit buffered '#' content"
        );
    }

    #[test]
    fn test_md_code_block_backtick_start_buffers_correctly() {
        // A token starting with ` at code block line start must buffer
        // (could be closing fence ```)
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("```\n");
        let _ = r.render_delta("content\n");

        // Backtick at line start — could be closing fence
        let out = r.render_delta("`");
        assert_eq!(
            out, "",
            "Single backtick at code block line start should buffer"
        );

        // Complete the closing fence
        let out2 = r.render_delta("``\n");
        assert!(!r.in_code_block, "Should have closed the code block");
        assert!(!out2.is_empty(), "Closing fence should produce output");
    }

    // --- render_latency_budget: document the expected flush behavior ---
    //
    // The streaming pipeline has the following latency budget per text delta:
    //
    // 1. Spinner stop (first token only): ~0.1ms
    //    - Synchronous eprint!("\r\x1b[K") + stderr flush
    //    - Sends cancel signal to async spinner task
    //    - Aborts the spawned task handle
    //
    // 2. MarkdownRenderer::render_delta(): ~0 allocation for mid-line tokens
    //    - Mid-line fast path: no buffering, immediate String return
    //    - Line-start: buffers 1-4 chars for fence/header detection
    //    - Code block line-start: buffers until first non-backtick char
    //
    // 3. print!() + io::stdout().flush(): system call, ~0.01ms
    //    - Called after every render_delta that produces output
    //    - Ensures tokens are visible immediately, not batched by stdio
    //
    // Total per-token latency: <0.2ms for first token, <0.05ms for subsequent
    // The bottleneck is always the network/API, not the renderer.

    // --- Digit-word and dash-word early flush tests (issue #147) ---

    #[test]
    fn test_streaming_digit_nonlist_flushes_early() {
        // "2n" at line start — digit followed by a letter can't be a numbered list.
        // Should flush on the 2nd char since 'n' isn't '.' or ')'.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("2n");
        // "2n" should flush immediately — not a numbered list pattern
        assert!(
            !out1.is_empty(),
            "Digit followed by letter should flush immediately, got empty"
        );
        // Subsequent token is mid-line, should be immediate
        let out2 = r.render_delta("d");
        assert!(
            !out2.is_empty(),
            "Mid-line token after digit-word flush should be immediate, got empty"
        );
    }

    #[test]
    fn test_streaming_dash_nonlist_flushes_early() {
        // "-b" at line start — dash followed by a non-space, non-dash char
        // can't be a list item or horizontal rule. Should flush immediately.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("-b");
        assert!(
            !out1.is_empty(),
            "Dash followed by letter should flush immediately, got empty"
        );
        // Subsequent token is mid-line
        let out2 = r.render_delta("ased");
        assert!(
            !out2.is_empty(),
            "Mid-line token after dash-word flush should be immediate, got empty"
        );
    }

    #[test]
    fn test_streaming_numbered_list_still_buffers() {
        // "1." at line start — could be a numbered list, must keep buffering.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("1.");
        // "1." — digit followed by '.', still ambiguous (could be "1. item")
        assert!(
            out1.is_empty(),
            "Digit-dot should still buffer (potential numbered list), got: '{out1}'"
        );
        // "1. " confirms it's a list — should resolve via try_resolve_block_prefix
        let out2 = r.render_delta(" item");
        assert!(
            !out2.is_empty(),
            "Numbered list '1. item' should eventually produce output, got empty"
        );
    }

    #[test]
    fn test_streaming_dash_list_still_buffers() {
        // "- " at line start is a list item — should buffer correctly.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("- ");
        // "- " is a confirmed unordered list item
        // try_resolve_block_prefix should handle it
        // Whether it's empty or not depends on whether prefix resolves at "- "
        // The key: subsequent content should stream
        let out2 = r.render_delta("item");
        let total = format!("{out1}{out2}");
        assert!(
            total.contains("item"),
            "Dash list '- item' should produce output, got: '{total}'"
        );
    }

    #[test]
    fn test_streaming_dash_hr_still_buffers() {
        // "---" should still buffer as a potential horizontal rule.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("-");
        assert!(
            out1.is_empty(),
            "Single dash should buffer (ambiguous), got: '{out1}'"
        );
        let out2 = r.render_delta("-");
        assert!(
            out2.is_empty(),
            "Double dash should buffer (potential HR), got: '{out2}'"
        );
        let out3 = r.render_delta("-");
        // "---" is a horizontal rule, should still be buffered/handled correctly
        assert!(
            out3.is_empty(),
            "Triple dash should still buffer as HR, got: '{out3}'"
        );
    }

    #[test]
    fn test_streaming_mid_line_always_immediate() {
        // Once line_start is false, ALL tokens should be immediate regardless of content.
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("Hello ");
        assert!(!r.line_start, "Should be mid-line after 'Hello '");

        // Tokens that would trigger buffering at line start should be immediate mid-line
        for token in &["-", "1.", "```", "#", ">", "---"] {
            let out = r.render_delta(token);
            assert!(
                !out.is_empty(),
                "Mid-line token '{token}' should produce immediate output, got empty"
            );
        }
    }

    #[test]
    fn test_streaming_fence_still_buffers() {
        // "```" at line start should still buffer as a code fence.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("`");
        assert!(
            out1.is_empty(),
            "Single backtick should buffer, got: '{out1}'"
        );
        let out2 = r.render_delta("``");
        // Now buffer is "```" — still buffering as potential fence
        assert!(
            out2.is_empty(),
            "Triple backtick without newline should still buffer, got: '{out2}'"
        );
        // A newline confirms the fence
        let out3 = r.render_delta("\n");
        assert!(
            r.in_code_block,
            "Code fence should be detected after newline"
        );
        assert!(
            !out3.is_empty(),
            "Fence line should produce output on newline"
        );
    }

    #[test]
    fn test_streaming_plain_text_immediate() {
        // "Hello" at line start — first char 'H' is not special, should flush immediately.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("H");
        assert!(
            !out.is_empty(),
            "Non-special char 'H' at line start should flush immediately, got empty"
        );
    }

    #[test]
    fn test_streaming_digit_paren_still_buffers() {
        // "1)" at line start — digit followed by ')', could be a numbered list variant.
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("1)");
        assert!(
            out.is_empty(),
            "Digit-paren should still buffer (potential list), got: '{out}'"
        );
    }

    #[test]
    fn test_streaming_multi_digit_nonlist_flushes() {
        // "100m" — multi-digit number followed by letter, not a list.
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("10");
        // "10" — all digits, could still be "10. " — should buffer
        assert!(
            out1.is_empty(),
            "All-digit '10' should buffer (could be list number), got: '{out1}'"
        );
        let out2 = r.render_delta("0m");
        // "100m" — the 'm' disambiguates: not a list number
        assert!(
            !out2.is_empty(),
            "'100m' should flush — letter after digits means not a list, got empty"
        );
    }

    #[test]
    fn test_md_render_delta_latency_budget_mid_line() {
        // Verify the mid-line fast path produces output without allocating
        // a line buffer — this is the hot path for streaming latency.
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("Start ");
        assert!(!r.line_start, "Should be mid-line after first token");

        // Mid-line token should not touch line_buffer
        let out = r.render_delta("word");
        assert!(!out.is_empty(), "Mid-line should produce output");
        assert!(
            r.line_buffer.is_empty(),
            "Mid-line fast path should not use line_buffer"
        );
    }

    // --- Live tool progress formatting tests ---

    #[test]
    fn test_format_duration_live_seconds() {
        assert_eq!(format_duration_live(Duration::from_secs(0)), "0s");
        assert_eq!(format_duration_live(Duration::from_secs(5)), "5s");
        assert_eq!(format_duration_live(Duration::from_secs(59)), "59s");
    }

    #[test]
    fn test_format_duration_live_minutes() {
        assert_eq!(format_duration_live(Duration::from_secs(60)), "1m");
        assert_eq!(format_duration_live(Duration::from_secs(65)), "1m 5s");
        assert_eq!(format_duration_live(Duration::from_secs(120)), "2m");
        assert_eq!(format_duration_live(Duration::from_secs(3599)), "59m 59s");
    }

    #[test]
    fn test_format_duration_live_hours() {
        assert_eq!(format_duration_live(Duration::from_secs(3600)), "1h");
        assert_eq!(format_duration_live(Duration::from_secs(3660)), "1h 1m");
        assert_eq!(format_duration_live(Duration::from_secs(7200)), "2h");
    }

    #[test]
    fn test_format_tool_progress_no_lines() {
        let output = format_tool_progress("bash", Duration::from_secs(5), 0, None);
        assert!(output.contains("bash"), "should contain tool name");
        assert!(output.contains("⏱"), "should contain timer emoji");
        assert!(output.contains("5s"), "should contain elapsed time");
        // Should contain spinner frame
        assert!(
            output.contains('⠋'),
            "should contain spinner frame for tick 0"
        );
    }

    #[test]
    fn test_format_tool_progress_with_lines() {
        let output = format_tool_progress("bash", Duration::from_secs(12), 3, Some(142));
        assert!(output.contains("bash"), "should contain tool name");
        assert!(output.contains("12s"), "should contain elapsed time");
        assert!(output.contains("142 lines"), "should contain line count");
    }

    #[test]
    fn test_format_tool_progress_single_line() {
        let output = format_tool_progress("bash", Duration::from_secs(1), 0, Some(1));
        assert!(output.contains("1 line"), "should use singular 'line'");
        assert!(!output.contains("1 lines"), "should not use plural for 1");
    }

    #[test]
    fn test_format_tool_progress_zero_lines_hidden() {
        let output = format_tool_progress("bash", Duration::from_secs(3), 0, Some(0));
        assert!(!output.contains("line"), "zero lines should be hidden");
    }

    #[test]
    fn test_format_partial_tail_empty() {
        assert_eq!(format_partial_tail("", 3), "");
    }

    #[test]
    fn test_format_partial_tail_zero_lines() {
        assert_eq!(format_partial_tail("hello\nworld", 0), "");
    }

    #[test]
    fn test_format_partial_tail_fewer_lines_than_max() {
        let output = format_partial_tail("line1\nline2", 5);
        assert!(output.contains("line1"), "should show all lines");
        assert!(output.contains("line2"), "should show all lines");
        assert!(
            !output.contains("above"),
            "should not show 'above' indicator"
        );
    }

    #[test]
    fn test_format_partial_tail_more_lines_than_max() {
        let output = format_partial_tail("line1\nline2\nline3\nline4\nline5", 2);
        assert!(!output.contains("line1"), "should not show early lines");
        assert!(!output.contains("line2"), "should not show early lines");
        assert!(!output.contains("line3"), "should not show line3");
        assert!(output.contains("line4"), "should show tail lines");
        assert!(output.contains("line5"), "should show tail lines");
        assert!(output.contains("3 lines above"), "should show skip count");
    }

    #[test]
    fn test_format_partial_tail_uses_pipe_indent() {
        let output = format_partial_tail("hello", 1);
        assert!(
            output.contains("┆"),
            "should use dotted pipe for indentation"
        );
    }

    #[test]
    fn test_count_result_lines() {
        let result = ToolResult {
            content: vec![Content::Text {
                text: "line1\nline2\nline3".to_string(),
            }],
            details: serde_json::Value::Null,
        };
        assert_eq!(count_result_lines(&result), 3);
    }

    #[test]
    fn test_count_result_lines_empty() {
        let result = ToolResult {
            content: vec![],
            details: serde_json::Value::Null,
        };
        assert_eq!(count_result_lines(&result), 0);
    }

    #[test]
    fn test_extract_result_text() {
        let result = ToolResult {
            content: vec![
                Content::Text {
                    text: "hello".to_string(),
                },
                Content::Text {
                    text: "world".to_string(),
                },
            ],
            details: serde_json::Value::Null,
        };
        assert_eq!(extract_result_text(&result), "hello\nworld");
    }

    #[test]
    fn test_extract_result_text_empty() {
        let result = ToolResult {
            content: vec![],
            details: serde_json::Value::Null,
        };
        assert_eq!(extract_result_text(&result), "");
    }

    #[test]
    fn test_active_tool_state_new() {
        let state = ActiveToolState::new("bash".to_string());
        assert_eq!(state.tool_name, "bash");
        assert_eq!(state.line_count, 0);
        assert!(state.last_output.is_empty());
    }

    #[test]
    fn test_active_tool_state_update_partial() {
        let mut state = ActiveToolState::new("bash".to_string());
        state.update_partial("line1\nline2\nline3");
        assert_eq!(state.line_count, 3);
        assert_eq!(state.last_output, "line1\nline2\nline3");
    }
}
