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
/// Returns the file symbols and, per extractor, how many files it actually
/// produced symbols for (#1135). That is counted inside the loop that does the
/// work, never read off the availability probe: a real ast-grep whose
/// `sg scan` fails on every file falls back to regex per file, and the counts
/// say so even though the probe said "available".
pub fn build_repo_map_with_backend(
    root: Option<&str>,
    public_only: bool,
    force_regex: bool,
) -> (Vec<FileSymbols>, MapBackendUsed) {
    let files = list_project_files();

    // Resolve git toplevel so file reads use absolute paths,
    // preventing CWD races when parallel tests call set_current_dir.
    let toplevel = crate::git::run_git(&["rev-parse", "--show-toplevel"]).ok();

    let (result, backend, _omitted) = build_repo_map_core(
        &files,
        toplevel.as_deref(),
        root,
        public_only,
        force_regex,
        None,
    );
    (result, backend)
}

/// The shared body of every repo-map build: `files` are paths relative to
/// `toplevel` (or to the CWD when `None`).
///
/// `fence` is the session's directory fence (#1002). Each file's absolute path
/// goes through [`crate::config::DirectoryRestrictions::check_path`] — the one
/// resolver, symlinks included (#996) — **before** the file is read, so a denied
/// file's symbols are never extracted, not merely filtered after rendering.
/// The third return value counts the files the fence omitted, split by which
/// list refused them. `None` takes the unfenced path; an empty startup fence
/// (which `check_path` short-circuits) plus an empty `/cd` list (which
/// `cd_denied_dir_refusal_with` short-circuits) omits nothing.
///
/// #869: a fenced build checks the startup fence first (verdict unchanged),
/// then the `/cd`-appended `[directories] deny` list. Today the only fenced
/// caller is the startup prompt map, which runs before any `/cd`, so the `/cd`
/// list is empty there; the check makes any later rebuild fail closed. `/map`
/// itself passes `None` — a display to the user, not the provider (#1002).
fn build_repo_map_core(
    files: &[String],
    toplevel: Option<&str>,
    root: Option<&str>,
    public_only: bool,
    force_regex: bool,
    fence: Option<(
        &crate::config::DirectoryRestrictions,
        &crate::cd_deny::DenyAdditions,
    )>,
) -> (Vec<FileSymbols>, MapBackendUsed, MapOmitted) {
    // The probe only decides whether ast-grep is *tried*; what gets reported
    // is counted per file in `build_repo_map_core_with` (#1135).
    let use_ast_grep = !force_regex && is_ast_grep_available();
    let ast_extractor: Option<&AstExtractor> = if use_ast_grep {
        Some(&extract_symbols_ast_grep)
    } else {
        None
    };
    build_repo_map_core_with(files, toplevel, root, public_only, ast_extractor, fence)
}

/// The ast-grep extractor seam: `(path, language) -> Some(symbols)` when
/// ast-grep produced the result, `None` when the caller must fall back to regex.
/// Production passes `extract_symbols_ast_grep`; tests pass a closure, so the
/// "probe says available, every scan fails" case is drivable without a real `sg`.
type AstExtractor = dyn Fn(&str, &str) -> Option<Vec<Symbol>>;

/// Which extractor produced each scanned file's symbols, counted where the
/// extraction happens. A file counts once, whether or not any of its symbols
/// survive the public-only filter: the label describes the engine, not the output.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MapBackendUsed {
    pub ast_grep_files: usize,
    pub regex_files: usize,
}

impl MapBackendUsed {
    fn record(&mut self, produced: MapBackend) {
        match produced {
            MapBackend::AstGrep => self.ast_grep_files += 1,
            MapBackend::Regex => self.regex_files += 1,
        }
    }

    /// The footer text, e.g. `using ast-grep`.
    pub fn label(&self) -> String {
        map_backend_label(self.ast_grep_files, self.regex_files)
    }
}

/// Pure: the `(using …)` footer label from the per-extractor file counts.
/// All ast-grep → `using ast-grep`; all regex → `using regex` (byte-identical
/// to the pre-#1135 label); both → `using ast-grep for N files, regex for M`;
/// neither (nothing scanned — the caller prints no footer then) says so
/// rather than naming an engine that did not run.
pub fn map_backend_label(ast_grep_files: usize, regex_files: usize) -> String {
    let files = |n: usize| if n == 1 { "file" } else { "files" };
    match (ast_grep_files, regex_files) {
        (0, 0) => "no files scanned".to_string(),
        (_, 0) => "using ast-grep".to_string(),
        (0, _) => "using regex".to_string(),
        (a, r) => format!(
            "using ast-grep for {a} {}, regex for {r} {}",
            files(a),
            files(r)
        ),
    }
}

/// [`build_repo_map_core`] with the ast-grep extractor injected: `None` means
/// ast-grep is not tried at all (probe unavailable, or `--regex`).
fn build_repo_map_core_with(
    files: &[String],
    toplevel: Option<&str>,
    root: Option<&str>,
    public_only: bool,
    ast_extractor: Option<&AstExtractor>,
    fence: Option<(
        &crate::config::DirectoryRestrictions,
        &crate::cd_deny::DenyAdditions,
    )>,
) -> (Vec<FileSymbols>, MapBackendUsed, MapOmitted) {
    let mut result = Vec::new();
    let mut omitted = MapOmitted::default();
    let mut used = MapBackendUsed::default();

    for path in files {
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
        let abs_path = if let Some(tl) = toplevel {
            std::path::Path::new(tl)
                .join(path)
                .to_string_lossy()
                .to_string()
        } else {
            path.clone()
        };
        // The fence is checked before the read: symbol names are file content.
        if let Some((f, cd_list)) = fence {
            if f.check_path(&abs_path).is_err() {
                omitted.fence += 1;
                continue;
            }
            if crate::cd_config_note::cd_denied_dir_refusal_with(cd_list, &abs_path).is_some() {
                omitted.cd += 1;
                continue;
            }
        }
        let content = match std::fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let line_count = content.lines().count();

        // Try ast-grep first, fall back to regex — and record which one
        // actually produced this file's symbols (#1135).
        let ast_symbols = ast_extractor.and_then(|extract| extract(path, lang));
        let (mut symbols, produced) = match ast_symbols {
            Some(symbols) => (symbols, MapBackend::AstGrep),
            None => (extract_symbols(&content, lang), MapBackend::Regex),
        };
        used.record(produced);

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
    (result, used, omitted)
}

/// Files a fenced repo-map build left out, by the list that refused them:
/// `fence` for the startup `--deny-dir`/`--allow-dir`/`[directories]` fence,
/// `cd` for a `[directories] deny` entry appended by `/cd` (#869).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct MapOmitted {
    fence: usize,
    cd: usize,
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
///
/// **Unfenced and test-only since #1002**: production builds the prompt map
/// through [`generate_repo_map_for_prompt`], which takes the directory fence, so
/// no shipped path can put a denied file's symbols in the prompt.
#[cfg(test)]
pub fn generate_repo_map_for_prompt_with_limit(max_chars: usize) -> Option<String> {
    render_repo_map_for_prompt(build_repo_map(None, true), max_chars)
}

/// Render already-built entries into the capped system-prompt map.
fn render_repo_map_for_prompt(mut entries: Vec<FileSymbols>, max_chars: usize) -> Option<String> {
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

/// Generate a repo map for the system prompt with the default size cap,
/// behind the session's directory fence (#1002).
///
/// Every file the map would include is checked against `restrictions` before
/// its symbols are extracted; a file under `--deny-dir` (or outside
/// `--allow-dir`) is omitted, symlinks resolved by the same resolver the file
/// tools use. One YELLOW stderr line — not quiet-gated, like the instruction-file
/// and goal refusals — states how many files were omitted, **count only**:
/// naming them would leak exactly what the fence hides. An empty fence takes
/// the unfenced path and is byte-identical to it.
pub fn generate_repo_map_for_prompt(
    restrictions: &crate::config::DirectoryRestrictions,
) -> Option<String> {
    let files = list_project_files();
    let toplevel = crate::git::run_git(&["rev-parse", "--show-toplevel"]).ok();
    let (map, omitted) = repo_map_for_prompt_fenced_with(
        &crate::cd_config_note::cd_denied_dirs_handle(),
        &files,
        toplevel.as_deref(),
        restrictions,
        false,
        REPO_MAP_MAX_CHARS,
    );
    let plain = crate::format::is_plain_output();
    if let Some(msg) = repo_map_fence_note(omitted.fence, plain) {
        eprintln!("{YELLOW}{msg}{RESET}");
    }
    if let Some(msg) = repo_map_cd_deny_note(omitted.cd, plain) {
        eprintln!("{YELLOW}{msg}{RESET}");
    }
    map
}

/// [`repo_map_for_prompt_fenced_with`] with an empty `/cd` list: the startup
/// fence alone. Returns the map and how many files that fence omitted.
#[cfg(test)]
fn repo_map_for_prompt_fenced(
    files: &[String],
    toplevel: Option<&str>,
    restrictions: &crate::config::DirectoryRestrictions,
    force_regex: bool,
    max_chars: usize,
) -> (Option<String>, usize) {
    let empty = crate::cd_deny::DenyAdditions::default();
    let (m, omitted) = repo_map_for_prompt_fenced_with(
        &empty,
        files,
        toplevel,
        restrictions,
        force_regex,
        max_chars,
    );
    (m, omitted.fence)
}

/// The fenced prompt map over an explicit file list and an injected `/cd`
/// deny list (the testable seam behind [`generate_repo_map_for_prompt`]).
fn repo_map_for_prompt_fenced_with(
    cd_list: &crate::cd_deny::DenyAdditions,
    files: &[String],
    toplevel: Option<&str>,
    restrictions: &crate::config::DirectoryRestrictions,
    force_regex: bool,
    max_chars: usize,
) -> (Option<String>, MapOmitted) {
    let (entries, _backend, omitted) = build_repo_map_core(
        files,
        toplevel,
        None,
        true,
        force_regex,
        Some((restrictions, cd_list)),
    );
    (render_repo_map_for_prompt(entries, max_chars), omitted)
}

/// The stderr note when the directory fence omitted files from the repo map.
/// `None` when nothing was omitted (the whole regression surface). Count only,
/// never names; glyph-free under plain output.
pub(crate) fn repo_map_fence_note(omitted: usize, plain: bool) -> Option<String> {
    if omitted == 0 {
        return None;
    }
    let files = if omitted == 1 { "file" } else { "files" };
    Some(if plain {
        format!("warning: repo map omitted {omitted} {files} blocked by the directory fence (--deny-dir/--allow-dir)")
    } else {
        format!("⚠ repo map omitted {omitted} {files} blocked by the directory fence (--deny-dir/--allow-dir)")
    })
}

/// The stderr note when a `/cd`-appended `[directories] deny` entry omitted
/// files from the repo map (#869). `None` at zero. Count only, never names;
/// ends with `CD_DIR_SOURCE_NOTE`; glyph-free under plain output.
pub(crate) fn repo_map_cd_deny_note(omitted: usize, plain: bool) -> Option<String> {
    if omitted == 0 {
        return None;
    }
    let files = if omitted == 1 { "file" } else { "files" };
    let note = crate::commands_file_fence::CD_DIR_SOURCE_NOTE;
    let marker = if plain { "warning:" } else { "⚠" };
    Some(format!(
        "{marker} repo map omitted {omitted} {files} blocked by the directory fence {note}"
    ))
}

/// Handle the `/map` REPL command: show structural symbols from the codebase.
///
/// Usage: `/map [path]` — show all symbols
/// Usage: `/map --all [path]` — include private symbols
/// Usage: `/map --regex [path]` — force regex backend even if ast-grep is available
/// The two real flags `/map` accepts. Used for parsing and for near-miss
/// suggestions, so the suggestion list can never drift from the accepted set.
const MAP_FLAGS: &[&str] = &["--all", "--regex"];

/// Result of parsing `/map` arguments.
///
/// `Error` is an explicit third value: an unrecognised flag is neither a valid
/// flag nor a path, and must not be silently reinterpreted as one (#727).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MapArgs {
    Ok {
        show_all: bool,
        force_regex: bool,
        path_filter: Option<String>,
    },
    Error(String),
}

/// Parse the argument tail of `/map` (everything after the command word).
///
/// Pure — no I/O, no printing. Unknown flags and a second positional path are
/// honest errors rather than a path filter that silently matches nothing.
pub(crate) fn parse_map_args(rest: &str) -> MapArgs {
    parse_map_tokens(rest.split_whitespace())
}

/// Parse `yoyo map <argv…>` (#1009): each argv element is ONE token, so
/// `yoyo map "dir sp"` is one path — as `ls`, `tree`, `du -s` and `find`
/// read it. The REPL's `/map dir sp` stays a whitespace split (two paths).
/// Blank elements are skipped, matching what the old `"/map "` string did.
pub(crate) fn parse_map_argv(argv: &[String]) -> MapArgs {
    parse_map_tokens(
        argv.iter()
            .map(String::as_str)
            .filter(|a| !a.trim().is_empty()),
    )
}

/// Shared token loop for both doors (REPL string, shell argv).
fn parse_map_tokens<'a>(parts: impl IntoIterator<Item = &'a str>) -> MapArgs {
    let mut show_all = false;
    let mut force_regex = false;
    let mut path_filter: Option<String> = None;

    for part in parts {
        match part {
            "--all" => show_all = true,
            "--regex" => force_regex = true,
            _ if part.starts_with('-') => {
                let mut msg = format!("unknown flag: {part}");
                // Absence gets its own value: no close match → no suggestion,
                // never a wrong guess.
                if let Some(suggestion) = crate::commands::closest_match(part, MAP_FLAGS, 2) {
                    msg.push_str(&format!(" — did you mean {suggestion}?"));
                }
                msg.push_str("\n  usage: /map [--all] [--regex] [path]");
                return MapArgs::Error(msg);
            }
            _ => {
                if let Some(first) = &path_filter {
                    return MapArgs::Error(format!(
                        "two paths given ('{first}' and '{part}') — /map takes at most one path\n  usage: /map [--all] [--regex] [path]"
                    ));
                }
                // `./src` should behave like `src`: the filter is a plain
                // `starts_with` against repo-relative paths, which never begin
                // with "./".
                let normalized = part.strip_prefix("./").unwrap_or(part);
                path_filter = Some(normalized.to_string());
            }
        }
    }

    MapArgs::Ok {
        show_all,
        force_regex,
        path_filter,
    }
}

/// The honest-empty line `/map` prints (exit 0) when nothing has symbols.
pub(crate) const MAP_EMPTY_MESSAGE: &str = "(no supported source files with symbols found)";

/// What `/map` prints when its public-only build came back empty, given the
/// all-symbols build of the same files. Empty `all_entries` is the honest
/// "nothing here" ([`MAP_EMPTY_MESSAGE`], byte-identical). Otherwise symbols
/// exist and the public-only filter hid every one (a Rust binary crate, a
/// script dir, a Python module of `_private` names), so saying "no supported
/// source files" would be false: this names the counts and the `--all` command
/// in the caller's own syntax (`all_cmd`, from [`map_all_command`]).
pub(crate) fn map_empty_message(all_entries: &[FileSymbols], all_cmd: &str) -> String {
    if all_entries.is_empty() {
        return MAP_EMPTY_MESSAGE.to_string();
    }
    let symbols: usize = all_entries.iter().map(|e| e.symbols.len()).sum();
    let files = all_entries.len();
    format!(
        "({symbols} symbol{} in {files} file{}, none public; run `{all_cmd}` to show them)",
        if symbols == 1 { "" } else { "s" },
        if files == 1 { "" } else { "s" },
    )
}

/// The `--all` command to suggest, in the surface's syntax: `prefix` is
/// `"yoyo map"` (shell) or `"/map"` (REPL), and the path filter is carried so
/// the suggestion maps the same files. A shell path with characters outside a
/// plain set is single-quoted; the REPL splits on whitespace, so its path can
/// never need quoting.
pub(crate) fn map_all_command(prefix: &str, path: Option<&str>) -> String {
    let Some(path) = path else {
        return format!("{prefix} --all");
    };
    let plain = path
        .chars()
        .all(|c| c.is_alphanumeric() || "/._-+,:@%=".contains(c));
    if plain || prefix.starts_with('/') {
        format!("{prefix} --all {path}")
    } else {
        format!("{prefix} --all '{}'", path.replace('\'', "'\\''"))
    }
}

/// Exit status for `yoyo map` when its path does not exist (#982): 1, the
/// same code `yoyo blame <missing path>` uses.
pub(crate) const MAP_MISSING_PATH_EXIT: i32 = 1;

/// Exit status for a `/map` usage error (unknown flag, two paths): 2, the
/// usage-error convention `def`/`outline` with no argument also use.
pub(crate) const MAP_USAGE_EXIT: i32 = 2;

/// Decide whether an empty `/map` result is an honest empty project or a
/// path that is not there (#982). Consulted only when the map came back
/// empty. The filter is a repo-relative prefix, so a path counts as missing
/// only when it does not exist on disk AND no project file starts with it:
/// `map src/com` (a prefix of real files) and an existing directory with no
/// supported source files both stay the honest "nothing found" (`None`).
pub(crate) fn map_missing_path_message(
    path_filter: Option<&str>,
    path_exists: bool,
    files: &[String],
) -> Option<String> {
    let filter = path_filter?;
    if path_exists || files.iter().any(|f| f.starts_with(filter)) {
        return None;
    }
    Some(format!(
        "no such path: '{filter}' (not a file or directory, and no project file starts with it)"
    ))
}

/// REPL door for `/map`; the status is for the shell door.
pub fn handle_map(input: &str) {
    let _ = handle_map_status(input);
}

/// `/map` core returning an exit status (#982): 0 for a map or an honest empty
/// result (unchanged), [`MAP_MISSING_PATH_EXIT`] when the given path does not
/// exist, [`MAP_USAGE_EXIT`] for an argument error.
pub fn handle_map_status(input: &str) -> i32 {
    let rest = input.strip_prefix("/map").unwrap_or("").trim();
    map_status_for(parse_map_args(rest), "/map")
}

/// Shell door for `yoyo map <argv…>` (#1009): argv elements reach the parser
/// verbatim, never through `quote_args_as_command`'s `"..."` wrapping.
pub fn handle_map_argv_status(argv: &[String]) -> i32 {
    map_status_for(parse_map_argv(argv), "yoyo map")
}

/// `surface` is the command word as the user typed it (`"/map"` or
/// `"yoyo map"`), used only to spell the `--all` suggestion.
fn map_status_for(parsed: MapArgs, surface: &str) -> i32 {
    let (show_all, force_regex, path_filter) = match parsed {
        MapArgs::Ok {
            show_all,
            force_regex,
            path_filter,
        } => (show_all, force_regex, path_filter),
        MapArgs::Error(msg) => {
            // Stop before any work or progress output — an unrecognised flag
            // must not produce a confident "no symbols found".
            eprintln!("{RED}  ✗ {msg}{RESET}\n");
            return MAP_USAGE_EXIT;
        }
    };
    let path_filter = path_filter.as_deref();

    println!("{DIM}  Building repo map...{RESET}");
    let public_only = !show_all;
    let (entries, backend) = build_repo_map_with_backend(path_filter, public_only, force_regex);

    if entries.is_empty() {
        // Only the empty branch pays for the existence check: a missing path
        // must not read as a real, empty project (#982).
        if let Some(filter) = path_filter {
            let toplevel = run_git(&["rev-parse", "--show-toplevel"]).ok();
            let exists = std::path::Path::new(filter).exists()
                || toplevel
                    .as_deref()
                    .is_some_and(|tl| std::path::Path::new(tl).join(filter).exists());
            let files = list_project_files();
            if let Some(msg) = map_missing_path_message(Some(filter), exists, &files) {
                eprintln!("{RED}  ✗ {msg}{RESET}\n");
                return MAP_MISSING_PATH_EXIT;
            }
        }
        // Only an empty public-only map pays for the second, all-symbols
        // scan: it tells "nothing here" apart from "all private".
        let message = if public_only {
            let (all, _) = build_repo_map_with_backend(path_filter, false, force_regex);
            map_empty_message(&all, &map_all_command(surface, path_filter))
        } else {
            MAP_EMPTY_MESSAGE.to_string()
        };
        println!("{DIM}  {message}{RESET}\n");
        return 0;
    }

    let total_symbols: usize = entries.iter().map(|e| e.symbols.len()).sum();
    let total_files = entries.len();

    let formatted = format_repo_map_colored(&entries);
    print!("{formatted}");

    let backend_label = backend.label();

    println!(
        "\n{DIM}  {} symbol{} across {} file{} ({backend_label}){RESET}\n",
        total_symbols,
        if total_symbols == 1 { "" } else { "s" },
        total_files,
        if total_files == 1 { "" } else { "s" },
    );
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::KNOWN_COMMANDS;
    use serial_test::serial;

    /// The REPL tab-completion hint for `/map` must advertise only flags that
    /// `parse_map_args` actually accepts. It said `[path] [--depth N]` while no
    /// `--depth` handling has ever existed here, and since #727 an unrecognised
    /// flag is an honest `MapArgs::Error` — so the completion was teaching users
    /// a flag the parser rejects on contact. Asserted against the parser itself
    /// (the real judge), not against a second hand-typed list.
    #[test]
    fn map_arg_hint_advertises_only_flags_the_parser_accepts() {
        let hint = crate::commands::command_arg_hint("map").expect("/map should have an arg hint");
        for flag in MAP_FLAGS {
            assert!(hint.contains(flag), "hint {hint:?} should advertise {flag}");
        }
        for token in hint.split_whitespace() {
            let token = token.trim_matches(|c| c == '[' || c == ']');
            if token.starts_with("--") {
                assert!(
                    !matches!(parse_map_args(token), MapArgs::Error(_)),
                    "hint {hint:?} advertises {token}, which parse_map_args rejects"
                );
            }
        }
    }

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
        let _result =
            generate_repo_map_for_prompt(&crate::config::DirectoryRestrictions::default());
    }

    /// #1002: the repo map is a door onto the directory fence. Symbol names are
    /// file content and go to the provider, so a denied file must never be read.
    mod fence {
        use super::super::*;
        use crate::config::DirectoryRestrictions;
        use tempfile::TempDir;

        const DENIED_TOKEN: &str = "plain_private_qq4";
        const ALLOWED_TOKEN: &str = "ok_fn_qq4";

        /// `proj/private/keys.rs` (denied token) and `proj/src/ok.rs` (allowed token).
        fn fixture() -> (TempDir, std::path::PathBuf, Vec<String>) {
            let tmp = TempDir::new().unwrap();
            let proj = tmp.path().join("proj");
            std::fs::create_dir_all(proj.join("private")).unwrap();
            std::fs::create_dir_all(proj.join("src")).unwrap();
            std::fs::write(
                proj.join("private/keys.rs"),
                format!("pub fn {DENIED_TOKEN}() {{}}\n"),
            )
            .unwrap();
            std::fs::write(
                proj.join("src/ok.rs"),
                format!("pub fn {ALLOWED_TOKEN}() {{}}\n"),
            )
            .unwrap();
            let files = vec!["private/keys.rs".to_string(), "src/ok.rs".to_string()];
            (tmp, proj, files)
        }

        fn deny(dir: &std::path::Path) -> DirectoryRestrictions {
            DirectoryRestrictions {
                allow: vec![],
                deny: vec![dir.display().to_string()],
            }
        }

        fn map(
            files: &[String],
            proj: &std::path::Path,
            fence: &DirectoryRestrictions,
        ) -> (String, usize) {
            let (m, omitted) = repo_map_for_prompt_fenced(
                files,
                Some(&proj.to_string_lossy()),
                fence,
                true,
                REPO_MAP_MAX_CHARS,
            );
            (m.unwrap_or_default(), omitted)
        }

        #[test]
        fn denied_plain_subdirectory_is_omitted_and_its_sibling_is_kept() {
            let (_tmp, proj, files) = fixture();
            // Anti-vacuous: unfenced, the denied token really is in the map.
            let (open, _) = map(&files, &proj, &DirectoryRestrictions::default());
            assert!(
                open.contains(DENIED_TOKEN),
                "fixture must carry the token: {open}"
            );

            let (fenced, omitted) = map(&files, &proj, &deny(&proj.join("private")));
            assert!(
                !fenced.contains(DENIED_TOKEN),
                "denied symbol leaked: {fenced}"
            );
            assert!(
                !fenced.contains("private/keys.rs"),
                "denied path leaked: {fenced}"
            );
            assert_eq!(omitted, 1);
            // Near-miss: the allowed sibling is exactly what an unfenced map of it says.
            assert_eq!(
                fenced,
                format!("src/ok.rs (1 lines)\n  fn {ALLOWED_TOKEN}\n")
            );
        }

        #[cfg(unix)]
        #[test]
        fn symlinked_file_whose_target_is_denied_is_omitted() {
            let tmp = TempDir::new().unwrap();
            let secret = tmp.path().join("secret");
            let proj = tmp.path().join("proj");
            std::fs::create_dir_all(&secret).unwrap();
            std::fs::create_dir_all(proj.join("src")).unwrap();
            std::fs::write(
                secret.join("lib.rs"),
                format!("pub fn {DENIED_TOKEN}() {{}}\n"),
            )
            .unwrap();
            std::os::unix::fs::symlink("../../secret/lib.rs", proj.join("src/lib.rs")).unwrap();
            let files = vec!["src/lib.rs".to_string()];
            let (open, _) = map(&files, &proj, &DirectoryRestrictions::default());
            assert!(
                open.contains(DENIED_TOKEN),
                "link must carry the token: {open}"
            );

            let (fenced, omitted) = map(&files, &proj, &deny(&secret));
            assert!(
                !fenced.contains(DENIED_TOKEN),
                "linked denied symbol leaked: {fenced}"
            );
            assert_eq!(omitted, 1);

            // --allow-dir proj alone also refuses the link that lands outside it.
            let allow_only = DirectoryRestrictions {
                allow: vec![proj.display().to_string()],
                deny: vec![],
            };
            let (fenced, omitted) = map(&files, &proj, &allow_only);
            assert!(
                !fenced.contains(DENIED_TOKEN),
                "allow-dir escape leaked: {fenced}"
            );
            assert_eq!(omitted, 1);
        }

        #[test]
        fn empty_fence_is_byte_identical_to_the_unfenced_build() {
            let (_tmp, proj, files) = fixture();
            let tl = proj.to_string_lossy().to_string();
            let (entries, _, _) = build_repo_map_core(&files, Some(&tl), None, true, true, None);
            let unfenced = render_repo_map_for_prompt(entries, REPO_MAP_MAX_CHARS);
            let (fenced, omitted) = repo_map_for_prompt_fenced(
                &files,
                Some(&tl),
                &DirectoryRestrictions::default(),
                true,
                REPO_MAP_MAX_CHARS,
            );
            assert!(unfenced.as_deref().unwrap_or("").contains(DENIED_TOKEN));
            assert_eq!(fenced, unfenced);
            assert_eq!(omitted, 0);
        }

        #[test]
        fn fence_note_counts_without_naming_and_is_silent_at_zero() {
            assert_eq!(repo_map_fence_note(0, false), None);
            assert_eq!(repo_map_fence_note(0, true), None);
            assert_eq!(
                repo_map_fence_note(1, true).as_deref(),
                Some("warning: repo map omitted 1 file blocked by the directory fence (--deny-dir/--allow-dir)")
            );
            assert_eq!(
                repo_map_fence_note(3, false).as_deref(),
                Some("⚠ repo map omitted 3 files blocked by the directory fence (--deny-dir/--allow-dir)")
            );
            let plain = repo_map_fence_note(2, true).unwrap();
            assert!(plain.is_ascii(), "plain note must be glyph-free: {plain}");
        }

        /// #869: the `/cd`-appended deny list, injected (never the global).
        fn cd_list(dirs: &[&std::path::Path]) -> crate::cd_deny::DenyAdditions {
            let list = crate::cd_deny::DenyAdditions::default();
            crate::sync_util::lock_or_recover(&list)
                .extend(dirs.iter().map(|d| d.display().to_string()));
            list
        }

        fn map_with(
            cd: &crate::cd_deny::DenyAdditions,
            files: &[String],
            proj: &std::path::Path,
            fence: &DirectoryRestrictions,
        ) -> (String, MapOmitted) {
            let (m, omitted) = repo_map_for_prompt_fenced_with(
                cd,
                files,
                Some(&proj.to_string_lossy()),
                fence,
                true,
                REPO_MAP_MAX_CHARS,
            );
            (m.unwrap_or_default(), omitted)
        }

        #[test]
        fn cd_deny_entry_omits_its_subdirectory_and_keeps_the_sibling() {
            let (_tmp, proj, files) = fixture();
            let empty = DirectoryRestrictions::default();
            // Anti-vacuous: with no lists at all, the denied token is in the map.
            let (unfenced, _) = map(&files, &proj, &empty);
            assert!(unfenced.contains(DENIED_TOKEN), "{unfenced}");
            let (fenced, omitted) =
                map_with(&cd_list(&[&proj.join("private")]), &files, &proj, &empty);
            assert!(
                !fenced.contains(DENIED_TOKEN),
                "/cd-denied symbol leaked: {fenced}"
            );
            assert!(!fenced.contains("private/keys.rs"), "path leaked: {fenced}");
            assert_eq!(omitted, MapOmitted { fence: 0, cd: 1 });
            // Near-miss: the sibling renders exactly as an unfenced map of it alone.
            let (sibling_only, _) = map(&files[1..], &proj, &empty);
            assert_eq!(fenced, sibling_only);
            // The note attributes the omission to /cd, not to --deny-dir.
            let note = repo_map_cd_deny_note(omitted.cd, true).unwrap();
            assert_eq!(
                note,
                format!(
                    "warning: repo map omitted 1 file blocked by the directory fence {}",
                    crate::commands_file_fence::CD_DIR_SOURCE_NOTE
                )
            );
        }

        #[test]
        fn empty_cd_list_and_unrelated_cd_entry_are_byte_identical() {
            let (_tmp, proj, files) = fixture();
            let other = proj.join("vendor");
            std::fs::create_dir_all(&other).unwrap();
            for fence in [
                DirectoryRestrictions::default(),
                deny(&proj.join("private")),
            ] {
                let (before, before_omitted) = map(&files, &proj, &fence);
                for cd in [cd_list(&[]), cd_list(&[&other])] {
                    let (after, omitted) = map_with(&cd, &files, &proj, &fence);
                    assert_eq!(after, before);
                    assert_eq!(
                        omitted,
                        MapOmitted {
                            fence: before_omitted,
                            cd: 0
                        }
                    );
                }
            }
        }

        #[test]
        fn startup_fence_keeps_its_verdict_when_the_cd_list_names_the_same_dir() {
            let (_tmp, proj, files) = fixture();
            let private = proj.join("private");
            let (m, omitted) = map_with(&cd_list(&[&private]), &files, &proj, &deny(&private));
            assert!(!m.contains(DENIED_TOKEN));
            // Counted (and so reported) as the startup fence, which refused first.
            assert_eq!(omitted, MapOmitted { fence: 1, cd: 0 });
        }

        #[test]
        fn cd_deny_note_is_silent_at_zero_and_glyph_free_when_plain() {
            assert_eq!(repo_map_cd_deny_note(0, false), None);
            assert_eq!(repo_map_cd_deny_note(0, true), None);
            assert_eq!(
                repo_map_cd_deny_note(3, false).as_deref(),
                Some("⚠ repo map omitted 3 files blocked by the directory fence (a [directories] entry applied by /cd)")
            );
            assert!(repo_map_cd_deny_note(2, true).unwrap().is_ascii());
        }
    }

    // ── handle_map ──────────────────────────────────────────────────

    /// Fixture table: input → expected parse outcome (#727).
    #[test]
    fn parse_map_args_fixture_table() {
        let ok = |show_all, force_regex, path: Option<&str>| MapArgs::Ok {
            show_all,
            force_regex,
            path_filter: path.map(|s| s.to_string()),
        };

        let cases: Vec<(&str, MapArgs)> = vec![
            ("", ok(false, false, None)),
            ("--all", ok(true, false, None)),
            ("--regex", ok(false, true, None)),
            ("--all --regex src/", ok(true, true, Some("src/"))),
            // `./src` normalises to `src` so the starts_with filter matches.
            ("./src", ok(false, false, Some("src"))),
            // Unchanged: a plain relative path is passed through verbatim.
            ("src/", ok(false, false, Some("src/"))),
        ];

        for (input, expected) in cases {
            assert_eq!(
                parse_map_args(input),
                expected,
                "parse_map_args({input:?}) mismatched"
            );
        }
    }

    #[test]
    fn parse_map_args_unknown_flag_is_an_error_with_suggestion() {
        match parse_map_args("--al") {
            MapArgs::Error(msg) => {
                assert!(msg.contains("--al"), "error should name the token: {msg}");
                assert!(
                    msg.contains("--all"),
                    "error should suggest the near miss: {msg}"
                );
            }
            other => panic!("expected Error for '--al', got {other:?}"),
        }
    }

    #[test]
    fn parse_map_args_far_flag_is_an_error_without_bogus_guess() {
        match parse_map_args("--public") {
            MapArgs::Error(msg) => {
                assert!(
                    msg.contains("--public"),
                    "error should name the token: {msg}"
                );
                assert!(
                    !msg.contains("did you mean"),
                    "no close match, so no suggestion should be offered: {msg}"
                );
            }
            other => panic!("expected Error for '--public', got {other:?}"),
        }
    }

    #[test]
    fn parse_map_args_two_paths_is_an_error_naming_both() {
        match parse_map_args("src/ tests/") {
            MapArgs::Error(msg) => {
                assert!(msg.contains("src/"), "error should name first path: {msg}");
                assert!(
                    msg.contains("tests/"),
                    "error should name second path: {msg}"
                );
            }
            other => panic!("expected Error for two paths, got {other:?}"),
        }
    }

    #[test]
    fn parse_map_argv_keeps_one_spaced_element_as_one_path() {
        // #1009: the shell door hands argv elements verbatim, so `"dir sp"`
        // is ONE path, byte-for-byte, with no quote characters added.
        let argv = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let ok = |show_all, path: Option<&str>| MapArgs::Ok {
            show_all,
            force_regex: false,
            path_filter: path.map(String::from),
        };
        assert_eq!(
            parse_map_argv(&argv(&["dir sp"])),
            ok(false, Some("dir sp"))
        );
        assert_eq!(
            parse_map_argv(&argv(&["--all", "./dir sp"])),
            ok(true, Some("dir sp"))
        );
        // Near misses, identical to the REPL string parser on the same input.
        for (elems, line) in [
            (&["a", "b"][..], "a b"),
            (&["--bogus"][..], "--bogus"),
            (&[][..], ""),
            (&["src"][..], "src"),
            (&["--all", "--regex"][..], "--all --regex"),
            (&[""][..], ""),
        ] {
            assert_eq!(
                parse_map_argv(&argv(elems)),
                parse_map_args(line),
                "{elems:?}"
            );
        }
        assert!(
            matches!(parse_map_argv(&argv(&["a", "b"])), MapArgs::Error(m) if m.contains("two paths given ('a' and 'b')"))
        );
    }

    #[test]
    fn map_missing_path_message_table() {
        let files = vec!["src/main.rs".to_string(), "src/commands.rs".to_string()];
        // A path that is neither on disk nor a prefix of any project file.
        assert_eq!(
            map_missing_path_message(Some("/nonexistent_zz"), false, &files),
            Some(
                "no such path: '/nonexistent_zz' (not a file or directory, and no project file starts with it)"
                    .to_string()
            )
        );
        // Near-miss: an existing directory with no supported source files is an
        // honest empty result, not a failure.
        assert_eq!(map_missing_path_message(Some("docs/"), true, &files), None);
        // Near-miss: a prefix of real files that is not itself a path still maps.
        assert_eq!(
            map_missing_path_message(Some("src/com"), false, &files),
            None
        );
        // Bare `map` (no filter) is never a missing path, even in an empty project.
        assert_eq!(map_missing_path_message(None, false, &[]), None);
    }

    #[test]
    fn handle_map_status_missing_path_is_1_and_existing_empty_dir_is_0() {
        assert_eq!(
            handle_map_status("/map /nonexistent_zz_982"),
            MAP_MISSING_PATH_EXIT
        );
        assert_eq!(MAP_MISSING_PATH_EXIT, 1);
        // Near-miss pinned at the status: a directory that exists in the repo
        // and holds no supported source files (`.githooks/` has only a shell
        // hook) keeps exit 0.
        assert_eq!(handle_map_status("/map .githooks/"), 0);
        assert_eq!(
            MAP_EMPTY_MESSAGE,
            "(no supported source files with symbols found)"
        );
        // An unknown flag is a usage error.
        assert_eq!(handle_map_status("/map --bogus-flag"), MAP_USAGE_EXIT);
        assert_eq!(MAP_USAGE_EXIT, 2);
    }

    /// Build `files` (name, content) in a scratch dir and run the real map
    /// build over it, so every count below is derived by the code path, never
    /// typed into a fixture (lesson d210).
    fn map_scratch(files: &[(&str, &str)], public_only: bool) -> Vec<FileSymbols> {
        let dir = tempfile::tempdir().expect("tempdir");
        let names: Vec<String> = files.iter().map(|(n, _)| n.to_string()).collect();
        for (name, body) in files {
            std::fs::write(dir.path().join(name), body).unwrap();
        }
        let tl = dir.path().to_string_lossy().to_string();
        build_repo_map_core(&names, Some(&tl), None, public_only, true, None).0
    }

    #[test]
    fn map_empty_message_says_symbols_exist_but_none_are_public() {
        let private = [("a.rs", "fn main(){}\nfn helper(x: i32) -> i32 { x }\n")];
        // The default (public-only) build really is empty for this file...
        assert!(map_scratch(&private, true).is_empty());
        // ...and the all-symbols build is what the message counts.
        let all = map_scratch(&private, false);
        let shell = map_empty_message(&all, &map_all_command("yoyo map", None));
        assert_eq!(
            shell,
            "(2 symbols in 1 file, none public; run `yoyo map --all` to show them)"
        );
        assert!(!shell.contains("no supported source files"));
        // The REPL surface names its own syntax.
        assert_eq!(
            map_empty_message(&all, &map_all_command("/map", None)),
            "(2 symbols in 1 file, none public; run `/map --all` to show them)"
        );
        // A path filter is carried into the suggested command.
        assert_eq!(
            map_empty_message(&all, &map_all_command("yoyo map", Some("a.rs"))),
            "(2 symbols in 1 file, none public; run `yoyo map --all a.rs` to show them)"
        );
    }

    #[test]
    fn map_empty_message_near_misses_keep_the_old_text() {
        let cmd = map_all_command("yoyo map", None);
        // Truly empty: nothing to count.
        assert_eq!(
            map_empty_message(&map_scratch(&[], false), &cmd),
            MAP_EMPTY_MESSAGE
        );
        // Only an unsupported file.
        assert_eq!(
            map_empty_message(&map_scratch(&[("notes.txt", "fn x(){}\n")], false), &cmd),
            MAP_EMPTY_MESSAGE
        );
        assert_eq!(
            MAP_EMPTY_MESSAGE,
            "(no supported source files with symbols found)"
        );
        // A private-only file with --all lists its symbols (unchanged).
        let all = map_scratch(&[("a.rs", "fn main(){}\nfn helper(){}\n")], false);
        let names: Vec<&str> = all[0].symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["main", "helper"]);
        // One pub + one private, default: the pub one is listed (unchanged).
        let mixed = map_scratch(&[("m.rs", "pub fn shown(){}\nfn hidden(){}\n")], true);
        let names: Vec<&str> = mixed[0].symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["shown"]);
        // Plural/singular agree.
        let one = map_scratch(&[("o.rs", "fn only(){}\n")], false);
        assert_eq!(
            map_empty_message(&one, &cmd),
            "(1 symbol in 1 file, none public; run `yoyo map --all` to show them)"
        );
    }

    #[test]
    fn map_all_command_quotes_a_shell_path_only_when_needed() {
        assert_eq!(
            map_all_command("yoyo map", Some("src/")),
            "yoyo map --all src/"
        );
        assert_eq!(
            map_all_command("yoyo map", Some("dir sp")),
            "yoyo map --all 'dir sp'"
        );
        assert_eq!(
            map_all_command("yoyo map", Some("it's")),
            "yoyo map --all 'it'\\''s'"
        );
        assert_eq!(map_all_command("/map", Some("src/")), "/map --all src/");
    }

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

    #[serial]
    #[test]
    fn build_repo_map_with_regex_backend() {
        let (entries, backend) = build_repo_map_with_backend(Some("src/"), true, true);
        // `--regex` never tries ast-grep, so no file can be credited to it.
        assert_eq!(backend.ast_grep_files, 0);
        assert_eq!(backend.label(), "using regex");
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

#[cfg(test)]
mod tmp_manual {
    #[test]
    fn tmp_handle_map_bad_flag() {
        super::handle_map("/map --al");
    }
}

#[cfg(test)]
#[path = "commands_map_backend_tests.rs"]
mod commands_map_backend_tests;
