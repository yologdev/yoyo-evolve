Title: Add /teach mode toggle for explanatory coding
Files: src/repl.rs, src/commands.rs, src/help.rs
Issue: #238

## What

Add a `/teach` toggle command that enables "teach mode" — when active, yoyo prepends a
teaching-focused instruction to its system prompt so it explains its reasoning, uses readable
code patterns, and helps the user learn as it works. This extracts the useful kernel from
Issue #238's ambitious proposal (TUI settings, RAG, GraphRAG) into something small and shippable.

## Why

Issue #238 asks for teach mode and learning features. The full proposal (TUI settings, memory
tiers, RAG) is way too big for one task. But the core idea — a mode where yoyo explains what
it's doing and why — is simple and valuable. Vibe-coding is real; people want to learn while
the agent works. A system prompt modifier that says "explain your reasoning step-by-step,
prefer readable code, add comments explaining non-obvious choices" is achievable and useful.

## Implementation

### In `src/commands.rs`:
1. Add a thread-local or static atomic bool: `TEACH_MODE` (similar to how `BELL_ENABLED` works in format/mod.rs)
2. Add `set_teach_mode(enabled: bool)`, `is_teach_mode() -> bool` functions
3. Add `TEACH_MODE_PROMPT` constant with the teaching instruction text:
   ```
   TEACHING MODE: You are in teach mode. For every change you make:
   1. Explain WHY you're making the change before showing the code
   2. Use clear, readable code patterns — prefer clarity over cleverness
   3. Add brief comments on non-obvious lines
   4. After completing a task, summarize what the user should learn from it
   Keep explanations concise but educational.
   ```
4. Add `handle_teach(input: &str)` function that toggles the mode, prints status
5. Add "teach" to KNOWN_COMMANDS

### In `src/repl.rs`:
1. In the REPL command dispatch, add `/teach` routing to `handle_teach()`
2. When constructing prompts for the agent, check `is_teach_mode()` — if true,
   prepend `TEACH_MODE_PROMPT` to the user's message (not system prompt, since
   we can't modify system prompt mid-session — prepend to user message instead)

### In `src/help.rs`:
1. Add `/teach` to the help text under an appropriate category
2. Description: "Toggle teach mode — yoyo explains its reasoning as it works"
3. Add to `command_short_description()` and `help_command_completions()`

### Tests to add:
- `test_teach_mode_default_off` — verify teach mode starts disabled
- `test_teach_mode_toggle` — verify set/get cycle works
- `test_teach_known_command` — verify "teach" is in KNOWN_COMMANDS
- Verify help text includes /teach

### What NOT to build:
- No TUI settings panel
- No /learn command (defer to future)
- No memory tiers or RAG
- No persistent teach mode across sessions (it's a session toggle)
