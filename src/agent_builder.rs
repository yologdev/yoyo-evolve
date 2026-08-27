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
use yoagent::tools::SharedStateTool;
use yoagent::*;

use crate::cli;
use crate::config;
use crate::format::*;
use crate::hooks;
use crate::prompt::{run_prompt, run_prompt_with_content, PromptOutcome};
use crate::prompt_budget::is_audit_enabled;
use crate::tool_wrappers::{with_session_cap, SESSION_TOOL_CALL_CAP};
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
    "web_search",
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

/// Look up a yoagent 0.9 fleet preset for an Anthropic model name.
///
/// The fleet models (claude-fable-5, claude-opus-5, claude-opus-4-8,
/// claude-sonnet-5, claude-haiku-4-5) ship with authoritative pricing, context-window, and
/// max-output defaults inside yoagent's presets — that's where truth lives,
/// so we start from the preset instead of a hand-rolled config. Dated
/// variants (e.g. "claude-opus-4-8-20260301") match by prefix and keep the
/// requested id so the API receives the exact name the user asked for.
///
/// Returns `None` for non-fleet models — callers fall back to
/// `ModelConfig::anthropic` (yoagent's generic Anthropic defaults).
pub fn anthropic_preset(model: &str) -> Option<ModelConfig> {
    let mut config = if model.starts_with("claude-fable-5") {
        ModelConfig::claude_fable_5()
    } else if model.starts_with("claude-opus-5") {
        ModelConfig::claude_opus_5()
    } else if model.starts_with("claude-opus-4-8") {
        ModelConfig::claude_opus_4_8()
    } else if model.starts_with("claude-sonnet-5") {
        ModelConfig::claude_sonnet_5()
    } else if model.starts_with("claude-haiku-4-5") {
        ModelConfig::claude_haiku_4_5()
    } else {
        return None;
    };
    if config.id != model {
        config.id = model.to_string();
        config.name = model.to_string();
    }
    Some(config)
}

/// Build the ModelConfig for the default Anthropic path: fleet preset when
/// the model name matches one, generic Anthropic config otherwise.
/// Callers still apply `insert_client_headers` afterwards.
pub fn anthropic_model_config(model: &str) -> ModelConfig {
    anthropic_preset(model).unwrap_or_else(|| ModelConfig::anthropic(model, model))
}

/// Normalize a user-supplied `--base-url` for the Anthropic provider.
///
/// yoagent 0.9's `AnthropicProvider` builds the request URL as
/// `{base_url.trim_end_matches('/')}/messages` — it appends only `/messages`,
/// never `/v1/messages`. The official preset therefore carries
/// `https://api.anthropic.com/v1`, and a proxy URL must also end in the path
/// segment the gateway expects (usually `/v1`).
///
/// Accepted forms:
/// - `https://proxy.com/v1` → kept as-is (canonical)
/// - `https://proxy.com/v1/` → trailing slash trimmed
/// - `https://proxy.com` → `/v1` appended (the natural bare-host spelling
///   would otherwise produce `https://proxy.com/messages`, missing `/v1`)
/// - any URL with an explicit path (e.g. a gateway like
///   `https://opencode.ai/zen/v1`) → kept as-is, only trailing `/` trimmed —
///   we never second-guess a deliberate path
pub fn normalize_anthropic_base_url(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    // Everything after "scheme://"; if no scheme, treat the whole string as
    // the host part (e.g. "localhost:8080").
    let after_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    if after_scheme.is_empty() || after_scheme.contains('/') {
        // Has an explicit path (or is degenerate) — respect it verbatim.
        trimmed.to_string()
    } else {
        // Bare host: append the /v1 the Anthropic Messages API expects.
        format!("{trimmed}/v1")
    }
}

/// Create a ModelConfig for a provider, honoring an optional custom base URL.
pub fn create_model_config(provider: &str, model: &str, base_url: Option<&str>) -> ModelConfig {
    let mut config = match provider {
        "anthropic" => {
            // Fleet preset (pricing/context truth) or generic Anthropic config.
            // yoagent 0.9 honors ModelConfig.base_url (0.8 ignored it), so a
            // proxy/gateway URL now works on the native Anthropic protocol —
            // normalized so bare hosts don't lose the /v1 path (#568 item 4).
            let mut config = anthropic_model_config(model);
            if let Some(url) = base_url {
                config.base_url = normalize_anthropic_base_url(url);
            }
            config
        }
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
            // ModelConfig is #[non_exhaustive] as of yoagent 0.9 — build via
            // custom() and mutate. Bedrock-hosted Claude models get a 200K
            // context window; max_tokens follows yoagent's default (16K).
            let mut config = ModelConfig::custom(
                ApiProtocol::BedrockConverseStream,
                "bedrock",
                url,
                model,
                model,
            );
            config.context_window = 200_000;
            config
        }
        "github" => {
            // GitHub Models — OpenAI-compatible API at models.github.ai
            // Uses GITHUB_TOKEN for auth, model names are publisher/model format
            let mut config = ModelConfig::openai(model, model);
            config.provider = "github".into();
            config.base_url = base_url
                .unwrap_or("https://models.github.ai/inference")
                .to_string();
            config.compat = Some(OpenAiCompat::openai());
            config
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

/// Factual grounding so the model doesn't confabulate a vendor identity when
/// asked what it is. A model with no such context answers from training priors
/// — a DeepSeek model running under yoyo confidently claimed to be "Claude,
/// made by Anthropic" (#664), which is exactly the unfounded certainty the
/// system prompt's evidence rules forbid.
///
/// Deliberately identity-free: it states technical facts about the runtime, not
/// a persona, so it composes cleanly with a user's own `--system "You are
/// Jarvis"`. Provider-neutral — it prevents Anthropic-, OpenAI- and
/// yoyo-flavored false claims alike.
fn model_identity_note(provider: &str, model: &str) -> String {
    format!(
        "# Runtime facts\n\n\
         You are being served by the provider `{provider}` under the model id `{model}`. \
         That string pair is the only evidence available about which model you are. \
         You have no direct knowledge of your own architecture, training data, vendor, \
         or origin — anything you might feel certain about there comes from training \
         priors, not from observation, and is exactly the kind of unfounded claim the \
         evidence rules above forbid. If asked what model you are, who made you, or how \
         you were trained, report the provider and model id above and say plainly that \
         you cannot verify anything beyond them. Do not name a vendor or model family \
         that isn't in that pair. The name of the tool running you is a front-end label, \
         not evidence about the model."
    )
}

/// Compose the effective system prompt: whatever prompt was resolved (default,
/// `--system`, `--system-file`, or config) followed by the factual grounding
/// note. Appended, never substituted, so a user prompt keeps its priority
/// position at the top.
pub(crate) fn compose_system_prompt(base: &str, provider: &str, model: &str) -> String {
    let note = model_identity_note(provider, model);
    if base.trim().is_empty() {
        note
    } else {
        format!("{base}\n\n{note}")
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
    pub auto_commit: bool,
    pub permissions: cli::PermissionConfig,
    pub dir_restrictions: cli::DirectoryRestrictions,
    pub context_strategy: cli::ContextStrategy,
    pub context_window: Option<u32>,
    pub shell_hooks: Vec<hooks::ShellHook>,
    pub fallback_provider: Option<String>,
    pub fallback_model: Option<String>,
    pub auto_watch: bool,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub no_tools: bool,
    pub lite: bool,
    /// When set, the agent's bash tool runs every command with this working
    /// directory (used by /spawn worktree isolation). `None` (the default)
    /// keeps the process cwd — interactive/normal-agent behavior unchanged.
    pub bash_cwd: Option<String>,
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

        // Single choke point for every system-prompt source (default, --system,
        // --system-file, config): resolve_system_prompt has already picked one,
        // so appending the grounding note here covers all of them (#664).
        agent = agent
            .with_system_prompt(compose_system_prompt(
                &self.system_prompt,
                &self.provider,
                &self.model,
            ))
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
                self.bash_cwd.clone(),
            );

            // Filter to only allowed tools (--allowed-tools whitelist)
            if !self.allowed_tools.is_empty() {
                tools.retain(|t| self.allowed_tools.contains(&t.name().to_string()));
                eprintln!(
                    "{DIM}  🔒 Allowed tools: {}{RESET}",
                    self.allowed_tools.join(", ")
                );
            }

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

            // Add sub-agent tool (separate from build_tools count and the
            // allowed/disallowed filters above, same as the old with_sub_agent
            // wiring — which just pushed the tool into this same list). Wrapped
            // with a session-wide call cap as a runaway-loop circuit breaker.
            // The parent also gets `shared_state` here (#715): the documented RLM
            // step is store-then-reference, so the parent needs a handle on the same
            // store its sub-agents read. Paired with `sub_agent` deliberately — a
            // store with nobody on the other end is not worth a tool slot.
            let (sub_agent_tool, shared_state) = build_sub_agent_tool(self);
            // Already `Box<dyn AgentTool>` — and possibly a
            // `FallbackSubAgentTool` wrapping the real one, so the session cap
            // must sit OUTSIDE it: one capped slot per delegation, whichever
            // model ends up answering.
            tools.push(with_session_cap(sub_agent_tool, SESSION_TOOL_CALL_CAP));
            tools.push(Box::new(SharedStateTool::new(shared_state)));

            agent = agent.with_tools(tools);
        }

        // Tell yoagent the context window size so its built-in compaction knows the budget.
        // Uses 80% of the effective context window as the compaction threshold.
        agent = agent.with_context_config(ContextConfig {
            max_context_tokens: effective_tokens as usize,
            system_prompt_tokens: 4_000,
            keep_recent: 10,
            keep_first: 2,
            // 200, not 50: as of yoagent 0.15 `truncate_tool_output_on_append`
            // defaults to true, so this cap applies the moment a tool result is
            // appended — every call, every session — not only during compaction
            // as it did in 0.14. At 50 every `cargo build`/`cargo test` result
            // would lose most of its error list immediately. 200 matches
            // yoagent's own default for the same reason.
            tool_output_max_lines: 200,
            ..ContextConfig::default()
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

        if self.provider == "anthropic" {
            // Anthropic path — native protocol; a custom --base-url (proxy or
            // gateway) is honored by yoagent 0.13 and normalized in
            // create_model_config, so it no longer falls through to the
            // OpenAI-compat unknown-provider path.
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            let context_window = model_config.context_window;
            let agent = Agent::from_provider(AnthropicProvider, model_config);
            self.configure_agent(agent, context_window)
        } else if self.provider == "google" {
            // Google uses its own provider
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            let context_window = model_config.context_window;
            let agent = Agent::from_provider(GoogleProvider, model_config);
            self.configure_agent(agent, context_window)
        } else if self.provider == "bedrock" {
            // Bedrock uses AWS SigV4 signing with ConverseStream protocol
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            let context_window = model_config.context_window;
            let agent = Agent::from_provider(BedrockProvider, model_config);
            self.configure_agent(agent, context_window)
        } else {
            // All other providers use OpenAI-compatible API
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            let context_window = model_config.context_window;
            let agent = Agent::from_provider(OpenAiCompatProvider, model_config);
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

        let agent = if self.provider == "anthropic" {
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            Agent::from_provider(AnthropicProvider, model_config)
        } else if self.provider == "google" {
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            Agent::from_provider(GoogleProvider, model_config)
        } else if self.provider == "bedrock" {
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            Agent::from_provider(BedrockProvider, model_config)
        } else {
            let model_config = create_model_config(&self.provider, &self.model, base_url);
            Agent::from_provider(OpenAiCompatProvider, model_config)
        };

        let mut agent = agent
            .with_system_prompt(compose_system_prompt(
                side_prompt,
                &self.provider,
                &self.model,
            ))
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

        let agent = if self.provider == "anthropic" {
            let model_config = create_model_config(&self.provider, architect_model, base_url);
            Agent::from_provider(AnthropicProvider, model_config)
        } else if self.provider == "google" {
            let model_config = create_model_config(&self.provider, architect_model, base_url);
            Agent::from_provider(GoogleProvider, model_config)
        } else if self.provider == "bedrock" {
            let model_config = create_model_config(&self.provider, architect_model, base_url);
            Agent::from_provider(BedrockProvider, model_config)
        } else {
            let model_config = create_model_config(&self.provider, architect_model, base_url);
            Agent::from_provider(OpenAiCompatProvider, model_config)
        };

        // The architect runs on `architect_model`, which is usually NOT
        // `self.model` — so the grounding note must name the model actually
        // serving this agent, not the main one (#671).
        let mut agent = agent
            .with_system_prompt(compose_system_prompt(
                &self.system_prompt,
                &self.provider,
                architect_model,
            ))
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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

        // Validate the fallback API key BEFORE mutating any state. If the
        // fallback provider requires a key and its env var is unset/empty,
        // refuse the switch honestly instead of retrying with the old
        // provider's credential (which would surface as a baffling 401).
        let fallback_key = match cli::provider_api_key_env(&fallback) {
            Some(env_var) => match std::env::var(env_var) {
                Ok(key) if !key.is_empty() => Some(key),
                _ => {
                    eprintln!(
                        "{DIM}  fallback provider {fallback} skipped: ${env_var} not set{RESET}"
                    );
                    return false;
                }
            },
            // Keyless/local provider (e.g. ollama) — no key required.
            None => None,
        };

        self.provider = fallback.clone();
        self.model = self
            .fallback_model
            .clone()
            .unwrap_or_else(|| cli::default_model_for_provider(&fallback));
        if let Some(key) = fallback_key {
            self.api_key = key;
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

    #[test]
    fn test_model_identity_note_states_provider_and_model() {
        let note = model_identity_note("openrouter", "qwen/qwen3-coder");
        assert!(note.contains("openrouter"), "note must name the provider");
        assert!(
            note.contains("qwen/qwen3-coder"),
            "note must name the model id"
        );
    }

    #[test]
    fn test_model_identity_note_is_provider_neutral() {
        // Regression for #664: a DeepSeek model told the user it was
        // "Claude, made by Anthropic". The grounding note must never plant
        // a vendor name that isn't the one actually in use.
        let note = model_identity_note("deepseek", "deepseek-v4-flash");
        assert!(!note.contains("Anthropic"), "note leaked a vendor name");
        assert!(!note.contains("Claude"), "note leaked a model family name");
    }

    #[test]
    fn test_model_identity_note_has_no_persona() {
        // Identity lives in IDENTITY.md / PERSONALITY.md, which don't exist in
        // a normal user's project. The product prompt stays facts-only.
        let note = model_identity_note("anthropic", "claude-sonnet-5");
        assert!(
            !note.to_lowercase().contains("yoyo"),
            "note added a persona"
        );
    }

    #[test]
    fn test_compose_system_prompt_appends_rather_than_replaces() {
        let composed = compose_system_prompt("You are Jarvis.", "groq", "llama-4-70b");
        assert!(
            composed.starts_with("You are Jarvis."),
            "user prompt must keep the top position: {composed}"
        );
        assert!(composed.contains("groq"));
        assert!(composed.contains("llama-4-70b"));
    }

    #[test]
    fn test_compose_system_prompt_with_empty_base_is_just_the_note() {
        let composed = compose_system_prompt("   ", "ollama", "qwen3:8b");
        assert_eq!(composed, model_identity_note("ollama", "qwen3:8b"));
        assert!(!composed.starts_with('\n'), "no leading blank lines");
    }

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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
        }
    }

    #[test]
    fn test_anthropic_preset_claude_opus_5_hits_fleet_arm() {
        // claude-opus-5 must resolve to the yoagent preset (authoritative
        // pricing), not fall through to the generic passthrough path.
        let preset =
            anthropic_preset("claude-opus-5").expect("claude-opus-5 should map to a fleet preset");
        let expected = ModelConfig::claude_opus_5();
        assert_eq!(
            preset.cost.input_per_million,
            expected.cost.input_per_million
        );
        assert_eq!(
            preset.cost.output_per_million,
            expected.cost.output_per_million
        );
        assert_eq!(preset.context_window, expected.context_window);
        assert_eq!(preset.id, "claude-opus-5");
        // A dated variant keeps the requested id but still hits the fleet arm.
        let dated = anthropic_preset("claude-opus-5-20260724")
            .expect("dated claude-opus-5 variant should still map to the preset");
        assert_eq!(dated.id, "claude-opus-5-20260724");
        assert_eq!(
            dated.cost.input_per_million,
            expected.cost.input_per_million
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
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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

    // -----------------------------------------------------------------------
    // normalize_anthropic_base_url tests (#568 checklist item 4)
    //
    // yoagent 0.9's AnthropicProvider builds the request URL as
    // `{base_url.trim_end_matches('/')}/messages` — it appends only
    // `/messages`, never `/v1/messages`. These tests pin both sides of the
    // boundary so proxy users get `.../v1/messages` regardless of whether
    // they typed the /v1 themselves.
    // -----------------------------------------------------------------------

    #[test]
    fn test_normalize_anthropic_base_url_with_trailing_v1_kept_verbatim() {
        // Explicit /v1 path is the canonical form — no double-/v1.
        assert_eq!(
            normalize_anthropic_base_url("https://my-proxy.com/v1"),
            "https://my-proxy.com/v1"
        );
    }

    #[test]
    fn test_normalize_anthropic_base_url_trailing_slash_trimmed() {
        // Trailing slash after /v1 is trimmed (provider would otherwise
        // still work, but keep the config canonical).
        assert_eq!(
            normalize_anthropic_base_url("https://my-proxy.com/v1/"),
            "https://my-proxy.com/v1"
        );
    }

    #[test]
    fn test_normalize_anthropic_base_url_bare_host_gets_v1_appended() {
        // A bare host would produce `https://my-proxy.com/messages`
        // (missing /v1) — append the path the Messages API expects.
        assert_eq!(
            normalize_anthropic_base_url("https://my-proxy.com"),
            "https://my-proxy.com/v1"
        );
    }

    #[test]
    fn test_normalize_anthropic_base_url_bare_host_trailing_slash() {
        assert_eq!(
            normalize_anthropic_base_url("https://my-proxy.com/"),
            "https://my-proxy.com/v1"
        );
    }

    #[test]
    fn test_normalize_anthropic_base_url_explicit_path_respected() {
        // A deliberate non-/v1 gateway path is never second-guessed.
        assert_eq!(
            normalize_anthropic_base_url("https://opencode.ai/zen/v1"),
            "https://opencode.ai/zen/v1"
        );
        assert_eq!(
            normalize_anthropic_base_url("https://gateway.example.com/anthropic"),
            "https://gateway.example.com/anthropic"
        );
    }

    #[test]
    fn test_normalize_anthropic_base_url_schemeless_host() {
        // No scheme: the whole string is the host part; still gets /v1.
        assert_eq!(
            normalize_anthropic_base_url("localhost:8080"),
            "localhost:8080/v1"
        );
    }

    #[test]
    fn test_create_model_config_anthropic_normalizes_base_url() {
        // The anthropic arm applies normalization: bare host gains /v1 ...
        let bare = create_model_config("anthropic", "claude-opus-4-6", Some("https://proxy.com"));
        assert_eq!(bare.base_url, "https://proxy.com/v1");
        // ... and an explicit /v1 is not doubled.
        let with_v1 =
            create_model_config("anthropic", "claude-opus-4-6", Some("https://proxy.com/v1"));
        assert_eq!(with_v1.base_url, "https://proxy.com/v1");
        // Client headers are still applied on the normalized config.
        assert_eq!(
            with_v1.headers.get("User-Agent").unwrap(),
            &yoyo_user_agent()
        );
    }

    #[test]
    fn test_create_model_config_non_anthropic_base_url_unaffected() {
        // Non-anthropic providers take the base_url verbatim — no /v1
        // appended, no trimming beyond what the user typed.
        let openai = create_model_config("openai", "gpt-4o", Some("https://proxy.com"));
        assert_eq!(openai.base_url, "https://proxy.com");
        let openai_slash = create_model_config("openai", "gpt-4o", Some("https://proxy.com/"));
        assert_eq!(openai_slash.base_url, "https://proxy.com/");
        let google = create_model_config("google", "gemini-2.5-pro", Some("https://proxy.com"));
        assert_eq!(google.base_url, "https://proxy.com");
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            ("deepseek", "deepseek-v4-pro"),
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

    fn test_build_agent_anthropic_with_base_url_stays_native() {
        // Since yoagent 0.9, Anthropic with a custom base_url stays on the
        // native Anthropic path (it no longer falls through to OpenAI-compat);
        // the URL is honored and normalized by create_model_config.
        let config = AgentConfig {
            base_url: Some("https://custom-api.example.com/v1".to_string()),
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        // Should not panic — the native Anthropic path handles the base_url
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
        };
        // This should not panic — context config and execution limits are wired
        let agent = config.configure_agent(
            Agent::from_provider(
                yoagent::provider::AnthropicProvider,
                yoagent::provider::ModelConfig::mock(),
            ),
            200_000,
        );
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
        };
        // Should not panic — limits are set with defaults
        let agent = config_no_turns.configure_agent(
            Agent::from_provider(
                yoagent::provider::AnthropicProvider,
                yoagent::provider::ModelConfig::mock(),
            ),
            200_000,
        );
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
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
        };
        let agent = config_with_turns.configure_agent(
            Agent::from_provider(
                yoagent::provider::AnthropicProvider,
                yoagent::provider::ModelConfig::mock(),
            ),
            200_000,
        );
        let _ = agent;
    }

    #[test]
    #[serial]
    fn test_fallback_switch_success() {
        // When fallback is configured, different from current, and its key is
        // available, switch should succeed
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::set_var("GOOGLE_API_KEY", "test-google-key");
        }
        let mut config = AgentConfig {
            fallback_provider: Some("google".to_string()),
            fallback_model: Some("gemini-2.0-flash".to_string()),
            auto_watch: true,
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.provider, "google");
        assert_eq!(config.model, "gemini-2.0-flash");
        assert_eq!(config.api_key, "test-google-key");
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::remove_var("GOOGLE_API_KEY");
        }
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
    #[serial]
    fn test_fallback_switch_derives_default_model() {
        // When fallback_model is None, should derive the default model for the provider
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::set_var("OPENAI_API_KEY", "test-openai-key");
        }
        let mut config = AgentConfig {
            fallback_provider: Some("openai".to_string()),
            fallback_model: None,
            auto_watch: true,
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, cli::default_model_for_provider("openai"));
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
        }
    }

    #[test]
    #[serial]
    fn test_fallback_switch_uses_explicit_model() {
        // When fallback_model is Some, should use it instead of the default
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::set_var("OPENAI_API_KEY", "test-openai-key");
        }
        let mut config = AgentConfig {
            fallback_provider: Some("openai".to_string()),
            fallback_model: Some("gpt-4-turbo".to_string()),
            auto_watch: true,
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "gpt-4-turbo");
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
        }
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
    #[serial]
    fn test_fallback_switch_refuses_when_key_env_missing() {
        // If the fallback provider requires a key and its env var isn't set,
        // the switch must be refused WITHOUT mutating any state — otherwise
        // the retry hits the fallback with the OLD provider's credential and
        // the user gets a baffling 401 instead of an honest "key not set".
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
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
        assert!(!config.try_switch_to_fallback());
        // No-mutation invariant: provider, model, and api_key all unchanged.
        assert_eq!(config.provider, "anthropic");
        assert_eq!(config.model, "claude-opus-4-6");
        assert_eq!(config.api_key, original_key);
    }

    #[test]
    #[serial]
    fn test_fallback_switch_refuses_when_key_env_empty() {
        // An empty env var is as unusable as a missing one — refuse without mutating.
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::set_var("XAI_API_KEY", "");
        }
        let mut config = AgentConfig {
            fallback_provider: Some("xai".to_string()),
            fallback_model: Some("grok-3".to_string()),
            auto_watch: true,
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        let original_key = config.api_key.clone();
        assert!(!config.try_switch_to_fallback());
        assert_eq!(config.provider, "anthropic");
        assert_eq!(config.model, "claude-opus-4-6");
        assert_eq!(config.api_key, original_key);
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::remove_var("XAI_API_KEY");
        }
    }

    #[test]
    fn test_fallback_switch_keyless_provider_needs_no_key() {
        // Providers with no API key env var (local, e.g. ollama) must still
        // switch fine — don't require a key that isn't required.
        assert!(cli::provider_api_key_env("ollama").is_none());
        let mut config = AgentConfig {
            fallback_provider: Some("ollama".to_string()),
            fallback_model: Some("llama3".to_string()),
            auto_watch: true,
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };
        let original_key = config.api_key.clone();
        assert!(config.try_switch_to_fallback());
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.model, "llama3");
        // Keyless switch keeps the existing api_key untouched.
        assert_eq!(config.api_key, original_key);
    }

    #[test]
    #[serial]
    fn test_fallback_switch_idempotent() {
        // Calling try_switch_to_fallback twice: first call switches, second returns false
        // (because provider now matches fallback)
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::set_var("GOOGLE_API_KEY", "test-google-key");
        }
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
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::remove_var("GOOGLE_API_KEY");
        }
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
            text_since_last_tool: String::new(),
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
            text_since_last_tool: String::new(),
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
    #[serial]
    fn test_fallback_prompt_api_error_with_fallback_switches() {
        // When API error occurs and fallback is configured, the config should switch
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::set_var("GOOGLE_API_KEY", "test-google-key");
        }
        let mut config = AgentConfig {
            fallback_provider: Some("google".to_string()),
            fallback_model: Some("gemini-2.0-flash".to_string()),
            auto_watch: true,
            ..test_agent_config("anthropic", "claude-opus-4-6")
        };

        let response = PromptOutcome {
            text: String::new(),
            text_since_last_tool: String::new(),
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
        // SAFETY: Test runs serially (#[serial]), no concurrent env var access.
        unsafe {
            std::env::remove_var("GOOGLE_API_KEY");
        }
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

    #[test]
    fn test_allowed_tools_filters_to_whitelist() {
        // Test the allowed_tools filtering logic: build the full tool list,
        // then apply the same retain() filter that build_agent uses. Verify
        // that only whitelisted tools survive.
        use crate::tools::build_tools;

        let mut tools = build_tools(
            true,
            &cli::PermissionConfig::default(),
            &cli::DirectoryRestrictions::default(),
            8000,
            false,
            vec![],
            None,
        );

        let all_names: Vec<String> = tools.iter().map(|t| t.name().to_string()).collect();
        // Sanity: the full tool list should have bash, write_file, etc.
        assert!(
            all_names.contains(&"bash".to_string()),
            "full tool list should contain bash: {all_names:?}"
        );
        assert!(
            all_names.contains(&"write_file".to_string()),
            "full tool list should contain write_file: {all_names:?}"
        );

        // Apply the same allowed_tools filter as build_agent
        let allowed = ["read_file".to_string(), "search".to_string()];
        tools.retain(|t| allowed.contains(&t.name().to_string()));

        let filtered_names: Vec<String> = tools.iter().map(|t| t.name().to_string()).collect();

        // Whitelisted tools must be present
        assert!(
            filtered_names.contains(&"read_file".to_string()),
            "read_file should survive allowed_tools filter: {filtered_names:?}"
        );
        assert!(
            filtered_names.contains(&"search".to_string()),
            "search should survive allowed_tools filter: {filtered_names:?}"
        );

        // Non-whitelisted tools must be absent
        assert!(
            !filtered_names.contains(&"bash".to_string()),
            "bash should NOT survive allowed_tools filter: {filtered_names:?}"
        );
        assert!(
            !filtered_names.contains(&"write_file".to_string()),
            "write_file should NOT survive allowed_tools filter: {filtered_names:?}"
        );
        assert!(
            !filtered_names.contains(&"edit_file".to_string()),
            "edit_file should NOT survive allowed_tools filter: {filtered_names:?}"
        );

        // Only the 2 whitelisted tools should remain
        assert_eq!(
            filtered_names.len(),
            2,
            "exactly 2 tools should survive: {filtered_names:?}"
        );
    }
    /// Drift guard (blind round 83, Day 180): every tool name `tools::build_tools`
    /// actually registers must appear in `BUILTIN_TOOL_NAMES`, because that const is
    /// the *only* input to `detect_mcp_collisions` — a registered builtin missing from
    /// it means an MCP server exposing that same name is waved through, and the
    /// Anthropic API then rejects the first turn with "Tool names must be unique",
    /// which is the exact failure the guard exists to prevent.
    ///
    /// `BUILTIN_TOOL_NAMES` is a hand-maintained second copy of an enumeration whose
    /// authority lives in another module — the same shape as `ROUTED_SUBCOMMANDS` and
    /// `GLOBAL_SETTERS`, both of which needed a second test to stay tied to reality.
    /// Before this test the only checks were two single-name spot assertions
    /// (`web_search` in `src/tools.rs`, `shared_state` in `tests/integration.rs`), so
    /// a *newly added* builtin could drift out silently. Measured when this landed:
    /// the two lists agreed, so this guards the future rather than repairing a hole.
    ///
    /// Deliberately a SUPERSET check, not an equality check: `BUILTIN_TOOL_NAMES` is
    /// correctly wider than `build_tools`, because `ask_user`, `sub_agent` and
    /// `shared_state` are pushed elsewhere (conditionally, in `agent_builder`) and
    /// still must be guarded against collision. Asserting equality would fail on
    /// exactly the names the guard most needs to keep.
    #[test]
    fn every_registered_builtin_tool_is_named_in_builtin_tool_names() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = crate::tools::build_tools(true, &perms, &dirs, 10_000, false, vec![], None);
        let registered: Vec<&str> = tools.iter().map(|t| t.name()).collect();

        // Anti-vacuous: a scan that finds nothing must fail loudly rather than pass
        // on an empty list — a guard that can never fire is the quieter defect.
        assert!(
            !registered.is_empty(),
            "build_tools registered no tools at all — this drift guard would be vacuous"
        );

        for name in &registered {
            assert!(
                BUILTIN_TOOL_NAMES.contains(name),
                "tools::build_tools registers '{name}' but BUILTIN_TOOL_NAMES does not \
                 list it, so detect_mcp_collisions cannot catch an MCP server exposing \
                 that name. Fix: add \"{name}\" to BUILTIN_TOOL_NAMES in src/agent_builder.rs. \
                 Registered: {registered:?}"
            );
        }
    }

    /// Near-miss guard for the drift check above: the three builtins that are pushed
    /// OUTSIDE `build_tools` must stay in `BUILTIN_TOOL_NAMES`. A future "cleanup" that
    /// derived the const from `build_tools` alone would drop exactly these and silently
    /// un-guard them — `shared_state` and `sub_agent` are the RLM pair (#715) and
    /// `ask_user` is interactive-only, so none of them is reachable from that call.
    #[test]
    fn builtin_tool_names_keeps_the_builtins_build_tools_does_not_register() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = crate::tools::build_tools(true, &perms, &dirs, 10_000, false, vec![], None);
        let registered: Vec<&str> = tools.iter().map(|t| t.name()).collect();

        for name in ["ask_user", "sub_agent", "shared_state"] {
            assert!(
                BUILTIN_TOOL_NAMES.contains(&name),
                "'{name}' must stay in BUILTIN_TOOL_NAMES even though build_tools does \
                 not register it — it is pushed elsewhere and still collides"
            );
            assert!(
                !registered.contains(&name),
                "'{name}' is now registered by build_tools — the superset rationale in \
                 every_registered_builtin_tool_is_named_in_builtin_tool_names needs updating"
            );
        }
    }
}
