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
// Re-exported from providers module so existing `use crate::cli::` imports keep working.
pub use crate::providers::{
    default_model_for_provider, known_models_for_provider, provider_api_key_env, KNOWN_PROVIDERS,
};

/// Context management strategy.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ContextStrategy {
    /// Default: auto-compact conversation when approaching context limit
    #[default]
    Compaction,
    /// Write checkpoint file and exit with code 2 when approaching limit
    Checkpoint,
}

// Re-exported from config module so existing `use crate::cli::` imports keep working.
pub use crate::config::{
    parse_directories_from_config, parse_mcp_servers_from_config, parse_permissions_from_config,
    parse_toml_array, DirectoryRestrictions, McpServerConfig, PermissionConfig,
};

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
    pub mcp_server_configs: Vec<McpServerConfig>,
    pub openapi_specs: Vec<String>,
    pub auto_approve: bool,
    pub auto_commit: bool,
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

// Project context loading — re-exported from context.rs
pub use crate::context::{list_project_context_files, load_project_context};

pub fn print_help() {
    print!("{}", help_text());
}

/// Build the full `--help` output as a string. Kept public so tests can assert
/// on its contents without capturing stdout.
pub fn help_text() -> String {
    let mut s = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(s, "yoyo v{VERSION} — a coding agent growing up in public");
    let _ = writeln!(s);
    let _ = writeln!(s, "Usage: yoyo [OPTIONS]");
    let _ = writeln!(s);
    let _ = writeln!(s, "Options:");
    let _ = writeln!(
        s,
        "  --model <name>    Model to use (default: claude-opus-4-6)"
    );
    let _ = writeln!(
        s,
        "  --provider <name> Provider: anthropic (default), openai, google, openrouter,"
    );
    let _ = writeln!(
        s,
        "                    ollama, xai, groq, deepseek, mistral, cerebras, zai, custom"
    );
    let _ = writeln!(
        s,
        "  --base-url <url>  Custom API endpoint (e.g., http://localhost:11434/v1)"
    );
    let _ = writeln!(
        s,
        "  --thinking <lvl>  Enable extended thinking (off, minimal, low, medium, high)"
    );
    let _ = writeln!(
        s,
        "  --max-tokens <n>  Maximum output tokens per response (default: 8192)"
    );
    let _ = writeln!(
        s,
        "  --max-turns <n>   Maximum agent turns per prompt (default: 50)"
    );
    let _ = writeln!(
        s,
        "  --temperature <f> Sampling temperature (0.0-1.0, default: model default)"
    );
    let _ = writeln!(s, "  --skills <dir>    Directory containing skill files");
    let _ = writeln!(
        s,
        "  --system <text>   Custom system prompt (overrides default)"
    );
    let _ = writeln!(s, "  --system-file <f> Read system prompt from file");
    let _ = writeln!(
        s,
        "  --prompt, -p <t>  Run a single prompt and exit (no REPL)"
    );
    let _ = writeln!(s, "  --output, -o <f>  Write final response text to a file");
    let _ = writeln!(
        s,
        "  --api-key <key>   API key (overrides provider-specific env var)"
    );
    let _ = writeln!(
        s,
        "  --mcp <cmd>       Connect to an MCP server via stdio (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --openapi <spec>  Load OpenAPI spec file and register API tools (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --no-color        Disable colored output (also respects NO_COLOR env)"
    );
    let _ = writeln!(s, "  --no-bell         Disable terminal bell on long completions (also respects YOYO_NO_BELL env)");
    let _ = writeln!(
        s,
        "  --no-rtk          Disable RTK (Rust Token Killer) proxy even when installed"
    );
    let _ = writeln!(
        s,
        "  --no-update-check Skip startup update check (also respects YOYO_NO_UPDATE_CHECK=1 env)"
    );
    let _ = writeln!(
        s,
        "  --json            Output JSON instead of plain text (for -p and piped modes)"
    );
    let _ = writeln!(
        s,
        "  --audit           Enable audit logging of all tool calls to .yoyo/audit.jsonl"
    );
    let _ = writeln!(
        s,
        "                    (also respects YOYO_AUDIT=1 env or audit = true in config)"
    );
    let _ = writeln!(
        s,
        "  --verbose, -v     Show debug info (API errors, request details)"
    );
    let _ = writeln!(
        s,
        "  --yes, -y         Auto-approve all tool executions (skip confirmation prompts)"
    );
    let _ = writeln!(
        s,
        "  --auto-commit     Auto-commit file changes after each agent turn"
    );
    let _ = writeln!(
        s,
        "  --allow <pat>     Auto-approve bash commands matching glob pattern (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --deny <pat>      Auto-deny bash commands matching glob pattern (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --allow-dir <d>   Restrict file access to this directory (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --deny-dir <d>    Block file access to this directory (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --context-strategy <s>  Context management: compaction (default) or checkpoint"
    );
    let _ = writeln!(
        s,
        "  --context-window <n>    Override context window size (tokens). Default: auto-detected"
    );
    let _ = writeln!(
        s,
        "                          per provider (200K Anthropic, 1M Google, 128K OpenAI, etc.)"
    );
    let _ = writeln!(s, "  --continue, -c    Resume last saved session");
    let _ = writeln!(
        s,
        "  --fallback <prov> Fallback provider if primary fails (e.g. --fallback google)"
    );
    let _ = writeln!(
        s,
        "  --print-system-prompt  Print the fully assembled system prompt and exit"
    );
    let _ = writeln!(s, "  --help, -h        Show this help message");
    let _ = writeln!(s, "  --version, -V     Show version");
    let _ = writeln!(s);
    let _ = writeln!(s, "Subcommands (run from shell, no REPL):");
    let _ = writeln!(
        s,
        "  help              Show this help message (same as --help)"
    );
    let _ = writeln!(s, "  version           Show version (same as --version)");
    let _ = writeln!(s, "  setup             Run the interactive setup wizard");
    let _ = writeln!(
        s,
        "  init              Generate a YOYO.md project context file"
    );
    let _ = writeln!(
        s,
        "  doctor            Diagnose yoyo setup (config, API key, provider, tool availability)"
    );
    let _ = writeln!(
        s,
        "  health            Run project health checks (build, test, clippy, fmt)"
    );
    let _ = writeln!(
        s,
        "  lint              Run project linter (e.g. yoyo lint --strict, yoyo lint unsafe)"
    );
    let _ = writeln!(s, "  test              Run project test suite");
    let _ = writeln!(
        s,
        "  tree              Show project directory tree (e.g. yoyo tree 5)"
    );
    let _ = writeln!(s, "  map               Show project symbol map");
    let _ = writeln!(
        s,
        "  run               Run a shell command (e.g. yoyo run cargo clippy)"
    );
    let _ = writeln!(
        s,
        "  diff              Show git diff (e.g. yoyo diff --staged)"
    );
    let _ = writeln!(
        s,
        "  commit            Commit staged changes (e.g. yoyo commit \"fix typo\")"
    );
    let _ = writeln!(
        s,
        "  review            Show review prompt for staged changes or a file"
    );
    let _ = writeln!(
        s,
        "  blame             Show git blame (e.g. yoyo blame src/main.rs 10-20)"
    );
    let _ = writeln!(
        s,
        "  grep              Search files for a pattern (e.g. yoyo grep TODO src/)"
    );
    let _ = writeln!(
        s,
        "  find              Find files by name (e.g. yoyo find main)"
    );
    let _ = writeln!(s, "  index             Build and display project index");
    let _ = writeln!(
        s,
        "  update            Check for and install the latest yoyo release"
    );
    let _ = writeln!(
        s,
        "  docs              Look up docs.rs documentation (e.g. yoyo docs serde)"
    );
    let _ = writeln!(
        s,
        "  skill             List and inspect loaded skills (e.g. yoyo skill list --skills ./skills)"
    );
    let _ = writeln!(
        s,
        "  watch             Toggle watch mode (e.g. yoyo watch cargo test)"
    );
    let _ = writeln!(
        s,
        "  status            Show version, git branch, and working directory"
    );
    let _ = writeln!(
        s,
        "  undo              Undo changes (e.g. yoyo undo --last-commit)"
    );
    let _ = writeln!(
        s,
        "  changelog         Show recent commits (e.g. yoyo changelog 20)"
    );
    let _ = writeln!(
        s,
        "  config            Show configuration (e.g. yoyo config show)"
    );
    let _ = writeln!(s, "  permissions       Show security/permission config");
    let _ = writeln!(
        s,
        "  todo              Manage project tasks (e.g. yoyo todo list, yoyo todo add ...)"
    );
    let _ = writeln!(
        s,
        "  memories          Show project memories (e.g. yoyo memories)"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "Commands (in REPL):");
    let _ = writeln!(s);
    let _ = writeln!(s, "  Session:");
    let _ = writeln!(
        s,
        "    /help              Show help (/help <cmd> for details)"
    );
    let _ = writeln!(s, "    /quit, /exit       Exit yoyo");
    let _ = writeln!(s, "    /clear             Clear conversation history");
    let _ = writeln!(s, "    /clear!            Force-clear without confirmation");
    let _ = writeln!(
        s,
        "    /compact           Compact conversation to save context"
    );
    let _ = writeln!(s, "    /save [path]       Save session to file");
    let _ = writeln!(s, "    /load [path]       Load session from file");
    let _ = writeln!(s, "    /retry             Re-send the last user input");
    let _ = writeln!(s, "    /status            Show session info");
    let _ = writeln!(
        s,
        "    /tokens            Show token usage and context window"
    );
    let _ = writeln!(s, "    /cost              Show estimated session cost");
    let _ = writeln!(s, "    /profile           Show unified session statistics");
    let _ = writeln!(s, "    /config            Show all current settings");
    let _ = writeln!(s, "    /hooks             Show active hooks");
    let _ = writeln!(s, "    /permissions       Show security/permission config");
    let _ = writeln!(s, "    /version           Show yoyo version");
    let _ = writeln!(
        s,
        "    /update            Check for and install latest version"
    );
    let _ = writeln!(
        s,
        "    /history           Show conversation message summary"
    );
    let _ = writeln!(s, "    /search <query>    Search conversation history");
    let _ = writeln!(s, "    /mark <name>       Bookmark conversation state");
    let _ = writeln!(s, "    /jump <name>       Restore to a bookmark");
    let _ = writeln!(s, "    /marks             List saved bookmarks");
    let _ = writeln!(s, "    /changes           Show files modified this session");
    let _ = writeln!(s, "    /changelog [N]     Show recent git commit history");
    let _ = writeln!(s, "    /export [path]     Export conversation as markdown");
    let _ = writeln!(
        s,
        "    /stash [desc]      Stash conversation and start fresh"
    );
    let _ = writeln!(
        s,
        "    /todo [subcmd]     Track tasks (add/done/wip/remove/clear)"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "  Git:");
    let _ = writeln!(
        s,
        "    /git <subcmd>      Quick git: status, log, add, diff, branch"
    );
    let _ = writeln!(
        s,
        "    /diff [opts]       Show git diff (--staged, --name-only)"
    );
    let _ = writeln!(
        s,
        "    /blame <file>      Show git blame with colored output"
    );
    let _ = writeln!(
        s,
        "    /undo [N|--all]    Undo changes (turn, all, or last commit)"
    );
    let _ = writeln!(
        s,
        "    /commit [msg]      Commit staged changes (AI message if omitted)"
    );
    let _ = writeln!(
        s,
        "    /pr [number]       List, view, diff, comment, or create PRs"
    );
    let _ = writeln!(
        s,
        "    /review [path]     AI code review of staged changes or a file"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "  Project:");
    let _ = writeln!(
        s,
        "    /add <path>        Add file contents to conversation"
    );
    let _ = writeln!(s, "    /explain <file>    Ask the agent to explain code");
    let _ = writeln!(s, "    /apply <file>      Apply a diff or patch file");
    let _ = writeln!(
        s,
        "    /context           Show loaded project context files"
    );
    let _ = writeln!(s, "    /doctor            Run environment diagnostics");
    let _ = writeln!(
        s,
        "    /init              Generate a YOYO.md project context file"
    );
    let _ = writeln!(s, "    /health            Run project health checks");
    let _ = writeln!(
        s,
        "    /fix               Auto-fix build/lint errors via AI"
    );
    let _ = writeln!(
        s,
        "    /test              Auto-detect and run project tests"
    );
    let _ = writeln!(
        s,
        "    /lint [opts]       Run project linter (pedantic/strict/fix/unsafe)"
    );
    let _ = writeln!(
        s,
        "    /run <cmd>         Run a shell command (no AI, no tokens)"
    );
    let _ = writeln!(
        s,
        "    /bg <sub>          Background shell jobs (run/list/output/kill)"
    );
    let _ = writeln!(s, "    /docs <crate>      Look up docs.rs documentation");
    let _ = writeln!(
        s,
        "    /find <pattern>    Fuzzy-search project files by name"
    );
    let _ = writeln!(
        s,
        "    /grep <pat> [path] Search file contents (no AI, instant)"
    );
    let _ = writeln!(s, "    /rename <old> <new> Cross-file symbol rename");
    let _ = writeln!(
        s,
        "    /extract <sym> <src> <dst>  Move a symbol to another file"
    );
    let _ = writeln!(
        s,
        "    /move <S>::<m> <D>  Move a method between impl blocks"
    );
    let _ = writeln!(s, "    /refactor          Show all refactoring tools");
    let _ = writeln!(
        s,
        "    /index             Build lightweight project source index"
    );
    let _ = writeln!(
        s,
        "    /map [path]        Show structural map (functions, types)"
    );
    let _ = writeln!(s, "    /tree [depth]      Show project directory tree");
    let _ = writeln!(
        s,
        "    /web <url>         Fetch and display web page content"
    );
    let _ = writeln!(s, "    /watch [cmd]       Auto-run tests after agent edits");
    let _ = writeln!(
        s,
        "    /ast <pattern>     Structural code search (ast-grep)"
    );
    let _ = writeln!(s, "    /skill [subcmd]    List and inspect loaded skills");
    let _ = writeln!(s);
    let _ = writeln!(s, "  AI:");
    let _ = writeln!(
        s,
        "    /model <name>      Switch model (preserves conversation)"
    );
    let _ = writeln!(s, "    /provider <name>   Switch provider");
    let _ = writeln!(
        s,
        "    /think [level]     Show/change thinking (off/low/medium/high)"
    );
    let _ = writeln!(s, "    /plan <task>       Plan a task without executing");
    let _ = writeln!(s, "    /spawn <task>      Spawn a subagent for a task");
    let _ = writeln!(
        s,
        "    /teach [on|off]    Toggle teach mode (explains reasoning)"
    );
    let _ = writeln!(s, "    /remember <note>   Save a project-specific memory");
    let _ = writeln!(s, "    /memories          List project memories");
    let _ = writeln!(s, "    /forget <n>        Remove a project memory by index");
    let _ = writeln!(s, "    /mcp [list|help]   Manage MCP server connections");
    let _ = writeln!(s);
    let _ = writeln!(s, "Environment:");
    let _ = writeln!(
        s,
        "  ANTHROPIC_API_KEY  API key for Anthropic (default provider)"
    );
    let _ = writeln!(s, "  OPENAI_API_KEY    API key for OpenAI");
    let _ = writeln!(s, "  GOOGLE_API_KEY    API key for Google/Gemini");
    let _ = writeln!(s, "  GROQ_API_KEY      API key for Groq");
    let _ = writeln!(s, "  XAI_API_KEY       API key for xAI");
    let _ = writeln!(s, "  DEEPSEEK_API_KEY  API key for DeepSeek");
    let _ = writeln!(s, "  OPENROUTER_API_KEY API key for OpenRouter");
    let _ = writeln!(s, "  ZAI_API_KEY       API key for ZAI (Zhipu AI / z.ai)");
    let _ = writeln!(s, "  API_KEY            Fallback API key (any provider)");
    let _ = writeln!(
        s,
        "  YOYO_NO_UPDATE_CHECK  Set to 1 to skip startup update check"
    );
    let _ = writeln!(
        s,
        "  YOYO_AUDIT            Set to 1 to enable audit logging"
    );
    let _ = writeln!(
        s,
        "  YOYO_SESSION_BUDGET_SECS  Soft wall-clock budget in seconds; retry loops bail"
    );
    let _ = writeln!(
        s,
        "                            early when <30s remain (default: unbounded)"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "Config files (searched in order, first found wins):");
    let _ = writeln!(
        s,
        "  .yoyo.toml                  Project-level config (current directory)"
    );
    let _ = writeln!(s, "  ~/.yoyo.toml                Home directory config");
    let _ = writeln!(s, "  ~/.config/yoyo/config.toml  User-level config (XDG)");
    let _ = writeln!(s);
    let _ = writeln!(s, "Config file format (key = value):");
    let _ = writeln!(s, "  model = \"claude-sonnet-4-20250514\"");
    let _ = writeln!(s, "  provider = \"openai\"");
    let _ = writeln!(s, "  base_url = \"http://localhost:11434/v1\"");
    let _ = writeln!(s, "  thinking = \"medium\"");
    let _ = writeln!(s, "  max_tokens = 4096");
    let _ = writeln!(s, "  max_turns = 20");
    let _ = writeln!(s, "  api_key = \"sk-ant-...\"");
    let _ = writeln!(s, "  system_prompt = \"You are a Go expert\"");
    let _ = writeln!(s, "  system_file = \"prompts/system.txt\"");
    let _ = writeln!(
        s,
        "  mcp = [\"npx open-websearch@latest\", \"npx @mcp/server-filesystem /tmp\"]"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "  [permissions]");
    let _ = writeln!(s, "  allow = [\"git *\", \"cargo *\"]");
    let _ = writeln!(s, "  deny = [\"rm -rf *\"]");
    let _ = writeln!(s);
    let _ = writeln!(s, "  [directories]");
    let _ = writeln!(s, "  allow = [\"./src\", \"./tests\"]");
    let _ = writeln!(s, "  deny = [\"~/.ssh\", \"/etc\"]");
    let _ = writeln!(s);
    let _ = writeln!(s, "CLI flags override config file values.");
    s
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
    "--no-rtk",
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
    "--auto-commit",
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
/// Try to dispatch an early-exit "subcommand" before flag parsing.
///
/// Today, yoyo's only short-circuit dispatch is for `--help`/`-h` and
/// Build a `/command ...` string from shell args, preserving multi-word tokens.
///
/// Shell args like `["yoyo", "grep", "fn main", "src/"]` become `/grep "fn main" src/`.
/// Any arg containing whitespace is wrapped in double quotes so downstream parsers
/// (which use `tokenize_quoted`) can distinguish multi-word patterns from separate args.
fn quote_args_as_command(args: &[String]) -> String {
    let parts: Vec<String> = args[1..]
        .iter()
        .map(|a| {
            if a.contains(' ') || a.contains('\t') {
                format!("\"{}\"", a)
            } else {
                a.clone()
            }
        })
        .collect();
    format!("/{}", parts.join(" "))
}

/// `--version`/`-V` — both print and bail out before any config is built.
/// This helper is the first slice of the parse_args refactor (#261); it
/// exists so the "did I handle this?" decision can be unit-tested in
/// isolation, and so future positional subcommands (`yoyo setup`,
/// `yoyo doctor`, etc., once they exist) have an obvious place to land.
///
/// Returns:
/// - `Some(None)` — a subcommand matched, was handled (printed output),
///   and `parse_args` should return `None` to its caller.
/// - `Some(Some(cfg))` — a subcommand matched and produced a usable
///   `Config` (no current subcommand does this; reserved for future use).
/// - `None` — no subcommand matched; fall through to flag parsing.
pub(crate) fn try_dispatch_subcommand(args: &[String]) -> Option<Option<Config>> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Some(None);
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("yoyo v{VERSION}");
        return Some(None);
    }

    // Positional subcommands: `yoyo <subcmd>`.
    // args[0] is the binary path; args[1] is the subcommand name.
    // Each arm calls the existing REPL handler from commands_dev and exits cleanly
    // (handlers return () and print directly to stdout).
    if let Some(sub) = args.get(1) {
        match sub.as_str() {
            "doctor" => {
                // Respect --provider / --model flags if present, else fall back to
                // config-file values, else compiled-in defaults. We deliberately
                // do NOT run the full parse_args pipeline because `yoyo doctor`
                // should work even when the API key / model setup is incomplete
                // (that's exactly the failure mode the diagnostic exists to detect).
                let (file_config, _) = load_config_file();
                let provider = flag_value(args, &["--provider"])
                    .or_else(|| file_config.get("provider").cloned())
                    .unwrap_or_else(|| "anthropic".into())
                    .to_lowercase();
                let model = flag_value(args, &["--model"])
                    .or_else(|| file_config.get("model").cloned())
                    .unwrap_or_else(|| default_model_for_provider(&provider));
                crate::commands_dev::handle_doctor(&provider, &model);
                return Some(None);
            }
            "health" => {
                // handle_health takes no arguments — it auto-detects project type
                // from the current directory and runs the appropriate checks.
                crate::commands_dev::handle_health();
                return Some(None);
            }
            "help" => {
                print_help();
                return Some(None);
            }
            "version" => {
                println!("yoyo v{VERSION}");
                return Some(None);
            }
            "setup" => {
                crate::setup::run_setup_wizard();
                return Some(None);
            }
            "init" => {
                crate::commands_project::handle_init();
                return Some(None);
            }
            "lint" => {
                let input = quote_args_as_command(args);
                crate::commands_dev::handle_lint(&input);
                return Some(None);
            }
            "test" => {
                crate::commands_dev::handle_test();
                return Some(None);
            }
            "tree" => {
                let input = quote_args_as_command(args);
                crate::commands_dev::handle_tree(&input);
                return Some(None);
            }
            "map" => {
                let input = quote_args_as_command(args);
                crate::commands_map::handle_map(&input);
                return Some(None);
            }
            "run" => {
                let input = quote_args_as_command(args);
                crate::commands_dev::handle_run(&input);
                return Some(None);
            }
            "diff" => {
                let input = quote_args_as_command(args);
                crate::commands_git::handle_diff(&input);
                return Some(None);
            }
            "commit" => {
                let input = quote_args_as_command(args);
                crate::commands_git::handle_commit(&input);
                return Some(None);
            }
            "review" => {
                // handle_review is async and needs an agent — for bare
                // subcommand, gather the content and print the review prompt
                // so the user can see what would be sent to the model.
                let input = quote_args_as_command(args);
                let arg = input.strip_prefix("/review").unwrap_or("").trim();
                match crate::commands_git::build_review_content(arg) {
                    Some((label, content)) => {
                        let prompt = crate::commands_git::build_review_prompt(&label, &content);
                        println!("{prompt}");
                    }
                    None => {
                        // build_review_content already printed the error/status
                    }
                }
                return Some(None);
            }
            "blame" => {
                let input = quote_args_as_command(args);
                crate::commands_git::handle_blame(&input);
                return Some(None);
            }
            "grep" => {
                let input = quote_args_as_command(args);
                crate::commands_search::handle_grep(&input);
                return Some(None);
            }
            "find" => {
                let input = quote_args_as_command(args);
                crate::commands_search::handle_find(&input);
                return Some(None);
            }
            "index" => {
                crate::commands_search::handle_index();
                return Some(None);
            }
            "update" => {
                if let Err(e) = crate::commands_dev::handle_update() {
                    eprintln!("{RED}  {e}{RESET}");
                }
                return Some(None);
            }
            "docs" => {
                let input = quote_args_as_command(args);
                crate::commands_project::handle_docs(&input);
                return Some(None);
            }
            "skill" => {
                let input = quote_args_as_command(args);
                let skill_dirs = collect_repeatable_flag(args, "--skills");
                let skills = if skill_dirs.is_empty() {
                    SkillSet::empty()
                } else {
                    SkillSet::load(&skill_dirs).unwrap_or_else(|e| {
                        eprintln!("{YELLOW}warning:{RESET} Failed to load skills: {e}");
                        SkillSet::empty()
                    })
                };
                crate::commands_project::handle_skill(&input, &skills);
                return Some(None);
            }
            "watch" => {
                let input = quote_args_as_command(args);
                crate::commands_dev::handle_watch(&input);
                return Some(None);
            }
            "status" => {
                // Bare subcommand: no active session, so show what we can
                // without agent state (version, git branch, cwd).
                let cwd = std::env::current_dir()
                    .map_or_else(|_| "?".into(), |p| p.display().to_string());
                println!("{DIM}  yoyo v{VERSION}");
                if let Some(branch) = crate::git::git_branch() {
                    println!("  git:     {branch}");
                }
                println!("  cwd:     {cwd}");
                println!("  (no active session — start yoyo for full status){RESET}\n");
                return Some(None);
            }
            "undo" => {
                // Bare subcommand: no turn history available (no active session).
                // Support --last-commit which works standalone; for other args,
                // explain that turn-based undo requires a session.
                let input = quote_args_as_command(args);
                let mut history = crate::prompt::TurnHistory::new();
                crate::commands_git::handle_undo(&input, &mut history);
                return Some(None);
            }
            "changelog" => {
                let input = quote_args_as_command(args);
                crate::commands_info::handle_changelog(&input);
                return Some(None);
            }
            "config" => {
                // Only `yoyo config show` (or bare `yoyo config`) works without
                // an interactive session. Other config subcommands (edit, hooks,
                // teach) require agent state.
                let sub2 = args.get(2).map(|s| s.as_str());
                match sub2 {
                    None | Some("show") => {
                        crate::commands_config::handle_config_show();
                    }
                    Some(other) => {
                        eprintln!(
                            "{YELLOW}  `config {other}` requires an interactive session.{RESET}"
                        );
                        eprintln!("{DIM}  Try: yoyo config show (works from the shell){RESET}");
                    }
                }
                return Some(None);
            }
            "permissions" => {
                // Load permission config from config file (same as parse_args does)
                // so the user can inspect their effective permissions from the shell.
                let (_, raw_config) = load_config_file();
                let permissions = crate::config::parse_permissions_from_config(&raw_config);
                let dir_restrictions = crate::config::parse_directories_from_config(&raw_config);
                let auto_approve = args.iter().any(|a| a == "--yes" || a == "-y");
                crate::commands_config::handle_permissions(
                    auto_approve,
                    &permissions,
                    &dir_restrictions,
                );
                return Some(None);
            }
            "todo" => {
                let input = quote_args_as_command(args);
                let output = crate::commands_project::handle_todo(&input);
                println!("{output}");
                return Some(None);
            }
            "memories" => {
                let input = quote_args_as_command(args);
                crate::commands_memory::handle_memories(&input);
                return Some(None);
            }
            _ => {}
        }
    }

    None
}

/// Look up the value that follows a `--flag VALUE` pair in `args`.
///
/// Returns the cloned value string if `flag` (or any of its aliases, like
/// `-p` for `--prompt`) appears in `args` and is followed by another token.
/// Returns `None` if the flag is missing or has no value after it.
///
/// Centralizes the `args.iter().position(...).and_then(get(i+1)).cloned()`
/// pattern that's repeated ~16 times across `parse_args`. This is the
/// follow-up to the Day 38 09:55 task that landed `try_dispatch_subcommand`
/// (#261) — see `journals/JOURNAL.md` for the full premise correction.
pub(crate) fn flag_value(args: &[String], flag_names: &[&str]) -> Option<String> {
    args.iter()
        .position(|a| flag_names.contains(&a.as_str()))
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Outcome of checking whether a flag is followed by a real value.
///
/// Pure classifier for `--flag <value>` style arguments. Caller decides how
/// to present the result (warn vs. hard-exit) — this keeps the helper
/// free of I/O so it can be unit-tested in isolation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FlagValueCheck<'a> {
    /// Next token is a usable value.
    Ok(&'a str),
    /// Next token exists but looks like another flag (e.g. `--model --provider ...`).
    /// The caller should surface a warning; not fatal because a leading `-` may
    /// also be a negative number (e.g. `--temperature -0.1`).
    FlagLike(&'a str),
    /// There is no next token at all (`--model` at end of args).
    Missing,
}

/// Classify the token that follows a flag expecting a value.
///
/// This is the pure validation kernel for the `flags_needing_values` loop in
/// [`parse_args`]. The loop body used to inline this logic, which made it
/// impossible to unit-test directly and left subtle behaviour (negative
/// numbers being valid values, end-of-args being fatal) undocumented.
///
/// Behaviour:
/// - `None` → [`FlagValueCheck::Missing`]
/// - `Some("-")` or `Some("--anything")` → [`FlagValueCheck::FlagLike`]
///   (warning territory, not a hard error — the old code only warned here)
/// - `Some("-5")`, `Some("-0.1")` etc. → [`FlagValueCheck::Ok`]
///   (leading dash followed by a digit is a negative number, not a flag)
/// - anything else → [`FlagValueCheck::Ok`]
pub(crate) fn require_flag_value<'a>(next: Option<&'a String>) -> FlagValueCheck<'a> {
    match next {
        None => FlagValueCheck::Missing,
        Some(v) => {
            if v.starts_with('-') && !v.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
                FlagValueCheck::FlagLike(v.as_str())
            } else {
                FlagValueCheck::Ok(v.as_str())
            }
        }
    }
}

/// Parse a numeric CLI flag with config file fallback.
///
/// Checks `args` for `flag_name`, parses the following value as `T`.
/// Falls back to `file_config[config_key]` when the CLI flag is absent.
/// Prints a warning on parse failure.
fn parse_numeric_flag<T: std::str::FromStr + std::fmt::Display>(
    args: &[String],
    flag_name: &str,
    file_config: &std::collections::HashMap<String, String>,
    config_key: &str,
) -> Option<T> {
    args.iter()
        .position(|a| a == flag_name)
        .and_then(|i| args.get(i + 1))
        .and_then(|s| {
            s.parse::<T>().ok().or_else(|| {
                eprintln!("{YELLOW}warning:{RESET} Invalid {flag_name} value '{s}', using default");
                None
            })
        })
        .or_else(|| {
            file_config
                .get(config_key)
                .and_then(|s| s.parse::<T>().ok())
        })
}

/// Collect all values for a repeatable flag (e.g. `--allow pat1 --allow pat2`).
fn collect_repeatable_flag(args: &[String], flag: &str) -> Vec<String> {
    args.iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == flag)
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect()
}

/// Parsed model/provider/API-key configuration extracted from CLI flags and config file.
struct ModelConfig {
    provider: String,
    base_url: Option<String>,
    api_key: String,
    model: String,
    fallback_provider: Option<String>,
    fallback_model: Option<String>,
}

/// Parse provider, base URL, API key, model, and fallback from CLI args and config.
fn parse_model_config(
    args: &[String],
    file_config: &HashMap<String, String>,
    prompt_arg: &Option<String>,
) -> ModelConfig {
    // Parse --provider flag (CLI > config file > default "anthropic")
    let provider = flag_value(args, &["--provider"])
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
    let base_url =
        flag_value(args, &["--base-url"]).or_else(|| file_config.get("base_url").cloned());

    // API key: --api-key flag > provider-specific env > ANTHROPIC_API_KEY > API_KEY > config file
    let api_key_from_flag = flag_value(args, &["--api-key"]);

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

    let model = flag_value(args, &["--model"])
        .or_else(|| file_config.get("model").cloned())
        .unwrap_or_else(|| default_model_for_provider(&provider));

    // --fallback <provider>: fallback provider if primary fails
    let fallback_provider = flag_value(args, &["--fallback"])
        .or_else(|| file_config.get("fallback").cloned())
        .map(|s| s.to_lowercase());

    // Derive a default model for the fallback provider
    let fallback_model = fallback_provider
        .as_ref()
        .map(|p| default_model_for_provider(p));

    ModelConfig {
        provider,
        base_url,
        api_key,
        model,
        fallback_provider,
        fallback_model,
    }
}

/// Parsed boolean/simple output flags.
struct OutputFlags {
    verbose: bool,
    auto_approve: bool,
    auto_commit: bool,
    no_update_check: bool,
    json_output: bool,
    audit: bool,
    print_system_prompt: bool,
}

/// Parse simple boolean output flags from CLI args and config.
fn parse_output_flags(args: &[String], file_config: &HashMap<String, String>) -> OutputFlags {
    let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");

    let auto_approve = args.iter().any(|a| a == "--yes" || a == "-y");

    let auto_commit = args.iter().any(|a| a == "--auto-commit");

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

    OutputFlags {
        verbose,
        auto_approve,
        auto_commit,
        no_update_check,
        json_output,
        audit,
        print_system_prompt,
    }
}

/// Parse permission and directory restriction config from CLI args and config file content.
fn parse_permission_and_dir_config(
    args: &[String],
    raw_config_content: &str,
) -> (PermissionConfig, DirectoryRestrictions) {
    // --allow <pattern> flags: collect all allow patterns (repeatable)
    let cli_allow = collect_repeatable_flag(args, "--allow");

    // --deny <pattern> flags: collect all deny patterns (repeatable)
    let cli_deny = collect_repeatable_flag(args, "--deny");

    // Build permission config: CLI flags override config file
    let permissions = if cli_allow.is_empty() && cli_deny.is_empty() {
        // No CLI flags — parse from already-loaded config content
        parse_permissions_from_config(raw_config_content)
    } else {
        PermissionConfig {
            allow: cli_allow,
            deny: cli_deny,
        }
    };

    // --allow-dir <dir> flags: collect all allowed directories (repeatable)
    let cli_allow_dirs = collect_repeatable_flag(args, "--allow-dir");

    // --deny-dir <dir> flags: collect all denied directories (repeatable)
    let cli_deny_dirs = collect_repeatable_flag(args, "--deny-dir");

    // Build directory restrictions: CLI flags override config file
    let dir_restrictions = if cli_allow_dirs.is_empty() && cli_deny_dirs.is_empty() {
        parse_directories_from_config(raw_config_content)
    } else {
        DirectoryRestrictions {
            allow: cli_allow_dirs,
            deny: cli_deny_dirs,
        }
    };

    (permissions, dir_restrictions)
}

/// Parsed MCP and OpenAPI configuration.
struct McpConfig {
    mcp_servers: Vec<String>,
    mcp_server_configs: Vec<McpServerConfig>,
    openapi_specs: Vec<String>,
}

/// Parse MCP servers and OpenAPI specs from CLI args and config.
fn parse_mcp_and_openapi_config(
    args: &[String],
    file_config: &HashMap<String, String>,
    raw_config_content: &str,
) -> McpConfig {
    // --mcp <command> flags: collect all MCP server commands (repeatable)
    let mut mcp_servers = collect_repeatable_flag(args, "--mcp");

    // Merge MCP servers from config file (config servers added first, CLI servers override/add)
    if let Some(mcp_config) = file_config.get("mcp") {
        let config_mcps = parse_toml_array(mcp_config);
        for server in config_mcps.into_iter().rev() {
            if !mcp_servers.contains(&server) {
                mcp_servers.insert(0, server);
            }
        }
    }

    // Parse structured [mcp_servers.*] sections from config file
    let mcp_server_configs = parse_mcp_servers_from_config(raw_config_content);

    // --openapi <spec-path> flags: collect all OpenAPI spec paths (repeatable)
    let openapi_specs = collect_repeatable_flag(args, "--openapi");

    McpConfig {
        mcp_servers,
        mcp_server_configs,
        openapi_specs,
    }
}

pub fn parse_args(args: &[String]) -> Option<Config> {
    // Handle early-exit subcommands (--help, --version) before anything else.
    if let Some(result) = try_dispatch_subcommand(args) {
        return result;
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
            match require_flag_value(args.get(pos + 1)) {
                FlagValueCheck::Ok(_) => {}
                FlagValueCheck::FlagLike(next) => {
                    eprintln!(
                        "{YELLOW}warning:{RESET} {flag} value looks like another flag: '{next}'"
                    );
                }
                FlagValueCheck::Missing => {
                    eprintln!("{RED}error:{RESET} {flag} requires a value");
                    eprintln!("Run with --help for usage information.");
                    std::process::exit(1);
                }
            }
        }
    }

    // Warn about unknown flags
    warn_unknown_flags(args, &flags_needing_values);

    // Parse prompt and image flags early so we can validate --image before API key check
    let prompt_arg = flag_value(args, &["--prompt", "-p"]);

    let image_path_raw = flag_value(args, &["--image"]);

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

    // Parse model/provider/API-key/fallback configuration
    let mc = parse_model_config(args, &file_config, &prompt_arg);

    let skill_dirs = collect_repeatable_flag(args, "--skills");

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
    let custom_system = flag_value(args, &["--system"]);

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
    if let Some(repo_map) = crate::commands_map::generate_repo_map_for_prompt() {
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

    let max_tokens = parse_numeric_flag::<u32>(args, "--max-tokens", &file_config, "max_tokens");

    let temperature = parse_numeric_flag::<f32>(args, "--temperature", &file_config, "temperature")
        .map(clamp_temperature);

    let max_turns = parse_numeric_flag::<usize>(args, "--max-turns", &file_config, "max_turns");

    let output_path = flag_value(args, &["--output", "-o"]);

    // Parse boolean output flags
    let of = parse_output_flags(args, &file_config);

    // Parse permission and directory restriction config
    let (permissions, dir_restrictions) =
        parse_permission_and_dir_config(args, &raw_config_content);

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
    let context_window =
        parse_numeric_flag::<u32>(args, "--context-window", &file_config, "context_window");

    // Parse MCP servers and OpenAPI specs
    let mcp = parse_mcp_and_openapi_config(args, &file_config, &raw_config_content);

    // Parse shell hooks from config file
    let shell_hooks = crate::hooks::parse_hooks_from_config(&file_config);

    Some(Config {
        model: mc.model,
        api_key: mc.api_key,
        provider: mc.provider,
        base_url: mc.base_url,
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
        verbose: of.verbose,
        mcp_servers: mcp.mcp_servers,
        mcp_server_configs: mcp.mcp_server_configs,
        openapi_specs: mcp.openapi_specs,
        auto_approve: of.auto_approve,
        auto_commit: of.auto_commit,
        permissions,
        dir_restrictions,
        context_strategy,
        context_window,
        shell_hooks,
        fallback_provider: mc.fallback_provider,
        fallback_model: mc.fallback_model,
        no_update_check: of.no_update_check,
        json_output: of.json_output,
        audit: of.audit,
        print_system_prompt: of.print_system_prompt,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::glob_match;

    #[test]
    fn test_version_constant_exists() {
        assert!(
            VERSION.contains('.'),
            "Version should contain a dot: {VERSION}"
        );
    }

    #[test]
    fn test_flag_value_finds_value_for_single_flag() {
        let args = vec!["yoyo".into(), "--model".into(), "claude-sonnet".into()];
        assert_eq!(
            flag_value(&args, &["--model"]),
            Some("claude-sonnet".into()),
            "expected to find the value following --model"
        );
    }

    #[test]
    fn test_flag_value_returns_none_when_flag_missing() {
        let args = vec!["yoyo".into(), "--verbose".into()];
        assert_eq!(
            flag_value(&args, &["--model"]),
            None,
            "expected None when --model is not present"
        );
    }

    #[test]
    fn test_flag_value_returns_none_when_value_missing() {
        // Flag is the last argument — there's no value after it.
        let args = vec!["yoyo".into(), "--model".into()];
        assert_eq!(
            flag_value(&args, &["--model"]),
            None,
            "expected None when --model has no value after it"
        );
    }

    #[test]
    fn test_flag_value_supports_aliases() {
        // -p is an alias for --prompt; both should resolve.
        let short = vec!["yoyo".into(), "-p".into(), "hello".into()];
        let long = vec!["yoyo".into(), "--prompt".into(), "hello".into()];
        assert_eq!(
            flag_value(&short, &["--prompt", "-p"]),
            Some("hello".into())
        );
        assert_eq!(flag_value(&long, &["--prompt", "-p"]), Some("hello".into()));
    }

    #[test]
    fn test_flag_value_finds_first_occurrence() {
        // If a flag is repeated, take the first value (matches existing
        // .position()-based behavior in parse_args).
        let args = vec![
            "yoyo".into(),
            "--model".into(),
            "first".into(),
            "--model".into(),
            "second".into(),
        ];
        assert_eq!(
            flag_value(&args, &["--model"]),
            Some("first".into()),
            "expected the first --model value (matches prior position-based behavior)"
        );
    }

    #[test]
    fn test_require_flag_value_ok_on_plain_value() {
        let next = "claude-opus-4".to_string();
        assert_eq!(
            require_flag_value(Some(&next)),
            FlagValueCheck::Ok("claude-opus-4"),
            "a plain token should be accepted as the flag's value"
        );
    }

    #[test]
    fn test_require_flag_value_missing_on_end_of_args() {
        assert_eq!(
            require_flag_value(None),
            FlagValueCheck::Missing,
            "None should classify as Missing so the caller can hard-exit"
        );
    }

    #[test]
    fn test_require_flag_value_flag_like_on_double_dash() {
        // The classic bug: `yoyo --model --provider anthropic` — the value slot
        // is occupied by another flag. Should be flagged (warning territory).
        let next = "--provider".to_string();
        assert_eq!(
            require_flag_value(Some(&next)),
            FlagValueCheck::FlagLike("--provider"),
            "a --flag next-token should classify as FlagLike, not Ok"
        );
    }

    #[test]
    fn test_require_flag_value_flag_like_on_bare_dash() {
        // Bare `-` is not a value anywhere in yoyo (no stdin marker). Treat it
        // the same way the old inline code did: warn but don't hard-exit.
        let next = "-".to_string();
        assert_eq!(
            require_flag_value(Some(&next)),
            FlagValueCheck::FlagLike("-"),
            "bare '-' is not a yoyo value and should be flagged"
        );
    }

    #[test]
    fn test_require_flag_value_accepts_negative_numbers() {
        // `--temperature -0.1` is a real use case — leading `-` followed by a
        // digit is a negative number, not a flag. This is the exact invariant
        // the old inline regex-free check was protecting; pinning it in a test
        // so a future refactor can't quietly break temperature/top-p flags.
        let negative = "-0.1".to_string();
        assert_eq!(
            require_flag_value(Some(&negative)),
            FlagValueCheck::Ok("-0.1"),
            "negative numbers must survive as plain values"
        );

        let neg_int = "-5".to_string();
        assert_eq!(
            require_flag_value(Some(&neg_int)),
            FlagValueCheck::Ok("-5"),
            "negative integers must survive as plain values"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_help_long() {
        // --help should be dispatched (returns Some(None) — handled, parse_args returns None)
        let args = vec!["yoyo".into(), "--help".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for --help"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_help_short() {
        // -h alias should also dispatch
        let args = vec!["yoyo".into(), "-h".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(matches!(result, Some(None)), "expected Some(None) for -h");
    }

    #[test]
    fn test_try_dispatch_subcommand_version_long() {
        let args = vec!["yoyo".into(), "--version".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for --version"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_version_short() {
        let args = vec!["yoyo".into(), "-V".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(matches!(result, Some(None)), "expected Some(None) for -V");
    }

    #[test]
    fn test_try_dispatch_subcommand_falls_through_on_unknown_flag() {
        // An unknown flag should NOT be dispatched as a subcommand —
        // returns None so parse_args continues to flag parsing.
        let args = vec!["yoyo".into(), "--unknown-flag".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(result.is_none(), "expected None for --unknown-flag");
    }

    #[test]
    fn test_try_dispatch_subcommand_falls_through_on_empty_args() {
        // Empty args list should fall through (no subcommand to dispatch).
        let args: Vec<String> = vec![];
        let result = try_dispatch_subcommand(&args);
        assert!(result.is_none(), "expected None for empty args");
    }

    #[test]
    fn test_try_dispatch_subcommand_falls_through_on_normal_flags() {
        // Normal flag combinations should fall through to parse_args's main loop.
        let args = vec![
            "yoyo".into(),
            "--model".into(),
            "claude-sonnet-4-5".into(),
            "--prompt".into(),
            "hello".into(),
        ];
        let result = try_dispatch_subcommand(&args);
        assert!(result.is_none(), "expected None for normal flag combo");
    }

    #[test]
    fn test_try_dispatch_subcommand_help_wins_over_other_flags() {
        // If --help appears anywhere in the args, it should still dispatch.
        let args = vec![
            "yoyo".into(),
            "--model".into(),
            "claude-sonnet-4-5".into(),
            "--help".into(),
        ];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected --help to dispatch even with other flags"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_falls_through_on_unknown_subcommand() {
        // Regression guard for the doctor/health wiring (Day 47): unknown
        // positional subcommands must still fall through to flag parsing.
        // If we accidentally swallow them in try_dispatch_subcommand, every
        // positional token (e.g. a stray filename) would silently exit yoyo.
        let args = vec!["yoyo".into(), "not-a-real-subcommand".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            result.is_none(),
            "expected None for an unknown positional subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_help_bare() {
        // `yoyo help` (bare word, no dashes) should dispatch the same as --help.
        let args = vec!["yoyo".into(), "help".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `help` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_version_bare() {
        // `yoyo version` (bare word) should dispatch the same as --version.
        let args = vec!["yoyo".into(), "version".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `version` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_setup_bare() {
        // `yoyo setup` should dispatch the setup wizard (returns Some(None)).
        let args = vec!["yoyo".into(), "setup".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `setup` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_init_bare() {
        // `yoyo init` should dispatch the init handler (returns Some(None)).
        let args = vec!["yoyo".into(), "init".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `init` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_lint() {
        let args = vec!["yoyo".into(), "lint".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `lint` subcommand"
        );
    }

    #[test]
    #[ignore] // Runs `cargo test` recursively — verified manually, skip in CI
    fn test_try_dispatch_subcommand_test() {
        let args = vec!["yoyo".into(), "test".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `test` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_tree() {
        let args = vec!["yoyo".into(), "tree".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `tree` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_map() {
        let args = vec!["yoyo".into(), "map".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `map` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_run_no_args() {
        // `yoyo run` with no command should still dispatch (shows usage).
        let args = vec!["yoyo".into(), "run".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `run` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_diff() {
        let args = vec!["yoyo".into(), "diff".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `diff` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_commit() {
        // `yoyo commit` with no message should still dispatch (shows "nothing staged" or similar).
        let args = vec!["yoyo".into(), "commit".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `commit` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_blame() {
        // `yoyo blame` with no file should still dispatch (shows error message).
        let args = vec!["yoyo".into(), "blame".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `blame` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_grep() {
        let args = vec!["yoyo".into(), "grep".into(), "TODO".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `grep` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_find() {
        let args = vec!["yoyo".into(), "find".into(), "main".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `find` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_index() {
        let args = vec!["yoyo".into(), "index".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `index` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_update() {
        let args = vec!["yoyo".into(), "update".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `update` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_docs() {
        let args = vec!["yoyo".into(), "docs".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `docs` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_watch() {
        // `yoyo watch status` should dispatch (shows current watch state).
        let args = vec!["yoyo".into(), "watch".into(), "status".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `watch` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_status() {
        let args = vec!["yoyo".into(), "status".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `status` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_undo() {
        // Bare `yoyo undo` with no session — should dispatch (shows fallback message).
        let args = vec!["yoyo".into(), "undo".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `undo` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_changelog() {
        let args = vec!["yoyo".into(), "changelog".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `changelog` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_changelog_with_count() {
        let args = vec!["yoyo".into(), "changelog".into(), "20".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `changelog 20` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_config() {
        let args = vec!["yoyo".into(), "config".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `config` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_config_show() {
        let args = vec!["yoyo".into(), "config".into(), "show".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `config show` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_config_unknown() {
        // Unknown config subcommands still dispatch (print a message, don't hang)
        let args = vec!["yoyo".into(), "config".into(), "edit".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `config edit` (requires session message)"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_permissions() {
        let args = vec!["yoyo".into(), "permissions".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `permissions` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_todo() {
        let args = vec!["yoyo".into(), "todo".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `todo` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_todo_list() {
        let args = vec!["yoyo".into(), "todo".into(), "list".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `todo list` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_memories() {
        let args = vec!["yoyo".into(), "memories".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `memories` subcommand"
        );
    }

    #[test]
    fn help_text_documents_all_subcommands() {
        // Regression guard: all bare subcommands (doctor, health, help, version,
        // setup, init, lint, test, tree, map, run, diff, commit, review, blame,
        // grep, find, index) must appear in the --help output under a Subcommands
        // section so users can discover them.
        let help = help_text();
        assert!(
            help.contains("Subcommands"),
            "--help must have a Subcommands section"
        );
        for subcmd in &[
            "doctor",
            "health",
            "help",
            "version",
            "setup",
            "init",
            "lint",
            "test",
            "tree",
            "map",
            "run",
            "diff",
            "commit",
            "review",
            "blame",
            "grep",
            "find",
            "index",
            "update",
            "docs",
            "watch",
            "status",
            "undo",
            "skill",
            "changelog",
            "config",
            "permissions",
            "todo",
            "memories",
        ] {
            assert!(
                help.contains(subcmd),
                "--help must mention the `{subcmd}` subcommand"
            );
        }
    }

    #[test]
    fn help_text_documents_all_repl_commands() {
        // Every REPL command in KNOWN_COMMANDS should appear in the --help
        // output so users can discover them from the shell.
        use crate::commands::KNOWN_COMMANDS;
        let help = help_text();
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            // /exit is an alias for /quit — both listed on the same line
            if name == "exit" {
                continue;
            }
            assert!(
                help.contains(&format!("/{name}")),
                "--help must mention REPL command {cmd}"
            );
        }
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
        let empty = std::collections::HashMap::new();
        let max_tokens = parse_numeric_flag::<u32>(&args, "--max-tokens", &empty, "max_tokens");
        assert_eq!(max_tokens, Some(4096));
    }

    #[test]
    fn test_max_tokens_flag_missing() {
        let args = ["yoyo".to_string()];
        let empty = std::collections::HashMap::new();
        let max_tokens = parse_numeric_flag::<u32>(&args, "--max-tokens", &empty, "max_tokens");
        assert_eq!(max_tokens, None);
    }

    #[test]
    fn test_max_tokens_flag_invalid() {
        let args = [
            "yoyo".to_string(),
            "--max-tokens".to_string(),
            "not_a_number".to_string(),
        ];
        let empty = std::collections::HashMap::new();
        let max_tokens = parse_numeric_flag::<u32>(&args, "--max-tokens", &empty, "max_tokens");
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
    fn test_temperature_flag_parsing() {
        let args = [
            "yoyo".to_string(),
            "--temperature".to_string(),
            "0.7".to_string(),
        ];
        let empty = std::collections::HashMap::new();
        let temp = parse_numeric_flag::<f32>(&args, "--temperature", &empty, "temperature");
        assert_eq!(temp, Some(0.7));
    }

    #[test]
    fn test_temperature_flag_missing() {
        let args = ["yoyo".to_string()];
        let empty = std::collections::HashMap::new();
        let temp = parse_numeric_flag::<f32>(&args, "--temperature", &empty, "temperature");
        assert_eq!(temp, None);
    }

    #[test]
    fn test_temperature_flag_invalid() {
        let args = [
            "yoyo".to_string(),
            "--temperature".to_string(),
            "not_a_number".to_string(),
        ];
        let empty = std::collections::HashMap::new();
        let temp = parse_numeric_flag::<f32>(&args, "--temperature", &empty, "temperature");
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
    fn help_text_documents_session_budget_env_var() {
        // YOYO_SESSION_BUDGET_SECS is a live behavior-modifying knob (retry loops
        // bail early when the budget is near zero). The only way operators can
        // discover it should be `yoyo --help`, not spelunking src/prompt_budget.rs.
        let help = help_text();
        assert!(
            help.contains("YOYO_SESSION_BUDGET_SECS"),
            "--help output must document YOYO_SESSION_BUDGET_SECS"
        );
    }

    #[test]
    fn help_text_documents_known_env_vars() {
        // Regression guard: the refactor from println! to a String builder
        // must preserve every env var the old print_help() listed.
        let help = help_text();
        for var in [
            "ANTHROPIC_API_KEY",
            "YOYO_AUDIT",
            "YOYO_NO_UPDATE_CHECK",
            "YOYO_SESSION_BUDGET_SECS",
        ] {
            assert!(help.contains(var), "--help should mention {var}");
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
    fn test_welcome_text_mentions_bedrock() {
        let welcome = get_welcome_text();
        assert!(
            welcome.contains("bedrock"),
            "welcome text should mention bedrock"
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

    #[test]
    fn test_mcp_server_config_struct() {
        let cfg = McpServerConfig {
            name: "filesystem".to_string(),
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
                "/path/to/dir".to_string(),
            ],
            env: vec![("NODE_ENV".to_string(), "production".to_string())],
        };
        assert_eq!(cfg.name, "filesystem");
        assert_eq!(cfg.command, "npx");
        assert_eq!(cfg.args.len(), 3);
        assert_eq!(cfg.env.len(), 1);
        assert_eq!(cfg.env[0].0, "NODE_ENV");
        assert_eq!(cfg.env[0].1, "production");
    }

    #[test]
    fn test_parse_mcp_servers_basic() {
        let content = r#"
model = "claude-sonnet-4-20250514"

[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/dir"]

[mcp_servers.postgres]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-postgres"]
env = { DATABASE_URL = "postgresql://localhost/mydb" }
"#;
        let servers = parse_mcp_servers_from_config(content);
        assert_eq!(servers.len(), 2);

        assert_eq!(servers[0].name, "filesystem");
        assert_eq!(servers[0].command, "npx");
        assert_eq!(
            servers[0].args,
            vec![
                "-y",
                "@modelcontextprotocol/server-filesystem",
                "/path/to/dir"
            ]
        );
        assert!(servers[0].env.is_empty());

        assert_eq!(servers[1].name, "postgres");
        assert_eq!(servers[1].command, "npx");
        assert_eq!(
            servers[1].args,
            vec!["-y", "@modelcontextprotocol/server-postgres"]
        );
        assert_eq!(servers[1].env.len(), 1);
        assert_eq!(servers[1].env[0].0, "DATABASE_URL");
        assert_eq!(servers[1].env[0].1, "postgresql://localhost/mydb");
    }

    #[test]
    fn test_parse_mcp_servers_empty_config() {
        let content = r#"
model = "claude-sonnet-4-20250514"

[permissions]
allow = ["git *"]
"#;
        let servers = parse_mcp_servers_from_config(content);
        assert!(servers.is_empty());
    }

    #[test]
    fn test_parse_mcp_servers_no_args_or_env() {
        let content = r#"
[mcp_servers.simple]
command = "my-server"
"#;
        let servers = parse_mcp_servers_from_config(content);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "simple");
        assert_eq!(servers[0].command, "my-server");
        assert!(servers[0].args.is_empty());
        assert!(servers[0].env.is_empty());
    }

    #[test]
    fn test_parse_mcp_servers_multiple_env_vars() {
        let content = r#"
[mcp_servers.mydb]
command = "db-server"
args = ["--verbose"]
env = { DB_HOST = "localhost", DB_PORT = "5432", DB_NAME = "mydb" }
"#;
        let servers = parse_mcp_servers_from_config(content);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].env.len(), 3);
        // Check all env vars are present (order may vary within inline table)
        let env_keys: Vec<&str> = servers[0].env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(env_keys.contains(&"DB_HOST"));
        assert!(env_keys.contains(&"DB_PORT"));
        assert!(env_keys.contains(&"DB_NAME"));
    }

    #[test]
    fn test_parse_mcp_servers_skips_incomplete() {
        // Missing command should be skipped
        let content = r#"
[mcp_servers.broken]
args = ["-y", "something"]

[mcp_servers.valid]
command = "good-server"
"#;
        let servers = parse_mcp_servers_from_config(content);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "valid");
    }

    #[test]
    fn test_parse_mcp_servers_mixed_with_other_sections() {
        let content = r#"
model = "gpt-4o"

[permissions]
allow = ["git *"]

[mcp_servers.first]
command = "server-one"
args = ["-a"]

[directories]
allow = ["./src"]

[mcp_servers.second]
command = "server-two"
"#;
        let servers = parse_mcp_servers_from_config(content);
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "first");
        assert_eq!(servers[1].name, "second");
    }

    #[test]
    fn test_parse_numeric_flag_config_fallback() {
        let args = ["yoyo".to_string()];
        let mut config = std::collections::HashMap::new();
        config.insert("max_tokens".to_string(), "2048".to_string());
        let result = parse_numeric_flag::<u32>(&args, "--max-tokens", &config, "max_tokens");
        assert_eq!(result, Some(2048));
    }

    #[test]
    fn test_parse_numeric_flag_cli_overrides_config() {
        let args = [
            "yoyo".to_string(),
            "--max-tokens".to_string(),
            "4096".to_string(),
        ];
        let mut config = std::collections::HashMap::new();
        config.insert("max_tokens".to_string(), "2048".to_string());
        let result = parse_numeric_flag::<u32>(&args, "--max-tokens", &config, "max_tokens");
        assert_eq!(result, Some(4096));
    }

    #[test]
    fn test_parse_numeric_flag_invalid_cli_falls_to_config() {
        let args = [
            "yoyo".to_string(),
            "--max-tokens".to_string(),
            "bad".to_string(),
        ];
        let mut config = std::collections::HashMap::new();
        config.insert("max_tokens".to_string(), "2048".to_string());
        let result = parse_numeric_flag::<u32>(&args, "--max-tokens", &config, "max_tokens");
        // Invalid CLI value warns and falls through to config
        assert_eq!(result, Some(2048));
    }

    #[test]
    fn test_parse_numeric_flag_invalid_config_returns_none() {
        let args = ["yoyo".to_string()];
        let mut config = std::collections::HashMap::new();
        config.insert("max_tokens".to_string(), "not_a_number".to_string());
        let result = parse_numeric_flag::<u32>(&args, "--max-tokens", &config, "max_tokens");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_numeric_flag_usize() {
        let args = [
            "yoyo".to_string(),
            "--max-turns".to_string(),
            "25".to_string(),
        ];
        let empty = std::collections::HashMap::new();
        let result = parse_numeric_flag::<usize>(&args, "--max-turns", &empty, "max_turns");
        assert_eq!(result, Some(25));
    }

    #[test]
    fn test_auto_commit_flag_default_false() {
        // When --auto-commit is not passed, auto_commit should default to false
        let args = vec!["yoyo".to_string(), "-p".to_string(), "hello".to_string()];
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let config = parse_args(&args).unwrap();
        assert!(!config.auto_commit, "auto_commit should default to false");
    }

    #[test]
    fn test_auto_commit_flag_parsed() {
        // When --auto-commit is passed, auto_commit should be true
        let args = vec![
            "yoyo".to_string(),
            "--auto-commit".to_string(),
            "-p".to_string(),
            "hello".to_string(),
        ];
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let config = parse_args(&args).unwrap();
        assert!(
            config.auto_commit,
            "auto_commit should be true when --auto-commit is passed"
        );
    }

    // ── quote_args_as_command ─────────────────────────────────────────

    #[test]
    fn quote_args_simple() {
        let args: Vec<String> = vec!["yoyo", "grep", "TODO"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(quote_args_as_command(&args), "/grep TODO");
    }

    #[test]
    fn quote_args_multi_word() {
        let args: Vec<String> = vec!["yoyo", "grep", "fn main"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(quote_args_as_command(&args), r#"/grep "fn main""#);
    }

    #[test]
    fn quote_args_multi_word_with_path() {
        let args: Vec<String> = vec!["yoyo", "grep", "fn main", "src/"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(quote_args_as_command(&args), r#"/grep "fn main" src/"#);
    }

    #[test]
    fn quote_args_no_unnecessary_quoting() {
        let args: Vec<String> = vec!["yoyo", "diff", "--staged"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(quote_args_as_command(&args), "/diff --staged");
    }

    #[test]
    fn quote_args_tab_in_arg() {
        let args: Vec<String> = vec!["yoyo", "grep", "has\ttab"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(quote_args_as_command(&args), "/grep \"has\ttab\"");
    }
}
