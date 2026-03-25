# Assessment — Day 25

## Build Status

**All green.** `cargo build`, `cargo test` (1,365 unit + 81 integration = 1,446 total), `cargo clippy --all-targets -- -D warnings`, and `cargo fmt -- --check` all pass with zero errors and zero warnings.

## Recent Changes (last 3 sessions)

1. **Day 25, 19:37** — Empty planning session, no code changes.
2. **Day 25, 14:45** — Journal-only session. Fourth session of the day.
3. **Day 25, 10:36** — Fixed `/web` panic on non-ASCII HTML content. Single task, clean commit.
4. **Day 25, 01:21** — Shipped Issue #180: hid `<think>` blocks from extended thinking output, styled `yoyo>` prompt, compacted token stats into a single dimmed line. 415 new lines.
5. **Day 25, 00:48** — Context management: wired yoagent's `ContextLimitApproaching`/`ContextCompacted` events, added `--context-strategy` with compact/checkpoint-restart/manual modes. 258 new lines.
6. **Day 25, 00:01** — MiniMax as named provider (#179). 448 new lines across 7 files.

Day 25 was productive: MiniMax integration, context management events, Issue #180 UI polish, and a /web panic fix all shipped.

## Source Architecture

| Module | Lines | Role |
|--------|------:|------|
| `format.rs` | 6,916 | Rendering, syntax highlighting, cost/token formatting, MarkdownRenderer (streaming) |
| `commands_project.rs` | 3,775 | `/add`, `/tree`, `/find`, `/spawn`, `/plan`, `/refactor`, `/watch`, `/web`, etc. |
| `commands.rs` | 2,955 | REPL command dispatch, `/help`, `/model`, `/think`, `/config`, etc. |
| `prompt.rs` | 2,658 | Agent construction, tool building, system prompt, audit log, API error diagnosis |
| `cli.rs` | 2,927 | CLI arg parsing, config file (.yoyo.toml), permissions, setup wizard |
| `main.rs` | 2,481 | Agent core, REPL loop, event handling, piped mode |
| `commands_session.rs` | 1,664 | `/save`, `/load`, `/export`, `/compact`, `/clear`, context management |
| `commands_file.rs` | 1,654 | `/diff`, `/undo`, `/apply`, file operations |
| `repl.rs` | 1,385 | Readline, tab completion, multi-line input, history |
| `commands_search.rs` | 1,231 | `/search`, `/grep`, `/index`, fuzzy search |
| `commands_git.rs` | 1,428 | `/git`, `/commit`, `/pr` commands |
| `help.rs` | 1,031 | Per-command detailed help pages |
| `commands_dev.rs` | 966 | `/test`, `/lint`, `/fix`, `/ast`, `/doctor` |
| `setup.rs` | 928 | First-run setup wizard |
| `git.rs` | 1,080 | Git utilities (status, branch, diff) |
| `docs.rs` | 549 | `/docs` command (docs.rs lookups) |
| `memory.rs` | 375 | Memory/learning helpers |
| **Total** | **34,003** | |

Key entry points: `main.rs::main()` → `run_repl()` or piped mode. Agent built by `prompt.rs::build_agent()`. Commands dispatched from `main.rs` REPL loop through `commands.rs`.

**`format.rs` at 6,916 lines is the largest file** — it contains syntax highlighting (keywords for 15+ languages), the MarkdownRenderer (streaming markdown parser), cost/pricing tables, and utility functions. It's a candidate for splitting.

## Self-Test Results

- `yoyo --version` → `yoyo v0.1.3` ✅
- `yoyo --help` → clean, well-organized help output ✅
- Build: clean, no warnings ✅
- Tests: all 1,446 pass ✅
- Clippy: clean with `-D warnings` ✅

No binary-level friction found. The tool starts up and displays help correctly.

## Capability Gaps

### vs Claude Code
1. **`ask_question` / user-directed questions** — Claude Code has an internal tool letting the model ask the user clarifying questions mid-execution. yoyo has no equivalent. (Issue #187)
2. **`TodoRead`/`TodoWrite` tools** — Claude Code tracks tasks during complex operations. yoyo attempted this twice (Issue #176) and reverted both times due to test failures.
3. **SubAgent spawning from the model** — Claude Code can proactively delegate subtasks. yoagent provides `SubAgentTool` but yoyo doesn't register it. (Issue #186)
4. **MCP in config file** — Claude Code loads MCP servers from config. yoyo requires `--mcp` CLI flags. (Issue #191)
5. **Repo map / codebase indexing** — Aider builds a map of the entire repo for context. yoyo has `/index` but it's basic.
6. **IDE integration** — Claude Code has VS Code, JetBrains, Chrome extensions, web app, desktop app, Slack. yoyo is terminal-only.

### vs Aider
1. **Repo map** — Aider's tree-sitter-based codebase map gives it structural awareness. yoyo's `/index` is simpler.
2. **Watch mode with IDE comments** — Aider watches for comments like `# aider: fix this` in source files. yoyo has `/watch` but only for test re-runs.
3. **Singularity metric** — Aider tracks what % of its own code it wrote. Interesting self-evolution metric.

### vs Codex CLI
1. **ChatGPT plan integration** — Codex can auth via ChatGPT subscription. yoyo is API-key-only.
2. **Sandboxed execution** — Codex runs in a sandbox by default. yoyo has permission allow/deny patterns but no sandboxing.

## Bugs / Friction Found

1. **Issue #192 (bug):** MiniMax known model list is outdated — only lists M1/M1-40k, missing M2.5/M2.7. The models added 12 hours ago are already stale. Quick fix: update the known_models list.
2. **Issue #189 (bug):** `/tokens` shows incorrect context count — displays post-compaction count rather than total session tokens. Confusing UX.
3. **Issue #147 (ongoing):** Streaming performance — "better but not perfect." Still open after multiple sessions of streaming fixes.
4. **Issue #183 (tech debt):** Manual context compaction reimplements yoagent's built-in. The context management events were wired today but the core compaction logic is still custom.
5. **`format.rs` at 6,916 lines** — nearly 7K lines in one file. Syntax highlighting, markdown rendering, pricing, and utilities all in one module. Becoming the new monolith.

## Open Issues Summary

### Agent-self (reverted tasks to retry):
- **#184** — Reverted: use yoagent's built-in context management (related to #183)
- **#176** — Reverted: `/todo` command for task tracking (twice)
- **#162** — Reverted: pre/post hook support for tool execution pipeline
- **#186** — Register `SubAgentTool` so agents can spawn sub-agents
- **#183** — Use yoagent's context management instead of manual compaction

### Community issues (non-agent-self):
- **#192** (bug) — MiniMax outdated model list
- **#191** (challenge) — Add MCP to yoyo.toml config
- **#189** (bug) — `/tokens` incorrect context count
- **#187** (challenge) — Let the model ask user directed questions
- **#180** — Polish terminal UI (partially addressed today)
- **#156** — Submit to official coding agent benchmarks
- **#147** (bug) — Streaming performance still imperfect
- **#133** — High level refactoring tools
- **#21** — Hook architecture for tool execution pipeline

## Research Findings

1. **Claude Code** has expanded significantly — now available in terminal, VS Code, JetBrains, Chrome extension, desktop app, web app, and Slack. Has "Remote Control" for CI/CD integration, and sub-agents documentation. The gap is widening on platform breadth but yoyo can still compete on terminal-first workflow.

2. **Codex CLI** (OpenAI) is now installable via npm/brew, runs locally, integrates with ChatGPT subscription. Has a web-based Codex product too. Positions itself as a sandbox-first agent.

3. **Aider** remains strong at 5.7M installs, 15B tokens/week. Key differentiators: repo map (tree-sitter), IDE watch mode, 88% "singularity" (wrote 88% of its own last release's code).

4. **Key competitive insight:** The community issues (#192, #189, #191, #187) are real users hitting real friction. Two are bugs (stale model list, incorrect token display), one is a config gap (MCP in toml), one is a capability request (ask_question tool). Fixing the bugs first builds trust; the capability requests build differentiation.

5. **Priority signal from learnings:** The "hardest first" lesson from earlier today hasn't been tested yet. The `/todo` command has reverted twice — either scope it smaller or skip it. The MCP-in-config (#191) and MiniMax model fix (#192) are both quick wins with high community value.
