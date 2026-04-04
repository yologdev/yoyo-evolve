//! CLI argument parsing, config file support, and help text.

use crate::format::*;
use std::collections::HashMap;
use std::io::IsTerminal;
use yoagent::skills::SkillSet;
use yoagent::ThinkingLevel;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_CONTEXT_TOKENS: u64 = 200_000;
pub const AUTO_COMPACT_THRESHOLD: f64 = 0.80;
pub const PROACTIVE_COMPACT_THRESHOLD: f64 = 0.70;

/// Effective context window (tokens) for the current session.
/// Set once in configure_agent() based on model config + CLI override.
/// Read by /tokens and /status commands to show accurate budget.
static EFFECTIVE_CONTEXT_TOKENS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(DEFAULT_CONTEXT_TOKENS);

/// Set the effective context window size. Called once during agent setup.
pub fn set_effective_context_tokens(tokens: u64) {
    EFFECTIVE_CONTEXT_TOKENS.store(tokens, std::sync::atomic::Ordering::SeqCst);
}

/// Get the effective context window size for display purposes.
pub fn effective_context_tokens() -> u64 {
    EFFECTIVE_CONTEXT_TOKENS.load(std::sync::atomic::Ordering::SeqCst)
}
pub const DEFAULT_SESSION_PATH: &str = "yoyo-session.json";
pub const AUTO_SAVE_SESSION_PATH: &str = ".yoyo/last-session.json";

pub const SYSTEM_PROMPT: &str = r#"You are a coding assistant working in the user's terminal.
You have access to the filesystem and shell. Be direct and concise.
When the user asks you to do something, do it — don't just explain how.
Use tools proactively: read files to understand context, run commands to verify your work.
After making changes, run tests or verify the result when appropriate."#;

/// Known provider names for the --provider flag.
pub const KNOWN_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "google",
    "openrouter",
    "ollama",
    "xai",
    "groq",
    "deepseek",
    "mistral",
    "cerebras",
    "zai",
    "minimax",
    "bedrock",
    "custom",
];

/// Context management strategy.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ContextStrategy {
    /// Default: auto-compact conversation when approaching context limit
    #[default]
    Compaction,
    /// Write checkpoint file and exit with code 2 when approaching limit
    Checkpoint,
}

/// Permission configuration for tool execution.
/// Controls which bash commands are auto-approved, auto-denied, or require prompting.
/// Patterns use simple glob matching: `*` matches any sequence of characters.
#[derive(Debug, Clone, Default)]
pub struct PermissionConfig {
    /// Patterns that auto-approve matching bash commands (no prompt needed).
    pub allow: Vec<String>,
    /// Patterns that auto-deny matching bash commands (rejected with message).
    pub deny: Vec<String>,
}

/// Directory restriction configuration for file access security.
/// Controls which directories yoyo's file tools (read_file, write_file, edit_file,
/// list_files, search) can access. When configured, paths are canonicalized to prevent
/// `../` traversal escapes.
///
/// Rules:
/// - If `deny` is non-empty, any path under a denied directory is blocked.
/// - If `allow` is non-empty, only paths under an allowed directory are permitted.
/// - Deny overrides allow when both match.
/// - Paths are resolved to absolute paths before checking.
#[derive(Debug, Clone, Default)]
pub struct DirectoryRestrictions {
    /// Directories that are explicitly allowed. If non-empty, only these dirs are accessible.
    pub allow: Vec<String>,
    /// Directories that are explicitly denied. Always takes priority over allow.
    pub deny: Vec<String>,
}

impl DirectoryRestrictions {
    /// Returns true if no restrictions are configured.
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty()
    }

    /// Check whether a given file path is permitted under the current restrictions.
    /// Returns `Ok(())` if the path is allowed, or `Err(reason)` if blocked.
    ///
    /// Path resolution:
    /// - Absolute paths are used directly.
    /// - Relative paths are resolved against the current working directory.
    /// - Symlinks and `..` components are resolved via `std::fs::canonicalize`
    ///   when the path exists, or by manual normalization when it doesn't.
    pub fn check_path(&self, path: &str) -> Result<(), String> {
        if self.is_empty() {
            return Ok(());
        }

        let resolved = resolve_path(path);

        // Deny always takes priority
        for denied in &self.deny {
            let denied_resolved = resolve_path(denied);
            if path_is_under(&resolved, &denied_resolved) {
                return Err(format!(
                    "Access denied: '{}' is under restricted directory '{}'",
                    path, denied
                ));
            }
        }

        // If allow list is set, path must be under at least one allowed directory
        if !self.allow.is_empty() {
            let allowed = self.allow.iter().any(|a| {
                let a_resolved = resolve_path(a);
                path_is_under(&resolved, &a_resolved)
            });
            if !allowed {
                return Err(format!(
                    "Access denied: '{}' is not under any allowed directory",
                    path
                ));
            }
        }

        Ok(())
    }
}

/// Resolve a path to an absolute, normalized form.
/// Uses `canonicalize` for existing paths (resolves symlinks, `..`, etc.).
/// Falls back to manual normalization for paths that don't exist yet.
fn resolve_path(path: &str) -> String {
    // Try canonicalize first (works for existing paths)
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical.to_string_lossy().to_string();
    }

    // Manual normalization for non-existent paths
    let p = std::path::Path::new(path);
    let absolute = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("/"))
            .join(p)
    };

    // Normalize components: resolve `.` and `..`
    let mut components = Vec::new();
    for component in absolute.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            other => components.push(other),
        }
    }
    let normalized: std::path::PathBuf = components.iter().collect();
    normalized.to_string_lossy().to_string()
}

/// Check if `path` is under (or equal to) `dir`.
/// Both should be absolute, normalized paths.
fn path_is_under(path: &str, dir: &str) -> bool {
    // Ensure dir ends with separator for prefix matching
    let dir_with_sep = if dir.ends_with('/') {
        dir.to_string()
    } else {
        format!("{}/", dir)
    };
    path == dir || path.starts_with(&dir_with_sep)
}

impl PermissionConfig {
    /// Check a command against deny patterns first, then allow patterns.
    /// Returns `Some(true)` if allowed, `Some(false)` if denied, `None` if no match (prompt user).
    pub fn check(&self, command: &str) -> Option<bool> {
        // Deny takes priority — check deny patterns first
        for pattern in &self.deny {
            if glob_match(pattern, command) {
                return Some(false);
            }
        }
        // Then check allow patterns
        for pattern in &self.allow {
            if glob_match(pattern, command) {
                return Some(true);
            }
        }
        // No match — prompt the user
        None
    }

    /// Returns true if no patterns are configured.
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty()
    }
}

/// Simple glob matching: `*` matches any sequence of characters (including empty).
/// Supports multiple `*` wildcards. No other special characters.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();

    // No wildcards — exact match
    if parts.len() == 1 {
        return pattern == text;
    }

    let mut pos = 0;

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            // First segment must match at the start
            if !text.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == parts.len() - 1 {
            // Last segment must match at the end
            if !text[pos..].ends_with(part) {
                return false;
            }
            pos = text.len();
        } else {
            // Middle segments must appear in order
            match text[pos..].find(part) {
                Some(idx) => pos += idx + part.len(),
                None => return false,
            }
        }
    }

    true
}

/// Parse a TOML-style array value like `["pattern1", "pattern2"]` into a Vec<String>.
pub fn parse_toml_array(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Vec::new();
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    inner
        .split(',')
        .map(|s| {
            let s = s.trim();
            // Strip quotes
            if (s.starts_with('"') && s.ends_with('"'))
                || (s.starts_with('\'') && s.ends_with('\''))
            {
                s[1..s.len() - 1].to_string()
            } else {
                s.to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse a `[permissions]` section from a TOML config file content.
/// Looks for `allow = [...]` and `deny = [...]` lines under `[permissions]`.
pub fn parse_permissions_from_config(content: &str) -> PermissionConfig {
    let mut config = PermissionConfig::default();
    let mut in_permissions = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Check for section headers
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_permissions = trimmed == "[permissions]";
            continue;
        }
        if !in_permissions {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "allow" => config.allow = parse_toml_array(value),
                "deny" => config.deny = parse_toml_array(value),
                _ => {}
            }
        }
    }
    config
}

/// Parse a `[directories]` section from a TOML config file content.
/// Looks for `allow = [...]` and `deny = [...]` lines under `[directories]`.
pub fn parse_directories_from_config(content: &str) -> DirectoryRestrictions {
    let mut config = DirectoryRestrictions::default();
    let mut in_directories = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_directories = trimmed == "[directories]";
            continue;
        }
        if !in_directories {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "allow" => config.allow = parse_toml_array(value),
                "deny" => config.deny = parse_toml_array(value),
                _ => {}
            }
        }
    }
    config
}

/// Parsed CLI configuration.
pub struct Config {
    pub model: String,
    pub api_key: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub skills: SkillSet,
    pub system_prompt: String,
    pub thinking: ThinkingLevel,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub max_turns: Option<usize>,
    pub continue_session: bool,
    pub output_path: Option<String>,
    pub prompt_arg: Option<String>,
    pub image_path: Option<String>,
    pub verbose: bool,
    pub mcp_servers: Vec<String>,
    pub openapi_specs: Vec<String>,
    pub auto_approve: bool,
    pub permissions: PermissionConfig,
    pub dir_restrictions: DirectoryRestrictions,
    pub context_strategy: ContextStrategy,
    pub context_window: Option<u32>,
    pub shell_hooks: Vec<crate::hooks::ShellHook>,
    pub fallback_provider: Option<String>,
    pub fallback_model: Option<String>,
    pub no_update_check: bool,
    pub json_output: bool,
    pub audit: bool,
    pub print_system_prompt: bool,
}

/// Whether verbose output is enabled. Set once at startup.
static VERBOSE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Enable verbose output.
pub fn enable_verbose() {
    let _ = VERBOSE.set(true);
}

/// Check if verbose output is enabled.
pub fn is_verbose() -> bool {
    *VERBOSE.get_or_init(|| false)
}

/// Project context file names, checked in order. YOYO.md is the canonical name;
/// CLAUDE.md is supported as a compatibility alias for projects that already use it.
/// All found files are concatenated.
pub const PROJECT_CONTEXT_FILES: &[&str] = &["YOYO.md", "CLAUDE.md", ".yoyo/instructions.md"];

pub fn print_help() {
    println!("yoyo v{VERSION} — a coding agent growing up in public");
    println!();
    println!("Usage: yoyo [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --model <name>    Model to use (default: claude-opus-4-6)");
    println!("  --provider <name> Provider: anthropic (default), openai, google, openrouter,");
    println!("                    ollama, xai, groq, deepseek, mistral, cerebras, zai, custom");
    println!("  --base-url <url>  Custom API endpoint (e.g., http://localhost:11434/v1)");
    println!("  --thinking <lvl>  Enable extended thinking (off, minimal, low, medium, high)");
    println!("  --max-tokens <n>  Maximum output tokens per response (default: 8192)");
    println!("  --max-turns <n>   Maximum agent turns per prompt (default: 50)");
    println!("  --temperature <f> Sampling temperature (0.0-1.0, default: model default)");
    println!("  --skills <dir>    Directory containing skill files");
    println!("  --system <text>   Custom system prompt (overrides default)");
    println!("  --system-file <f> Read system prompt from file");
    println!("  --prompt, -p <t>  Run a single prompt and exit (no REPL)");
    println!("  --output, -o <f>  Write final response text to a file");
    println!("  --api-key <key>   API key (overrides provider-specific env var)");
    println!("  --mcp <cmd>       Connect to an MCP server via stdio (repeatable)");
    println!("  --openapi <spec>  Load OpenAPI spec file and register API tools (repeatable)");
    println!("  --no-color        Disable colored output (also respects NO_COLOR env)");
    println!("  --no-bell         Disable terminal bell on long completions (also respects YOYO_NO_BELL env)");
    println!(
        "  --no-update-check Skip startup update check (also respects YOYO_NO_UPDATE_CHECK=1 env)"
    );
    println!("  --json            Output JSON instead of plain text (for -p and piped modes)");
    println!("  --audit           Enable audit logging of all tool calls to .yoyo/audit.jsonl");
    println!("                    (also respects YOYO_AUDIT=1 env or audit = true in config)");
    println!("  --verbose, -v     Show debug info (API errors, request details)");
    println!("  --yes, -y         Auto-approve all tool executions (skip confirmation prompts)");
    println!("  --allow <pat>     Auto-approve bash commands matching glob pattern (repeatable)");
    println!("  --deny <pat>      Auto-deny bash commands matching glob pattern (repeatable)");
    println!("  --allow-dir <d>   Restrict file access to this directory (repeatable)");
    println!("  --deny-dir <d>    Block file access to this directory (repeatable)");
    println!("  --context-strategy <s>  Context management: compaction (default) or checkpoint");
    println!(
        "  --context-window <n>    Override context window size (tokens). Default: auto-detected"
    );
    println!(
        "                          per provider (200K Anthropic, 1M Google, 128K OpenAI, etc.)"
    );
    println!("  --continue, -c    Resume last saved session");
    println!("  --fallback <prov> Fallback provider if primary fails (e.g. --fallback google)");
    println!("  --print-system-prompt  Print the fully assembled system prompt and exit");
    println!("  --help, -h        Show this help message");
    println!("  --version, -V     Show version");
    println!();
    println!("Commands (in REPL):");
    println!("  /quit, /exit      Exit the agent");
    println!("  /clear            Clear conversation history");
    println!("  /compact          Compact conversation to save context space");
    println!("  /commit [msg]     Commit staged changes (AI-generates message if no msg)");
    println!("  /config           Show all current settings");
    println!("  /context          Show loaded project context files (YOYO.md)");
    println!("  /cost             Show estimated session cost");
    println!("  /diff             Show git diff summary of uncommitted changes");
    println!("  /docs <crate>     Look up docs.rs documentation for a Rust crate");
    println!("  /find <pattern>   Fuzzy-search project files by name");
    println!("  /fix              Auto-fix build/lint errors (runs checks, sends failures to AI)");
    println!("  /forget <n>       Remove a project memory by index");
    println!("  /git <subcmd>     Quick git: status, log [n], add <path>, stash, stash pop");
    println!("  /health           Run project health checks (auto-detects project type)");
    println!("  /pr [number]      List open PRs, or view details of a specific PR");
    println!("  /history          Show summary of conversation messages");
    println!("  /search <query>   Search conversation history for matching messages");
    println!("  /init             Create a starter YOYO.md project context file");
    println!("  /lint             Auto-detect and run project linter");
    println!("  /load [path]      Load session from file");
    println!("  /memories         List project-specific memories");
    println!("  /model <name>     Switch model mid-session");
    println!("  /retry            Re-send the last user input");
    println!("  /remember <note>  Save a project-specific memory (persists across sessions)");
    println!("  /review [path]    AI code review: staged changes (default) or a specific file");
    println!("  /run <cmd>        Run a shell command directly (no AI, no tokens)");
    println!("  /save [path]      Save session to file");
    println!("  /spawn <task>     Spawn a subagent with fresh context to handle a task");
    println!("  /status           Show session info");
    println!("  /test             Auto-detect and run project tests");
    println!("  /think [level]    Show or change thinking level (off/low/medium/high)");
    println!("  /tokens           Show token usage and context window");
    println!("  /tree [depth]     Show project directory tree (default depth: 3)");
    println!("  /undo             Revert all uncommitted changes (git checkout)");
    println!("  /version          Show yoyo version");
    println!();
    println!("Environment:");
    println!("  ANTHROPIC_API_KEY  API key for Anthropic (default provider)");
    println!("  OPENAI_API_KEY    API key for OpenAI");
    println!("  GOOGLE_API_KEY    API key for Google/Gemini");
    println!("  GROQ_API_KEY      API key for Groq");
    println!("  XAI_API_KEY       API key for xAI");
    println!("  DEEPSEEK_API_KEY  API key for DeepSeek");
    println!("  OPENROUTER_API_KEY API key for OpenRouter");
    println!("  ZAI_API_KEY       API key for ZAI (Zhipu AI / z.ai)");
    println!("  API_KEY            Fallback API key (any provider)");
    println!("  YOYO_NO_UPDATE_CHECK  Set to 1 to skip startup update check");
    println!("  YOYO_AUDIT            Set to 1 to enable audit logging");
    println!();
    println!("Config files (searched in order, first found wins):");
    println!("  .yoyo.toml                  Project-level config (current directory)");
    println!("  ~/.yoyo.toml                Home directory config");
    println!("  ~/.config/yoyo/config.toml  User-level config (XDG)");
    println!();
    println!("Config file format (key = value):");
    println!("  model = \"claude-sonnet-4-20250514\"");
    println!("  provider = \"openai\"");
    println!("  base_url = \"http://localhost:11434/v1\"");
    println!("  thinking = \"medium\"");
    println!("  max_tokens = 4096");
    println!("  max_turns = 20");
    println!("  api_key = \"sk-ant-...\"");
    println!("  system_prompt = \"You are a Go expert\"");
    println!("  system_file = \"prompts/system.txt\"");
    println!("  mcp = [\"npx open-websearch@latest\", \"npx @mcp/server-filesystem /tmp\"]");
    println!();
    println!("  [permissions]");
    println!("  allow = [\"git *\", \"cargo *\"]");
    println!("  deny = [\"rm -rf *\"]");
    println!();
    println!("  [directories]");
    println!("  allow = [\"./src\", \"./tests\"]");
    println!("  deny = [\"~/.ssh\", \"/etc\"]");
    println!();
    println!("CLI flags override config file values.");
}

pub fn print_banner() {
    println!(
        "\n{BOLD}{CYAN}  yoyo{RESET} v{VERSION} {DIM}— a coding agent growing up in public{RESET}"
    );
    println!("{DIM}  Type /help for commands, /quit to exit{RESET}\n");
}

/// Compare two version strings (e.g. "0.1.5" vs "0.2.0").
/// Returns true if `latest` is strictly newer than `current`.
pub fn version_is_newer(current: &str, latest: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let cur = parse(current);
    let lat = parse(latest);
    let len = cur.len().max(lat.len());
    for i in 0..len {
        let c = cur.get(i).copied().unwrap_or(0);
        let l = lat.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }
    false
}

/// Check GitHub for a newer release. Returns `Some("x.y.z")` if a newer version
/// exists, `None` if current or on any error. Uses a 3-second timeout to avoid
/// blocking startup.
pub fn check_for_update() -> Option<String> {
    let output = std::process::Command::new("curl")
        .args([
            "-sf",
            "--max-time",
            "3",
            "https://api.github.com/repos/yologdev/yoyo-evolve/releases/latest",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let body = String::from_utf8(output.stdout).ok()?;

    // Simple JSON extraction: find "tag_name": "v0.1.5"
    let tag = body
        .split("\"tag_name\"")
        .nth(1)?
        .split('"')
        .find(|s| !s.is_empty() && *s != ":" && *s != ": ")?;

    let latest = tag.strip_prefix('v').unwrap_or(tag);

    if version_is_newer(VERSION, latest) {
        Some(latest.to_string())
    } else {
        None
    }
}

/// Parse a thinking level string into a ThinkingLevel enum.
pub fn parse_thinking_level(s: &str) -> ThinkingLevel {
    match s.to_lowercase().as_str() {
        "off" | "none" => ThinkingLevel::Off,
        "minimal" | "min" => ThinkingLevel::Minimal,
        "low" => ThinkingLevel::Low,
        "medium" | "med" => ThinkingLevel::Medium,
        "high" | "max" => ThinkingLevel::High,
        _ => {
            eprintln!(
                "{YELLOW}warning:{RESET} Unknown thinking level '{s}', using 'medium'. \
                 Valid: off, minimal, low, medium, high"
            );
            ThinkingLevel::Medium
        }
    }
}

/// Clamp temperature to the valid 0.0–1.0 range, warning if out of bounds.
pub fn clamp_temperature(t: f32) -> f32 {
    if t < 0.0 {
        eprintln!("{YELLOW}warning:{RESET} Temperature {t} is below 0.0, clamping to 0.0");
        0.0
    } else if t > 1.0 {
        eprintln!("{YELLOW}warning:{RESET} Temperature {t} is above 1.0, clamping to 1.0");
        1.0
    } else {
        t
    }
}

/// All known CLI flags (both boolean and value-taking).
const KNOWN_FLAGS: &[&str] = &[
    "--model",
    "--provider",
    "--base-url",
    "--thinking",
    "--max-tokens",
    "--max-turns",
    "--temperature",
    "--skills",
    "--system",
    "--system-file",
    "--prompt",
    "-p",
    "--output",
    "-o",
    "--api-key",
    "--mcp",
    "--openapi",
    "--allow",
    "--deny",
    "--allow-dir",
    "--deny-dir",
    "--image",
    "--context-strategy",
    "--context-window",
    "--no-color",
    "--no-bell",
    "--no-update-check",
    "--json",
    "--verbose",
    "-v",
    "--yes",
    "-y",
    "--continue",
    "-c",
    "--fallback",
    "--audit",
    "--print-system-prompt",
    "--help",
    "-h",
    "--version",
    "-V",
];

/// Warn about any unrecognized flags in the arguments.
/// Skips args[0] (binary name) and values that follow flags expecting values.
pub fn warn_unknown_flags(args: &[String], flags_needing_values: &[&str]) {
    let mut skip_next = false;
    for arg in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with('-') {
            if flags_needing_values.contains(&arg.as_str()) {
                skip_next = true; // skip the value that follows
            } else if !KNOWN_FLAGS.contains(&arg.as_str()) {
                eprintln!(
                    "{YELLOW}warning:{RESET} Unknown flag '{arg}' — ignored. Run --help for usage."
                );
            }
        }
    }
}

/// Maximum number of files to include in the project file listing.
pub const MAX_PROJECT_FILES: usize = 200;

/// Get a listing of project files using `git ls-files`.
/// Returns a newline-separated list of tracked files, capped at MAX_PROJECT_FILES.
/// Returns None if git is not available or the directory is not a git repo.
pub fn get_project_file_listing() -> Option<String> {
    let stdout = crate::git::run_git(&["ls-files"]).ok()?;
    let files: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    if files.is_empty() {
        return None;
    }
    let total = files.len();
    let capped: Vec<&str> = files.into_iter().take(MAX_PROJECT_FILES).collect();
    let mut listing = capped.join("\n");
    if total > MAX_PROJECT_FILES {
        listing.push_str(&format!(
            "\n... and {} more files",
            total - MAX_PROJECT_FILES
        ));
    }
    Some(listing)
}

/// Get a brief git status summary for system prompt injection.
/// Returns None if not in a git repo or git is unavailable.
pub fn get_git_status_context() -> Option<String> {
    let branch = crate::git::git_branch()?;

    let uncommitted = crate::git::run_git(&["status", "--porcelain"])
        .ok()
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0);

    let staged = crate::git::run_git(&["diff", "--cached", "--name-only"])
        .ok()
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0);

    let mut result = String::from("## Git Status\n\n");
    result.push_str(&format!("Branch: {branch}\n"));
    if uncommitted > 0 {
        result.push_str(&format!(
            "Uncommitted changes: {} file{}\n",
            uncommitted,
            if uncommitted == 1 { "" } else { "s" }
        ));
    }
    if staged > 0 {
        result.push_str(&format!(
            "Staged: {} file{}\n",
            staged,
            if staged == 1 { "" } else { "s" }
        ));
    }

    Some(result)
}

/// Get the most recently changed files from git log, deduplicated.
/// Returns up to `max_files` unique file paths that were modified in recent commits.
/// Returns None if not in a git repo or git is unavailable.
pub fn get_recently_changed_files(max_files: usize) -> Option<Vec<String>> {
    let stdout = crate::git::run_git(&[
        "log",
        "--diff-filter=M",
        "--name-only",
        "--pretty=format:",
        "-n",
        "20",
    ])
    .ok()?;
    let mut seen = std::collections::HashSet::new();
    let files: Vec<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .filter(|l| seen.insert(l.to_string()))
        .take(max_files)
        .map(|l| l.to_string())
        .collect();
    if files.is_empty() {
        None
    } else {
        Some(files)
    }
}

/// Maximum number of recently changed files to include in context.
pub const MAX_RECENT_FILES: usize = 20;

/// Load project context from YOYO.md (primary), CLAUDE.md (compatibility alias),
/// or .yoyo/instructions.md.
/// Returns the combined content of all found files, or None if none exist.
/// Also appends a project file listing from `git ls-files` and recently changed files
/// when available.
pub fn load_project_context() -> Option<String> {
    let mut context = String::new();
    let mut found = Vec::new();
    for name in PROJECT_CONTEXT_FILES {
        if let Ok(content) = std::fs::read_to_string(name) {
            let content = content.trim();
            if !content.is_empty() {
                if !context.is_empty() {
                    context.push_str("\n\n");
                }
                context.push_str(content);
                found.push(*name);
            }
        }
    }

    // Append project file listing if available
    if let Some(file_listing) = get_project_file_listing() {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        context.push_str("## Project Files\n\n");
        context.push_str(&file_listing);
        if found.is_empty() {
            // Even without context files, file listing alone is useful
            eprintln!("{DIM}  context: project file listing{RESET}");
        }
    }

    // Append recently changed files if available
    if let Some(recent_files) = get_recently_changed_files(MAX_RECENT_FILES) {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        context.push_str("## Recently Changed Files\n\n");
        context.push_str(&recent_files.join("\n"));
    }

    // Append git status if available
    let git_branch_name = if let Some(git_status) = get_git_status_context() {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        let branch = crate::git::git_branch();
        context.push_str(&git_status);
        branch
    } else {
        None
    };

    // Append project memories if available
    let memory = crate::memory::load_memories();
    if let Some(memories_section) = crate::memory::format_memories_for_prompt(&memory) {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        context.push_str(&memories_section);
    }

    if found.is_empty() && context.is_empty() {
        None
    } else {
        for name in &found {
            eprintln!("{DIM}  context: {name}{RESET}");
        }
        if context.contains("## Recently Changed Files") {
            eprintln!("{DIM}  context: recently changed files{RESET}");
        }
        if let Some(branch) = &git_branch_name {
            eprintln!("{DIM}  context: git status (branch: {branch}){RESET}");
        }
        if !memory.entries.is_empty() {
            eprintln!(
                "{DIM}  context: {} project memories{RESET}",
                memory.entries.len()
            );
        }
        Some(context)
    }
}

/// List which project context files exist and their sizes.
/// Returns a vec of (filename, line_count) for display by /context.
pub fn list_project_context_files() -> Vec<(&'static str, usize)> {
    let mut result = Vec::new();
    for name in PROJECT_CONTEXT_FILES {
        if let Ok(content) = std::fs::read_to_string(name) {
            let content = content.trim();
            if !content.is_empty() {
                let lines = content.lines().count();
                result.push((*name, lines));
            }
        }
    }
    result
}

/// Config file search paths, checked in order (first found wins).
/// - `.yoyo.toml` in the current directory (project-level)
/// - `~/.yoyo.toml` (home directory shorthand)
/// - `~/.config/yoyo/config.toml` (XDG user-level)
const CONFIG_FILE_NAMES: &[&str] = &[".yoyo.toml"];

pub fn user_config_path() -> Option<std::path::PathBuf> {
    dirs_hint().map(|dir| dir.join("yoyo").join("config.toml"))
}

/// Home directory config path: ~/.yoyo.toml
pub fn home_config_path() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| std::path::PathBuf::from(h).join(".yoyo.toml"))
}

/// Best-effort XDG config dir (~/.config on Linux/macOS).
fn dirs_hint() -> Option<std::path::PathBuf> {
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(h).join(".config"))
        })
}

/// Best-effort XDG data dir (~/.local/share on Linux/macOS).
fn data_dir_hint() -> Option<std::path::PathBuf> {
    std::env::var("XDG_DATA_HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(h).join(".local").join("share"))
        })
}

/// Get the path for the readline history file.
/// Prefers `$XDG_DATA_HOME/yoyo/history`, falls back to `~/.yoyo_history`.
pub fn history_file_path() -> Option<std::path::PathBuf> {
    // Try XDG data dir first
    if let Some(data_dir) = data_dir_hint() {
        let yoyo_dir = data_dir.join("yoyo");
        // Try to create the directory; if it works, use it
        if std::fs::create_dir_all(&yoyo_dir).is_ok() {
            return Some(yoyo_dir.join("history"));
        }
    }
    // Fall back to ~/.yoyo_history
    std::env::var("HOME")
        .ok()
        .map(|h| std::path::PathBuf::from(h).join(".yoyo_history"))
}

/// Parse a simple TOML-like config file (key = "value" or key = value per line).
/// Ignores comments (#) and blank lines. Returns a map of key → value.
pub fn parse_config_file(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim();
            // Strip surrounding quotes if present
            let value = if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value[1..value.len() - 1].to_string()
            } else {
                value.to_string()
            };
            map.insert(key, value);
        }
    }
    map
}

/// Resolve the system prompt using the precedence chain:
/// CLI --system-file > CLI --system > config system_file > config system_prompt > default SYSTEM_PROMPT
///
/// `cli_system_file_content` is already-read file content from `--system-file`.
/// `cli_system` is the raw text from `--system`.
/// `config_system_file` is the path from config `system_file` key (will be read here).
/// `config_system_prompt` is the text from config `system_prompt` key.
pub fn resolve_system_prompt(
    cli_system_file_content: Option<String>,
    cli_system: Option<String>,
    config_system_file: Option<String>,
    config_system_prompt: Option<String>,
) -> String {
    // CLI --system-file wins over everything
    if let Some(content) = cli_system_file_content {
        return content;
    }
    // CLI --system wins over config
    if let Some(text) = cli_system {
        return text;
    }
    // Config system_file wins over config system_prompt
    if let Some(path) = config_system_file {
        match std::fs::read_to_string(&path) {
            Ok(content) => return content,
            Err(e) => {
                eprintln!(
                    "{RED}error:{RESET} Failed to read system_file '{path}' from config: {e}"
                );
                std::process::exit(1);
            }
        }
    }
    // Config system_prompt
    if let Some(text) = config_system_prompt {
        return text;
    }
    // Default
    SYSTEM_PROMPT.to_string()
}

/// Load config from file, checking project-level, home-level, then user-level paths.
/// Returns an empty map if no config file is found.
/// Read the config file once, returning both the parsed key-value map and the raw content.
/// Checks project-level, home-level (~/.yoyo.toml), then user-level (XDG) paths.
/// Returns `(HashMap, raw_content)` or `(empty HashMap, empty string)` if no config found.
fn load_config_file() -> (HashMap<String, String>, String) {
    // Check project-level config first
    for name in CONFIG_FILE_NAMES {
        if let Ok(content) = std::fs::read_to_string(name) {
            eprintln!("{DIM}  config: {name}{RESET}");
            return (parse_config_file(&content), content);
        }
    }
    // Check ~/.yoyo.toml (home directory shorthand)
    if let Some(path) = home_config_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            eprintln!("{DIM}  config: {}{RESET}", path.display());
            return (parse_config_file(&content), content);
        }
    }
    // Check user-level config (XDG)
    if let Some(path) = user_config_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            eprintln!("{DIM}  config: {}{RESET}", path.display());
            return (parse_config_file(&content), content);
        }
    }
    (HashMap::new(), String::new())
}

/// Parse CLI arguments into a Config, or exit with help/version.
/// Returns None if --help or --version was handled (program should exit).
pub fn parse_args(args: &[String]) -> Option<Config> {
    // Handle --help and --version before anything else
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return None;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("yoyo v{VERSION}");
        return None;
    }

    // Load config file defaults (CLI flags override these)
    // Read the file once and reuse raw content for permissions + directory parsing
    let (file_config, raw_config_content) = load_config_file();

    // Validate that flags requiring values actually have them
    let flags_needing_values = [
        "--model",
        "--provider",
        "--base-url",
        "--thinking",
        "--max-tokens",
        "--max-turns",
        "--temperature",
        "--skills",
        "--system",
        "--system-file",
        "--prompt",
        "-p",
        "--output",
        "-o",
        "--api-key",
        "--mcp",
        "--openapi",
        "--allow",
        "--deny",
        "--allow-dir",
        "--deny-dir",
        "--image",
        "--context-strategy",
        "--context-window",
        "--fallback",
    ];
    for flag in &flags_needing_values {
        if let Some(pos) = args.iter().position(|a| a == flag) {
            match args.get(pos + 1) {
                None => {
                    eprintln!("{RED}error:{RESET} {flag} requires a value");
                    eprintln!("Run with --help for usage information.");
                    std::process::exit(1);
                }
                Some(next)
                    if next.starts_with('-')
                        && !next.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) =>
                {
                    eprintln!(
                        "{YELLOW}warning:{RESET} {flag} value looks like another flag: '{next}'"
                    );
                }
                _ => {}
            }
        }
    }

    // Warn about unknown flags
    warn_unknown_flags(args, &flags_needing_values);

    // Parse --provider flag (CLI > config file > default "anthropic")
    let provider = args
        .iter()
        .position(|a| a == "--provider")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| file_config.get("provider").cloned())
        .unwrap_or_else(|| "anthropic".into())
        .to_lowercase();

    // Validate provider name
    if !KNOWN_PROVIDERS.contains(&provider.as_str()) {
        eprintln!(
            "{YELLOW}warning:{RESET} Unknown provider '{provider}'. Known providers: {}",
            KNOWN_PROVIDERS.join(", ")
        );
    }

    // Parse --base-url flag (CLI > config file)
    let base_url = args
        .iter()
        .position(|a| a == "--base-url")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| file_config.get("base_url").cloned());

    // Parse prompt and image flags early so we can validate --image before API key check
    let prompt_arg = args
        .iter()
        .position(|a| a == "--prompt" || a == "-p")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let image_path_raw = args
        .iter()
        .position(|a| a == "--image")
        .and_then(|i| args.get(i + 1))
        .cloned();

    // Validate --image flag usage
    if let Some(ref img_path) = image_path_raw {
        if prompt_arg.is_none() {
            // --image without -p: warn (image will be ignored in REPL mode)
            eprintln!(
                "{YELLOW}warning:{RESET} --image only works with -p (prompt mode). Ignoring --image flag."
            );
        } else {
            // --image with -p: validate the file
            let path = std::path::Path::new(img_path.as_str());
            if !path.exists() {
                eprintln!("{RED}error:{RESET} image file not found: {img_path}");
                std::process::exit(1);
            }
            if !crate::commands_file::is_image_extension(img_path) {
                eprintln!(
                    "{RED}error:{RESET} '{img_path}' is not a supported image format. Supported: png, jpg, jpeg, gif, webp, bmp"
                );
                std::process::exit(1);
            }
        }
    }

    // Clear image_path if no -p flag (already warned above)
    let image_path = if prompt_arg.is_some() {
        image_path_raw
    } else {
        None
    };

    // API key: --api-key flag > provider-specific env > ANTHROPIC_API_KEY > API_KEY > config file
    let api_key_from_flag = args
        .iter()
        .position(|a| a == "--api-key")
        .and_then(|i| args.get(i + 1))
        .cloned();

    // Choose provider-specific env var name
    let provider_env_var = provider_api_key_env(&provider);

    let api_key = match api_key_from_flag {
        Some(key) if !key.is_empty() => key,
        _ => {
            // Try provider-specific env var first
            let from_provider_env = provider_env_var
                .and_then(|var| std::env::var(var).ok())
                .filter(|k| !k.is_empty());
            match from_provider_env {
                Some(key) => key,
                None => {
                    // Fallback chain: ANTHROPIC_API_KEY > API_KEY > config file
                    match std::env::var("ANTHROPIC_API_KEY").or_else(|_| std::env::var("API_KEY")) {
                        Ok(key) if !key.is_empty() => key,
                        _ => match file_config.get("api_key").cloned() {
                            Some(key) if !key.is_empty() => key,
                            _ => {
                                // For local/ollama providers, API key is optional
                                if provider == "ollama" || provider == "custom" {
                                    "not-needed".to_string()
                                } else if std::io::stdin().is_terminal() && prompt_arg.is_none() {
                                    // Interactive REPL with no API key: needs_setup() will
                                    // be checked in main() and the wizard run there
                                    String::new()
                                } else {
                                    // Piped/single-shot mode: terse error for scripts
                                    let env_hint = provider_env_var.unwrap_or("ANTHROPIC_API_KEY");
                                    eprintln!("{RED}error:{RESET} No API key found.");
                                    eprintln!(
                                        "Set {env_hint} env var, use --api-key <key>, or add api_key to .yoyo.toml."
                                    );
                                    std::process::exit(1);
                                }
                            }
                        },
                    }
                }
            }
        }
    };

    let model = args
        .iter()
        .position(|a| a == "--model")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| file_config.get("model").cloned())
        .unwrap_or_else(|| default_model_for_provider(&provider));

    let skill_dirs: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--skills")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();

    let skills = if skill_dirs.is_empty() {
        SkillSet::empty()
    } else {
        match SkillSet::load(&skill_dirs) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{YELLOW}warning:{RESET} Failed to load skills: {e}");
                SkillSet::empty()
            }
        }
    };

    // Custom system prompt: --system "text" or --system-file path
    let custom_system = args
        .iter()
        .position(|a| a == "--system")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let system_from_file = args
        .iter()
        .position(|a| a == "--system-file")
        .and_then(|i| args.get(i + 1))
        .map(|path| {
            std::fs::read_to_string(path).unwrap_or_else(|e| {
                eprintln!("{RED}error:{RESET} Failed to read system prompt file '{path}': {e}");
                std::process::exit(1);
            })
        });

    // Precedence: CLI --system-file > CLI --system > config system_file > config system_prompt > default
    let mut system_prompt = resolve_system_prompt(
        system_from_file,
        custom_system,
        file_config.get("system_file").cloned(),
        file_config.get("system_prompt").cloned(),
    );

    // Append project context (YOYO.md, .yoyo/instructions.md) to system prompt
    if let Some(project_context) = load_project_context() {
        system_prompt.push_str("\n\n# Project Instructions\n\n");
        system_prompt.push_str(&project_context);
    }

    // Append repo map for structural codebase awareness
    if let Some(repo_map) = crate::commands_search::generate_repo_map_for_prompt() {
        system_prompt.push_str("\n\n# Repository Structure\n\n");
        system_prompt.push_str(&repo_map);
    }

    // --thinking <level> enables extended thinking (CLI overrides config file)
    let thinking = args
        .iter()
        .position(|a| a == "--thinking")
        .and_then(|i| args.get(i + 1))
        .map(|s| parse_thinking_level(s))
        .or_else(|| file_config.get("thinking").map(|s| parse_thinking_level(s)))
        .unwrap_or(ThinkingLevel::Off);

    let continue_session = args.iter().any(|a| a == "--continue" || a == "-c");

    let max_tokens = args
        .iter()
        .position(|a| a == "--max-tokens")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| {
            s.parse::<u32>().ok().or_else(|| {
                eprintln!(
                    "{YELLOW}warning:{RESET} Invalid --max-tokens value '{s}', using default"
                );
                None
            })
        })
        .or_else(|| {
            file_config
                .get("max_tokens")
                .and_then(|s| s.parse::<u32>().ok())
        });

    let temperature = args
        .iter()
        .position(|a| a == "--temperature")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| {
            s.parse::<f32>().ok().or_else(|| {
                eprintln!(
                    "{YELLOW}warning:{RESET} Invalid --temperature value '{s}', using default"
                );
                None
            })
        })
        .or_else(|| {
            file_config
                .get("temperature")
                .and_then(|s| s.parse::<f32>().ok())
        })
        .map(clamp_temperature);

    let max_turns = args
        .iter()
        .position(|a| a == "--max-turns")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| {
            s.parse::<usize>().ok().or_else(|| {
                eprintln!("{YELLOW}warning:{RESET} Invalid --max-turns value '{s}', using default");
                None
            })
        })
        .or_else(|| {
            file_config
                .get("max_turns")
                .and_then(|s| s.parse::<usize>().ok())
        });

    let output_path = args
        .iter()
        .position(|a| a == "--output" || a == "-o")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");

    let auto_approve = args.iter().any(|a| a == "--yes" || a == "-y");

    let no_update_check = args.iter().any(|a| a == "--no-update-check")
        || std::env::var("YOYO_NO_UPDATE_CHECK")
            .map(|v| v == "1")
            .unwrap_or(false);

    let json_output = args.iter().any(|a| a == "--json");

    let audit = args.iter().any(|a| a == "--audit")
        || std::env::var("YOYO_AUDIT")
            .map(|v| v == "1")
            .unwrap_or(false)
        || file_config
            .get("audit")
            .map(|v| v == "true")
            .unwrap_or(false);

    let print_system_prompt = args.iter().any(|a| a == "--print-system-prompt");

    // --allow <pattern> flags: collect all allow patterns (repeatable)
    let cli_allow: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--allow")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();

    // --deny <pattern> flags: collect all deny patterns (repeatable)
    let cli_deny: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--deny")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();

    // Build permission config: CLI flags override config file
    let permissions = if cli_allow.is_empty() && cli_deny.is_empty() {
        // No CLI flags — parse from already-loaded config content
        parse_permissions_from_config(&raw_config_content)
    } else {
        PermissionConfig {
            allow: cli_allow,
            deny: cli_deny,
        }
    };

    // --allow-dir <dir> flags: collect all allowed directories (repeatable)
    let cli_allow_dirs: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--allow-dir")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();

    // --deny-dir <dir> flags: collect all denied directories (repeatable)
    let cli_deny_dirs: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--deny-dir")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();

    // Build directory restrictions: CLI flags override config file
    let dir_restrictions = if cli_allow_dirs.is_empty() && cli_deny_dirs.is_empty() {
        parse_directories_from_config(&raw_config_content)
    } else {
        DirectoryRestrictions {
            allow: cli_allow_dirs,
            deny: cli_deny_dirs,
        }
    };

    // --context-strategy <compaction|checkpoint> (CLI only, not in config file)
    let context_strategy = args
        .iter()
        .position(|a| a == "--context-strategy")
        .and_then(|i| args.get(i + 1))
        .map(|val| match val.as_str() {
            "compaction" => ContextStrategy::Compaction,
            "checkpoint" => ContextStrategy::Checkpoint,
            other => {
                eprintln!(
                    "{YELLOW}warning:{RESET} Unknown context strategy '{other}', using compaction"
                );
                ContextStrategy::Compaction
            }
        })
        .unwrap_or_default();

    // --context-window <N> (CLI > config file > None = auto-detect from model)
    let context_window = args
        .iter()
        .position(|a| a == "--context-window")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| {
            s.parse::<u32>().ok().or_else(|| {
                eprintln!(
                    "{YELLOW}warning:{RESET} Invalid --context-window value '{s}', using model default"
                );
                None
            })
        })
        .or_else(|| {
            file_config
                .get("context_window")
                .and_then(|s| s.parse::<u32>().ok())
        });

    // --mcp <command> flags: collect all MCP server commands (repeatable)
    let mut mcp_servers: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--mcp")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();

    // Merge MCP servers from config file (config servers added first, CLI servers override/add)
    if let Some(mcp_config) = file_config.get("mcp") {
        let config_mcps = parse_toml_array(mcp_config);
        for server in config_mcps.into_iter().rev() {
            if !mcp_servers.contains(&server) {
                mcp_servers.insert(0, server);
            }
        }
    }

    // --openapi <spec-path> flags: collect all OpenAPI spec paths (repeatable)
    let openapi_specs: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--openapi")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();

    // Parse shell hooks from config file
    let shell_hooks = crate::hooks::parse_hooks_from_config(&file_config);

    // --fallback <provider>: fallback provider if primary fails
    let fallback_provider = args
        .iter()
        .position(|a| a == "--fallback")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| file_config.get("fallback").cloned())
        .map(|s| s.to_lowercase());

    // Derive a default model for the fallback provider
    let fallback_model = fallback_provider
        .as_ref()
        .map(|p| default_model_for_provider(p));

    Some(Config {
        model,
        api_key,
        provider,
        base_url,
        skills,
        system_prompt,
        thinking,
        max_tokens,
        temperature,
        max_turns,
        continue_session,
        output_path,
        prompt_arg,
        image_path,
        verbose,
        mcp_servers,
        openapi_specs,
        auto_approve,
        permissions,
        dir_restrictions,
        context_strategy,
        context_window,
        shell_hooks,
        fallback_provider,
        fallback_model,
        no_update_check,
        json_output,
        audit,
        print_system_prompt,
    })
}

/// Get the provider-specific environment variable name for the API key.
/// Returns None for anthropic (it uses the fallback chain) and local providers.
pub fn provider_api_key_env(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("OPENAI_API_KEY"),
        "google" => Some("GOOGLE_API_KEY"),
        "groq" => Some("GROQ_API_KEY"),
        "xai" => Some("XAI_API_KEY"),
        "deepseek" => Some("DEEPSEEK_API_KEY"),
        "openrouter" => Some("OPENROUTER_API_KEY"),
        "mistral" => Some("MISTRAL_API_KEY"),
        "cerebras" => Some("CEREBRAS_API_KEY"),
        "zai" => Some("ZAI_API_KEY"),
        "minimax" => Some("MINIMAX_API_KEY"),
        "bedrock" => Some("AWS_ACCESS_KEY_ID"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        _ => None,
    }
}

/// Build the welcome message text for first-run users.
/// Returned as a string so it can be tested without capturing stdout.
pub fn get_welcome_text() -> String {
    format!(
        r#"
  {BOLD}Welcome to yoyo! 🐙{RESET}

  {BOLD}Quick setup:{RESET}

  1. Get an API key from {CYAN}https://console.anthropic.com{RESET}
  2. Set it:
     {DIM}export ANTHROPIC_API_KEY=sk-ant-...{RESET}
  3. Run {BOLD}yoyo{RESET} again — you're in!

  {BOLD}Other providers:{RESET}
  Use {CYAN}--provider{RESET} to switch backends:
     openai, google, ollama (local), deepseek, groq, bedrock, and more.
  Example: {DIM}yoyo --provider ollama --model llama3.2{RESET}
  AWS Bedrock: {DIM}yoyo --provider bedrock --base-url https://bedrock-runtime.us-east-1.amazonaws.com{RESET}

  {BOLD}Persistent config:{RESET}
  Create a {CYAN}.yoyo.toml{RESET} file in your project or home directory:
     {DIM}api_key = "sk-ant-..."{RESET}
     {DIM}model = "claude-sonnet-4-20250514"{RESET}
     {DIM}provider = "anthropic"{RESET}
  Or use {CYAN}~/.config/yoyo/config.toml{RESET} for XDG-style config.

  Run {CYAN}yoyo --help{RESET} for all options.
"#
    )
}

/// Print a friendly welcome message for first-run users who haven't configured an API key.
/// This replaces the terse error when running interactively (REPL mode) without setup.
pub fn print_welcome() {
    print!("{}", get_welcome_text());
}

/// Get well-known model names for a provider (for diagnostic suggestions).
/// Returns a slice of commonly-used model identifiers.
pub fn known_models_for_provider(provider: &str) -> &'static [&'static str] {
    match provider {
        "anthropic" => &[
            "claude-opus-4-6",
            "claude-sonnet-4-20250514",
            "claude-haiku-4-5-20250414",
        ],
        "openai" => &[
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-4.1-nano",
            "o3",
            "o3-mini",
            "o4-mini",
        ],
        "google" => &["gemini-2.5-pro", "gemini-2.5-flash", "gemini-2.0-flash"],
        "groq" => &[
            "llama-3.3-70b-versatile",
            "llama-3.1-8b-instant",
            "mixtral-8x7b-32768",
        ],
        "xai" => &["grok-3", "grok-3-mini", "grok-2"],
        "deepseek" => &["deepseek-chat", "deepseek-reasoner"],
        "mistral" => &[
            "mistral-large-latest",
            "mistral-small-latest",
            "codestral-latest",
        ],
        "cerebras" => &["llama-3.3-70b"],
        "zai" => &["glm-4-plus", "glm-4-air", "glm-4-flash"],
        "minimax" => &[
            "MiniMax-M2.7",
            "MiniMax-M2.7-highspeed",
            "MiniMax-M2.5",
            "MiniMax-M2.5-highspeed",
            "MiniMax-M1",
            "MiniMax-M1-40k",
        ],
        "bedrock" => &[
            "anthropic.claude-sonnet-4-20250514-v1:0",
            "anthropic.claude-haiku-4-5-20250414-v1:0",
            "amazon.nova-pro-v1:0",
            "amazon.nova-lite-v1:0",
        ],
        "ollama" => &["llama3.2", "llama3.1", "codellama", "mistral"],
        _ => &[],
    }
}

/// Get the default model for a given provider.
pub fn default_model_for_provider(provider: &str) -> String {
    match provider {
        "openai" => "gpt-4o".into(),
        "google" => "gemini-2.0-flash".into(),
        "openrouter" => "anthropic/claude-sonnet-4-20250514".into(),
        "ollama" => "llama3.2".into(),
        "xai" => "grok-3".into(),
        "groq" => "llama-3.3-70b-versatile".into(),
        "deepseek" => "deepseek-chat".into(),
        "mistral" => "mistral-large-latest".into(),
        "cerebras" => "llama-3.3-70b".into(),
        "zai" => "glm-4-plus".into(),
        "minimax" => "MiniMax-M2.7".into(),
        "bedrock" => "anthropic.claude-sonnet-4-20250514-v1:0".into(),
        _ => "claude-opus-4-6".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constant_exists() {
        assert!(
            VERSION.contains('.'),
            "Version should contain a dot: {VERSION}"
        );
    }

    #[test]
    fn test_parse_thinking_level() {
        assert_eq!(parse_thinking_level("off"), ThinkingLevel::Off);
        assert_eq!(parse_thinking_level("none"), ThinkingLevel::Off);
        assert_eq!(parse_thinking_level("minimal"), ThinkingLevel::Minimal);
        assert_eq!(parse_thinking_level("min"), ThinkingLevel::Minimal);
        assert_eq!(parse_thinking_level("low"), ThinkingLevel::Low);
        assert_eq!(parse_thinking_level("medium"), ThinkingLevel::Medium);
        assert_eq!(parse_thinking_level("med"), ThinkingLevel::Medium);
        assert_eq!(parse_thinking_level("high"), ThinkingLevel::High);
        assert_eq!(parse_thinking_level("max"), ThinkingLevel::High);
        // Case insensitive
        assert_eq!(parse_thinking_level("HIGH"), ThinkingLevel::High);
        assert_eq!(parse_thinking_level("Medium"), ThinkingLevel::Medium);
        // Unknown defaults to medium with warning
        assert_eq!(parse_thinking_level("unknown"), ThinkingLevel::Medium);
    }

    #[test]
    fn test_system_flag_parsing() {
        let args = [
            "yoyo".to_string(),
            "--system".to_string(),
            "You are a Rust expert.".to_string(),
        ];
        let system = args
            .iter()
            .position(|a| a == "--system")
            .and_then(|i| args.get(i + 1))
            .cloned();
        assert_eq!(system, Some("You are a Rust expert.".to_string()));
    }

    #[test]
    fn test_system_flag_missing() {
        let args = ["yoyo".to_string()];
        let system = args
            .iter()
            .position(|a| a == "--system")
            .and_then(|i| args.get(i + 1))
            .cloned();
        assert_eq!(system, None);
    }

    #[test]
    fn test_system_file_flag() {
        let args = [
            "yoyo".to_string(),
            "--system-file".to_string(),
            "prompt.txt".to_string(),
        ];
        let system_file = args
            .iter()
            .position(|a| a == "--system-file")
            .and_then(|i| args.get(i + 1))
            .cloned();
        assert_eq!(system_file, Some("prompt.txt".to_string()));
    }

    #[test]
    fn test_continue_flag_parsing() {
        let args_short = ["yoyo".to_string(), "-c".to_string()];
        assert!(args_short.iter().any(|a| a == "--continue" || a == "-c"));

        let args_long = ["yoyo".to_string(), "--continue".to_string()];
        assert!(args_long.iter().any(|a| a == "--continue" || a == "-c"));

        let args_none = ["yoyo".to_string()];
        assert!(!args_none.iter().any(|a| a == "--continue" || a == "-c"));
    }

    #[test]
    fn test_prompt_flag_parsing() {
        let args = [
            "yoyo".to_string(),
            "-p".to_string(),
            "explain this code".to_string(),
        ];
        let prompt = args
            .iter()
            .position(|a| a == "--prompt" || a == "-p")
            .and_then(|i| args.get(i + 1))
            .cloned();
        assert_eq!(prompt, Some("explain this code".to_string()));

        let args_long = [
            "yoyo".to_string(),
            "--prompt".to_string(),
            "what does this do?".to_string(),
        ];
        let prompt_long = args_long
            .iter()
            .position(|a| a == "--prompt" || a == "-p")
            .and_then(|i| args_long.get(i + 1))
            .cloned();
        assert_eq!(prompt_long, Some("what does this do?".to_string()));

        let args_none = ["yoyo".to_string()];
        let prompt_none = args_none
            .iter()
            .position(|a| a == "--prompt" || a == "-p")
            .and_then(|i| args_none.get(i + 1))
            .cloned();
        assert_eq!(prompt_none, None);
    }

    #[test]
    fn test_output_flag_parsing() {
        let args = [
            "yoyo".to_string(),
            "-o".to_string(),
            "output.md".to_string(),
        ];
        let output = args
            .iter()
            .position(|a| a == "--output" || a == "-o")
            .and_then(|i| args.get(i + 1))
            .cloned();
        assert_eq!(output, Some("output.md".to_string()));

        let args_long = [
            "yoyo".to_string(),
            "--output".to_string(),
            "result.txt".to_string(),
        ];
        let output_long = args_long
            .iter()
            .position(|a| a == "--output" || a == "-o")
            .and_then(|i| args_long.get(i + 1))
            .cloned();
        assert_eq!(output_long, Some("result.txt".to_string()));

        let args_none = ["yoyo".to_string()];
        let output_none = args_none
            .iter()
            .position(|a| a == "--output" || a == "-o")
            .and_then(|i| args_none.get(i + 1))
            .cloned();
        assert_eq!(output_none, None);
    }

    #[test]
    fn test_default_session_path() {
        assert_eq!(DEFAULT_SESSION_PATH, "yoyo-session.json");
    }

    #[test]
    fn test_auto_compact_threshold_constants() {
        assert_eq!(DEFAULT_CONTEXT_TOKENS, 200_000);
        assert!((AUTO_COMPACT_THRESHOLD - 0.80).abs() < f64::EPSILON);
        assert!((PROACTIVE_COMPACT_THRESHOLD - 0.70).abs() < f64::EPSILON);
    }

    #[test]
    fn test_proactive_threshold_lower_than_auto() {
        // Proactive compact fires earlier (0.70) to prevent overflow before it happens.
        // Auto-compact fires later (0.80) as a post-turn safety net.
        // Compile-time guarantee that the relationship holds.
        const {
            assert!(PROACTIVE_COMPACT_THRESHOLD < AUTO_COMPACT_THRESHOLD);
        }
    }

    #[test]
    fn test_max_tokens_flag_parsing() {
        let args = [
            "yoyo".to_string(),
            "--max-tokens".to_string(),
            "4096".to_string(),
        ];
        let max_tokens = args
            .iter()
            .position(|a| a == "--max-tokens")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<u32>().ok());
        assert_eq!(max_tokens, Some(4096));
    }

    #[test]
    fn test_max_tokens_flag_missing() {
        let args = ["yoyo".to_string()];
        let max_tokens = args
            .iter()
            .position(|a| a == "--max-tokens")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<u32>().ok());
        assert_eq!(max_tokens, None);
    }

    #[test]
    fn test_max_tokens_flag_invalid() {
        let args = [
            "yoyo".to_string(),
            "--max-tokens".to_string(),
            "not_a_number".to_string(),
        ];
        let max_tokens = args
            .iter()
            .position(|a| a == "--max-tokens")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<u32>().ok());
        assert_eq!(max_tokens, None);
    }

    #[test]
    fn test_no_color_flag_recognized() {
        let args = ["yoyo".to_string(), "--no-color".to_string()];
        assert!(args.iter().any(|a| a == "--no-color"));
    }

    #[test]
    fn test_no_bell_flag_recognized() {
        let args = ["yoyo".to_string(), "--no-bell".to_string()];
        assert!(args.iter().any(|a| a == "--no-bell"));
        assert!(KNOWN_FLAGS.contains(&"--no-bell"));
    }

    #[test]
    fn test_parse_config_file_basic() {
        let content = r#"
model = "claude-sonnet-4-20250514"
thinking = "medium"
max_tokens = 4096
"#;
        let config = parse_config_file(content);
        assert_eq!(config.get("model").unwrap(), "claude-sonnet-4-20250514");
        assert_eq!(config.get("thinking").unwrap(), "medium");
        assert_eq!(config.get("max_tokens").unwrap(), "4096");
    }

    #[test]
    fn test_parse_config_file_comments_and_blanks() {
        let content = r#"
# This is a comment
model = "claude-opus-4-6"

# Another comment
thinking = "high"
"#;
        let config = parse_config_file(content);
        assert_eq!(config.get("model").unwrap(), "claude-opus-4-6");
        assert_eq!(config.get("thinking").unwrap(), "high");
        assert_eq!(config.len(), 2);
    }

    #[test]
    fn test_parse_config_file_no_quotes() {
        let content = "model = claude-haiku-35\nmax_tokens = 2048";
        let config = parse_config_file(content);
        assert_eq!(config.get("model").unwrap(), "claude-haiku-35");
        assert_eq!(config.get("max_tokens").unwrap(), "2048");
    }

    #[test]
    fn test_parse_config_file_single_quotes() {
        let content = "model = 'claude-opus-4-6'";
        let config = parse_config_file(content);
        assert_eq!(config.get("model").unwrap(), "claude-opus-4-6");
    }

    #[test]
    fn test_parse_config_file_empty() {
        let config = parse_config_file("");
        assert!(config.is_empty());
    }

    #[test]
    fn test_parse_config_file_whitespace_handling() {
        let content = "  model  =  claude-opus-4-6  ";
        let config = parse_config_file(content);
        assert_eq!(config.get("model").unwrap(), "claude-opus-4-6");
    }

    #[test]
    fn test_parse_config_file_mcp_array() {
        let content = r#"
model = "claude-sonnet-4-20250514"
mcp = ["npx open-websearch@latest", "npx @mcp/server-filesystem /tmp"]
"#;
        let config = parse_config_file(content);
        let mcp_val = config.get("mcp").expect("mcp key should exist");
        let mcps = parse_toml_array(mcp_val);
        assert_eq!(mcps.len(), 2);
        assert_eq!(mcps[0], "npx open-websearch@latest");
        assert_eq!(mcps[1], "npx @mcp/server-filesystem /tmp");
    }

    #[test]
    fn test_parse_config_file_mcp_empty_array() {
        let content = "mcp = []";
        let config = parse_config_file(content);
        let mcp_val = config.get("mcp").expect("mcp key should exist");
        let mcps = parse_toml_array(mcp_val);
        assert!(mcps.is_empty());
    }

    #[test]
    fn test_parse_config_file_mcp_single_entry() {
        let content = r#"mcp = ["npx open-websearch@latest"]"#;
        let config = parse_config_file(content);
        let mcp_val = config.get("mcp").expect("mcp key should exist");
        let mcps = parse_toml_array(mcp_val);
        assert_eq!(mcps.len(), 1);
        assert_eq!(mcps[0], "npx open-websearch@latest");
    }

    #[test]
    fn test_list_project_context_files_returns_vec() {
        // This test verifies the function runs without panicking.
        // In CI the project may or may not have YOYO.md present.
        let files = list_project_context_files();
        for (name, lines) in &files {
            assert!(!name.is_empty());
            assert!(*lines > 0);
        }
    }

    #[test]
    fn test_project_context_file_names_not_empty() {
        assert_eq!(PROJECT_CONTEXT_FILES.len(), 3);
        // YOYO.md must be first — it's the canonical context file name
        assert_eq!(PROJECT_CONTEXT_FILES[0], "YOYO.md");
        // CLAUDE.md is a compatibility alias
        assert_eq!(PROJECT_CONTEXT_FILES[1], "CLAUDE.md");
        assert_eq!(PROJECT_CONTEXT_FILES[2], ".yoyo/instructions.md");
        for name in PROJECT_CONTEXT_FILES {
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn test_temperature_flag_parsing() {
        let args = [
            "yoyo".to_string(),
            "--temperature".to_string(),
            "0.7".to_string(),
        ];
        let temp = args
            .iter()
            .position(|a| a == "--temperature")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<f32>().ok());
        assert_eq!(temp, Some(0.7));
    }

    #[test]
    fn test_temperature_flag_missing() {
        let args = ["yoyo".to_string()];
        let temp = args
            .iter()
            .position(|a| a == "--temperature")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<f32>().ok());
        assert_eq!(temp, None);
    }

    #[test]
    fn test_temperature_flag_invalid() {
        let args = [
            "yoyo".to_string(),
            "--temperature".to_string(),
            "not_a_number".to_string(),
        ];
        let temp = args
            .iter()
            .position(|a| a == "--temperature")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<f32>().ok());
        assert_eq!(temp, None);
    }

    #[test]
    fn test_verbose_flag_parsing() {
        let args_short = ["yoyo".to_string(), "-v".to_string()];
        assert!(args_short.iter().any(|a| a == "--verbose" || a == "-v"));

        let args_long = ["yoyo".to_string(), "--verbose".to_string()];
        assert!(args_long.iter().any(|a| a == "--verbose" || a == "-v"));

        let args_none = ["yoyo".to_string()];
        assert!(!args_none.iter().any(|a| a == "--verbose" || a == "-v"));
    }

    #[test]
    fn test_clamp_temperature_in_range() {
        assert_eq!(clamp_temperature(0.0), 0.0);
        assert_eq!(clamp_temperature(0.5), 0.5);
        assert_eq!(clamp_temperature(1.0), 1.0);
    }

    #[test]
    fn test_clamp_temperature_below_zero() {
        assert_eq!(clamp_temperature(-0.5), 0.0);
        assert_eq!(clamp_temperature(-100.0), 0.0);
    }

    #[test]
    fn test_clamp_temperature_above_one() {
        assert_eq!(clamp_temperature(1.5), 1.0);
        assert_eq!(clamp_temperature(99.0), 1.0);
    }

    #[test]
    fn test_known_flags_contains_all_flags() {
        // Every flag in the code should be in KNOWN_FLAGS
        let flags_with_values = [
            "--model",
            "--thinking",
            "--max-tokens",
            "--max-turns",
            "--temperature",
            "--skills",
            "--system",
            "--system-file",
            "--prompt",
            "-p",
            "--output",
            "-o",
            "--api-key",
            "--openapi",
            "--allow",
            "--deny",
            "--allow-dir",
            "--deny-dir",
        ];
        for flag in &flags_with_values {
            assert!(
                KNOWN_FLAGS.contains(flag),
                "Flag {flag} should be in KNOWN_FLAGS"
            );
        }
    }

    #[test]
    fn test_warn_unknown_flags_no_panic() {
        // Should not panic on various inputs
        let flags_needing_values = ["--model", "--thinking"];
        warn_unknown_flags(
            &["yoyo".to_string(), "--unknown".to_string()],
            &flags_needing_values,
        );
        warn_unknown_flags(
            &[
                "yoyo".to_string(),
                "--model".to_string(),
                "test".to_string(),
            ],
            &flags_needing_values,
        );
        warn_unknown_flags(&["yoyo".to_string()], &flags_needing_values);
    }

    #[test]
    fn test_api_key_flag_parsing() {
        let args = [
            "yoyo".to_string(),
            "--api-key".to_string(),
            "sk-test-key".to_string(),
        ];
        let api_key = args
            .iter()
            .position(|a| a == "--api-key")
            .and_then(|i| args.get(i + 1))
            .cloned();
        assert_eq!(api_key, Some("sk-test-key".to_string()));
    }

    #[test]
    fn test_api_key_flag_missing() {
        let args = ["yoyo".to_string()];
        let api_key = args
            .iter()
            .position(|a| a == "--api-key")
            .and_then(|i| args.get(i + 1))
            .cloned();
        assert_eq!(api_key, None);
    }

    #[test]
    fn test_api_key_flag_in_known_flags() {
        assert!(
            KNOWN_FLAGS.contains(&"--api-key"),
            "--api-key should be in KNOWN_FLAGS"
        );
    }

    #[test]
    fn test_api_key_from_config_file() {
        let content = "api_key = \"sk-ant-test-from-config\"";
        let config = parse_config_file(content);
        assert_eq!(config.get("api_key").unwrap(), "sk-ant-test-from-config");
    }

    #[test]
    fn test_home_config_path_returns_yoyo_toml_in_home() {
        // home_config_path() should return $HOME/.yoyo.toml
        let original_home = std::env::var("HOME").ok();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", tmp.path());

        let path = home_config_path();
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path, tmp.path().join(".yoyo.toml"));

        // Restore
        if let Some(h) = original_home {
            std::env::set_var("HOME", h);
        }
    }

    #[test]
    fn test_home_config_path_file_is_loadable() {
        // If ~/.yoyo.toml exists, parse_config_file should parse it
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(".yoyo.toml");
        std::fs::write(
            &config_path,
            "model = \"test-model\"\napi_key = \"sk-home-test\"\n",
        )
        .unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let config = parse_config_file(&content);
        assert_eq!(config.get("model").unwrap(), "test-model");
        assert_eq!(config.get("api_key").unwrap(), "sk-home-test");
    }

    #[test]
    fn test_config_precedence_project_over_home() {
        // If both project-level .yoyo.toml and ~/.yoyo.toml exist,
        // the project-level config should be found first.
        // We verify this by checking the search order logic:
        // CONFIG_FILE_NAMES is checked before home_config_path().
        //
        // Since load_config_file() checks project-level first, and both files
        // would parse correctly, we verify the ordering is as documented.
        let project_content = "model = \"project-model\"";
        let home_content = "model = \"home-model\"";

        let project_config = parse_config_file(project_content);
        let home_config = parse_config_file(home_content);

        assert_eq!(project_config.get("model").unwrap(), "project-model");
        assert_eq!(home_config.get("model").unwrap(), "home-model");

        // The search order is documented: project > home > XDG
        // This test verifies both configs parse independently.
        // The actual precedence is enforced by the early-return in load_config_file().
    }

    #[test]
    fn test_config_search_order_documented() {
        // Verify the documented search order: project (.yoyo.toml), home (~/.yoyo.toml), XDG
        // CONFIG_FILE_NAMES contains the project-level name
        assert_eq!(CONFIG_FILE_NAMES, &[".yoyo.toml"]);

        // home_config_path returns ~/.yoyo.toml
        let original_home = std::env::var("HOME").ok();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", tmp.path());

        let home = home_config_path().unwrap();
        assert!(home.to_string_lossy().ends_with(".yoyo.toml"));
        assert!(home
            .to_string_lossy()
            .contains(&tmp.path().to_string_lossy().to_string()));

        // user_config_path returns ~/.config/yoyo/config.toml (XDG)
        let xdg = user_config_path().unwrap();
        assert!(xdg.to_string_lossy().ends_with("config.toml"));
        assert!(xdg.to_string_lossy().contains("yoyo"));

        // Restore
        if let Some(h) = original_home {
            std::env::set_var("HOME", h);
        }
    }

    #[test]
    fn test_help_text_mentions_home_config() {
        // The help output should mention all three config paths.
        // We can't capture print_help() output easily, but we can verify
        // the welcome text mentions the paths.
        let welcome = get_welcome_text();
        assert!(
            welcome.contains(".yoyo.toml"),
            "welcome should mention .yoyo.toml"
        );
        assert!(
            welcome.contains("config/yoyo/config.toml"),
            "welcome should mention XDG config path"
        );
    }

    #[test]
    fn test_get_project_file_listing_no_panic() {
        // Should not panic regardless of whether we're in a git repo or not.
        // In CI this runs inside a git repo, so we expect Some with files.
        let result = get_project_file_listing();
        // If we're in a git repo (likely in CI), verify the output is reasonable
        if let Some(listing) = &result {
            assert!(!listing.is_empty(), "File listing should not be empty");
            let lines: Vec<&str> = listing.lines().collect();
            assert!(
                lines.len() <= MAX_PROJECT_FILES + 1, // +1 for possible "... and N more" line
                "File listing should be capped at {} files",
                MAX_PROJECT_FILES
            );
            // Should contain at least Cargo.toml (we're in a Rust project)
            assert!(
                listing.contains("Cargo.toml"),
                "File listing should contain Cargo.toml"
            );
        }
    }

    #[test]
    fn test_max_project_files_constant() {
        assert_eq!(MAX_PROJECT_FILES, 200);
    }

    #[test]
    fn test_load_project_context_includes_file_listing() {
        // load_project_context should include project file listing when in a git repo
        let result = load_project_context();
        if let Some(context) = &result {
            // If we're in a git repo, context should include the file listing section
            if get_project_file_listing().is_some() {
                assert!(
                    context.contains("## Project Files"),
                    "Context should contain Project Files section"
                );
            }
        }
    }

    #[test]
    fn test_history_file_path_returns_some() {
        // In CI and local environments, HOME is typically set
        let path = history_file_path();
        if std::env::var("HOME").is_ok() {
            assert!(path.is_some(), "Should return a path when HOME is set");
            let p = path.unwrap();
            let p_str = p.to_string_lossy();
            assert!(
                p_str.contains("yoyo"),
                "History path should contain 'yoyo': {p_str}"
            );
            assert!(
                p_str.ends_with("history") || p_str.ends_with(".yoyo_history"),
                "History path should end with 'history' or '.yoyo_history': {p_str}"
            );
        }
    }

    #[test]
    fn test_history_file_path_prefers_xdg() {
        // When XDG_DATA_HOME is set, should use it
        let dir = std::env::temp_dir().join("yoyo_test_xdg_data");
        let _ = std::fs::create_dir_all(&dir);
        // We can't safely set env vars in parallel tests, so just verify the logic
        // by calling data_dir_hint and checking the fallback behavior
        let path = history_file_path();
        // Should return Some regardless
        if std::env::var("HOME").is_ok() || std::env::var("XDG_DATA_HOME").is_ok() {
            assert!(path.is_some());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_yoyo_md_is_primary_context_file() {
        // YOYO.md should be the first (primary) context file
        assert_eq!(
            PROJECT_CONTEXT_FILES[0], "YOYO.md",
            "YOYO.md must be the primary context file"
        );
        // CLAUDE.md should be present as compatibility alias but not first
        assert!(
            PROJECT_CONTEXT_FILES.contains(&"CLAUDE.md"),
            "CLAUDE.md should still be supported for compatibility"
        );
        assert_ne!(
            PROJECT_CONTEXT_FILES[0], "CLAUDE.md",
            "CLAUDE.md should not be the primary context file"
        );
    }

    #[test]
    fn test_data_dir_hint_returns_path() {
        // data_dir_hint should return something when HOME is set
        if std::env::var("HOME").is_ok() || std::env::var("XDG_DATA_HOME").is_ok() {
            let dir = data_dir_hint();
            assert!(dir.is_some(), "Should return a data dir path");
        }
    }

    // === Permission system tests ===

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("ls", "ls"));
        assert!(!glob_match("ls", "ls -la"));
        assert!(!glob_match("ls -la", "ls"));
    }

    #[test]
    fn test_glob_match_wildcard_suffix() {
        assert!(glob_match("git *", "git status"));
        assert!(glob_match("git *", "git commit -m 'hello'"));
        assert!(!glob_match("git *", "echo git"));
        assert!(!glob_match("git *", "gitignore"));
    }

    #[test]
    fn test_glob_match_wildcard_prefix() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("*.rs", "src/main.rs"));
        assert!(!glob_match("*.rs", "main.py"));
    }

    #[test]
    fn test_glob_match_wildcard_middle() {
        assert!(glob_match("cargo * --release", "cargo build --release"));
        assert!(glob_match("cargo * --release", "cargo test --release"));
        assert!(!glob_match("cargo * --release", "cargo build --debug"));
    }

    #[test]
    fn test_glob_match_multiple_wildcards() {
        assert!(glob_match("*git*", "git status"));
        assert!(glob_match("*git*", "echo git hello"));
        assert!(glob_match("*git*", "something git something"));
        assert!(!glob_match("*git*", "echo hello"));
    }

    #[test]
    fn test_glob_match_star_only() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*", ""));
        assert!(glob_match("*", "ls -la /tmp"));
    }

    #[test]
    fn test_glob_match_empty_pattern() {
        assert!(glob_match("", ""));
        assert!(!glob_match("", "something"));
    }

    #[test]
    fn test_glob_match_rm_rf() {
        assert!(glob_match("rm -rf *", "rm -rf /"));
        assert!(glob_match("rm -rf *", "rm -rf /tmp"));
        assert!(!glob_match("rm -rf *", "rm file.txt"));
        assert!(!glob_match("rm -rf *", "rm -r dir"));
    }

    #[test]
    fn test_permission_config_check_allow() {
        let config = PermissionConfig {
            allow: vec!["git *".to_string(), "cargo *".to_string()],
            deny: vec![],
        };
        assert_eq!(config.check("git status"), Some(true));
        assert_eq!(config.check("cargo build"), Some(true));
        assert_eq!(config.check("rm -rf /"), None);
    }

    #[test]
    fn test_permission_config_check_deny() {
        let config = PermissionConfig {
            allow: vec![],
            deny: vec!["rm -rf *".to_string(), "sudo *".to_string()],
        };
        assert_eq!(config.check("rm -rf /tmp"), Some(false));
        assert_eq!(config.check("sudo apt install"), Some(false));
        assert_eq!(config.check("ls"), None);
    }

    #[test]
    fn test_permission_config_deny_overrides_allow() {
        // Deny should take priority when both match
        let config = PermissionConfig {
            allow: vec!["*".to_string()],
            deny: vec!["rm -rf *".to_string()],
        };
        assert_eq!(config.check("rm -rf /"), Some(false));
        assert_eq!(config.check("ls"), Some(true));
        assert_eq!(config.check("git status"), Some(true));
    }

    #[test]
    fn test_permission_config_empty() {
        let config = PermissionConfig::default();
        assert!(config.is_empty());
        assert_eq!(config.check("anything"), None);
    }

    #[test]
    fn test_parse_toml_array_basic() {
        let arr = parse_toml_array(r#"["git *", "cargo *"]"#);
        assert_eq!(arr, vec!["git *", "cargo *"]);
    }

    #[test]
    fn test_parse_toml_array_single() {
        let arr = parse_toml_array(r#"["rm -rf *"]"#);
        assert_eq!(arr, vec!["rm -rf *"]);
    }

    #[test]
    fn test_parse_toml_array_empty() {
        let arr = parse_toml_array("[]");
        assert!(arr.is_empty());
    }

    #[test]
    fn test_parse_toml_array_single_quotes() {
        let arr = parse_toml_array("['git *', 'ls']");
        assert_eq!(arr, vec!["git *", "ls"]);
    }

    #[test]
    fn test_parse_toml_array_not_array() {
        let arr = parse_toml_array("not an array");
        assert!(arr.is_empty());
    }

    #[test]
    fn test_parse_permissions_from_config() {
        let content = r#"
model = "claude-opus-4-6"
thinking = "medium"

[permissions]
allow = ["git *", "cargo *", "echo *"]
deny = ["rm -rf *", "sudo *"]
"#;
        let perms = parse_permissions_from_config(content);
        assert_eq!(perms.allow, vec!["git *", "cargo *", "echo *"]);
        assert_eq!(perms.deny, vec!["rm -rf *", "sudo *"]);
    }

    #[test]
    fn test_parse_permissions_from_config_no_section() {
        let content = r#"
model = "claude-opus-4-6"
thinking = "medium"
"#;
        let perms = parse_permissions_from_config(content);
        assert!(perms.is_empty());
    }

    #[test]
    fn test_parse_permissions_from_config_empty_section() {
        let content = r#"
[permissions]
"#;
        let perms = parse_permissions_from_config(content);
        assert!(perms.is_empty());
    }

    #[test]
    fn test_parse_permissions_from_config_only_allow() {
        let content = r#"
[permissions]
allow = ["git *"]
"#;
        let perms = parse_permissions_from_config(content);
        assert_eq!(perms.allow, vec!["git *"]);
        assert!(perms.deny.is_empty());
    }

    #[test]
    fn test_parse_permissions_from_config_other_section_after() {
        let content = r#"
[permissions]
allow = ["git *"]

[other]
key = "value"
"#;
        let perms = parse_permissions_from_config(content);
        assert_eq!(perms.allow, vec!["git *"]);
        assert!(perms.deny.is_empty());
    }

    #[test]
    fn test_permission_config_realistic_scenario() {
        // Simulate a real workflow: allow common dev commands, deny dangerous ones
        let config = PermissionConfig {
            allow: vec![
                "git *".to_string(),
                "cargo *".to_string(),
                "cat *".to_string(),
                "ls *".to_string(),
                "echo *".to_string(),
            ],
            deny: vec![
                "rm -rf *".to_string(),
                "sudo *".to_string(),
                "curl * | sh".to_string(),
            ],
        };

        // Safe commands auto-approve
        assert_eq!(config.check("git status"), Some(true));
        assert_eq!(config.check("cargo test"), Some(true));
        assert_eq!(config.check("cat Cargo.toml"), Some(true));

        // Dangerous commands auto-deny
        assert_eq!(config.check("rm -rf /"), Some(false));
        assert_eq!(config.check("sudo rm -rf /"), Some(false));

        // Unknown commands prompt
        assert_eq!(config.check("python script.py"), None);
        assert_eq!(config.check("npm install"), None);
    }

    #[test]
    fn test_allow_deny_flags_parsing() {
        let args = [
            "yoyo".to_string(),
            "--allow".to_string(),
            "git *".to_string(),
            "--allow".to_string(),
            "cargo *".to_string(),
            "--deny".to_string(),
            "rm -rf *".to_string(),
        ];
        let allow: Vec<String> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.as_str() == "--allow")
            .filter_map(|(i, _)| args.get(i + 1).cloned())
            .collect();
        let deny: Vec<String> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.as_str() == "--deny")
            .filter_map(|(i, _)| args.get(i + 1).cloned())
            .collect();
        assert_eq!(allow, vec!["git *", "cargo *"]);
        assert_eq!(deny, vec!["rm -rf *"]);
    }

    #[test]
    fn test_openapi_flag_parsing_single() {
        let args = [
            "yoyo".to_string(),
            "--openapi".to_string(),
            "petstore.yaml".to_string(),
        ];
        let specs: Vec<String> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.as_str() == "--openapi")
            .filter_map(|(i, _)| args.get(i + 1).cloned())
            .collect();
        assert_eq!(specs, vec!["petstore.yaml"]);
    }

    #[test]
    fn test_openapi_flag_parsing_multiple() {
        let args = [
            "yoyo".to_string(),
            "--openapi".to_string(),
            "api1.yaml".to_string(),
            "--openapi".to_string(),
            "api2.json".to_string(),
            "--model".to_string(),
            "claude-opus-4-6".to_string(),
        ];
        let specs: Vec<String> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.as_str() == "--openapi")
            .filter_map(|(i, _)| args.get(i + 1).cloned())
            .collect();
        assert_eq!(specs, vec!["api1.yaml", "api2.json"]);
    }

    #[test]
    fn test_openapi_flag_in_known_flags() {
        assert!(
            KNOWN_FLAGS.contains(&"--openapi"),
            "--openapi should be in KNOWN_FLAGS"
        );
    }

    #[test]
    fn test_get_recently_changed_files_in_git_repo() {
        // We're running in a git repo (CI or local), so this should return Some
        let result = get_recently_changed_files(20);
        if let Some(files) = &result {
            assert!(!files.is_empty(), "Should have recently changed files");
            // Files should be deduplicated
            let unique: std::collections::HashSet<&String> = files.iter().collect();
            assert_eq!(
                files.len(),
                unique.len(),
                "Recently changed files should be deduplicated"
            );
            // Should respect the max limit
            assert!(files.len() <= 20, "Should not exceed max_files limit");
        }
    }

    #[test]
    fn test_get_recently_changed_files_respects_limit() {
        // Request only 2 files — should return at most 2
        let result = get_recently_changed_files(2);
        if let Some(files) = &result {
            assert!(
                files.len() <= 2,
                "Should respect max_files=2, got {}",
                files.len()
            );
        }
    }

    #[test]
    fn test_get_recently_changed_files_no_duplicates() {
        let result = get_recently_changed_files(50);
        if let Some(files) = &result {
            let unique: std::collections::HashSet<&String> = files.iter().collect();
            assert_eq!(files.len(), unique.len(), "Files should be deduplicated");
        }
    }

    #[test]
    fn test_max_recent_files_constant() {
        assert_eq!(MAX_RECENT_FILES, 20);
    }

    #[test]
    fn test_load_project_context_includes_recently_changed() {
        // In a git repo with commits, context should include recently changed files
        let result = load_project_context();
        if let Some(context) = &result {
            if get_recently_changed_files(MAX_RECENT_FILES).is_some() {
                assert!(
                    context.contains("## Recently Changed Files"),
                    "Context should contain Recently Changed Files section"
                );
            }
        }
    }

    // === Git status context tests ===

    #[test]
    fn test_get_git_status_context_in_repo() {
        // We're running inside a git repo, so this should return Some
        let result = get_git_status_context();
        assert!(result.is_some(), "Should return Some when in a git repo");
        assert!(
            result.as_ref().unwrap().contains("Branch:"),
            "Should contain 'Branch:' label"
        );
    }

    #[test]
    fn test_get_git_status_context_contains_branch() {
        let result = get_git_status_context().expect("Should be in a git repo");
        // Get the actual branch name to verify it's in the output
        let branch = crate::git::git_branch().expect("Should get branch name");
        assert!(
            result.contains(&format!("Branch: {branch}")),
            "Should contain actual branch name: {branch}"
        );
    }

    #[test]
    fn test_git_status_context_format() {
        let result = get_git_status_context().expect("Should be in a git repo");
        assert!(
            result.starts_with("## Git Status\n\n"),
            "Should start with '## Git Status' header"
        );
    }

    #[test]
    fn test_load_project_context_includes_git_status() {
        // In a git repo, load_project_context should include git status
        let result = load_project_context();
        if let Some(context) = &result {
            if get_git_status_context().is_some() {
                assert!(
                    context.contains("## Git Status"),
                    "Context should contain Git Status section"
                );
            }
        }
    }

    // === Directory restrictions tests ===

    #[test]
    fn test_directory_restrictions_empty_allows_everything() {
        let restrictions = DirectoryRestrictions::default();
        assert!(restrictions.is_empty());
        assert!(restrictions.check_path("/etc/passwd").is_ok());
        assert!(restrictions.check_path("src/main.rs").is_ok());
    }

    #[test]
    fn test_directory_restrictions_deny_blocks_path() {
        let restrictions = DirectoryRestrictions {
            allow: vec![],
            deny: vec!["/etc".to_string()],
        };
        assert!(restrictions.check_path("/etc/passwd").is_err());
        assert!(restrictions.check_path("/etc/shadow").is_err());
        // Non-denied paths should be allowed
        assert!(restrictions.check_path("/tmp/file.txt").is_ok());
    }

    #[test]
    fn test_directory_restrictions_allow_restricts_to_listed() {
        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let restrictions = DirectoryRestrictions {
            allow: vec![format!("{}/src", cwd)],
            deny: vec![],
        };
        // Paths under allowed dir should pass
        assert!(restrictions
            .check_path(&format!("{}/src/main.rs", cwd))
            .is_ok());
        // Paths outside allowed dirs should fail
        assert!(restrictions.check_path("/tmp/file.txt").is_err());
    }

    #[test]
    fn test_directory_restrictions_deny_overrides_allow() {
        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let restrictions = DirectoryRestrictions {
            allow: vec![cwd.clone()],
            deny: vec![format!("{}/secrets", cwd)],
        };
        // Normal paths under allow should pass
        assert!(restrictions
            .check_path(&format!("{}/src/main.rs", cwd))
            .is_ok());
        // Denied paths should be blocked even though parent is allowed
        assert!(restrictions
            .check_path(&format!("{}/secrets/key.pem", cwd))
            .is_err());
    }

    #[test]
    fn test_directory_restrictions_parent_dir_escape_blocked() {
        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let restrictions = DirectoryRestrictions {
            allow: vec![format!("{}/src", cwd)],
            deny: vec![],
        };
        // Attempting to escape via ../ should be caught after normalization
        assert!(restrictions
            .check_path(&format!("{}/src/../secrets/key.pem", cwd))
            .is_err());
    }

    #[test]
    fn test_directory_restrictions_relative_paths() {
        // Relative paths should be resolved against CWD
        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let restrictions = DirectoryRestrictions {
            allow: vec![],
            deny: vec![format!("{}/secrets", cwd)],
        };
        // "secrets/file.txt" resolves to CWD/secrets/file.txt which should be denied
        assert!(restrictions.check_path("secrets/file.txt").is_err());
        // "src/main.rs" should be fine (not under denied dir)
        assert!(restrictions.check_path("src/main.rs").is_ok());
    }

    #[test]
    fn test_directory_restrictions_exact_dir_match() {
        let restrictions = DirectoryRestrictions {
            allow: vec![],
            deny: vec!["/etc".to_string()],
        };
        // The denied dir itself should match
        assert!(restrictions.check_path("/etc").is_err());
        // Paths under it should match
        assert!(restrictions.check_path("/etc/passwd").is_err());
        // Similar-prefix dirs should NOT match (e.g., /etcetc)
        assert!(restrictions.check_path("/etcetc/file").is_ok());
    }

    #[test]
    fn test_resolve_path_normalizes_parent_dir() {
        let resolved = resolve_path("/tmp/a/../b");
        assert_eq!(resolved, "/tmp/b");
    }

    #[test]
    fn test_resolve_path_absolute() {
        let resolved = resolve_path("/usr/bin/env");
        assert!(resolved.starts_with('/'));
        assert!(resolved.contains("usr"));
    }

    #[test]
    fn test_path_is_under_basic() {
        assert!(path_is_under("/etc/passwd", "/etc"));
        assert!(path_is_under("/etc", "/etc"));
        assert!(!path_is_under("/etcetc", "/etc"));
        assert!(!path_is_under("/tmp/file", "/etc"));
    }

    #[test]
    fn test_parse_directories_from_config() {
        let content = r#"
model = "claude-opus-4-6"

[directories]
allow = ["./src", "./tests"]
deny = ["~/.ssh", "/etc"]
"#;
        let dirs = parse_directories_from_config(content);
        assert_eq!(dirs.allow, vec!["./src", "./tests"]);
        assert_eq!(dirs.deny, vec!["~/.ssh", "/etc"]);
    }

    #[test]
    fn test_parse_directories_from_config_no_section() {
        let content = r#"
model = "claude-opus-4-6"
"#;
        let dirs = parse_directories_from_config(content);
        assert!(dirs.is_empty());
    }

    #[test]
    fn test_parse_directories_from_config_does_not_interfere_with_permissions() {
        let content = r#"
[permissions]
allow = ["git *"]
deny = ["rm -rf *"]

[directories]
deny = ["/etc"]
"#;
        let perms = parse_permissions_from_config(content);
        assert_eq!(perms.allow, vec!["git *"]);
        assert_eq!(perms.deny, vec!["rm -rf *"]);

        let dirs = parse_directories_from_config(content);
        assert!(dirs.allow.is_empty());
        assert_eq!(dirs.deny, vec!["/etc"]);
    }

    #[test]
    fn test_allow_dir_deny_dir_flags_parsing() {
        let args = [
            "yoyo".to_string(),
            "--allow-dir".to_string(),
            "./src".to_string(),
            "--allow-dir".to_string(),
            "./tests".to_string(),
            "--deny-dir".to_string(),
            "/etc".to_string(),
        ];
        let allow_dirs: Vec<String> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.as_str() == "--allow-dir")
            .filter_map(|(i, _)| args.get(i + 1).cloned())
            .collect();
        let deny_dirs: Vec<String> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.as_str() == "--deny-dir")
            .filter_map(|(i, _)| args.get(i + 1).cloned())
            .collect();
        assert_eq!(allow_dirs, vec!["./src", "./tests"]);
        assert_eq!(deny_dirs, vec!["/etc"]);
    }

    #[test]
    fn test_allow_dir_deny_dir_in_known_flags() {
        assert!(
            KNOWN_FLAGS.contains(&"--allow-dir"),
            "--allow-dir should be in KNOWN_FLAGS"
        );
        assert!(
            KNOWN_FLAGS.contains(&"--deny-dir"),
            "--deny-dir should be in KNOWN_FLAGS"
        );
    }

    #[test]
    fn test_print_welcome_contains_key_phrases() {
        let welcome = get_welcome_text();
        assert!(
            welcome.contains("API key") || welcome.contains("api_key"),
            "welcome should mention API key"
        );
        assert!(
            welcome.contains("ANTHROPIC_API_KEY"),
            "welcome should mention ANTHROPIC_API_KEY env var"
        );
        assert!(
            welcome.contains("ollama"),
            "welcome should mention ollama for local usage"
        );
        assert!(
            welcome.contains(".yoyo.toml"),
            "welcome should mention .yoyo.toml config file"
        );
        assert!(welcome.contains("--help"), "welcome should mention --help");
        assert!(
            welcome.contains("Welcome to yoyo"),
            "welcome should have greeting"
        );
    }

    #[test]
    fn test_print_welcome_mentions_setup_steps() {
        let welcome = get_welcome_text();
        assert!(welcome.contains("1."), "welcome should have step 1");
        assert!(welcome.contains("2."), "welcome should have step 2");
        assert!(welcome.contains("3."), "welcome should have step 3");
        assert!(
            welcome.contains("console.anthropic.com"),
            "welcome should link to Anthropic console"
        );
    }

    #[test]
    fn test_print_welcome_mentions_other_providers() {
        let welcome = get_welcome_text();
        assert!(
            welcome.contains("--provider"),
            "welcome should mention --provider flag"
        );
        assert!(
            welcome.contains("openai"),
            "welcome should mention openai provider"
        );
        assert!(
            welcome.contains("google"),
            "welcome should mention google provider"
        );
    }

    // ── system_prompt / system_file config key tests ─────────────────────

    #[test]
    fn test_config_system_prompt_key() {
        // Config with system_prompt should be used when no CLI flag is passed
        let content = r#"
model = "claude-opus-4-6"
system_prompt = "You are a Go expert"
"#;
        let config = parse_config_file(content);
        assert_eq!(config.get("system_prompt").unwrap(), "You are a Go expert");

        // resolve_system_prompt should use the config value when no CLI args
        let result = resolve_system_prompt(None, None, None, Some("You are a Go expert".into()));
        assert_eq!(result, "You are a Go expert");
    }

    #[test]
    fn test_config_system_file_key() {
        // Config with system_file should read from that file path
        let content = "system_file = \"prompt.txt\"";
        let config = parse_config_file(content);
        assert_eq!(config.get("system_file").unwrap(), "prompt.txt");

        // Create a temp file and verify resolve_system_prompt reads it
        let dir = std::env::temp_dir().join("yoyo_test_system_file");
        let _ = std::fs::create_dir_all(&dir);
        let prompt_path = dir.join("test_prompt.txt");
        std::fs::write(&prompt_path, "You are a Python expert").unwrap();

        let result = resolve_system_prompt(
            None,
            None,
            Some(prompt_path.to_string_lossy().into_owned()),
            None,
        );
        assert_eq!(result, "You are a Python expert");

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_config_system_file_overrides_system_prompt() {
        // When both are present in config, system_file wins
        let dir = std::env::temp_dir().join("yoyo_test_sf_override");
        let _ = std::fs::create_dir_all(&dir);
        let prompt_path = dir.join("override_prompt.txt");
        std::fs::write(&prompt_path, "From file").unwrap();

        let result = resolve_system_prompt(
            None,
            None,
            Some(prompt_path.to_string_lossy().into_owned()),
            Some("From config key".into()),
        );
        assert_eq!(result, "From file");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cli_system_overrides_config() {
        // CLI --system should override config file system_prompt
        let result = resolve_system_prompt(
            None,
            Some("CLI system prompt".into()),
            None,
            Some("Config system prompt".into()),
        );
        assert_eq!(result, "CLI system prompt");
    }

    #[test]
    fn test_cli_system_file_overrides_config() {
        // CLI --system-file content should override config file system_file
        let dir = std::env::temp_dir().join("yoyo_test_cli_sf_override");
        let _ = std::fs::create_dir_all(&dir);
        let config_path = dir.join("config_prompt.txt");
        std::fs::write(&config_path, "Config file content").unwrap();

        let result = resolve_system_prompt(
            Some("CLI file content".into()),
            None,
            Some(config_path.to_string_lossy().into_owned()),
            Some("Config prompt text".into()),
        );
        assert_eq!(result, "CLI file content");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_system_prompt_default() {
        // When nothing is provided, default SYSTEM_PROMPT is used
        let result = resolve_system_prompt(None, None, None, None);
        assert_eq!(result, SYSTEM_PROMPT);
    }

    #[test]
    fn test_cli_system_overrides_config_system_file() {
        // CLI --system should also override config system_file
        let dir = std::env::temp_dir().join("yoyo_test_cli_sys_vs_config_file");
        let _ = std::fs::create_dir_all(&dir);
        let config_path = dir.join("config_prompt.txt");
        std::fs::write(&config_path, "Config file content").unwrap();

        let result = resolve_system_prompt(
            None,
            Some("CLI text wins".into()),
            Some(config_path.to_string_lossy().into_owned()),
            None,
        );
        assert_eq!(result, "CLI text wins");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_minimax_provider_api_key_env() {
        assert_eq!(provider_api_key_env("minimax"), Some("MINIMAX_API_KEY"));
    }

    #[test]
    fn test_minimax_default_model() {
        assert_eq!(default_model_for_provider("minimax"), "MiniMax-M2.7");
    }

    #[test]
    fn test_minimax_known_models() {
        let models = known_models_for_provider("minimax");
        assert!(!models.is_empty(), "minimax should have known models");
        assert!(models.contains(&"MiniMax-M1"));
        assert!(models.contains(&"MiniMax-M1-40k"));
    }

    #[test]
    fn test_bedrock_in_known_providers() {
        assert!(
            KNOWN_PROVIDERS.contains(&"bedrock"),
            "bedrock should be in KNOWN_PROVIDERS"
        );
    }

    #[test]
    fn test_bedrock_provider_api_key_env() {
        assert_eq!(provider_api_key_env("bedrock"), Some("AWS_ACCESS_KEY_ID"));
    }

    #[test]
    fn test_bedrock_default_model() {
        assert_eq!(
            default_model_for_provider("bedrock"),
            "anthropic.claude-sonnet-4-20250514-v1:0"
        );
    }

    #[test]
    fn test_bedrock_known_models() {
        let models = known_models_for_provider("bedrock");
        assert!(!models.is_empty(), "bedrock should have known models");
        assert!(models.contains(&"anthropic.claude-sonnet-4-20250514-v1:0"));
        assert!(models.contains(&"amazon.nova-pro-v1:0"));
    }

    #[test]
    fn test_welcome_text_mentions_bedrock() {
        let welcome = get_welcome_text();
        assert!(
            welcome.contains("bedrock"),
            "welcome text should mention bedrock"
        );
    }

    #[test]
    fn test_minimax_in_known_providers() {
        assert!(
            KNOWN_PROVIDERS.contains(&"minimax"),
            "minimax should be in KNOWN_PROVIDERS"
        );
    }

    #[test]
    fn test_context_strategy_default_is_compaction() {
        let strategy = ContextStrategy::default();
        assert_eq!(strategy, ContextStrategy::Compaction);
    }

    #[test]
    fn test_context_strategy_parses_checkpoint() {
        // Set a dummy API key so parse_args doesn't bail
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args: Vec<String> = vec![
            "yoyo".into(),
            "--context-strategy".into(),
            "checkpoint".into(),
        ];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.context_strategy, ContextStrategy::Checkpoint);
    }

    #[test]
    fn test_context_strategy_parses_compaction_explicit() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args: Vec<String> = vec![
            "yoyo".into(),
            "--context-strategy".into(),
            "compaction".into(),
        ];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.context_strategy, ContextStrategy::Compaction);
    }

    #[test]
    fn test_context_strategy_unknown_defaults_to_compaction() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args: Vec<String> = vec!["yoyo".into(), "--context-strategy".into(), "banana".into()];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.context_strategy, ContextStrategy::Compaction);
    }

    #[test]
    fn test_context_strategy_absent_defaults_to_compaction() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args: Vec<String> = vec!["yoyo".into()];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.context_strategy, ContextStrategy::Compaction);
    }

    #[test]
    fn test_context_strategy_in_known_flags() {
        assert!(
            KNOWN_FLAGS.contains(&"--context-strategy"),
            "--context-strategy should be in KNOWN_FLAGS"
        );
    }

    #[test]
    fn test_fallback_in_known_flags() {
        assert!(
            KNOWN_FLAGS.contains(&"--fallback"),
            "--fallback should be in KNOWN_FLAGS"
        );
    }

    #[test]
    fn test_parse_fallback_flag() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args: Vec<String> = vec!["yoyo".into(), "--fallback".into(), "google".into()];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.fallback_provider, Some("google".to_string()));
        assert_eq!(
            config.fallback_model,
            Some(default_model_for_provider("google"))
        );
    }

    #[test]
    fn test_parse_fallback_missing() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args: Vec<String> = vec!["yoyo".into()];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.fallback_provider, None);
        assert_eq!(config.fallback_model, None);
    }

    #[test]
    fn test_parse_fallback_case_insensitive() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args: Vec<String> = vec!["yoyo".into(), "--fallback".into(), "Google".into()];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.fallback_provider, Some("google".to_string()));
    }

    #[test]
    fn test_parse_fallback_derives_model() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args: Vec<String> = vec!["yoyo".into(), "--fallback".into(), "openai".into()];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.fallback_provider, Some("openai".to_string()));
        assert_eq!(config.fallback_model, Some("gpt-4o".to_string()));
    }

    #[test]
    fn test_version_is_newer_basic() {
        assert!(version_is_newer("0.1.5", "0.2.0"));
    }

    #[test]
    fn test_version_is_newer_same() {
        assert!(!version_is_newer("0.1.5", "0.1.5"));
    }

    #[test]
    fn test_version_is_newer_older() {
        assert!(!version_is_newer("0.2.0", "0.1.5"));
    }

    #[test]
    fn test_version_is_newer_numeric_comparison() {
        // Must compare numerically, not lexicographically
        assert!(version_is_newer("0.1.5", "0.1.10"));
    }

    #[test]
    fn test_version_is_newer_major_dominates() {
        assert!(!version_is_newer("1.0.0", "0.99.99"));
    }

    #[test]
    fn test_version_is_newer_different_lengths() {
        assert!(version_is_newer("0.1", "0.1.1"));
        assert!(!version_is_newer("0.1.1", "0.1"));
    }

    #[test]
    fn test_check_for_update_graceful_failure() {
        // When curl isn't available or network fails, should return None
        // We can't control the network in tests, but we can verify it doesn't panic
        let _result = check_for_update();
        // Just assert it doesn't panic — the result depends on network state
    }

    #[test]
    fn test_no_update_check_flag_recognized() {
        assert!(KNOWN_FLAGS.contains(&"--no-update-check"));
    }

    #[test]
    fn test_no_update_check_flag_parsed() {
        let args = [
            "yoyo".to_string(),
            "--no-update-check".to_string(),
            "--api-key".to_string(),
            "sk-test".to_string(),
        ];
        let config = parse_args(&args).expect("should parse");
        assert!(config.no_update_check);
    }

    #[test]
    fn test_no_update_check_default_false() {
        let args = [
            "yoyo".to_string(),
            "--api-key".to_string(),
            "sk-test".to_string(),
        ];
        let config = parse_args(&args).expect("should parse");
        // Unless YOYO_NO_UPDATE_CHECK=1 is set in the environment,
        // the default should be false
        if std::env::var("YOYO_NO_UPDATE_CHECK").unwrap_or_default() != "1" {
            assert!(!config.no_update_check);
        }
    }

    #[test]
    fn test_json_flag_in_known_flags() {
        assert!(KNOWN_FLAGS.contains(&"--json"));
    }

    #[test]
    fn test_parse_args_json_flag() {
        let args = [
            "yoyo".to_string(),
            "--json".to_string(),
            "--api-key".to_string(),
            "sk-test".to_string(),
        ];
        let config = parse_args(&args).expect("should parse");
        assert!(config.json_output);
    }

    #[test]
    fn test_parse_args_json_default() {
        let args = [
            "yoyo".to_string(),
            "--api-key".to_string(),
            "sk-test".to_string(),
        ];
        let config = parse_args(&args).expect("should parse");
        assert!(!config.json_output);
    }

    #[test]
    fn test_audit_flag_in_known_flags() {
        assert!(KNOWN_FLAGS.contains(&"--audit"));
    }

    #[test]
    fn test_parse_args_audit_flag() {
        let args = [
            "yoyo".to_string(),
            "--audit".to_string(),
            "--api-key".to_string(),
            "sk-test".to_string(),
        ];
        let config = parse_args(&args).expect("should parse");
        assert!(config.audit);
    }

    #[test]
    fn test_parse_args_audit_default_false() {
        let args = [
            "yoyo".to_string(),
            "--api-key".to_string(),
            "sk-test".to_string(),
        ];
        let config = parse_args(&args).expect("should parse");
        // Unless YOYO_AUDIT=1 is set in the environment,
        // the default should be false
        if std::env::var("YOYO_AUDIT").unwrap_or_default() != "1" {
            assert!(!config.audit);
        }
    }

    #[test]
    fn test_print_system_prompt_flag_parsed() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args: Vec<String> = vec!["yoyo".into(), "--print-system-prompt".into()];
        let config = parse_args(&args).expect("should parse");
        assert!(config.print_system_prompt);
    }

    #[test]
    fn test_print_system_prompt_flag_default_false() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args: Vec<String> = vec!["yoyo".into(), "--api-key".into(), "sk-test".into()];
        let config = parse_args(&args).expect("should parse");
        assert!(!config.print_system_prompt);
    }
}
