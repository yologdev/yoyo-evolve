Title: Wire skills into sub-agents via SubAgentTool::with_skills
Files: src/tools.rs
Issue: #518

## What to do

In `build_sub_agent_tool()` (src/tools.rs ~line 1080), add `.with_skills(config.skills.clone())`
to the `SubAgentTool` builder chain. The chain currently ends with `.with_shared_state(shared_state.clone())`.
Add `.with_skills(config.skills.clone())` before or after that call.

yoagent 0.8.4 is already locked in Cargo.lock. `AgentConfig` already has `pub skills: SkillSet` at line 394
of agent_builder.rs. The `build_agent` method already calls `.with_skills(self.skills.clone())` at line 436.
This is a one-line fix.

## Why it matters

Sub-agents dispatched via `sub_agent` tool (e.g., research sub-agents during evolve sessions) currently
don't receive the SkillSet. This means skills like `research` ("use the web_search tool; never answer
from memory") never reach them, so they answer from training data instead of following the skill.
This completes the Exa migration from the previous session.

## Add a test

Add a test `test_build_sub_agent_tool_inherits_skills` that:
1. Creates an `AgentConfig` with a non-empty skills field (use `SkillSet::empty()` is fine as baseline;
   the key is verifying the builder chain compiles and doesn't panic)
2. Calls `build_sub_agent_tool(&config)`
3. Asserts the tool was created successfully (the test validates the builder chain compiles with `.with_skills()`)

Note: We can't easily inspect the SubAgentTool's internal skills field, so the test mainly verifies
the chain compiles and runs without panic. The real verification is that `cargo build` succeeds with
the new `.with_skills()` call.

## Verify

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```

After landing, close issue #518 with:
```
gh issue close 518 --repo yologdev/yoyo-evolve -c "Landed — sub-agents now inherit the SkillSet via \`.with_skills(config.skills.clone())\`. The research skill (and all others) will now reach dispatched sub-agents. 🐙"
```
