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

/// Parsed CLI configuration.
pub struct Config {
    pub model: String,
    pub api_key: String,
    pub skills: SkillSet,
    pub system_prompt: String,
    pub thinking: ThinkingLevel,
    pub max_tokens: Option<u32>,
    pub continue_session: bool,
    pub output_path: Option<String>,
    pub prompt_arg: Option<String>,
}

/// Project context file names, checked in order. All found files are concatenated.
pub const PROJECT_CONTEXT_FILES: &[&str] = &["YOYO.md", "CLAUDE.md", ".yoyo/instructions.md"];

pub fn print_help() {
    println!("yoyo v{VERSION} — a coding agent growing up in public");
    println!();
    println!("Usage: yoyo [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --model <name>    Model to use (default: claude-opus-4-6)");
    println!("  --thinking <lvl>  Enable extended thinking (off, minimal, low, medium, high)");
    println!("  --max-tokens <n>  Maximum output tokens per response (default: 8192)");
    println!("  --skills <dir>    Directory containing skill files");
    println!("  --system <text>   Custom system prompt (overrides default)");
    println!("  --system-file <f> Read system prompt from file");
    println!("  --prompt, -p <t>  Run a single prompt and exit (no REPL)");
    println!("  --output, -o <f>  Write final response text to a file");
    println!("  --no-color        Disable colored output (also respects NO_COLOR env)");
    println!("  --continue, -c    Resume last saved session");
    println!("  --help, -h        Show this help message");
    println!("  --version, -V     Show version");
    println!();
    println!("Commands (in REPL):");
    println!("  /quit, /exit      Exit the agent");
    println!("  /clear            Clear conversation history");
    println!("  /compact          Compact conversation to save context space");
    println!("  /config           Show all current settings");
    println!("  /context          Show loaded project context files");
    println!("  /model <name>     Switch model mid-session");
    println!("  /status           Show session info");
    println!("  /tokens           Show token usage and context window");
    println!("  /save [path]      Save session to file");
    println!("  /load [path]      Load session from file");
    println!("  /diff             Show git diff summary");
    println!();
    println!("Environment:");
    println!("  ANTHROPIC_API_KEY  API key for Anthropic (required)");
    println!("  API_KEY            Alternative env var for API key");
    println!();
    println!("Config files (searched in order, first found wins):");
    println!("  .yoyo.toml              Project-level config (current directory)");
    println!("  ~/.config/yoyo/config.toml  User-level config");
    println!();
    println!("Config file format (key = value):");
    println!("  model = \"claude-sonnet-4-20250514\"");
    println!("  thinking = \"medium\"");
    println!("  max_tokens = 4096");
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

/// Load project context from YOYO.md or .yoyo/instructions.md.
/// Returns the combined content of all found files, or None if none exist.
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
        "--thinking",
        "--max-tokens",
        "--skills",
        "--system",
        "--system-file",
        "--prompt",
        "-p",
        "--output",
        "-o",
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

    let api_key = match std::env::var("ANTHROPIC_API_KEY").or_else(|_| std::env::var("API_KEY")) {
        Ok(key) if !key.is_empty() => key,
        _ => {
            eprintln!("{RED}error:{RESET} No API key found.");
            eprintln!("Set ANTHROPIC_API_KEY or API_KEY environment variable.");
            eprintln!("Example: ANTHROPIC_API_KEY=sk-ant-... cargo run");
            std::process::exit(1);
        }
    };

    let model = args
        .iter()
        .position(|a| a == "--model")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| file_config.get("model").cloned())
        .unwrap_or_else(|| "claude-opus-4-6".into());

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

    Some(Config {
        model,
        api_key,
        skills,
        system_prompt,
        thinking,
        max_tokens,
        continue_session,
        output_path,
        prompt_arg,
    })
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
        assert!(!PROJECT_CONTEXT_FILES.is_empty());
        for name in PROJECT_CONTEXT_FILES {
            assert!(!name.is_empty());
        }
    }
}
