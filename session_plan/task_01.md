Title: Proactive context management to prevent 400 Bad Request from API overflow
Files: src/prompt.rs, src/commands_session.rs, src/format.rs
Issue: #173

## Context

Issue #173 documents that evolution sessions hit 400 Bad Request from the Anthropic API because the agent accumulates tool outputs (file reads, search results, build output) across turns without any context management, eventually exceeding the model's 200K token limit.

**Current state:**
- `auto_compact_if_needed()` exists and runs between REPL turns (in `repl.rs` after each `run_prompt_with_changes`)
- Context overflow detection exists in `run_prompt_with_changes` — when a 400 happens, it auto-compacts and retries once
- `TruncatingTool` already truncates individual tool outputs at 30,000 chars
- `MAX_CONTEXT_TOKENS = 200_000` and `AUTO_COMPACT_THRESHOLD = 0.80` are defined in `cli.rs`

**The gap:** During multi-turn agent execution (one prompt that triggers many tool calls), the agent's internal message list grows with every tool result. By the time the next API call is made, context may already exceed the limit. The auto-compact between REPL turns is too late — the overflow happens *within* a single prompt's agentic loop.

## Implementation

### 1. Add proactive context check in the event loop

In `src/prompt.rs`, inside `handle_prompt_events()`, after each `AgentEvent::ToolExecutionEnd`:
- After the agent processes tool results and before it makes the next API call, check total token count
- If tokens exceed `MAX_CONTEXT_TOKENS * 0.85` (a tighter threshold than the post-turn 0.80), proactively compact
- Log a dimmed message: `"⚡ proactive compact: ~{before} → ~{after} tokens (approaching limit)"`

The tricky part: we need access to the agent's messages within the event loop. Currently `handle_prompt_events` doesn't have `&mut Agent`. We need to restructure slightly:

**Option A (recommended):** Add a token count tracker that estimates context growth from tool results. Each `ToolExecutionEnd` result's text length can be used to estimate tokens (~4 chars/token). When estimated total exceeds threshold, set a flag. The calling `run_prompt_with_changes` function checks this flag between attempts and compacts if needed.

**Option B:** Since `handle_prompt_events` doesn't own the agent, add a `should_compact` field to `PromptResult::Done` that signals the caller to compact before the next prompt.

Go with **Option B** — it's simpler and doesn't require changing the function signature.

### 2. Modify PromptResult/PromptOutcome to carry a compact signal

In `src/prompt.rs`:
- Add a `context_pressure: bool` field to the `Done` variant of `PromptResult`
- In `handle_prompt_events`, track accumulated tool result sizes. After `AgentEnd`, if the total estimated tokens (from usage stats) exceed `MAX_CONTEXT_TOKENS * 0.85`, set `context_pressure = true`
- In `run_prompt_with_changes`, when `PromptResult::Done` has `context_pressure: true`, auto-compact the agent after processing

### 3. Add token estimation from tool results

Add a helper `fn estimate_tool_tokens(result: &ToolResult) -> u64` that counts text characters / 4 (rough token estimate). Track this running total in `handle_prompt_events`.

### 4. Wire proactive compaction into PromptOutcome

Add `context_pressure: bool` to `PromptOutcome` struct. In `run_prompt_with_changes`, after the main loop, if `context_pressure` is true:
- Call `compact_agent(agent)`
- Log the compaction

### 5. Reduce TOOL_OUTPUT_MAX_CHARS for piped mode

In `src/format.rs`, add `pub const TOOL_OUTPUT_MAX_CHARS_PIPED: usize = 15_000;`. In `src/main.rs`, when building tools in piped mode (stdin is not terminal), use the smaller limit. This reduces context growth rate in CI/evolution sessions.

### 6. Tests

- `test_prompt_outcome_context_pressure_field` — verify the struct field exists and defaults
- `test_estimate_tool_tokens_basic` — estimate_tool_tokens returns reasonable values
- `test_estimate_tool_tokens_empty` — empty result returns 0
- `test_tool_output_max_chars_piped` — verify the piped constant is smaller than interactive
- Update existing PromptOutcome tests to include the new field

### 7. Documentation

Update CLAUDE.md architecture section if the context management strategy changes significantly. Add a note about proactive compaction in the codebase comments.
