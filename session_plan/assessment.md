# Assessment — Day 124

## Build Status
All green. `cargo build`, `cargo test` (4,064 + 88 = 4,152 tests, 0 failures, 1 ignored), `cargo clippy --all-targets -- -D warnings` — all pass clean. No warnings, no errors.

## Recent Changes (last 3 sessions)

**Day 124 (earlier today):** Fixed two vacuous context tests in `context.rs` that wrapped assertions inside `if let Some(...)` — they silently passed when the function returned `None` in CI. Fixed flaky risk-score sort by adding a filename tiebreaker. Hardened `--model` flag parsing in `cli.rs` (trim whitespace, warn on unrecognized model names).

**Day 123:** Three sessions. (1) Major safety.rs refactor — broke the 170-line monolith `analyze_bash_command` into 29 individual check functions with a dispatch table. (2) Planning-only session (no code). (3) Fixed `truncate_tool_output` to enforce byte limits even when line count is low — a few long lines could bypass the line-count guard.

**Day 122:** Deduplicated three copies of tail-preview/word-boundary truncation logic into shared utilities in `format/mod.rs`. Fixed `iptables -F` vs `-f` case-sensitivity bug in safety.rs. Added `chmod 777` detection. Fixed a reverse-shell byte-slice trimming bug.

**External work (llm-wiki):** Last entry May 4 — MCP server tools, storage migration. Dormant for ~2 months.

## Source Architecture
65 source files, ~111,162 total lines of Rust. Key modules by size:

| Module | Lines | Role |
|--------|------:|------|
| commands_risk.rs | 5,373 | File risk scoring, prediction, validation |
| commands_git.rs | 3,760 | Git operations, commit, PR, diff |
| symbols.rs | 3,679 | Symbol extraction (tree-sitter-like) |
| cli.rs | 3,379 | CLI argument parsing, flags |
| commands_project.rs | 3,159 | Project context, /init, auto-context |
| watch.rs | 3,135 | Watch mode, auto-fix loop |
| commands_search.rs | 3,001 | Find, grep, index, outline |
| commands_info.rs | 2,987 | Status, version, cost, evolution info |
| tool_wrappers.rs | 2,938 | Tool decorators (guard, truncate, confirm) |
| format/markdown.rs | 2,865 | Streaming markdown renderer |
| tools.rs | 2,775 | Core tool implementations |
| format/output.rs | 2,608 | Output compression, truncation |
| commands_file.rs | 2,568 | /add, /apply, /open |
| safety.rs | 2,143 | Bash command safety analysis |
| format/mod.rs | 2,176 | Color, formatting utilities |

3,993 `#[test]` functions across source files. Entry point: `main.rs`. Agent built via `agent_builder.rs` using yoagent.

## Self-Test Results
- Binary builds successfully
- All 4,152 tests pass (4,064 lib + 88 integration)
- Clippy clean with `-D warnings`
- No flaky tests observed this run (the two previously flaky tests — context recently-changed and risk sort — were both fixed earlier today)

## Evolution History (last 5 runs)
All five most recent evolve runs: **success**.
Last 10 evolve runs: all success. Last 10 CI runs: all success.
Zero reverts in the current trajectory window. Zero CI failures in the last 20 runs.

The trajectory data shows some historical CI errors (4× `test failed` and 2× `test_load_project_context_includes_recently_changed`) but those were fixed in the Day 124 morning session.

## Capability Gaps

### vs Claude Code (biggest gap: dynamic workflows / parallel orchestration)
Claude Code shipped **dynamic workflows** on May 28 (Opus 4.8): the system writes JavaScript orchestration scripts that spawn tens to hundreds of parallel subagents in a single session. Users can run `/workflows` to view runs, and an "ultracode" effort level auto-triggers workflows for complex tasks. This is a qualitative leap — not just "has subagents" but "auto-decomposes and orchestrates at scale." yoyo has `SubAgentTool` + `SharedState` (the RLM substrate) but dispatch is manual, serial, capped at depth 3. The gap is **autonomy of orchestration**, not presence of subagents.

Claude Code also has: browser integration (Chrome), `/simplify` for cleanup-only reviews, Agents Window for viewing background sessions, and fast mode on Opus 4.8 at reduced cost.

### vs Cursor (parallel agents + visual design mode)
Cursor 3.0 (April 2026) shipped an **Agents Window** for running many agents in parallel across repos and environments. `/multitask` breaks large tasks into subagent fleets. `/worktree` creates isolated git worktrees. `/best-of-n` runs the same task across multiple models and compares outcomes. **Design Mode** lets users annotate UI elements in a browser and feed visual context to the agent. Multi-root workspaces enable cross-repo changes.

### vs Aider
Aider at 47K stars, latest release v0.86.0. Supports Claude 4.5/4.6, GPT-5.3-codex, Grok-4. Claims 88% self-written code per release. Core capabilities (repo map, multi-language, git auto-commit, voice-to-code) are mature. Aider's main advantage: wide model support and established community.

### vs Codex CLI (OpenAI)
Codex CLI at 95K stars. Full-screen TUI with syntax-highlighted markdown. Subagent workflows for parallel exploration, test, triage. Session resumption from stored transcripts. Auto-review for sandboxed execution. Key advantage: polished TUI and seamless GPT-5 integration.

### Summary of gaps (priority order):
1. **Parallel/autonomous orchestration** — all competitors now have it, we don't
2. **Visual/browser integration** — Cursor has Design Mode, Claude Code has Chrome
3. **TUI polish** — Codex has a full-screen TUI; we have a basic REPL
4. **Model breadth** — Community issue #544 asks for GitHub Copilot as a provider

## Bugs / Friction Found

1. **389 `let _ =` instances remain** — down from 400+ but still a large surface of silently swallowed errors. Many are in test setup (harmless) but some are in production paths (e.g., `banner.rs:452` create_dir_all, `commands_fork.rs:396` create_dir_all).

2. **Issue #543 is partially done** — the `--model` flag now trims whitespace and warns on unrecognized names (committed today), but the `empty/whitespace reaches API` guard described in the issue body hasn't been fully addressed at the `parse_model_config` level in `cli.rs`.

3. **Issue #542 (architect editor-model)** — creator decision filed: remove the auto-downgrade editor-model map entirely and let users configure their own editor model. Not yet implemented.

4. **No provider for GitHub Copilot** (#544) — community request, would need OAuth device flow for authentication.

## Open Issues Summary

| # | Title | Labels | Status |
|---|-------|--------|--------|
| 544 | GitHub Copilot as model provider | (community) | New, needs research |
| 543 | Harden --model handling | agent-input | Partially done (Day 124) |
| 542 | Replace architect auto-downgrade editor-map | agent-input | Creator decision filed, not started |
| 530 | Exa deep search for hard queries | agent-self | Open, self-filed |
| 529 | Exa includeHtmlTags for code/tables | agent-self | Open, self-filed |
| 341 | RLM future-capability roadmap | (tracking) | Long-term tracking |
| 215 | Beautiful modern TUI | (challenge) | Long-term challenge |
| 156 | Submit to coding agent benchmarks | help wanted | Long-term |

## Research Findings

1. **The orchestration gap is real and widening.** Claude Code, Cursor, and Codex CLI all shipped parallel multi-agent orchestration in Q1-Q2 2026. yoyo's RLM substrate has the primitives (SubAgentTool, SharedState) but no autonomous decomposition or parallel dispatch. This is the single largest competitive gap.

2. **Opus 4.8 exists** — Claude's latest model with "4x less likely to let code flaws pass unremarked." yoyo's provider list doesn't include opus-4-8 in its known models (though users can pass any model name).

3. **GPT-5.3-codex is current** — Aider already supports it. yoyo's OpenAI known models list should be checked for currency.

4. **Table stakes are solved** — every competitor has file editing, git integration, test running, context management. The frontier is now at scale (parallel agents), autonomy (self-orchestration), and integration (browser, IDE).
