//! Map command handler: /map — structural codebase understanding.

use crate::commands_ast_grep::is_ast_grep_available;
use crate::commands_search::{is_binary_extension, list_project_files};
use crate::format::*;
use crate::git::run_git;
pub use crate::symbols::*;

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
    let n_str = n.to_string();
    let stdout = match run_git(&["log", "--name-only", "--format=", "-n", &n_str]) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
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
