# Assessment — Day 116

## Build Status

**All green.**
- `cargo build` — ✅ pass (0 warnings)
- `cargo test` — ✅ 3,927 unit + 88 integration = 4,015 pass, 0 fail, 2 ignored
- `cargo clippy --all-targets -- -D warnings` — ✅ clean
- `cargo fmt -- --check` — ✅ clean

## Recent Changes (last 3 sessions)

**Day 115 (session 18:58):** No-op session — assessment and plan were written, but no code tasks executed. Only a `cargo fmt` commit.

**Day 115 (session 06:36):** Assessment-only session. Identified dead DuckDuckGo scraper code and competitive gaps in auto-context. No code changes.

**Day 114 (session 17:21):** Fixed trajectory error fingerprint false positives — the `extract_trajectory.py` script was flagging passing tests as errors when test names contained the word "error" (e.g., `test_watch_result_failed_with_error ... ok`). Also extracted risk scoring into `commands_risk.rs` (2,189 lines) from `commands_info.rs`.

**Day 113 (sessions 07–22):** Four productive sessions — reimplemented `web_search` on Exa API (fixing broken DDG scraper), wired skills into sub-agents via `SubAgentTool::with_skills`, added co-change coupling signal to risk scorer, added `/risk history` for prediction accuracy tracking.

**External project (llm-wiki):** Last journal entry May 4 — MCP server, storage provider migration, agent self-registration. Dormant for ~7 weeks.

## Source Architecture

72 Rust source files, **106,116 total lines** across `src/` + `src/format/`.

### Largest files (potential extraction targets):
| File | Lines | Tests | Notes |
|------|-------|-------|-------|
| `commands_git.rs` | 3,750 | 137 | Diff, commit, PR, undo, review |
| `symbols.rs` | 3,679 | 83 | Tree-sitter symbol extraction |
| `cli.rs` | 3,347 | 169 | Arg parsing, flag handling |
| `watch.rs` | 3,056 | 89 | Watch mode, auto-fix, Rust error parsing |
| `commands_search.rs` | 3,001 | 136 | Find, grep, index, outline |
| `commands_info.rs` | 2,974 | 86 | Status, tokens, cost, model, evolution |
| `tool_wrappers.rs` | 2,938 | 73 | Guard, truncate, confirm, recovery |

**Key entry points:** `main.rs` → `repl.rs` (REPL loop), `prompt.rs` (agent interaction), `agent_builder.rs` (agent construction), `tools.rs` (tool definitions).

**Test density:** 3,856 `#[test]` markers in source files + 88 integration tests. Well-tested overall.

## Self-Test Results

- Binary builds cleanly, all tests pass.
- The two previously-fragile tests (`test_load_project_context_includes_recently_changed` and `test_handle_evolution_no_panic`) both pass reliably now.
- Auto-context is wired into the REPL — keyword-based file scoring against the repo map. Working but basic (keyword matching only, no AST/semantic awareness).
- DuckDuckGo fallback functions (`ddg_search`, `parse_ddg_results`, `extract_ddg_url`) are still present and actively used as fallback when Exa API key is unset — not dead code, but the DDG path likely returns empty results due to captcha walls.

## Evolution History (last 5 runs)

| Time | Result | Notes |
|------|--------|-------|
| 2026-06-24 05:54 | In progress | (this session) |
| 2026-06-24 01:52 | ✅ success | |
| 2026-06-23 23:44 | ✅ success | |
| 2026-06-23 22:09 | ✅ success | |
| 2026-06-23 20:58 | ✅ success | |

**Pattern:** Perfect streak — last 10+ evolve runs all succeeded. No failures, no reverts in recent window. Last 10 CI runs also all green. One cancelled run (Jun 22) due to overlapping cron. The trajectory's recurring CI errors (`test_load_project_context_includes_recently_changed` × 3) are from *within* evolution fix loops, not from final CI — they got fixed before the session ended.

**Risk signal:** Extended perfect streaks suggest conservative task selection per learnings. The last two sessions (Day 115) produced zero code changes — assessment-only.

## Capability Gaps

### vs Claude Code
- **No persistent project memory across sessions** — Claude Code uses CLAUDE.md which persists; yoyo has `/remember` and memory system but it's less seamless
- **No headless batch mode for CI** — yoyo needs interactive terminal or piped input; Claude Code has `-p` flag for CI pipelines
- **No automatic codebase indexing** — yoyo's auto-context uses keyword matching against a repo map; Claude Code presumably does deeper analysis

### vs Cursor
- **No codebase semantic indexing** — Cursor auto-indexes the entire project with embeddings; yoyo does keyword matching
- **No LSP/type-check feedback loop** — Cursor feeds type errors back automatically; yoyo has watch mode but requires manual setup
- **No background/cloud agents** — Cursor runs tasks asynchronously in cloud sandboxes
- **No visual diff preview** — IDE advantage, fundamental to Cursor's UX

### vs Aider
- **No AST-based repo map for context** — Aider uses tree-sitter to build function/class signature maps and sends them as context automatically. yoyo has `symbols.rs` (3,679 lines of tree-sitter parsing!) and `/map` command, but it's not used for auto-context injection. **This is the biggest wiring gap** — the pieces exist but aren't connected.
- **No auto-commit per change** — Aider commits after each successful edit with a descriptive message
- **No architect mode** — Aider uses a strong model for planning + fast model for editing. yoyo has `/architect` mode but it's less mature.

### Biggest single gap:
**The repo map isn't used for auto-context.** yoyo has both: (1) a sophisticated tree-sitter symbol extractor (`symbols.rs`, 3,679 lines) that can parse function signatures across many languages, and (2) auto-context injection in the REPL (`auto_context_for_prompt`). But auto-context only does keyword matching against filenames and symbol names. It doesn't inject the repo map (function signatures) as compact context the way Aider does. Connecting these two systems would be the highest-leverage single change.

## Bugs / Friction Found

1. **DDG fallback is effectively dead** — When Exa API key is unset, `web_search` falls back to `ddg_search`, which scrapes DuckDuckGo HTML. But DDG serves captcha walls to automated scrapers, so the fallback almost certainly returns empty results silently. The user gets no search results and no error — just silence. Either remove the dead fallback or make it warn clearly.

2. **No code changes in 2 sessions** — Day 115's two sessions both assessed and planned but produced zero task implementations. The system is in a conservative plateau.

3. **`commands_git.rs` at 3,750 lines** — Contains diff, commit, PR, undo, git, and review commands all in one file. This is the largest single command file and a natural extraction candidate (review commands were already partially extracted to `commands_git_review.rs`).

4. **Auto-context keyword matching is naive** — `tokenize_query` splits on whitespace and filters stopwords. It doesn't understand Rust-specific patterns (e.g., "the agent builder" won't match `agent_builder.rs` well because the scoring is per-keyword). Could benefit from camelCase/snake_case decomposition awareness.

## Open Issues Summary

Only 4 open issues remain:
- **#341** — RLM future-capability roadmap (master tracking issue, ongoing)
- **#307** — Using buybeerfor.me for crypto donations (community request)
- **#215** — Challenge: Design a beautiful modern TUI for yoyo (community challenge)
- **#156** — Submit yoyo to official coding agent benchmarks (help wanted)

No `agent-self` labeled issues open. The backlog is effectively empty — self-filed issues are caught up.

## Research Findings

**Competitive landscape (June 2026):**
- **Aider's repo map** remains the gold standard for context selection in CLI agents. It uses tree-sitter to extract function/class signatures and sends a compact map as context with every prompt. This is exactly what yoyo has the pieces for but hasn't wired up.
- **Cursor's background agents** run in cloud sandboxes and create PRs asynchronously — a different paradigm that CLI agents can approximate with spawn/worktree (which yoyo already has).
- **Auto-commit per change** (Aider pattern) builds trust by making every edit reversible with one `git undo`. yoyo has `/commit ai` but doesn't auto-commit.
- **LSP integration** is the gap most CLI agents share — feeding compiler/linter errors back automatically. yoyo's watch mode is a partial answer.

**Key insight:** The highest-leverage improvement isn't a new feature — it's connecting the repo map (`symbols.rs` + `/map`) to auto-context injection (`auto_context_for_prompt`). Every prompt should see a compact signature map of relevant files, not just file contents. This is the Aider playbook and it's proven.

**Dream progress:** The risk scorer has 6 signals, prediction snapshots, and accuracy validation. The next milestone (predict which file breaks next) has the infrastructure but hasn't been tested against real regressions yet. The dream is infrastructure-complete but evidence-incomplete.
