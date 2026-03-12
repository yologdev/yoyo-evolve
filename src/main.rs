//! yoyo — a coding agent that evolves itself.
//!
//! Started as ~200 lines. Grows one commit at a time.
//! Read IDENTITY.md and JOURNAL.md for the full story.
//!
//! Usage:
//!   ANTHROPIC_API_KEY=sk-... cargo run
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --model claude-opus-4-6
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --thinking high
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --skills ./skills
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --mcp "npx -y @modelcontextprotocol/server-filesystem /tmp"
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system "You are a Rust expert."
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system-file prompt.txt
//!   ANTHROPIC_API_KEY=sk-... cargo run -- -p "explain this code"
//!   ANTHROPIC_API_KEY=sk-... cargo run -- -p "write a README" -o README.md
//!   echo "prompt" | cargo run  (piped mode: single prompt, no REPL)
//!
//! Commands:
//!   /quit, /exit    Exit the agent
//!   /clear          Clear conversation history
//!   /commit [msg]   Commit staged changes (AI-generates message if no msg)
//!   /docs <crate>   Look up docs.rs documentation for a Rust crate
//!   /docs <c> <i>   Look up a specific item within a crate
//!   /fix            Auto-fix build/lint errors (runs checks, sends failures to AI)
//!   /git <subcmd>   Quick git: status, log, add, diff, branch, stash
//!   /model <name>   Switch model mid-session
//!   /search <query> Search conversation history
//!   /spawn <task>   Spawn a subagent with fresh context
//!   /tree [depth]   Show project directory tree
//!   /test           Auto-detect and run project tests
//!   /lint           Auto-detect and run project linter
//!   /pr [number]    List open PRs, view/diff/comment/checkout a PR
//!   /retry          Re-send the last user input

mod cli;
mod commands;
mod docs;
mod format;
mod git;
mod prompt;
mod repl;

use cli::*;
use format::*;
use prompt::*;

use std::io::{self, IsTerminal, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use yoagent::agent::Agent;
use yoagent::context::ExecutionLimits;
use yoagent::openapi::{OpenApiConfig, OperationFilter};
use yoagent::provider::{
    AnthropicProvider, GoogleProvider, ModelConfig, OpenAiCompat, OpenAiCompatProvider,
};
use yoagent::tools::bash::BashTool;
use yoagent::tools::edit::EditFileTool;
use yoagent::tools::file::{ReadFileTool, WriteFileTool};
use yoagent::tools::list::ListFilesTool;
use yoagent::tools::search::SearchTool;
use yoagent::types::AgentTool;
use yoagent::*;

/// Build the tool set, optionally with a bash confirmation prompt.
/// When `auto_approve` is false (default), bash commands require user approval.
/// The "always" option sets a session-wide flag so subsequent commands are auto-approved.
/// When `permissions` has patterns, matching commands are auto-approved or auto-denied.
pub fn build_tools(
    auto_approve: bool,
    permissions: &cli::PermissionConfig,
) -> Vec<Box<dyn AgentTool>> {
    let bash = if auto_approve {
        BashTool::default()
    } else {
        let always_approved = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&always_approved);
        let perms = permissions.clone();
        BashTool::default().with_confirm(move |cmd: &str| {
            // If user previously chose "always", skip the prompt
            if flag.load(Ordering::Relaxed) {
                eprintln!(
                    "{GREEN}  ✓ Auto-approved: {RESET}{}",
                    truncate_with_ellipsis(cmd, 120)
                );
                return true;
            }
            // Check permission patterns before prompting
            if let Some(allowed) = perms.check(cmd) {
                if allowed {
                    eprintln!(
                        "{GREEN}  ✓ Permitted: {RESET}{}",
                        truncate_with_ellipsis(cmd, 120)
                    );
                    return true;
                } else {
                    eprintln!(
                        "{RED}  ✗ Denied by permission rule: {RESET}{}",
                        truncate_with_ellipsis(cmd, 120)
                    );
                    return false;
                }
            }
            use std::io::BufRead;
            // Show the command and ask for approval
            eprint!(
                "{YELLOW}  ⚠ Allow: {RESET}{}{YELLOW} ? {RESET}({GREEN}y{RESET}/{RED}n{RESET}/{GREEN}a{RESET}lways) ",
                truncate_with_ellipsis(cmd, 120)
            );
            io::stderr().flush().ok();
            let mut response = String::new();
            let stdin = io::stdin();
            if stdin.lock().read_line(&mut response).is_err() {
                return false;
            }
            let response = response.trim().to_lowercase();
            let approved = matches!(response.as_str(), "y" | "yes" | "a" | "always");
            if matches!(response.as_str(), "a" | "always") {
                flag.store(true, Ordering::Relaxed);
                eprintln!(
                    "{GREEN}  ✓ All subsequent commands will be auto-approved this session.{RESET}"
                );
            }
            approved
        })
    };
    vec![
        Box::new(bash),
        Box::new(ReadFileTool::default()),
        Box::new(WriteFileTool::new()),
        Box::new(EditFileTool::new()),
        Box::new(ListFilesTool::default()),
        Box::new(SearchTool::default()),
    ]
}

/// Create a ModelConfig for non-Anthropic providers.
pub fn create_model_config(provider: &str, model: &str, base_url: Option<&str>) -> ModelConfig {
    match provider {
        "openai" => {
            let mut config = ModelConfig::openai(model, model);
            if let Some(url) = base_url {
                config.base_url = url.to_string();
            }
            config
        }
        "google" => {
            let mut config = ModelConfig::google(model, model);
            if let Some(url) = base_url {
                config.base_url = url.to_string();
            }
            config
        }
        "ollama" => {
            let url = base_url.unwrap_or("http://localhost:11434/v1");
            ModelConfig::local(url, model)
        }
        "openrouter" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "openrouter".into();
            config.base_url = base_url
                .unwrap_or("https://openrouter.ai/api/v1")
                .to_string();
            config.compat = Some(OpenAiCompat::openrouter());
            config
        }
        "xai" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "xai".into();
            config.base_url = base_url.unwrap_or("https://api.x.ai/v1").to_string();
            config.compat = Some(OpenAiCompat::xai());
            config
        }
        "groq" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "groq".into();
            config.base_url = base_url
                .unwrap_or("https://api.groq.com/openai/v1")
                .to_string();
            config.compat = Some(OpenAiCompat::groq());
            config
        }
        "deepseek" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "deepseek".into();
            config.base_url = base_url
                .unwrap_or("https://api.deepseek.com/v1")
                .to_string();
            config.compat = Some(OpenAiCompat::deepseek());
            config
        }
        "mistral" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "mistral".into();
            config.base_url = base_url.unwrap_or("https://api.mistral.ai/v1").to_string();
            config.compat = Some(OpenAiCompat::mistral());
            config
        }
        "cerebras" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "cerebras".into();
            config.base_url = base_url.unwrap_or("https://api.cerebras.ai/v1").to_string();
            config.compat = Some(OpenAiCompat::cerebras());
            config
        }
        "custom" => {
            let url = base_url.unwrap_or("http://localhost:8080/v1");
            ModelConfig::local(url, model)
        }
        _ => {
            // Unknown provider — treat as OpenAI-compatible with custom base URL
            let url = base_url.unwrap_or("http://localhost:8080/v1");
            let mut config = ModelConfig::local(url, model);
            config.provider = provider.to_string();
            config
        }
    }
}

/// Holds all configuration needed to build an Agent.
/// Extracted from the 12-argument `build_agent` function so that
/// creating or rebuilding an agent is just `config.build_agent()`.
pub struct AgentConfig {
    pub model: String,
    pub api_key: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub skills: yoagent::skills::SkillSet,
    pub system_prompt: String,
    pub thinking: ThinkingLevel,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub max_turns: Option<usize>,
    pub auto_approve: bool,
    pub permissions: cli::PermissionConfig,
}

impl AgentConfig {
    /// Build a fresh Agent from this configuration.
    pub fn build_agent(&self) -> Agent {
        let base_url = self.base_url.as_deref();
        let mut agent = if self.provider == "anthropic" && base_url.is_none() {
            // Default Anthropic path — unchanged
            Agent::new(AnthropicProvider)
                .with_system_prompt(&self.system_prompt)
                .with_model(&self.model)
                .with_api_key(&self.api_key)
                .with_thinking(self.thinking)
                .with_skills(self.skills.clone())
                .with_tools(build_tools(self.auto_approve, &self.permissions))
        } else if self.provider == "google" {
            // Google uses its own provider
            let config = create_model_config(&self.provider, &self.model, base_url);
            Agent::new(GoogleProvider)
                .with_system_prompt(&self.system_prompt)
                .with_model(&self.model)
                .with_api_key(&self.api_key)
                .with_thinking(self.thinking)
                .with_skills(self.skills.clone())
                .with_tools(build_tools(self.auto_approve, &self.permissions))
                .with_model_config(config)
        } else {
            // All other providers use OpenAI-compatible API
            let config = create_model_config(&self.provider, &self.model, base_url);
            Agent::new(OpenAiCompatProvider)
                .with_system_prompt(&self.system_prompt)
                .with_model(&self.model)
                .with_api_key(&self.api_key)
                .with_thinking(self.thinking)
                .with_skills(self.skills.clone())
                .with_tools(build_tools(self.auto_approve, &self.permissions))
                .with_model_config(config)
        };

        if let Some(max) = self.max_tokens {
            agent = agent.with_max_tokens(max);
        }
        if let Some(temp) = self.temperature {
            agent.temperature = Some(temp);
        }
        if let Some(turns) = self.max_turns {
            agent = agent.with_execution_limits(ExecutionLimits {
                max_turns: turns,
                ..ExecutionLimits::default()
            });
        }
        agent
    }
}
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check --no-color before any output (must happen before parse_args prints anything)
    // Also auto-disable color when stdout is not a terminal (piped output)
    if args.iter().any(|a| a == "--no-color") || !io::stdout().is_terminal() {
        disable_color();
    }

    let Some(config) = parse_args(&args) else {
        return; // --help or --version was handled
    };

    if config.verbose {
        enable_verbose();
    }

    let continue_session = config.continue_session;
    let output_path = config.output_path;
    let mcp_servers = config.mcp_servers;
    let openapi_specs = config.openapi_specs;
    // Auto-approve in non-interactive modes (piped, --prompt) or when --yes is set
    let is_interactive = io::stdin().is_terminal() && config.prompt_arg.is_none();
    let auto_approve = config.auto_approve || !is_interactive;

    let mut agent_config = AgentConfig {
        model: config.model,
        api_key: config.api_key,
        provider: config.provider,
        base_url: config.base_url,
        skills: config.skills,
        system_prompt: config.system_prompt,
        thinking: config.thinking,
        max_tokens: config.max_tokens,
        temperature: config.temperature,
        max_turns: config.max_turns,
        auto_approve,
        permissions: config.permissions,
    };

    let mut agent = agent_config.build_agent();

    // Connect to MCP servers (--mcp flags)
    let mut mcp_count = 0u32;
    for mcp_cmd in &mcp_servers {
        let parts: Vec<&str> = mcp_cmd.split_whitespace().collect();
        if parts.is_empty() {
            eprintln!("{YELLOW}warning:{RESET} Empty --mcp command, skipping");
            continue;
        }
        let command = parts[0];
        let args_slice: Vec<&str> = parts[1..].to_vec();
        eprintln!("{DIM}  mcp: connecting to {mcp_cmd}...{RESET}");
        // with_mcp_server_stdio consumes self; we must always update agent
        let result = agent
            .with_mcp_server_stdio(command, &args_slice, None)
            .await;
        match result {
            Ok(updated) => {
                agent = updated;
                mcp_count += 1;
                eprintln!("{GREEN}  ✓ mcp: {command} connected{RESET}");
            }
            Err(e) => {
                eprintln!("{RED}  ✗ mcp: failed to connect to '{mcp_cmd}': {e}{RESET}");
                // Agent was consumed on error — rebuild it with previous MCP connections lost
                agent = agent_config.build_agent();
                eprintln!("{DIM}  mcp: agent rebuilt (previous MCP connections lost){RESET}");
            }
        }
    }

    // Load OpenAPI specs (--openapi flags)
    let mut openapi_count = 0u32;
    for spec_path in &openapi_specs {
        eprintln!("{DIM}  openapi: loading {spec_path}...{RESET}");
        let result = agent
            .with_openapi_file(spec_path, OpenApiConfig::default(), &OperationFilter::All)
            .await;
        match result {
            Ok(updated) => {
                agent = updated;
                openapi_count += 1;
                eprintln!("{GREEN}  ✓ openapi: {spec_path} loaded{RESET}");
            }
            Err(e) => {
                eprintln!("{RED}  ✗ openapi: failed to load '{spec_path}': {e}{RESET}");
                // Agent was consumed on error — rebuild it
                agent = agent_config.build_agent();
                eprintln!("{DIM}  openapi: agent rebuilt (previous connections lost){RESET}");
            }
        }
    }

    // --continue / -c: resume last saved session
    if continue_session {
        match std::fs::read_to_string(DEFAULT_SESSION_PATH) {
            Ok(json) => match agent.restore_messages(&json) {
                Ok(_) => {
                    eprintln!(
                        "{DIM}  resumed session: {} messages from {DEFAULT_SESSION_PATH}{RESET}",
                        agent.messages().len()
                    );
                }
                Err(e) => eprintln!("{YELLOW}warning:{RESET} Failed to restore session: {e}"),
            },
            Err(_) => eprintln!("{DIM}  no previous session found ({DEFAULT_SESSION_PATH}){RESET}"),
        }
    }

    // --prompt / -p: single-shot mode with a prompt argument
    if let Some(prompt_text) = config.prompt_arg {
        if agent_config.provider != "anthropic" {
            eprintln!(
                "{DIM}  yoyo (prompt mode) — provider: {}, model: {}{RESET}",
                agent_config.provider, agent_config.model
            );
        } else {
            eprintln!(
                "{DIM}  yoyo (prompt mode) — model: {}{RESET}",
                agent_config.model
            );
        }
        let mut session_total = Usage::default();
        let response = run_prompt(
            &mut agent,
            prompt_text.trim(),
            &mut session_total,
            &agent_config.model,
        )
        .await;
        write_output_file(&output_path, &response);
        return;
    }

    // Piped mode: read all of stdin as a single prompt, run once, exit
    if !io::stdin().is_terminal() {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input).ok();
        let input = input.trim();
        if input.is_empty() {
            eprintln!("No input on stdin.");
            std::process::exit(1);
        }

        eprintln!(
            "{DIM}  yoyo (piped mode) — model: {}{RESET}",
            agent_config.model
        );
        let mut session_total = Usage::default();
        let response = run_prompt(&mut agent, input, &mut session_total, &agent_config.model).await;
        write_output_file(&output_path, &response);
        return;
    }

    // Interactive REPL mode
    repl::run_repl(
        &mut agent_config,
        &mut agent,
        mcp_count,
        openapi_count,
        continue_session,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use commands::{
        build_fix_prompt, build_project_tree, detect_project_type, format_tree_from_paths,
        health_checks_for_project, is_unknown_command, lint_command_for_project,
        run_health_check_for_project, run_health_checks_full_output, test_command_for_project,
        thinking_level_name, ProjectType, KNOWN_COMMANDS,
    };

    #[test]
    fn test_command_parsing_quit() {
        let quit_commands = ["/quit", "/exit"];
        for cmd in &quit_commands {
            assert!(
                *cmd == "/quit" || *cmd == "/exit",
                "Unrecognized quit command: {cmd}"
            );
        }
    }

    #[test]
    fn test_command_parsing_model() {
        let input = "/model claude-opus-4-6";
        assert!(input.starts_with("/model "));
        let model_name = input.trim_start_matches("/model ").trim();
        assert_eq!(model_name, "claude-opus-4-6");
    }

    #[test]
    fn test_command_parsing_model_whitespace() {
        let input = "/model   claude-opus-4-6  ";
        let model_name = input.trim_start_matches("/model ").trim();
        assert_eq!(model_name, "claude-opus-4-6");
    }

    #[test]
    fn test_command_help_recognized() {
        let commands = [
            "/help", "/quit", "/exit", "/clear", "/compact", "/commit", "/config", "/context",
            "/cost", "/docs", "/fix", "/init", "/status", "/tokens", "/save", "/load", "/diff",
            "/undo", "/health", "/retry", "/run", "/history", "/search", "/model", "/think",
            "/version", "/tree", "/pr", "/git", "/test", "/lint", "/spawn",
        ];
        for cmd in &commands {
            assert!(
                KNOWN_COMMANDS.contains(cmd),
                "Command not in KNOWN_COMMANDS: {cmd}"
            );
        }
    }

    #[test]
    fn test_model_switch_updates_variable() {
        let original = "claude-opus-4-6";
        let input = "/model claude-haiku-35";
        let new_model = input.trim_start_matches("/model ").trim();
        assert_ne!(new_model, original);
        assert_eq!(new_model, "claude-haiku-35");
    }

    #[test]
    fn test_bare_model_command_is_recognized() {
        let input = "/model";
        assert_eq!(input, "/model");
        assert!(!input.starts_with("/model "));
    }

    #[test]
    fn test_unknown_slash_command_detection() {
        assert!(is_unknown_command("/foo"));
        assert!(is_unknown_command("/foo bar baz"));
        assert!(is_unknown_command("/unknown argument"));
        // Verify typo-like commands are caught as unknown
        assert!(is_unknown_command("/savefile"));
        assert!(is_unknown_command("/loadfile"));

        assert!(!is_unknown_command("/help"));
        assert!(!is_unknown_command("/quit"));
        assert!(!is_unknown_command("/model"));
        assert!(!is_unknown_command("/model claude-opus-4-6"));
        assert!(!is_unknown_command("/save"));
        assert!(!is_unknown_command("/save myfile.json"));
        assert!(!is_unknown_command("/load"));
        assert!(!is_unknown_command("/load myfile.json"));
        assert!(!is_unknown_command("/config"));
        assert!(!is_unknown_command("/context"));
        assert!(!is_unknown_command("/version"));
    }

    #[test]
    fn test_thinking_level_name() {
        assert_eq!(thinking_level_name(ThinkingLevel::Off), "off");
        assert_eq!(thinking_level_name(ThinkingLevel::Minimal), "minimal");
        assert_eq!(thinking_level_name(ThinkingLevel::Low), "low");
        assert_eq!(thinking_level_name(ThinkingLevel::Medium), "medium");
        assert_eq!(thinking_level_name(ThinkingLevel::High), "high");
    }

    #[test]
    fn test_health_check_function() {
        // run_health_check_for_project skips "cargo test" under #[cfg(test)] to avoid recursion
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_check_for_project(&project_type);
        assert!(
            !results.is_empty(),
            "Health check should return at least one result"
        );
        for (name, passed, _) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
        // "test" check should be excluded under cfg(test)
        assert!(
            !results.iter().any(|(name, _, _)| *name == "test"),
            "cargo test check should be skipped to avoid recursion"
        );
    }

    #[test]
    fn test_detect_project_type_rust() {
        // Current directory has Cargo.toml, so should detect Rust
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(detect_project_type(&cwd), ProjectType::Rust);
    }

    #[test]
    fn test_detect_project_type_node() {
        let tmp = std::env::temp_dir().join("yoyo_test_node");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("package.json"), "{}").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Node);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_python_pyproject() {
        let tmp = std::env::temp_dir().join("yoyo_test_python_pyproject");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("pyproject.toml"), "[project]").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_python_setup_py() {
        let tmp = std::env::temp_dir().join("yoyo_test_python_setup");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("setup.py"), "").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_go() {
        let tmp = std::env::temp_dir().join("yoyo_test_go");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("go.mod"), "module example.com/test").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Go);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_makefile() {
        let tmp = std::env::temp_dir().join("yoyo_test_make");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("Makefile"), "test:\n\techo ok").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Make);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_unknown() {
        let tmp = std::env::temp_dir().join("yoyo_test_unknown");
        let _ = std::fs::create_dir_all(&tmp);
        // Empty dir — no marker files
        assert_eq!(detect_project_type(&tmp), ProjectType::Unknown);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_priority_rust_over_makefile() {
        // If both Cargo.toml and Makefile exist, Rust wins
        let tmp = std::env::temp_dir().join("yoyo_test_priority");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(tmp.join("Makefile"), "test:").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Rust);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_health_checks_for_rust_project() {
        let checks = health_checks_for_project(&ProjectType::Rust);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Rust should have build check");
        assert!(names.contains(&"clippy"), "Rust should have clippy check");
        assert!(names.contains(&"fmt"), "Rust should have fmt check");
        // test is excluded under cfg(test)
        assert!(
            !names.contains(&"test"),
            "test should be excluded in cfg(test)"
        );
    }

    #[test]
    fn test_health_checks_for_node_project() {
        let checks = health_checks_for_project(&ProjectType::Node);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Node should have lint check");
    }

    #[test]
    fn test_health_checks_for_go_project() {
        let checks = health_checks_for_project(&ProjectType::Go);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Go should have build check");
        assert!(names.contains(&"vet"), "Go should have vet check");
    }

    #[test]
    fn test_health_checks_for_python_project() {
        let checks = health_checks_for_project(&ProjectType::Python);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Python should have lint check");
        assert!(names.contains(&"typecheck"), "Python should have typecheck");
    }

    #[test]
    fn test_health_checks_for_unknown_returns_empty() {
        let checks = health_checks_for_project(&ProjectType::Unknown);
        assert!(checks.is_empty(), "Unknown project should return no checks");
    }

    #[test]
    fn test_project_type_display() {
        assert_eq!(format!("{}", ProjectType::Rust), "Rust (Cargo)");
        assert_eq!(format!("{}", ProjectType::Node), "Node.js (npm)");
        assert_eq!(format!("{}", ProjectType::Python), "Python");
        assert_eq!(format!("{}", ProjectType::Go), "Go");
        assert_eq!(format!("{}", ProjectType::Make), "Makefile");
        assert_eq!(format!("{}", ProjectType::Unknown), "Unknown");
    }

    #[test]
    fn test_run_command_recognized() {
        assert!(!is_unknown_command("/run"));
        assert!(!is_unknown_command("/run echo hello"));
        assert!(!is_unknown_command("/run ls -la"));
    }

    #[test]
    fn test_run_shell_command_basic() {
        // Verify run_shell_command doesn't panic on basic commands
        // (output goes to stdout/stderr, we just check it doesn't crash)
        commands::run_shell_command("echo hello");
    }

    #[test]
    fn test_run_shell_command_failing() {
        // Non-zero exit should not panic
        commands::run_shell_command("false");
    }

    #[test]
    fn test_bang_shortcut_matching() {
        // ! prefix should match for /run shortcut
        let bang_matches = |s: &str| s.starts_with('!') && s.len() > 1;
        assert!(bang_matches("!ls"));
        assert!(bang_matches("!echo hello"));
        assert!(bang_matches("! ls")); // space after bang is fine
        assert!(!bang_matches("!")); // bare bang alone should not match
    }

    #[test]
    fn test_run_command_matching() {
        // /run should only match /run or /run <cmd>, not /running
        let run_matches = |s: &str| s == "/run" || s.starts_with("/run ");
        assert!(run_matches("/run"));
        assert!(run_matches("/run echo hello"));
        assert!(!run_matches("/running"));
        assert!(!run_matches("/runaway"));
    }

    #[test]
    fn test_format_tree_from_paths_basic() {
        let paths = vec![
            "Cargo.toml".to_string(),
            "README.md".to_string(),
            "src/cli.rs".to_string(),
            "src/format.rs".to_string(),
            "src/main.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
        assert!(tree.contains("src/"));
        assert!(tree.contains("  main.rs"));
        assert!(tree.contains("  cli.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_nested() {
        let paths = vec![
            "src/main.rs".to_string(),
            "src/utils/helpers.rs".to_string(),
            "src/utils/format.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("src/"));
        assert!(tree.contains("  utils/"));
        assert!(tree.contains("    helpers.rs"));
        assert!(tree.contains("    format.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_limit() {
        let paths = vec![
            "a/b/c/d/deep.txt".to_string(),
            "a/shallow.txt".to_string(),
            "top.txt".to_string(),
        ];
        // depth 1: show dirs at level 0 ('a/'), files at depth ≤ 1
        let tree = format_tree_from_paths(&paths, 1);
        assert!(tree.contains("top.txt"));
        assert!(tree.contains("a/"));
        assert!(tree.contains("  shallow.txt"));
        // Files deeper than max_depth should not appear
        assert!(!tree.contains("deep.txt"));
        // Directory 'b/' is at level 1, beyond max_depth=1 for dirs
        assert!(!tree.contains("b/"));
    }

    #[test]
    fn test_format_tree_from_paths_empty() {
        let paths: Vec<String> = vec![];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_format_tree_from_paths_root_files_only() {
        let paths = vec![
            "Cargo.lock".to_string(),
            "Cargo.toml".to_string(),
            "README.md".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // No directories, just root files
        assert!(!tree.contains('/'));
        assert!(tree.contains("Cargo.lock"));
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_zero() {
        let paths = vec!["README.md".to_string(), "src/main.rs".to_string()];
        let tree = format_tree_from_paths(&paths, 0);
        // Depth 0: only root-level files shown
        assert!(tree.contains("README.md"));
        // main.rs is at depth 1, should not show at depth 0
        assert!(!tree.contains("main.rs"));
    }

    #[test]
    fn test_format_tree_dir_printed_once() {
        let paths = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // "src/" should appear exactly once
        assert_eq!(tree.matches("src/").count(), 1);
    }

    #[test]
    fn test_build_project_tree_runs() {
        // build_project_tree should return something non-empty
        let tree = build_project_tree(3);
        assert!(!tree.is_empty());
        // In a git repo, should contain Cargo.toml; outside one (e.g. cargo-mutants
        // temp dir) the tree still works but uses filesystem walk instead of git ls-files
    }

    #[test]
    fn test_tree_command_recognized() {
        assert!(!is_unknown_command("/tree"));
        assert!(!is_unknown_command("/tree 2"));
        assert!(!is_unknown_command("/tree 5"));
    }

    #[test]
    fn test_pr_command_recognized() {
        assert!(!is_unknown_command("/pr"));
        assert!(!is_unknown_command("/pr 42"));
        assert!(!is_unknown_command("/pr 123"));
    }

    #[test]
    fn test_pr_command_matching() {
        // /pr should match exact or with space separator, not /print etc.
        let pr_matches = |s: &str| s == "/pr" || s.starts_with("/pr ");
        assert!(pr_matches("/pr"));
        assert!(pr_matches("/pr 42"));
        assert!(pr_matches("/pr 123"));
        assert!(!pr_matches("/print"));
        assert!(!pr_matches("/process"));
    }

    #[test]
    fn test_pr_number_parsing() {
        // Verify we can parse a PR number from /pr <number>
        let input = "/pr 42";
        let arg = input.strip_prefix("/pr").unwrap_or("").trim();
        assert_eq!(arg, "42");
        assert!(arg.parse::<u32>().is_ok());
        assert_eq!(arg.parse::<u32>().unwrap(), 42);

        // Bare /pr has empty arg
        let input_bare = "/pr";
        let arg_bare = input_bare.strip_prefix("/pr").unwrap_or("").trim();
        assert!(arg_bare.is_empty());
    }

    #[test]
    fn test_pr_subcommand_list() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args(""), PrSubcommand::List);
        assert_eq!(parse_pr_args("  "), PrSubcommand::List);
    }

    #[test]
    fn test_pr_subcommand_view() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args("42"), PrSubcommand::View(42));
        assert_eq!(parse_pr_args("123"), PrSubcommand::View(123));
        assert_eq!(parse_pr_args("1"), PrSubcommand::View(1));
    }

    #[test]
    fn test_pr_subcommand_diff() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args("42 diff"), PrSubcommand::Diff(42));
        assert_eq!(parse_pr_args("7 diff"), PrSubcommand::Diff(7));
    }

    #[test]
    fn test_pr_subcommand_checkout() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args("42 checkout"), PrSubcommand::Checkout(42));
        assert_eq!(parse_pr_args("99 checkout"), PrSubcommand::Checkout(99));
    }

    #[test]
    fn test_pr_subcommand_comment() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(
            parse_pr_args("42 comment looks good!"),
            PrSubcommand::Comment(42, "looks good!".to_string())
        );
        assert_eq!(
            parse_pr_args("10 comment LGTM, merging now"),
            PrSubcommand::Comment(10, "LGTM, merging now".to_string())
        );
    }

    #[test]
    fn test_pr_subcommand_comment_requires_text() {
        use commands::{parse_pr_args, PrSubcommand};
        // comment without text should show help
        assert_eq!(parse_pr_args("42 comment"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 comment  "), PrSubcommand::Help);
    }

    #[test]
    fn test_pr_subcommand_invalid() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args("abc"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 unknown"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 merge"), PrSubcommand::Help);
    }

    #[test]
    fn test_pr_subcommand_case_insensitive() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args("42 DIFF"), PrSubcommand::Diff(42));
        assert_eq!(parse_pr_args("42 Checkout"), PrSubcommand::Checkout(42));
        assert_eq!(
            parse_pr_args("42 Comment nice work"),
            PrSubcommand::Comment(42, "nice work".to_string())
        );
    }

    #[test]
    fn test_pr_subcommand_recognized() {
        // Subcommands should not be flagged as unknown commands
        assert!(!is_unknown_command("/pr 42 diff"));
        assert!(!is_unknown_command("/pr 42 comment hello"));
        assert!(!is_unknown_command("/pr 42 checkout"));
    }

    #[test]
    fn test_save_load_command_matching() {
        // /save and /load should only match exact word or with space separator
        // This tests the fix for /savefile being treated as /save
        let save_matches = |s: &str| s == "/save" || s.starts_with("/save ");
        let load_matches = |s: &str| s == "/load" || s.starts_with("/load ");

        assert!(save_matches("/save"));
        assert!(save_matches("/save myfile.json"));
        assert!(!save_matches("/savefile"));
        assert!(!save_matches("/saveXYZ"));

        assert!(load_matches("/load"));
        assert!(load_matches("/load myfile.json"));
        assert!(!load_matches("/loadfile"));
        assert!(!load_matches("/loadXYZ"));
    }

    #[test]
    fn test_always_approve_flag_starts_false() {
        // The "always" flag should start as false
        let flag = Arc::new(AtomicBool::new(false));
        assert!(!flag.load(Ordering::Relaxed));
    }

    #[test]
    fn test_always_approve_flag_persists_across_clones() {
        // Simulates the confirm closure: flag is shared via Arc
        let always_approved = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&always_approved);

        // Initially not set
        assert!(!flag_clone.load(Ordering::Relaxed));

        // User answers "always" — set the flag
        always_approved.store(true, Ordering::Relaxed);

        // The clone sees the update (simulates next confirm call)
        assert!(flag_clone.load(Ordering::Relaxed));
    }

    #[test]
    fn test_always_approve_response_matching() {
        // Verify the response matching logic for "always" variants
        let responses_that_approve = ["y", "yes", "a", "always"];
        let responses_that_deny = ["n", "no", "", "maybe", "nope"];

        for r in &responses_that_approve {
            let normalized = r.trim().to_lowercase();
            assert!(
                matches!(normalized.as_str(), "y" | "yes" | "a" | "always"),
                "Expected '{}' to be approved",
                r
            );
        }

        for r in &responses_that_deny {
            let normalized = r.trim().to_lowercase();
            assert!(
                !matches!(normalized.as_str(), "y" | "yes" | "a" | "always"),
                "Expected '{}' to be denied",
                r
            );
        }
    }

    #[test]
    fn test_always_approve_only_on_a_or_always() {
        // Only "a" and "always" should set the persist flag, not "y" or "yes"
        let always_responses = ["a", "always"];
        let single_responses = ["y", "yes"];

        for r in &always_responses {
            let normalized = r.trim().to_lowercase();
            assert!(
                matches!(normalized.as_str(), "a" | "always"),
                "Expected '{}' to trigger always-approve",
                r
            );
        }

        for r in &single_responses {
            let normalized = r.trim().to_lowercase();
            assert!(
                !matches!(normalized.as_str(), "a" | "always"),
                "Expected '{}' NOT to trigger always-approve",
                r
            );
        }
    }

    #[test]
    fn test_always_approve_flag_used_in_confirm_simulation() {
        // End-to-end simulation of the confirm flow with "always"
        let always_approved = Arc::new(AtomicBool::new(false));

        // Simulate three bash commands in sequence
        let commands = ["ls", "echo hello", "cat file.txt"];
        let user_responses = ["a", "", ""]; // user answers "always" first time

        for (i, cmd) in commands.iter().enumerate() {
            let approved = if always_approved.load(Ordering::Relaxed) {
                // Auto-approved — no prompt needed
                true
            } else {
                let response = user_responses[i].trim().to_lowercase();
                let result = matches!(response.as_str(), "y" | "yes" | "a" | "always");
                if matches!(response.as_str(), "a" | "always") {
                    always_approved.store(true, Ordering::Relaxed);
                }
                result
            };

            match i {
                0 => assert!(
                    approved,
                    "First command '{}' should be approved via 'a'",
                    cmd
                ),
                1 => assert!(approved, "Second command '{}' should be auto-approved", cmd),
                2 => assert!(approved, "Third command '{}' should be auto-approved", cmd),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn test_build_tools_returns_six_tools() {
        // build_tools should return 6 tools regardless of auto_approve
        let perms = cli::PermissionConfig::default();
        let tools_approved = build_tools(true, &perms);
        let tools_confirm = build_tools(false, &perms);
        assert_eq!(tools_approved.len(), 6);
        assert_eq!(tools_confirm.len(), 6);
    }

    #[test]
    fn test_agent_config_struct_fields() {
        // AgentConfig should hold all the fields needed to build an agent
        let config = AgentConfig {
            model: "claude-opus-4-6".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "You are helpful.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: Some(4096),
            temperature: Some(0.7),
            max_turns: Some(10),
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
        };
        assert_eq!(config.model, "claude-opus-4-6");
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.provider, "anthropic");
        assert!(config.base_url.is_none());
        assert_eq!(config.system_prompt, "You are helpful.");
        assert_eq!(config.thinking, ThinkingLevel::Off);
        assert_eq!(config.max_tokens, Some(4096));
        assert_eq!(config.temperature, Some(0.7));
        assert_eq!(config.max_turns, Some(10));
        assert!(config.auto_approve);
        assert!(config.permissions.is_empty());
    }

    #[test]
    fn test_agent_config_build_agent_anthropic() {
        // build_agent should produce an Agent for the anthropic provider
        let config = AgentConfig {
            model: "claude-opus-4-6".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test prompt.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
        };
        let agent = config.build_agent();
        // Agent should have 6 tools (bash, read, write, edit, list, search)
        // Agent created successfully — verify it has empty message history
        assert_eq!(agent.messages().len(), 0);
    }

    #[test]
    fn test_agent_config_build_agent_openai() {
        // build_agent should produce an Agent for a non-anthropic provider
        let config = AgentConfig {
            model: "gpt-4o".to_string(),
            api_key: "test-key".to_string(),
            provider: "openai".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: Some(2048),
            temperature: Some(0.5),
            max_turns: Some(20),
            auto_approve: false,
            permissions: cli::PermissionConfig::default(),
        };
        let agent = config.build_agent();
        // Agent created successfully — verify it has empty message history
        assert_eq!(agent.messages().len(), 0);
        assert_eq!(agent.temperature, Some(0.5));
    }

    #[test]
    fn test_agent_config_build_agent_google() {
        // Google provider should also work
        let config = AgentConfig {
            model: "gemini-2.0-flash".to_string(),
            api_key: "test-key".to_string(),
            provider: "google".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
        };
        let agent = config.build_agent();
        // Agent created successfully — verify it has empty message history
        assert_eq!(agent.messages().len(), 0);
    }

    #[test]
    fn test_agent_config_build_agent_with_base_url() {
        // Anthropic with a base_url should use OpenAI-compat path
        let config = AgentConfig {
            model: "claude-opus-4-6".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: Some("http://localhost:8080/v1".to_string()),
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
        };
        let agent = config.build_agent();
        // Agent created successfully — verify it has empty message history
        assert_eq!(agent.messages().len(), 0);
    }

    #[test]
    fn test_agent_config_rebuild_produces_fresh_agent() {
        // Calling build_agent twice should produce two independent agents
        let config = AgentConfig {
            model: "claude-opus-4-6".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
        };
        let agent1 = config.build_agent();
        let agent2 = config.build_agent();
        // Both should have empty message history
        assert_eq!(agent1.messages().len(), 0);
        assert_eq!(agent2.messages().len(), 0);
    }

    #[test]
    fn test_agent_config_mutable_model_switch() {
        // Simulates /model switch: change config.model, rebuild agent
        let mut config = AgentConfig {
            model: "claude-opus-4-6".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
        };
        assert_eq!(config.model, "claude-opus-4-6");
        config.model = "claude-haiku-35".to_string();
        let _agent = config.build_agent();
        assert_eq!(config.model, "claude-haiku-35");
    }

    #[test]
    fn test_agent_config_mutable_thinking_switch() {
        // Simulates /think switch: change config.thinking, rebuild agent
        let mut config = AgentConfig {
            model: "claude-opus-4-6".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
        };
        assert_eq!(config.thinking, ThinkingLevel::Off);
        config.thinking = ThinkingLevel::High;
        let _agent = config.build_agent();
        assert_eq!(config.thinking, ThinkingLevel::High);
    }

    #[test]
    fn test_fix_command_recognized() {
        assert!(!is_unknown_command("/fix"));
        assert!(
            KNOWN_COMMANDS.contains(&"/fix"),
            "/fix should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_run_health_checks_full_output_returns_results() {
        // In a Rust project, should return results with full error output
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_checks_full_output(&project_type);
        assert!(
            !results.is_empty(),
            "Should return at least one check result"
        );
        for (name, passed, _output) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
    }

    #[test]
    fn test_build_fix_prompt_with_failures() {
        let failures = vec![
            (
                "build",
                "error[E0308]: mismatched types\n  --> src/main.rs:42",
            ),
            (
                "clippy",
                "warning: unused variable `x`\n  --> src/lib.rs:10",
            ),
        ];
        let prompt = build_fix_prompt(&failures);
        assert!(prompt.contains("build"), "Prompt should mention build");
        assert!(prompt.contains("clippy"), "Prompt should mention clippy");
        assert!(
            prompt.contains("error[E0308]"),
            "Prompt should include build error"
        );
        assert!(
            prompt.contains("unused variable"),
            "Prompt should include clippy warning"
        );
    }

    #[test]
    fn test_build_fix_prompt_empty_failures() {
        let failures: Vec<(&str, &str)> = vec![];
        let prompt = build_fix_prompt(&failures);
        assert!(
            prompt.is_empty() || prompt.contains("Fix"),
            "Empty failures should produce empty or minimal prompt"
        );
    }

    #[test]
    fn test_test_command_recognized() {
        assert!(!is_unknown_command("/test"));
        assert!(
            KNOWN_COMMANDS.contains(&"/test"),
            "/test should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_test_command_for_rust_project() {
        let cmd = test_command_for_project(&ProjectType::Rust);
        assert!(cmd.is_some(), "Rust project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("cargo"),
            "Rust test label should mention cargo"
        );
        assert_eq!(args[0], "cargo");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_node_project() {
        let cmd = test_command_for_project(&ProjectType::Node);
        assert!(cmd.is_some(), "Node project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("npm"), "Node test label should mention npm");
        assert_eq!(args[0], "npm");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_python_project() {
        let cmd = test_command_for_project(&ProjectType::Python);
        assert!(cmd.is_some(), "Python project should have a test command");
        let (label, _args) = cmd.unwrap();
        assert!(
            label.contains("pytest"),
            "Python test label should mention pytest"
        );
    }

    #[test]
    fn test_test_command_for_go_project() {
        let cmd = test_command_for_project(&ProjectType::Go);
        assert!(cmd.is_some(), "Go project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("go"), "Go test label should mention go");
        assert_eq!(args[0], "go");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_make_project() {
        let cmd = test_command_for_project(&ProjectType::Make);
        assert!(cmd.is_some(), "Make project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("make"),
            "Make test label should mention make"
        );
        assert_eq!(args[0], "make");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_unknown_project() {
        let cmd = test_command_for_project(&ProjectType::Unknown);
        assert!(
            cmd.is_none(),
            "Unknown project should not have a test command"
        );
    }

    #[test]
    fn test_lint_command_recognized() {
        assert!(!is_unknown_command("/lint"));
        assert!(
            KNOWN_COMMANDS.contains(&"/lint"),
            "/lint should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_lint_command_for_rust_project() {
        let cmd = lint_command_for_project(&ProjectType::Rust);
        assert!(cmd.is_some(), "Rust project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("clippy"),
            "Rust lint label should mention clippy"
        );
        assert_eq!(args[0], "cargo");
        assert!(args.contains(&"clippy"));
    }

    #[test]
    fn test_lint_command_for_node_project() {
        let cmd = lint_command_for_project(&ProjectType::Node);
        assert!(cmd.is_some(), "Node project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("eslint"),
            "Node lint label should mention eslint"
        );
        assert_eq!(args[0], "npx");
        assert!(args.contains(&"eslint"));
    }

    #[test]
    fn test_lint_command_for_python_project() {
        let cmd = lint_command_for_project(&ProjectType::Python);
        assert!(cmd.is_some(), "Python project should have a lint command");
        let (label, _args) = cmd.unwrap();
        assert!(
            label.contains("ruff"),
            "Python lint label should mention ruff"
        );
    }

    #[test]
    fn test_lint_command_for_go_project() {
        let cmd = lint_command_for_project(&ProjectType::Go);
        assert!(cmd.is_some(), "Go project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("golangci-lint"),
            "Go lint label should mention golangci-lint"
        );
        assert_eq!(args[0], "golangci-lint");
    }

    #[test]
    fn test_lint_command_for_make_project() {
        let cmd = lint_command_for_project(&ProjectType::Make);
        assert!(cmd.is_none(), "Make project should not have a lint command");
    }

    #[test]
    fn test_lint_command_for_unknown_project() {
        let cmd = lint_command_for_project(&ProjectType::Unknown);
        assert!(
            cmd.is_none(),
            "Unknown project should not have a lint command"
        );
    }

    #[test]
    fn test_docs_command_recognized() {
        assert!(!is_unknown_command("/docs"));
        assert!(!is_unknown_command("/docs serde"));
        assert!(!is_unknown_command("/docs tokio"));
        assert!(
            KNOWN_COMMANDS.contains(&"/docs"),
            "/docs should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_docs_command_matching() {
        // /docs should match exact or with space, not /docstring etc.
        let docs_matches = |s: &str| s == "/docs" || s.starts_with("/docs ");
        assert!(docs_matches("/docs"));
        assert!(docs_matches("/docs serde"));
        assert!(docs_matches("/docs tokio-runtime"));
        assert!(!docs_matches("/docstring"));
        assert!(!docs_matches("/docsify"));
    }

    #[test]
    fn test_docs_crate_arg_extraction() {
        let input = "/docs serde";
        let crate_name = input.trim_start_matches("/docs ").trim();
        assert_eq!(crate_name, "serde");

        let input2 = "/docs tokio-runtime";
        let crate_name2 = input2.trim_start_matches("/docs ").trim();
        assert_eq!(crate_name2, "tokio-runtime");

        // Bare /docs has empty after stripping
        let input_bare = "/docs";
        assert_eq!(input_bare, "/docs");
        assert!(!input_bare.starts_with("/docs "));
    }

    #[test]
    fn test_spawn_command_recognized() {
        assert!(!is_unknown_command("/spawn"));
        assert!(!is_unknown_command("/spawn read src/main.rs and summarize"));
        assert!(
            KNOWN_COMMANDS.contains(&"/spawn"),
            "/spawn should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_spawn_command_matching() {
        // /spawn should match exact or with space separator, not /spawning
        let spawn_matches = |s: &str| s == "/spawn" || s.starts_with("/spawn ");
        assert!(spawn_matches("/spawn"));
        assert!(spawn_matches("/spawn read file"));
        assert!(spawn_matches("/spawn analyze the codebase"));
        assert!(!spawn_matches("/spawning"));
        assert!(!spawn_matches("/spawnpoint"));
    }

    #[test]
    fn test_parse_spawn_task_with_task() {
        use commands::parse_spawn_task;
        let task = parse_spawn_task("/spawn read src/main.rs and summarize");
        assert_eq!(task, Some("read src/main.rs and summarize".to_string()));
    }

    #[test]
    fn test_parse_spawn_task_empty() {
        use commands::parse_spawn_task;
        let task = parse_spawn_task("/spawn");
        assert_eq!(task, None);
    }

    #[test]
    fn test_parse_spawn_task_whitespace_only() {
        use commands::parse_spawn_task;
        let task = parse_spawn_task("/spawn   ");
        assert_eq!(task, None);
    }

    #[test]
    fn test_parse_spawn_task_preserves_full_task() {
        use commands::parse_spawn_task;
        let task = parse_spawn_task("/spawn analyze src/ and list all public functions");
        assert_eq!(
            task,
            Some("analyze src/ and list all public functions".to_string())
        );
    }
}
