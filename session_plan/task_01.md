Title: Register SubAgentTool so the model can spawn sub-agents
Files: src/main.rs, src/cli.rs, src/help.rs, src/commands.rs
Issue: #186

## Context

This is the biggest capability gap between yoyo and Claude Code right now. Claude Code can proactively spawn sub-agents for complex subtasks. yoagent 0.7.4 provides `SubAgentTool` (in `yoagent::sub_agent::SubAgentTool`) — a fully implemented tool that runs a fresh `agent_loop()` with its own context window. yoyo just needs to register it.

@yuanhao (the creator) explicitly asked for this twice in the issue comments.

## Implementation

### 1. Create a `build_sub_agent_tool()` function in `src/main.rs`

The sub-agent needs the same provider, model, API key, and base tools as the parent — but WITHOUT the SubAgentTool itself (no recursive spawning, which yoagent enforces by design).

```rust
use yoagent::sub_agent::SubAgentTool;

/// Build a SubAgentTool that inherits the parent agent's provider/model/key
/// but runs in a fresh context window with its own tool set.
fn build_sub_agent_tool(config: &AgentConfig) -> Box<dyn AgentTool> {
    // Build the same tools the parent has (minus sub-agent to prevent recursion)
    let child_tools: Vec<Arc<dyn AgentTool>> = build_tools(
        true, // sub-agents always auto-approve (parent already approved the delegation)
        &config.permissions,
        &config.dir_restrictions,
        TOOL_OUTPUT_MAX_CHARS,
    )
    .into_iter()
    .map(|t| Arc::from(t))
    .collect();

    let provider: Arc<dyn StreamProvider> = if config.provider == "anthropic" {
        Arc::new(AnthropicProvider)
    } else if config.provider == "google" {
        Arc::new(GoogleProvider)
    } else {
        Arc::new(OpenAiCompatProvider)
    };

    let model_config = if config.provider == "anthropic" && config.base_url.is_none() {
        let mut mc = ModelConfig::anthropic(&config.model, &config.model);
        insert_client_headers(&mut mc);
        Some(mc)
    } else {
        Some(create_model_config(&config.provider, &config.model, config.base_url.as_deref()))
    };

    let mut sub = SubAgentTool::new("sub_agent", provider)
        .with_description(
            "Delegate a subtask to a fresh sub-agent with its own context window. \
             Use this for complex, self-contained subtasks like: researching a codebase, \
             running a series of tests, or implementing a well-scoped change. \
             The sub-agent has the same tools as you but starts with a clean context."
        )
        .with_system_prompt(
            "You are a focused sub-agent. Complete the given task efficiently using the tools available. \
             Be concise in your final response — summarize what you did and what you found."
        )
        .with_model(&config.model)
        .with_api_key(&config.api_key)
        .with_tools(child_tools)
        .with_thinking(config.thinking)
        .with_max_turns(25); // Sub-agents get 25 turns max (generous but bounded)

    Box::new(sub)
}
```

**Note:** `StreamProvider` is a trait in yoagent. Check what it's called exactly — might be `StreamProvider` or similar. Check the import:
```rust
use yoagent::provider::StreamProvider;  // or whatever the trait is called
```

Look at how `SubAgentTool::new()` takes `Arc<dyn StreamProvider>` in the yoagent source.

### 2. Register in `configure_agent()`

In the `configure_agent` method, after building the base tools, add the sub-agent tool:

The cleanest approach: modify `configure_agent` to also add the sub-agent tool to the tools list. Since `configure_agent` takes `&self` (the AgentConfig), it has access to all the config needed.

However, `build_tools` returns `Vec<Box<dyn AgentTool>>` and `.with_tools()` takes ownership. So we need to either:
- Add the sub-agent tool to the vec before passing to `.with_tools()`, OR
- Call `.with_tool()` (singular) after `.with_tools()` if yoagent supports it

Check if yoagent's Agent has `.with_tool()` (singular). If not, build the full vec including the sub-agent tool and pass it all at once.

Likely approach — modify `configure_agent`:
```rust
fn configure_agent(&self, mut agent: Agent) -> Agent {
    let mut tools = build_tools(
        self.auto_approve,
        &self.permissions,
        &self.dir_restrictions,
        /* ... */
    );
    // Add sub-agent tool
    tools.push(build_sub_agent_tool(self));
    
    agent = agent
        .with_system_prompt(&self.system_prompt)
        .with_model(&self.model)
        .with_api_key(&self.api_key)
        .with_thinking(self.thinking)
        .with_skills(self.skills.clone())
        .with_tools(tools);
    // ... rest stays the same
```

### 3. Handle the AgentEvent for sub-agent activity

In the main event handling loop (wherever AgentEvent variants are matched in main.rs), the sub-agent's events are forwarded through the parent's event stream. Check if there are new event variants for sub-agent activity. If `AgentEvent::ProgressMessage` comes through, display it with a prefix like `[sub-agent]`.

### 4. Add `--no-sub-agent` CLI flag (optional, only if simple)

Consider adding a flag to disable sub-agent registration for users who don't want it. Low priority — skip if it adds complexity.

### 5. Tests

- `test_build_sub_agent_tool_returns_tool` — verify it returns a tool with name "sub_agent"
- `test_sub_agent_tool_in_build_tools` — verify the configured agent includes the sub-agent tool (check tool count)
- `test_sub_agent_tool_schema` — verify the tool schema has a "task" parameter
- `test_build_sub_agent_tool_inherits_model` — verify the sub-agent tool's properties if accessible

### 6. Update docs

- Add `/spawn` or sub-agent mention to `docs/src/features/context.md` or create a new `docs/src/features/sub-agents.md`
- Update CLAUDE.md architecture section if tool count changes
- Mention in help text that the model can delegate subtasks

## Key risks

- The `SubAgentTool` needs `Arc<dyn StreamProvider>` — make sure the provider types implement this trait
- The sub-agent auto-approves all tools since the parent already authorized the delegation
- Make sure `Arc::from(Box<dyn AgentTool>)` conversion works — may need `Arc<dyn AgentTool>` directly
- If yoagent's `SubAgentTool` has a `with_model_config()` method, use it instead of separate model/api_key/base_url
