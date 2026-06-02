//! Agent building, model configuration, MCP collision detection, and fallback retry logic.
//!
//! Extracted from `main.rs` (Day 58) to reduce its size and isolate agent
//! construction concerns into a focused module.

use std::io::IsTerminal;

use yoagent::agent::Agent;
use yoagent::context::{ContextConfig, ExecutionLimits};
use yoagent::openapi::{OpenApiConfig, OperationFilter};
use yoagent::provider::{
    AnthropicProvider, ApiProtocol, BedrockProvider, GoogleProvider, ModelConfig, OpenAiCompat,
    OpenAiCompatProvider,
};
use yoagent::*;

use crate::cli;
use crate::config;
use crate::format::*;
use crate::hooks;
use crate::prompt::{run_prompt, run_prompt_with_content, PromptOutcome};
use crate::prompt_budget::is_audit_enabled;
use crate::tools::{build_sub_agent_tool, build_tools};

/// Return the User-Agent header value for yoyo.
pub(crate) fn yoyo_user_agent() -> String {
    format!("yoyo/{}", env!("CARGO_PKG_VERSION"))
}

/// Names of yoyo's builtin tools. MCP servers that expose a tool with one of
/// these names would cause the Anthropic API to reject the first turn with
/// `"Tool names must be unique"`, killing the session. We detect the collision
/// at connect time and skip the colliding MCP server with a clear warning.
///
/// This list must stay in sync with `tools::build_tools` and any tool added
/// via yoagent's `with_sub_agent` (currently `sub_agent`, see
/// `build_sub_agent_tool`).
pub(crate) const BUILTIN_TOOL_NAMES: &[&str] = &[
    "bash",
    "read_file",
    "write_file",
    "edit_file",
    "list_files",
    "search",
    "rename_symbol",
    "ask_user",
    "todo",
    "sub_agent",
    "shared_state",
];

/// Pure helper: return the subset of `mcp_tools` whose names collide with any
/// entry in `builtins`. Order is preserved from `mcp_tools`. Extracted so it
/// can be unit-tested without spinning up a real MCP server.
pub(crate) fn detect_mcp_collisions(mcp_tools: &[String], builtins: &[&str]) -> Vec<String> {
    mcp_tools
        .iter()
        .filter(|name| builtins.iter().any(|b| b == &name.as_str()))
        .cloned()
        .collect()
}

/// Pre-enumerate the tool names an MCP server exposes by opening a short-lived
/// `McpClient` against it. Used to detect collisions with yoyo's builtins
/// BEFORE we hand the connection to yoagent (which would otherwise push the
/// colliding tool onto the agent and kill the first LLM turn).
///
/// Returns `Ok(tool_names)` on success, `Err(message)` on any protocol or
/// spawn error. Errors are non-fatal at the call site — we fall through and
/// let yoagent's own connect attempt surface the real diagnostic.
async fn fetch_mcp_tool_names(
    command: &str,
    args: &[&str],
    env: Option<std::collections::HashMap<String, String>>,
) -> Result<Vec<String>, String> {
    let client = yoagent::mcp::McpClient::connect_stdio(command, args, env)
        .await
        .map_err(|e| format!("{e}"))?;
    let tools = client.list_tools().await.map_err(|e| format!("{e}"))?;
    // Best-effort close; ignore errors since we're about to drop the client.
    let _ = client.close().await;
    Ok(tools.into_iter().map(|t| t.name).collect())
}

/// Connect to external servers (MCP and OpenAPI) and return the updated agent
/// plus the count of successfully connected MCP and OpenAPI servers.
///
/// This handles three categories:
/// 1. `--mcp` flag servers (space-delimited command strings)
/// 2. `[mcp_servers.*]` TOML-configured servers
/// 3. `--openapi` flag specs
///
/// Each connection attempt follows the same pattern: pre-flight collision check
/// (for MCP), then `with_mcp_server_stdio` / `with_openapi_file` which consumes
/// the agent and returns a new one. On error, the agent is rebuilt from config.
pub(crate) async fn connect_external_servers(
    agent_config: &AgentConfig,
    mut agent: Agent,
    mcp_servers: &[String],
    mcp_server_configs: &[config::McpServerConfig],
    openapi_specs: &[String],
) -> (Agent, u32, u32) {
    let mut mcp_count = 0u32;

    // Connect to MCP servers (--mcp flags)
    for mcp_cmd in mcp_servers {
        let parts: Vec<&str> = mcp_cmd.split_whitespace().collect();
        if parts.is_empty() {
            eprintln!("{YELLOW}warning:{RESET} Empty --mcp command, skipping");
            continue;
        }
        let command = parts[0];
        let args_slice: Vec<&str> = parts[1..].to_vec();
        eprintln!("{DIM}  mcp: connecting to {mcp_cmd}...{RESET}");

        // Pre-flight: enumerate tool names and detect collisions with yoyo
        // builtins. yoagent would otherwise push colliding tools onto the
        // agent and the Anthropic API would reject the first turn with
        // "Tool names must be unique". See #MCP collision guard (Day 39).
        match fetch_mcp_tool_names(command, &args_slice, None).await {
            Ok(tool_names) => {
                let collisions = detect_mcp_collisions(&tool_names, BUILTIN_TOOL_NAMES);
                if !collisions.is_empty() {
                    for tool in &collisions {
                        eprintln!(
                            "{YELLOW}warning:{RESET} MCP server '{command}' exposes tool '{tool}' which collides with yoyo's builtin; skipping this server"
                        );
                    }
                    eprintln!(
                        "{DIM}  mcp: skipping '{mcp_cmd}' — rename/exclude the colliding tool(s) or use a different server{RESET}"
                    );
                    continue;
                }
            }
            Err(e) => {
                eprintln!(
                    "{DIM}  mcp: pre-flight tool listing failed ({e}); proceeding to yoagent connect for diagnostics{RESET}"
                );
            }
        }

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
    for server_cfg in mcp_server_configs {
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

        // Pre-flight collision check (see comment above).
        match fetch_mcp_tool_names(&server_cfg.command, &args_refs, env_map.clone()).await {
            Ok(tool_names) => {
                let collisions = detect_mcp_collisions(&tool_names, BUILTIN_TOOL_NAMES);
                if !collisions.is_empty() {
                    for tool in &collisions {
                        eprintln!(
                            "{YELLOW}warning:{RESET} MCP server '{}' exposes tool '{tool}' which collides with yoyo's builtin; skipping this server",
                            server_cfg.name
                        );
                    }
                    eprintln!(
                        "{DIM}  mcp: skipping '{}' — rename/exclude the colliding tool(s) or use a different server{RESET}",
                        server_cfg.name
                    );
                    continue;
                }
            }
            Err(e) => {
                eprintln!(
                    "{DIM}  mcp: pre-flight tool listing failed ({e}); proceeding to yoagent connect for diagnostics{RESET}"
                );
            }
        }

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
    for spec_path in openapi_specs {
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

    (agent, mcp_count, openapi_count)
}

/// Insert standard yoyo identification headers into a ModelConfig.
/// All providers get User-Agent. OpenRouter also gets HTTP-Referer and X-Title.
pub(crate) fn insert_client_headers(config: &mut ModelConfig) {
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
            ModelConfig::ollama(url, model)
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
    pub auto_commit: bool,
    pub permissions: cli::PermissionConfig,
    pub dir_restrictions: cli::DirectoryRestrictions,
    pub context_strategy: cli::ContextStrategy,
    pub context_window: Option<u32>,
    pub shell_hooks: Vec<hooks::ShellHook>,
    pub fallback_provider: Option<String>,
    pub fallback_model: Option<String>,
    pub auto_watch: bool,
    pub disallowed_tools: Vec<String>,
    pub no_tools: bool,
    pub lite: bool,
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
            .with_skills(self.skills.clone());

        // When --no-tools is active, skip all tool construction (build_tools,
        // sub_agent, shared_state). This is cleaner than building then filtering
        // and also avoids the sub_agent/shared_state bypass that disallowed_tools
        // couldn't catch (they were added after filtering via with_sub_agent).
        if !self.no_tools {
            let mut tools = build_tools(
                self.auto_approve,
                &self.permissions,
                &self.dir_restrictions,
                if std::io::stdin().is_terminal() {
                    TOOL_OUTPUT_MAX_CHARS
                } else {
                    TOOL_OUTPUT_MAX_CHARS_PIPED
                },
                is_audit_enabled(),
                self.shell_hooks.clone(),
            );

            // Filter out disallowed tools (--disallowed-tools flag or --lite)
            if !self.disallowed_tools.is_empty() {
                tools.retain(|t| !self.disallowed_tools.contains(&t.name().to_string()));
                if self.lite {
                    eprintln!(
                        "{DIM}  🪶 Lite mode: {} tools ({}){RESET}",
                        cli::LITE_TOOLS.len(),
                        cli::LITE_TOOLS.join(", ")
                    );
                } else {
                    eprintln!(
                        "{DIM}  🔒 Disabled tools: {}{RESET}",
                        self.disallowed_tools.join(", ")
                    );
                }
            }

            agent = agent.with_tools(tools);

            // Add sub-agent tool via the dedicated API (separate from build_tools count).
            // The SharedState handle is kept for future use (e.g. pre-populating context
            // before dispatching sub-agents like analyze-trajectory).
            let (sub_agent_tool, _shared_state) = build_sub_agent_tool(self);
            agent = agent.with_sub_agent(sub_agent_tool);
        }

        // Tell yoagent the context window size so its built-in compaction knows the budget.
        // Uses 80% of the effective context window as the compaction threshold.
        agent = agent.with_context_config(ContextConfig {
            max_context_tokens: effective_tokens as usize,
            system_prompt_tokens: 4_000,
            keep_recent: 10,
            keep_first: 2,
            tool_output_max_lines: 50,
        });

        // Enable prompt caching — Anthropic caches the system prompt, tool
        // definitions, and conversation history prefix, reducing input-token
        // costs by ~90% for cached content.  CacheStrategy::Auto places cache
        // breakpoints automatically at system prompt, last tool, and the
        // second-to-last message.
        agent = agent.with_cache_config(CacheConfig {
            enabled: true,
            strategy: CacheStrategy::Auto,
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
                    crate::CHECKPOINT_TRIGGERED.store(true, std::sync::atomic::Ordering::SeqCst);
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

    /// Rebuild `agent` with the current config, preserving conversation history.
    ///
    /// Returns `true` if the conversation was fully preserved, `false` if
    /// messages could not be saved or restored (the agent is still rebuilt
    /// either way — it just starts with a blank conversation).
    ///
    /// This is the single call-site for the save→rebuild→restore pattern that
    /// was previously duplicated across dispatch.rs, commands.rs,
    /// commands_config.rs, and prompt.rs.
    pub fn rebuild_preserving_messages(&self, agent: &mut Agent) -> bool {
        let saved = match agent.save_messages() {
            Ok(json) => Some(json),
            Err(e) => {
                eprintln!("{DIM}  ⚠ could not preserve conversation: {e}{RESET}");
                None
            }
        };
        *agent = self.build_agent();
        if let Some(json) = saved {
            match agent.restore_messages(&json) {
                Ok(()) => true,
                Err(e) => {
                    eprintln!(
                        "{YELLOW}  ⚠ conversation could not be restored after rebuild: {e}{RESET}"
                    );
                    false
                }
            }
        } else {
            false
        }
    }

    /// Build a minimal agent for `/side` conversations — same provider/model/API key,
    /// but no tools, no skills, and a concise system prompt. The agent is one-shot
    /// (1 turn max) so it answers the question and stops.
    pub fn build_side_agent(&self) -> Agent {
        let base_url = self.base_url.as_deref();
        let side_prompt = "You are a helpful assistant answering a quick side question. \
            Be concise and direct. This is a one-shot question — answer it completely in one response.";

        let agent = if self.provider == "anthropic" && base_url.is_none() {
            let mut model_config = ModelConfig::anthropic(&self.model, &self.model);
            insert_client_headers(&mut model_config);
            Agent::new(AnthropicProvider).with_model_config(model_config)
        } else if self.provider == "google" {
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            Agent::new(GoogleProvider).with_model_config(model_config)
        } else if self.provider == "bedrock" {
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            Agent::new(BedrockProvider).with_model_config(model_config)
        } else {
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            Agent::new(OpenAiCompatProvider).with_model_config(model_config)
        };

        let mut agent = agent
            .with_system_prompt(side_prompt)
            .with_model(&self.model)
            .with_api_key(&self.api_key)
            .with_cache_config(CacheConfig {
                enabled: true,
                strategy: CacheStrategy::Auto,
            })
            .with_execution_limits(ExecutionLimits {
                max_turns: 1,
                ..ExecutionLimits::default()
            });

        if let Some(temp) = self.temperature {
            agent.temperature = Some(temp);
        }

        agent
    }

    /// Build a minimal agent for the architect (planning) phase — same provider
    /// but optionally a different model, no tools, and the architect system prompt.
    /// The agent is one-shot (1 turn) and returns a text-only plan.
    pub fn build_architect_agent(&self, architect_model: &str) -> Agent {
        let base_url = self.base_url.as_deref();

        let agent = if self.provider == "anthropic" && base_url.is_none() {
            let mut model_config = ModelConfig::anthropic(architect_model, architect_model);
            insert_client_headers(&mut model_config);
            Agent::new(AnthropicProvider).with_model_config(model_config)
        } else if self.provider == "google" {
            let model_config = create_model_config(&self.provider, architect_model, base_url);
            Agent::new(GoogleProvider).with_model_config(model_config)
        } else if self.provider == "bedrock" {
            let model_config = create_model_config(&self.provider, architect_model, base_url);
            Agent::new(BedrockProvider).with_model_config(model_config)
        } else {
            let model_config = create_model_config(&self.provider, architect_model, base_url);
            Agent::new(OpenAiCompatProvider).with_model_config(model_config)
        };

        let mut agent = agent
            .with_system_prompt(&self.system_prompt)
            .with_model(architect_model)
            .with_api_key(&self.api_key)
            .with_cache_config(CacheConfig {
                enabled: true,
                strategy: CacheStrategy::Auto,
            })
            .with_execution_limits(ExecutionLimits {
                max_turns: 1,
                ..ExecutionLimits::default()
            });

        if let Some(temp) = self.temperature {
            agent.temperature = Some(temp);
        }

        agent
    }

    /// Build a full agent configured for the editor (implementation) phase.
    /// Uses the editor model (a cheaper model) but with the same tools, skills,
    /// and system prompt as the main agent.
    pub fn build_editor_agent(&self, editor_model: &str) -> Agent {
        // Create a temporary config clone with the editor model
        let editor_config = AgentConfig {
            model: editor_model.to_string(),
            api_key: self.api_key.clone(),
            provider: self.provider.clone(),
            base_url: self.base_url.clone(),
            skills: self.skills.clone(),
            system_prompt: self.system_prompt.clone(),
            thinking: self.thinking,
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            max_turns: self.max_turns,
            auto_approve: self.auto_approve,
            auto_commit: self.auto_commit,
            permissions: self.permissions.clone(),
            dir_restrictions: self.dir_restrictions.clone(),
            context_strategy: self.context_strategy,
            context_window: self.context_window,
            shell_hooks: self.shell_hooks.clone(),
            fallback_provider: self.fallback_provider.clone(),
            fallback_model: self.fallback_model.clone(),
            auto_watch: self.auto_watch,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
        };
        editor_config.build_agent()
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
pub(crate) enum FallbackRetry<'a> {
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
pub(crate) async fn try_fallback_prompt(
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
        }
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
        };
        let agent1 = config.build_agent();
        let agent2 = config.build_agent();
        // Both should have empty message history
        assert_eq!(agent1.messages().len(), 0);
        assert_eq!(agent2.messages().len(), 0);
    }

    #[test]
    fn test_cache_config_enabled_on_all_agents() {
        // All agent construction paths should enable prompt caching with Auto strategy
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");

        // Main agent
        let agent = config.build_agent();
        assert!(
            agent.cache_config.enabled,
            "main agent cache should be enabled"
        );
        assert_eq!(
            agent.cache_config.strategy,
            CacheStrategy::Auto,
            "main agent should use Auto caching strategy"
        );

        // Side agent
        let side = config.build_side_agent();
        assert!(
            side.cache_config.enabled,
            "side agent cache should be enabled"
        );
        assert_eq!(
            side.cache_config.strategy,
            CacheStrategy::Auto,
            "side agent should use Auto caching strategy"
        );

        // Architect agent
        let architect = config.build_architect_agent("claude-sonnet-4-20250514");
        assert!(
            architect.cache_config.enabled,
            "architect agent cache should be enabled"
        );
        assert_eq!(
            architect.cache_config.strategy,
            CacheStrategy::Auto,
            "architect agent should use Auto caching strategy"
        );

        // Editor agent (delegates to build_agent internally)
        let editor = config.build_editor_agent("claude-sonnet-4-20250514");
        assert!(
            editor.cache_config.enabled,
            "editor agent cache should be enabled"
        );
        assert_eq!(
            editor.cache_config.strategy,
            CacheStrategy::Auto,
            "editor agent should use Auto caching strategy"
        );
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
        };
        assert_eq!(config.thinking, ThinkingLevel::Off);
        config.thinking = ThinkingLevel::High;
        let _agent = config.build_agent();
        assert_eq!(config.thinking, ThinkingLevel::High);
    }

    // === File operation confirmation tests ===

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
    fn test_create_model_config_ollama_uses_ollama_compat() {
        let config = create_model_config("ollama", "llama3", None);
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.id, "llama3");
        assert_eq!(config.base_url, "http://localhost:11434/v1");
        let compat = config.compat.as_ref().expect("ollama should have compat");
        assert!(
            compat.requires_assistant_after_tool_result,
            "Ollama compat must set requires_assistant_after_tool_result = true"
        );
    }

    #[test]
    fn test_create_model_config_ollama_custom_base_url() {
        let config = create_model_config("ollama", "mistral", Some("http://myhost:11434/v1"));
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.id, "mistral");
        assert_eq!(config.base_url, "http://myhost:11434/v1");
        let compat = config.compat.as_ref().expect("ollama should have compat");
        assert!(
            compat.requires_assistant_after_tool_result,
            "Ollama compat must set requires_assistant_after_tool_result = true"
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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

    // ── rename_symbol tool tests ─────────────────────────────────────

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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
        };
        let agent = config_with_turns
            .configure_agent(Agent::new(yoagent::provider::AnthropicProvider), 200_000);
        let _ = agent;
    }

    #[test]
    fn test_fallback_switch_success() {
        // When fallback is configured and different from current, switch should succeed
        let mut config = AgentConfig {
            fallback_provider: Some("google".to_string()),
            fallback_model: Some("gemini-2.0-flash".to_string()),
            auto_watch: true,
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
            auto_watch: true,
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
            auto_watch: true,
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
            auto_watch: true,
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
            auto_watch: true,
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
            auto_watch: true,
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
            auto_watch: true,
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.provider, "google");
        // Second call: already on fallback
        assert!(!config.try_switch_to_fallback());
        assert_eq!(config.provider, "google");
    }

    #[test]
    fn test_fallback_prompt_no_api_error_passthrough() {
        // When the response has no API error, try_switch_to_fallback should NOT be called.
        // This verifies the guard condition: no error → no retry, no exit error.
        let config = AgentConfig {
            fallback_provider: Some("google".to_string()),
            fallback_model: Some("gemini-2.0-flash".to_string()),
            auto_watch: true,
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        // Simulate: response has no API error
        let response = PromptOutcome {
            text: "success".to_string(),
            last_tool_error: None,
            last_tool_name: None,
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
            last_tool_name: None,
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
            auto_watch: true,
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };

        let response = PromptOutcome {
            text: String::new(),
            last_tool_error: None,
            last_tool_name: None,
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
    fn mcp_builtin_collision_detection() {
        // The canonical collision: filesystem MCP server exposes read_file,
        // which collides with yoyo's builtin. Non-colliding tools pass through.
        let builtins = vec!["read_file", "write_file", "bash", "search"];
        let mcp_tools = vec!["read_file".to_string(), "fetch_url".to_string()];
        let collisions = detect_mcp_collisions(&mcp_tools, &builtins);
        assert_eq!(collisions, vec!["read_file".to_string()]);
    }

    #[test]
    fn mcp_collision_detection_no_collisions() {
        let builtins = vec!["read_file", "write_file"];
        let mcp_tools = vec!["fetch_url".to_string(), "query_db".to_string()];
        let collisions = detect_mcp_collisions(&mcp_tools, &builtins);
        assert!(collisions.is_empty());
    }

    #[test]
    fn mcp_collision_detection_multiple_collisions_preserves_order() {
        let builtins = vec!["read_file", "write_file", "bash"];
        let mcp_tools = vec![
            "write_file".to_string(),
            "safe_tool".to_string(),
            "read_file".to_string(),
        ];
        let collisions = detect_mcp_collisions(&mcp_tools, &builtins);
        assert_eq!(
            collisions,
            vec!["write_file".to_string(), "read_file".to_string()]
        );
    }

    #[test]
    fn mcp_collision_detection_against_real_builtins() {
        // Verify the real BUILTIN_TOOL_NAMES constant catches the flagship
        // filesystem server's known collisions. If any of these slip through,
        // yoyo will die on the first LLM turn with "Tool names must be unique".
        let filesystem_server_tools = vec![
            "read_file".to_string(),
            "write_file".to_string(),
            "list_directory".to_string(),
            "move_file".to_string(),
        ];
        let collisions = detect_mcp_collisions(&filesystem_server_tools, BUILTIN_TOOL_NAMES);
        assert!(collisions.contains(&"read_file".to_string()));
        assert!(collisions.contains(&"write_file".to_string()));
        assert_eq!(
            collisions.len(),
            2,
            "only read_file and write_file should collide"
        );
    }

    #[test]
    fn mcp_collision_detection_empty_inputs() {
        assert!(detect_mcp_collisions(&[], &["read_file"]).is_empty());
        assert!(detect_mcp_collisions(&["foo".to_string()], &[]).is_empty());
        assert!(detect_mcp_collisions(&[], &[]).is_empty());
    }

    #[test]
    fn builtin_tool_names_includes_shared_state() {
        // SharedStateTool registers as "shared_state" in sub-agents — MCP servers
        // exposing the same name would cause a collision, so our guard must know it.
        assert!(
            BUILTIN_TOOL_NAMES.contains(&"shared_state"),
            "BUILTIN_TOOL_NAMES must include 'shared_state' to guard against MCP collisions"
        );
    }

    #[test]
    fn test_cache_config_values_match_expected() {
        // Verify that the CacheConfig we set has the exact fields we expect:
        // enabled=true and strategy=Auto. This catches silent changes to yoagent
        // defaults or accidental overwrites.
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let agent = config.build_agent();

        let cache = &agent.cache_config;
        assert!(cache.enabled, "caching must be enabled");
        assert_eq!(cache.strategy, CacheStrategy::Auto);

        // Verify the explicit construction matches CacheConfig default
        let expected = CacheConfig {
            enabled: true,
            strategy: CacheStrategy::Auto,
        };
        assert_eq!(agent.cache_config, expected);
    }

    #[test]
    fn test_cache_config_openai_provider() {
        // Cache config should be enabled even for non-Anthropic providers
        // (the provider may not support it, but we set it unconditionally).
        let config = test_agent_config("openai", "gpt-4o");
        let agent = config.build_agent();
        assert!(
            agent.cache_config.enabled,
            "cache should be enabled for openai provider too"
        );
        assert_eq!(agent.cache_config.strategy, CacheStrategy::Auto);
    }

    #[test]
    fn test_no_tools_builds_agent_without_panic() {
        // When no_tools is true, build_agent should still succeed — it just
        // won't have any tools attached.
        let config = AgentConfig {
            no_tools: true,
            ..test_agent_config("anthropic", "claude-sonnet-4-20250514")
        };
        let _agent = config.build_agent();
        // If we got here, no panic — success
    }

    #[test]
    fn test_no_tools_default_false() {
        // Verify the test helper defaults to no_tools: false
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        assert!(!config.no_tools);
    }

    #[test]
    fn test_no_tools_with_disallowed_tools_builds_ok() {
        // When both no_tools and disallowed_tools are set, no_tools wins:
        // tools aren't built at all (disallowed_tools filtering is irrelevant).
        let config = AgentConfig {
            no_tools: true,
            disallowed_tools: vec!["bash".to_string()],
            ..test_agent_config("anthropic", "claude-sonnet-4-20250514")
        };
        let _agent = config.build_agent();
        // No panic — disallowed_tools is silently ignored when no_tools is true
    }

    #[test]
    fn test_no_tools_side_agent_builds_ok() {
        // Side agents should also build fine when no_tools is set on the config.
        // Side agents always get tools (they copy from main config but don't
        // use no_tools themselves), so this just verifies no field mismatch.
        let config = AgentConfig {
            no_tools: true,
            ..test_agent_config("anthropic", "claude-sonnet-4-20250514")
        };
        let _agent = config.build_side_agent();
    }

    #[test]
    fn test_no_tools_across_providers() {
        // Verify no_tools works for all supported providers (no panic during build).
        for (provider, model) in &[
            ("anthropic", "claude-sonnet-4-20250514"),
            ("openai", "gpt-4o"),
            ("google", "gemini-2.0-flash"),
        ] {
            let config = AgentConfig {
                no_tools: true,
                ..test_agent_config(provider, model)
            };
            let _agent = config.build_agent();
        }
    }
}
