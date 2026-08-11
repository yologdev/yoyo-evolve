//! Config, hooks, permissions, teach, and MCP command handlers.
//!
//! Extracted from `commands.rs` (issue #260) — these are all
//! "settings/state inspection" handlers that form a coherent module.

use crate::cli::{is_verbose, AUTO_COMPACT_THRESHOLD};
use crate::commands::thinking_level_name;
use crate::format::{
    format_token_count, truncate_with_ellipsis, BOLD, DIM, GREEN, RED, RESET, YELLOW,
};
use crate::git::git_branch;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use yoagent::agent::Agent;
use yoagent::ThinkingLevel;

// ── Teach mode state ──────────────────────────────────────────────────────
// Session toggle: when enabled, a teaching instruction is prepended to
// each user message so the agent explains its reasoning as it works.

static TEACH_MODE: AtomicBool = AtomicBool::new(false);

/// Enable or disable teach mode.
pub fn set_teach_mode(enabled: bool) {
    TEACH_MODE.store(enabled, Ordering::Relaxed);
}

/// Check whether teach mode is currently active.
pub fn is_teach_mode() -> bool {
    TEACH_MODE.load(Ordering::Relaxed)
}

/// Instruction prepended to user messages when teach mode is on.
pub const TEACH_MODE_PROMPT: &str = "\
[TEACH MODE] You are in teach mode. For every change you make:
1. Explain WHY you're making the change before showing the code
2. Use clear, readable code patterns — prefer clarity over cleverness
3. Add brief comments on non-obvious lines
4. After completing a task, summarize what the user should learn from it
Keep explanations concise but educational.";

// ── Read mode ──
//
// Read-only oracle mode: the agent can read, search, list, and analyze code
// but must not write, edit, or run destructive commands. Competitive with
// Amp's "Oracle mode."

static READ_MODE: AtomicBool = AtomicBool::new(false);

/// Enable or disable read-only mode.
pub fn set_read_mode(enabled: bool) {
    READ_MODE.store(enabled, Ordering::Relaxed);
}

/// Check whether read-only mode is currently active.
pub fn is_read_mode() -> bool {
    READ_MODE.load(Ordering::Relaxed)
}

/// Instruction prepended to user messages when read mode is on.
pub const READ_MODE_PROMPT: &str = "\
READ-ONLY MODE ACTIVE. You may ONLY:\n\
- Read files (read_file)\n\
- Search code (search, bash with grep/find/cat/head/wc/rg)\n\
- List files (list_files)\n\
- Run non-destructive bash commands for analysis\n\n\
You MUST NOT:\n\
- Use write_file or edit_file\n\
- Run bash commands that modify files (rm, mv, cp, tee, sed -i, git commit, etc.)\n\
- Create, delete, or modify any file\n\n\
Focus on understanding the code and answering the user's question.";

/// Toggle or set read-only mode.
pub fn handle_read(input: &str) {
    let arg = input.trim();
    match arg {
        "on" => {
            set_read_mode(true);
            println!("{BOLD}  🔍 Read-only mode ON{RESET} — analyze but not modify\n");
        }
        "off" => {
            set_read_mode(false);
            println!("  🔍 Read-only mode OFF — full access restored\n");
        }
        "" => {
            let new_state = !is_read_mode();
            set_read_mode(new_state);
            if new_state {
                println!("{BOLD}  🔍 Read-only mode ON{RESET} — analyze but not modify\n");
            } else {
                println!("  🔍 Read-only mode OFF — full access restored\n");
            }
        }
        _ => {
            println!("  Usage: /read [on|off]\n");
        }
    }
}

// ── Architect mode ──
//
// Dual-model workflow: a strong reasoning model plans the changes (text-only,
// no tools), then a cheaper editor model implements the plan with full tool
// access. Inspired by Aider's architect mode — saves 60-80% on costs for
// complex tasks.

static ARCHITECT_MODE: AtomicBool = AtomicBool::new(false);

/// The model override for the architect (planning) phase.
/// `None` means use the current model.
static ARCHITECT_MODEL: Mutex<Option<String>> = Mutex::new(None);

/// Enable or disable architect mode, optionally setting a specific architect model.
pub fn set_architect_mode(on: bool, model: Option<String>) {
    ARCHITECT_MODE.store(on, Ordering::Relaxed);
    if let Ok(mut m) = ARCHITECT_MODEL.lock() {
        *m = if on { model } else { None };
    }
}

/// Check whether architect mode is currently active.
pub fn is_architect_mode() -> bool {
    ARCHITECT_MODE.load(Ordering::Relaxed)
}

/// Get the architect model override (if set). Returns `None` to use the current model.
pub fn architect_model() -> Option<String> {
    ARCHITECT_MODEL.lock().ok().and_then(|m| m.clone())
}

/// Explicit editor-model override for architect mode (issue #542).
/// `None` means the editor uses the same model as the architect.
static EDITOR_MODEL: Mutex<Option<String>> = Mutex::new(None);

/// Set (or clear) the explicit editor model for architect mode.
pub fn set_editor_model(model: Option<String>) {
    if let Ok(mut m) = EDITOR_MODEL.lock() {
        *m = model;
    }
}

/// Get the explicit editor model override (if set).
pub fn editor_model() -> Option<String> {
    EDITOR_MODEL.lock().ok().and_then(|m| m.clone())
}

/// Choose the editor model given the current (architect) model.
///
/// Models are named explicitly in config, never inferred (issue #542): if an
/// explicit editor model was set via `/architect <arch> <editor>` or
/// `--editor-model`, return it; otherwise the editor is the current model.
/// The old auto-downgrade map (opus → sonnet, gpt-4o → gpt-4o-mini, …) is gone
/// — inferred model IDs go stale and 404 when providers retire them.
pub fn default_editor_model(current_model: &str) -> String {
    if let Some(explicit) = editor_model() {
        return explicit;
    }
    current_model.to_string()
}

/// System prompt suffix for the architect (planning) phase.
pub const ARCHITECT_PROMPT: &str = "\
[ARCHITECT MODE] You are in architect mode. Your job is to PLAN, not implement.

Describe exactly what changes to make:
- Which files to create, modify, or delete
- What code to add, remove, or change — include specific code snippets
- The order of operations if it matters

Be specific and precise. Reference line numbers when helpful.
Do NOT use any tools. Do NOT write code to files. Just describe the plan.";

/// Handle the `/architect` command.
pub fn handle_architect(input: &str) {
    let arg = input.strip_prefix("/architect").unwrap_or("").trim();
    match arg {
        "on" => {
            set_architect_mode(true, None);
            let current = "current model";
            let editor = editor_model().unwrap_or_else(|| "same as architect model".to_string());
            eprintln!(
                "{GREEN}  ✓ architect mode: ON{RESET}\n\
                 {DIM}    architect: {current}\n\
                 {DIM}    editor: {editor}{RESET}\n"
            );
        }
        "off" => {
            set_architect_mode(false, None);
            eprintln!("{YELLOW}  ✗ architect mode: OFF{RESET}\n");
        }
        "" => {
            // Toggle
            let was_on = is_architect_mode();
            if was_on {
                set_architect_mode(false, None);
                eprintln!("{YELLOW}  ✗ architect mode: OFF{RESET}\n");
            } else {
                set_architect_mode(true, None);
                let editor =
                    editor_model().unwrap_or_else(|| "same as architect model".to_string());
                eprintln!(
                    "{GREEN}  ✓ architect mode: ON{RESET}\n\
                     {DIM}    architect: current model\n\
                     {DIM}    editor: {editor}{RESET}\n"
                );
            }
        }
        model => {
            // Enable with a specific architect model, and optionally an
            // explicit editor model as a second token (issue #542):
            //   /architect <arch-model> [editor-model]
            let mut tokens = model.split_whitespace();
            let arch = tokens.next().unwrap_or(model).to_string();
            let editor = tokens.next().map(|t| t.to_string());
            set_architect_mode(true, Some(arch.clone()));
            set_editor_model(editor.clone());
            let editor_desc = editor.unwrap_or_else(|| "same as architect model".to_string());
            eprintln!(
                "{GREEN}  ✓ architect mode: ON{RESET}\n\
                 {DIM}    architect: {arch}\n\
                 {DIM}    editor: {editor_desc}{RESET}\n"
            );
        }
    }
}

/// Format a status line for architect mode (used by /status).
pub fn architect_status(current_model: &str) -> Option<String> {
    if !is_architect_mode() {
        return None;
    }
    let arch_model = architect_model().unwrap_or_else(|| current_model.to_string());
    let editor = editor_model().unwrap_or_else(|| "same as architect model".to_string());
    Some(format!("architect: {arch_model} → editor: {editor}"))
}

// ── /config ──────────────────────────────────────────────────────────────

/// Bundled parameters for `/config` display, replacing a long argument list.
pub struct ConfigDisplay<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub base_url: &'a Option<String>,
    pub thinking: ThinkingLevel,
    pub max_tokens: Option<u32>,
    pub max_turns: Option<usize>,
    pub temperature: Option<f32>,
    pub skills: &'a yoagent::skills::SkillSet,
    pub system_prompt: &'a str,
    pub mcp_count: u32,
    pub openapi_count: u32,
    pub hook_count: usize,
    pub agent: &'a Agent,
    pub cwd: &'a str,
}

pub fn handle_config(cfg: &ConfigDisplay<'_>) {
    println!("{DIM}  Configuration:");
    println!("    provider:   {}", cfg.provider);
    println!("    model:      {}", cfg.model);
    if let Some(ref url) = cfg.base_url {
        println!("    base_url:   {url}");
    }
    println!("    thinking:   {}", thinking_level_name(cfg.thinking));
    println!(
        "    max_tokens: {}",
        cfg.max_tokens
            .map(|m| m.to_string())
            .unwrap_or_else(|| "default (8192)".to_string())
    );
    println!(
        "    max_turns:  {}",
        cfg.max_turns
            .map(|m| m.to_string())
            .unwrap_or_else(|| "default (200)".to_string())
    );
    println!(
        "    temperature: {}",
        cfg.temperature
            .map(|t| format!("{t:.1}"))
            .unwrap_or_else(|| "default".to_string())
    );
    println!(
        "    skills:     {}",
        if cfg.skills.is_empty() {
            "none".to_string()
        } else {
            format!("{} loaded", cfg.skills.len())
        }
    );
    let system_preview =
        truncate_with_ellipsis(cfg.system_prompt.lines().next().unwrap_or("(empty)"), 60);
    println!("    system:     {system_preview}");
    if cfg.mcp_count > 0 {
        println!("    mcp:        {} server(s)", cfg.mcp_count);
    }
    if cfg.openapi_count > 0 {
        println!("    openapi:    {} spec(s)", cfg.openapi_count);
    }
    if cfg.hook_count > 0 {
        println!("    hooks:      {} active", cfg.hook_count);
    }
    println!(
        "    verbose:    {}",
        if is_verbose() { "on" } else { "off" }
    );
    if let Some(branch) = git_branch() {
        println!("    git:        {branch}");
    }
    println!("    cwd:        {}", cfg.cwd);
    println!(
        "    context:    {} max tokens",
        format_token_count(crate::cli::effective_context_tokens())
    );
    println!(
        "    auto-compact: at {:.0}%",
        AUTO_COMPACT_THRESHOLD * 100.0
    );
    println!("    messages:   {}", cfg.agent.messages().len());
    println!(
        "    session:    auto-save on exit ({})",
        crate::cli::AUTO_SAVE_SESSION_PATH
    );
    println!("{RESET}");
}

// ── /config show ─────────────────────────────────────────────────────────
//
// `/config show` is the runtime config-introspection surface (Day 40,
// Crush-parity work). Unlike `/config` which shows the *agent's live
// runtime state* (model, thinking level, message count, etc.),
// `/config show` answers a different question: "what did my config
// file actually contribute to this session, and which file was it?"
//
// The split matters for debugging: when a user says "why isn't my
// override being picked up?", they need to see (a) which file was
// read and (b) the merged key=value pairs that came out of it —
// not a snapshot of in-memory runtime values that might have been
// further mutated by CLI flags, env vars, or interactive /model
// switches. Keeping the two handlers separate means `/config` stays
// a runtime mirror and `/config show` stays a file-introspection
// tool. They're complementary, not redundant.

/// Detect which on-disk config file (if any) would be loaded by
/// `cli::load_config_file()`, using the same precedence order:
/// 1. `./.yoyo.toml` (project-level)
/// 2. `~/.yoyo.toml` (home shorthand)
/// 3. `~/.config/yoyo/config.toml` (XDG user-level)
///
/// Returns the path to the first file that exists, or `None` if no
/// config file is present in any location. This is a read-only
/// introspection helper — it never reads or parses the file itself,
/// it just tells you which path would be chosen.
///
/// Kept as a separate function (rather than calling `load_config_file`
/// directly) because the existing loader is private to `cli.rs` and
/// this path-only view is all `/config show` needs. The loader path
/// and this one are unit-tested together indirectly via
/// `test_config_file_path_precedence` below.
fn detect_loaded_config_path() -> Option<std::path::PathBuf> {
    existing_config_paths().into_iter().next()
}

/// Every config file that exists on disk, highest precedence first.
///
/// Mirrors `config::load_config_file`, which loads exactly ONE file — the
/// first that exists — and does **not** merge them:
/// `./.yoyo.toml` → `~/.yoyo.toml` → `~/.config/yoyo/config.toml`.
fn existing_config_paths() -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    // Project-level: ./.yoyo.toml
    let project = std::path::PathBuf::from(".yoyo.toml");
    if project.exists() {
        found.push(project);
    }
    // Home shorthand: ~/.yoyo.toml
    if let Some(path) = crate::cli::home_config_path() {
        if path.exists() {
            found.push(path);
        }
    }
    // XDG user-level: ~/.config/yoyo/config.toml
    if let Some(path) = crate::cli::user_config_path() {
        if path.exists() {
            found.push(path);
        }
    }
    found
}

/// Decide whether a config file that was just written will actually be read.
///
/// Because loading is first-existing-file-wins (never a merge), writing to a
/// lower-precedence file while a higher-precedence one exists is a write that
/// nothing will ever read — the whole file is shadowed, not just the key.
///
/// `existing` must be the config files that exist on disk, highest precedence
/// first (see [`existing_config_paths`]). Returns the path that shadows
/// `written`, or `None` when `written` is itself the highest-precedence
/// existing file.
///
/// If `written` is not part of the precedence chain at all we return `None` and
/// make **no claim** — that's an explicit "unknown", not a quiet vote either
/// way.
fn shadowing_config_file(
    written: &std::path::Path,
    existing: &[std::path::PathBuf],
) -> Option<std::path::PathBuf> {
    let position = existing.iter().position(|p| p == written)?;
    if position == 0 {
        None
    } else {
        existing.first().cloned()
    }
}

/// The honest note printed when a `/config set` write landed in a file that a
/// higher-precedence config file shadows.
fn shadowed_write_warning(written: &std::path::Path, shadow: &std::path::Path) -> String {
    format!(
        "⚠ {} is not the config yoyo loads here — {} takes precedence and is read instead, \
so this value will not take effect in this directory (it applies wherever {} is absent).",
        written.display(),
        shadow.display(),
        shadow.display()
    )
}

/// Return `true` if a config key looks like a secret and its value
/// should be masked in any user-visible output. Matches are
/// case-insensitive substring checks against `key`, `token`, `secret`,
/// and `password`. Keep this list in sync with anything that gets
/// stored in `.yoyo.toml` as a sensitive value (e.g. API keys).
fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("key")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
}

/// Pure, testable formatter for `/config show` output. Takes the
/// already-loaded config HashMap and an optional path to the file
/// it came from, and returns a stable, human-readable block.
///
/// Secrets (keys matching `is_secret_key`) are always masked with
/// `***` — the raw value must never appear in the output, even in
/// debug builds. This is the whole point of the test below.
///
/// Keys are emitted in sorted order so the output is deterministic
/// and easy to diff across sessions. An empty HashMap with no path
/// is the "no config loaded, running on defaults" case and produces
/// a friendly one-liner rather than an empty block.
pub fn format_config_output(
    config: &std::collections::HashMap<String, String>,
    path: Option<&std::path::Path>,
) -> String {
    let mut out = String::new();
    match path {
        Some(p) => {
            out.push_str(&format!("Loaded config: {}\n", p.display()));
        }
        None => {
            out.push_str("No config file loaded — using defaults.\n");
            // Still dump whatever was passed in (for completeness),
            // but if the map is also empty we're done.
            if config.is_empty() {
                return out;
            }
        }
    }

    if config.is_empty() {
        // A path was given but the map is empty — file parsed to
        // nothing (all comments / whitespace). Note it explicitly so
        // the user knows the file was read but contributed nothing.
        out.push_str("\n  (no keys parsed from this file)\n");
        return out;
    }

    // Determine column width for pretty alignment. Cap it so a single
    // pathological key doesn't throw off everything else.
    let max_key_len = config.keys().map(|k| k.len()).max().unwrap_or(0).min(24);

    let mut keys: Vec<&String> = config.keys().collect();
    keys.sort();

    out.push('\n');
    for key in keys {
        let value = config.get(key).map(String::as_str).unwrap_or("");
        let display_value = if is_secret_key(key) {
            "***".to_string()
        } else {
            value.to_string()
        };
        out.push_str(&format!(
            "  {:<width$}  = {}\n",
            key,
            display_value,
            width = max_key_len
        ));
    }
    out
}

/// Handler for `/config show`: prints which config file was loaded
/// (if any) and the merged key-value pairs it contributed.
///
/// This is the user-facing surface; all formatting logic lives in
/// `format_config_output` so it can be unit-tested without touching
/// the filesystem. This handler's only jobs are (1) detect the path,
/// (2) read+parse the file via the existing `cli::parse_config_file`
/// helper, and (3) println the result inside the dim block the rest
/// of the `/config` family uses.
pub fn handle_config_show() {
    let path = detect_loaded_config_path();
    let config = match path.as_ref() {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(content) => crate::cli::parse_config_file(&content),
            Err(e) => {
                println!(
                    "{RED}  Failed to read config file {}: {e}{RESET}",
                    p.display()
                );
                return;
            }
        },
        None => std::collections::HashMap::new(),
    };
    let output = format_config_output(&config, path.as_deref());
    print!("{DIM}{output}{RESET}");
}

// ── /config edit ─────────────────────────────────────────────────────────

/// Resolve which config file to open for editing.
///
/// Resolution follows [`crate::config::load_config_file`]'s precedence, so the
/// file `/config edit` opens is the file yoyo would actually *read*:
/// 1. `./.yoyo.toml` (project-level) — only if it exists
/// 2. `~/.yoyo.toml` (home shorthand) — only if it exists
/// 3. `~/.config/yoyo/config.toml` (XDG user-level) — only if it exists
/// 4. Otherwise `~/.yoyo.toml`, the same path `/config set --global` writes
///
/// Returns the path to open, or `None` when no home directory can be
/// determined (e.g. `$HOME` unset) — never panics.
///
/// This is a pure function (no I/O side effects beyond `exists()` checks)
/// so it can be tested.
pub fn resolve_config_edit_path() -> Option<std::path::PathBuf> {
    resolve_config_edit_path_in(std::path::Path::new("."))
}

/// Like [`resolve_config_edit_path`] but searches for `.yoyo.toml` under an
/// explicit `root` directory instead of the process CWD. This avoids the need
/// for `set_current_dir` in tests (global mutable state that races across
/// parallel threads).
///
/// The thin I/O half: does the `.exists()` checks and delegates the decision to
/// the env-free [`resolve_config_edit_path_from`].
fn resolve_config_edit_path_in(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let project = root.join(".yoyo.toml");
    let home = crate::cli::home_config_path();
    let xdg = crate::cli::user_config_path();

    resolve_config_edit_path_from(
        Some(project.as_path()).filter(|p| p.exists()),
        home.as_deref().filter(|p| p.exists()),
        xdg.as_deref().filter(|p| p.exists()),
        // Where `/config set --global` would write, existing or not.
        home.as_deref(),
    )
}

/// Env-free core of [`resolve_config_edit_path_in`]. Each `Option` is `Some`
/// only when that file *exists*; `home_default` is where `--global` writes
/// (`config::write_config_value`'s non-project branch = `home_config_path()`),
/// existing or not.
///
/// The decision, made explicitly (#733): `/config edit` must open the config
/// file yoyo would actually read — so this mirrors the loader's precedence
/// rather than jumping straight to the XDG path (which the loader consults
/// *last*, and which `--global` never writes). When nothing exists yet, it
/// returns the `--global` destination, so set-then-edit agrees on one file.
fn resolve_config_edit_path_from(
    project: Option<&std::path::Path>,
    home: Option<&std::path::Path>,
    xdg: Option<&std::path::Path>,
    home_default: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    project
        .or(home)
        .or(xdg)
        .or(home_default)
        .map(|p| p.to_path_buf())
}

/// Open the config file in the user's preferred editor.
pub fn handle_config_edit() {
    let config_path = match resolve_config_edit_path() {
        Some(p) => p,
        None => {
            eprintln!("{RED}Could not determine config file path{RESET}");
            return;
        }
    };

    // Ensure parent directory exists for user-level config
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!(
                    "{RED}Failed to create config directory {}: {e}{RESET}",
                    parent.display()
                );
                return;
            }
        }
    }

    // Get editor from $EDITOR, $VISUAL, or fall back to common editors
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            if cfg!(target_os = "windows") {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        });

    println!(
        "{DIM}  Opening {} in {editor}{RESET}",
        config_path.display()
    );
    let status = std::process::Command::new(&editor)
        .arg(&config_path)
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("{GREEN}  Config saved.{RESET}");
        }
        Ok(_) => {
            eprintln!("  Editor exited with non-zero status");
        }
        Err(e) => {
            eprintln!("{RED}  Failed to open editor '{editor}': {e}{RESET}");
            eprintln!("  Set $EDITOR to your preferred editor");
        }
    }
}

// ── /config set & /config get ──────────────────────────────────────

/// Parse `/config set <key> <value> [--global]` input.
///
/// Returns `(key, value, is_global)` or an error message.
pub fn parse_config_set_args(input: &str) -> Result<(String, String, bool), String> {
    // Strip "/config set " prefix
    let rest = input
        .strip_prefix("/config set ")
        .or_else(|| input.strip_prefix("/config set"))
        .unwrap_or("")
        .trim();

    if rest.is_empty() {
        return Err("usage: /config set <key> <value> [--global]".to_string());
    }

    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 2 {
        return Err("usage: /config set <key> <value> [--global]".to_string());
    }

    let key = parts[0].to_string();
    let is_global = parts.contains(&"--global");

    // Value is everything between key and --global (or all remaining)
    let value_parts: Vec<&&str> = parts[1..].iter().filter(|p| **p != "--global").collect();

    if value_parts.is_empty() {
        return Err("usage: /config set <key> <value> [--global]".to_string());
    }

    let value = value_parts
        .iter()
        .map(|p| **p)
        .collect::<Vec<_>>()
        .join(" ");

    // #732: shell-style quotes arrive attached. Strip exactly one matching
    // layer — never unbalanced, never inner, never recursively. An inner
    // quote survives and is escaped later by `format_toml_value`.
    let value = strip_one_quote_layer(&value);

    Ok((key, value, is_global))
}

/// Remove exactly one layer of matching surrounding `"` or `'`.
///
/// Uses `chars()` rather than byte slicing — `&s[1..s.len() - 1]` panics on a
/// multi-byte first character (CLAUDE.md rule, #250).
fn strip_one_quote_layer(value: &str) -> String {
    let mut chars = value.chars();
    let (Some(first), Some(last)) = (chars.next(), chars.next_back()) else {
        // Fewer than 2 chars: nothing can be a layer.
        return value.to_string();
    };
    if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
        chars.as_str().to_string()
    } else {
        value.to_string()
    }
}

/// Handle `/config set <key> <value> [--global]`.
///
/// Validates the key/value, writes to the config file, and updates the
/// live `AgentConfig` so the change takes effect immediately within the
/// current session.
pub fn handle_config_set(input: &str, agent_config: &mut crate::AgentConfig, agent: &mut Agent) {
    let (key, value, is_global) = match parse_config_set_args(input) {
        Ok(parsed) => parsed,
        Err(msg) => {
            println!("{YELLOW}  {msg}{RESET}");
            println!("{DIM}  settable keys: {}{RESET}", settable_keys_list());
            return;
        }
    };

    // Validate the value for this key
    let canonical = match crate::config::validate_config_value(&key, &value) {
        Ok(v) => v,
        Err(msg) => {
            println!("{RED}  {msg}{RESET}");
            return;
        }
    };

    // Write to disk
    let project_local = !is_global;
    match crate::config::write_config_value(&key, &canonical, project_local) {
        Ok(path) => {
            println!(
                "{GREEN}  ✓ Set {key} = {canonical} in {}{RESET}",
                path.display()
            );
            // The write succeeding is the container, not the payload: yoyo
            // loads exactly one config file (first existing wins, no merge),
            // so a write into a shadowed file is a write nothing will read.
            // Say so instead of letting the green checkmark imply effect.
            if let Some(shadow) = shadowing_config_file(&path, &existing_config_paths()) {
                println!(
                    "{YELLOW}  {}{RESET}",
                    shadowed_write_warning(&path, &shadow)
                );
            }
        }
        Err(msg) => {
            println!("{RED}  {msg}{RESET}");
            return;
        }
    }

    // Apply to live runtime so it takes effect immediately
    apply_config_to_runtime(&key, &canonical, agent_config, agent);
}

/// Apply a validated config key/value to the live runtime state.
fn apply_config_to_runtime(
    key: &str,
    value: &str,
    agent_config: &mut crate::AgentConfig,
    agent: &mut Agent,
) {
    match key {
        "model" => {
            agent_config.model = value.to_string();
            // yoagent 0.13: Agent::set_model swaps the model id (and re-resolves
            // the provider from the config's protocol when it wasn't set
            // explicitly) WITHOUT touching self.messages — conversation history
            // is preserved by construction. This retires the old
            // save_messages → build_agent → restore_messages dance for a plain
            // model switch (#597 step 2). Build the ModelConfig the same way
            // build_agent does: provider + new model + base_url.
            let cfg = crate::agent_builder::create_model_config(
                &agent_config.provider,
                &agent_config.model,
                agent_config.base_url.as_deref(),
            );
            agent.set_model(cfg);
        }
        "provider" => {
            crate::commands::handle_provider_switch(value, agent_config, agent);
        }
        "thinking" => {
            let level = crate::cli::parse_thinking_level(value);
            agent_config.thinking = level;
            let saved = match agent.save_messages() {
                Ok(json) => Some(json),
                Err(e) => {
                    eprintln!("{DIM}  ⚠ could not preserve conversation: {e}{RESET}");
                    None
                }
            };
            *agent = agent_config.build_agent();
            if let Some(json) = saved {
                if let Err(e) = agent.restore_messages(&json) {
                    eprintln!("{DIM}  ⚠ could not restore conversation: {e}{RESET}");
                }
            }
        }
        "temperature" => {
            if let Ok(t) = value.parse::<f32>() {
                agent_config.temperature = Some(t);
            }
        }
        "max_tokens" => {
            if let Ok(n) = value.parse::<u32>() {
                agent_config.max_tokens = Some(n);
            }
        }
        "max_turns" => {
            if let Ok(n) = value.parse::<usize>() {
                agent_config.max_turns = Some(n);
            }
        }
        _ => {}
    }
}

/// Handle `/config get <key>`.
///
/// Shows the current runtime value for a single config key.
pub fn handle_config_get(input: &str) {
    let key = input
        .strip_prefix("/config get ")
        .or_else(|| input.strip_prefix("/config get"))
        .unwrap_or("")
        .trim();

    if key.is_empty() {
        println!("{YELLOW}  usage: /config get <key>{RESET}");
        println!("{DIM}  settable keys: {}{RESET}", settable_keys_list());
        return;
    }

    // Read from the detected config file
    let path = detect_loaded_config_path();
    let config = match path.as_ref() {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(content) => crate::cli::parse_config_file(&content),
            Err(_) => std::collections::HashMap::new(),
        },
        None => std::collections::HashMap::new(),
    };

    match config.get(key) {
        Some(value) => {
            let display = if is_secret_key(key) {
                "***".to_string()
            } else {
                value.clone()
            };
            let source = path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "defaults".to_string());
            println!("{DIM}  {key} = {display}  ({source}){RESET}");
        }
        None => {
            println!("{DIM}  {key} is not set in config file (using default){RESET}");
        }
    }
}

/// Helper: comma-separated list of settable key names.
fn settable_keys_list() -> String {
    crate::config::SETTABLE_KEYS
        .iter()
        .map(|(k, _)| *k)
        .collect::<Vec<_>>()
        .join(", ")
}

// ── /hooks ───────────────────────────────────────────────────────────────

pub fn handle_hooks(hooks: &[crate::hooks::ShellHook]) {
    if hooks.is_empty() {
        println!("{DIM}  No hooks configured.");
        println!();
        println!("  Add hooks to .yoyo.toml:");
        println!();
        println!("    # Pre-hook: runs before every bash tool call");
        println!("    hooks.pre.bash = \"echo 'About to run bash'\"");
        println!();
        println!("    # Post-hook: runs after every tool call (wildcard)");
        println!("    hooks.post.* = \"echo 'Tool finished'\"");
        println!();
        println!("  Pre-hooks that exit non-zero block the tool.");
        println!("  Post-hooks always pass through the tool output.");
        println!("  All hooks have a 5-second timeout.{RESET}");
        return;
    }

    println!("{DIM}  Active hooks ({}):", hooks.len());
    println!();
    for hook in hooks {
        let phase = match hook.phase {
            crate::hooks::HookPhase::Pre => "pre",
            crate::hooks::HookPhase::Post => "post",
        };
        println!(
            "    {BOLD}{}{RESET}{DIM}  ({}, pattern: {})",
            hook.name, phase, hook.tool_pattern
        );
        println!("      command: {}", hook.command);
    }
    println!("{RESET}");
}

// ── /permissions ─────────────────────────────────────────────────────────

pub fn handle_permissions(
    auto_approve: bool,
    permissions: &crate::cli::PermissionConfig,
    dir_restrictions: &crate::cli::DirectoryRestrictions,
) {
    println!("{DIM}  Security Configuration:\n");

    // Auto-approve status
    if auto_approve {
        println!("    {YELLOW}⚠ Auto-approve: ON{RESET}{DIM} (--yes flag active)");
        println!("      All tool operations run without confirmation{RESET}");
    } else {
        println!("    {GREEN}✓ Confirmation: required{RESET}");
        println!("    {DIM}  Tools will prompt before write/edit/bash operations{RESET}");
    }
    println!();

    // Bash command permissions
    if permissions.is_empty() {
        println!("    Command patterns: none configured");
    } else {
        if !permissions.allow.is_empty() {
            println!("    {GREEN}Allow patterns:{RESET}");
            for pat in &permissions.allow {
                println!("      {GREEN}✓{RESET} {pat}");
            }
        }
        if !permissions.deny.is_empty() {
            println!("    {RED}Deny patterns:{RESET}");
            for pat in &permissions.deny {
                println!("      {RED}✗{RESET} {pat}");
            }
        }
    }
    println!();

    // Directory restrictions
    if dir_restrictions.is_empty() {
        println!("    Directory restrictions: none (full filesystem access)");
    } else {
        if !dir_restrictions.allow.is_empty() {
            println!("    {GREEN}Allowed directories:{RESET}");
            for dir in &dir_restrictions.allow {
                println!("      {GREEN}✓{RESET} {dir}");
            }
        }
        if !dir_restrictions.deny.is_empty() {
            println!("    {RED}Denied directories:{RESET}");
            for dir in &dir_restrictions.deny {
                println!("      {RED}✗{RESET} {dir}");
            }
        }
    }
    println!();

    // Quick reference
    println!(
        "    {DIM}Configure with: --allow <pat>, --deny <pat>, --allow-dir <d>, --deny-dir <d>"
    );
    println!("    Or in .yoyo.toml: allow = [...], deny = [...]{RESET}\n");
}

/// Toggle teach mode on/off. When active, yoyo explains its reasoning as it works.
pub fn handle_teach(input: &str) {
    let arg = input.strip_prefix("/teach").unwrap_or("").trim();
    match arg {
        "on" => {
            set_teach_mode(true);
            println!("{GREEN}  🎓 Teach mode enabled — yoyo will explain its reasoning as it works{RESET}\n");
        }
        "off" => {
            set_teach_mode(false);
            println!("{DIM}  Teach mode disabled{RESET}\n");
        }
        "" => {
            // Toggle
            let new_state = !is_teach_mode();
            set_teach_mode(new_state);
            if new_state {
                println!("{GREEN}  🎓 Teach mode enabled — yoyo will explain its reasoning as it works{RESET}\n");
            } else {
                println!("{DIM}  Teach mode disabled{RESET}\n");
            }
        }
        _ => {
            println!("{DIM}  usage: /teach [on|off]");
            println!("  Toggle teach mode. When active, yoyo explains its reasoning as it works.{RESET}\n");
        }
    }
}

// ── Effort level command ──

/// Handle the `/effort` REPL command.
pub fn handle_effort(input: &str) {
    use crate::cli_config::{effort_level, set_effort_level, EffortLevel};

    let arg = input.strip_prefix("/effort").unwrap_or("").trim();
    if arg.is_empty() {
        let level = effort_level();
        let icon = match level {
            EffortLevel::Low => "⚡",
            EffortLevel::Medium => "⚖️",
            EffortLevel::High => "🔬",
        };
        println!("{BOLD}  {icon} Effort level: {}{RESET}\n", level.label());
        return;
    }
    match arg {
        "low" | "lo" | "l" => {
            set_effort_level(EffortLevel::Low);
            println!("{GREEN}  ⚡ Effort: low{RESET} — concise answers, skip deep analysis\n");
        }
        "medium" | "med" | "m" | "default" => {
            set_effort_level(EffortLevel::Medium);
            println!("{GREEN}  ⚖️  Effort: medium{RESET} — balanced (default)\n");
        }
        "high" | "hi" | "h" => {
            set_effort_level(EffortLevel::High);
            println!("{GREEN}  🔬 Effort: high{RESET} — thorough analysis, explore alternatives\n");
        }
        _ => {
            println!("{DIM}  usage: /effort [low|medium|high]");
            println!("  Set how much work yoyo puts into each response.");
            println!("    low    — concise, skip deep analysis");
            println!("    medium — balanced (default)");
            println!("    high   — thorough, explore alternatives{RESET}\n");
        }
    }
}

/// Build the `/mcp help` text. Extracted as a pure function so tests can
/// assert on its contents (e.g. to guard against the stale "coming soon"
/// string returning, or server-filesystem sneaking back in as the primary
/// example — it collides with yoyo's read_file/write_file builtins and is
/// skipped at startup).
pub(crate) fn mcp_help_text() -> String {
    // server-fetch is the primary example because it exposes a single `fetch`
    // tool that does NOT collide with any name in BUILTIN_TOOL_NAMES. Do not
    // replace with server-filesystem — see the Day 39 collision guard.
    let mut s = String::new();
    s.push_str("  MCP (Model Context Protocol) Server Configuration\n");
    s.push('\n');
    s.push_str("  Add MCP servers to .yoyo.toml or ~/.config/yoyo/config.toml:\n");
    s.push('\n');
    s.push_str("  # Structured format (recommended):\n");
    s.push_str("  [mcp_servers.fetch]\n");
    s.push_str("  command = \"npx\"\n");
    s.push_str("  args = [\"-y\", \"@modelcontextprotocol/server-fetch\"]\n");
    s.push('\n');
    s.push_str("  [mcp_servers.postgres]\n");
    s.push_str("  command = \"npx\"\n");
    s.push_str("  args = [\"-y\", \"@modelcontextprotocol/server-postgres\"]\n");
    s.push_str("  env = { DATABASE_URL = \"postgresql://localhost/mydb\" }\n");
    s.push('\n');
    s.push_str("  # Simple format (legacy):\n");
    s.push_str("  mcp = [\"npx -y @modelcontextprotocol/server-fetch\"]\n");
    s.push('\n');
    s.push_str("  Or pass via CLI:\n");
    s.push_str("  yoyo --mcp \"npx -y @modelcontextprotocol/server-fetch\"\n");
    s.push('\n');
    s.push_str("  Note: @modelcontextprotocol/server-filesystem exposes read_file and\n");
    s.push_str("  write_file tools which collide with yoyo's builtins — yoyo skips any\n");
    s.push_str("  server whose tool names collide (see CLAUDE.md → \"MCP gotchas\").\n");
    s.push_str("  Prefer server-fetch, server-memory, or server-sequential-thinking.\n");
    s.push('\n');
    s.push_str("  Subcommands:\n");
    s.push_str("    /mcp         List configured MCP servers\n");
    s.push_str("    /mcp list    List configured MCP servers\n");
    s.push_str("    /mcp help    Show this help\n");
    s
}

/// Build the "configured but not connected" status message shown by
/// `/mcp list` when servers are configured but zero managed to connect.
/// Pure function so tests can assert it never contains "coming soon" again.
pub(crate) fn mcp_not_connected_message(total: usize) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "  {total} server(s) configured but none connected.\n"
    ));
    s.push('\n');
    s.push_str("  Common causes:\n");
    s.push_str("    • Tool name collision with a yoyo builtin. For example,\n");
    s.push_str("      @modelcontextprotocol/server-filesystem exposes read_file and\n");
    s.push_str("      write_file which collide — such servers are skipped at startup.\n");
    s.push_str("      Check stderr for a \"skipping MCP server\" warning.\n");
    s.push_str("    • Server failed to spawn (bad command path or args in your config).\n");
    s.push('\n');
    s.push_str("  See CLAUDE.md → \"MCP gotchas\" for the full list of reserved tool names.\n");
    s
}

/// Handle the `/mcp` command: list configured MCP servers and show help.
pub fn handle_mcp(
    input: &str,
    cli_servers: &[String],
    server_configs: &[crate::cli::McpServerConfig],
    mcp_count: u32,
) {
    let arg = input.strip_prefix("/mcp").unwrap_or("").trim();

    match arg {
        "help" => {
            println!("{DIM}{}{RESET}", mcp_help_text());
        }
        "" | "list" => {
            let has_cli = !cli_servers.is_empty();
            let has_configs = !server_configs.is_empty();

            if !has_cli && !has_configs {
                println!("{DIM}  No MCP servers configured.");
                println!();
                println!("  Add servers to .yoyo.toml:");
                println!("    [mcp_servers.myserver]");
                println!("    command = \"npx\"");
                println!("    args = [\"-y\", \"@modelcontextprotocol/server-fetch\"]");
                println!();
                println!("  See /mcp help for more details.{RESET}\n");
                return;
            }

            println!("{DIM}  MCP Servers:");

            // List structured configs first
            for cfg in server_configs {
                let full_cmd = if cfg.args.is_empty() {
                    cfg.command.clone()
                } else {
                    format!("{} {}", cfg.command, cfg.args.join(" "))
                };
                println!("    {:<14}{}", cfg.name, full_cmd);
            }

            // List CLI --mcp servers
            for cmd in cli_servers {
                // Use the command name (first word) as an identifier
                let label = cmd.split_whitespace().next().unwrap_or("unknown");
                println!("    {:<14}{}", label, cmd);
            }

            let total = cli_servers.len() + server_configs.len();
            println!();
            if mcp_count > 0 {
                println!(
                    "  {} server(s) configured, {} connected{RESET}\n",
                    total, mcp_count
                );
            } else {
                println!("{}{RESET}", mcp_not_connected_message(total));
            }
        }
        _ => {
            println!("{DIM}  Unknown /mcp subcommand: {arg}");
            println!("  Usage: /mcp [list|help]{RESET}\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
    use serial_test::serial;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn test_format_config_masks_secret_values() {
        let mut config = HashMap::new();
        let raw_key = "sk-ant-super-secret-do-not-leak-12345";
        config.insert("anthropic_api_key".to_string(), raw_key.to_string());
        config.insert("model".to_string(), "claude-sonnet-4-6".to_string());

        let path = PathBuf::from("/fake/path/.yoyo.toml");
        let out = format_config_output(&config, Some(&path));

        // The raw secret value must never appear in the output.
        assert!(
            !out.contains(raw_key),
            "raw secret leaked into /config show output:\n{out}"
        );
        // The mask must appear so the user can see the key exists.
        assert!(
            out.contains("***"),
            "expected masked placeholder in output:\n{out}"
        );
        // Non-secret keys should be visible as-is.
        assert!(
            out.contains("claude-sonnet-4-6"),
            "non-secret value should be visible:\n{out}"
        );
        // The loaded path should be named.
        assert!(
            out.contains("/fake/path/.yoyo.toml"),
            "loaded config path should be shown:\n{out}"
        );
    }

    #[test]
    fn test_format_config_no_file_loaded() {
        let config: HashMap<String, String> = HashMap::new();
        let out = format_config_output(&config, None);

        // Must say something clear about the no-config case.
        assert!(
            out.to_lowercase().contains("no config file loaded"),
            "expected 'no config file loaded' message, got:\n{out}"
        );
        // Must not crash and must not print stale path markers.
        assert!(
            !out.contains("Loaded config:"),
            "should not claim a config was loaded:\n{out}"
        );
    }

    #[test]
    fn test_is_secret_key_matches_common_patterns() {
        // Positive — all of these should be masked.
        assert!(is_secret_key("anthropic_api_key"));
        assert!(is_secret_key("API_KEY"));
        assert!(is_secret_key("openai_token"));
        assert!(is_secret_key("client_secret"));
        assert!(is_secret_key("db_password"));
        assert!(is_secret_key("AccessToken"));

        // Negative — ordinary config keys should pass through.
        assert!(!is_secret_key("model"));
        assert!(!is_secret_key("provider"));
        assert!(!is_secret_key("thinking"));
        assert!(!is_secret_key("temperature"));
    }

    #[test]
    fn test_format_config_sorts_keys_deterministically() {
        let mut config = HashMap::new();
        config.insert("zebra".to_string(), "z".to_string());
        config.insert("alpha".to_string(), "a".to_string());
        config.insert("mike".to_string(), "m".to_string());
        let path = PathBuf::from(".yoyo.toml");
        let out = format_config_output(&config, Some(&path));

        let alpha_pos = out.find("alpha").expect("alpha should appear");
        let mike_pos = out.find("mike").expect("mike should appear");
        let zebra_pos = out.find("zebra").expect("zebra should appear");
        assert!(
            alpha_pos < mike_pos && mike_pos < zebra_pos,
            "keys should be sorted alphabetically:\n{out}"
        );
    }

    #[test]
    fn test_hooks_command_recognized() {
        assert!(!is_unknown_command("/hooks"));
        assert!(
            KNOWN_COMMANDS.contains(&"/hooks"),
            "/hooks should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_handle_hooks_empty() {
        // Should not panic with empty hooks
        handle_hooks(&[]);
    }

    #[test]
    fn test_handle_hooks_with_hooks() {
        use crate::hooks::{HookPhase, ShellHook};
        let hooks = vec![
            ShellHook {
                name: "pre:bash".to_string(),
                phase: HookPhase::Pre,
                tool_pattern: "bash".to_string(),
                command: "echo before".to_string(),
            },
            ShellHook {
                name: "post:*".to_string(),
                phase: HookPhase::Post,
                tool_pattern: "*".to_string(),
                command: "echo after".to_string(),
            },
        ];
        // Should not panic with hooks present
        handle_hooks(&hooks);
    }

    #[test]
    #[serial]
    fn test_teach_mode_default_off() {
        // Reset to known state (tests may run in any order)
        set_teach_mode(false);
        assert!(!is_teach_mode());
    }

    #[test]
    #[serial]
    fn test_teach_mode_toggle() {
        set_teach_mode(false);
        assert!(!is_teach_mode());
        set_teach_mode(true);
        assert!(is_teach_mode());
        set_teach_mode(false);
        assert!(!is_teach_mode());
    }

    #[test]
    fn test_teach_known_command() {
        assert!(KNOWN_COMMANDS.contains(&"/teach"));
    }

    #[test]
    fn test_teach_mode_prompt_not_empty() {
        assert!(!TEACH_MODE_PROMPT.is_empty());
        assert!(TEACH_MODE_PROMPT.contains("TEACH MODE"));
    }

    #[test]
    fn test_teach_in_help_text() {
        let text = crate::help::help_text();
        assert!(
            text.contains("/teach"),
            "help text should list the /teach command"
        );
    }

    #[test]
    fn test_teach_command_help_exists() {
        let help = crate::help::command_help("teach");
        assert!(help.is_some(), "/help teach should have detailed help");
        let help_text = help.unwrap();
        assert!(help_text.contains("teach mode"));
    }

    #[test]
    fn test_teach_short_description_exists() {
        let desc = crate::help::command_short_description("teach");
        assert!(desc.is_some(), "teach should have a short description");
    }

    #[test]
    fn test_mcp_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/mcp"),
            "/mcp should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_mcp_short_description_exists() {
        let desc = crate::help::command_short_description("mcp");
        assert!(desc.is_some(), "mcp should have a short description");
    }

    #[test]
    fn test_handle_mcp_no_servers() {
        // Should not panic with empty server lists
        handle_mcp("/mcp", &[], &[], 0);
        handle_mcp("/mcp list", &[], &[], 0);
        handle_mcp("/mcp help", &[], &[], 0);
    }

    #[test]
    fn test_handle_mcp_with_configs() {
        use crate::cli::McpServerConfig;
        let configs = vec![McpServerConfig {
            name: "filesystem".to_string(),
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
            ],
            env: vec![],
        }];
        // Should not panic
        handle_mcp("/mcp", &[], &configs, 0);
        handle_mcp("/mcp list", &[], &configs, 1);
    }

    #[test]
    fn test_handle_mcp_unknown_subcommand() {
        // Should not panic on unknown subcommand
        handle_mcp("/mcp foobar", &[], &[], 0);
    }

    // --- Regression: stale "coming soon" string and server-filesystem as
    // --- primary example (Day 40). MCP protocol support shipped on Day 39;
    // --- anything in /mcp help or /mcp list that still says "coming soon"
    // --- is an outright lie to the user, and recommending server-filesystem
    // --- as the first example sends them straight into the collision guard.

    #[test]
    fn test_mcp_help_text_no_coming_soon() {
        let help = mcp_help_text();
        assert!(
            !help.contains("coming soon"),
            "/mcp help must not claim MCP support is 'coming soon' — it shipped Day 39.\nGot:\n{help}"
        );
    }

    #[test]
    fn test_mcp_not_connected_message_no_coming_soon() {
        let msg = mcp_not_connected_message(2);
        assert!(
            !msg.contains("coming soon"),
            "/mcp list 'not connected' message must not say 'coming soon'.\nGot:\n{msg}"
        );
        // Positive assertion: the replacement must actually explain WHY.
        assert!(
            msg.contains("collision") || msg.contains("collide"),
            "not-connected message should mention the collision guard as a likely cause.\nGot:\n{msg}"
        );
    }

    #[test]
    fn test_mcp_help_primary_example_is_not_filesystem() {
        // The help text may still MENTION server-filesystem (annotated with
        // the collision warning), but the primary example — the first
        // [mcp_servers.X] block — must not be filesystem, because the
        // Day 39 collision guard refuses to connect to it.
        let help = mcp_help_text();
        let first_block_start = help
            .find("[mcp_servers.")
            .expect("help text should contain at least one [mcp_servers.X] example");
        // The first example block should not contain "server-filesystem"
        // before the next blank line. Slice from first block to end and
        // look only at the first ~5 lines.
        let tail = &help[first_block_start..];
        let first_block: String = tail.lines().take(5).collect::<Vec<_>>().join("\n");
        assert!(
            !first_block.contains("server-filesystem"),
            "primary /mcp help example must not be server-filesystem \
             (it collides with read_file/write_file and is skipped at startup).\nFirst block:\n{first_block}"
        );
    }

    #[test]
    fn test_mcp_help_mentions_collision_warning() {
        // If we leave server-filesystem in the help text at all, it must
        // be annotated with the collision warning so users know why it
        // won't work.
        let help = mcp_help_text();
        if help.contains("server-filesystem") {
            assert!(
                help.contains("collide") || help.contains("skipped"),
                "if server-filesystem is mentioned in /mcp help it must be \
                 annotated with the collision warning.\nGot:\n{help}"
            );
        }
    }

    #[test]

    fn test_permissions_command_recognized() {
        assert!(!is_unknown_command("/permissions"));
        assert!(
            KNOWN_COMMANDS.contains(&"/permissions"),
            "/permissions should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_handle_permissions_defaults() {
        // No permissions, no dir restrictions, auto_approve off
        let perms = crate::cli::PermissionConfig::default();
        let dirs = crate::cli::DirectoryRestrictions::default();
        handle_permissions(false, &perms, &dirs);
    }

    #[test]
    fn test_handle_permissions_auto_approve() {
        let perms = crate::cli::PermissionConfig::default();
        let dirs = crate::cli::DirectoryRestrictions::default();
        handle_permissions(true, &perms, &dirs);
    }

    #[test]
    fn test_handle_permissions_with_patterns() {
        let perms = crate::cli::PermissionConfig {
            allow: vec!["cargo *".to_string(), "git *".to_string()],
            deny: vec!["rm -rf *".to_string()],
        };
        let dirs = crate::cli::DirectoryRestrictions::default();
        handle_permissions(false, &perms, &dirs);
    }

    #[test]
    fn test_handle_permissions_with_dir_restrictions() {
        let perms = crate::cli::PermissionConfig::default();
        let dirs = crate::cli::DirectoryRestrictions {
            allow: vec!["/home/user/project".to_string()],
            deny: vec!["/etc".to_string(), "/usr".to_string()],
        };
        handle_permissions(false, &perms, &dirs);
    }

    #[test]
    fn test_handle_permissions_fully_configured() {
        let perms = crate::cli::PermissionConfig {
            allow: vec!["cargo *".to_string()],
            deny: vec!["rm *".to_string()],
        };
        let dirs = crate::cli::DirectoryRestrictions {
            allow: vec!["/project".to_string()],
            deny: vec!["/secret".to_string()],
        };
        handle_permissions(true, &perms, &dirs);
    }

    #[test]
    fn test_resolve_config_edit_path_prefers_project_config() {
        // When .yoyo.toml exists in the root dir, it should be returned
        let tmp = std::env::temp_dir().join("yoyo_test_config_edit");
        let _ = std::fs::create_dir_all(&tmp);
        let project_config = tmp.join(".yoyo.toml");
        std::fs::write(&project_config, "# test config\n").unwrap();

        let result = resolve_config_edit_path_in(&tmp);
        assert!(result.is_some(), "should return a path");
        let path = result.unwrap();
        assert_eq!(
            path,
            tmp.join(".yoyo.toml"),
            "should prefer project-level config"
        );

        // Clean up
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolve_config_edit_path_falls_back_to_user_config() {
        // When no project-level .yoyo.toml exists, we fall back to a home-level
        // path. Which one depends on what exists on this machine (#733), so the
        // env-dependent assertion here is deliberately weak — the exact
        // precedence is pinned env-free by
        // `test_resolve_config_edit_path_from_fixtures`.
        let tmp = std::env::temp_dir().join("yoyo_test_config_edit_fallback");
        let _ = std::fs::create_dir_all(&tmp);
        // Make sure there's no .yoyo.toml
        let _ = std::fs::remove_file(tmp.join(".yoyo.toml"));

        let result = resolve_config_edit_path_in(&tmp);
        // As long as HOME is set, we should get a path
        if std::env::var("HOME").is_ok() {
            assert!(result.is_some(), "should return a home-level config path");
            let path = result.unwrap();
            let home = crate::cli::home_config_path();
            let xdg = crate::cli::user_config_path();
            assert!(
                Some(path.clone()) == home || Some(path.clone()) == xdg,
                "should point at one of the two home-level config paths, got: {}",
                path.display()
            );
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolve_config_edit_path_from_fixtures() {
        use std::path::{Path, PathBuf};
        let project = Path::new("/repo/.yoyo.toml");
        let home = Path::new("/home/u/.yoyo.toml");
        let xdg = Path::new("/home/u/.config/yoyo/config.toml");

        // (project, home, xdg, home_default, expected, label)
        type Case<'a> = (
            Option<&'a Path>,
            Option<&'a Path>,
            Option<&'a Path>,
            Option<&'a Path>,
            Option<PathBuf>,
            &'a str,
        );
        let p = Some(project);
        let h = Some(home);
        let x = Some(xdg);
        let cases: &[Case] = &[
            (
                p,
                h,
                x,
                h,
                Some(project.into()),
                "project-level wins over all",
            ),
            (None, h, None, h, Some(home.into()), "home-only"),
            (
                None,
                None,
                x,
                h,
                Some(xdg.into()),
                "xdg-only (wizard machine)",
            ),
            (
                None,
                h,
                x,
                h,
                Some(home.into()),
                "home beats xdg — loader order",
            ),
            (
                None,
                None,
                None,
                h,
                Some(home.into()),
                "none exist — --global dest",
            ),
            (
                None,
                None,
                None,
                None,
                None,
                "no home at all — None, no panic",
            ),
        ];

        for (p, h, x, d, expected, label) in cases {
            assert_eq!(
                resolve_config_edit_path_from(*p, *h, *x, *d),
                *expected,
                "{label}"
            );
        }
    }

    #[test]
    fn test_config_edit_default_agrees_with_global_set_destination() {
        // The user-facing promise of #733, asserted at the emission point:
        // when no config file exists, `/config edit` opens exactly the file
        // `/config set --global` would create. Both halves call
        // `home_config_path()` here so they cannot drift.
        let home_default = crate::cli::home_config_path();
        let resolved = resolve_config_edit_path_from(None, None, None, home_default.as_deref());
        assert_eq!(
            resolved, home_default,
            "config edit fallback must be byte-equal to the --global write target"
        );

        if let Some(expected) = home_default {
            // And that target really is what write_config_value picks for
            // --global: it writes to `home_config_path()`, so a temp-file write
            // through the same helper round-trips at that exact path.
            assert!(
                expected.ends_with(".yoyo.toml"),
                "global writes land on ~/.yoyo.toml, got {}",
                expected.display()
            );
        }
    }

    // --- /config set argument parsing tests ---

    #[test]
    fn test_parse_config_set_args_basic() {
        let (key, value, global) =
            parse_config_set_args("/config set model claude-sonnet-4-6").unwrap();
        assert_eq!(key, "model");
        assert_eq!(value, "claude-sonnet-4-6");
        assert!(!global);
    }

    #[test]
    fn test_parse_config_set_args_with_global() {
        let (key, value, global) =
            parse_config_set_args("/config set model claude-opus-4-6 --global").unwrap();
        assert_eq!(key, "model");
        assert_eq!(value, "claude-opus-4-6");
        assert!(global);
    }

    #[test]
    fn test_parse_config_set_args_numeric() {
        let (key, value, _) = parse_config_set_args("/config set max_tokens 8192").unwrap();
        assert_eq!(key, "max_tokens");
        assert_eq!(value, "8192");
    }

    #[test]
    fn test_parse_config_set_args_empty() {
        assert!(parse_config_set_args("/config set").is_err());
        assert!(parse_config_set_args("/config set ").is_err());
    }

    #[test]
    fn test_parse_config_set_args_missing_value() {
        assert!(parse_config_set_args("/config set model").is_err());
    }

    #[test]
    fn test_parse_config_set_args_global_only_no_value() {
        // "/config set model --global" — --global is filtered out, no value remains
        assert!(parse_config_set_args("/config set model --global").is_err());
    }

    #[test]
    fn test_parse_config_set_args_strips_one_quote_layer() {
        // #732: shell-style quotes arrived attached and landed in the file as
        // `notify_command = ""notify-send done""`.
        let cases = [
            (
                r#"/config set notify_command "notify-send done""#,
                "notify-send done",
            ),
            (
                "/config set notify_command 'notify-send done'",
                "notify-send done",
            ),
            (
                "/config set notify_command notify-send done",
                "notify-send done",
            ),
            // Unbalanced: leave it alone, half 1 escapes it.
            (r#"/config set k "unbalanced"#, r#""unbalanced"#),
            (r#"/config set k unbalanced""#, r#"unbalanced""#),
            // Exactly one layer — inner quotes survive.
            (r#"/config set k ""double"""#, r#""double""#),
            (r#"/config set k "a"b""#, r#"a"b"#),
            // Mismatched delimiters are not a layer.
            (r#"/config set k "mixed'"#, r#""mixed'"#),
            // Empty quoted value.
            (r#"/config set k """#, ""),
            // A lone quote is 1 char: not a layer, must not panic.
            (r#"/config set k ""#, "\""),
            // Multi-byte first char: never byte-index (#250).
            ("/config set k → x", "→ x"),
            ("/config set k \"→ x\"", "→ x"),
        ];
        for (input, expected) in cases {
            let (_, value, _) = parse_config_set_args(input).unwrap();
            assert_eq!(value, expected, "input: {input}");
        }
    }

    #[test]
    fn test_parse_config_set_args_quoted_value_survives_to_toml() {
        // End-to-end on the promise: what the user typed is what the file
        // holds, and what the reader gives back.
        let (key, value, _) =
            parse_config_set_args(r#"/config set notify_command "notify-send \"done\"""#).unwrap();
        let line = format!("{key} = {}", crate::config::format_toml_value(&value));
        let parsed = crate::config::parse_config_file(&line);
        assert_eq!(parsed.get("notify_command"), Some(&value));
    }

    // --- architect mode tests ---

    #[test]
    #[serial]
    fn test_default_editor_model_defaults_to_main_model() {
        // No explicit override → editor is the main model, never inferred (#542)
        set_editor_model(None);
        assert_eq!(default_editor_model("claude-opus-4-6"), "claude-opus-4-6");
        assert_eq!(default_editor_model("gpt-4o"), "gpt-4o");
        assert_eq!(
            default_editor_model("anthropic.claude-sonnet-4-6"),
            "anthropic.claude-sonnet-4-6"
        );
    }

    #[test]
    #[serial]
    fn test_default_editor_model_explicit_override_wins() {
        set_editor_model(Some("my-cheap-editor".to_string()));
        // Returned regardless of the current model
        assert_eq!(default_editor_model("claude-opus-4-6"), "my-cheap-editor");
        assert_eq!(default_editor_model("gpt-4o"), "my-cheap-editor");
        // Clean up global state
        set_editor_model(None);
    }

    #[test]
    #[serial]
    fn test_architect_parse_two_tokens_sets_both_models() {
        set_architect_mode(false, None);
        set_editor_model(None);

        // Simulate `/architect claude-opus-4-6 claude-haiku-4-5`
        handle_architect("/architect claude-opus-4-6 claude-haiku-4-5");
        assert!(is_architect_mode());
        assert_eq!(architect_model().as_deref(), Some("claude-opus-4-6"));
        assert_eq!(editor_model().as_deref(), Some("claude-haiku-4-5"));

        // Clean up
        set_architect_mode(false, None);
        set_editor_model(None);
    }

    #[test]
    #[serial]
    fn test_architect_parse_one_token_leaves_editor_unset() {
        set_architect_mode(false, None);
        // Pre-set an editor to verify the one-token form clears it
        set_editor_model(Some("stale-editor".to_string()));

        handle_architect("/architect claude-opus-4-6");
        assert!(is_architect_mode());
        assert_eq!(architect_model().as_deref(), Some("claude-opus-4-6"));
        assert_eq!(editor_model(), None);

        // Clean up
        set_architect_mode(false, None);
        set_editor_model(None);
    }

    #[test]
    #[serial]
    fn test_architect_toggle_on_off() {
        // Start from a known state
        set_architect_mode(false, None);
        assert!(!is_architect_mode());

        // Toggle on
        set_architect_mode(true, None);
        assert!(is_architect_mode());

        // Toggle off
        set_architect_mode(false, None);
        assert!(!is_architect_mode());
    }

    #[test]
    #[serial]
    fn test_architect_parse_sets_model() {
        // Reset state
        set_architect_mode(false, None);

        // Simulate `/architect claude-sonnet-4-20250514`
        handle_architect("/architect claude-sonnet-4-20250514");
        assert!(is_architect_mode());
        assert_eq!(
            architect_model().as_deref(),
            Some("claude-sonnet-4-20250514")
        );

        // Clean up
        set_architect_mode(false, None);
    }

    #[test]
    #[serial]
    fn test_read_mode_default_off() {
        set_read_mode(false);
        assert!(!is_read_mode());
    }

    #[test]
    #[serial]
    fn test_read_mode_set_and_check() {
        set_read_mode(false);
        assert!(!is_read_mode());
        set_read_mode(true);
        assert!(is_read_mode());
        set_read_mode(false);
        assert!(!is_read_mode());
    }

    // === /config set --global shadowing (Day 151) ===
    //
    // yoyo loads exactly ONE config file (first existing in precedence order,
    // no merging). Writing to a lower-precedence file while a higher-precedence
    // one exists is a write nothing will ever read — so the unconditional
    // "✓ Set k = v in <path>" confirmation asserted the container (the write
    // landed) and not the payload (the setting will be honoured).

    #[test]
    fn test_shadowing_none_when_written_is_highest_precedence() {
        let project = std::path::PathBuf::from(".yoyo.toml");
        let home = std::path::PathBuf::from("/home/u/.yoyo.toml");
        let existing = vec![project.clone(), home];
        assert_eq!(shadowing_config_file(&project, &existing), None);
    }

    #[test]
    fn test_shadowing_detects_project_file_over_global_write() {
        let project = std::path::PathBuf::from(".yoyo.toml");
        let home = std::path::PathBuf::from("/home/u/.yoyo.toml");
        let existing = vec![project.clone(), home.clone()];
        assert_eq!(
            shadowing_config_file(&home, &existing),
            Some(project),
            "a --global write must be reported as shadowed by an existing project config"
        );
    }

    #[test]
    fn test_shadowing_none_for_sole_existing_file() {
        let home = std::path::PathBuf::from("/home/u/.yoyo.toml");
        let existing = vec![home.clone()];
        assert_eq!(shadowing_config_file(&home, &existing), None);
    }

    #[test]
    fn test_shadowing_makes_no_claim_for_path_outside_precedence_chain() {
        // Explicit third value (Day 144): "not part of the chain" is unknown,
        // not "shadowed" and not silently "fine by omission" — we say nothing
        // rather than let the convenient neighbour absorb it.
        let project = std::path::PathBuf::from(".yoyo.toml");
        let elsewhere = std::path::PathBuf::from("/tmp/some-other.toml");
        let existing = vec![project];
        assert_eq!(shadowing_config_file(&elsewhere, &existing), None);
    }

    #[test]
    fn test_shadowed_write_warning_names_both_files() {
        let msg = shadowed_write_warning(
            &std::path::PathBuf::from("/home/u/.yoyo.toml"),
            &std::path::PathBuf::from(".yoyo.toml"),
        );
        assert!(
            msg.contains("/home/u/.yoyo.toml"),
            "warning must name the file written: {msg}"
        );
        assert!(
            msg.contains(".yoyo.toml"),
            "warning must name the shadowing file: {msg}"
        );
        // It must say the write will not take effect — the payload claim.
        let lower = msg.to_ascii_lowercase();
        assert!(
            lower.contains("not") || lower.contains("won't") || lower.contains("override"),
            "warning must state the write is not in effect: {msg}"
        );
    }

    #[test]
    fn test_existing_config_paths_are_in_precedence_order() {
        // Whatever exists on this machine, the order must mirror
        // config::load_config_file's first-wins chain.
        let paths = existing_config_paths();
        let project = std::path::PathBuf::from(".yoyo.toml");
        if paths.len() > 1 && paths.contains(&project) {
            assert_eq!(
                paths[0], project,
                "project-level .yoyo.toml must come first in precedence order"
            );
        }
        // detect_loaded_config_path must be exactly the head of that list.
        assert_eq!(detect_loaded_config_path(), paths.first().cloned());
    }
}
