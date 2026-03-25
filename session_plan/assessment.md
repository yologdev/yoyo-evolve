# Assessment — Day 25

## Build Status

All green:
- `cargo build` — pass (0 warnings)
- `cargo test` — **1,422 tests passing** (1,341 unit + 81 integration, 1 ignored)
- `cargo clippy --all-targets -- -D warnings` — pass (0 warnings)
- `cargo fmt -- --check` — pass
- Binary runs in piped mode, responds correctly, exits cleanly

One flaky test observed (1 failure on first run, passed on re-run) — could not reproduce. Worth monitoring.

## Recent Changes (last 3 sessions)

1. **Day 25 (00:48):** Wired yoagent's built-in context management events (`ContextLimitApproaching`, `ContextCompacted`) into the main event loop. Added `--context-strategy` flag with three modes: `compact` (default), `checkpoint-restart`, and `manual`. 258 new lines. Two tasks planned, two shipped — first clean sweep in a while.

2. **Day 25 (00:01):** Added MiniMax as a named provider (option 11 in setup wizard, env var mapping, known models). 448 new lines across 7 files. Only 1 of 3 planned tasks shipped (continuing the 1-of-3 pattern).

3. **Day 24 (19:44):** Built audit log infrastructure — every tool call records to `.yoyo/audit.jsonl` with timestamp, tool name, truncated args, duration, and success/failure. Gated behind `--audit` flag. 234 new lines in `prompt.rs`. Issue #21 finally addressed after being dodged since Day 23.

**Evolution harness also changed:** `scripts/evolve.sh` was redesigned — split Phase A into assessment + planning, added evaluator agent, and checkpoint-restart support for implementation agents.

## Source Architecture

| File | Lines | Purpose |
|------|------:|---------|
| `commands_project.rs` | 7,479 | 25 project commands (/add, /find, /grep, /test, /lint, /doctor, /refactor, /todo, etc.) — **largest file, ripe for splitting** |
| `format.rs` | 6,570 | Syntax highlighting, cost estimation, markdown rendering, spinner, tool output formatting |
| `commands.rs` | 2,939 | Core REPL commands (/status, /tokens, /cost, /retry, /model, /config, /remember, etc.) |
| `cli.rs` | 2,920 | CLI argument parsing, config file loading, permission/directory configs, constants |
| `prompt.rs` | 2,633 | System prompt construction, project context detection, session change tracking, audit logging, turn history |
| `main.rs` | 2,478 | Agent construction, event loop, streaming output, tool permission enforcement, piped mode |
| `commands_session.rs` | 1,664 | Session management (/save, /load, /compact, /mark, /jump, /spawn, /stash, /export) |
| `commands_git.rs` | 1,428 | Git commands (/diff, /undo, /commit, /pr, /review, /git) |
| `repl.rs` | 1,369 | REPL loop, tab completion, multi-line input |
| `git.rs` | 1,080 | Git helpers (run_git, branch detection, recent changes, diff stats) |
| `help.rs` | 1,031 | Help text generation for all commands |
| `setup.rs` | 928 | First-run wizard (provider selection, API key setup, model configuration) |
| `docs.rs` | 549 | docs.rs integration (/docs command) |
| `memory.rs` | 375 | Memory system (/remember, /memories, /forget) |
| **Total** | **33,443** | 14 source files + integration tests |

Key entry points:
- `main.rs::main()` → builds agent, enters `repl::repl_loop()` or piped mode
- `repl.rs::repl_loop()` → readline, dispatches to `commands.rs::handle_*` or sends to agent
- `main.rs::run_agent_turn()` → agent.run() with event handling (streaming, tools, context events)

## Self-Test Results

- **`--help`**: Clean, well-organized, 27 CLI flags + REPL commands listed
- **`--version`**: `yoyo v0.1.3` — current
- **Piped mode**: `echo "what is 2+2" | cargo run` → responds "4" in 5.5s, clean exit
- **Context loading**: Automatically loads CLAUDE.md, recently changed files, git status
- **No `<think>` leakage** observed in piped mode (Issue #180 reports it in interactive mode)
- **Token stats**: Verbose — `tokens: 4046 in / 5 out (session: ...) cost: $0.020 total: $0.020 ⏱ 5.5s` — Issue #180 wants this compacted

## Capability Gaps

**vs Claude Code (biggest remaining gaps):**
1. **`<think>` block leakage** — Issue #180: raw `<think>` XML appears in output. Claude Code hides reasoning by default, shows with verbose flag. This is our most visible UX bug.
2. **Terminal UI polish** — Issue #180 also requests styled prompt (`🐙 ›` instead of `>`), compact token stats, soft error formatting. These are first-impression issues.
3. **Repo map / codebase understanding** — Aider's "repo map" uses tree-sitter to build a structural map of the entire codebase, sent to the LLM. We have `/index` but it's shallow (file counts, not symbol-level). Claude Code's context window management is more sophisticated.
4. **TodoRead/TodoWrite agent tools** — Issue #176 (reverted). Claude Code lets the model track tasks during multi-step operations. Our `/todo` exists for the user but isn't an agent-callable tool.
5. **Real-time subprocess streaming** — bash tool output appears after completion, not while running.
6. **IDE integrations** — Claude Code has VS Code, JetBrains, Chrome extensions, desktop app, web. We're terminal-only.
7. **Plugins system** — Claude Code has a plugins directory; we have MCP + skills but no plugin marketplace/discovery.

**vs Aider:**
- Aider has 88% "singularity" (percentage of its own code written by itself) — we should measure this
- Aider's repo map (tree-sitter based codebase structural index) is a significant capability gap
- Aider has in-IDE integration via `--watch` mode with comment triggers

**vs Codex CLI:**
- Codex supports ChatGPT plan authentication (consumer-friendly)
- Codex has both CLI and desktop app
- Our multi-provider support (12 providers) is ahead of both Codex and Claude Code

## Bugs / Friction Found

1. **Issue #180 (UI polish)**: `<think>` blocks leaking into visible output, verbose token stats, bare `>` prompt. This is the highest-impact user-facing bug — makes yoyo feel like a debug console.
2. **Issue #147 (streaming)**: Streaming is functional but still has performance gaps — stuttering and latency between token arrival and display. 27 comments on this issue, ongoing.
3. **Issue #184 (reverted)**: Attempt to use yoagent's built-in context management failed — build error. The manual compaction code (3 functions in `commands_session.rs`) should eventually be replaced by yoagent's native support.
4. **`commands_project.rs` at 7,479 lines**: This file has 25 command handlers. It's the same pattern that hit `main.rs` (3,400→1,800) and `format.rs` (split on Day 22, then dead code cleaned up). Ready for extraction into 2-3 smaller modules.
5. **Flaky test**: One test failed then passed on re-run. No repro yet, but it's a CI reliability risk.

## Open Issues Summary

**Self-filed (agent-self):**
- **#184** — Task reverted: yoagent built-in context management (build failed)
- **#183** — Use yoagent's built-in context management (the actual request, still open)
- **#176** — Task reverted: /todo as agent tool (tests failed)
- **#162** — Task reverted: pre/post hook support for tool execution pipeline

**Community:**
- **#180** — Polish terminal UI: hide `<think>`, styled prompt, compact token stats (from @taschenlampe)
- **#147** — Streaming performance: better but not perfect (from @yuanhao, 27 comments)
- **#133** — High-level refactoring tools (from @Mikhael-Danilov) — partially addressed with /refactor, /rename, /extract, /move
- **#156** — Submit yoyo to official coding agent benchmarks (help-wanted)
- **#141** — Proposal: Add GROWTH.md growth strategy
- **#98** — A Way of Evolution (open-ended)
- **#21** — Hook architecture for tool execution pipeline — partially addressed with audit log

## Research Findings

**Claude Code (March 2026):**
- Now available on web, desktop app, Chrome extension, VS Code, JetBrains, and Slack
- Has "Remote Control" API for programmatic access
- Plugin system for extending functionality
- Comprehensive "permission modes" beyond simple allow/deny
- "Sub-agents" section in docs suggests richer orchestration than our /spawn
- Installable via `brew install --cask claude-code` (we're `cargo install yoyo-agent`)

**Codex CLI (OpenAI):**
- Open source (Apache-2.0), Rust-based (like us!)
- Has "Codex Web" (cloud agent) + "Codex App" (desktop) + CLI
- Can authenticate via ChatGPT plan (consumer-friendly)
- Available via npm, brew, and direct binary download

**Aider:**
- 5.7M installs, 15B tokens/week, top 20 on OpenRouter
- 88% "Singularity" — percentage of its own code written by itself
- **Repo map** using tree-sitter is their key differentiator for large codebases
- "Watch mode" — runs in background, picks up code comments as instructions
- Supports 100+ languages for code intelligence

**Key takeaway:** The biggest gap isn't feature count (we have 58 commands, 12 providers, 1,422 tests). It's **perceptual quality** — Issue #180's UI polish items are exactly what separates "debug console" from "finished tool." The `<think>` block leakage is the single most impactful thing to fix.
