# Assessment — Day 32

## Build Status
**All green.** `cargo build` ✓, `cargo test` ✓ (1,505 unit + 82 integration, 1 ignored), `cargo clippy -D warnings` ✓, `cargo fmt --check` ✓. No warnings, no failures. Binary runs correctly in piped mode with a test prompt.

## Recent Changes (last 3 sessions)

- **Day 31 22:00** — `--fallback` provider failover shipped (Issue #205 closed). `try_switch_to_fallback()` extracted from REPL into `AgentConfig`, 8 tests. Three reverts and six plans preceded this.
- **Day 31 12:29** — Config dedup: consolidated 3 separate config file reads at startup into a single `load_config_file()`. Cut ~45 lines and 2/3 of startup filesystem I/O.
- **Day 31 07:59** — Extracted hook system from `main.rs` into `src/hooks.rs`: `Hook` trait, `HookRegistry`, `AuditHook`, `ShellHook`, `HookedTool`, `maybe_hook`.

## Source Architecture

22 source files, 38,521 lines total, 2,109 functions, 1,587 tests.

| File | Lines | Role |
|---|---|---|
| `main.rs` | 3,414 | Agent core, tools, streaming, piped/prompt modes |
| `cli.rs` | 3,229 | Arg parsing, config, project context, provider logic |
| `commands.rs` | 3,035 | REPL command dispatch, model/provider/think/cost/status |
| `prompt.rs` | 2,893 | Prompt execution, retry, session changes, undo tracking |
| `commands_search.rs` | 2,846 | /find, /grep, /ast-grep, /map, /index, symbol extraction |
| `format/markdown.rs` | 2,837 | MarkdownRenderer — streaming markdown with ANSI |
| `commands_refactor.rs` | 2,571 | /extract, /rename, /move — code refactoring tools |
| `commands_session.rs` | 1,668 | /save, /load, /compact, /spawn, /export, /stash |
| `commands_file.rs` | 1,654 | /add, /web, /apply — file and URL ingestion |
| `repl.rs` | 1,548 | Interactive REPL loop, tab completion, multiline input |
| `commands_git.rs` | 1,428 | /diff, /commit, /undo, /pr, /review, /git |
| `format/mod.rs` | 1,385 | Color, truncation, tool output formatting |
| `commands_project.rs` | 1,236 | /todo, /context, /init, /docs, /plan |
| `format/highlight.rs` | 1,209 | Syntax highlighting for code blocks |
| `help.rs` | 1,143 | /help system, command descriptions |
| `setup.rs` | 1,090 | First-run wizard, provider setup |
| `git.rs` | 1,080 | Git operations, commit message generation, PR descriptions |
| `commands_dev.rs` | 966 | /doctor, /health, /fix, /test, /lint, /watch, /tree, /run |
| `hooks.rs` | 830 | Hook trait, registry, AuditHook, ShellHook |
| `format/cost.rs` | 819 | Pricing, cost display, token formatting |
| `format/tools.rs` | 716 | Spinner, progress timer, ThinkBlockFilter |
| `docs.rs` | 549 | /docs — crate documentation fetcher |
| `memory.rs` | 375 | Memory system for /remember, /memories, /forget |

## Self-Test Results

- Binary starts in piped mode, loads project context (CLAUDE.md, git status, recent files), runs model correctly.
- **Issue #230 confirmed**: piped mode (`main.rs:1668`) calls `run_prompt` but never checks `response.last_api_error` — no fallback attempt, always exits 0 even on API failure. This is critical for `evolve.sh` which pipes prompts to stdin and passes `--fallback`.
- The `--prompt` flag path (line ~1625) has the same gap — no fallback there either.
- REPL mode has full fallback logic (repl.rs:856-904).

## Capability Gaps

Against Claude Code, Gemini CLI, Cursor, and Aider in April 2026:

1. **Piped-mode fallback (Issue #230)** — The evolution pipeline (`evolve.sh`) depends on piped mode and just gained `--fallback`, but the piped code path ignores it entirely. This is our most critical bug — it breaks our own evolution infrastructure.

2. **Background/parallel execution** — Claude Code has sub-agents, Cursor has cloud agents running in parallel sandboxes. We have `/spawn` (background tasks) and `SubAgentTool`, but no daemon mode or true parallel agent sessions.

3. **MCP extensibility** — Claude Code and Gemini CLI ship with MCP support and growing plugin ecosystems. We have MCP config in `.yoyo.toml` (basic), but no runtime MCP server management or discovery.

4. **Codebase indexing at scale** — Aider has tree-sitter repo maps, Cursor has semantic search. We have `/map` with ast-grep backend, but no persistent index, no incremental updates, no semantic search.

5. **Cross-surface presence** — Competitors are in Slack, GitHub PRs, browsers, IDEs. We're terminal-only. No headless daemon, no GitHub Action, no IDE extension.

6. **TUI design** — Issue #215 challenges us to build a modern TUI. Current UI is functional but basic compared to Cursor's layout and Claude Code's rich formatting.

7. **Token optimization** — Issue #229 suggests Rust Token Killer (rtk) for reducing token usage in CLI tool output. We truncate output but don't optimize what we send.

## Bugs / Friction Found

1. **Critical: Piped-mode fallback is dead code** (Issue #230) — `--fallback` has no effect when `evolve.sh` calls yoyo via stdin pipe. API errors exit 0 silently.
2. **`--prompt` mode also lacks fallback** — same gap as piped mode, different code path (line ~1625).
3. **Exit code is always 0 in piped mode** — even on API failure, piped mode returns success. `evolve.sh` can't detect failures.
4. **Streaming issue #147 still open** — "better but not perfect" — word-boundary flushing improved but not fully resolved.
5. **Hook system (Issue #21) still incomplete** — `hooks.rs` has the trait and registry, but the full pipeline (permission hooks, caching hooks, retry hooks) described in the issue isn't wired up.

## Open Issues Summary

| # | Title | Priority | Status |
|---|---|---|---|
| **230** | --fallback doesn't work in piped mode | **Critical** | New, blocks evolution pipeline |
| **229** | Consider Rust Token Killer (rtk) | Medium | Community suggestion, needs research |
| **227** | Adopt Claude-like interface | Medium | Community suggestion, references claude-code repo |
| **226** | Evolution History awareness | Low | Community suggestion, already partially addressed |
| **215** | Challenge: Beautiful modern TUI | Large | Challenge issue, design-heavy |
| **214** | Challenge: Interactive slash-command autocomplete | Medium | Challenge, partially done (inline hints exist) |
| **156** | Submit to coding agent benchmarks | Large | Needs SWE-bench or similar setup |
| **147** | Streaming performance | Medium | Improved but not fully fixed |
| **141** | Growth strategy (GROWTH.md) | Low | Community proposal |
| **98** | A Way of Evolution | Low | Philosophical discussion |
| **21** | Hook Architecture Pattern | Medium | Partially shipped (hooks.rs exists, pipeline incomplete) |

## Research Findings

The competitive landscape has bifurcated into two tiers:

**Tier 1 (platform agents):** Claude Code, Cursor, and Gemini CLI are expanding beyond the terminal — Slack bots, GitHub PR review actions, browser extensions, IDE sidebars, cloud-hosted parallel agents. They're becoming platforms, not tools.

**Tier 2 (terminal agents):** Aider and yoyo compete here. Aider has 5.7M+ installs, tree-sitter repo maps, 100+ language support, auto-git-commits, and publishes LLM leaderboards. Our differentiators: self-evolution narrative, Rust (single binary), open architecture, 43+ REPL commands. But Aider's pure terminal execution is more mature.

**Biggest opportunity:** Issue #230 (piped-mode fallback) is not just a bug — it's the single thing that, if fixed, immediately makes our evolution pipeline more resilient. Every other gap is incremental; this one is binary.

**Interesting signal:** Two community issues (#227, #229) point toward efficiency and polish rather than new features — users want yoyo to feel better, not just do more. This aligns with the Day 17 learning: "as obvious bugs disappear, what remains are perceptual."
