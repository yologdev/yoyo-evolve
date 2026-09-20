# Gap Analysis: yoyo vs Claude Code

**This file is a stale snapshot, not a current assessment.** Its verification date is
`Day 74` — **as of Day 204 (2026-09-20) that is 130 days ago**. Every row below was last
read on Day 74 and is **unverified until re-read**: the ✅/❌ marks describe yoyo as it was
on 2026-05-13, not as it is now, and a great deal of work has landed since. Re-read a row
(and the code it names) before you act on it. Nothing in `scripts/`, `src/` or `tests/`
re-derives this header for you — with one mechanical exception, added Day 204:
`render_doc_freshness` in `scripts/extract_trajectory.py` reads the `Last verified:` line
below and prints the age into the planner's `## Doc freshness` section each session.

**Deliberately still outstanding:** the body below has *not* been re-verified since Day 74.
Refreshing it is a project in its own right, not a line edit, so it is left honestly stale
rather than half-refreshed — a half-refreshed gap analysis is worse than a dated one.

Last verified: Day 74 (2026-05-13)
Last updated: Day 24 (2026-03-24) — major refresh on Day 38, stats refresh on Day 50, Day 54, Day 59, Day 61, Day 63, Day 64, Day 67, Day 74

This document tracks the feature gap between yoyo and Claude Code, used to inform
development priorities when there are no community issues to address. It is a
**snapshot**, not a TODO list — the priority queue at the bottom names the real
remaining gaps, but task selection still happens through the normal planning loop.

## Legend
- ✅ **Implemented** — yoyo has this
- 🟡 **Partial** — yoyo has a basic version, Claude Code's is better
- ❌ **Missing** — yoyo doesn't have this yet

---

## Core Agent Loop

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Streaming text output | ✅ | ✅ | True token-by-token streaming — mid-line tokens render immediately, line-start briefly buffers for fence/header detection (Day 17, fixed line-buffering bug); streaming flush improvements (Day 23) |
| Tool execution | ✅ | ✅ | bash (with per-command timeout), read_file, write_file, edit_file, search, list_files, rename_symbol, ask_user, todo |
| Multi-turn conversation | ✅ | ✅ | Both maintain conversation history |
| Thinking/reasoning display | ✅ | ✅ | yoyo shows thinking dimmed; --thinking flag controls budget |
| Error recovery / auto-retry | ✅ | ✅ | yoagent retries 3x with exponential backoff by default; tool-specific recovery hints escalate on repeated failures (Day 70) |
| Auto-continue | ✅ | ✅ | Automatic follow-up on incomplete responses — `looks_incomplete` heuristic detects unclosed code blocks, numbered steps, continuation phrases; up to 5 auto-continues (configurable via `max_auto_continues` in `.yoyo.toml`) (Day 73) |
| Subagent / task spawning | 🟡 | ✅ | `/spawn` runs tasks in separate context; yoagent's `SubAgentTool` exposes subagents as tools; `SharedState` key-value store for parent↔child data sharing (Day 58); no named-role persistent orchestration yet |
| Tool output streaming | ✅ | ✅ | `ToolExecutionUpdate` events handled and rendered live (line counts, partial tail); real-time per-line subprocess streaming via `on_progress` (Day 62) — stdout lines print as they arrive, stderr lines prefixed with `stderr:` |
| Background processes | ✅ | ✅ | `/bg` command (Day 45): launch, list, view output, kill background jobs with persistent tracker; Claude Code has similar with `/bashes` |

## CLI & UX

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Interactive REPL | ✅ | ✅ | |
| Piped/stdin mode | ✅ | ✅ | Improved piped mode handling (Day 23) |
| Single-shot prompt (-p) | ✅ | ✅ | |
| Output to file (-o) | ✅ | ✅ | |
| Model selection | ✅ | ✅ | --model flag and /model command |
| Session save/load | ✅ | ✅ | /save, /load, --continue, /history |
| Git integration | ✅ | ✅ | Branch in prompt, /diff, /undo, /commit (with co-authored-by trailer), /pr; git-aware system prompt gives agent branch/dirty state automatically |
| Readline / line editing | ✅ | ✅ | rustyline: arrow keys, history (~/.local/share/yoyo/history), Ctrl-A/E/K/W |
| Tab completion | ✅ | ✅ | Slash commands, file paths, and argument-aware completion (--model values, git subcommands, /pr subcommands) (Day 14) |
| Fuzzy file search | ✅ | ✅ | `/find` with scoring, git-aware file listing, top-10 ranked results (Day 12) |
| Syntax highlighting | ✅ | ✅ | Language-aware ANSI highlighting for Rust, Python, JS/TS, Go, Shell, C/C++, JSON, YAML, TOML |
| Markdown rendering | ✅ | ✅ | Incremental ANSI: headers, bold, code blocks, inline code, syntax-highlighted code blocks |
| Progress indicators | ✅ | ✅ | Braille spinner animation during AI responses (Day 8); per-tool live progress timer |
| Multi-line input | ✅ | ✅ | Backslash continuation and code fences |
| Image input support | ✅ | ✅ | `/add` reads images as base64; `--image` flag for CLI; auto-detects png/jpg/gif/webp/bmp (v0.1.1) |
| Custom system prompts | ✅ | ✅ | --system, --system-file, plus config file `system_prompt`/`system_file` keys (Day 23) |
| Extended thinking control | ✅ | ✅ | --thinking flag |
| Color control | ✅ | ✅ | --no-color, NO_COLOR env |
| Edit diff display | ✅ | ✅ | Colored inline diffs for `edit_file` tool output — red/green removed/added lines (Day 14) |
| Inline @file mentions | ✅ | ✅ | `@path` in prompts expands to file contents; supports line ranges `@file:10-20` and images (Day 21) |
| Conversation bookmarks | ✅ | ❌ | `/mark`, `/jump`, `/marks` — name points in conversation and jump back (Day 14) |
| First-run onboarding | ✅ | ✅ | Detects first run, shows welcome message, guides API key and model configuration (Day 22) |
| Terminal bell notifications | ✅ | ✅ | Bell on long completions; --no-bell flag and YOYO_NO_BELL env to disable (Day 23) |
| Conversation stash | ✅ | ❌ | `/stash` saves/restores conversation context without files (Day 22) |
| File patch application | ✅ | ❌ | `/apply` applies unified diff patches to files (Day 23) |
| AST structural search | ✅ | ❌ | `/ast` searches code by structure using tree-sitter patterns (Day 23) |
| Auto-test watcher | ✅ | ❌ | `/watch` auto-runs tests on file changes (Day 23) |
| Clipboard integration | ✅ | ✅ | `/copy` copies last response or code block to system clipboard — auto-detects pbcopy/xclip/wl-copy/clip.exe (Day 71) |
| Desktop notifications | ✅ | ✅ | Notify on long completions (>10s); `--no-notify` / `YOYO_NO_NOTIFY` to disable (Day 71) |
| Output speed display | ✅ | ✅ | Tokens/sec shown after each turn for real-time performance visibility (Day 73) |
| Write file diff preview | ✅ | ✅ | `write_file` tool now shows colored red/green diff before overwriting — parity with `edit_file` diff display (Day 73) |
| Refactoring umbrella | ✅ | ❌ | `/refactor` with subcommands: rename, extract, move (Day 23) |

## Context Management

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Proactive context compaction | ✅ | ✅ | Proactive at 70% + auto-compact at 80% context (Day 23, upgraded from auto-only) |
| Manual compaction | ✅ | ✅ | /compact command |
| Token usage display | ✅ | ✅ | /tokens with visual bar; live context-window percentage in prompt |
| Smart context injection | ✅ | ✅ | `/add` with intelligent truncation — files over 500 lines get head+tail with omission marker (Day 56); URL support fetches web content inline (Day 72) |
| Cost estimation | ✅ | ✅ | Per-request and session totals |
| Context window awareness | ✅ | ✅ | Per-model context limit tracked (no longer hardcoded to 200k — #195 fix) |
| Persistent cross-session memory | ✅ | ✅ | yoyo has `memory/` system with JSONL archives + synthesized active context (learnings, social); Codex Chronicle has similar persistent project memory. Different implementation, same capability (Day 67) |

## Permission System

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Tool approval prompts | ✅ | ✅ | `--yes`/`-y` to auto-approve; interactive confirm for bash, write_file, and edit_file; "always" persists per-session (Day 15) |
| Allowlist/blocklist | ✅ | ✅ | `--allow`/`--deny` flags with glob matching; `[permissions]` config section; deny overrides allow (`PermissionConfig` in `src/config.rs`) |
| Directory restrictions | ✅ | ✅ | `--allow-dir`/`--deny-dir` flags + `[directories]` config; canonicalized path checks prevent traversal; sub-agents inherit restrictions (Day 35) (`DirectoryRestrictions` in `src/config.rs`) |
| Auto-approve patterns | ✅ | ✅ | `--allow` glob patterns + config file `allow` array; "always" option during confirm |
| User-configurable hooks | ✅ | ✅ | `[[hooks]]` config blocks for shell hooks on tool calls; `Hook` trait + `HookRegistry` in `src/hooks.rs` (Issue #21, Day 34) |

## Project Understanding

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Project context files | ✅ | ✅ | yoyo reads YOYO.md, CLAUDE.md, and .yoyo/instructions.md (`src/context.rs`) |
| Auto-detect project type | ✅ | ✅ | `detect_project_type` used by `/test`, `/lint`, `/health`, `/fix` (Rust, Node, Python, Go, Make); `/doctor` extended for Java (Maven/Gradle), Ruby, C/C++ (CMake) (Day 73) |
| Project scaffolding | ✅ | ✅ | `/init` scans project and generates a YOYO.md context file (Day 13) |
| Git-aware file selection | ✅ | ✅ | `get_recently_changed_files` appended to project context (Day 12) |
| Git-aware system prompt | ✅ | ✅ | Agent always sees current branch and dirty state in system prompt (Day 23) |
| Codebase indexing | ✅ | ✅ | `/index` builds lightweight project index: file count, language breakdown, key files (Day 14) |
| Repo map for prompt context | ✅ | ✅ | `/map` builds tree-sitter or ast-grep symbol map for the agent |

## Developer Workflow

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Run tests | ✅ | ✅ | `/test` auto-detects project type and runs tests (Day 12) |
| Auto-fix lint errors | ✅ | ✅ | `/lint` auto-detects and runs linter; `/fix` sends failures to AI (Day 9+12) |
| PR description generation | ✅ | ✅ | `/pr create [--draft]` generates AI-powered PR descriptions |
| Commit message generation | ✅ | ✅ | `/commit` with heuristic-based message generation from staged diff (Day 8) |
| Code review | ✅ | ✅ | `/review` provides AI-powered code review of staged/unstaged changes (Day 13); `yoyo review` works non-interactively for CI pipelines — supports commit ranges (`HEAD~3..HEAD`) and PR review (`--pr 42`) (Day 63) |
| Multi-file refactoring | ✅ | ✅ | `/refactor` umbrella command (rename, extract, move); `rename_symbol` agent tool for cross-project renames (Day 23) |
| Architect mode | ✅ | ✅ | `/architect` dual-model mode — cheap model plans, expensive model edits; inspired by Aider's architect mode (Day 59) |
| Iterative prompt loop | ✅ | ❌ | `/loop <N|until-pass> <prompt>` runs a prompt repeatedly, useful for iterative refinement (Day 59) |
| Plan workflow | ✅ | ❌ | `/plan` generates a structured plan, `/plan show` reviews it, `/plan apply` executes — full generate/review/execute workflow (Day 72) |
| Auto-lint-test per edit | ✅ | ✅ | `AutoCheckTool` wrapper runs the first watch phase (typically lint/check) after each write_file/edit_file and appends errors inline; uses existing `/watch set` mechanism (Day 68) |
| Automated PR review agent | 🟡 | ✅ | Cursor BugBot provides event-driven automated PR review; yoyo has `/review` but it's on-demand, not triggered automatically by PR events (Day 67) |

## Configuration

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Config file | ✅ | ✅ | yoyo reads .yoyo.toml and ~/.config/yoyo/config.toml |
| Per-project settings | ✅ | ✅ | .yoyo.toml in project directory |
| MCP server support | ✅ | ✅ | `--mcp` flag + `[[mcp.servers]]` config blocks; `McpServerConfig` + `parse_mcp_servers_from_config` in `src/config.rs`; stdio transport, used in production |
| Multi-provider support | ✅ | ❌ | yoyo supports 25 providers via `--provider` (anthropic, openai, google, ollama, bedrock, z.ai, cerebras, etc.) — `KNOWN_PROVIDERS` in `src/providers.rs` |
| Skills system | ✅ | ✅ | yoyo loads skills via `--skills <dir>` (yoagent's `SkillSet`); `/skill install`, `/skill search`, `/skill create`, `/skill list/show/enable/disable` (Days 60-61) |
| OpenAPI tool support | ✅ | ❌ | `--openapi <spec>` loads OpenAPI specs and registers API tools (Day 9) |
| Config system_prompt/system_file | ✅ | ✅ | `system_prompt` and `system_file` keys in .yoyo.toml for persistent custom prompts (Day 23) |
| Prompt caching | ✅ | ✅ | Anthropic prompt caching enabled — system prompt and early conversation turns cached for ~90% cost reduction on repeated context; cache hit-rate visible in `/cost` (Day 71) |
| Skill install & discovery | 🟡 | ✅ | `/skill install <dir>` (local) + `/skill install gh:user/repo` (remote) + `/skill search <query>` (GitHub discovery) shipped Days 60-61; still missing signed bundles, curation/ratings, formal marketplace with reviews |

## Error Handling

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| API error display | ✅ | ✅ | Shows error messages |
| Network retry | ✅ | ✅ | yoagent handles 3 retries with exponential backoff by default |
| Rate limit handling | ✅ | ✅ | yoagent respects retry-after headers on 429s |
| Context overflow recovery | ✅ | ✅ | Auto-compacts conversation and retries on context overflow errors (Day 20) |
| Provider fallback | ✅ | ❌ | `--fallback` chains providers; auto-switches on hard errors (#205, Day 31) |
| Graceful degradation | 🟡 | ✅ | Retry logic, error handling, context overflow recovery, provider fallback; not yet full fallback on partial tool failures |
| Ctrl+C handling | ✅ | ✅ | Both handle interrupts |

## Deployment & Isolation

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Cloud background agents | ❌ | ✅ | Cursor Cloud Agents run on cloud git worktrees while user works locally; different deployment model (cloud vs CLI) — yoyo is a local CLI tool by design (Day 67) |
| Event-driven triggers / webhooks | ❌ | ✅ | Cursor agents triggered by GitHub events (PR opened, issue filed, etc.); yoyo has cron-based evolution but no event-driven hooks for arbitrary repo events (Day 67) |
| Sandboxed execution | ❌ | ✅ | Codex uses Docker/VM-based tool isolation for safe execution; yoyo runs tools directly in the user's environment (Day 67) |

---

## Priority Queue (real remaining gaps)

After the Day 38 refresh, the gaps that are actually still gaps. Re-evaluated
on Day 74 — two core gaps remain, plus deployment-model gaps and one
skills sub-gap.

1. **Persistent named subagents with orchestration** (since Day ≤38) — yoyo now has
   `/spawn`, yoagent's `SubAgentTool`, AND `SharedState` for parent↔child data
   sharing (Day 58), but still no named-role persistent subagent system (e.g., a
   long-lived "reviewer" or "tester" subagent the orchestrator can delegate to
   repeatedly across turns). SharedState closes the data-sharing gap; the
   orchestration gap remains.
2. **Full graceful degradation on partial tool failures** (since Day ≤38) — provider fallback
   covers hard API errors, but there's no story for "this tool call failed,
   try a different tool that achieves the same effect."
3. ~~**Per-edit auto-lint-test** (Aider parity)~~ — ✅ Closed Day 68.
   `AutoCheckTool` wrapper runs the first watch phase after each write_file/edit_file.
4. **Skill marketplace curation** (since Day 61) — `/skill install` and `/skill search`
   shipped on Days 60-61, closing the install/discovery gap. Still missing vs
   Claude Code: signed skill bundles, curation/ratings system, formal marketplace
   with reviews. A real but lower-priority gap — the install mechanics work,
   the trust/quality layer doesn't exist yet.

### Competitive landscape shift (Day 74)

The gap is no longer just yoyo vs Claude Code. The field has widened, and the
nature of the gaps has shifted:

**The biggest remaining gaps are deployment-model, not feature-level.** Cloud
agents (Cursor Cloud Agents running on remote worktrees), event-driven triggers
(Cursor BugBot auto-reviewing PRs), and sandboxed execution (Codex Docker/VM
isolation) represent architectural choices that a CLI tool can't replicate
without fundamentally changing what it is. These are ❌ by design choice, not
by oversight.

**Feature parity is close.** MCP, hooks, skills, multi-provider support,
sub-agent dispatch, persistent memory, prompt caching, clipboard integration,
plan workflows — these are now table-stakes across
competitors. yoyo has all of them (✅), which means they're no longer
differentiators but keep-pace requirements.

**Competitor highlights (Day 74):**
- **Claude Code** has a formal plugin ecosystem with 12+ bundled plugins,
  a marketplace with discoverability and install commands, and exposes web
  search, web fetch, code execution, advisor, and memory tools as first-class
  API capabilities. yoyo now matches on install/discovery mechanics
  (`/skill install`, `/skill search`) but lacks the curation/trust layer.
- **Cursor** has Cloud Agents (background cloud worktrees), BugBot (automated
  PR review), and event-driven triggers — pushing toward always-on agent
  presence rather than on-demand invocation.
- **Codex CLI** (OpenAI) has npm/brew install, ChatGPT plan integration,
  sandboxed Docker execution, and a desktop app — lowering the barrier to
  entry for non-terminal users.
- **Aider** v0.85–0.86 added GPT-5 family, Grok-4, and o3-pro support,
  self-contribution metric (88% self-written), plus per-edit auto-lint-test —
  yoyo now matches auto-lint with `AutoCheckTool` (Day 68) and has its own
  self-written metric via `compute_self_written_pct` (Day 68).

yoyo's differentiators remain and have grown: open-source self-evolution,
multi-provider support (14 backends), the skills ecosystem (`/skill install`,
`/skill search`, `/skill create`, 13 skills), `/architect` dual-model mode
(Aider parity), `/loop` iterative refinement, `/plan` generate/review/execute
workflow, `SharedState` for sub-agent data sharing, persistent memory system
(`memory/` JSONL + active context), prompt caching, and the explore-codebase +
x-research + synthesis skills for RLM-style sub-agent dispatch. The plugin gap
has shifted from "no install/discovery at all" to "install works, curation
doesn't exist yet."

### Competitive Notes (Day 74)

The competitive landscape has matured. The key insight from Day 67 still holds:
**the biggest gaps are now deployment-model (cloud agents, IDE integration,
sandboxed execution) rather than feature-level.** Feature parity is close — yoyo
has MCP, hooks, skills, memory, multi-provider, sub-agents, prompt caching,
clipboard, plan workflows, and most developer workflow commands that competitors
offer. The remaining feature-level gaps (persistent named subagents) are
tractable engineering work.

The deployment-model gaps (cloud worktrees, event-driven triggers, Docker
isolation) represent a different class of challenge: they require
fundamentally rethinking what a CLI tool is. These aren't bugs to fix
or features to add — they're architectural decisions about where and how
the agent runs. yoyo's strength as a lightweight, local, open-source CLI
tool is precisely what makes these gaps hard to close, and possibly not
worth closing. A CLI tool that tries to be a cloud service is neither.

MCP and hooks are now table-stakes — all competitors have them. They're
no longer differentiators but ✅ means yoyo keeps pace.

### What was on the old priority queue and is now done

These were listed as gaps on Day 24 but have shipped since:

- ✅ **MCP server support** — `--mcp` flag, `[[mcp.servers]]` config blocks,
  `McpServerConfig` and `parse_mcp_servers_from_config` in `src/config.rs`,
  used in production for weeks.
- ✅ **User-configurable hooks** — `[[hooks]]` config blocks, `Hook` trait and
  `HookRegistry` in `src/hooks.rs`, closing Issue #21 (Day 34).
- ✅ **Sub-agent tool** — `build_sub_agent_tool` in `src/tools.rs` exposes
  yoagent's `SubAgentTool` to the model.
- ✅ **Per-model context window** — Issue #195 fix removed the hardcoded
  200k limit; `effective_context_tokens` in `src/cli.rs` reads per-model
  defaults.
- ✅ **Provider fallback** — `--fallback` chains providers and auto-switches
  on hard errors (Issue #205, Day 31, `try_switch_to_fallback` in `src/agent_builder.rs`).
- ✅ **Bedrock provider wiring** — both the wizard and the actual provider
  construction landed (Day 30 trap closed).
- ✅ **Background process management** — `/bg` command in `src/commands_bg.rs`
  (Day 45): launch, list, view output, kill background jobs. Persistent
  `BackgroundJobTracker` with async completion detection.
- ✅ Recently completed (Day 23–37): `/refactor` umbrella + `rename_symbol`,
  `/watch` auto-test watcher, `/ast` structural search, `/apply` patch
  application, `/stash` conversation stash, terminal bell notifications,
  config `system_prompt`/`system_file` keys, git-aware system prompt,
  proactive context compaction (70% + 80%), streaming flush improvements,
  piped mode improvements, sub-agent directory restriction inheritance,
  audit-log wiring, autocompact thrash detection, live context-window
  percentage, byte-indexing safety pass on tool output pipeline (#250).
- ✅ Recently completed (Day 38–44): per-command bash timeout (`"timeout": N`
  parameter, 1–600s, Day 44), co-authored-by trailer on `/commit` (Day 43),
  `/status` shows session elapsed time and turn count (Day 43), `/changelog`
  command for recent git evolution history (Day 44), CWD race condition fix
  in repo map tests (Day 44), multi-provider fork guide (Day 43).
- ✅ Recently completed (Day 45–46): `/bg` background process management
  (Day 45), multi-provider fork guide (Day 45), destructive-git-command
  guard in `run_git()` (Day 45), streaming output for `/run` and `/watch`
  (Day 45), `/lint fix`, `/lint pedantic`, `/lint strict`, `/lint unsafe`
  (Day 46).
- ✅ Recently completed (Day 47–49): piped mode graceful slash-command
  handling (Day 47), `/blame` with colorized output (Day 48), proper
  unified diffs (LCS-based) for edit_file operations (Day 48), dead code
  cleanup (Day 48), 23 shell subcommands wired for direct CLI invocation
  (Days 48–49), comprehensive categorized help with 68+ commands (Day 49).
- ✅ Recently completed (Day 50–51): context budget warnings at 60/80/90/95%
  (Day 50), `/status` enriched with token counts (Day 50), `/explain`
  file explanation command (Day 50), fuzzy command suggestions via
  Levenshtein distance (Day 50), tool output compression for noisy build
  logs (Day 50), v0.1.8 release (Day 50), integration test speedup —
  removed 2.5 min of unnecessary network waits (Day 51), live output
  improvements for long-running bash commands (Day 51), `/profile`
  session statistics command (Day 51), CWD race fix in repo map tests
  (Day 51).
- ✅ Recently completed (Day 52–53): poison-proof mutex/rwlock handling
  across all production code (Days 52), v0.1.9 release prep (Day 52),
  safety sweep — `.unwrap()` hardening in non-test code including
  `commands_refactor.rs` UTF-8 safety (Day 53), `--stat` flag for `/diff`
  with compact diffstat view (Day 53), exit summary enriched with tokens,
  cost, and duration (Day 53), format module extraction —
  `format/output.rs` (1,543 lines) and `format/diff.rs` (298 lines)
  split from `format/mod.rs` (Day 53), `/checkpoint` command with save,
  restore, list, diff, delete (Day 53).
- ✅ Recently completed (Day 54): `src/safety.rs` extracted from
  `tools.rs` (bash command safety analysis, 510 lines), `yoyo version`
  enriched with build metadata (git hash, build date, yoagent version).
- ✅ Recently completed (Day 55–57): `/quick` quick-question mode (Day 55),
  smart `/add` truncation — files over 500 lines get head+tail with
  omission marker (Day 56), custom commands visible in `/help` (Day 56),
  system prompt sections visible in `/context tokens` (Day 56), RTK
  dependency checkable in `/doctor` (Day 56), commands module extraction —
  `dispatch.rs` + `commands_*.rs` split (Days 55–57).
- ✅ Recently completed (Day 58–59): `SharedState` integration for sub-agent
  data sharing (Day 58), `agent_builder.rs` extracted from `main.rs`
  (Day 58), `DispatchContext` struct to reduce parameter passing (Day 58),
  `/architect` dual-model mode — cheap model plans, expensive model edits
  (Day 59), `/loop <N|until-pass>` iterative prompt command (Day 59),
  `commands_run.rs` extracted from `commands_dev.rs` (Day 59),
  analyze-trajectory JSON contract + token-aware chunking (Day 59).
- ✅ **Plugin / skills ecosystem** — `/skill install <dir>` for local skills
  (Day 60), `/skill install gh:user/repo` for remote GitHub skills (Day 61),
  `/skill search <query>` for GitHub skill discovery (Day 61), `/skill create`
  for scaffolding, `/skill list/show/enable/disable` for management. Closes
  the install/discovery gap that was #1 priority since Day ≤38. Remaining
  sub-gap: signed bundles, curation/ratings, formal marketplace with reviews.
- ✅ Recently completed (Day 60–61): `/skill install` local directories
  (Day 60), CHANGELOG generation (Day 60), `config.rs` extraction (Day 60),
  x-research skill for X/Twitter reading (Day 61), `commands_skill.rs`
  extraction (Day 61), `/skill install gh:user/repo` remote GitHub skills
  (Day 61), `/skill search` GitHub skill discovery (Day 61),
  explore-codebase RLM skill (Day 61), `dispatch_sub.rs` extraction (Day 61),
  positional CLI arguments (Day 59).
- ✅ Recently completed (Day 62–64): real-time subprocess streaming via
  `on_progress` callback (Day 62), `/context files` showing touched files
  by operation type (Day 62), synthesis skill for multi-source research
  (Day 62), tool-specific recovery hints in retry prompts (Day 62),
  non-interactive `yoyo review` for CI pipelines — commit ranges and PR
  review (Day 63), `PromptEventState` struct consolidation (Day 63),
  `ReplConfig` struct (Day 63), module extractions: `rtk.rs` from
  `tools.rs`, `commands_plan.rs` and `commands_ast_grep.rs` from their
  parents (Day 63), `prompt_retry.rs` and `prompt_utils.rs` from
  `prompt.rs` (Day 64), `commands_goal.rs`, `commands_move.rs`,
  `commands_rename.rs`, `commands_todo.rs`, `commands_git_review.rs`
  extractions (Days 62–64), flaky test fix for destructive_guard CWD
  race (Day 64).
- ✅ Recently completed (Day 65–67): `.ok()` error-silencing audit and fixes
  across pipe handling, retry saves, and session state (Day 68),
  `compute_self_written_pct` self-authorship metric via `git blame` (Day 68),
  `AutoCheckTool` wrapper for per-edit auto-lint-test (Day 68), re-export
  chain cleanup in `prompt.rs` — seven files migrated to direct imports
  (Day 67), competitive scorecard refresh (Day 67).
- ✅ Recently completed (Day 68–74): prompt caching configuration for
  Anthropic API — ~90% cost reduction on repeated context (Day 71), desktop
  notifications on long completions with `--no-notify` toggle (Day 71),
  cache hit-rate display in `/cost` (Day 71), `/copy` clipboard command —
  auto-detects pbcopy/xclip/wl-copy/clip.exe (Day 71), `dispatch_command`
  routing refactor — pure `route_command` function + `CommandRoute` enum
  with 92 variants and 18 tests (Day 72), `commands_fork.rs` extraction
  (881 lines, fork + checkpoint logic, Day 72), `commands_stash.rs`
  extraction (Day 72), `commands_web.rs` extraction (803 lines, `/web` +
  `/copy`, Day 72), `/plan` generate/show/apply workflow (Day 72),
  `/map` expanded for C, C++, Ruby, Shell (Day 72), help completeness
  test guard catching `/evolution` and `/copy` (Day 72), `/add` URL
  support — fetch web content inline (Day 72), `write_file` colored diff
  preview (Day 73), output tokens/sec display (Day 73), `/grep --include`
  file-type filtering (Day 73), `/doctor` extended for Java
  (Maven/Gradle), Ruby, C/C++ (CMake) (Day 73), 57 new tests for
  `prompt_retry.rs` (Day 73), 31 new tests for `commands_bg.rs` (Day 72),
  `looks_incomplete` heuristic improvements — unclosed code blocks,
  numbered steps, continuation phrases (Day 73), auto-continue raised
  to 5 follow-ups (configurable via `max_auto_continues`, Day 73),
  `/run` error awareness — exit code preview with analysis offer (Day 73),
  tool-specific recovery hints with escalating fallback paths (Day 70),
  GPT-5/5.5/Grok-4/Gemini 2.5 Flash Lite pricing data (Day 70),
  v0.1.11 release with CHANGELOG (Day 72).

## Stats (Day 74)

- yoyo: ~71,675 lines of Rust across 66 source files (incl. `src/format/`) + integration tests
- 66 source files (was 62 on Day 67): commands split into 30 `commands_*.rs`
  files (`commands.rs`, `commands_ast_grep.rs`, `commands_bg.rs`,
  `commands_config.rs`, `commands_dev.rs`, `commands_file.rs`,
  `commands_fork.rs`, `commands_git.rs`, `commands_git_review.rs`,
  `commands_goal.rs`, `commands_info.rs`, `commands_lint.rs`,
  `commands_map.rs`, `commands_memory.rs`, `commands_move.rs`,
  `commands_plan.rs`, `commands_project.rs`, `commands_refactor.rs`,
  `commands_rename.rs`, `commands_retry.rs`, `commands_revisit.rs`,
  `commands_run.rs`, `commands_search.rs`, `commands_session.rs`,
  `commands_skill.rs`, `commands_spawn.rs`, `commands_stash.rs`,
  `commands_todo.rs`, `commands_tree.rs`, `commands_update.rs`,
  `commands_web.rs`),
  format split into `format/{mod,markdown,highlight,cost,tools,output,diff}.rs`,
  plus `agent_builder.rs`, `hooks.rs`, `memory.rs`, `setup.rs`, `docs.rs`,
  `repl.rs`, `git.rs`, `providers.rs`, `context.rs`, `config.rs`, `prompt.rs`,
  `prompt_budget.rs`, `prompt_retry.rs`, `prompt_utils.rs`, `session.rs`,
  `sync_util.rs`, `dispatch.rs`, `dispatch_sub.rs`, `tools.rs`, `rtk.rs`,
  `safety.rs`, `help.rs`, `cli.rs`, `main.rs`, `watch.rs`,
  `tool_wrappers.rs`, `update.rs`, `conversations.rs`
- 2,792 tests (2,704 unit + 88 integration)
- 13 skills (7 core/creator, 6 yoyo-origin): self-assess, evolve, communicate,
  research, skill-evolve, skill-creator, analyze-trajectory (core);
  social, family, release, explore-codebase, x-research, synthesis (yoyo)
- ~84 REPL commands, ~34 shell subcommands (help, version, setup, init, diff,
  commit, review, blame, grep, find, index, lint, test, doctor, map, tree,
  run, watch, status, undo, docs, update, config, health, skill, todo,
  outline, changelog, evolution, memories, permissions, goal, extended)
- 14 provider backends (anthropic, openai, google, openrouter, ollama, xai,
  groq, deepseek, mistral, cerebras, zai, minimax, bedrock, custom)
- **Published:** v0.1.11 on crates.io (`cargo install yoyo-agent`)
- MCP server support (production)
- User-configurable hooks (`[[hooks]]` config blocks)
- OpenAPI tool loading
- Config file support (.yoyo.toml + ~/.config/yoyo/config.toml)
- Permission system (allow/deny globs + interactive prompts for all tools)
- Directory restrictions (allow-dir/deny-dir, sub-agent inherited)
- Subagent spawning (/spawn) + yoagent `SubAgentTool` exposed to model
- Provider fallback chain (`--fallback`)
- Per-model context window (no longer hardcoded)
- Fuzzy file search (/find)
- Git-aware project context + git-aware system prompt
- Syntax highlighting for 8+ languages
- Conversation bookmarks (/mark, /jump, /marks)
- Codebase indexing (/index) + repo map (/map)
- Argument-aware tab completion
- Inline @file mentions with line ranges and image support
- Image input support (base64 encoding for png/jpg/gif/webp/bmp)
- Context overflow auto-recovery + autocompact thrash detection
- First-run welcome & guided setup
- Proper unified diffs (LCS-based) for edit operations
- `/refactor` umbrella (rename, extract, move) + `rename_symbol` agent tool
- `/watch` auto-test watcher
- `/ast` structural code search
- `/apply` patch application
- `/stash` conversation stash
- Terminal bell notifications
- Config `system_prompt`/`system_file` keys
- Proactive context compaction (70% + 80%)
- Live context-window percentage in prompt
- Per-command bash timeout (`"timeout"` parameter, 1–600s)
- Co-authored-by trailer on `/commit`
- `/status` with session elapsed time and turn count
- `/changelog` command for recent evolution history
- `/bg` background process management
- `/blame` with colorized git blame output
- `/lint fix`, `/lint pedantic`, `/lint strict`, `/lint unsafe`
- Comprehensive categorized help (88 REPL commands, 32 shell subcommands)
- Fuzzy command suggestions (Levenshtein distance)
- Context budget warnings (60/80/90/95%)
- `/profile` session statistics
- `/checkpoint` file-state snapshots (save, restore, list, diff, delete)
- `/explain` file explanation
- Poison-proof mutex/rwlock handling (no panics on poisoned locks)
- `--stat` flag for `/diff` (compact diffstat view)
- Exit summary with tokens, cost, and duration
- `src/safety.rs` — dedicated bash command safety analysis module
- `/quick` quick-question mode (lightweight single-turn)
- Smart `/add` truncation (head+tail for large files)
- `/architect` dual-model mode (cheap planner + expensive editor)
- `/loop` iterative prompt refinement
- `SharedState` key-value store for sub-agent data sharing
- `DispatchContext` struct for clean command dispatch
- `agent_builder.rs` — dedicated agent construction module
- `/skill install` local + remote (`gh:user/repo`) skill installation
- `/skill search` GitHub skill discovery
- `/skill create` new skill scaffolding
- `commands_skill.rs` — dedicated skill command module
- `commands_lint.rs` — dedicated lint command module
- `dispatch_sub.rs` — CLI subcommand routing
- x-research skill (X/Twitter reading via xurl)
- explore-codebase RLM skill (sub-agent codebase comprehension)
- CHANGELOG generation
- Non-interactive `yoyo review` for CI pipelines (commit ranges, PR review)
- Real-time subprocess streaming via `on_progress` callback
- `/context files` showing touched files by operation type
- Synthesis skill for multi-source research (sub-agent dispatch + SharedState)
- Tool-specific recovery hints in retry prompts
- `PromptEventState` struct consolidation
- `ReplConfig` struct for REPL configuration
- Module extractions: `rtk.rs`, `prompt_retry.rs`, `prompt_utils.rs`, `commands_plan.rs`, `commands_ast_grep.rs`, `commands_goal.rs`, `commands_move.rs`, `commands_rename.rs`, `commands_todo.rs`, `commands_git_review.rs`
- Flaky test fix for destructive_guard CWD race
