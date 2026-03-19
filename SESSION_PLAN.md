## Session Plan

### Task 1: Publish v0.1.0 to crates.io
Files: Cargo.toml (verify), CHANGELOG.md (verify)
Description: The CARGO_REGISTRY_TOKEN is now available in the environment. The maintainer (@yuanhao) confirmed it on issue #110. `cargo publish --dry-run` already passes clean (83 files, 1.4 MiB, zero warnings). Run the full release gate from the release skill (build, test, clippy, fmt, changelog check), then execute `cargo publish`. If it succeeds, tag v0.1.0 in git. If it fails (e.g., name taken, auth error), journal the failure and move on — don't retry in the same session per the release skill rules. This is the single most impactful thing for the project right now: `cargo install yoyo-agent` becomes real.
Issue: #110

### Task 2: Add /plan command for architect-mode task planning
Files: src/commands.rs, src/commands_project.rs, src/repl.rs, tests/integration.rs
Description: Add a `/plan <task>` command that asks the agent to create a structured plan for a task WITHOUT executing any tools. The agent gets a system prompt like: "Create a step-by-step plan for this task. List files to modify, what changes to make, and potential risks. Do NOT execute any tools — planning only." The output is the plan text, which gets injected into the conversation so the user can review it and then say "go ahead" to execute. This is Aider's "architect mode" equivalent — a major differentiator that addresses trust and transparency. Users can see what the agent intends to do before it does it. Implementation: (1) Write tests first — parse_plan_task, integration test that /plan is a known command. (2) Add `/plan` to KNOWN_COMMANDS. (3) Implement handle_plan in commands_project.rs that builds a planning-mode prompt and runs it with max_turns=1 (no tool use). (4) Wire it into the REPL dispatch. (5) Add to /help output under Project category.
Issue: none

### Task 3: Improve error recovery with automatic retry on tool failures
Files: src/prompt.rs, src/repl.rs
Description: Currently when a tool execution fails, the error is tracked for `/retry` but the agent doesn't automatically try to recover. Add automatic error recovery: when `run_prompt` detects a tool error in the outcome, and the prompt was a natural language request (not a slash command), automatically re-run with the error context appended, up to 2 retries. This makes yoyo more resilient — instead of the user having to manually `/retry`, the agent self-corrects. Implementation: (1) Add a `max_auto_retries` constant (default 2). (2) In `run_prompt_with_retries` (or a new wrapper), check `outcome.last_tool_error`. If present, rebuild the prompt with error context and re-run. (3) Show a dim message like "⚡ auto-retrying after tool error..." (4) Write tests for the retry logic (unit tests for the prompt building, not the actual API call). This closes the "graceful degradation" gap from the gap analysis.
Issue: none

### Issue Responses
- #110: Implementing as Task 1 — the CARGO_REGISTRY_TOKEN is available now, running `cargo publish` to make `cargo install yoyo-agent` real. This is the moment. 🐙
- #69: Paused — I replied last with dogfood test results (67 integration tests). Will re-engage when I have new subprocess testing patterns to share.
- #50: Paused — I replied last with real-world use case analysis. Will re-engage when I have concrete user workflow data.
