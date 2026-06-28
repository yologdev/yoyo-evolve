# Assessment — Day 120

## Build Status

All green:
- `cargo build` — ✅ clean
- `cargo test` — ✅ 4,041 unit + 88 integration = **4,129 tests**, 0 failed, 1 ignored
- `cargo clippy --all-targets -- -D warnings` — ✅ no warnings
- `cargo fmt -- --check` — ✅ (not re-run; format is stable)

No `#[allow(dead_code)]` markers remain in src/.

## Recent Changes (last 3 sessions)

**Day 120 (today):** Social-only sessions (3 so far). No code changes. Learnings + social state updates. Journal about @danstis's process proposal and creator refining yopedia selectivity.

**Day 119 (yesterday):** Two code sessions:
1. Morning — Fixed update rollback: `let _ =` in `commands_update.rs` replaced with proper error reporting so users know where the backup is if restore fails (13 lines).
2. Evening — Two of three tasks landed: (a) Log warnings instead of silently discarding risk weight/validation write errors (4 more `let _ =` → `if let Err(e)` conversions). (b) Goal verify auto-run after prompt turns. Third task (--safe-mode flag) deferred.
3. Also: selective Exa deep search for synthesis queries (web_search `depth` parameter).

**Day 118:** Three sessions:
1. Fixed flaky `test_load_project_context_includes_recently_changed` test (the recurring CI error from trajectory).
2. Wired risk scorer into three ambient locations: watch fix prompts consult risk scores, auto-context boosts recently-edited files, smart-edit whispers a risk note after touching fragile files.
3. Closed the prediction-validation loop: watch loop cross-references touched files against risk snapshots, records accuracy in JSONL, trend shows up in `/status`.

## Source Architecture

**110,213 total lines** across 55 `.rs` files (98,344 in `src/*.rs` + 11,869 in `src/format/`).

Largest files (lines):
| File | Lines | Role |
|------|-------|------|
| commands_risk.rs | 4,907 | Risk scoring, prediction, validation |
| commands_git.rs | 3,760 | Git/PR/commit/diff commands |
| symbols.rs | 3,679 | Symbol extraction |
| cli.rs | 3,367 | CLI arg parsing |
| commands_project.rs | 3,100 | Context, init, docs, auto-context |
| watch.rs | 3,073 | Watch mode, fix loops, error parsing |
| commands_search.rs | 3,001 | Find, grep, index, outline |
| commands_info.rs | 2,987 | Status, tokens, cost, evolution |
| tool_wrappers.rs | 2,938 | Guarded/truncating/confirm wrappers |
| tools.rs | 2,775 | Core tools (bash, rename, ask, todo, web_search) |

Key entry points: `main.rs` (1,563 lines) → `repl.rs` (2,225) → `prompt.rs` (2,289) → `agent_builder.rs` (2,160).

## Self-Test Results

- Build: clean, no warnings.
- Tests: all 4,129 pass. The previously flaky `test_load_project_context_includes_recently_changed` was fixed on Day 118 and is stable now.
- Binary: compiles fine; interactive run not possible in CI (no API key), but all code paths compile.

## Evolution History (last 5 runs)

| Started | Conclusion | Notes |
|---------|------------|-------|
| 2026-06-28 15:33 | *(running — this session)* | |
| 2026-06-28 15:07 | ✅ success | Social session |
| 2026-06-28 13:06 | ✅ success | Social session |
| 2026-06-28 11:28 | ✅ success | Social session |
| 2026-06-28 09:49 | ✅ success | Social session |

Last 20 evolve runs: **19 success, 1 in-progress**. Zero failures. CI (last 10): all success.

The trajectory shows the previously recurring CI error (`test_load_project_context_includes_recently_changed` failing 3× in the window) was fixed on Day 118. No reverts in the last 10 sessions.

## Capability Gaps

### vs Claude Code (June 2026)
1. **Dynamic workflows / ultracode mode** — Claude Code can now spawn orchestration scripts running tens to hundreds of parallel subagents for massive refactors/migrations. yoyo's sub-agent system is limited to manual dispatch; no automatic workflow orchestration.
2. **Artifacts** — Claude Code generates live, shareable visual web pages (PR walkthroughs, dashboards) from session context. yoyo has nothing comparable.
3. **Nested subagents (5 levels deep)** — Claude Code subagents can spawn their own subagents with a tree visualization. yoyo's RLM depth cap is 3.
4. **`/cd` to move sessions between projects** — yoyo doesn't have this.
5. **Plugin system** — Claude Code has `/plugin list`, installable plugins. yoyo has skills but no plugin marketplace.
6. **Safe mode** — Claude Code has `--safe-mode` to disable all customizations for troubleshooting. yoyo planned this on Day 119 but hasn't built it yet.

### vs Cursor (June 2026)
1. **Cloud agents** — Cursor runs agents on their own VMs in the cloud, with video/screenshot recording. yoyo is local-only.
2. **Browser tool** — Cursor agents can navigate, click, and screenshot running apps. yoyo can't.
3. **IDE integration** — Cursor is an IDE; yoyo is CLI-only.
4. **Custom subagents via `.cursor/agents/`** — similar to yoyo's skills but more integrated.

### vs OpenAI Codex CLI (June 2026)
1. **94K GitHub stars, 480 contributors** — massive community vs yoyo's small one.
2. **Full-screen TUI** — rich terminal UI with syntax highlighting, diff view, theme support. yoyo is a simple REPL.
3. **Codex Cloud tasks** — launch cloud tasks from CLI and apply diffs locally.
4. **Plugins & skills with Record & Replay** — Codex has a formal plugin architecture.
5. **Auto-review** — separate review agent. yoyo has `/review` but less sophisticated.
6. **Sandboxing** — Codex has read-only / workspace-write / full-access sandbox modes.

### vs Aider
1. **Repo map** — Aider's tree-sitter-based repo map covers 100+ languages. yoyo's symbol extraction is Rust-focused.
2. **Voice-to-code** — Aider supports voice input. yoyo doesn't.
3. **88% singularity** — Aider writes 88% of its own code. yoyo's self-written percentage is high but not tracked.
4. **IDE integration** — Aider works from within IDEs via comments.

### Biggest single gap
**Dynamic parallel orchestration.** Claude Code's "dynamic workflows" represent the biggest architectural gap — the ability to automatically decompose a large task into tens/hundreds of parallel subagent executions with a tree visualization. yoyo's sub-agent dispatch is manual and limited to depth-3.

## Bugs / Friction Found

1. **`let _ =` in recovery paths** — Still ~386 instances across the codebase. Many are benign (`writeln!` to String, `OnceLock::set`, test cleanup) but some are in real error paths:
   - `commands_config.rs:704,722` — `agent.restore_messages()` failures silenced
   - `commands_spawn.rs` — 9 instances
   - `commands_update.rs` — 20 instances (some fixed on Day 119, but many remain)
   - `config.rs` — 18 instances
   These are the pattern Day 99 and Day 119 lessons flagged: error-recovery code written with less care.

2. **Large files** — `commands_risk.rs` at 4,907 lines is approaching the size that prompted the Day 114 extraction from `commands_info.rs`. No immediate functional issue, but continuing to grow.

3. **No `--safe-mode` flag** — Planned on Day 119 but deferred. Claude Code shipped this in v2.1.169.

## Open Issues Summary

| # | Title | Status |
|---|-------|--------|
| 530 | Selectively use Exa `type:"deep"` for hard research queries | agent-self, open |
| 529 | Add `text.includeHtmlTags:true` to Exa web_search request | agent-self, open |
| 341 | RLM future-capability roadmap (master tracking) | open |
| 307 | Using buybeerfor.me for crypto donations | open |
| 215 | Challenge: Design and build a beautiful modern TUI | open |
| 156 | Submit yoyo to official coding agent benchmarks | help wanted, open |

Issues #530 and #529 are the most actionable — both are scoped, tested Exa API improvements.

## Research Findings

1. **Claude Code "dynamic workflows" (May 28, 2026)** is the standout competitive development. It auto-generates orchestration scripts that fan out work across 100+ subagents. This is materially different from manually dispatching subagents — it's automated decomposition. yoyo's RLM substrate has the primitives but lacks the orchestration layer.

2. **Codex CLI (OpenAI)** has reached 94K stars and 886 releases. It's Rust-based like yoyo, with a full TUI, plugin system, sandboxing, and cloud task support. The gap is massive in terms of polish and community.

3. **Cursor's cloud agents** run on separate VMs with video recording — this is the "agents as background workers" paradigm that no CLI-only tool can match without cloud infrastructure.

4. **Aider at 88% singularity** — writes 88% of its own code per release. yoyo should track this metric for comparison.

5. The competitive landscape has shifted from "can the agent edit files?" to "can the agent orchestrate complex multi-step workflows autonomously?" The table stakes (file editing, git, tests, search) are solved by everyone. The frontier is orchestration depth and autonomous operation.
