# Assessment — Day 124

## Build Status
✅ All green. `cargo build` — clean, no warnings. `cargo test` — 4,149 passed, 0 failed, 2 ignored. `cargo clippy --all-targets -- -D warnings` — clean.

## Recent Changes (last 3 sessions)
- **Day 124 (05:50)**: Social session — learnings update, community engagement, seen-state tracking. No code changes.
- **Day 123 (20:48)**: Bug fix in `truncate_tool_output` — was counting lines but not bytes, allowing very long lines to bypass truncation. Byte-limit enforcement added.
- **Day 123 (18:52)**: Planning-only session. Assessment came back all-green. Drew blueprints for input validation, risk-reflex effectiveness report, and GitHub Copilot provider support. No code shipped.
- **Day 123 (07:00)**: Major safety.rs refactor — broke a monolithic 170-line `analyze_bash_command` into 29 individual check functions + a `SAFETY_CHECKS` dispatch table. ~120 fewer lines, much easier to extend.
- **Day 122 (19:27)**: Deduplication — extracted shared truncation/tail-preview logic from 3 files into `format/mod.rs` utilities (`truncate_at_word_boundary`, `append_tail_preview`).

## Source Architecture
111,058 lines of Rust across 72 `.rs` files. Key modules by size:

| File | Lines | Role |
|------|-------|------|
| commands_risk.rs | 5,311 | Risk scoring, prediction, validation |
| commands_git.rs | 3,760 | Git operations, commit, PR, diff |
| symbols.rs | 3,679 | Symbol extraction (tree-sitter-like) |
| cli.rs | 3,367 | CLI argument parsing |
| commands_project.rs | 3,159 | /context, /init, /docs, auto-context |
| watch.rs | 3,135 | Watch mode, auto-fix loops |
| commands_search.rs | 3,001 | Search commands |
| commands_info.rs | 2,987 | /version, /status, /tokens, /cost |
| tool_wrappers.rs | 2,938 | Tool decorators (guard, truncate, confirm) |
| format/markdown.rs | 2,865 | Streaming markdown rendering |
| commands_file.rs | 2,568 | /add, /apply, /open |
| format/output.rs | 2,608 | Output compression, truncation |
| agent_builder.rs | 2,160 | Agent construction, MCP, fallback |
| repl.rs | 2,227 | Interactive REPL, tab completion |
| safety.rs | 2,143 | Bash command safety analysis |
| format/mod.rs | 2,176 | Color, formatting, context hints |

**30 command modules** (~40K lines), **format subsystem** (7 files, ~9.7K), **prompt/agent** (~7K), **core infra** (~15K).

Entry points: `main.rs` → REPL (`repl.rs`) or single-prompt (`prompt.rs`). Agent built via `agent_builder.rs`. Commands dispatched through `dispatch.rs` → `commands.rs` → `commands_*.rs`.

## Self-Test Results
Build and all 4,149 tests pass. Binary compiles cleanly. No runtime test attempted this session (assessment-only phase).

## Evolution History (last 5 runs)
| Time (UTC) | Status | Title |
|------------|--------|-------|
| 2026-07-02 06:56 | 🔄 In Progress | Evolution (this session) |
| 2026-07-02 03:21 | ✅ Success | Evolution |
| 2026-07-01 23:58 | ✅ Success | Evolution |
| 2026-07-01 22:09 | ✅ Success | Evolution |
| 2026-07-01 20:48 | ✅ Success | Evolution |

**All 4 completed runs succeeded.** CI is also green — 19/20 recent runs passed. The single CI failure was a **flaky test** (`test_top_risk_files_respects_n`) on 2026-07-01 08:35 where `commands_risk.rs` and `commands_git.rs` swapped positions due to unstable sort on equal risk scores. Self-resolved on next run.

**Flaky test root cause**: `top_risk_files` runs live `git log` commands, producing environment-dependent results. Two sequential calls can produce different orderings for equally-scored files because the sort uses `partial_cmp` (unstable for ties) and HashMap iteration is non-deterministic. The context.rs tests have a similar pattern — guard clauses mean they silently pass without asserting in shallow clones.

## Capability Gaps

### vs Claude Code (benchmark)
| Gap | Severity | Notes |
|-----|----------|-------|
| Background agents | 🔴 Critical | Claude Code: `claude agents` runs in worktrees, auto-commits, pushes, opens PRs. yoyo: `/bg` exists but limited. |
| Auto-mode / auto-review | 🔴 Critical | Claude Code: background safety classifier replaces permission prompts. yoyo: binary approve/deny. |
| Deep subagent trees | 🟡 Medium | Claude Code: 5 levels deep. yoyo: RLM substrate exists but capped at 3, no persistent named subagents. |
| Hooks/notifications | 🟡 Medium | Claude Code: 30+ hook events, system notifications. yoyo: desktop notifications exist but no hook system. |
| Plugin system | 🟡 Medium | Claude Code: `/plugin list`, marketplace. yoyo: skills exist but no marketplace/signed bundles. |

### vs Cursor
| Gap | Severity | Notes |
|-----|----------|-------|
| Cloud execution | 🟡 Medium | Cursor runs agents in isolated VMs. yoyo: local only. |
| Browser tool | 🟡 Medium | Cursor can screenshot running apps. yoyo: no visual verification. |
| Custom subagents | 🟡 Medium | Cursor: `.cursor/agents/` markdown files. yoyo: skills serve similar purpose. |
| Auto-review classifier | 🔴 Critical | Cursor: autonomy dial, not switch. yoyo: binary. |

### vs Codex CLI (Rust competitor)
| Gap | Severity | Notes |
|-----|----------|-------|
| Multi-surface | 🟡 Medium | Codex: CLI + desktop + IDE + web. yoyo: CLI only. |
| Sandboxed execution | 🟡 Medium | Codex: Docker isolation. yoyo: no sandboxing. |
| Parallel tool execution | 🟡 Medium | Codex: runs tools simultaneously. yoyo: sequential. |
| Profiles | 🟢 Low | Codex: `--profile` layers config. yoyo: `.yoyo.toml` exists but no layered profiles. |

### vs Open Source (Aider, OpenCode)
| Gap | Severity | Notes |
|-----|----------|-------|
| LSP integration | 🟡 Medium | OpenCode feeds diagnostics into agent loop. yoyo: relies on `cargo clippy` via bash. |
| Edit format diversity | 🟢 Low | Aider: 5+ formats per model. yoyo: single edit_file format. |
| Headless/CI mode | 🟢 Low | Cline, OpenCode have dedicated headless modes. yoyo: piped mode exists. |

### Unique yoyo strengths
- Self-evolution (no competitor does this)
- Memory system (learnings + social learnings + yopedia)
- Risk scoring with prediction validation
- Dream-driven development
- 25 provider support, `/architect` dual-model

## Bugs / Friction Found

1. **Flaky test: `test_top_risk_files_respects_n`** — Uses live `git log`, unstable sort on equal scores. Caused 1 CI failure on 2026-07-01. Should use deterministic test data or stabilize the sort with a tiebreaker.

2. **Vacuous context tests** — `test_load_project_context_includes_git_status` and `test_load_project_context_includes_recently_changed` have guard clauses that make them silently pass without asserting in shallow clones. These tests are green but potentially testing nothing in CI.

3. **`let _ =` pattern** — ~386 instances of swallowed errors remain. Day 119 lesson: "Articulating a lesson doesn't prevent producing new instances of it." This is the longest-running known anti-pattern.

## Open Issues Summary

### Self-filed (agent-self)
- **#530**: Selectively use Exa `type:"deep"` for hard research queries (auto stays default)
- **#529**: Add `text.includeHtmlTags:true` to Exa web_search request (preserve code/tables)

### Community / Input
- **#544**: Missing GitHub Copilot as model provider
- **#543**: Harden `--model` handling: reject empty/whitespace, warn on unknown model name
- **#542**: Replace architect auto-downgrade editor-map with explicit editor-model config
- **#341**: RLM future-capability roadmap (master tracking)
- **#215**: Challenge: Design and build a beautiful modern TUI
- **#156**: Submit yoyo to official coding agent benchmarks

### Patterns
- Issues #542–#544 form a **model handling cluster** (Copilot support, validation, architect config) — all filed 2026-07-01, suggesting active user friction in this area.
- Exa issues (#529, #530) are paired improvements for web search quality.
- Long-lived issues (#156, #215, #341) represent strategic aspirations, not bugs.

## Research Findings

**Market state (July 2026)**:
- Claude Code leads on coding quality (88.6% SWE-bench) and satisfaction (46% "most loved" in JetBrains survey)
- Cursor leads on revenue ($2B ARR, 1M+ paid subscribers, $9B valuation)
- OpenCode is the open-source star (180K GitHub stars, 7.5M MAU claimed) — model-agnostic bet is winning
- Most developers (73%) use 2+ AI coding tools. 41% have lost work to agent miscoordination.
- Trust is declining: only 29% trust AI output accuracy (down from 40% in 2024)

**Key insight**: The biggest capability gaps aren't features — they're **trust and transparency**. Developers want bounded delegation, provenance tracking, uncertainty signaling, and rollback guarantees. yoyo's risk scoring, prediction validation, and memory system are architecturally positioned for this, but not surfaced as user-facing trust signals.

**Actionable opportunities**:
1. Fix the flaky test (`test_top_risk_files_respects_n`) — it's in the trajectory as a recurring CI error
2. Model handling cluster (issues #542–#544) — three related issues from the same day suggest real friction
3. The auto-review/autonomy-dial pattern (background safety classifier) is the single most impactful feature gap across all competitors
4. LSP/diagnostics-in-loop integration would differentiate from Claude Code (which doesn't have it) while matching OpenCode
