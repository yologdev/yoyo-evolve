# Assessment — Day 211 (21:13)

(Written under a context ceiling — research step skipped; everything below was observed this session.)

## Build Status
Pass — the harness verified it at session start. HEAD is f31ce545 (creator commits e08025e0 + f31ce545: `.yoyo.toml` is now the only place the model is set; no MODEL secret; harness_logic.sh fails on a hardcoded model id). Loop now runs `provider = anthropic`, `model = claude-opus-5-5`, `max_tokens = 128000`, `thinking = high`.

## Recent Changes (last 3 sessions)
- 19:30: #742 fixed (/retry uses `last_tool_name` from the prompt loop, not a string scan). Issue #742 is **still open** and should be closed with a pointer to 8d1f0ff3. daily_diary.sh now reports per-run spend from the audit log (#944 slice).
- 16:28: dispatch tests no longer run the real setup wizard or init (they had overwritten `.yoyo.toml`). The per-edit auto-check now skips cargo when the edit cannot affect it.
- 00:17 / 09:54 / 15:06: three sessions with no commits. The creator traced them to the model/provider mismatch (a DeepSeek id sent to Anthropic, then max_tokens 131072 > 128000 → 400 on every call). The config fix landed.

## Source Architecture
About 116k lines under src/. Key entry points: main.rs, cli.rs, agent_builder.rs (build_agent/configure_agent, ~3600 lines), prompt.rs, repl.rs, dispatch.rs, tool_wrappers.rs (~5600). Scripts: evolve.sh (protected), extract_trajectory.py, check_assertion_weakening.py, counterfactual_green.py.

## Self-Test Results
Reproduced the reported repro for **#964**: `echo hi | ANTHROPIC_API_KEY=dummy ./target/debug/yoyo --model claude-opus-5-5 --max-tokens 131072` printed the banner, the auto-watch line and the 401, and **no max_tokens ceiling warning**. The warning code is at agent_builder.rs:1397-1413 (`if let Some(max) = self.max_tokens` → `max_tokens_ceiling_warning(max, model_output_ceiling, …)`, gated on `!is_quiet()`). All four provider arms of `build_agent` call `configure_agent`, so the cause is probably one of two things: `self.max_tokens` is None at that point (the CLI flag may not reach AgentConfig.max_tokens), or `output_ceiling` for opus-5-5 is ≥131072 or falls back to something large. Not yet determined, so the task should start by probing this.

## Evolution History (last 5 runs)
36350794354 is in progress (this run). 36344537031 success (19:30). 36333281296 success (16:28). 36328548109 **cancelled** (15:10; max_tokens 400s). 36327816764 and 36310458925 "success" but committed nothing (the green-but-empty failure mode). Rows three to five of the trajectory read "did not reach a verdict", and the provider mismatch explains them.

## Capability Gaps
Unchanged from recent assessments. The biggest product-facing gap is config honesty: a user who sets max_tokens above their model's limit gets a bare 400 (#964). Other gaps: no composite safe mode (#879), /cd doesn't reload project config (#869), TUI (#215).

## Bugs / Friction Found
1. **#964 (creator-filed, product):** the ceiling warning is silent on the Anthropic path, and it compares against the preset default (64K) instead of the model's maximum (128K). If only the silence is fixed, the warning would fire on this repo's correct 128000, which is cry-wolf. Both halves have to land together. Tests belong at the emission point: 131072 → warning naming 128000; 128000 → none. If the model's maximum is unknown, gate the warning off.
2. **#963 (creator-filed, evolve):** flaky `test_compact_thrash_detection_*` (src/commands_session.rs:975/989) race on the global `COMPACT_THRASH_COUNT` (line 23). This is a gate hazard: it can revert correct tasks. The fix is a `_with(&AtomicU32)` seam or one shared lock. Positive control: 20/20 loop passes. Small, well-specified, high leverage.
3. #742 is fixed but still open (housekeeping).

## Open Issues Summary
Top priority is the creator's agent-input issues: **#964** and **#963**, both filed tonight with precise specs. Then: #958 (unverified unhittable denominator), #944 (social.sh spend is still unmeasured), #937 price drift, #902/#879/#869 trust/safe-mode, #858 skill-evolve gate defects, #916 impl-loop API-error abort blind to plain output (this is directly relevant to today's three empty sessions: green runs with no verdict).

## Research Findings
Skipped this session (context ceiling). Recommendation to planner: Task 1 = #963 (small, removes a gate hazard). Task 2 = #964 (product; probe first for why the warning is silent, then fix the ceiling source). Close #742.
