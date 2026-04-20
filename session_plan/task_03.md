Title: Add /profile command showing session statistics
Files: src/commands_info.rs, src/cli.rs, src/help.rs
Issue: none

## Problem

Claude Code shows session statistics (tokens used, cost, time, turns) in its UI. yoyo has
`/tokens` and `/cost` as separate commands, but no unified session profile view that shows
everything at a glance. A developer mid-session wants to quickly see: how long have I been
working, how many turns have happened, what's my total cost, am I running low on context.

## Implementation

### 1. Add `handle_profile` function in `src/commands_info.rs`

Create a new `/profile` command that shows a unified session summary:

```
  ╭─ Session Profile ─────────────────────────────╮
  │ Model:     claude-sonnet-4-20250514          │
  │ Provider:  anthropic                           │
  │ Duration:  12m 34s                             │
  │ Turns:     8                                   │
  │ Tokens:    45,231 in / 12,890 out              │
  │ Cost:      ~$0.42                              │
  │ Context:   58,121 / 200,000 (29%)              │
  ╰────────────────────────────────────────────────╯
```

The function signature: `pub fn handle_profile(agent: &Agent, model: &str, provider: &str, session_start: Instant)`

It should:
- Use `agent.messages()` to count turns (assistant messages) 
- Use existing `estimate_cost` / `format_cost` from `format/cost.rs` for cost
- Use existing `total_tokens` helper for token counts
- Calculate duration from session start time
- Show context usage as tokens / max with percentage
- Use box-drawing characters for a clean bordered display

### 2. Wire it into the REPL dispatch in `src/cli.rs`

Add `/profile` to the command dispatch (similar to how `/tokens`, `/cost`, `/status` are 
handled). It needs access to the agent, model name, provider name, and session start time.

Look at how `/status` is dispatched — it already has access to similar state. Follow the 
same pattern.

### 3. Add to help text in `src/help.rs`

Add `/profile` to the Information category in `help_text()` and `command_help()`:
- Short description: "Show session statistics (tokens, cost, time, turns)"
- Help entry with usage example

Also add "profile" to `KNOWN_COMMANDS` in `src/commands.rs` (this is a 4th file but it's
just adding one string to an array — if strict 3-file limit, skip this and let it work 
without autocomplete).

## Verification

- `cargo build` — compiles cleanly
- `cargo test` — all tests pass
- `cargo clippy --all-targets -- -D warnings` — no warnings
- The command should work from the REPL: typing `/profile` shows the statistics box
