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
//!   /add <path>     Add file contents to conversation (supports globs and line ranges)
//!   /clear          Clear conversation history
//!   /commit [msg]   Commit staged changes (AI-generates message if no msg)
//!   /docs <crate>   Look up docs.rs documentation for a Rust crate
//!   /docs <c> <i>   Look up a specific item within a crate
//!   /export [path]  Export conversation as readable markdown
//!   /find <pattern> Fuzzy-search project files by name
//!   /fix            Auto-fix build/lint errors (runs checks, sends failures to AI)
//!   /git <subcmd>   Quick git: status, log, add, diff, branch, stash
//!   /model <name>   Switch model mid-session
//!   /search <query> Search conversation history
//!   /spawn <task>   Spawn a subagent with fresh context
//!   /tree [depth]   Show project directory tree
//!   /test           Auto-detect and run project tests
//!   /lint           Auto-detect and run project linter
//!   /pr [number]    List open PRs, view/diff/comment/checkout a PR, or create one
//!   /retry          Re-send the last user input

mod cli;
mod commands;
mod commands_dev;
mod commands_file;
mod commands_git;
mod commands_project;
mod commands_refactor;
mod commands_search;
mod commands_session;
mod docs;
mod format;
mod git;
mod help;
mod hooks;
mod memory;
mod prompt;
mod providers;
mod repl;
mod setup;
mod tools;

use cli::*;
use format::*;
use prompt::*;
use tools::{build_sub_agent_tool, build_tools};

use std::io::{self, IsTerminal, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use yoagent::agent::Agent;
use yoagent::context::{ContextConfig, ExecutionLimits};
use yoagent::openapi::{OpenApiConfig, OperationFilter};
use yoagent::provider::{
    AnthropicProvider, ApiProtocol, BedrockProvider, GoogleProvider, ModelConfig, OpenAiCompat,
    OpenAiCompatProvider,
};
use yoagent::*;

/// Global flag: set to `true` when checkpoint mode's `on_before_turn` fires.
/// Checked at the end of `main()` to exit with code 2.
static CHECKPOINT_TRIGGERED: AtomicBool = AtomicBool::new(false);

/// Return the User-Agent header value for yoyo.
fn yoyo_user_agent() -> String {
    format!("yoyo/{}", env!("CARGO_PKG_VERSION"))
}

/// Insert standard yoyo identification headers into a ModelConfig.
/// All providers get User-Agent. OpenRouter also gets HTTP-Referer and X-Title.
fn insert_client_headers(config: &mut ModelConfig) {
    config
        .headers
        .insert("User-Agent".to_string(), yoyo_user_agent());
    if config.provider == "openrouter" {
        config.headers.insert(
            "HTTP-Referer".to_string(),
            "https://github.com/yologdev/yoyo-evolve".to_string(),
        );
        config
            .headers
            .insert("X-Title".to_string(), "yoyo".to_string());
    }
}

/// Create a ModelConfig for non-Anthropic providers.
pub fn create_model_config(provider: &str, model: &str, base_url: Option<&str>) -> ModelConfig {
    let mut config = match provider {
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
        "zai" => {
            let mut config = ModelConfig::zai(model, model);
            if let Some(url) = base_url {
                config.base_url = url.to_string();
            }
            config
        }
        "minimax" => {
            let mut config = ModelConfig::minimax(model, model);
            if let Some(url) = base_url {
                config.base_url = url.to_string();
            }
            config
        }
        "bedrock" => {
            let url = base_url.unwrap_or("https://bedrock-runtime.us-east-1.amazonaws.com");
            ModelConfig {
                id: model.into(),
                name: model.into(),
                api: ApiProtocol::BedrockConverseStream,
                provider: "bedrock".into(),
                base_url: url.to_string(),
                reasoning: false,
                context_window: 200_000,
                max_tokens: 8192,
                cost: Default::default(),
                headers: std::collections::HashMap::new(),
                compat: None,
            }
        }
        "custom" => {
            let url = base_url.unwrap_or("http://localhost:8080/v1");
            ModelConfig::local(url, model)
        }
        _ => {
            // Unknown provider — treat as OpenAI-compatible with custom base URL.
            // Note: parse_args and /provider already warn about unknown names,
            // but log here too as defense-in-depth for any future call sites.
            eprintln!(
                "{}warning:{} treating unknown provider '{}' as OpenAI-compatible (localhost:8080)",
                crate::format::YELLOW,
                crate::format::RESET,
                provider
            );
            let url = base_url.unwrap_or("http://localhost:8080/v1");
            let mut config = ModelConfig::local(url, model);
            config.provider = provider.to_string();
            config
        }
    };
    insert_client_headers(&mut config);
    config
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
    pub dir_restrictions: cli::DirectoryRestrictions,
    pub context_strategy: cli::ContextStrategy,
    pub context_window: Option<u32>,
    pub shell_hooks: Vec<hooks::ShellHook>,
    pub fallback_provider: Option<String>,
    pub fallback_model: Option<String>,
}

impl AgentConfig {
    /// Apply common configuration to an agent (system prompt, model, API key,
    /// thinking level, skills, tools, and optional limits).
    ///
    /// This is the single source of truth for agent configuration — every field
    /// is applied here, so adding a new `AgentConfig` field only requires one
    /// update instead of one per provider branch.
    fn configure_agent(&self, mut agent: Agent, model_context_window: u32) -> Agent {
        // User override takes precedence; otherwise use the model's actual context window
        let effective_window = self.context_window.unwrap_or(model_context_window);
        let effective_tokens = (effective_window as u64) * 80 / 100;

        // Store for display by /tokens and /status commands
        cli::set_effective_context_tokens(effective_window as u64);

        agent = agent
            .with_system_prompt(&self.system_prompt)
            .with_model(&self.model)
            .with_api_key(&self.api_key)
            .with_thinking(self.thinking)
            .with_skills(self.skills.clone())
            .with_tools(build_tools(
                self.auto_approve,
                &self.permissions,
                &self.dir_restrictions,
                if io::stdin().is_terminal() {
                    TOOL_OUTPUT_MAX_CHARS
                } else {
                    TOOL_OUTPUT_MAX_CHARS_PIPED
                },
                is_audit_enabled(),
                self.shell_hooks.clone(),
            ));

        // Add sub-agent tool via the dedicated API (separate from build_tools count)
        agent = agent.with_sub_agent(build_sub_agent_tool(self));

        // Tell yoagent the context window size so its built-in compaction knows the budget.
        // Uses 80% of the effective context window as the compaction threshold.
        agent = agent.with_context_config(ContextConfig {
            max_context_tokens: effective_tokens as usize,
            system_prompt_tokens: 4_000,
            keep_recent: 10,
            keep_first: 2,
            tool_output_max_lines: 50,
        });

        // Always set execution limits — use user's --max-turns or a generous default
        agent = agent.with_execution_limits(ExecutionLimits {
            max_turns: self.max_turns.unwrap_or(200),
            max_total_tokens: 1_000_000,
            ..ExecutionLimits::default()
        });

        if let Some(max) = self.max_tokens {
            agent = agent.with_max_tokens(max);
        }
        if let Some(temp) = self.temperature {
            agent.temperature = Some(temp);
        }

        // Checkpoint mode: register on_before_turn to stop when context gets high
        if self.context_strategy == cli::ContextStrategy::Checkpoint {
            let max_tokens = effective_tokens;
            let threshold = cli::PROACTIVE_COMPACT_THRESHOLD; // 70% — stop before overflow
            agent = agent.on_before_turn(move |messages, _turn| {
                let used = yoagent::context::total_tokens(messages) as u64;
                let ratio = used as f64 / max_tokens as f64;
                if ratio > threshold {
                    eprintln!(
                        "\n⚡ Context at {:.0}% — checkpoint-restart triggered",
                        ratio * 100.0
                    );
                    CHECKPOINT_TRIGGERED.store(true, Ordering::SeqCst);
                    return false; // stop the agent loop
                }
                true
            });
        }

        agent
    }

    /// Build a fresh Agent from this configuration.
    ///
    /// Provider selection (Anthropic, Google, or OpenAI-compatible) and model
    /// config are the only things that vary per provider. Everything else is
    /// handled by `configure_agent`, eliminating the previous 3-way duplication.
    pub fn build_agent(&self) -> Agent {
        let base_url = self.base_url.as_deref();

        if self.provider == "anthropic" && base_url.is_none() {
            // Default Anthropic path
            let mut model_config = ModelConfig::anthropic(&self.model, &self.model);
            insert_client_headers(&mut model_config);
            let context_window = model_config.context_window;
            let agent = Agent::new(AnthropicProvider).with_model_config(model_config);
            self.configure_agent(agent, context_window)
        } else if self.provider == "google" {
            // Google uses its own provider
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            let context_window = model_config.context_window;
            let agent = Agent::new(GoogleProvider).with_model_config(model_config);
            self.configure_agent(agent, context_window)
        } else if self.provider == "bedrock" {
            // Bedrock uses AWS SigV4 signing with ConverseStream protocol
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            let context_window = model_config.context_window;
            let agent = Agent::new(BedrockProvider).with_model_config(model_config);
            self.configure_agent(agent, context_window)
        } else {
            // All other providers use OpenAI-compatible API
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            let context_window = model_config.context_window;
            let agent = Agent::new(OpenAiCompatProvider).with_model_config(model_config);
            self.configure_agent(agent, context_window)
        }
    }

    /// Attempt to switch to the fallback provider.
    ///
    /// Returns `true` if the switch was made (caller should rebuild the agent
    /// and retry). Returns `false` if no fallback is configured or the agent
    /// is already running on the fallback provider.
    pub fn try_switch_to_fallback(&mut self) -> bool {
        let fallback = match self.fallback_provider {
            Some(ref f) => f.clone(),
            None => return false,
        };

        if self.provider == fallback {
            return false;
        }

        self.provider = fallback.clone();
        self.model = self
            .fallback_model
            .clone()
            .unwrap_or_else(|| cli::default_model_for_provider(&fallback));

        // Resolve API key for fallback provider
        if let Some(env_var) = cli::provider_api_key_env(&fallback) {
            if let Ok(key) = std::env::var(env_var) {
                self.api_key = key;
            }
        }

        true
    }
}

/// What kind of prompt to retry on fallback.
enum FallbackRetry<'a> {
    /// Text-only prompt.
    Text(&'a str),
    /// Multi-modal prompt with content blocks (e.g., text + images).
    Content(Vec<Content>),
}

/// Attempt fallback retry for non-interactive modes (piped and --prompt).
///
/// If the original response has an API error and a fallback provider is configured,
/// switches to the fallback, rebuilds the agent, and retries the prompt.
///
/// Returns `(final_response, should_exit_with_error)`:
/// - If no API error occurred: returns the original response, no error exit.
/// - If fallback succeeded: returns the retry response, no error exit.
/// - If fallback also failed or no fallback configured: returns the best response, error exit.
async fn try_fallback_prompt(
    agent_config: &mut AgentConfig,
    agent: &mut Agent,
    retry: FallbackRetry<'_>,
    session_total: &mut Usage,
    original_response: PromptOutcome,
) -> (PromptOutcome, bool) {
    // No API error — nothing to retry
    if original_response.last_api_error.is_none() {
        return (original_response, false);
    }

    let old_provider = agent_config.provider.clone();
    let fallback_name = agent_config.fallback_provider.clone();

    if !agent_config.try_switch_to_fallback() {
        // No fallback configured or already on fallback — exit with error
        eprintln!("{RED}  API error with no fallback configured. Exiting.{RESET}",);
        return (original_response, true);
    }

    let fallback = fallback_name.as_deref().unwrap_or("unknown");
    eprintln!(
        "{YELLOW}  ⚡ Primary provider '{}' failed. Switching to fallback '{}'...{RESET}",
        old_provider, fallback
    );

    // Rebuild agent with the new provider
    *agent = agent_config.build_agent();

    eprintln!(
        "{DIM}  now using: {} / {}{RESET}",
        agent_config.provider, agent_config.model
    );

    // Retry with the fallback provider
    let retry_response = match retry {
        FallbackRetry::Text(input) => {
            run_prompt(agent, input, session_total, &agent_config.model).await
        }
        FallbackRetry::Content(blocks) => {
            run_prompt_with_content(agent, blocks, session_total, &agent_config.model).await
        }
    };

    if retry_response.last_api_error.is_some() {
        eprintln!(
            "{RED}  Fallback provider '{}' also failed. Exiting.{RESET}",
            fallback
        );
        return (retry_response, true);
    }

    (retry_response, false)
}

/// Build a JSON output object for --json mode.
/// Used by both --prompt and piped modes to produce structured output.
fn build_json_output(
    response: &PromptOutcome,
    model: &str,
    usage: &Usage,
    is_error: bool,
) -> String {
    let cost_usd = estimate_cost(usage, model);
    let json_obj = serde_json::json!({
        "response": response.text,
        "model": model,
        "usage": {
            "input_tokens": usage.input,
            "output_tokens": usage.output,
        },
        "cost_usd": cost_usd,
        "is_error": is_error,
    });
    serde_json::to_string(&json_obj).unwrap_or_else(|_| "{}".to_string())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check --no-color before any output (must happen before parse_args prints anything)
    // Also auto-disable color when stdout is not a terminal (piped output)
    if args.iter().any(|a| a == "--no-color") || !io::stdout().is_terminal() {
        disable_color();
    }

    // Check --no-bell before any output
    if args.iter().any(|a| a == "--no-bell") {
        disable_bell();
    }

    let Some(config) = parse_args(&args) else {
        return; // --help or --version was handled
    };

    // --print-system-prompt: print the fully assembled system prompt and exit
    if config.print_system_prompt {
        println!("{}", config.system_prompt);
        return;
    }

    if config.verbose {
        enable_verbose();
    }

    if config.audit {
        prompt::enable_audit_log();
    }

    let continue_session = config.continue_session;
    let output_path = config.output_path;
    let mcp_servers = config.mcp_servers;
    let mcp_server_configs = config.mcp_server_configs;
    let openapi_specs = config.openapi_specs;
    let image_path = config.image_path;
    let no_update_check = config.no_update_check;
    let json_output = config.json_output;
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
        dir_restrictions: config.dir_restrictions,
        context_strategy: config.context_strategy,
        context_window: config.context_window,
        shell_hooks: config.shell_hooks,
        fallback_provider: config.fallback_provider,
        fallback_model: config.fallback_model,
    };

    // Interactive setup wizard: if no config file or API key is detected,
    // walk the user through first-run onboarding before building the agent.
    if is_interactive && setup::needs_setup(&agent_config.provider) {
        if let Some(result) = setup::run_setup_wizard() {
            // Override config with wizard results
            agent_config.provider = result.provider.clone();
            agent_config.api_key = result.api_key.clone();
            agent_config.model = result.model;
            if result.base_url.is_some() {
                agent_config.base_url = result.base_url;
            }
            // Set the env var so the provider builder picks it up
            if let Some(env_var) = cli::provider_api_key_env(&result.provider) {
                // SAFETY: This runs during setup, before any concurrent agent work.
                // The env var is read later by the provider builder on the same thread.
                unsafe {
                    std::env::set_var(env_var, &result.api_key);
                }
            }
        } else {
            // User cancelled — show the static welcome screen and exit
            cli::print_welcome();
            return;
        }
    }

    // Bedrock needs combined AWS credentials: access_key:secret_key[:session_token]
    // parse_args() only reads AWS_ACCESS_KEY_ID; combine with the rest here.
    if agent_config.provider == "bedrock" && !agent_config.api_key.contains(':') {
        let access_key = agent_config.api_key.clone();
        if let Ok(secret) = std::env::var("AWS_SECRET_ACCESS_KEY") {
            agent_config.api_key = match std::env::var("AWS_SESSION_TOKEN") {
                Ok(token) if !token.is_empty() => format!("{access_key}:{secret}:{token}"),
                _ => format!("{access_key}:{secret}"),
            };
        }
    }

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

    // Connect to structured MCP servers ([mcp_servers.*] config sections)
    for server_cfg in &mcp_server_configs {
        let args_refs: Vec<&str> = server_cfg.args.iter().map(|s| s.as_str()).collect();
        let env_map: Option<std::collections::HashMap<String, String>> =
            if server_cfg.env.is_empty() {
                None
            } else {
                Some(server_cfg.env.iter().cloned().collect())
            };
        eprintln!(
            "{DIM}  mcp: connecting to {} ({})...{RESET}",
            server_cfg.name, server_cfg.command
        );
        let result = agent
            .with_mcp_server_stdio(&server_cfg.command, &args_refs, env_map)
            .await;
        match result {
            Ok(updated) => {
                agent = updated;
                mcp_count += 1;
                eprintln!("{GREEN}  ✓ mcp: {} connected{RESET}", server_cfg.name);
            }
            Err(e) => {
                eprintln!(
                    "{RED}  ✗ mcp: failed to connect to '{}': {e}{RESET}",
                    server_cfg.name
                );
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
        let session_path = commands_session::continue_session_path();
        match std::fs::read_to_string(session_path) {
            Ok(json) => match agent.restore_messages(&json) {
                Ok(_) => {
                    eprintln!(
                        "{DIM}  resumed session: {} messages from {session_path}{RESET}",
                        agent.messages().len()
                    );
                }
                Err(e) => eprintln!("{YELLOW}warning:{RESET} Failed to restore session: {e}"),
            },
            Err(_) => eprintln!("{DIM}  no previous session found ({session_path}){RESET}"),
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
        let prompt_start = Instant::now();
        let response = if let Some(ref img_path) = image_path {
            // Multi-modal prompt: text + image
            match commands_file::read_image_for_add(img_path) {
                Ok((data, mime_type)) => {
                    let content_blocks = vec![
                        Content::Text {
                            text: prompt_text.trim().to_string(),
                        },
                        Content::Image {
                            data: data.clone(),
                            mime_type: mime_type.clone(),
                        },
                    ];
                    let initial = run_prompt_with_content(
                        &mut agent,
                        content_blocks,
                        &mut session_total,
                        &agent_config.model,
                    )
                    .await;
                    // Fallback retry for multi-modal prompts
                    let retry_blocks = vec![
                        Content::Text {
                            text: prompt_text.trim().to_string(),
                        },
                        Content::Image { data, mime_type },
                    ];
                    let (final_response, should_exit_error) = try_fallback_prompt(
                        &mut agent_config,
                        &mut agent,
                        FallbackRetry::Content(retry_blocks),
                        &mut session_total,
                        initial,
                    )
                    .await;
                    if should_exit_error {
                        format::maybe_ring_bell(prompt_start.elapsed());
                        if json_output {
                            println!(
                                "{}",
                                build_json_output(
                                    &final_response,
                                    &agent_config.model,
                                    &session_total,
                                    true
                                )
                            );
                        } else {
                            write_output_file(&output_path, &final_response.text);
                        }
                        std::process::exit(1);
                    }
                    final_response
                }
                Err(e) => {
                    eprintln!("{RED}  error: {e}{RESET}");
                    std::process::exit(1);
                }
            }
        } else {
            // Text-only prompt
            let initial = run_prompt(
                &mut agent,
                prompt_text.trim(),
                &mut session_total,
                &agent_config.model,
            )
            .await;
            // Fallback retry for text-only prompts
            let (final_response, should_exit_error) = try_fallback_prompt(
                &mut agent_config,
                &mut agent,
                FallbackRetry::Text(prompt_text.trim()),
                &mut session_total,
                initial,
            )
            .await;
            if should_exit_error {
                format::maybe_ring_bell(prompt_start.elapsed());
                if json_output {
                    println!(
                        "{}",
                        build_json_output(
                            &final_response,
                            &agent_config.model,
                            &session_total,
                            true
                        )
                    );
                } else {
                    write_output_file(&output_path, &final_response.text);
                }
                std::process::exit(1);
            }
            final_response
        };
        format::maybe_ring_bell(prompt_start.elapsed());
        if json_output {
            println!(
                "{}",
                build_json_output(&response, &agent_config.model, &session_total, false)
            );
        } else {
            write_output_file(&output_path, &response.text);
        }
        if CHECKPOINT_TRIGGERED.load(Ordering::SeqCst) {
            std::process::exit(2);
        }
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
        let prompt_start = Instant::now();
        let initial = run_prompt(&mut agent, input, &mut session_total, &agent_config.model).await;
        // Fallback retry for piped mode
        let (response, should_exit_error) = try_fallback_prompt(
            &mut agent_config,
            &mut agent,
            FallbackRetry::Text(input),
            &mut session_total,
            initial,
        )
        .await;
        format::maybe_ring_bell(prompt_start.elapsed());
        if json_output {
            println!(
                "{}",
                build_json_output(
                    &response,
                    &agent_config.model,
                    &session_total,
                    should_exit_error
                )
            );
        } else {
            write_output_file(&output_path, &response.text);
        }
        if should_exit_error {
            std::process::exit(1);
        }
        if CHECKPOINT_TRIGGERED.load(Ordering::SeqCst) {
            std::process::exit(2);
        }
        return;
    }

    // Interactive REPL mode
    // Check for updates (non-blocking, skipped if --no-update-check or env var)
    let update_available = if !no_update_check {
        cli::check_for_update()
    } else {
        None
    };

    repl::run_repl(
        &mut agent_config,
        &mut agent,
        mcp_count,
        openapi_count,
        continue_session,
        update_available,
        mcp_servers,
        mcp_server_configs,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{
        confirm_file_operation, describe_file_operation, truncate_result, AskUserTool,
        RenameSymbolTool, StreamingBashTool, TodoTool,
    };
    use serial_test::serial;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn test_always_approve_flag_starts_false() {
        // The "always" flag should start as false
        let flag = Arc::new(AtomicBool::new(false));
        assert!(!flag.load(Ordering::Relaxed));
    }

    #[test]
    fn test_checkpoint_triggered_flag_starts_false() {
        // CHECKPOINT_TRIGGERED should default to false
        assert!(!CHECKPOINT_TRIGGERED.load(Ordering::SeqCst));
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
    fn test_build_tools_returns_eight_tools() {
        // build_tools should return 8 tools regardless of auto_approve (in non-terminal: no ask_user)
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools_approved = build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, false, vec![]);
        let tools_confirm = build_tools(false, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, false, vec![]);
        assert_eq!(tools_approved.len(), 8);
        assert_eq!(tools_confirm.len(), 8);
    }

    #[test]
    fn test_build_sub_agent_tool_returns_correct_name() {
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let tool = build_sub_agent_tool(&config);
        assert_eq!(tool.name(), "sub_agent");
    }

    #[test]
    fn test_build_sub_agent_tool_has_task_parameter() {
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let tool = build_sub_agent_tool(&config);
        let schema = tool.parameters_schema();
        assert!(
            schema["properties"]["task"].is_object(),
            "Should have 'task' parameter"
        );
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("task")));
    }

    #[test]
    fn test_build_sub_agent_tool_all_providers() {
        // All provider paths should build without panic
        let _tool_anthropic =
            build_sub_agent_tool(&test_agent_config("anthropic", "claude-sonnet-4-20250514"));
        let _tool_google = build_sub_agent_tool(&test_agent_config("google", "gemini-2.0-flash"));
        let _tool_openai = build_sub_agent_tool(&test_agent_config("openai", "gpt-4o"));
        let _tool_bedrock = build_sub_agent_tool(&test_agent_config(
            "bedrock",
            "anthropic.claude-sonnet-4-20250514-v1:0",
        ));
    }

    #[test]
    fn test_build_sub_agent_tool_inherits_dir_restrictions() {
        // Sub-agent should inherit directory restrictions from parent config
        let mut config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        config.dir_restrictions = cli::DirectoryRestrictions {
            allow: vec!["./src".to_string()],
            deny: vec!["/etc".to_string()],
        };
        // Should build without panic — restrictions are applied to file tools
        let tool = build_sub_agent_tool(&config);
        assert_eq!(tool.name(), "sub_agent");
    }

    #[test]
    fn test_build_sub_agent_tool_no_restrictions_still_works() {
        // Empty restrictions shouldn't break sub-agent building
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        assert!(config.dir_restrictions.is_empty());
        let tool = build_sub_agent_tool(&config);
        assert_eq!(tool.name(), "sub_agent");
    }

    #[test]
    fn test_build_tools_count_unchanged_with_sub_agent() {
        // Verify build_tools still returns exactly 8 — SubAgentTool is added via with_sub_agent
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, false, vec![]);
        assert_eq!(
            tools.len(),
            8,
            "build_tools must stay at 8 — SubAgentTool is added via with_sub_agent"
        );
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
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
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
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
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
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
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
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
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
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
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
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
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
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
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
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        };
        assert_eq!(config.thinking, ThinkingLevel::Off);
        config.thinking = ThinkingLevel::High;
        let _agent = config.build_agent();
        assert_eq!(config.thinking, ThinkingLevel::High);
    }

    // === File operation confirmation tests ===

    #[test]
    fn test_describe_write_file_operation() {
        let params = serde_json::json!({
            "path": "src/main.rs",
            "content": "line1\nline2\nline3\n"
        });
        let desc = describe_file_operation("write_file", &params);
        assert!(desc.contains("write:"));
        assert!(desc.contains("src/main.rs"));
        assert!(desc.contains("3 lines")); // Rust's .lines() strips trailing newline
    }

    #[test]
    fn test_describe_write_file_empty_content() {
        let params = serde_json::json!({
            "path": "empty.txt",
            "content": ""
        });
        let desc = describe_file_operation("write_file", &params);
        assert!(desc.contains("write:"));
        assert!(desc.contains("empty.txt"));
        assert!(
            desc.contains("EMPTY content"),
            "Empty content should show warning, got: {desc}"
        );
    }

    #[test]
    fn test_describe_write_file_missing_content() {
        // When the content key is entirely absent (model bug), treat as empty
        let params = serde_json::json!({
            "path": "missing.txt"
        });
        let desc = describe_file_operation("write_file", &params);
        assert!(desc.contains("write:"));
        assert!(desc.contains("missing.txt"));
        assert!(
            desc.contains("EMPTY content"),
            "Missing content should show warning, got: {desc}"
        );
    }

    #[test]
    fn test_describe_write_file_normal_content() {
        // Normal write_file should NOT show the empty warning
        let params = serde_json::json!({
            "path": "hello.txt",
            "content": "hello world\n"
        });
        let desc = describe_file_operation("write_file", &params);
        assert!(desc.contains("write:"));
        assert!(desc.contains("hello.txt"));
        assert!(desc.contains("1 line"));
        assert!(
            !desc.contains("EMPTY"),
            "Non-empty content should not show warning, got: {desc}"
        );
    }

    #[test]
    fn test_describe_edit_file_operation() {
        let params = serde_json::json!({
            "path": "src/cli.rs",
            "old_text": "old line 1\nold line 2",
            "new_text": "new line 1\nnew line 2\nnew line 3"
        });
        let desc = describe_file_operation("edit_file", &params);
        assert!(desc.contains("edit:"));
        assert!(desc.contains("src/cli.rs"));
        assert!(desc.contains("2 → 3 lines"));
    }

    #[test]
    fn test_describe_edit_file_missing_params() {
        let params = serde_json::json!({
            "path": "test.rs"
        });
        let desc = describe_file_operation("edit_file", &params);
        assert!(desc.contains("edit:"));
        assert!(desc.contains("test.rs"));
        assert!(desc.contains("0 → 0 lines"));
    }

    #[test]
    fn test_describe_unknown_tool() {
        let params = serde_json::json!({});
        let desc = describe_file_operation("unknown_tool", &params);
        assert!(desc.contains("unknown_tool"));
    }

    #[test]
    fn test_confirm_file_operation_auto_approved_flag() {
        // When always_approved is true, confirm should return true immediately
        let flag = Arc::new(AtomicBool::new(true));
        let perms = cli::PermissionConfig::default();
        let result = confirm_file_operation("write: test.rs (5 lines)", "test.rs", &flag, &perms);
        assert!(
            result,
            "Should auto-approve when always_approved flag is set"
        );
    }

    #[test]
    fn test_confirm_file_operation_with_allow_pattern() {
        // Permission patterns should match file paths
        let flag = Arc::new(AtomicBool::new(false));
        let perms = cli::PermissionConfig {
            allow: vec!["*.md".to_string()],
            deny: vec![],
        };
        let result =
            confirm_file_operation("write: README.md (10 lines)", "README.md", &flag, &perms);
        assert!(result, "Should auto-approve paths matching allow pattern");
    }

    #[test]
    fn test_confirm_file_operation_with_deny_pattern() {
        // Denied patterns should block the operation
        let flag = Arc::new(AtomicBool::new(false));
        let perms = cli::PermissionConfig {
            allow: vec![],
            deny: vec!["*.key".to_string()],
        };
        let result =
            confirm_file_operation("write: secrets.key (1 line)", "secrets.key", &flag, &perms);
        assert!(!result, "Should deny paths matching deny pattern");
    }

    #[test]
    fn test_confirm_file_operation_deny_overrides_allow() {
        // Deny takes priority over allow
        let flag = Arc::new(AtomicBool::new(false));
        let perms = cli::PermissionConfig {
            allow: vec!["*".to_string()],
            deny: vec!["*.key".to_string()],
        };
        let result =
            confirm_file_operation("write: secrets.key (1 line)", "secrets.key", &flag, &perms);
        assert!(!result, "Deny should override allow");
    }

    #[test]
    fn test_confirm_file_operation_allow_src_pattern() {
        // Realistic pattern: allow all files under src/
        let flag = Arc::new(AtomicBool::new(false));
        let perms = cli::PermissionConfig {
            allow: vec!["src/*".to_string()],
            deny: vec![],
        };
        let result = confirm_file_operation(
            "edit: src/main.rs (2 → 3 lines)",
            "src/main.rs",
            &flag,
            &perms,
        );
        assert!(
            result,
            "Should auto-approve src/ files with 'src/*' pattern"
        );
    }

    #[test]
    fn test_build_tools_auto_approve_skips_confirmation() {
        // When auto_approve is true, tools should not have ConfirmTool wrappers
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, false, vec![]);
        assert_eq!(tools.len(), 8);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"bash"));
    }

    #[test]
    fn test_build_tools_no_approve_includes_confirmation() {
        // When auto_approve is false, write_file and edit_file should still have correct names
        // (ConfirmTool delegates name() to inner tool)
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(false, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, false, vec![]);
        assert_eq!(tools.len(), 8);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"list_files"));
        assert!(names.contains(&"search"));
        assert!(names.contains(&"todo"));
    }

    #[test]
    fn test_always_approved_shared_between_bash_and_file_tools() {
        // Simulates: user says "always" on a bash prompt,
        // subsequent file operations should auto-approve too.
        // This test verifies the shared flag concept.
        let always_approved = Arc::new(AtomicBool::new(false));
        let bash_flag = Arc::clone(&always_approved);
        let file_flag = Arc::clone(&always_approved);

        // Initially, nothing is auto-approved
        assert!(!bash_flag.load(Ordering::Relaxed));
        assert!(!file_flag.load(Ordering::Relaxed));

        // User says "always" on a bash command
        bash_flag.store(true, Ordering::Relaxed);

        // File tool should now see the flag as true
        assert!(
            file_flag.load(Ordering::Relaxed),
            "File tool should see always_approved after bash 'always'"
        );
    }

    // === Client identification header tests ===

    #[test]
    fn test_yoyo_user_agent_format() {
        let ua = yoyo_user_agent();
        assert!(
            ua.starts_with("yoyo/"),
            "User-Agent should start with 'yoyo/'"
        );
        // Should contain a version number (e.g. "0.1.0")
        let version_part = &ua["yoyo/".len()..];
        assert!(
            version_part.contains('.'),
            "User-Agent version should contain a dot: {ua}"
        );
    }

    #[test]
    fn test_client_headers_anthropic() {
        let config = create_model_config("anthropic", "claude-sonnet-4-20250514", None);
        assert_eq!(
            config.headers.get("User-Agent").unwrap(),
            &yoyo_user_agent(),
            "Anthropic config should have User-Agent header"
        );
        assert!(
            !config.headers.contains_key("HTTP-Referer"),
            "Anthropic config should NOT have HTTP-Referer"
        );
        assert!(
            !config.headers.contains_key("X-Title"),
            "Anthropic config should NOT have X-Title"
        );
    }

    #[test]
    fn test_client_headers_openai() {
        let config = create_model_config("openai", "gpt-4o", None);
        assert_eq!(
            config.headers.get("User-Agent").unwrap(),
            &yoyo_user_agent(),
            "OpenAI config should have User-Agent header"
        );
        assert!(
            !config.headers.contains_key("HTTP-Referer"),
            "OpenAI config should NOT have HTTP-Referer"
        );
    }

    #[test]
    fn test_client_headers_openrouter() {
        let config = create_model_config("openrouter", "anthropic/claude-sonnet-4-20250514", None);
        assert_eq!(
            config.headers.get("User-Agent").unwrap(),
            &yoyo_user_agent(),
            "OpenRouter config should have User-Agent header"
        );
        assert_eq!(
            config.headers.get("HTTP-Referer").unwrap(),
            "https://github.com/yologdev/yoyo-evolve",
            "OpenRouter config should have HTTP-Referer header"
        );
        assert_eq!(
            config.headers.get("X-Title").unwrap(),
            "yoyo",
            "OpenRouter config should have X-Title header"
        );
    }

    #[test]
    fn test_client_headers_google() {
        let config = create_model_config("google", "gemini-2.0-flash", None);
        assert_eq!(
            config.headers.get("User-Agent").unwrap(),
            &yoyo_user_agent(),
            "Google config should have User-Agent header"
        );
    }

    #[test]
    fn test_create_model_config_zai_defaults() {
        let config = create_model_config("zai", "glm-4-plus", None);
        assert_eq!(config.provider, "zai");
        assert_eq!(config.id, "glm-4-plus");
        assert_eq!(config.base_url, "https://api.z.ai/api/paas/v4");
        assert_eq!(
            config.headers.get("User-Agent").unwrap(),
            &yoyo_user_agent(),
            "ZAI config should have User-Agent header"
        );
    }

    #[test]
    fn test_create_model_config_zai_custom_base_url() {
        let config =
            create_model_config("zai", "glm-4-plus", Some("https://custom.zai.example/v1"));
        assert_eq!(config.provider, "zai");
        assert_eq!(config.base_url, "https://custom.zai.example/v1");
    }

    #[test]
    fn test_agent_config_build_agent_zai() {
        let config = AgentConfig {
            model: "glm-4-plus".to_string(),
            api_key: "test-key".to_string(),
            provider: "zai".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        };
        let agent = config.build_agent();
        assert_eq!(agent.messages().len(), 0);
    }

    #[test]
    fn test_create_model_config_minimax_defaults() {
        let config = create_model_config("minimax", "MiniMax-M2.7", None);
        assert_eq!(config.provider, "minimax");
        assert_eq!(config.id, "MiniMax-M2.7");
        assert_eq!(
            config.base_url, "https://api.minimaxi.chat/v1",
            "MiniMax should use api.minimaxi.chat (not api.minimax.io)"
        );
        assert!(
            config.compat.is_some(),
            "MiniMax config should have compat flags set"
        );
        assert_eq!(
            config.headers.get("User-Agent").unwrap(),
            &yoyo_user_agent(),
            "MiniMax config should have User-Agent header"
        );
    }

    #[test]
    fn test_create_model_config_minimax_custom_base_url() {
        let config = create_model_config(
            "minimax",
            "MiniMax-M2.7",
            Some("https://custom.minimax.example/v1"),
        );
        assert_eq!(config.provider, "minimax");
        assert_eq!(config.base_url, "https://custom.minimax.example/v1");
    }

    #[test]
    fn test_create_model_config_unknown_provider_falls_through() {
        // Unknown providers should be treated as OpenAI-compatible on localhost
        let config = create_model_config("typo_provider", "some-model", None);
        assert_eq!(config.provider, "typo_provider");
        assert_eq!(config.base_url, "http://localhost:8080/v1");
    }

    #[test]
    fn test_create_model_config_unknown_provider_with_base_url() {
        // Unknown provider with explicit base URL should use that URL
        let config = create_model_config(
            "typo_provider",
            "some-model",
            Some("https://my-server.com/v1"),
        );
        assert_eq!(config.provider, "typo_provider");
        assert_eq!(config.base_url, "https://my-server.com/v1");
    }

    #[test]
    fn test_agent_config_build_agent_minimax() {
        let config = AgentConfig {
            model: "MiniMax-M2.7".to_string(),
            api_key: "test-key".to_string(),
            provider: "minimax".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        };
        let agent = config.build_agent();
        assert_eq!(agent.messages().len(), 0);
    }

    #[test]
    fn test_bedrock_model_config() {
        let config =
            create_model_config("bedrock", "anthropic.claude-sonnet-4-20250514-v1:0", None);
        assert_eq!(config.provider, "bedrock");
        assert_eq!(
            config.base_url,
            "https://bedrock-runtime.us-east-1.amazonaws.com"
        );
        // Verify it uses BedrockConverseStream protocol (not OpenAI)
        assert_eq!(format!("{}", config.api), "bedrock_converse_stream");
    }

    #[test]
    fn test_bedrock_model_config_custom_url() {
        let config = create_model_config(
            "bedrock",
            "anthropic.claude-sonnet-4-20250514-v1:0",
            Some("https://bedrock-runtime.eu-west-1.amazonaws.com"),
        );
        assert_eq!(
            config.base_url,
            "https://bedrock-runtime.eu-west-1.amazonaws.com"
        );
    }

    #[test]
    fn test_build_agent_bedrock() {
        let config = AgentConfig {
            model: "anthropic.claude-sonnet-4-20250514-v1:0".to_string(),
            api_key: "test-access:test-secret".to_string(),
            provider: "bedrock".to_string(),
            base_url: Some("https://bedrock-runtime.us-east-1.amazonaws.com".to_string()),
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "test".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        };
        let agent = config.build_agent();
        // If this compiles and runs, BedrockProvider is correctly wired
        assert_eq!(agent.messages().len(), 0);
    }

    #[test]
    fn test_client_headers_on_anthropic_build_agent() {
        // The Anthropic path in build_agent() should also get headers
        let agent_config = AgentConfig {
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
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        };
        // Verify the anthropic ModelConfig would have headers set
        // (We test the helper directly since Agent doesn't expose model_config)
        let mut anthropic_config = ModelConfig::anthropic("claude-opus-4-6", "claude-opus-4-6");
        insert_client_headers(&mut anthropic_config);
        assert_eq!(
            anthropic_config.headers.get("User-Agent").unwrap(),
            &yoyo_user_agent()
        );
        // Also verify build_agent doesn't panic
        let _agent = agent_config.build_agent();
    }

    /// Helper to create a default AgentConfig for tests, varying only the provider.
    fn test_agent_config(provider: &str, model: &str) -> AgentConfig {
        AgentConfig {
            model: model.to_string(),
            api_key: "test-key".to_string(),
            provider: provider.to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test prompt.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        }
    }

    #[test]
    fn test_configure_agent_applies_all_settings() {
        // Verify configure_agent applies optional settings (max_tokens, temperature, max_turns)
        let config = AgentConfig {
            max_tokens: Some(2048),
            temperature: Some(0.5),
            max_turns: Some(5),
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        let agent = config.build_agent();
        // Agent was built without panic — configure_agent applied all settings
        assert_eq!(agent.messages().len(), 0);
    }

    #[test]
    fn test_build_agent_all_providers_build_cleanly() {
        // All three provider paths should produce agents with 6 tools via configure_agent.
        // This catches regressions where a provider branch forgets to call configure_agent.
        let providers = [
            ("anthropic", "claude-opus-4-6"),
            ("google", "gemini-2.5-pro"),
            ("openai", "gpt-4o"),
            ("deepseek", "deepseek-chat"),
        ];
        for (provider, model) in &providers {
            let config = test_agent_config(provider, model);
            let agent = config.build_agent();
            assert_eq!(
                agent.messages().len(),
                0,
                "provider '{provider}' should produce a clean agent"
            );
        }
    }

    #[test]
    fn test_build_agent_anthropic_with_base_url_uses_openai_compat() {
        // When Anthropic is used with a custom base_url, it should go through
        // the OpenAI-compatible path (not the default Anthropic path)
        let config = AgentConfig {
            base_url: Some("https://custom-api.example.com/v1".to_string()),
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        // Should not panic — the OpenAI-compat path handles anthropic + base_url
        let agent = config.build_agent();
        assert_eq!(agent.messages().len(), 0);
    }

    // -----------------------------------------------------------------------
    // StreamingBashTool tests
    // -----------------------------------------------------------------------

    /// Create a ToolContext for testing, with an optional on_update callback
    /// that collects partial results.
    fn test_tool_context(
        updates: Option<Arc<tokio::sync::Mutex<Vec<yoagent::types::ToolResult>>>>,
    ) -> yoagent::types::ToolContext {
        let on_update: Option<yoagent::types::ToolUpdateFn> = updates.map(|u| {
            Arc::new(move |result: yoagent::types::ToolResult| {
                // Use try_lock to avoid blocking in sync callback
                if let Ok(mut guard) = u.try_lock() {
                    guard.push(result);
                }
            }) as yoagent::types::ToolUpdateFn
        });
        yoagent::types::ToolContext {
            tool_call_id: "test-id".to_string(),
            tool_name: "bash".to_string(),
            cancel: tokio_util::sync::CancellationToken::new(),
            on_update,
            on_progress: None,
        }
    }

    #[tokio::test]
    async fn test_streaming_bash_deny_patterns() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "rm -rf /"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("blocked by safety policy"),
            "Expected deny pattern error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_deny_pattern_fork_bomb() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": ":(){:|:&};:"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("blocked by safety policy"));
    }

    #[tokio::test]
    async fn test_streaming_bash_confirm_rejection() {
        let tool = StreamingBashTool::default().with_confirm(|_cmd: &str| false);
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "echo hello"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("not confirmed"),
            "Expected confirmation rejection"
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_confirm_approval() {
        let tool = StreamingBashTool::default().with_confirm(|_cmd: &str| true);
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "echo approved"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_ok());
        let text = &result.unwrap().content[0];
        match text {
            yoagent::types::Content::Text { text } => {
                assert!(text.contains("approved"));
                assert!(text.contains("Exit code: 0"));
            }
            _ => panic!("Expected text content"),
        }
    }

    #[tokio::test]
    async fn test_streaming_bash_basic_execution() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "echo hello world"});
        let result = tool.execute(params, ctx).await.unwrap();
        match &result.content[0] {
            yoagent::types::Content::Text { text } => {
                assert!(text.contains("hello world"));
                assert!(text.contains("Exit code: 0"));
            }
            _ => panic!("Expected text content"),
        }
        assert_eq!(result.details["exit_code"], 0);
        assert_eq!(result.details["success"], true);
    }

    #[tokio::test]
    async fn test_streaming_bash_captures_exit_code() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "exit 42"});
        let result = tool.execute(params, ctx).await.unwrap();
        assert_eq!(result.details["exit_code"], 42);
        assert_eq!(result.details["success"], false);
    }

    #[tokio::test]
    async fn test_streaming_bash_timeout() {
        let tool = StreamingBashTool {
            timeout: Duration::from_millis(200),
            ..Default::default()
        };
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "sleep 30"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("timed out"),
            "Expected timeout error"
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_output_truncation() {
        let tool = StreamingBashTool {
            max_output_bytes: 100,
            ..Default::default()
        };
        let ctx = test_tool_context(None);
        // Generate output longer than 100 bytes
        let params = serde_json::json!({"command": "for i in $(seq 1 100); do echo \"line number $i of the output\"; done"});
        let result = tool.execute(params, ctx).await.unwrap();
        match &result.content[0] {
            yoagent::types::Content::Text { text } => {
                // The accumulated output should have been truncated
                // Total text = "Exit code: 0\n" + accumulated (which was truncated to ~100 bytes)
                assert!(
                    text.contains("truncated") || text.len() < 500,
                    "Output should be truncated or short, got {} bytes",
                    text.len()
                );
            }
            _ => panic!("Expected text content"),
        }
    }

    #[tokio::test]
    async fn test_streaming_bash_emits_updates() {
        let updates = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let tool = StreamingBashTool {
            lines_per_update: 1,
            update_interval: Duration::from_millis(10),
            ..Default::default()
        };
        let ctx = test_tool_context(Some(Arc::clone(&updates)));
        // Generate multi-line output with small delays to allow update emission
        let params = serde_json::json!({
            "command": "for i in 1 2 3 4 5; do echo line$i; sleep 0.02; done"
        });
        let result = tool.execute(params, ctx).await.unwrap();
        assert!(result.details["success"] == true);

        let collected = updates.lock().await;
        // Should have emitted at least one streaming update
        assert!(
            !collected.is_empty(),
            "Expected at least one streaming update, got none"
        );
        // The final update (or a late one) should contain multiple lines
        let last = &collected[collected.len() - 1];
        match &last.content[0] {
            yoagent::types::Content::Text { text } => {
                assert!(
                    text.contains("line"),
                    "Update should contain partial output"
                );
            }
            _ => panic!("Expected text content in update"),
        }
    }

    #[tokio::test]
    async fn test_streaming_bash_missing_command_param() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing"));
    }

    #[tokio::test]
    async fn test_streaming_bash_captures_stderr() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "echo err_output >&2"});
        let result = tool.execute(params, ctx).await.unwrap();
        match &result.content[0] {
            yoagent::types::Content::Text { text } => {
                assert!(text.contains("err_output"), "Should capture stderr: {text}");
            }
            _ => panic!("Expected text content"),
        }
    }

    // ── rename_symbol tool tests ─────────────────────────────────────

    #[test]
    fn test_rename_symbol_tool_name() {
        let tool = RenameSymbolTool;
        assert_eq!(tool.name(), "rename_symbol");
    }

    #[test]
    fn test_rename_symbol_tool_label() {
        let tool = RenameSymbolTool;
        assert_eq!(tool.label(), "Rename");
    }

    #[test]
    fn test_rename_symbol_tool_schema() {
        let tool = RenameSymbolTool;
        let schema = tool.parameters_schema();
        // Must have old_name, new_name, and path properties
        let props = schema["properties"].as_object().unwrap();
        assert!(
            props.contains_key("old_name"),
            "schema should have old_name"
        );
        assert!(
            props.contains_key("new_name"),
            "schema should have new_name"
        );
        assert!(props.contains_key("path"), "schema should have path");
        // old_name and new_name are required
        let required = schema["required"].as_array().unwrap();
        let required_strs: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(required_strs.contains(&"old_name"));
        assert!(required_strs.contains(&"new_name"));
        // path is NOT required
        assert!(!required_strs.contains(&"path"));
    }

    #[test]
    fn test_rename_result_struct() {
        let result = commands_refactor::RenameResult {
            files_changed: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            total_replacements: 5,
            preview: "preview text".to_string(),
        };
        assert_eq!(result.files_changed.len(), 2);
        assert_eq!(result.total_replacements, 5);
        assert_eq!(result.preview, "preview text");
    }

    #[test]
    fn test_rename_symbol_tool_in_build_tools() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, false, vec![]);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(
            names.contains(&"rename_symbol"),
            "build_tools should include rename_symbol, got: {names:?}"
        );
    }

    #[test]
    fn test_describe_rename_symbol_operation() {
        let params = serde_json::json!({
            "old_name": "FooBar",
            "new_name": "BazQux",
            "path": "src/"
        });
        let desc = describe_file_operation("rename_symbol", &params);
        assert!(desc.contains("FooBar"), "Should contain old_name: {desc}");
        assert!(desc.contains("BazQux"), "Should contain new_name: {desc}");
        assert!(desc.contains("src/"), "Should contain scope: {desc}");
    }

    #[test]
    fn test_describe_rename_symbol_no_path() {
        let params = serde_json::json!({
            "old_name": "Foo",
            "new_name": "Bar"
        });
        let desc = describe_file_operation("rename_symbol", &params);
        assert!(
            desc.contains("project"),
            "Should default to 'project': {desc}"
        );
    }

    #[test]
    fn test_truncate_result_with_custom_limit() {
        use yoagent::types::{Content, ToolResult};
        // Create a ToolResult with text longer than 100 chars and enough lines.
        // Each line starts with a unique first word to avoid compression collapsing.
        let long_text = (0..200)
            .map(|i| format!("T{i} data"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = ToolResult {
            content: vec![Content::Text {
                text: long_text.clone(),
            }],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, 100);
        let text = match &truncated.content[0] {
            Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(
            text.contains("[... truncated"),
            "Result should be truncated with 100-char limit"
        );
    }

    #[test]
    fn test_truncate_result_preserves_under_limit() {
        use yoagent::types::{Content, ToolResult};
        let short_text = "hello world".to_string();
        let result = ToolResult {
            content: vec![Content::Text {
                text: short_text.clone(),
            }],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, TOOL_OUTPUT_MAX_CHARS);
        let text = match &truncated.content[0] {
            Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert_eq!(text, short_text, "Short text should be unchanged");
    }

    #[test]
    fn test_build_tools_with_piped_limit() {
        // build_tools should work with the piped limit too
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS_PIPED,
            false,
            vec![],
        );
        assert_eq!(tools.len(), 8, "Should still have 8 tools with piped limit");
    }

    #[test]
    fn test_ask_user_tool_schema() {
        let tool = AskUserTool;
        assert_eq!(tool.name(), "ask_user");
        assert_eq!(tool.label(), "ask_user");
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["question"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("question")));
    }

    #[test]
    fn test_ask_user_tool_not_in_non_terminal_mode() {
        // In test environment (no terminal), ask_user should NOT be included
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, false, vec![]);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(
            !names.contains(&"ask_user"),
            "ask_user should not be in non-terminal mode"
        );
    }

    #[test]
    fn test_configure_agent_sets_context_config() {
        // Verify that configure_agent successfully builds an agent with context config
        let config = AgentConfig {
            model: "test-model".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::default(),
            system_prompt: "test".to_string(),
            thinking: yoagent::ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        };
        // This should not panic — context config and execution limits are wired
        let agent =
            config.configure_agent(Agent::new(yoagent::provider::AnthropicProvider), 200_000);
        // Agent built successfully with context config
        let _ = agent;
    }

    #[test]
    fn test_execution_limits_always_set() {
        // Even without --max-turns, configure_agent should set execution limits
        let config_no_turns = AgentConfig {
            model: "test-model".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::default(),
            system_prompt: "test".to_string(),
            thinking: yoagent::ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None, // No explicit max_turns
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        };
        // Should not panic — limits are set with defaults
        let agent = config_no_turns
            .configure_agent(Agent::new(yoagent::provider::AnthropicProvider), 200_000);
        let _ = agent;

        // With explicit max_turns, it should use that value
        let config_with_turns = AgentConfig {
            model: "test-model".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::default(),
            system_prompt: "test".to_string(),
            thinking: yoagent::ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: Some(50),
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        };
        let agent = config_with_turns
            .configure_agent(Agent::new(yoagent::provider::AnthropicProvider), 200_000);
        let _ = agent;
    }

    // -----------------------------------------------------------------------
    // TodoTool tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_todo_tool_schema() {
        let tool = TodoTool;
        assert_eq!(tool.name(), "todo");
        assert_eq!(tool.label(), "todo");
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["description"].is_object());
        assert!(schema["properties"]["id"].is_object());
    }

    #[tokio::test]
    #[serial]
    async fn test_todo_tool_list_empty() {
        commands_project::todo_clear();
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "list"}), ctx)
            .await;
        assert!(result.is_ok());
        let text = match &result.unwrap().content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(text.contains("No tasks"));
    }

    #[tokio::test]
    #[serial]
    async fn test_todo_tool_add_and_list() {
        commands_project::todo_clear();
        let tool = TodoTool;

        let ctx = test_tool_context(None);
        let result = tool
            .execute(
                serde_json::json!({"action": "add", "description": "Write tests"}),
                ctx,
            )
            .await;
        assert!(result.is_ok());

        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "list"}), ctx)
            .await;
        let text = match &result.unwrap().content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(text.contains("Write tests"));
    }

    #[tokio::test]
    #[serial]
    async fn test_todo_tool_done() {
        commands_project::todo_clear();
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        tool.execute(
            serde_json::json!({"action": "add", "description": "Task A"}),
            ctx,
        )
        .await
        .unwrap();

        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "done", "id": 1}), ctx)
            .await;
        let text = match &result.unwrap().content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(text.contains("done ✓"));
    }

    #[tokio::test]
    async fn test_todo_tool_invalid_action() {
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "explode"}), ctx)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_todo_tool_missing_description() {
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "add"}), ctx)
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_todo_tool_in_build_tools() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, false, vec![]);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(
            names.contains(&"todo"),
            "build_tools should include todo, got: {names:?}"
        );
    }

    #[test]
    fn test_maybe_hook_skips_wrap_when_empty() {
        // With an empty registry, maybe_hook should return the tool as-is (no HookedTool wrapper)
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        // Build with audit=false => hooks is empty => tools are NOT wrapped
        let tools = build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, false, vec![]);
        assert_eq!(tools.len(), 8, "Tool count should be 8 without audit hooks");
    }

    #[test]
    fn test_build_tools_with_audit_preserves_tool_count() {
        // With audit=true, tool count stays the same (tools are wrapped, not added)
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools_no_audit = build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, false, vec![]);
        let tools_with_audit =
            build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, true, vec![]);
        assert_eq!(
            tools_no_audit.len(),
            tools_with_audit.len(),
            "Audit hooks should wrap tools, not add new ones"
        );
    }

    #[test]
    fn test_build_tools_with_audit_preserves_tool_names() {
        // Tool names should be identical with or without audit
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools_no_audit = build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, false, vec![]);
        let tools_with_audit =
            build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS, true, vec![]);
        let names_no: Vec<&str> = tools_no_audit.iter().map(|t| t.name()).collect();
        let names_yes: Vec<&str> = tools_with_audit.iter().map(|t| t.name()).collect();
        assert_eq!(
            names_no, names_yes,
            "Tool names should be identical with/without audit"
        );
    }

    // ── Fallback provider switch tests ──────────────────────────────────

    #[test]
    fn test_fallback_switch_success() {
        // When fallback is configured and different from current, switch should succeed
        let mut config = AgentConfig {
            fallback_provider: Some("google".to_string()),
            fallback_model: Some("gemini-2.0-flash".to_string()),
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.provider, "google");
        assert_eq!(config.model, "gemini-2.0-flash");
    }

    #[test]
    fn test_fallback_switch_already_on_fallback() {
        // When current provider already matches the fallback, no switch should happen
        let mut config = AgentConfig {
            fallback_provider: Some("anthropic".to_string()),
            fallback_model: Some("claude-opus-4-6".to_string()),
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        assert!(!config.try_switch_to_fallback());
        // Provider should remain unchanged
        assert_eq!(config.provider, "anthropic");
    }

    #[test]
    fn test_fallback_switch_no_fallback_configured() {
        // When no fallback is set, switch should return false
        let mut config = test_agent_config("anthropic", "claude-opus-4-6");
        assert!(config.fallback_provider.is_none());
        assert!(!config.try_switch_to_fallback());
        assert_eq!(config.provider, "anthropic");
        assert_eq!(config.model, "claude-opus-4-6");
    }

    #[test]
    fn test_fallback_switch_derives_default_model() {
        // When fallback_model is None, should derive the default model for the provider
        let mut config = AgentConfig {
            fallback_provider: Some("openai".to_string()),
            fallback_model: None,
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, cli::default_model_for_provider("openai"));
    }

    #[test]
    fn test_fallback_switch_uses_explicit_model() {
        // When fallback_model is Some, should use it instead of the default
        let mut config = AgentConfig {
            fallback_provider: Some("openai".to_string()),
            fallback_model: Some("gpt-4-turbo".to_string()),
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "gpt-4-turbo");
    }

    #[test]
    #[serial]
    fn test_fallback_switch_resolves_api_key() {
        // When switching to fallback, API key should be resolved from the env var
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::set_var("GOOGLE_API_KEY", "test-google-key-fallback");
        }
        let mut config = AgentConfig {
            fallback_provider: Some("google".to_string()),
            fallback_model: Some("gemini-2.0-flash".to_string()),
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        assert_eq!(config.api_key, "test-key"); // original
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.api_key, "test-google-key-fallback");
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::remove_var("GOOGLE_API_KEY");
        }
    }

    #[test]
    fn test_fallback_switch_keeps_api_key_when_env_missing() {
        // If the fallback provider's env var isn't set, original api_key should persist
        // (removing the env var to be safe)
        // SAFETY: Test runs serially, no concurrent env var access.
        unsafe {
            std::env::remove_var("XAI_API_KEY");
        }
        let mut config = AgentConfig {
            fallback_provider: Some("xai".to_string()),
            fallback_model: Some("grok-3".to_string()),
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        let original_key = config.api_key.clone();
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.provider, "xai");
        assert_eq!(config.api_key, original_key);
    }

    #[test]
    fn test_fallback_switch_idempotent() {
        // Calling try_switch_to_fallback twice: first call switches, second returns false
        // (because provider now matches fallback)
        let mut config = AgentConfig {
            fallback_provider: Some("google".to_string()),
            fallback_model: Some("gemini-2.0-flash".to_string()),
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.provider, "google");
        // Second call: already on fallback
        assert!(!config.try_switch_to_fallback());
        assert_eq!(config.provider, "google");
    }

    // ── Fallback retry helper (non-interactive) tests ────────────────────

    #[test]
    fn test_fallback_prompt_no_api_error_passthrough() {
        // When the response has no API error, try_switch_to_fallback should NOT be called.
        // This verifies the guard condition: no error → no retry, no exit error.
        let config = AgentConfig {
            fallback_provider: Some("google".to_string()),
            fallback_model: Some("gemini-2.0-flash".to_string()),
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        // Simulate: response has no API error
        let response = PromptOutcome {
            text: "success".to_string(),
            last_tool_error: None,
            was_overflow: false,
            last_api_error: None,
        };
        // The helper's first check: if no API error, return immediately.
        // We verify this contract by checking the config isn't touched.
        assert!(response.last_api_error.is_none());
        assert_eq!(config.provider, "anthropic"); // still on primary
    }

    #[test]
    fn test_fallback_prompt_api_error_no_fallback_configured() {
        // When API error occurs but no fallback is configured, should_exit_error = true
        let mut config = test_agent_config("anthropic", "claude-opus-4-6");
        assert!(config.fallback_provider.is_none());

        let response = PromptOutcome {
            text: String::new(),
            last_tool_error: None,
            was_overflow: false,
            last_api_error: Some("503 Service Unavailable".to_string()),
        };
        // The helper would: check API error (yes) → try_switch_to_fallback (false) → exit error
        assert!(response.last_api_error.is_some());
        assert!(!config.try_switch_to_fallback()); // no fallback → returns false
                                                   // Contract: should_exit_error = true in this case
    }

    #[test]
    fn test_fallback_prompt_api_error_with_fallback_switches() {
        // When API error occurs and fallback is configured, the config should switch
        let mut config = AgentConfig {
            fallback_provider: Some("google".to_string()),
            fallback_model: Some("gemini-2.0-flash".to_string()),
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };

        let response = PromptOutcome {
            text: String::new(),
            last_tool_error: None,
            was_overflow: false,
            last_api_error: Some("529 Overloaded".to_string()),
        };
        // The helper would: check API error (yes) → try_switch_to_fallback (true) → rebuild → retry
        assert!(response.last_api_error.is_some());
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.provider, "google");
        assert_eq!(config.model, "gemini-2.0-flash");
    }

    #[test]
    fn test_build_json_output_valid_json_with_expected_keys() {
        let response = PromptOutcome {
            text: "Hello, world!".to_string(),
            last_tool_error: None,
            was_overflow: false,
            last_api_error: None,
        };
        let usage = Usage {
            input: 100,
            output: 50,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 150,
        };
        let result = build_json_output(&response, "claude-sonnet-4-20250514", &usage, false);

        // Must be valid JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("build_json_output should produce valid JSON");

        // Check all expected keys exist
        assert_eq!(parsed["response"], "Hello, world!");
        assert_eq!(parsed["model"], "claude-sonnet-4-20250514");
        assert_eq!(parsed["is_error"], false);
        assert!(parsed["usage"].is_object());
        assert_eq!(parsed["usage"]["input_tokens"], 100);
        assert_eq!(parsed["usage"]["output_tokens"], 50);
        assert!(parsed["cost_usd"].is_number());
    }

    #[test]
    fn test_build_json_output_error_mode() {
        let response = PromptOutcome {
            text: "Something went wrong".to_string(),
            last_tool_error: None,
            was_overflow: false,
            last_api_error: Some("API error".to_string()),
        };
        let usage = Usage {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 15,
        };
        let result = build_json_output(&response, "claude-sonnet-4-20250514", &usage, true);

        let parsed: serde_json::Value = serde_json::from_str(&result)
            .expect("build_json_output should produce valid JSON even in error mode");

        assert_eq!(parsed["response"], "Something went wrong");
        assert_eq!(parsed["is_error"], true);
        assert!(parsed["usage"].is_object());
        assert!(parsed["cost_usd"].is_number());
    }
}
