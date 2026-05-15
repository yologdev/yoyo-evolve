//! CLI argument parsing, config file support, and help text.

use crate::dispatch_sub::{flag_value, require_flag_value, FlagValueCheck};
use crate::format::*;
use std::collections::HashMap;
use std::io::IsTerminal;
use yoagent::skills::SkillSet;
use yoagent::ThinkingLevel;

// Constants, Config struct, enums — extracted to cli_config.rs for readability.
pub use crate::cli_config::*;

/// Known provider names for the --provider flag.
// Re-exported from providers module so existing `use crate::cli::` imports keep working.
pub use crate::providers::{
    default_model_for_provider, known_models_for_provider, provider_api_key_env, KNOWN_PROVIDERS,
};

// Re-exported from config module so existing `use crate::cli::` imports keep working.
pub use crate::config::{
    history_file_path, home_config_path, load_config_file, parse_config_file,
    parse_directories_from_config, parse_mcp_servers_from_config, parse_permissions_from_config,
    parse_toml_array, user_config_path, DirectoryRestrictions, McpServerConfig, PermissionConfig,
};

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

/// Build the full `--help` output as a string.
///
/// Delegates to [`help::cli_help_text`] which is the canonical source.
/// Kept as a public re-export so existing `cli::help_text()` call sites
/// (including tests) continue to work without changing imports.
pub fn help_text() -> String {
    crate::help::cli_help_text()
}

pub fn print_banner() {
    let day_str = option_env!("DAY_COUNT").unwrap_or("");
    let day_suffix = if day_str.is_empty() {
        String::new()
    } else {
        format!(" — Day {day_str}")
    };
    println!(
        "\n{BOLD}{CYAN}  yoyo{RESET} v{VERSION}{day_suffix} {DIM}— a coding agent growing up in public{RESET}"
    );

    // Show project context if we can detect it
    let dir = std::path::Path::new(".");
    let project_type = crate::commands_project::detect_project_type(dir);
    let name = crate::commands_project::detect_project_name(dir);
    let branch = crate::git::git_branch();
    if let Some(line) = banner_project_line(&project_type, &name, branch.as_deref()) {
        println!("{DIM}  {line}{RESET}");
    }

    println!("{DIM}  Type /help for commands, /quit to exit{RESET}\n");
}

/// Build the project context line for the startup banner.
/// Returns `None` if the project type is Unknown (graceful degradation).
pub fn banner_project_line(
    project_type: &crate::commands_project::ProjectType,
    name: &str,
    branch: Option<&str>,
) -> Option<String> {
    use crate::commands_project::ProjectType;

    if *project_type == ProjectType::Unknown {
        return None;
    }

    let type_label = match project_type {
        ProjectType::Rust => "Rust",
        ProjectType::Node => "Node.js",
        ProjectType::Python => "Python",
        ProjectType::Go => "Go",
        ProjectType::Java => "Java",
        ProjectType::Ruby => "Ruby",
        ProjectType::Cpp => "C/C++",
        ProjectType::Make => "Make",
        ProjectType::Unknown => unreachable!(),
    };

    let name_part = if name.is_empty() {
        String::new()
    } else {
        format!(" ({name})")
    };

    let branch_part = if let Some(b) = branch {
        format!(" on {b}")
    } else {
        String::new()
    };

    Some(format!(
        "\u{1F4C1} {type_label} project{name_part}{branch_part}"
    ))
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
    "--no-notify",
    "--no-rtk",
    "--no-update-check",
    "--json",
    "--output-format",
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
    "--print",
    "--quiet",
    "-q",
    "--disallowed-tools",
    "--help",
    "-h",
    "--version",
    "-V",
];

/// Collect positional arguments that aren't flags, flag values, or known subcommands.
///
/// Walks `args` (skipping `args[0]` — the binary name) and collects any token
/// that is not a flag (`--foo` / `-x`), not consumed as a flag's value, and
/// not a known bare subcommand (e.g. `doctor`, `help`, `setup`).
///
/// Used to support bare positional prompts: `yoyo "fix this bug"` without
/// requiring `--prompt`. The caller joins the result with spaces.
pub(crate) fn collect_positional_args(
    args: &[String],
    flags_needing_values: &[&str],
) -> Vec<String> {
    // Subcommands dispatched by try_dispatch_subcommand — if args[1] is one of
    // these, the subcommand handler already ran and parse_args was never reached.
    // But we list them defensively so that a typo like `yoyo doctor something`
    // (which falls through because "doctor" was already dispatched) never
    // accidentally treats a subcommand name as a prompt token.
    const KNOWN_SUBCOMMANDS: &[&str] = &[
        "blame",
        "changelog",
        "commit",
        "config",
        "diff",
        "docs",
        "doctor",
        "evolution",
        "extended",
        "find",
        "grep",
        "health",
        "help",
        "index",
        "init",
        "lint",
        "map",
        "memories",
        "outline",
        "permissions",
        "review",
        "run",
        "setup",
        "skill",
        "status",
        "test",
        "todo",
        "tree",
        "undo",
        "update",
        "version",
        "watch",
    ];

    let mut positional = Vec::new();
    let mut skip_next = false;

    for (i, arg) in args.iter().enumerate() {
        // Skip the binary name
        if i == 0 {
            continue;
        }
        // This arg is consumed as a flag's value — skip it
        if skip_next {
            skip_next = false;
            continue;
        }
        // It's a flag: if it needs a value, mark the next arg for skipping
        if arg.starts_with('-') {
            if flags_needing_values.contains(&arg.as_str()) {
                skip_next = true;
            }
            continue;
        }
        // Skip known subcommands (only relevant for args[1])
        if i == 1 && KNOWN_SUBCOMMANDS.contains(&arg.as_str()) {
            continue;
        }
        positional.push(arg.clone());
    }
    positional
}

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

// Config-file path resolution and loading functions live in config.rs.
// Re-exported below so existing `use crate::cli::` imports keep working.

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
pub(crate) fn collect_repeatable_flag(args: &[String], flag: &str) -> Vec<String> {
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
    output_format: OutputFormat,
    audit: bool,
    print_system_prompt: bool,
    print_mode: bool,
}

/// Parse simple boolean output flags from CLI args and config.
fn parse_output_flags(args: &[String], file_config: &HashMap<String, String>) -> OutputFlags {
    let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");

    let auto_approve = args.iter().any(|a| a == "--yes" || a == "-y");

    let print_mode = args.iter().any(|a| a == "--print");

    // --print implies --yes (auto-approve all tool use)
    let auto_approve = auto_approve || print_mode;

    let auto_commit = args.iter().any(|a| a == "--auto-commit");

    let no_update_check = args.iter().any(|a| a == "--no-update-check")
        || std::env::var("YOYO_NO_UPDATE_CHECK")
            .map(|v| v == "1")
            .unwrap_or(false);

    let json_output = args.iter().any(|a| a == "--json");

    // Parse --output-format <value> (takes precedence over --json)
    let output_format = if let Some(pos) = args.iter().position(|a| a == "--output-format") {
        match args.get(pos + 1).map(|s| s.as_str()) {
            Some("stream-json") => OutputFormat::StreamJson,
            Some("json") => OutputFormat::Json,
            Some("text") => OutputFormat::Text,
            _ => {
                if json_output {
                    OutputFormat::Json
                } else {
                    OutputFormat::Text
                }
            }
        }
    } else if json_output {
        OutputFormat::Json
    } else {
        OutputFormat::Text
    };

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
        output_format,
        audit,
        print_system_prompt,
        print_mode,
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
    if let Some(result) = crate::dispatch_sub::try_dispatch_subcommand(args) {
        return result;
    }

    // Enable quiet mode early so config/context loading can check it.
    // Also auto-enable when both stdin and stdout are non-terminal (fully piped).
    // --print implies quiet mode (suppress all chrome).
    if args.iter().any(|a| a == "--quiet" || a == "-q")
        || args.iter().any(|a| a == "--print")
        || std::env::var("YOYO_QUIET")
            .map(|v| v == "1")
            .unwrap_or(false)
        || (!std::io::stdin().is_terminal() && !std::io::stdout().is_terminal())
    {
        crate::format::enable_quiet();
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
        "--disallowed-tools",
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
    let mut prompt_arg = flag_value(args, &["--prompt", "-p"]);

    // Support bare positional prompts: `yoyo "fix this bug"` without --prompt.
    // Only if --prompt/-p wasn't explicitly provided.
    if prompt_arg.is_none() {
        let positional = collect_positional_args(args, &flags_needing_values);
        if !positional.is_empty() {
            prompt_arg = Some(positional.join(" "));
        }
    }

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
        output_format: of.output_format,
        audit: of.audit,
        print_system_prompt: of.print_system_prompt,
        print_mode: of.print_mode,
        auto_watch: crate::config::parse_auto_watch_from_config(&file_config),
        disallowed_tools: collect_repeatable_flag(args, "--disallowed-tools")
            .iter()
            .flat_map(|v| v.split(','))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
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
    fn test_quiet_flag_recognized() {
        let args_long = ["yoyo".to_string(), "--quiet".to_string()];
        assert!(args_long.iter().any(|a| a == "--quiet" || a == "-q"));
        assert!(KNOWN_FLAGS.contains(&"--quiet"));
    }

    #[test]
    fn test_quiet_short_flag_recognized() {
        let args_short = ["yoyo".to_string(), "-q".to_string()];
        assert!(args_short.iter().any(|a| a == "--quiet" || a == "-q"));
        assert!(KNOWN_FLAGS.contains(&"-q"));
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
        assert_eq!(config.fallback_model, Some("gpt-5".to_string()));
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

    #[test]
    fn test_print_banner_does_not_panic() {
        // print_banner uses compile-time DAY_COUNT via option_env!().
        // When built from yoyo's repo, DAY_COUNT is baked in.
        // When built externally, option_env! returns None gracefully.
        // Either way, it must not panic.
        print_banner();
    }

    #[test]
    fn test_banner_project_line_rust() {
        use crate::commands_project::ProjectType;
        let line = banner_project_line(&ProjectType::Rust, "my-app", Some("main"));
        assert_eq!(
            line,
            Some("\u{1F4C1} Rust project (my-app) on main".to_string())
        );
    }

    #[test]
    fn test_banner_project_line_node_no_branch() {
        use crate::commands_project::ProjectType;
        let line = banner_project_line(&ProjectType::Node, "webapp", None);
        assert_eq!(line, Some("\u{1F4C1} Node.js project (webapp)".to_string()));
    }

    #[test]
    fn test_banner_project_line_unknown_returns_none() {
        use crate::commands_project::ProjectType;
        let line = banner_project_line(&ProjectType::Unknown, "something", Some("main"));
        assert_eq!(line, None);
    }

    #[test]
    fn test_banner_project_line_empty_name() {
        use crate::commands_project::ProjectType;
        let line = banner_project_line(&ProjectType::Python, "", Some("dev"));
        assert_eq!(line, Some("\u{1F4C1} Python project on dev".to_string()));
    }

    #[test]
    fn test_banner_project_line_go() {
        use crate::commands_project::ProjectType;
        let line = banner_project_line(&ProjectType::Go, "myservice", Some("feature/x"));
        assert_eq!(
            line,
            Some("\u{1F4C1} Go project (myservice) on feature/x".to_string())
        );
    }

    #[test]
    fn test_banner_project_line_make_no_name_no_branch() {
        use crate::commands_project::ProjectType;
        let line = banner_project_line(&ProjectType::Make, "", None);
        assert_eq!(line, Some("\u{1F4C1} Make project".to_string()));
    }

    // ── bare positional prompt tests ──────────────────────────────────

    #[test]
    fn test_collect_positional_bare_prompt() {
        let flags = ["--model", "--prompt", "-p"];
        let args: Vec<String> = vec!["yoyo", "fix this bug"]
            .into_iter()
            .map(String::from)
            .collect();
        let pos = collect_positional_args(&args, &flags);
        assert_eq!(pos, vec!["fix this bug"]);
    }

    #[test]
    fn test_collect_positional_with_flags() {
        let flags = ["--model", "--prompt", "-p"];
        let args: Vec<String> = vec!["yoyo", "--model", "gpt-4", "do something"]
            .into_iter()
            .map(String::from)
            .collect();
        let pos = collect_positional_args(&args, &flags);
        assert_eq!(pos, vec!["do something"]);
    }

    #[test]
    fn test_collect_positional_no_args() {
        let flags = ["--model", "--prompt", "-p"];
        let args: Vec<String> = vec!["yoyo"].into_iter().map(String::from).collect();
        let pos = collect_positional_args(&args, &flags);
        assert!(pos.is_empty());
    }

    #[test]
    fn test_collect_positional_only_flags() {
        let flags = ["--model", "--prompt", "-p"];
        let args: Vec<String> = vec!["yoyo", "--model", "gpt-4"]
            .into_iter()
            .map(String::from)
            .collect();
        let pos = collect_positional_args(&args, &flags);
        assert!(pos.is_empty());
    }

    #[test]
    fn test_collect_positional_skips_subcommand() {
        let flags = ["--model", "--prompt", "-p"];
        let args: Vec<String> = vec!["yoyo", "doctor"]
            .into_iter()
            .map(String::from)
            .collect();
        let pos = collect_positional_args(&args, &flags);
        assert!(
            pos.is_empty(),
            "known subcommands should not become prompts"
        );
    }

    #[test]
    fn test_collect_positional_prompt_flag_takes_precedence() {
        // When --prompt is explicitly passed, collect_positional_args still
        // collects positional args, but parse_args checks prompt_arg first.
        let flags = ["--model", "--prompt", "-p"];
        let args: Vec<String> = vec!["yoyo", "-p", "explicit prompt", "extra"]
            .into_iter()
            .map(String::from)
            .collect();
        // -p consumes "explicit prompt" as its value, "extra" is positional
        let pos = collect_positional_args(&args, &flags);
        assert_eq!(pos, vec!["extra"]);
        // But in parse_args, prompt_arg would already be Some("explicit prompt")
        // so the positional branch is never taken.
    }

    #[test]
    fn test_collect_positional_multiple_words() {
        // Multiple positional args get joined by the caller
        let flags = ["--model"];
        let args: Vec<String> = vec!["yoyo", "fix", "the", "bug"]
            .into_iter()
            .map(String::from)
            .collect();
        let pos = collect_positional_args(&args, &flags);
        assert_eq!(pos, vec!["fix", "the", "bug"]);
    }

    #[test]
    fn test_collect_positional_flag_after_prompt() {
        // `yoyo "do something" --json` — positional before a boolean flag
        let flags = ["--model"];
        let args: Vec<String> = vec!["yoyo", "do something", "--json"]
            .into_iter()
            .map(String::from)
            .collect();
        let pos = collect_positional_args(&args, &flags);
        assert_eq!(pos, vec!["do something"]);
    }

    #[test]
    fn test_bare_prompt_via_parse_args() {
        // End-to-end: `yoyo "fix bug"` should set prompt_arg
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec!["yoyo".to_string(), "fix bug".to_string()];
        let config = parse_args(&args).unwrap();
        assert_eq!(config.prompt_arg, Some("fix bug".to_string()));
    }

    #[test]
    fn test_bare_prompt_with_model_flag_via_parse_args() {
        // `yoyo --model gpt-4 "do something"` should work
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec![
            "yoyo".to_string(),
            "--model".to_string(),
            "gpt-4".to_string(),
            "do something".to_string(),
        ];
        let config = parse_args(&args).unwrap();
        assert_eq!(config.prompt_arg, Some("do something".to_string()));
    }

    #[test]
    fn test_explicit_prompt_flag_overrides_positional() {
        // `yoyo -p "explicit" "ignored"` — -p takes precedence
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec![
            "yoyo".to_string(),
            "-p".to_string(),
            "explicit".to_string(),
            "ignored".to_string(),
        ];
        let config = parse_args(&args).unwrap();
        assert_eq!(config.prompt_arg, Some("explicit".to_string()));
    }

    #[test]
    fn test_no_args_still_none_prompt() {
        // `yoyo` with no args → REPL mode, prompt_arg is None
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec!["yoyo".to_string()];
        let config = parse_args(&args).unwrap();
        assert_eq!(config.prompt_arg, None);
    }

    #[test]
    fn test_output_format_flag_in_known_flags() {
        assert!(KNOWN_FLAGS.contains(&"--output-format"));
    }

    #[test]
    fn test_output_format_stream_json() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec![
            "yoyo".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.output_format, OutputFormat::StreamJson);
    }

    #[test]
    fn test_output_format_json() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec![
            "yoyo".to_string(),
            "--output-format".to_string(),
            "json".to_string(),
        ];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.output_format, OutputFormat::Json);
    }

    #[test]
    fn test_output_format_text_explicit() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec![
            "yoyo".to_string(),
            "--output-format".to_string(),
            "text".to_string(),
        ];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.output_format, OutputFormat::Text);
    }

    #[test]
    fn test_json_flag_sets_output_format_json() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec!["yoyo".to_string(), "--json".to_string()];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.output_format, OutputFormat::Json);
        assert!(config.json_output); // legacy flag still set
    }

    #[test]
    fn test_output_format_default_is_text() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec!["yoyo".to_string()];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.output_format, OutputFormat::Text);
    }

    #[test]
    fn test_print_flag_recognized_as_known_flag() {
        assert!(
            KNOWN_FLAGS.contains(&"--print"),
            "--print should be in KNOWN_FLAGS"
        );
    }

    #[test]
    fn test_print_flag_sets_print_mode() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec!["yoyo".to_string(), "--print".to_string()];
        let config = parse_args(&args).expect("should parse");
        assert!(config.print_mode, "--print should set print_mode to true");
    }

    #[test]
    fn test_print_flag_implies_auto_approve() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec!["yoyo".to_string(), "--print".to_string()];
        let config = parse_args(&args).expect("should parse");
        assert!(
            config.auto_approve,
            "--print should imply --yes (auto_approve)"
        );
    }

    #[test]
    fn test_print_flag_without_prompt_warns() {
        // When --print is used without -p, print_mode is still set in Config
        // (the warning is emitted at runtime in main.rs, not during parsing).
        // Verify the flag is parsed correctly even without -p.
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec!["yoyo".to_string(), "--print".to_string()];
        let config = parse_args(&args).expect("should parse");
        assert!(config.print_mode);
        // prompt_arg should be None since -p was not provided
        assert!(
            config.prompt_arg.is_none(),
            "--print without -p should have no prompt"
        );
    }

    #[test]
    fn test_disallowed_tools_single() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec![
            "yoyo".to_string(),
            "--disallowed-tools".to_string(),
            "bash".to_string(),
        ];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(config.disallowed_tools, vec!["bash".to_string()]);
    }

    #[test]
    fn test_disallowed_tools_comma_separated() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec![
            "yoyo".to_string(),
            "--disallowed-tools".to_string(),
            "bash,write_file,edit_file".to_string(),
        ];
        let config = parse_args(&args).expect("should parse");
        assert_eq!(
            config.disallowed_tools,
            vec![
                "bash".to_string(),
                "write_file".to_string(),
                "edit_file".to_string(),
            ]
        );
    }

    #[test]
    fn test_disallowed_tools_in_known_flags() {
        assert!(
            KNOWN_FLAGS.contains(&"--disallowed-tools"),
            "--disallowed-tools should be in KNOWN_FLAGS"
        );
    }

    #[test]
    fn test_disallowed_tools_empty_when_not_provided() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let args = vec!["yoyo".to_string()];
        let config = parse_args(&args).expect("should parse");
        assert!(
            config.disallowed_tools.is_empty(),
            "disallowed_tools should be empty when flag is not provided"
        );
    }
}
