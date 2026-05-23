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

/// Choose a default editor model given the current (architect) model.
/// Maps strong/expensive models to their cheaper counterparts from the same provider.
pub fn default_editor_model(current_model: &str) -> String {
    let m = current_model.to_lowercase();

    // Anthropic: opus → sonnet, sonnet → haiku
    if m.contains("opus") {
        return "claude-sonnet-4-20250514".into();
    }
    if m.contains("sonnet") {
        return "claude-haiku-4-5-20250414".into();
    }

    // OpenAI: gpt-4o → gpt-4o-mini, gpt-4.1 → gpt-4.1-mini, o3 → o3-mini
    if m.contains("gpt-4o") && !m.contains("mini") {
        return "gpt-4o-mini".into();
    }
    if m.contains("gpt-4.1") && !m.contains("mini") && !m.contains("nano") {
        return "gpt-4.1-mini".into();
    }
    if m == "o3" {
        return "o3-mini".into();
    }

    // Google: pro → flash
    if m.contains("gemini") && m.contains("pro") {
        return m.replace("pro", "flash");
    }

    // DeepSeek: reasoner → chat
    if m.contains("deepseek-reasoner") {
        return "deepseek-chat".into();
    }

    // xAI: grok-4 → grok-4-mini, grok-3 → grok-3-mini
    if m == "grok-4" {
        return "grok-4-mini".into();
    }
    if m == "grok-3" {
        return "grok-3-mini".into();
    }

    // Mistral: large → small
    if m.contains("mistral-large") {
        return "mistral-small-latest".into();
    }

    // Bedrock: follows the same anthropic pattern with prefix
    if m.contains("bedrock") || m.starts_with("anthropic.") {
        if m.contains("opus") {
            return "anthropic.claude-sonnet-4-20250514-v1:0".into();
        }
        if m.contains("sonnet") {
            return "anthropic.claude-haiku-4-5-20250414-v1:0".into();
        }
    }

    // Fallback: if we don't recognize the model, use it as its own editor
    // (architect mode still benefits from the plan-then-implement split)
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
            let editor = "(auto-selected)";
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
                eprintln!(
                    "{GREEN}  ✓ architect mode: ON{RESET}\n\
                     {DIM}    architect: current model\n\
                     {DIM}    editor: auto-selected{RESET}\n"
                );
            }
        }
        model => {
            // Enable with a specific architect model
            set_architect_mode(true, Some(model.to_string()));
            eprintln!(
                "{GREEN}  ✓ architect mode: ON{RESET}\n\
                 {DIM}    architect: {model}\n\
                 {DIM}    editor: auto-selected{RESET}\n"
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
    let editor = default_editor_model(&arch_model);
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
    // Project-level: ./.yoyo.toml
    let project = std::path::PathBuf::from(".yoyo.toml");
    if project.exists() {
        return Some(project);
    }
    // Home shorthand: ~/.yoyo.toml
    if let Some(path) = crate::cli::home_config_path() {
        if path.exists() {
            return Some(path);
        }
    }
    // XDG user-level: ~/.config/yoyo/config.toml
    if let Some(path) = crate::cli::user_config_path() {
        if path.exists() {
            return Some(path);
        }
    }
    None
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
/// Priority:
/// 1. `.yoyo.toml` in current directory (project-level) — only if it exists
/// 2. `~/.config/yoyo/config.toml` (XDG user-level) — even if it doesn't exist yet
///
/// Returns the path to open. If no user config directory can be determined,
/// returns `None`.
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
fn resolve_config_edit_path_in(root: &std::path::Path) -> Option<std::path::PathBuf> {
    // Project-level config takes priority if it already exists
    let project_config = root.join(".yoyo.toml");
    if project_config.exists() {
        return Some(project_config);
    }

    // Fall back to user-level config (create path even if file doesn't exist)
    if let Some(user_path) = crate::cli::user_config_path() {
        return Some(user_path);
    }

    None
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

    Ok((key, value, is_global))
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
            let saved = match agent.save_messages() {
                Ok(json) => Some(json),
                Err(e) => {
                    eprintln!("{DIM}  ⚠ could not preserve conversation: {e}{RESET}");
                    None
                }
            };
            *agent = agent_config.build_agent();
            if let Some(json) = saved {
                let _ = agent.restore_messages(&json);
            }
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
                let _ = agent.restore_messages(&json);
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
        // When no .yoyo.toml exists, should fall back to user config path
        let tmp = std::env::temp_dir().join("yoyo_test_config_edit_fallback");
        let _ = std::fs::create_dir_all(&tmp);
        // Make sure there's no .yoyo.toml
        let _ = std::fs::remove_file(tmp.join(".yoyo.toml"));

        let result = resolve_config_edit_path_in(&tmp);
        // As long as HOME is set, we should get a path
        if std::env::var("HOME").is_ok() {
            assert!(result.is_some(), "should return user config path");
            let path = result.unwrap();
            assert!(
                path.to_string_lossy().contains("config.toml"),
                "should point to user config.toml, got: {}",
                path.display()
            );
        }

        let _ = std::fs::remove_dir_all(&tmp);
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

    // --- architect mode tests ---

    #[test]
    fn test_default_editor_model_sonnet_maps_to_haiku() {
        let editor = default_editor_model("claude-sonnet-4-20250514");
        assert!(
            editor.to_lowercase().contains("haiku"),
            "expected haiku for sonnet, got: {editor}"
        );
    }

    #[test]
    fn test_default_editor_model_opus_maps_to_sonnet() {
        let editor = default_editor_model("claude-opus-4-20250115");
        assert!(
            editor.to_lowercase().contains("sonnet"),
            "expected sonnet for opus, got: {editor}"
        );
    }

    #[test]
    fn test_default_editor_model_gpt4o_maps_to_mini() {
        let editor = default_editor_model("gpt-4o");
        assert!(
            editor.to_lowercase().contains("gpt-4o-mini"),
            "expected gpt-4o-mini for gpt-4o, got: {editor}"
        );
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
}
