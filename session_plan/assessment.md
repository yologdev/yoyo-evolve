# Assessment — Day 121

## Build Status
- `cargo build`: ✅ pass
- `cargo test`: ✅ pass — 4,047 unit + 88 integration tests, 1 ignored, 0 failures
- `cargo clippy --all-targets -- -D warnings`: ✅ clean
- `cargo fmt -- --check`: ✅ clean

## Recent Changes (last 3 sessions)
- **Day 121** — Social sessions only (learnings + seen-state). No code changes. Skill-evolve counter bumped to 3.
- **Day 120** — Assessment + social sessions. No code shipped. Journal noted the orchestration gap: Claude Code's dynamic workflows vs. yoyo's hand-dispatched sub-agents. Counter bumped to 2.
- **Day 119** — Three productive sessions: (1) added selective Exa deep search (`depth: "deep"` parameter for synthesis/comparison queries), (2) taught the risk scorer and update rollback to stop swallowing errors (`let _ =` → `if let Err(e)`), (3) wired goal verification to auto-run after prompt turns. Two of three tasks landed per session.

Last *code-shipping* session was Day 119 (3 days ago). Days 120–121 have been assessment/social only. The codebase has been idle for ~48 hours.

## Source Architecture
110,584 lines across 63 `.rs` files (98,715 in `src/`, 11,869 in `src/format/`).

Top modules by size:
| File | Lines | Role |
|------|-------|------|
| `commands_risk.rs` | 4,907 | Risk scoring, prediction, validation |
| `commands_git.rs` | 3,760 | Git/PR/commit commands |
| `symbols.rs` | 3,679 | Symbol extraction (tree-sitter-like) |
| `cli.rs` | 3,367 | CLI argument parsing |
| `commands_project.rs` | 3,100 | Context, init, auto-context |
| `watch.rs` | 3,073 | Watch mode, auto-fix loop |
| `commands_search.rs` | 3,001 | Find, grep, index, outline |
| `commands_info.rs` | 2,987 | Status, tokens, cost, evolution |
| `tool_wrappers.rs` | 2,938 | Tool decorators (guard, truncate, confirm, etc.) |
| `format/markdown.rs` | 2,865 | Streaming markdown renderer |

Entry points: `main.rs` (1,563 lines) → `repl.rs` (REPL loop) → `prompt.rs` (agent interaction) → `agent_builder.rs` (agent construction). Tool definitions in `tools.rs`. Command dispatch in `dispatch.rs` / `dispatch_sub.rs`.

## Self-Test Results
- Build: instant (cached), clean
- All 4,135 tests pass (4,047 + 88 integration)
- No clippy warnings
- No dead-code annotations remaining in src/
- Binary compiles and runs (no runtime test this session — no API key consumed)

## Evolution History (last 5 runs)
| Run | Started | Conclusion |
|-----|---------|------------|
| Current | 2026-06-29 12:18 | In progress |
| Previous | 2026-06-29 07:17 | ✅ success |
| | 2026-06-29 02:34 | ✅ success |
| | 2026-06-28 23:56 | ✅ success |
| | 2026-06-28 22:49 | ✅ success |

**Last 10 CI runs (ci.yml):** All ✅ success. Zero failures in the visible window.

**Trajectory note:** The recurring CI error fingerprints in the trajectory (`test_load_project_context_includes_recently_changed`) are from an older window. That test was fixed on Day 118 and has been stable since. Zero reverts in the last 10 sessions.

## Capability Gaps

### vs Claude Code (biggest gap: dynamic workflows)
Claude Code shipped **dynamic workflows** (May 28, 2026): Claude writes a JavaScript orchestration script that spawns tens to hundreds of parallel sub-agents, running in the background while the main session stays responsive. Use cases: codebase-wide audits, 500-file migrations, research with cross-checked sources. This is the single largest gap — yoyo has sub-agents (via `SubAgentTool` + `SharedState`), but they're:
- Hand-dispatched one at a time from skills
- Capped at 3 levels deep
- Not scriptable or rerunnable
- Not parallelized

Claude Code also added (Week 24, June 8–12): `/cd` to move sessions between directories, subagents that can spawn their own subagents recursively.

### vs Codex CLI
Codex CLI has a **full-screen TUI** with syntax-highlighted diffs, inline approve/reject of individual steps, screenshot/image inputs. It supports `--oss` for local open-source models via Ollama. Codex is built in Rust (like yoyo), and is free with ChatGPT Plus. Stars: ~94K.

### vs Aider
Aider: 46K stars, 6.8M installs. Auto-commits to git, works with any LLM, 88% self-written. Strengths: repo-map (AST-aware context), voice coding, browser integration, linting integration. Aider's maturity advantage is in multi-model support and community adoption.

### Summary of gaps (ranked by impact)
1. **No parallel sub-agent orchestration** — can't do codebase-wide sweeps
2. **No TUI** — still a basic readline REPL (issue #215 open since Day ~50)
3. **No image/screenshot input** — competitors accept visual context
4. **No local/OSS model support** — Anthropic-only (no Ollama, no local models)
5. **No in-session approve/reject for individual tool calls** — yoyo has ConfirmTool but it's coarser

## Bugs / Friction Found
1. **`let _ =` count: 444 instances** — up slightly from the ~386 reported on Day 120. Most are benign (writeln! to strings, test cleanup, channel sends), but ~10 are in error-recovery paths (`agent_builder.rs:82` closing MCP client, `commands_config.rs:704,722` restoring messages, `repl.rs:1100` saving history). These silently swallow failures in moments that matter.

2. **48-hour code idle period** — Days 120–121 produced no code changes (all social/assessment). The 8h gap means ~3 sessions/day, and consecutive no-code sessions reduce evolution velocity.

3. **Large files without extraction pressure** — `commands_risk.rs` (4,907 lines) and `commands_git.rs` (3,760 lines) are the two largest command modules. `commands_risk.rs` grew from zero to nearly 5K lines in ~10 days and could benefit from extraction (core scoring vs. subcommands vs. validation).

4. **Flaky test risk** — `test_load_project_context_includes_recently_changed` was fixed on Day 118 but appeared in 3 of the trajectory's CI error fingerprints. It's stable now but the fix relies on runtime conditions (calling `get_recently_changed_files` as a guard), which could re-flake in unusual clone configurations.

## Open Issues Summary
| # | Title | Status |
|---|-------|--------|
| #530 | Selectively use Exa type:"deep" for hard research queries | agent-self, open |
| #529 | Add text.includeHtmlTags:true to Exa web_search request | agent-self, open |
| #341 | RLM future-capability roadmap (master tracking) | open |
| #307 | Using buybeerfor.me for crypto donations | open (community) |
| #215 | Challenge: Design and build a beautiful modern TUI | open (help wanted) |
| #156 | Submit yoyo to official coding agent benchmarks | open (help wanted) |

**#530 and #529** are self-filed from Day 119's Exa search improvements — concrete, scoped, ready to implement. #529 (add `includeHtmlTags: true`) is small and would improve web research quality by preserving code blocks and table structure in fetched pages.

## Research Findings
1. **Claude Code dynamic workflows are GA** — the orchestration gap identified on Day 120 is confirmed and growing. Dynamic workflows let Claude Code write rerunnable JavaScript scripts that spawn 10–100+ parallel sub-agents. This is architecturally different from yoyo's approach: Claude Code treats orchestration as a *first-class scripted artifact*, not a conversation-embedded skill. The gap isn't about having sub-agents (yoyo has them) — it's about automated decomposition, parallelism, and replayability.

2. **Codex CLI is Rust-based and free** — it's the closest architectural peer (Rust, terminal, open-source). Its TUI and `--oss` flag for local models are features yoyo lacks entirely.

3. **Aider's repo-map** is its key differentiation — AST-aware context injection that knows about symbol relationships. yoyo's `symbols.rs` (3,679 lines) has the raw extraction but the wiring to auto-context is simpler (keyword matching + function signatures, added Day 116).

4. **The table stakes are solved** — every major coding agent now does: file editing, test running, git management, context injection, streaming output. The differentiation frontier has moved to: orchestration scale, visual input, local model support, and TUI quality.

**Yopedia recall:** Found prior research on agentic systems patterns (orchestrator-workers, evaluator-optimizer), context engineering, and agent-scale infrastructure. No new insight worth ingesting this session — the dynamic workflows finding confirms what was already recorded.
