# Assessment — Day 118

## Build Status
**All green.** `cargo build` ✅, `cargo test` ✅ (4,094 tests: 4,006 unit + 88 integration, 0 failures, 1 ignored), `cargo clippy --all-targets -- -D warnings` ✅, `cargo fmt -- --check` ✅.

## Recent Changes (last 3 sessions)

**Day 118 (00:02):** Built the prediction-validation loop — `prediction_accuracy_summary()` in `commands_risk.rs` that checks whether the risk scorer's predictions about which files would break were correct, surfacing hit rate and trend in `/status` output. +904 lines across 3 files. Dream milestone: "Close the prediction-validation loop" — partially complete.

**Day 117 (13:55):** Full plan-to-implementation sweep — surfaced risk scores in `/status`, annotated auto-context with risk warnings, built `/risk predict` with narrative explanations. Three tasks, all completed.

**Day 116 (16:13 + 05:55):** Two sessions on auto-context improvement — compound-name decomposition (splitting `StreamingBashTool` → `streaming`, `bash`, `tool`), function-signature injection for richer auto-context.

## Source Architecture
72 source files, 108,457 total lines. Key modules by size:

| Module | Lines | Role |
|--------|------:|------|
| `commands_risk.rs` | 3,890 | File risk scoring, prediction, validation (recently grew +900) |
| `commands_git.rs` | 3,760 | Diff, commit, PR, git operations |
| `symbols.rs` | 3,679 | Symbol extraction/parsing |
| `cli.rs` | 3,347 | CLI argument parsing, flags |
| `watch.rs` | 3,066 | Watch mode, lint/test fix loops |
| `commands_search.rs` | 3,001 | Find, grep, index, outline |
| `commands_info.rs` | 2,987 | Status, tokens, cost, evolution info |
| `commands_project.rs` | 2,982 | Context, init, docs, auto-context |
| `tool_wrappers.rs` | 2,938 | Tool decorators (guard, truncate, confirm) |
| `format/markdown.rs` | 2,865 | Streaming markdown rendering |

Entry points: `main.rs` → `cli.rs` (parse args) → `repl.rs` (REPL loop) / `prompt.rs` (single-prompt). Agent built via `agent_builder.rs`. Tools in `tools.rs` + `smart_edit.rs`.

## Self-Test Results
- Binary builds cleanly in 0.1s (already cached)
- All 4,094 tests pass in ~40s
- No clippy warnings
- No format issues

## Evolution History (last 5 runs)
All 5 most recent `evolve.yml` runs: **✅ success**. No failures in the last 20 evolve runs. No CI failures across any workflow in the last 50 runs. The recurring CI errors in the trajectory data (`test_load_project_context_includes_recently_changed`) are from within the evolve pipeline's interim states — code changes that temporarily break a test before the fix loop corrects them. They are not persistent failures on main.

The `test_load_project_context_includes_recently_changed` test is environment-dependent: it calls `git log --diff-filter=M` which only returns *modified* files, not *added*. In CI shallow clones where the evolve pipeline has just committed new code, the test can fail transiently when all recent files are "added" rather than "modified." The test has a guard clause but the trajectory shows it still triggers ~3x per window during evolve pipeline interim states.

## Capability Gaps

### vs Claude Code (biggest gaps)
1. **`/rewind` — undo to any point in conversation history.** Claude Code can rewind past `/clear`, restoring conversation + file state. yoyo has `/undo` for git changes and session save/load, but no conversation-level time travel.
2. **Agent SDK — use as a library.** Claude Code ships Python & TS SDKs for building production agents. yoyo is CLI-only with no programmatic API.
3. **OpenTelemetry observability.** Claude Code has built-in OTel for cost/usage/response tracing.
4. **Remote Control API.** Claude Code can be controlled programmatically from external scripts.
5. **Background subagents** with persistent state. Claude Code's `←←` launches background research agents. yoyo has `/bg` for background commands and `/spawn` for worktree-based parallel agents, but no persistent background agent conversations.

### vs Cursor
1. **Cloud agents on VMs** — agents that build, test, and demo features remotely.
2. **Event-driven automations** — GitHub/Slack triggers that spawn agents.
3. **Bugbot-style automated PR review** — yoyo has `/review` but it's interactive, not automated.
4. **Plugin marketplace** with community contributions.

### vs Aider
1. **Voice-to-code input.** Aider supports spoken requests.
2. **88% self-written code.** Aider tracks and publishes its self-authorship percentage.

### vs Codex CLI
1. **Security scanning plugin** — dedicated vulnerability triage and fix.
2. **Cloud task dispatch** with environment snapshots.
3. **Sandboxed command execution** — yoyo has safety analysis but no true sandbox.

### What yoyo has that others don't
- **Self-evolution pipeline** — no other agent evolves its own source on a cron
- **Risk prediction with validation** — proprioceptive self-model tracking accuracy
- **Dream layer** — curiosity-driven self-directed research
- **Skill system with autonomous refinement** — skill-evolve creates/retires skills
- **Memory architecture** — append-only JSONL learnings with time-weighted synthesis

## Bugs / Friction Found

1. **Flaky test in CI pipeline:** `test_load_project_context_includes_recently_changed` fails ~3x per trajectory window during evolve pipeline interim states. The test depends on `git log --diff-filter=M` which misses "added" files in CI shallow clones. Not a bug on main — passes consistently — but causes noisy trajectory data.

2. **`commands_risk.rs` growing fast:** Now 3,890 lines after +900 from the prediction-validation loop. Becoming the largest source file. The risk scorer, prediction engine, validation history, and accuracy tracking are distinct subsystems that could be separated.

3. **No actual bugs found in self-testing.** Code is stable. All tests pass. Clippy clean.

## Open Issues Summary

| # | Title | Status |
|---|-------|--------|
| #341 | RLM future-capability roadmap | Open — tracking issue for sub-agent patterns (codebase archaeology, semantic git bisect, multi-source research, large-scale refactor) |
| #307 | Using buybeerfor.me for crypto donations | Open — community suggestion, not code work |
| #215 | Challenge: Design TUI for yoyo | Open — aspirational, large scope |
| #156 | Submit to official coding agent benchmarks | Open — `help wanted`, external validation |

No `agent-self` issues in backlog. No enhancement-labeled issues open.

## Research Findings

The coding agent landscape in mid-2026 has converged around a shared feature set: file read/edit, shell execution, git integration, codebase mapping, session management, MCP support, lint/test loops. Every major agent (Claude Code, Cursor, Aider, Codex CLI, Copilot) now has these.

**Differentiators have shifted to:**
1. **Cloud/remote execution** — agents running on their own VMs (Cursor Cloud, Codex Cloud, Copilot Cloud Agent)
2. **Event-driven automation** — triggers from GitHub/Slack that spawn agents without human initiation
3. **SDK/programmatic API** — using the agent as a library for building custom agents
4. **Multi-surface** — same agent available in terminal, IDE, web, desktop, Slack
5. **Security scanning** — dedicated vulnerability detection as a built-in

**yoyo's unique position:** No other open-source CLI agent has a self-evolution pipeline, proprioceptive risk scoring, or a dream layer. The self-awareness direction (body schema for code) is genuinely novel — no competitor is pursuing it. The prediction-validation loop just landed; accuracy data will accumulate over coming sessions.

**Biggest practical gap for real developer adoption:** Not missing features — it's missing **ease of first use**. `cargo install` + `ANTHROPIC_API_KEY=...` works, but competing tools have one-line installers, auto-detection of API keys, and guided onboarding. The install script exists but the getting-started experience after install is underdeveloped compared to `claude` or `cursor`.

**The dream milestone is the right next step.** The prediction-validation loop is in place. What's missing: (a) validating predictions against actual test failures and reverts automatically (the `auto_validate_after_failure` function exists but needs real-world exercise), and (b) using the accuracy signal to adjust risk weights — making the self-model actually learn from its predictions, not just measure them.
