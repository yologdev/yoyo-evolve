Title: Complete --no-tools flag — suppress sub_agent, shared_state, and MCP connections
Files: src/agent_builder.rs, src/main.rs
Issue: none

## Problem

The `--no-tools` flag was added in Day 77. It works by adding all BUILTIN_TOOL_NAMES to `disallowed_tools`, which filters them out during `build_agent()`. However, two gaps remain:

1. **sub_agent and shared_state tools bypass the filter**: In `build_agent()` (agent_builder.rs ~line 460), the sub_agent tool is added via `agent.with_sub_agent(sub_agent_tool)` AFTER the `disallowed_tools` filtering. So even with `--no-tools`, the agent still has `sub_agent` and `shared_state` available.

2. **MCP/OpenAPI connections still happen**: In `main.rs` (~line 655-660), `connect_external_servers()` is called regardless of `--no-tools`. This means external tool servers are connected even in chat-only mode, which adds latency and potentially more tools.

## Solution

### In `agent_builder.rs`:
- Add a `no_tools: bool` field to `AgentConfig`
- In `build_agent()`, when `no_tools` is true, skip the `build_sub_agent_tool()` call and `agent.with_sub_agent()` line
- Also skip adding any tools at all (don't call `build_tools()` or `with_tools()`) — cleaner than adding then filtering

### In `main.rs`:
- Pass `config.no_tools` through to `AgentConfig`
- When `no_tools` is true, skip the `connect_external_servers()` call entirely
- The existing stderr message ("All tools disabled") is already there — keep it

### Tests to add:
- Test that `AgentConfig` with `no_tools: true` doesn't panic during build
- Test that `no_tools` field defaults to false in existing test configs
- Test that when `no_tools` is true, `disallowed_tools` is irrelevant (tools aren't built at all)

### Edge cases:
- When `no_tools` is true AND `--disallowed-tools` is also specified, `no_tools` wins (all tools disabled)
- Skills are still loaded (they're prompt context, not tools) — this is correct
- The system prompt still works normally
