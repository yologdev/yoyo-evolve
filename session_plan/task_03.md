Title: Register yoagent's SubAgentTool as a model-invocable tool
Files: src/main.rs, src/cli.rs, src/help.rs
Issue: #186

## Context

yoagent 0.7.2 exports `SubAgentTool` and `Agent::with_sub_agent()`. yoyo already has a `/spawn` command (in commands_session.rs) that manually creates a sub-agent, but the **model** can't spawn sub-agents on its own. This is one of Claude Code's biggest architectural advantages: the model can delegate sub-tasks to child agents with fresh context windows.

The existing `/spawn` command proves the concept works. This task makes it available as a tool the AI can invoke during agentic runs, closing the #186 gap.

## Implementation

### 1. Create and register SubAgentTool in `build_tools()` or `configure_agent()` (src/main.rs)

yoagent provides two approaches:
- `SubAgentTool::new(provider_name)` — creates a tool the model can call
- `agent.with_sub_agent(sub_agent_tool)` — registers it on the agent

Use the existing agent config to build the SubAgentTool:

```rust
use yoagent::SubAgentTool;

// In configure_agent() or after agent is built:
let sub_agent = SubAgentTool::new(&agent_config.provider)
    .with_model(&agent_config.model)
    .with_api_key(&agent_config.api_key)
    .with_system_prompt("You are a sub-agent spawned to handle a specific task. Complete the task thoroughly and report your results.")
    .with_max_turns(25)
    .with_tools(/* need to pass Arc<dyn AgentTool> versions of the base tools */);

agent = agent.with_sub_agent(sub_agent);
```

**Key challenge:** `SubAgentTool::with_tools()` needs `Vec<Arc<dyn AgentTool>>`, but `build_tools()` returns `Vec<Box<dyn AgentTool>>`. Two options:
- Option A: Build a second set of base tools (just the defaults from yoagent — bash, read_file, write_file, edit_file, search, list_files) wrapped in Arc for the sub-agent. The sub-agent doesn't need GuardedTool wrappers since it runs in a sandboxed context.
- Option B: Use `yoagent::tools::default_tools()` directly to get the base tools, then wrap in Arc.

Go with Option B — it's simpler and the sub-agent gets the standard tool set.

```rust
use yoagent::tools::default_tools;

let sub_tools: Vec<Arc<dyn AgentTool>> = default_tools()
    .into_iter()
    .map(|t| Arc::from(t) as Arc<dyn AgentTool>)
    .collect();

let sub_agent = SubAgentTool::new(&agent_config.provider)
    .with_model(&agent_config.model)
    .with_api_key(&agent_config.api_key)
    .with_system_prompt("You are a sub-agent. Complete your assigned task and report results concisely.")
    .with_max_turns(25)
    .with_tools(sub_tools);
```

### 2. Add --no-sub-agent flag (src/cli.rs)

Add a `--no-sub-agent` flag to disable the sub-agent tool (for users who don't want the model spawning sub-processes):
```rust
.arg(Arg::new("no-sub-agent")
    .long("no-sub-agent")
    .help("Disable the sub-agent tool (prevents model from spawning child agents)")
    .action(ArgAction::SetTrue))
```

Wire it into `AgentConfig` and only register the sub-agent when the flag is not set.

### 3. Update help (src/help.rs)

Add a note about the sub-agent capability in the general help output. No new slash command needed — this is a tool the model uses, not the user.

### 4. Tests

- `test_sub_agent_tool_registered` — verify the sub-agent tool appears in the agent's tool list when --no-sub-agent is not set
- `test_sub_agent_disabled_with_flag` — verify it's absent when --no-sub-agent is set
- `test_sub_agent_tool_name` — verify the tool has the expected name

### 5. Important notes

- The sub-agent tool does NOT need permission confirmation — it's the model talking to itself
- The sub-agent should NOT recursively create its own sub-agents (don't register SubAgentTool on the sub-agent)
- If the `Arc::from(Box<dyn AgentTool>)` conversion doesn't compile directly, use `Arc::from(t)` or create wrapper types
- Check if yoagent's `default_tools()` returns `Vec<Box<dyn AgentTool>>` — if it does, convert: `let arc_tool: Arc<dyn AgentTool> = Arc::from(t);` should work since `Box<T>` can convert to `Arc<T>` for sized types, but for `dyn` traits you may need `Into`

Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings` to verify.
