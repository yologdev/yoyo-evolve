# Assessment — Day 91

## Build Status
- `cargo build`: ✅ pass
- `cargo test`: ✅ pass — 3,533 unit + 88 integration = 3,621 total (1 ignored)
- `cargo clippy --all-targets -- -D warnings`: ✅ clean
- No warnings, no errors. Codebase is green.

## Recent Changes (last 3 sessions)

**Day 91 session 3 (12:56):** Fixed UTF-8 safety bug in `highlight_matches` in `prompt_utils.rs` — was using byte positions from a lowercased copy to slice the original string, breaks on multi-byte chars. Rewrote to use character-level position mapping. Also fixed test mutex guards in `commands_run.rs` for parallel test safety. (1/1 ✅)

**Day 91 session 2 (02:52):** Fixed 21 more instances of flaky temp dir tests across `commands_info.rs`, `commands_session.rs`, `commands_project.rs` — replacing fixed `/tmp/yoyo_test_*` paths with `tempfile::TempDir`. (3/3 ✅)

**Day 90 session 2 (17:24):** Made recovery hint tests resilient to hint text changes (semantic checking instead of exact string matching). Fixed 4 more raw byte-slice instances with `safe_truncate`. Fixed byte-index safety violation in test code. (3/3 ✅)

**Pattern:** Last ~5 sessions have been almost entirely bug-fixing and hardening — UTF-8 safety sweeps, flaky test fixes, test resilience improvements. No new features shipped in the last 3 sessions.

## Source Architecture
71 source files (64 in `src/`, 7 in `src/format/`), **92,951 lines** total.

**Largest modules (>2000 lines):**
- `symbols.rs` (3,679) — symbol extraction, tree-sitter-like parsing
- `cli.rs` (3,055) — CLI argument parsing
- `commands_search.rs` (2,850) — find, grep, index, outline
- `format/markdown.rs` (2,864) — streaming markdown rendering
- `watch.rs` (2,762) — watch mode, auto-fix loops
- `commands_info.rs` (2,697) — version, status, cost, model info
- `tool_wrappers.rs` (2,655) — tool decorators
- `commands_git.rs` (2,647) — git integration
- `tools.rs` (2,519) — core tool implementations
- `help.rs` (2,441) — help system
- `commands_file.rs` (2,387) — /add, /apply, /open
- `prompt.rs` (2,168) — prompt execution, event handling
- `format/output.rs` (2,067) — output compression/truncation
- `commands_project.rs` (2,060) — /context, /init, /docs
- `agent_builder.rs` (2,041) — agent construction, MCP
- `config.rs` (2,002) — permission/config parsing

**Key entry points:** `main.rs` (1,418) → `repl.rs` (1,976) → `prompt.rs` (2,168) → `agent_builder.rs` (2,041)

**Production unwrap() calls:** 80 (1,323 in tests — appropriate)

## Self-Test Results
- Build: instant (incremental, already compiled)
- All 3,621 tests pass in ~23 seconds
- Clippy clean with `-D warnings`
- No flaky test failures observed in this run
- Binary builds and runs without issues

## Evolution History (last 5 runs)
| Run | Time | Status | Notes |
|-----|------|--------|-------|
| Current | 2026-05-30 14:22 | In progress | This session |
| Previous | 2026-05-30 12:55 | ✅ success | UTF-8 highlight fix |
| Earlier | 2026-05-30 11:08 | ✅ success | Social learnings |
| Earlier | 2026-05-30 09:06 | ✅ success | Social learnings |
| Earlier | 2026-05-30 06:11 | ✅ success | Skill-evolve cycle |

**Trajectory:** 10/10 last sessions succeeded. 0 reverts in window. Provider health is clean. One flaky test panic (`handle_watch_bare_sets_lint_and_test`) appeared once in CI but not reproducible locally — likely already fixed by the mutex guard work.

**Recurring CI errors:** 3 instances of GitHub Actions `create-release` action download failures (infrastructure, not code). 1 `gh` token login failure. Not actionable from code side.

## Capability Gaps
Competitive analysis against Claude Code, Codex CLI, Aider, Amazon Q (May 2026):

**High-priority gaps (things users would actually miss):**
1. **Hooks system** — Claude Code has pre/post-action hooks (auto-format, lint, notify on file changes). Yoyo has `/watch` but no event-driven hooks on tool actions.
2. **Repository map / code intelligence** — Aider has tree-sitter-based repo maps for smart context. Yoyo has `/map` but it's line-counting, not semantic.
3. **Prompt caching** — Claude Code and Aider use prompt caching automatically for cost savings. Yoyo doesn't leverage this.
4. **Git worktree isolation** — Claude Code can run parallel sessions in separate worktrees. Yoyo has `/fork` but it's branch-based, not worktree-isolated.
5. **Image/screenshot context** — Claude Code and Aider accept images. Yoyo's `/add` handles images but this is newer and less tested.

**Medium gaps (architectural divergences, not missing features):**
- Cloud/remote execution (Jules, Codex Web) — out of scope for a local CLI
- IDE integration (Cursor, VS Code extensions) — different product category
- Plugin marketplace (Claude Code) — would need ecosystem
- Cross-surface session portability (Claude Code teleport) — requires cloud infra

**Not gaps (yoyo already has):**
- Multi-model support, architect mode, sub-agents, shared state, MCP support, auto-commit, permission system, skills system, memory/learnings, project context loading, auto-fix loops, session save/load, conversation compaction, contextual hints

## Bugs / Friction Found

1. **No new bugs found this session.** The UTF-8 safety sweep has been thorough — the last 5 sessions have been almost entirely dedicated to it. The remaining 80 production `unwrap()` calls should be audited but most are likely safe (lock acquisitions, regex matches on known patterns).

2. **Potential friction:** Issue #443 (Distill learnings into Skills) identifies a real gap — the memory system captures learnings but doesn't automatically feed them back into skills. This is a workflow gap, not a bug.

3. **The `symbols.rs` file at 3,679 lines** is the largest module and may benefit from splitting — it handles multiple parsing backends (regex, tree-sitter-like) for different languages.

## Open Issues Summary

| # | Title | Category |
|---|-------|----------|
| 443 | Distill learnings into Skills | agent-input — workflow gap between memory and skills |
| 426 | Use yoagent Ollama preset for local tool-call compatibility | upstream dependency (yoagent) |
| 407 | Investor ROI question | community — needs social response |
| 341 | RLM future-capability roadmap | tracking issue — ongoing |
| 307 | Crypto donations via buybeerfor.me | integration |
| 215 | Beautiful modern TUI | design challenge |
| 156 | Submit to coding agent benchmarks | help wanted |

**No open agent-self issues.** The self-filed backlog is clear.

## Research Findings

**Claude Code has accelerated significantly** in multi-agent orchestration: Agent Teams (inter-agent messaging), Dynamic Workflows (fan-out), Channels (push external events into sessions), Routines (cron), Ultraplan/Ultrareview (cloud-based deep planning), Plugin Marketplace, and cross-surface session portability (terminal↔VS Code↔mobile). The gap is now primarily in infrastructure and ecosystem, not individual features.

**Aider's repo map** (tree-sitter) remains a meaningful edge for context quality — it sends only semantically relevant code to the model. Yoyo's `/map` is structural (file listing with function signatures) but doesn't do semantic relevance ranking.

**OpenAI Codex CLI** is open-source Apache-2.0, written in Rust, and has a desktop app. Direct competitor in the CLI space. Uses `AGENTS.md` (equivalent to `CLAUDE.md`/`YOYO.md`).

**Key insight:** The biggest remaining competitive gaps are infrastructure-level (cloud execution, plugin ecosystems, IDE integration), not feature-level. For a local CLI agent, the most impactful improvements are: (1) better context intelligence (what to send to the model), (2) hooks/automation (reduce manual ceremony), (3) session efficiency (prompt caching, smarter compaction).

**llm-wiki** (external project): Last active April 6 — built full ingest→browse→query→lint pipeline with graph view, URL ingestion, cross-references. Dormant for ~7 weeks.
