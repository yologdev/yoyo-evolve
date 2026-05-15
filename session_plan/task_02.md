Title: Add context breakdown to /tokens — show what's filling your context window
Files: src/commands_info.rs, src/format/cost.rs
Issue: none

## What

Enhance `/tokens` to show a breakdown of what's consuming context tokens — not just the total, but a per-category breakdown: system prompt, project context, user messages, assistant responses, tool calls, tool results. This helps developers understand *why* their context is filling up and make better decisions about when to `/compact` or `/clear`.

This is a competitive gap — Claude Code shows context usage details, and understanding context consumption is a daily pain point for any coding agent user.

## Current State

`/tokens` currently shows:
- Message count
- Current context tokens / max tokens
- A progress bar
- Session totals (input, output, cache)
- Cost estimate

It does NOT show what's consuming the tokens — a user can see "I'm at 80% context" but not "50% is tool results from my last big search."

## Implementation

### In `commands_info.rs`:

1. **Add `context_breakdown(messages: &[AgentMessage]) -> ContextBreakdown`** function:
   ```
   struct ContextBreakdown {
       system_estimate: usize,  // from cli_config::SYSTEM_PROMPT + project context estimate
       user_messages: usize,    // sum of User message tokens
       assistant_messages: usize, // sum of Assistant message tokens  
       tool_calls: usize,       // sum of ToolCall content tokens
       tool_results: usize,     // sum of ToolResult message tokens
       thinking: usize,         // sum of Thinking content tokens (if any)
       total: usize,
   }
   ```
   Use `yoagent::context::message_tokens` and `yoagent::context::estimate_tokens` for counting.
   Walk through messages, categorize each by type.

2. **Modify `handle_tokens`** to call `context_breakdown` and display the results:
   ```
   Context breakdown:
     system prompt:    ~4,200 tokens  (12%)
     user messages:     8,100 tokens  (24%)
     assistant:        12,300 tokens  (36%)
     tool calls:        2,400 tokens   (7%)
     tool results:      7,000 tokens  (21%)
     ──────────────────────────────
     total:            34,000 / 128,000 tokens
   ```
   Show percentages. Highlight the largest category in bold. If tool results are >50%, suggest "tool results are dominating context — consider /compact."

### In `format/cost.rs`:

3. **Add `format_context_breakdown(breakdown: &ContextBreakdown) -> String`** helper that formats the breakdown table with colors and percentages. This keeps the formatting logic in the format module where it belongs.

### Tests to add:
- `test_context_breakdown_empty` — empty messages returns all zeros
- `test_context_breakdown_user_only` — only user messages
- `test_context_breakdown_mixed` — mix of user, assistant, tool results
- `test_context_breakdown_percentages` — verify percentage calculation
- `test_format_context_breakdown_output` — formatted string contains expected sections

### What NOT to do:
- Don't try to precisely count system prompt tokens — estimate from the constant
- Don't add a separate `/context` command — extend `/tokens` which already exists
- Don't change the existing `/tokens` output — add the breakdown as a new section
