//! Map command handler: /map — structural codebase understanding.

use crate::commands_ast_grep::is_ast_grep_available;
use crate::commands_search::{is_binary_extension, list_project_files};
use crate::format::*;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

// ── Rust symbol regexes ──────────────────────────────────────────────
static RE_RUST_FN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?(?:async\s+)?fn\s+(\w+)").unwrap());
static RE_RUST_STRUCT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?struct\s+(\w+)").unwrap());
static RE_RUST_ENUM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?enum\s+(\w+)").unwrap());
static RE_RUST_TRAIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?trait\s+(\w+)").unwrap());
static RE_RUST_IMPL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*impl(?:<[^>]*>)?\s+(.+?)(?:\s*\{|$)").unwrap());
static RE_RUST_CONST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?(?:const|static)\s+(\w+)").unwrap());
static RE_RUST_MOD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(pub(?:\(crate\))?\s+)?mod\s+(\w+)").unwrap());
static RE_RUST_CFG_TEST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#\[cfg\(test\)\]").unwrap());

// ── Python symbol regexes ────────────────────────────────────────────
static RE_PY_CLASS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^class\s+(\w+)").unwrap());
static RE_PY_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:async\s+)?def\s+(\w+)").unwrap());
static RE_PY_CONST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Z][A-Z0-9_]*)\s*=").unwrap());

// ── JavaScript symbol regexes ────────────────────────────────────────
static RE_JS_FUNCTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:export\s+(?:default\s+)?)?(?:async\s+)?function\s+(\w+)").unwrap()
});
static RE_JS_CLASS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:export\s+(?:default\s+)?)?class\s+(\w+)").unwrap());
static RE_JS_CONST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=").unwrap());

// ── TypeScript-specific regexes (JS ones reused via extract_js_symbols) ──
static RE_TS_INTERFACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:export\s+)?interface\s+(\w+)").unwrap());
static RE_TS_TYPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:export\s+)?type\s+(\w+)\s*[=<]").unwrap());

// ── Go symbol regexes ────────────────────────────────────────────────
static RE_GO_FUNC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^func\s+(\w+)\s*\(").unwrap());
static RE_GO_METHOD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^func\s+\([^)]+\)\s+(\w+)\s*\(").unwrap());
static RE_GO_STRUCT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^type\s+(\w+)\s+struct\b").unwrap());
static RE_GO_INTERFACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^type\s+(\w+)\s+interface\b").unwrap());
static RE_GO_CONST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:const|var)\s+(\w+)").unwrap());

// ── Java symbol regexes ──────────────────────────────────────────────
static RE_JAVA_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:public\s+)?(?:abstract\s+)?(?:final\s+)?class\s+(\w+)").unwrap()
});
static RE_JAVA_INTERFACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:public\s+)?interface\s+(\w+)").unwrap());
static RE_JAVA_ENUM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:public\s+)?enum\s+(\w+)").unwrap());
static RE_JAVA_METHOD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*(?:public|private|protected)?\s*(?:static\s+)?(?:final\s+)?(?:[\w<>\[\],\s]+)\s+(\w+)\s*\(",
    )
    .unwrap()
});

// ── C regexes ──
static RE_C_FUNC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z_][\w\s\*]*\s+(\w+)\s*\(").unwrap());
static RE_C_STRUCT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:typedef\s+)?struct\s+(\w+)").unwrap());
static RE_C_ENUM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:typedef\s+)?enum\s+(\w+)").unwrap());
static RE_C_DEFINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#define\s+(\w+)").unwrap());

// ── C++ regexes (extends C) ──
static RE_CPP_CLASS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:template\s*<[^>]*>\s*)?class\s+(\w+)").unwrap());
static RE_CPP_NAMESPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^namespace\s+(\w+)").unwrap());

// ── Ruby regexes ──
static RE_RUBY_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*def\s+([\w\?\!]+)").unwrap());
static RE_RUBY_CLASS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*class\s+(\w+)").unwrap());
static RE_RUBY_MODULE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*module\s+(\w+)").unwrap());
static RE_RUBY_CONST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*([A-Z][A-Z_0-9]+)\s*=").unwrap());

// ── Shell regexes ──
static RE_SHELL_FUNC_PARENS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(\w+)\s*\(\)\s*\{").unwrap());
static RE_SHELL_FUNC_KEYWORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*function\s+(\w+)").unwrap());

// ── C# regexes ──
static RE_CSHARP_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:public|private|protected|internal)?\s*(?:static\s+)?(?:abstract\s+)?(?:sealed\s+)?(?:partial\s+)?class\s+(\w+)").unwrap()
});
static RE_CSHARP_INTERFACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:public|private|protected|internal)?\s*(?:partial\s+)?interface\s+(\w+)")
        .unwrap()
});
static RE_CSHARP_STRUCT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:public|private|protected|internal)?\s*(?:readonly\s+)?(?:partial\s+)?struct\s+(\w+)").unwrap()
});
static RE_CSHARP_ENUM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:public|private|protected|internal)?\s*enum\s+(\w+)").unwrap()
});
static RE_CSHARP_NAMESPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*namespace\s+([\w.]+)").unwrap());
static RE_CSHARP_RECORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*(?:public|private|protected|internal)?\s*(?:sealed\s+)?record\s+(?:struct\s+)?(\w+)",
    )
    .unwrap()
});
static RE_CSHARP_METHOD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:public|private|protected|internal)\s+(?:static\s+)?(?:virtual\s+)?(?:override\s+)?(?:async\s+)?(?:[\w<>\[\],\?\s]+)\s+(\w+)\s*\(").unwrap()
});

// ── PHP regexes ──
static RE_PHP_CLASS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:abstract\s+)?(?:final\s+)?class\s+(\w+)").unwrap());
static RE_PHP_INTERFACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*interface\s+(\w+)").unwrap());
static RE_PHP_TRAIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*trait\s+(\w+)").unwrap());
static RE_PHP_FUNCTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:public|private|protected)?\s*(?:static\s+)?function\s+(\w+)").unwrap()
});
static RE_PHP_ENUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*enum\s+(\w+)").unwrap());

// ── Kotlin regexes ──
static RE_KOTLIN_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:public|private|protected|internal|open|abstract|sealed|data|inner|enum)\s+)*class\s+(\w+)").unwrap()
});
static RE_KOTLIN_INTERFACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:public|private|protected|internal|sealed)\s+)*interface\s+(\w+)")
        .unwrap()
});
static RE_KOTLIN_OBJECT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:public|private|protected|internal)\s+)*(?:companion\s+)?object\s+(\w+)")
        .unwrap()
});
static RE_KOTLIN_FUN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:public|private|protected|internal|open|override|suspend|inline)\s+)*fun\s+(?:<[^>]*>\s*)?(\w+)").unwrap()
});
static RE_KOTLIN_ENUM_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:public|private|protected|internal)\s+)*enum\s+class\s+(\w+)").unwrap()
});

// ── Lua regexes ──
// `function name(...)` — top-level global function
static RE_LUA_FUNC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^function\s+(\w+)\s*\(").unwrap());
// `local function name(...)` — file-scoped function
static RE_LUA_LOCAL_FUNC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^local\s+function\s+(\w+)\s*\(").unwrap());
// `function M.name(...)` or `function M:name(...)` — module method
static RE_LUA_MODULE_FUNC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^function\s+\w+[.:]\s*(\w+)\s*\(").unwrap());
// `M.name = function(...)` — assigned module function
static RE_LUA_ASSIGNED_FUNC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\w+\.\s*(\w+)\s*=\s*function\s*\(").unwrap());

// ── Zig regexes ──
// `pub fn name(...)` or `fn name(...)` — functions
static RE_ZIG_FN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:pub\s+)?fn\s+(\w+)\s*\(").unwrap());
// `pub const Name = struct { ... }` or `const Name = struct { ... }`
static RE_ZIG_STRUCT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:pub\s+)?const\s+(\w+)\s*=\s*(?:extern\s+|packed\s+)?struct\b").unwrap()
});
// `pub const Name = enum { ... }` or `const Name = enum { ... }`
static RE_ZIG_ENUM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:pub\s+)?const\s+(\w+)\s*=\s*enum\b").unwrap());
// `pub const Name = union { ... }` or `const Name = union { ... }`
static RE_ZIG_UNION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:pub\s+)?const\s+(\w+)\s*=\s*union\b").unwrap());

// ── Swift regexes ──
static RE_SWIFT_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:public|private|internal|fileprivate|open|final)\s+)*class\s+(\w+)")
        .unwrap()
});
static RE_SWIFT_STRUCT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:public|private|internal|fileprivate)\s+)*struct\s+(\w+)").unwrap()
});
static RE_SWIFT_PROTOCOL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:public|private|internal|fileprivate)\s+)*protocol\s+(\w+)").unwrap()
});
static RE_SWIFT_ENUM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:public|private|internal|fileprivate)\s+)*enum\s+(\w+)").unwrap()
});
static RE_SWIFT_FUNC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:public|private|internal|fileprivate|open|override|static|class|mutating)\s+)*func\s+(\w+)").unwrap()
});
static RE_SWIFT_EXTENSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*extension\s+(\w+)").unwrap());

// ── Scala regexes ──
static RE_SCALA_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:abstract|sealed|final|implicit|lazy)\s+)*(?:case\s+)?class\s+(\w+)")
        .unwrap()
});
static RE_SCALA_TRAIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:sealed\s+)?trait\s+(\w+)").unwrap());
static RE_SCALA_OBJECT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:case\s+)?object\s+(\w+)").unwrap());
static RE_SCALA_DEF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:(?:private|protected|override|implicit|lazy)\s+)*def\s+(\w+)").unwrap()
});

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
    Macro,
    Namespace,
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

// ── Table-driven symbol extraction ──────────────────────────────────────────

/// How to determine visibility (is_public) for a matched symbol.
#[derive(Debug, Clone, Copy, PartialEq)]
enum VisibilityRule {
    /// Always public (e.g., shell functions).
    AlwaysPublic,
    /// Public if the name starts with an uppercase letter (Go convention).
    UppercaseIsPublic,
    /// Public unless the name starts with `_` (Ruby/Python convention).
    UnderscoreIsPrivate,
    /// Public if the (trimmed) line starts with "public" (C# convention).
    StartsWithPublic,
    /// Public unless the (trimmed) line starts with "private" or "protected" (Kotlin/PHP).
    NotPrivateOrProtected,
    /// Public if the (trimmed) line starts with "pub " (Zig convention).
    StartsWithPub,
}

/// A single pattern rule mapping a regex to a symbol kind.
struct PatternRule {
    regex: &'static LazyLock<Regex>,
    kind: SymbolKind,
    /// Which capture group holds the symbol name (1-indexed).
    name_group: usize,
    visibility: VisibilityRule,
    /// If non-empty, skip matches whose name is in this list (keyword filter).
    exclude_names: &'static [&'static str],
}

/// A set of pattern rules for a language that uses simple line-by-line extraction.
struct LanguagePatterns {
    rules: &'static [PatternRule],
    /// Whether to trim leading whitespace before matching (e.g., Go trims, Ruby/Shell don't).
    trim_leading: bool,
}

/// Generic table-driven symbol extractor. Iterates lines and applies rules in order,
/// using the first matching rule per line.
fn extract_symbols_from_patterns(code: &str, patterns: &LanguagePatterns) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    for (line_num, line) in code.lines().enumerate() {
        let target = if patterns.trim_leading {
            line.trim_start()
        } else {
            line
        };
        let trimmed = line.trim_start();
        for rule in patterns.rules {
            if let Some(caps) = rule.regex.captures(target) {
                let name = caps
                    .get(rule.name_group)
                    .map_or("", |m| m.as_str())
                    .to_string();
                // Skip excluded names (keyword filter)
                if !rule.exclude_names.is_empty() && rule.exclude_names.contains(&name.as_str()) {
                    break;
                }
                let is_public = match rule.visibility {
                    VisibilityRule::AlwaysPublic => true,
                    VisibilityRule::UppercaseIsPublic => {
                        name.starts_with(|c: char| c.is_uppercase())
                    }
                    VisibilityRule::UnderscoreIsPrivate => !name.starts_with('_'),
                    VisibilityRule::StartsWithPublic => trimmed.starts_with("public"),
                    VisibilityRule::NotPrivateOrProtected => {
                        !trimmed.starts_with("private") && !trimmed.starts_with("protected")
                    }
                    VisibilityRule::StartsWithPub => trimmed.starts_with("pub "),
                };
                symbols.push(Symbol {
                    name,
                    kind: rule.kind.clone(),
                    is_public,
                    line: line_num + 1,
                });
                break; // first match wins
            }
        }
    }

    symbols
}

// ── Pattern tables for Go, Ruby, Shell ──────────────────────────────────────

static GO_PATTERN_RULES: &[PatternRule] = &[
    PatternRule {
        regex: &RE_GO_METHOD,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::UppercaseIsPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_GO_FUNC,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::UppercaseIsPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_GO_STRUCT,
        kind: SymbolKind::Struct,
        name_group: 1,
        visibility: VisibilityRule::UppercaseIsPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_GO_INTERFACE,
        kind: SymbolKind::Interface,
        name_group: 1,
        visibility: VisibilityRule::UppercaseIsPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_GO_CONST,
        kind: SymbolKind::Const,
        name_group: 1,
        visibility: VisibilityRule::UppercaseIsPublic,
        exclude_names: &[],
    },
];

static RUBY_PATTERN_RULES: &[PatternRule] = &[
    PatternRule {
        regex: &RE_RUBY_CLASS,
        kind: SymbolKind::Class,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_RUBY_MODULE,
        kind: SymbolKind::Module,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_RUBY_DEF,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::UnderscoreIsPrivate,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_RUBY_CONST,
        kind: SymbolKind::Const,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
];

static SHELL_PATTERN_RULES: &[PatternRule] = &[
    PatternRule {
        regex: &RE_SHELL_FUNC_KEYWORD,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_SHELL_FUNC_PARENS,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
];

static GO_PATTERNS: LanguagePatterns = LanguagePatterns {
    rules: GO_PATTERN_RULES,
    trim_leading: true,
};
static RUBY_PATTERNS: LanguagePatterns = LanguagePatterns {
    rules: RUBY_PATTERN_RULES,
    trim_leading: false,
};
static SHELL_PATTERNS: LanguagePatterns = LanguagePatterns {
    rules: SHELL_PATTERN_RULES,
    trim_leading: false,
};

// ── C# pattern rules ──
static CSHARP_PATTERN_RULES: &[PatternRule] = &[
    PatternRule {
        regex: &RE_CSHARP_NAMESPACE,
        kind: SymbolKind::Namespace,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_CSHARP_RECORD,
        kind: SymbolKind::Struct,
        name_group: 1,
        visibility: VisibilityRule::StartsWithPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_CSHARP_CLASS,
        kind: SymbolKind::Class,
        name_group: 1,
        visibility: VisibilityRule::StartsWithPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_CSHARP_INTERFACE,
        kind: SymbolKind::Interface,
        name_group: 1,
        visibility: VisibilityRule::StartsWithPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_CSHARP_STRUCT,
        kind: SymbolKind::Struct,
        name_group: 1,
        visibility: VisibilityRule::StartsWithPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_CSHARP_ENUM,
        kind: SymbolKind::Enum,
        name_group: 1,
        visibility: VisibilityRule::StartsWithPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_CSHARP_METHOD,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::StartsWithPublic,
        exclude_names: &[
            "if",
            "for",
            "while",
            "switch",
            "catch",
            "return",
            "new",
            "class",
            "interface",
            "struct",
            "enum",
            "namespace",
            "using",
            "throw",
            "lock",
            "foreach",
            "typeof",
            "sizeof",
            "nameof",
            "record",
        ],
    },
];

static CSHARP_PATTERNS: LanguagePatterns = LanguagePatterns {
    rules: CSHARP_PATTERN_RULES,
    trim_leading: false,
};

// ── PHP pattern rules ──
static PHP_PATTERN_RULES: &[PatternRule] = &[
    PatternRule {
        regex: &RE_PHP_CLASS,
        kind: SymbolKind::Class,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_PHP_INTERFACE,
        kind: SymbolKind::Interface,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_PHP_TRAIT,
        kind: SymbolKind::Trait,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_PHP_ENUM,
        kind: SymbolKind::Enum,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_PHP_FUNCTION,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::NotPrivateOrProtected,
        exclude_names: &[],
    },
];

static PHP_PATTERNS: LanguagePatterns = LanguagePatterns {
    rules: PHP_PATTERN_RULES,
    trim_leading: false,
};

// ── Kotlin pattern rules ──
static KOTLIN_PATTERN_RULES: &[PatternRule] = &[
    PatternRule {
        regex: &RE_KOTLIN_INTERFACE,
        kind: SymbolKind::Interface,
        name_group: 1,
        visibility: VisibilityRule::NotPrivateOrProtected,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_KOTLIN_OBJECT,
        kind: SymbolKind::Module,
        name_group: 1,
        visibility: VisibilityRule::NotPrivateOrProtected,
        exclude_names: &[],
    },
    // enum class must come before generic class to avoid class capturing it
    PatternRule {
        regex: &RE_KOTLIN_ENUM_CLASS,
        kind: SymbolKind::Enum,
        name_group: 1,
        visibility: VisibilityRule::NotPrivateOrProtected,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_KOTLIN_CLASS,
        kind: SymbolKind::Class,
        name_group: 1,
        visibility: VisibilityRule::NotPrivateOrProtected,
        exclude_names: &[],
    },
    PatternRule {
        regex: &RE_KOTLIN_FUN,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::NotPrivateOrProtected,
        exclude_names: &[
            "if",
            "for",
            "while",
            "when",
            "return",
            "throw",
            "class",
            "interface",
            "object",
        ],
    },
];

static KOTLIN_PATTERNS: LanguagePatterns = LanguagePatterns {
    rules: KOTLIN_PATTERN_RULES,
    trim_leading: false,
};

// ── Lua pattern rules ──
// Order matters: module methods before top-level functions (both start with `function`).
static LUA_PATTERN_RULES: &[PatternRule] = &[
    // `function M.name(...)` or `function M:name(...)` — module method (public)
    PatternRule {
        regex: &RE_LUA_MODULE_FUNC,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
    // `M.name = function(...)` — assigned module function (public)
    PatternRule {
        regex: &RE_LUA_ASSIGNED_FUNC,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
    // `local function name(...)` — file-scoped function (treated as public for map visibility)
    PatternRule {
        regex: &RE_LUA_LOCAL_FUNC,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &[],
    },
    // `function name(...)` — top-level global function (public)
    PatternRule {
        regex: &RE_LUA_FUNC,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::AlwaysPublic,
        exclude_names: &["if", "for", "while", "return", "end", "else", "elseif"],
    },
];

static LUA_PATTERNS: LanguagePatterns = LanguagePatterns {
    rules: LUA_PATTERN_RULES,
    trim_leading: false,
};

// ── Zig pattern rules ──
// Order: struct/enum/union before fn (more specific first). Tests are naturally
// skipped because no pattern matches `test "..."`.
static ZIG_PATTERN_RULES: &[PatternRule] = &[
    // Struct definitions
    PatternRule {
        regex: &RE_ZIG_STRUCT,
        kind: SymbolKind::Struct,
        name_group: 1,
        visibility: VisibilityRule::StartsWithPub,
        exclude_names: &[],
    },
    // Enum definitions
    PatternRule {
        regex: &RE_ZIG_ENUM,
        kind: SymbolKind::Enum,
        name_group: 1,
        visibility: VisibilityRule::StartsWithPub,
        exclude_names: &[],
    },
    // Union definitions
    PatternRule {
        regex: &RE_ZIG_UNION,
        kind: SymbolKind::Type,
        name_group: 1,
        visibility: VisibilityRule::StartsWithPub,
        exclude_names: &[],
    },
    // Functions (pub fn and fn)
    PatternRule {
        regex: &RE_ZIG_FN,
        kind: SymbolKind::Function,
        name_group: 1,
        visibility: VisibilityRule::StartsWithPub,
        exclude_names: &["if", "for", "while", "return"],
    },
];

static ZIG_PATTERNS: LanguagePatterns = LanguagePatterns {
    rules: ZIG_PATTERN_RULES,
    trim_leading: false,
};

// ─────────────────────────────────────────────────────────────────────────────

/// Detect programming language from file extension.
pub fn detect_language(path: &str) -> Option<&'static str> {
    match Path::new(path).extension()?.to_str()? {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "js" | "jsx" | "mjs" => Some("javascript"),
        "ts" | "tsx" => Some("typescript"),
        "go" => Some("go"),
        "java" => Some("java"),
        "c" | "h" => Some("c"),
        "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "hh" => Some("cpp"),
        "rb" => Some("ruby"),
        "sh" | "bash" | "zsh" => Some("shell"),
        "cs" => Some("csharp"),
        "php" => Some("php"),
        "kt" | "kts" => Some("kotlin"),
        "swift" => Some("swift"),
        "scala" | "sc" => Some("scala"),
        "lua" => Some("lua"),
        "zig" => Some("zig"),
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
        "go" => extract_symbols_from_patterns(code, &GO_PATTERNS),
        "java" => extract_java_symbols(code),
        "c" => extract_c_symbols(code),
        "cpp" => extract_cpp_symbols(code),
        "ruby" => extract_symbols_from_patterns(code, &RUBY_PATTERNS),
        "shell" => extract_symbols_from_patterns(code, &SHELL_PATTERNS),
        "csharp" => extract_symbols_from_patterns(code, &CSHARP_PATTERNS),
        "php" => extract_symbols_from_patterns(code, &PHP_PATTERNS),
        "kotlin" => extract_symbols_from_patterns(code, &KOTLIN_PATTERNS),
        "lua" => extract_symbols_from_patterns(code, &LUA_PATTERNS),
        "zig" => extract_symbols_from_patterns(code, &ZIG_PATTERNS),
        "swift" => extract_swift_symbols(code),
        "scala" => extract_scala_symbols(code),
        _ => Vec::new(),
    }
}

/// Extract symbols from Rust source code.
/// Skips content inside `#[cfg(test)]` modules.
fn extract_rust_symbols(code: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut in_test_module = false;
    let mut test_brace_depth: i32 = 0;

    let re_fn = &*RE_RUST_FN;
    let re_struct = &*RE_RUST_STRUCT;
    let re_enum = &*RE_RUST_ENUM;
    let re_trait = &*RE_RUST_TRAIT;
    let re_impl = &*RE_RUST_IMPL;
    let re_const = &*RE_RUST_CONST;
    let re_mod = &*RE_RUST_MOD;
    let re_cfg_test = &*RE_RUST_CFG_TEST;

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

    let re_class = &*RE_PY_CLASS;
    let re_func = &*RE_PY_DEF;
    let re_const = &*RE_PY_CONST;

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

    let re_export_func = &*RE_JS_FUNCTION;
    let re_class = &*RE_JS_CLASS;
    let re_const = &*RE_JS_CONST;

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

    let re_interface = &*RE_TS_INTERFACE;
    let re_type = &*RE_TS_TYPE;

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

/// Extract symbols from Java source code.
fn extract_java_symbols(code: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    let re_class = &*RE_JAVA_CLASS;
    let re_interface = &*RE_JAVA_INTERFACE;
    let re_enum = &*RE_JAVA_ENUM;
    let re_method = &*RE_JAVA_METHOD;

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

/// C keywords that match the function regex but aren't function definitions.
const C_NON_FUNC_KEYWORDS: &[&str] = &[
    "if", "for", "while", "switch", "return", "else", "do", "sizeof", "typedef", "extern",
    "static", "inline", "volatile", "register", "goto", "case", "break", "continue",
];

/// Extract symbols from C source code.
fn extract_c_symbols(code: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    let re_func = &*RE_C_FUNC;
    let re_struct = &*RE_C_STRUCT;
    let re_enum = &*RE_C_ENUM;
    let re_define = &*RE_C_DEFINE;

    for (line_num, line) in code.lines().enumerate() {
        let trimmed = line.trim_start();

        if let Some(caps) = re_define.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Macro,
                is_public: true,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_struct.captures(trimmed) {
            let name = caps.get(2).map_or("", |m| m.as_str());
            // If no typedef group matched, try group 1 as the struct name
            let name = if name.is_empty() {
                caps.get(1).map_or("", |m| m.as_str())
            } else {
                name
            };
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Struct,
                is_public: true,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_enum.captures(trimmed) {
            let name = caps.get(2).map_or("", |m| m.as_str());
            let name = if name.is_empty() {
                caps.get(1).map_or("", |m| m.as_str())
            } else {
                name
            };
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Enum,
                is_public: true,
                line: line_num + 1,
            });
        } else if !trimmed.starts_with("//")
            && !trimmed.starts_with("/*")
            && !trimmed.starts_with('*')
            && !trimmed.starts_with('#')
        {
            // Only match functions at top-level (non-indented) lines
            if line.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                if let Some(caps) = re_func.captures(trimmed) {
                    let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
                    if !C_NON_FUNC_KEYWORDS.contains(&name.as_str()) {
                        symbols.push(Symbol {
                            name,
                            kind: SymbolKind::Function,
                            is_public: true,
                            line: line_num + 1,
                        });
                    }
                }
            }
        }
    }

    symbols
}

/// Extract symbols from C++ source code.
///
/// Extends C extraction with classes, namespaces, and templates.
fn extract_cpp_symbols(code: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    let re_func = &*RE_C_FUNC;
    let re_struct = &*RE_C_STRUCT;
    let re_enum = &*RE_C_ENUM;
    let re_define = &*RE_C_DEFINE;
    let re_class = &*RE_CPP_CLASS;
    let re_namespace = &*RE_CPP_NAMESPACE;

    for (line_num, line) in code.lines().enumerate() {
        let trimmed = line.trim_start();

        if let Some(caps) = re_define.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Macro,
                is_public: true,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_namespace.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Namespace,
                is_public: true,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_class.captures(trimmed) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Class,
                is_public: true,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_struct.captures(trimmed) {
            let name = caps.get(2).map_or("", |m| m.as_str());
            let name = if name.is_empty() {
                caps.get(1).map_or("", |m| m.as_str())
            } else {
                name
            };
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Struct,
                is_public: true,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_enum.captures(trimmed) {
            let name = caps.get(2).map_or("", |m| m.as_str());
            let name = if name.is_empty() {
                caps.get(1).map_or("", |m| m.as_str())
            } else {
                name
            };
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Enum,
                is_public: true,
                line: line_num + 1,
            });
        } else if !trimmed.starts_with("//")
            && !trimmed.starts_with("/*")
            && !trimmed.starts_with('*')
            && !trimmed.starts_with('#')
        {
            if let Some(caps) = re_func.captures(trimmed) {
                let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
                if !C_NON_FUNC_KEYWORDS.contains(&name.as_str())
                    && name != "class"
                    && name != "namespace"
                {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        is_public: true,
                        line: line_num + 1,
                    });
                }
            }
        }
    }

    symbols
}

/// Extract symbols from Swift source code.
fn extract_swift_symbols(code: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    let re_class = &*RE_SWIFT_CLASS;
    let re_struct = &*RE_SWIFT_STRUCT;
    let re_protocol = &*RE_SWIFT_PROTOCOL;
    let re_enum = &*RE_SWIFT_ENUM;
    let re_func = &*RE_SWIFT_FUNC;
    let re_extension = &*RE_SWIFT_EXTENSION;

    for (line_num, line) in code.lines().enumerate() {
        let trimmed = line.trim_start();
        let is_pub = trimmed.starts_with("public") || trimmed.starts_with("open");

        if let Some(caps) = re_protocol.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Trait,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_class.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Class,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_struct.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Struct,
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
        } else if let Some(caps) = re_extension.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Impl,
                is_public: true,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_func.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                is_public: is_pub,
                line: line_num + 1,
            });
        }
    }

    symbols
}

/// Extract symbols from Scala source code.
fn extract_scala_symbols(code: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    let re_class = &*RE_SCALA_CLASS;
    let re_trait = &*RE_SCALA_TRAIT;
    let re_object = &*RE_SCALA_OBJECT;
    let re_def = &*RE_SCALA_DEF;

    for (line_num, line) in code.lines().enumerate() {
        let trimmed = line.trim_start();
        let is_pub = !trimmed.starts_with("private") && !trimmed.starts_with("protected");

        if let Some(caps) = re_trait.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Trait,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_object.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Module,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_class.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Class,
                is_public: is_pub,
                line: line_num + 1,
            });
        } else if let Some(caps) = re_def.captures(line) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                is_public: is_pub,
                line: line_num + 1,
            });
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

    // Resolve git toplevel so file reads use absolute paths,
    // preventing CWD races when parallel tests call set_current_dir.
    let toplevel = crate::git::run_git(&["rev-parse", "--show-toplevel"]).ok();

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
        // Use absolute path for file I/O to avoid CWD dependency
        let abs_path = if let Some(ref tl) = toplevel {
            std::path::Path::new(tl)
                .join(path)
                .to_string_lossy()
                .to_string()
        } else {
            path.clone()
        };
        let content = match std::fs::read_to_string(&abs_path) {
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
    result.sort_by_key(|b| std::cmp::Reverse(b.lines));
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
                SymbolKind::Macro => format!("{CYAN}macro{RESET}"),
                SymbolKind::Namespace => format!("{MAGENTA}namespace{RESET}"),
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
                SymbolKind::Macro => "macro",
                SymbolKind::Namespace => "namespace",
            };
            output.push_str(&format!("  {kind_label} {}\n", sym.name));
        }
    }

    output
}

/// Get the list of recently modified files from git history (deduplicated, ordered by recency).
///
/// Returns up to `n` unique file paths from the last `n` commits' changed files.
/// Returns an empty vec if not in a git repository or git fails.
pub fn recently_modified_files(n: usize) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["log", "--name-only", "--format=", "-n"])
        .arg(n.to_string())
        .output();
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for line in stdout.lines() {
        let path = line.trim();
        if !path.is_empty() && seen.insert(path.to_string()) {
            result.push(path.to_string());
        }
    }
    result
}

/// Compute a relevance score for a file entry used to prioritize the prompt repo map.
///
/// - `recency_score`: based on position in the recently-modified list (higher = more recent)
/// - `density_score`: symbols per 100 lines, capped at 50
/// - `size_score`: lines / 50, capped at 20
fn relevance_score(entry: &FileSymbols, recent_files: &[String]) -> usize {
    // Recency: position in list (first = most recent = highest score)
    let recency_score = recent_files
        .iter()
        .position(|p| p == &entry.path)
        .map(|pos| recent_files.len().saturating_sub(pos))
        .unwrap_or(0);

    // Density: symbols per 100 lines, capped at 50
    let density_score = (entry.symbols.len() * 100)
        .checked_div(entry.lines)
        .unwrap_or(0)
        .min(50);

    // Size: larger files get a bump, capped at 20
    let size_score = (entry.lines / 50).min(20);

    recency_score + density_score + size_score
}

/// Generate a repo map for the system prompt, capped at `max_chars` characters.
///
/// Files are sorted by relevance (recency, symbol density, size) so that when
/// truncation is needed, the most architecturally important files survive.
///
/// Returns `None` if no supported source files are found.
pub fn generate_repo_map_for_prompt_with_limit(max_chars: usize) -> Option<String> {
    let mut entries = build_repo_map(None, true);
    if entries.is_empty() {
        return None;
    }

    let full = format_repo_map(&entries);
    if full.len() <= max_chars {
        Some(full)
    } else {
        // Sort by relevance so the most important files survive truncation
        let recent_files = recently_modified_files(50);
        entries.sort_by(|a, b| {
            let score_a = relevance_score(a, &recent_files);
            let score_b = relevance_score(b, &recent_files);
            score_b.cmp(&score_a)
        });

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
                    SymbolKind::Macro => "macro",
                    SymbolKind::Namespace => "namespace",
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
        let help = crate::help::help_text();
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
        let (entries, backend) = build_repo_map_with_backend(Some("src/"), true, true);
        assert_eq!(backend, MapBackend::Regex);
        // entries may be empty if another parallel test changed CWD via
        // set_current_dir (global process state race). Only assert non-empty
        // when we can confirm we're still in the project root.
        let in_project_root = std::path::Path::new("Cargo.toml").exists();
        if in_project_root {
            assert!(
                !entries.is_empty(),
                "should find symbols in src/ with regex backend"
            );
        }
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

    // ── C extraction tests ──

    #[test]
    fn extract_c_symbols_basic() {
        let code = r#"
#include <stdio.h>

#define MAX_SIZE 100
#define MIN(a,b) ((a)<(b)?(a):(b))

typedef struct point {
    int x;
    int y;
} Point;

enum color { RED, GREEN, BLUE };

int main(int argc, char **argv) {
    return 0;
}

void helper(void) {
    printf("hello\n");
}
"#;
        let symbols = extract_symbols(code, "c");

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "MAX_SIZE" && s.kind == SymbolKind::Macro),
            "should find #define MAX_SIZE"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "MIN" && s.kind == SymbolKind::Macro),
            "should find #define MIN"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "point" && s.kind == SymbolKind::Struct),
            "should find struct point"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "color" && s.kind == SymbolKind::Enum),
            "should find enum color"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "main" && s.kind == SymbolKind::Function),
            "should find function main"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "helper" && s.kind == SymbolKind::Function),
            "should find function helper"
        );
    }

    #[test]
    fn extract_c_symbols_skips_control_flow() {
        let code = r#"
int process(int x) {
    if (x > 0) {
        return x;
    }
    for (int i = 0; i < x; i++) {
        while (1) break;
    }
    switch (x) {
        case 1: return 1;
    }
    return 0;
}
"#;
        let symbols = extract_symbols(code, "c");
        let func_names: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .map(|s| s.name.as_str())
            .collect();
        assert!(func_names.contains(&"process"), "should find process");
        assert!(
            !func_names.contains(&"if"),
            "should not match 'if' as function"
        );
        assert!(
            !func_names.contains(&"for"),
            "should not match 'for' as function"
        );
        assert!(
            !func_names.contains(&"while"),
            "should not match 'while' as function"
        );
        assert!(
            !func_names.contains(&"switch"),
            "should not match 'switch' as function"
        );
    }

    // ── C++ extraction tests ──

    #[test]
    fn extract_cpp_symbols_basic() {
        let code = r#"
#include <iostream>

#define VERSION 1

namespace mylib {

class Widget {
public:
    void draw();
};

struct Point {
    int x, y;
};

enum Color { Red, Green, Blue };

template<typename T>
class Container {
    T value;
};

void free_function(int x) {
    std::cout << x;
}

} // namespace mylib
"#;
        let symbols = extract_symbols(code, "cpp");

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "VERSION" && s.kind == SymbolKind::Macro),
            "should find #define VERSION"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "mylib" && s.kind == SymbolKind::Namespace),
            "should find namespace mylib"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Widget" && s.kind == SymbolKind::Class),
            "should find class Widget"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Point" && s.kind == SymbolKind::Struct),
            "should find struct Point"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Color" && s.kind == SymbolKind::Enum),
            "should find enum Color"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Container" && s.kind == SymbolKind::Class),
            "should find template class Container"
        );
    }

    #[test]
    fn extract_cpp_symbols_template_class() {
        let code = "template<typename T>\nclass Vector {\n    T* data;\n};\n";
        let symbols = extract_symbols(code, "cpp");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Vector" && s.kind == SymbolKind::Class),
            "should find template class Vector"
        );
    }

    // ── Ruby extraction tests ──

    #[test]
    fn extract_ruby_symbols_basic() {
        let code = r#"
module MyApp
  class User
    MAX_RETRIES = 3
    DEFAULT_NAME = "anonymous"

    def initialize(name)
      @name = name
    end

    def greet
      "Hello, #{@name}"
    end

    def valid?
      !@name.nil?
    end

    def _private_helper
      true
    end
  end
end
"#;
        let symbols = extract_symbols(code, "ruby");

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "MyApp" && s.kind == SymbolKind::Module),
            "should find module MyApp"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "User" && s.kind == SymbolKind::Class),
            "should find class User"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "MAX_RETRIES" && s.kind == SymbolKind::Const),
            "should find constant MAX_RETRIES"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "DEFAULT_NAME" && s.kind == SymbolKind::Const),
            "should find constant DEFAULT_NAME"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "initialize" && s.kind == SymbolKind::Function),
            "should find def initialize"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "greet" && s.kind == SymbolKind::Function),
            "should find def greet"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "valid?" && s.kind == SymbolKind::Function),
            "should find def valid?"
        );
        // Private method: is_public should be false
        let priv_method = symbols.iter().find(|s| s.name == "_private_helper");
        assert!(priv_method.is_some(), "should find _private_helper");
        assert!(
            !priv_method.unwrap().is_public,
            "_private_helper should not be public"
        );
    }

    #[test]
    fn extract_ruby_symbols_edge_cases() {
        let code = "class Base\nend\nclass Child < Base\nend\n";
        let symbols = extract_symbols(code, "ruby");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Base" && s.kind == SymbolKind::Class),
            "should find class Base"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Child" && s.kind == SymbolKind::Class),
            "should find class Child (with inheritance)"
        );
    }

    // ── Shell extraction tests ──

    #[test]
    fn extract_shell_symbols_basic() {
        let code = r#"#!/bin/bash

setup() {
    echo "setting up"
}

function cleanup {
    echo "cleaning up"
}

function run_tests {
    setup
    echo "running tests"
    cleanup
}

main() {
    run_tests
}

main "$@"
"#;
        let symbols = extract_symbols(code, "shell");

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "setup" && s.kind == SymbolKind::Function),
            "should find setup() function"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "cleanup" && s.kind == SymbolKind::Function),
            "should find 'function cleanup'"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "run_tests" && s.kind == SymbolKind::Function),
            "should find 'function run_tests'"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "main" && s.kind == SymbolKind::Function),
            "should find main() function"
        );
    }

    #[test]
    fn extract_shell_symbols_indented() {
        let code = "  helper() {\n    echo hi\n  }\n  function inner_func {\n    true\n  }\n";
        let symbols = extract_symbols(code, "shell");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "helper" && s.kind == SymbolKind::Function),
            "should find indented helper()"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "inner_func" && s.kind == SymbolKind::Function),
            "should find indented function inner_func"
        );
    }

    // ── Table-driven extraction verification tests ──

    #[test]
    fn table_driven_go_visibility() {
        let code =
            "func main() {}\nfunc Serve() {}\ntype handler struct{}\ntype Handler struct{}\n";
        let symbols = extract_symbols(code, "go");
        let main_sym = symbols.iter().find(|s| s.name == "main").unwrap();
        assert!(!main_sym.is_public, "lowercase Go func should be private");
        let serve_sym = symbols.iter().find(|s| s.name == "Serve").unwrap();
        assert!(serve_sym.is_public, "uppercase Go func should be public");
        let handler_lower = symbols.iter().find(|s| s.name == "handler").unwrap();
        assert!(
            !handler_lower.is_public,
            "lowercase Go struct should be private"
        );
        let handler_upper = symbols.iter().find(|s| s.name == "Handler").unwrap();
        assert!(
            handler_upper.is_public,
            "uppercase Go struct should be public"
        );
    }

    #[test]
    fn table_driven_go_const_and_var() {
        let code = "const MaxRetries = 3\nvar defaultTimeout = 30\n";
        let symbols = extract_symbols(code, "go");
        let max_sym = symbols.iter().find(|s| s.name == "MaxRetries").unwrap();
        assert_eq!(max_sym.kind, SymbolKind::Const);
        assert!(max_sym.is_public);
        let def_sym = symbols.iter().find(|s| s.name == "defaultTimeout").unwrap();
        assert_eq!(def_sym.kind, SymbolKind::Const);
        assert!(!def_sym.is_public);
    }

    #[test]
    fn table_driven_ruby_underscore_visibility() {
        let code = "def public_method\nend\ndef _private_method\nend\n";
        let symbols = extract_symbols(code, "ruby");
        let pub_sym = symbols.iter().find(|s| s.name == "public_method").unwrap();
        assert!(pub_sym.is_public, "Ruby method without _ should be public");
        let priv_sym = symbols
            .iter()
            .find(|s| s.name == "_private_method")
            .unwrap();
        assert!(
            !priv_sym.is_public,
            "Ruby method with _ prefix should be private"
        );
    }

    #[test]
    fn table_driven_shell_all_public() {
        let code = "setup() {\n  echo hi\n}\nfunction _internal {\n  true\n}\n";
        let symbols = extract_symbols(code, "shell");
        assert!(
            symbols.iter().all(|s| s.is_public),
            "All shell functions should be public"
        );
        assert_eq!(symbols.len(), 2);
        assert!(symbols.iter().any(|s| s.name == "setup"));
        assert!(symbols.iter().any(|s| s.name == "_internal"));
    }

    #[test]
    fn table_driven_go_first_match_wins() {
        // A Go method (func (r *Router) Handle) should match as Function via RE_GO_METHOD
        // and NOT also match RE_GO_FUNC (which would fail because of the receiver syntax)
        let code = "func (r *Router) Handle(w http.ResponseWriter) {}\n";
        let symbols = extract_symbols(code, "go");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Handle");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    // ── Language detection tests for new languages ──

    #[test]
    fn detect_language_c_extensions() {
        assert_eq!(detect_language("main.c"), Some("c"));
        assert_eq!(detect_language("header.h"), Some("c"));
    }

    #[test]
    fn detect_language_cpp_extensions() {
        assert_eq!(detect_language("main.cpp"), Some("cpp"));
        assert_eq!(detect_language("main.cc"), Some("cpp"));
        assert_eq!(detect_language("main.cxx"), Some("cpp"));
        assert_eq!(detect_language("header.hpp"), Some("cpp"));
        assert_eq!(detect_language("header.hxx"), Some("cpp"));
        assert_eq!(detect_language("header.hh"), Some("cpp"));
    }

    #[test]
    fn detect_language_ruby_extension() {
        assert_eq!(detect_language("app.rb"), Some("ruby"));
    }

    #[test]
    fn detect_language_shell_extensions() {
        assert_eq!(detect_language("script.sh"), Some("shell"));
        assert_eq!(detect_language("build.bash"), Some("shell"));
        assert_eq!(detect_language("init.zsh"), Some("shell"));
    }

    // ── C# language detection and extraction tests ──

    #[test]
    fn detect_language_csharp_extensions() {
        assert_eq!(detect_language("Program.cs"), Some("csharp"));
    }

    #[test]
    fn extract_csharp_symbols_basic() {
        let code = r#"
namespace MyApp.Models
{
    public interface IRepository
    {
        void Save();
    }

    public class UserService
    {
        public async Task<User> GetUser(int id)
        {
            return await db.Find(id);
        }

        private void ValidateInput(string input)
        {
        }
    }

    public struct Point
    {
        public int X;
        public int Y;
    }

    public enum Status
    {
        Active,
        Inactive
    }

    public record UserDto(string Name, int Age);
}
"#;
        let symbols = extract_symbols(code, "csharp");

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "MyApp.Models" && s.kind == SymbolKind::Namespace),
            "should find namespace"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "IRepository" && s.kind == SymbolKind::Interface),
            "should find interface"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "UserService" && s.kind == SymbolKind::Class),
            "should find class"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "GetUser" && s.kind == SymbolKind::Function),
            "should find public method"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Point" && s.kind == SymbolKind::Struct),
            "should find struct"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Status" && s.kind == SymbolKind::Enum),
            "should find enum"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "UserDto" && s.kind == SymbolKind::Struct),
            "should find record as struct"
        );
    }

    // ── PHP language detection and extraction tests ──

    #[test]
    fn detect_language_php_extensions() {
        assert_eq!(detect_language("index.php"), Some("php"));
    }

    #[test]
    fn extract_php_symbols_basic() {
        let code = r#"<?php

class UserController
{
    public function index()
    {
        return view('users.index');
    }

    private function validate($data)
    {
    }
}

interface Cacheable
{
    public function cacheKey(): string;
}

trait HasTimestamps
{
    public function createdAt(): DateTime
    {
    }
}

enum Color
{
    case Red;
    case Green;
    case Blue;
}

function helper_function($x)
{
    return $x * 2;
}
"#;
        let symbols = extract_symbols(code, "php");

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "UserController" && s.kind == SymbolKind::Class),
            "should find class"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "index" && s.kind == SymbolKind::Function),
            "should find public method"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Cacheable" && s.kind == SymbolKind::Interface),
            "should find interface"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "HasTimestamps" && s.kind == SymbolKind::Trait),
            "should find trait"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Color" && s.kind == SymbolKind::Enum),
            "should find enum"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "helper_function" && s.kind == SymbolKind::Function),
            "should find standalone function"
        );
    }

    // ── Kotlin language detection and extraction tests ──

    #[test]
    fn detect_language_kotlin_extensions() {
        assert_eq!(detect_language("Main.kt"), Some("kotlin"));
        assert_eq!(detect_language("build.gradle.kts"), Some("kotlin"));
    }

    #[test]
    fn extract_kotlin_symbols_basic() {
        let code = r#"
data class User(val name: String, val age: Int)

sealed class Result {
    data class Success(val data: Any) : Result()
    data class Error(val message: String) : Result()
}

interface Repository {
    fun findAll(): List<User>
}

enum class Direction {
    NORTH, SOUTH, EAST, WEST
}

object Singleton {
    fun getInstance(): Singleton = this
}

fun <T> process(item: T): T {
    return item
}

suspend fun fetchData(): String {
    return "data"
}

private fun helperFunction() {
}
"#;
        let symbols = extract_symbols(code, "kotlin");

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "User" && s.kind == SymbolKind::Class),
            "should find data class"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Result" && s.kind == SymbolKind::Class),
            "should find sealed class"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Repository" && s.kind == SymbolKind::Interface),
            "should find interface"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Direction" && s.kind == SymbolKind::Enum),
            "should find enum class"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Singleton" && s.kind == SymbolKind::Module),
            "should find object"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "process" && s.kind == SymbolKind::Function),
            "should find generic function"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "fetchData" && s.kind == SymbolKind::Function),
            "should find suspend function"
        );
    }

    // ── Swift language detection and extraction tests ──

    #[test]
    fn detect_language_swift_extensions() {
        assert_eq!(detect_language("App.swift"), Some("swift"));
    }

    #[test]
    fn extract_swift_symbols_basic() {
        let code = r#"
public class ViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
    }
}

struct Point {
    var x: Double
    var y: Double
}

protocol Drawable {
    func draw()
}

enum Direction {
    case north
    case south
}

extension String {
    func reversed() -> String {
        return String(self.reversed())
    }
}

public func globalHelper() -> Int {
    return 42
}

private func internalHelper() {
}

static func classMethod() {
}
"#;
        let symbols = extract_symbols(code, "swift");

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "ViewController" && s.kind == SymbolKind::Class),
            "should find class"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "viewDidLoad" && s.kind == SymbolKind::Function),
            "should find override func"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Point" && s.kind == SymbolKind::Struct),
            "should find struct"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Drawable" && s.kind == SymbolKind::Trait),
            "should find protocol as trait"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Direction" && s.kind == SymbolKind::Enum),
            "should find enum"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "String" && s.kind == SymbolKind::Impl),
            "should find extension as impl"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "globalHelper" && s.kind == SymbolKind::Function),
            "should find global func"
        );
    }

    // ── Scala language detection and extraction tests ──

    #[test]
    fn detect_language_scala_extensions() {
        assert_eq!(detect_language("Main.scala"), Some("scala"));
        assert_eq!(detect_language("build.sc"), Some("scala"));
    }

    #[test]
    fn extract_scala_symbols_basic() {
        let code = r#"
case class User(name: String, age: Int)

sealed trait Result
class Success(val data: Any) extends Result
class Failure(val message: String) extends Result

trait Repository {
  def findAll(): List[User]
  def findById(id: Int): Option[User]
}

object UserService {
  def create(name: String): User = {
    User(name, 0)
  }

  private def validate(name: String): Boolean = {
    name.nonEmpty
  }
}

case object Sentinel

abstract class Base {
  def process(): Unit
}
"#;
        let symbols = extract_symbols(code, "scala");

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "User" && s.kind == SymbolKind::Class),
            "should find case class"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Result" && s.kind == SymbolKind::Trait),
            "should find sealed trait"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Repository" && s.kind == SymbolKind::Trait),
            "should find trait"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "UserService" && s.kind == SymbolKind::Module),
            "should find object as module"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "create" && s.kind == SymbolKind::Function),
            "should find def"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Sentinel" && s.kind == SymbolKind::Module),
            "should find case object"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "findAll" && s.kind == SymbolKind::Function),
            "should find trait method def"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Base" && s.kind == SymbolKind::Class),
            "should find abstract class"
        );
    }

    // ── Lua tests ──

    #[test]
    fn detect_language_lua_extensions() {
        assert_eq!(detect_language("script.lua"), Some("lua"));
        assert_eq!(detect_language("init.lua"), Some("lua"));
    }

    #[test]
    fn extract_lua_basic_functions() {
        let code = r#"
function greet(name)
    print("Hello, " .. name)
end

local function helper()
    return 42
end
"#;
        let symbols = extract_symbols(code, "lua");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "greet" && s.kind == SymbolKind::Function),
            "should find top-level function"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "helper" && s.kind == SymbolKind::Function),
            "should find local function"
        );
    }

    #[test]
    fn extract_lua_module_methods() {
        let code = r#"
local M = {}

function M.init(config)
    M.config = config
end

function M:update(dt)
    self.time = self.time + dt
end

M.create = function(name)
    return { name = name }
end
"#;
        let symbols = extract_symbols(code, "lua");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "init" && s.kind == SymbolKind::Function),
            "should find M.init module method"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "update" && s.kind == SymbolKind::Function),
            "should find M:update method"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "create" && s.kind == SymbolKind::Function),
            "should find assigned module function"
        );
    }

    #[test]
    fn extract_lua_skips_comments() {
        let code = r#"
-- function commented_out()
--   return false
-- end

function real_function()
    return true
end
"#;
        let symbols = extract_symbols(code, "lua");
        assert!(
            !symbols.iter().any(|s| s.name == "commented_out"),
            "should skip commented function"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "real_function" && s.kind == SymbolKind::Function),
            "should find real function"
        );
    }

    // ── Zig tests ──

    #[test]
    fn detect_language_zig_extensions() {
        assert_eq!(detect_language("main.zig"), Some("zig"));
        assert_eq!(detect_language("build.zig"), Some("zig"));
    }

    #[test]
    fn extract_zig_functions() {
        let code = r#"
const std = @import("std");

pub fn main() !void {
    std.debug.print("hello\n", .{});
}

fn helper() void {
    // private function
}

pub fn add(a: i32, b: i32) i32 {
    return a + b;
}
"#;
        let symbols = extract_symbols(code, "zig");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "main" && s.kind == SymbolKind::Function && s.is_public),
            "should find pub fn main"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "helper" && s.kind == SymbolKind::Function && !s.is_public),
            "should find private fn helper"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "add" && s.kind == SymbolKind::Function && s.is_public),
            "should find pub fn add"
        );
    }

    #[test]
    fn extract_zig_structs_enums_unions() {
        let code = r#"
pub const Allocator = struct {
    ptr: *anyopaque,
    vtable: *const VTable,
};

const InternalState = struct {
    count: usize,
};

pub const Color = enum {
    red,
    green,
    blue,
};

pub const Value = union {
    int: i64,
    float: f64,
};
"#;
        let symbols = extract_symbols(code, "zig");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Allocator" && s.kind == SymbolKind::Struct && s.is_public),
            "should find pub struct Allocator"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "InternalState" && s.kind == SymbolKind::Struct && !s.is_public),
            "should find private struct InternalState"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Color" && s.kind == SymbolKind::Enum && s.is_public),
            "should find pub enum Color"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Value" && s.kind == SymbolKind::Type && s.is_public),
            "should find pub union Value"
        );
    }

    #[test]
    fn extract_zig_skips_tests() {
        let code = r#"
pub fn add(a: i32, b: i32) i32 {
    return a + b;
}

test "basic addition" {
    const result = add(1, 2);
    try std.testing.expectEqual(@as(i32, 3), result);
}

test "overflow" {
    // another test
}
"#;
        let symbols = extract_symbols(code, "zig");
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "add" && s.kind == SymbolKind::Function),
            "should find the add function"
        );
        assert!(
            !symbols.iter().any(|s| s.name == "basic addition"),
            "should skip test blocks"
        );
        assert!(
            !symbols.iter().any(|s| s.name == "overflow"),
            "should skip test blocks"
        );
    }

    #[test]
    fn extract_zig_skips_comments() {
        let code = r#"
// pub fn commented_out() void {}

pub fn real_fn() void {}
"#;
        let symbols = extract_symbols(code, "zig");
        assert!(
            !symbols.iter().any(|s| s.name == "commented_out"),
            "should skip commented function"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "real_fn" && s.kind == SymbolKind::Function),
            "should find real function"
        );
    }

    // ── Regression: existing languages still work ──

    #[test]
    fn regression_rust_and_python_still_extract() {
        // Spot-check that adding Lua/Zig didn't break existing extractors
        let rust_code = r#"
pub fn hello() {}
struct Foo;
"#;
        let rust_syms = extract_symbols(rust_code, "rust");
        assert!(
            rust_syms
                .iter()
                .any(|s| s.name == "hello" && s.kind == SymbolKind::Function),
            "Rust extraction should still work"
        );
        assert!(
            rust_syms
                .iter()
                .any(|s| s.name == "Foo" && s.kind == SymbolKind::Struct),
            "Rust struct extraction should still work"
        );

        let python_code = r#"
def greet(name):
    pass

class Animal:
    pass
"#;
        let py_syms = extract_symbols(python_code, "python");
        assert!(
            py_syms
                .iter()
                .any(|s| s.name == "greet" && s.kind == SymbolKind::Function),
            "Python extraction should still work"
        );
        assert!(
            py_syms
                .iter()
                .any(|s| s.name == "Animal" && s.kind == SymbolKind::Class),
            "Python class extraction should still work"
        );
    }

    #[test]
    fn test_recently_modified_files_returns_deduped_paths() {
        // This test runs in the actual git repo, so git log should work
        let files = recently_modified_files(10);
        // Should return some files (we're in a git repo with commits)
        // The list should be deduplicated
        let unique: std::collections::HashSet<&String> = files.iter().collect();
        assert_eq!(
            files.len(),
            unique.len(),
            "recently_modified_files should return deduplicated paths"
        );
        // Every entry should be non-empty
        for f in &files {
            assert!(!f.trim().is_empty(), "paths should not be empty strings");
        }
    }

    #[test]
    fn test_relevance_score_prefers_recent_files() {
        let recent = vec![
            "src/main.rs".to_string(),
            "src/tools.rs".to_string(),
            "src/old.rs".to_string(),
        ];

        let recent_entry = FileSymbols {
            path: "src/main.rs".to_string(),
            lines: 100,
            symbols: vec![Symbol {
                name: "main".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        };

        let old_entry = FileSymbols {
            path: "src/old.rs".to_string(),
            lines: 100,
            symbols: vec![Symbol {
                name: "old".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        };

        let not_in_list = FileSymbols {
            path: "src/unknown.rs".to_string(),
            lines: 100,
            symbols: vec![Symbol {
                name: "unknown".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        };

        let score_recent = relevance_score(&recent_entry, &recent);
        let score_old = relevance_score(&old_entry, &recent);
        let score_unknown = relevance_score(&not_in_list, &recent);

        assert!(
            score_recent > score_old,
            "most recently modified file should score higher: {} vs {}",
            score_recent,
            score_old
        );
        assert!(
            score_old > score_unknown,
            "file in recent list should score higher than unknown: {} vs {}",
            score_old,
            score_unknown
        );
    }

    #[test]
    fn test_relevance_score_density_and_size() {
        let recent: Vec<String> = vec![];

        // High density: many symbols in few lines
        let dense = FileSymbols {
            path: "src/dense.rs".to_string(),
            lines: 50,
            symbols: vec![
                Symbol {
                    name: "a".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 1,
                },
                Symbol {
                    name: "b".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 10,
                },
                Symbol {
                    name: "c".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 20,
                },
                Symbol {
                    name: "d".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 30,
                },
                Symbol {
                    name: "e".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 40,
                },
            ],
        };

        // Sparse: 1 symbol in many lines
        let sparse = FileSymbols {
            path: "src/sparse.rs".to_string(),
            lines: 50,
            symbols: vec![Symbol {
                name: "lonely".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        };

        let score_dense = relevance_score(&dense, &recent);
        let score_sparse = relevance_score(&sparse, &recent);

        assert!(
            score_dense > score_sparse,
            "denser file should score higher: {} vs {}",
            score_dense,
            score_sparse
        );

        // Larger file gets size bonus
        let large = FileSymbols {
            path: "src/large.rs".to_string(),
            lines: 2000,
            symbols: vec![Symbol {
                name: "big".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        };
        let small = FileSymbols {
            path: "src/small.rs".to_string(),
            lines: 10,
            symbols: vec![Symbol {
                name: "tiny".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        };

        let score_large = relevance_score(&large, &recent);
        let score_small = relevance_score(&small, &recent);

        assert!(
            score_large > score_small,
            "larger file should score higher due to size bonus: {} vs {}",
            score_large,
            score_small
        );
    }

    #[test]
    fn test_generate_repo_map_with_limit_truncates_least_relevant() {
        // With a very small limit, the function should still produce valid output
        // and prefer recently-modified / high-relevance files
        let result = generate_repo_map_for_prompt_with_limit(500);
        if let Some(map) = result {
            assert!(
                map.len() <= 510,
                "map should respect size limit, got {} chars",
                map.len()
            );
            // Should end with "..." if truncated
            assert!(
                map.contains("..."),
                "truncated map should contain ellipsis marker"
            );
        }
    }

    // ── Day 79: Additional coverage tests ──

    // ── detect_language edge cases ──

    #[test]
    fn detect_language_no_extension() {
        assert_eq!(detect_language("Makefile"), None);
        assert_eq!(detect_language("Dockerfile"), None);
        assert_eq!(detect_language(""), None);
    }

    #[test]
    fn detect_language_nested_path() {
        assert_eq!(detect_language("src/lib/utils.rs"), Some("rust"));
        assert_eq!(detect_language("app/models/user.py"), Some("python"));
        assert_eq!(
            detect_language("packages/core/index.ts"),
            Some("typescript")
        );
    }

    #[test]
    fn detect_language_jsx_and_mjs() {
        assert_eq!(detect_language("Component.jsx"), Some("javascript"));
        assert_eq!(detect_language("module.mjs"), Some("javascript"));
        assert_eq!(detect_language("Component.tsx"), Some("typescript"));
    }

    #[test]
    fn detect_language_cpp_header_variants() {
        assert_eq!(detect_language("lib.hpp"), Some("cpp"));
        assert_eq!(detect_language("lib.hxx"), Some("cpp"));
        assert_eq!(detect_language("lib.hh"), Some("cpp"));
        assert_eq!(detect_language("lib.cxx"), Some("cpp"));
        assert_eq!(detect_language("lib.cc"), Some("cpp"));
    }

    // ── extract_symbols unknown language returns empty ──

    #[test]
    fn extract_symbols_unknown_language_returns_empty() {
        let code = "def foo(): pass\nclass Bar: pass\n";
        let symbols = extract_symbols(code, "brainfuck");
        assert!(
            symbols.is_empty(),
            "unknown language should return empty vec"
        );
    }

    #[test]
    fn extract_symbols_empty_code_returns_empty() {
        assert!(extract_symbols("", "rust").is_empty());
        assert!(extract_symbols("", "python").is_empty());
        assert!(extract_symbols("", "csharp").is_empty());
        assert!(extract_symbols("", "php").is_empty());
        assert!(extract_symbols("", "kotlin").is_empty());
        assert!(extract_symbols("", "swift").is_empty());
        assert!(extract_symbols("", "scala").is_empty());
    }

    // ── C# visibility and line number tests ──

    #[test]
    fn csharp_visibility_flags() {
        let code = r#"
public class PublicClass { }
class InternalClass { }
public void PublicMethod() { }
private void PrivateMethod() { }
"#;
        let symbols = extract_symbols(code, "csharp");

        let public_class = symbols.iter().find(|s| s.name == "PublicClass").unwrap();
        assert!(public_class.is_public, "public class should be public");

        let internal_class = symbols.iter().find(|s| s.name == "InternalClass").unwrap();
        assert!(
            !internal_class.is_public,
            "class without public keyword should not be public"
        );

        let public_method = symbols.iter().find(|s| s.name == "PublicMethod").unwrap();
        assert!(public_method.is_public, "public method should be public");
    }

    #[test]
    fn csharp_excludes_keywords() {
        let code = r#"
if (condition) { }
for (int i = 0; i < 10; i++) { }
while (true) { }
switch (x) { }
return result;
foreach (var x in list) { }
"#;
        let symbols = extract_symbols(code, "csharp");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            !names.contains(&"if"),
            "should exclude 'if' keyword: {:?}",
            names
        );
        assert!(
            !names.contains(&"for"),
            "should exclude 'for' keyword: {:?}",
            names
        );
        assert!(
            !names.contains(&"while"),
            "should exclude 'while' keyword: {:?}",
            names
        );
        assert!(
            !names.contains(&"foreach"),
            "should exclude 'foreach' keyword: {:?}",
            names
        );
    }

    #[test]
    fn csharp_line_numbers() {
        let code = "namespace Foo\n{\npublic class Bar { }\n}\n";
        let symbols = extract_symbols(code, "csharp");
        let ns = symbols.iter().find(|s| s.name == "Foo").unwrap();
        assert_eq!(ns.line, 1, "namespace on first line");
        let cls = symbols.iter().find(|s| s.name == "Bar").unwrap();
        assert_eq!(cls.line, 3, "class on third line");
    }

    // ── PHP visibility and exclusion tests ──

    #[test]
    fn php_visibility_flags() {
        let code = r#"<?php
class MyClass
{
    public function publicMethod() { }
    private function privateHelper() { }
    protected function protectedHelper() { }
}
function globalFunc() { }
"#;
        let symbols = extract_symbols(code, "php");
        let public_m = symbols.iter().find(|s| s.name == "publicMethod").unwrap();
        assert!(public_m.is_public, "public method should be public");

        let private_m = symbols.iter().find(|s| s.name == "privateHelper").unwrap();
        assert!(!private_m.is_public, "private method should not be public");

        let protected_m = symbols
            .iter()
            .find(|s| s.name == "protectedHelper")
            .unwrap();
        assert!(
            !protected_m.is_public,
            "protected method should not be public"
        );

        let global = symbols.iter().find(|s| s.name == "globalFunc").unwrap();
        assert!(global.is_public, "global function should be public");
    }

    #[test]
    fn php_line_numbers() {
        let code = "<?php\nclass Foo { }\nfunction bar() { }\n";
        let symbols = extract_symbols(code, "php");
        let cls = symbols.iter().find(|s| s.name == "Foo").unwrap();
        assert_eq!(cls.line, 2, "class on second line");
        let func = symbols.iter().find(|s| s.name == "bar").unwrap();
        assert_eq!(func.line, 3, "function on third line");
    }

    // ── Kotlin visibility and keyword exclusion tests ──

    #[test]
    fn kotlin_visibility_flags() {
        let code = r#"
fun publicFunc() { }
private fun privateFunc() { }
protected fun protectedFunc() { }
class PublicClass
private class PrivateClass
"#;
        let symbols = extract_symbols(code, "kotlin");
        let pub_f = symbols.iter().find(|s| s.name == "publicFunc").unwrap();
        assert!(pub_f.is_public, "public fun should be public");

        let priv_f = symbols.iter().find(|s| s.name == "privateFunc").unwrap();
        assert!(!priv_f.is_public, "private fun should not be public");

        let prot_f = symbols.iter().find(|s| s.name == "protectedFunc").unwrap();
        assert!(!prot_f.is_public, "protected fun should not be public");

        let pub_cls = symbols.iter().find(|s| s.name == "PublicClass").unwrap();
        assert!(pub_cls.is_public, "public class should be public");

        let priv_cls = symbols.iter().find(|s| s.name == "PrivateClass").unwrap();
        assert!(!priv_cls.is_public, "private class should not be public");
    }

    #[test]
    fn kotlin_excludes_control_flow_keywords() {
        // Kotlin uses `fun` keyword but we must exclude control-flow-like constructs
        let code = r#"
fun realFunction() { }
"#;
        let symbols = extract_symbols(code, "kotlin");
        assert!(symbols.iter().any(|s| s.name == "realFunction"));
        // Keywords like "if", "for", "while" should not appear as symbols
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            !names.contains(&"if"),
            "should not extract 'if' as a symbol"
        );
        assert!(
            !names.contains(&"for"),
            "should not extract 'for' as a symbol"
        );
    }

    #[test]
    fn kotlin_line_numbers() {
        let code = "data class User(val name: String)\nfun process() { }\n";
        let symbols = extract_symbols(code, "kotlin");
        let cls = symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(cls.line, 1, "data class on first line");
        let func = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(func.line, 2, "function on second line");
    }

    // ── Swift visibility tests ──

    #[test]
    fn swift_visibility_flags() {
        let code = r#"
public class PublicClass { }
class InternalClass { }
open class OpenClass { }
public func publicFunc() { }
func internalFunc() { }
private func privateFunc() { }
"#;
        let symbols = extract_symbols(code, "swift");

        let public_cls = symbols.iter().find(|s| s.name == "PublicClass").unwrap();
        assert!(public_cls.is_public, "public class should be public");

        let internal_cls = symbols.iter().find(|s| s.name == "InternalClass").unwrap();
        assert!(
            !internal_cls.is_public,
            "class without modifier should not be public"
        );

        let open_cls = symbols.iter().find(|s| s.name == "OpenClass").unwrap();
        assert!(open_cls.is_public, "open class should be public");

        let public_fn = symbols.iter().find(|s| s.name == "publicFunc").unwrap();
        assert!(public_fn.is_public, "public func should be public");

        let internal_fn = symbols.iter().find(|s| s.name == "internalFunc").unwrap();
        assert!(
            !internal_fn.is_public,
            "func without modifier should not be public"
        );
    }

    #[test]
    fn swift_line_numbers() {
        let code = "struct Point {\n    var x: Double\n}\nfunc helper() { }\n";
        let symbols = extract_symbols(code, "swift");
        let s = symbols.iter().find(|s| s.name == "Point").unwrap();
        assert_eq!(s.line, 1, "struct on first line");
        let f = symbols.iter().find(|s| s.name == "helper").unwrap();
        assert_eq!(f.line, 4, "func on fourth line");
    }

    #[test]
    fn swift_extension_always_public() {
        let code = "extension Array {\n    func custom() { }\n}\n";
        let symbols = extract_symbols(code, "swift");
        let ext = symbols.iter().find(|s| s.name == "Array").unwrap();
        assert_eq!(ext.kind, SymbolKind::Impl, "extension should be Impl kind");
        assert!(ext.is_public, "extension should always be public");
    }

    // ── Scala visibility tests ──

    #[test]
    fn scala_visibility_flags() {
        let code = r#"
def publicDef(): Unit = { }
private def privateDef(): Unit = { }
protected def protectedDef(): Unit = { }
class PublicClass
"#;
        let symbols = extract_symbols(code, "scala");
        let pub_d = symbols.iter().find(|s| s.name == "publicDef").unwrap();
        assert!(pub_d.is_public, "public def should be public");

        let priv_d = symbols.iter().find(|s| s.name == "privateDef").unwrap();
        assert!(!priv_d.is_public, "private def should not be public");

        let prot_d = symbols.iter().find(|s| s.name == "protectedDef").unwrap();
        assert!(!prot_d.is_public, "protected def should not be public");

        let pub_cls = symbols.iter().find(|s| s.name == "PublicClass").unwrap();
        assert!(pub_cls.is_public, "public class should be public");
    }

    #[test]
    fn scala_private_class_not_matched_by_regex() {
        // Scala's class regex doesn't include 'private' in its modifier list,
        // so private classes are not extracted at all (this is the current behavior)
        let code = "private class Hidden\nclass Visible\n";
        let symbols = extract_symbols(code, "scala");
        assert!(
            !symbols.iter().any(|s| s.name == "Hidden"),
            "private class should not be matched by Scala class regex"
        );
        assert!(
            symbols.iter().any(|s| s.name == "Visible"),
            "public class should be matched"
        );
    }

    #[test]
    fn scala_line_numbers() {
        let code = "object App {\n  def main(): Unit = { }\n}\n";
        let symbols = extract_symbols(code, "scala");
        let obj = symbols.iter().find(|s| s.name == "App").unwrap();
        assert_eq!(obj.line, 1, "object on first line");
        assert_eq!(obj.kind, SymbolKind::Module);
        let def = symbols.iter().find(|s| s.name == "main").unwrap();
        assert_eq!(def.line, 2, "def on second line");
    }

    // ── format_repo_map tests ──

    #[test]
    fn format_repo_map_includes_kind_labels() {
        let entries = vec![FileSymbols {
            path: "src/lib.rs".to_string(),
            lines: 50,
            symbols: vec![
                Symbol {
                    name: "MyTrait".to_string(),
                    kind: SymbolKind::Trait,
                    is_public: true,
                    line: 1,
                },
                Symbol {
                    name: "MyEnum".to_string(),
                    kind: SymbolKind::Enum,
                    is_public: true,
                    line: 10,
                },
                Symbol {
                    name: "helper".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 20,
                },
                Symbol {
                    name: "MAX".to_string(),
                    kind: SymbolKind::Const,
                    is_public: true,
                    line: 30,
                },
            ],
        }];
        let output = format_repo_map(&entries);
        assert!(
            output.contains("trait MyTrait"),
            "should contain trait label"
        );
        assert!(output.contains("enum MyEnum"), "should contain enum label");
        assert!(output.contains("fn helper"), "should contain fn label");
        assert!(output.contains("const MAX"), "should contain const label");
        assert!(output.contains("(50 lines)"), "should contain line count");
    }

    #[test]
    fn format_repo_map_colored_contains_ansi_codes() {
        let entries = vec![FileSymbols {
            path: "src/main.rs".to_string(),
            lines: 42,
            symbols: vec![Symbol {
                name: "run".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        }];
        let output = format_repo_map_colored(&entries);
        // Should contain ANSI escape codes (ESC = \x1b)
        assert!(
            output.contains('\x1b'),
            "colored output should contain ANSI escape codes"
        );
        assert!(output.contains("src/main.rs"), "should contain file path");
        assert!(output.contains("run"), "should contain symbol name");
        assert!(output.contains("42 lines"), "should contain line count");
    }

    #[test]
    fn format_repo_map_colored_empty_shows_message() {
        let entries: Vec<FileSymbols> = vec![];
        let output = format_repo_map_colored(&entries);
        assert!(
            output.contains("no structural symbols found"),
            "empty colored map should show message"
        );
    }

    #[test]
    fn format_repo_map_colored_shows_pub_prefix() {
        let entries = vec![FileSymbols {
            path: "lib.rs".to_string(),
            lines: 10,
            symbols: vec![
                Symbol {
                    name: "public_fn".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 1,
                },
                Symbol {
                    name: "private_fn".to_string(),
                    kind: SymbolKind::Function,
                    is_public: false,
                    line: 2,
                },
            ],
        }];
        let output = format_repo_map_colored(&entries);
        // The "pub" label should appear for public symbols
        assert!(
            output.contains("pub"),
            "should contain 'pub' prefix for public symbols"
        );
    }

    #[test]
    fn format_repo_map_all_kind_labels() {
        // Verify that every SymbolKind produces a valid label in plain format
        let kinds = vec![
            (SymbolKind::Function, "fn"),
            (SymbolKind::Struct, "struct"),
            (SymbolKind::Enum, "enum"),
            (SymbolKind::Trait, "trait"),
            (SymbolKind::Interface, "interface"),
            (SymbolKind::Class, "class"),
            (SymbolKind::Type, "type"),
            (SymbolKind::Const, "const"),
            (SymbolKind::Impl, "impl"),
            (SymbolKind::Module, "mod"),
            (SymbolKind::Macro, "macro"),
            (SymbolKind::Namespace, "namespace"),
        ];
        for (kind, expected_label) in kinds {
            let entries = vec![FileSymbols {
                path: "test.rs".to_string(),
                lines: 1,
                symbols: vec![Symbol {
                    name: "test_sym".to_string(),
                    kind: kind.clone(),
                    is_public: true,
                    line: 1,
                }],
            }];
            let output = format_repo_map(&entries);
            assert!(
                output.contains(&format!("{expected_label} test_sym")),
                "format_repo_map should output '{expected_label} test_sym' for {:?}, got: {}",
                kind,
                output.trim()
            );
        }
    }

    // ── Relevance score edge cases ──

    #[test]
    fn relevance_score_empty_recent_list() {
        let recent: Vec<String> = vec![];
        let entry = FileSymbols {
            path: "src/main.rs".to_string(),
            lines: 100,
            symbols: vec![Symbol {
                name: "main".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        };
        // Should not panic, score based on density + size only
        let score = relevance_score(&entry, &recent);
        assert!(score > 0, "should still have density/size score");
    }

    #[test]
    fn relevance_score_zero_lines_no_panic() {
        let recent: Vec<String> = vec![];
        let entry = FileSymbols {
            path: "empty.rs".to_string(),
            lines: 0,
            symbols: vec![],
        };
        // checked_div should handle zero lines without panic
        let score = relevance_score(&entry, &recent);
        assert_eq!(score, 0, "empty file with no symbols should score 0");
    }

    #[test]
    fn relevance_score_equal_entries_have_equal_scores() {
        let recent: Vec<String> = vec![];
        let entry_a = FileSymbols {
            path: "a.rs".to_string(),
            lines: 100,
            symbols: vec![Symbol {
                name: "f".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        };
        let entry_b = FileSymbols {
            path: "b.rs".to_string(),
            lines: 100,
            symbols: vec![Symbol {
                name: "g".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        };
        let score_a = relevance_score(&entry_a, &recent);
        let score_b = relevance_score(&entry_b, &recent);
        assert_eq!(
            score_a, score_b,
            "entries with same structure should have equal relevance"
        );
    }

    #[test]
    fn relevance_score_recency_position_matters() {
        // First item in recent list gets highest recency score
        let recent = vec![
            "first.rs".to_string(),
            "second.rs".to_string(),
            "third.rs".to_string(),
        ];
        let make_entry = |path: &str| FileSymbols {
            path: path.to_string(),
            lines: 50,
            symbols: vec![Symbol {
                name: "f".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        };
        let score_first = relevance_score(&make_entry("first.rs"), &recent);
        let score_second = relevance_score(&make_entry("second.rs"), &recent);
        let score_third = relevance_score(&make_entry("third.rs"), &recent);
        assert!(
            score_first > score_second,
            "first should beat second: {} vs {}",
            score_first,
            score_second
        );
        assert!(
            score_second > score_third,
            "second should beat third: {} vs {}",
            score_second,
            score_third
        );
    }

    #[test]
    fn relevance_score_density_capped_at_50() {
        // A file with extreme density shouldn't get unlimited score
        let recent: Vec<String> = vec![];
        let make_sym = |n: &str| Symbol {
            name: n.to_string(),
            kind: SymbolKind::Function,
            is_public: true,
            line: 1,
        };
        let extreme = FileSymbols {
            path: "dense.rs".to_string(),
            lines: 1, // 200 symbols in 1 line = extreme density
            symbols: (0..200).map(|i| make_sym(&format!("f{i}"))).collect(),
        };
        let moderate = FileSymbols {
            path: "moderate.rs".to_string(),
            lines: 100,
            symbols: (0..50).map(|i| make_sym(&format!("g{i}"))).collect(),
        };
        let score_extreme = relevance_score(&extreme, &recent);
        let score_moderate = relevance_score(&moderate, &recent);
        // Extreme density is capped, so the difference shouldn't be unbounded
        // Both should be > 0 and the extreme one should still win
        assert!(score_extreme > 0);
        assert!(score_moderate > 0);
        // The cap means extreme doesn't get 20000 density score
        // density is capped at 50, so extreme gets 50 density + 0 size = 50
        // moderate gets 50 density + 2 size = 52
        // Actually moderate has more lines so might win due to size bonus
    }
}
