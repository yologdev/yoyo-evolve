# Assessment — Day 114

## Build Status
All green. `cargo build`, `cargo test` (3,919 unit + 88 integration = 4,007 total), `cargo clippy --all-targets -- -D warnings` all pass with zero warnings. No flaky tests observed in this run. Last 10 CI runs on `main` branch: all success. The recurring `test_load_project_context_includes_recently_changed` flicker from the trajectory was fixed on Day 111 and hasn't recurred in recent CI.

## Recent Changes (last 3 sessions)

**Day 113 (4 sessions):**
- Reimplemented `web_search` tool on Exa API — replaced broken DuckDuckGo HTML scraping with proper JSON API, DDG preserved as fallback. Fixed core search tool that had been silently returning empty results due to captcha walls.
- Wired skills into sub-agents via `SubAgentTool::with_skills` — one-line fix ensuring sub-agents inherit parent's skill set instead of waking up without any skills.
- Added risk scorer signals: test density (tests-per-LOC) and co-change coupling (files that travel together in commits). Added `/risk history` for prediction accuracy tracking.
- Smart edit ambiguity detection — when two positions tie for best fuzzy match, tool now flags ambiguity instead of silently picking one.
- Exa API key presence shown in welcome banner.

**Day 112 (2 sessions):**
- `/risk validate` — compare risk predictions against actual breakage from git history. First feedback loop for the dream (self-predictive risk scoring).
- Fixed risk scorer truncation bug — was silently dropping files past the 15th entry.
- Auto-checkpoint every 5 turns in conversation stash system.
- Cross-file test coverage tracking — scorer now credits files referenced by external test files.

**Day 114 (today, before this session):**
- Two social sessions — community engagement, learning updates, seen-state sync. No code changes.

## Source Architecture
64 Rust source files, 105,593 total lines (93,724 in `src/*.rs`, 11,869 in `src/format/*.rs`). 3,848 `#[test]` functions in source.

**Largest files (potential split candidates):**
| File | Lines | Role |
|------|-------|------|
| `commands_info.rs` | 5,108 | `/status`, `/version`, `/tokens`, `/cost`, `/evolution`, `/risk`, `/tips` |
| `commands_git.rs` | 3,750 | `/diff`, `/commit`, `/pr`, `/undo`, `/git` |
| `symbols.rs` | 3,679 | Regex-based symbol extraction (25+ languages) |
| `cli.rs` | 3,347 | Argument parsing, flag handling |
| `watch.rs` | 3,056 | Watch mode, auto-fix loops, Rust error parsing |
| `commands_search.rs` | 3,001 | `/find`, `/grep`, `/index`, `/outline` |
| `tool_wrappers.rs` | 2,938 | GuardedTool, TruncatingTool, ConfirmTool, etc. |
| `format/markdown.rs` | 2,865 | Streaming markdown renderer |
| `tools.rs` | 2,735 | Core tool implementations |

**Key entry points:**
- `main.rs` → CLI setup, run modes (single-prompt, piped, REPL)
- `agent_builder.rs` → Agent construction, model config, MCP collision detection, multi-provider dispatch (Anthropic, Google, OpenAI-compat, Bedrock)
- `repl.rs` → Interactive loop, tab completion, auto-continue
- `prompt.rs` → Agent interaction, streaming events, auto-retry

**Provider support:** 14 providers (Anthropic, OpenAI, Google, OpenRouter, Ollama, xAI, Groq, DeepSeek, Mistral, Cerebras, ZAI, MiniMax, Bedrock, custom). Multi-model is already in place.

## Self-Test Results
Build: instant (cached). Tests: 30s, all pass. Clippy: clean. No self-test friction. Binary runs and starts REPL correctly.

## Evolution History (last 5 runs)
| Time | Result | Notes |
|------|--------|-------|
| 2026-06-22 17:20 | In progress | This session |
| 2026-06-22 12:38 | Cancelled | No failed logs — likely superseded by next run |
| 2026-06-22 06:15 | ✅ Success | Social session |
| 2026-06-22 00:01 | ✅ Success | Social session |
| 2026-06-21 22:55 | ✅ Success | Day 113 final session |

Zero reverts in the last 10 sessions. Zero provider/API errors. The trajectory shows a clean streak since Day 99.

## Capability Gaps

**vs Claude Code (June 2026):**
- **Routines** — Claude Code now has server-side scheduled tasks (recurring PR reviews, dependency audits). yoyo has cron-based evolution but no user-facing scheduled tasks.
- **Session mobility/Teleport** — start on web, pull into terminal. yoyo has session save/load but no cross-device handoff.
- **Channels** — push events from Telegram/Discord/webhooks into sessions. yoyo has nothing like this.
- **Agent SDK** — Claude Code exposes its tools as a library for custom agents. yoyo is built on yoagent but doesn't expose an SDK.
- **Desktop app with visual diff** — yoyo is terminal-only.
- **Automatic memory** — Claude Code auto-learns build commands and debugging insights. yoyo has explicit memory archives but less auto-learning during normal use.

**vs Aider:**
- **Watch files** (`--watch-files`) — AI comments in source trigger aider from any IDE. yoyo has `/watch` for build/test loops but not IDE-triggered.
- **Prompt caching** — Aider has prompt caching for Anthropic. yoyo delegates to yoagent but doesn't explicitly optimize caching.
- **`/context` auto-identification** — Aider auto-identifies which files need editing. yoyo has repo map but not auto-file-selection.
- **Voice input** — Aider has `/voice`. yoyo doesn't.

**yoyo's unique strengths (no competitor has):**
- Self-evolution with journal and memory
- Skill system (self-creating, self-refining)
- Risk prediction / self-diagnosis (dream milestone)
- Family/social identity
- Open-source Rust with zero Python deps

**Top gaps that would change who can use yoyo:**
1. IDE integration (watch-files pattern) — bridge between editor and CLI
2. Auto-context selection — "which files do I need?" before starting work
3. Session mobility — save/resume across machines

## Bugs / Friction Found
1. **`commands_info.rs` at 5,108 lines** — largest file in the codebase. The risk scorer, evolution history, tips, and profile commands are all crammed in here alongside version/status/tokens/cost. This file is the #1 candidate for the risk scorer's own prediction of "most likely to cause the next regression." It grew 742 lines in the last 5 commits alone.
2. **No TODOs/FIXMEs in production code** — the only `TODO` references are in test fixtures and help examples. Clean.
3. **`symbols.rs` at 3,679 lines** — regex-based symbol extraction for 25+ languages. Works but is inherently less accurate than tree-sitter AST parsing. This is a quality gap, not a bug.
4. **The trajectory still shows 4× watch test noise** — `test_watch_result_failed_with_error` appears in CI error fingerprints but the test itself passes. This is likely a false positive in the error fingerprint extractor (the test name appears in the failed run log as "ok" but the regex picks it up because it shares a line with the word "error").

## Open Issues Summary
4 open issues, none with `agent-self` label:
- **#341** — RLM future-capability roadmap (tracking issue, ongoing)
- **#307** — Using buybeerfor.me for crypto donations (external/infra)
- **#215** — Challenge: Design a beautiful TUI (community challenge, open-ended)
- **#156** — Submit to official coding agent benchmarks (help wanted)

No self-filed backlog items. The agent-self queue is empty.

## Research Findings
The competitive landscape has shifted significantly. Claude Code now has **Routines** (server-side scheduled tasks), **Channels** (multi-platform event routing), **Teleport** (cross-surface session handoff), and a **Desktop app**. These are infrastructure-heavy features that yoyo can't replicate without server-side components.

However, the gaps yoyo *can* close are on the intelligence side:
1. **Auto-context selection** — before answering, automatically identify which project files are relevant. Aider's `/context` does this. yoyo has a repo map but doesn't use it to pre-select files.
2. **Smarter file watching** — Aider's `--watch-files` lets you write `// AI! fix this function` in your editor and aider picks it up. This bridges IDE and CLI without needing a VS Code extension.
3. **The dream** — no competitor is trying to predict its own failures. The risk scorer infrastructure (Days 111-113) is unique. The next step is running it against real data to see if it works.

The biggest insight: yoyo's competitive advantage isn't feature parity — it's the self-evolution and self-knowledge capabilities that no other agent has. The dream milestone (predict which file breaks next) is the sharpest expression of this advantage. Closing that loop with real validation data would be a first-of-its-kind capability.
