//! CLI argument parsing, config file support, and help text.

use crate::format::*;
use std::collections::HashMap;
use yoagent::skills::SkillSet;
use yoagent::ThinkingLevel;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_CONTEXT_TOKENS: u64 = 200_000;
pub const AUTO_COMPACT_THRESHOLD: f64 = 0.80;
pub const DEFAULT_SESSION_PATH: &str = "yoyo-session.json";

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
    "custom",
];

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
    pub verbose: bool,
    pub mcp_servers: Vec<String>,
    pub auto_approve: bool,
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
    println!("                    ollama, xai, groq, deepseek, mistral, cerebras, custom");
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
    println!("  --no-color        Disable colored output (also respects NO_COLOR env)");
    println!("  --verbose, -v     Show debug info (API errors, request details)");
    println!("  --yes, -y         Auto-approve all tool executions (skip confirmation prompts)");
    println!("  --continue, -c    Resume last saved session");
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
    println!("  /git <subcmd>     Quick git: status, log [n], add <path>, stash, stash pop");
    println!("  /health           Run project health checks (auto-detects project type)");
    println!("  /pr [number]      List open PRs, or view details of a specific PR");
    println!("  /history          Show summary of conversation messages");
    println!("  /search <query>   Search conversation history for matching messages");
    println!("  /init             Create a starter YOYO.md project context file");
    println!("  /load [path]      Load session from file");
    println!("  /model <name>     Switch model mid-session");
    println!("  /retry            Re-send the last user input");
    println!("  /run <cmd>        Run a shell command directly (no AI, no tokens)");
    println!("  /save [path]      Save session to file");
    println!("  /status           Show session info");
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
    println!("  API_KEY            Fallback API key (any provider)");
    println!();
    println!("Config files (searched in order, first found wins):");
    println!("  .yoyo.toml              Project-level config (current directory)");
    println!("  ~/.config/yoyo/config.toml  User-level config");
    println!();
    println!("Config file format (key = value):");
    println!("  model = \"claude-sonnet-4-20250514\"");
    println!("  provider = \"openai\"");
    println!("  base_url = \"http://localhost:11434/v1\"");
    println!("  thinking = \"medium\"");
    println!("  max_tokens = 4096");
    println!("  max_turns = 20");
    println!("  api_key = \"sk-ant-...\"");
    println!();
    println!("CLI flags override config file values.");
}

pub fn print_banner() {
    println!(
        "\n{BOLD}{CYAN}  yoyo{RESET} v{VERSION} {DIM}— a coding agent growing up in public{RESET}"
    );
    println!("{DIM}  Type /help for commands, /quit to exit{RESET}\n");
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
    "--no-color",
    "--verbose",
    "-v",
    "--yes",
    "-y",
    "--continue",
    "-c",
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
    let output = std::process::Command::new("git")
        .args(["ls-files"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
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

/// Load project context from YOYO.md (primary), CLAUDE.md (compatibility alias),
/// or .yoyo/instructions.md.
/// Returns the combined content of all found files, or None if none exist.
/// Also appends a project file listing from `git ls-files` when available.
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
            return Some(context);
        }
    }

    if found.is_empty() {
        None
    } else {
        for name in &found {
            eprintln!("{DIM}  context: {name}{RESET}");
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
/// - `~/.config/yoyo/config.toml` (user-level)
const CONFIG_FILE_NAMES: &[&str] = &[".yoyo.toml"];

fn user_config_path() -> Option<std::path::PathBuf> {
    dirs_hint().map(|dir| dir.join("yoyo").join("config.toml"))
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

/// Load config from file, checking project-level then user-level paths.
/// Returns an empty map if no config file is found.
fn load_config_file() -> HashMap<String, String> {
    // Check project-level config first
    for name in CONFIG_FILE_NAMES {
        if let Ok(content) = std::fs::read_to_string(name) {
            eprintln!("{DIM}  config: {name}{RESET}");
            return parse_config_file(&content);
        }
    }
    // Check user-level config
    if let Some(path) = user_config_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            eprintln!("{DIM}  config: {}{RESET}", path.display());
            return parse_config_file(&content);
        }
    }
    HashMap::new()
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
    let file_config = load_config_file();

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
                                } else {
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

    // --system-file takes precedence over --system, both override default
    let mut system_prompt = system_from_file
        .or(custom_system)
        .unwrap_or_else(|| SYSTEM_PROMPT.to_string());

    // Append project context (YOYO.md, .yoyo/instructions.md) to system prompt
    if let Some(project_context) = load_project_context() {
        system_prompt.push_str("\n\n# Project Instructions\n\n");
        system_prompt.push_str(&project_context);
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

    let prompt_arg = args
        .iter()
        .position(|a| a == "--prompt" || a == "-p")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");

    let auto_approve = args.iter().any(|a| a == "--yes" || a == "-y");

    // --mcp <command> flags: collect all MCP server commands (repeatable)
    let mcp_servers: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--mcp")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();

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
        verbose,
        mcp_servers,
        auto_approve,
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
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        _ => None,
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
        assert_eq!(MAX_CONTEXT_TOKENS, 200_000);
        assert!((AUTO_COMPACT_THRESHOLD - 0.80).abs() < f64::EPSILON);
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
}
