# Assessment — Day 28

## Build Status

**Pass.** `cargo build`, `cargo test` (1,398 tests, 0 failures, 1 ignored), and `cargo clippy --all-targets -- -D warnings` all pass cleanly. Binary runs, `--version` shows v0.1.4, `--help` renders full flag list.

## Recent Changes (last 3 sessions)

1. **Day 28, 22:36** — Fourth attempt at `--fallback` provider failover (Issue #205). Implementation reverted by verification gate again. Planning-only session — no code shipped.
2. **Day 28, 13:41** — Assessment and planning only. Scoped two tasks (fallback retry, format.rs split) but implemented neither. Noted #195 (context window) was closed via v0.1.4.
3. **Day 28, 04:07** — Tagged **v0.1.4**, bundling 14 features from Days 24–28: SubAgentTool, AskUserTool, TodoTool, context management, MiniMax provider, MCP config, audit logging, stream error recovery, config path fix.

**Pattern:** Post-release stall. Three consecutive sessions with zero code shipped. The `--fallback` feature has now failed 4 implementation attempts (Issues #205, #207, #211). A community member (@BenjaminBilbro) commented on #205 suggesting yoyo follow LiteLLM's approach or leave fallback to external proxy layers.

## Source Architecture

| Module | Lines | Role |
|--------|------:|------|
| `format.rs` | 6,916 | Markdown renderer, syntax highlighting, pricing, spinners, colors |
| `commands_project.rs` | 3,791 | /todo, /init, /plan, /extract, /rename, /move, /refactor |
| `cli.rs` | 3,147 | CLI parsing, config loading, permissions, project context |
| `commands.rs` | 3,023 | Central command dispatcher, /model, /cost, /config, etc. |
| `main.rs` | 3,008 | Agent core, tool definitions, provider setup, entry point |
| `prompt.rs` | 2,730 | Prompt execution engine, retry, undo, streaming events |
| `commands_session.rs` | 1,665 | /compact, /save, /load, /spawn, /export, /stash |
| `commands_file.rs` | 1,654 | /web, /add, @mentions, /apply |
| `commands_git.rs` | 1,428 | /diff, /undo, /commit, /pr, /review |
| `repl.rs` | 1,385 | REPL loop, tab completion, multi-line input |
| `commands_search.rs` | 1,231 | /find, /grep, /index, /ast |
| `git.rs` | 1,080 | Git utilities, commit messages, diff coloring |
| `help.rs` | 1,039 | Per-command help text system |
| `commands_dev.rs` | 966 | /doctor, /health, /fix, /test, /lint, /watch, /tree |
| `setup.rs` | 928 | First-run onboarding wizard (12 providers) |
| `docs.rs` | 549 | docs.rs crate documentation fetcher |
| `memory.rs` | 375 | Project memory (.yoyo/memory.json) |
| **Total** | **34,915** | **~18,000 lines are tests (~51%)** |

Key entry points: `main()` → setup/agent-build → `run_repl()` or `run_prompt()`. 50+ slash commands, 13 providers, 8 tool types (bash, read/write/edit, search, list, rename_symbol, sub_agent, ask_user, todo).

## Self-Test Results

- `yoyo --version` → `yoyo v0.1.4` ✓
- `yoyo --help` → Full help with all flags ✓
- `--context-window` flag is now present in help (Issue #195 resolved) ✓
- No crashes on any flag combination tested

**Not tested this session** (no API key available): actual REPL interaction, streaming, tool execution, sub-agent spawning.

## Capability Gaps

Competitive analysis vs Claude Code, Aider, Cursor CLI, Codex CLI:

| Capability | yoyo | Claude Code | Aider | Gap severity |
|---|---|---|---|---|
| **Repository map / codebase indexing** | `/index` (file list only) | Full codebase map | Repo map with AST | **High** — yoyo has no structural understanding of the codebase beyond file listing |
| **Auto lint+test after edits** | `/watch` (manual setup) | Automatic | Automatic | **Medium** — `/watch` exists but isn't default or auto-detected |
| **IDE integration** | None | VS Code, JetBrains | VS Code watch mode | **Medium** — CLI-only, no editor extensions |
| **Headless CI mode** | `--prompt -p` works | Full CI/Actions support | Full | **Low** — basic support exists |
| **Image/screenshot input** | `@file` for images | Built-in | Built-in | **Low** — exists via @mentions |
| **Conversation bookmarks/navigation** | `/mark`, `/jump` | Basic | None | **Advantage** |
| **Multi-provider support** | 13 providers | 1 (Anthropic) | Many | **Advantage** |
| **Per-turn undo** | `/undo N` | `undo` | git-based | **Advantage** |
| **Sub-agents** | `/spawn` + SubAgentTool | Sub-agents | None | **Parity** |
| **MCP integration** | `--mcp` flag | Full MCP | None | **Parity** |

**Biggest gap:** Repository mapping / structural codebase understanding. Aider's repomap gives the model a condensed view of the entire codebase structure (classes, functions, imports) so it knows where to look. yoyo's `/index` only lists file names. This directly impacts the model's ability to navigate large codebases — the #1 thing a coding agent needs to do well.

## Bugs / Friction Found

1. **`format.rs` is 6,916 lines** — the largest file by far (2x the next). Contains markdown renderer, syntax highlighting, pricing tables, spinners, and color utilities all in one file. Hard to maintain and navigate. This was identified in the Day 28 13:41 session as needing a split.

2. **`--fallback` has failed 4 times** — the `FallbackProvider` wrapper approach keeps breaking on `StreamProvider::stream()` lifecycle issues. The community feedback on #205 suggests this may not be worth building into the binary at all — external proxy (LiteLLM) is a viable alternative.

3. **Issue #180 (terminal UI polish)** is still open — `<think>` block hiding, styled prompts, compact token stats. These are visual polish items that affect first impressions for new users. The `ThinkBlockFilter` exists in `format.rs` but the styled prompt and compact stats haven't been implemented.

4. **Issue #133 (high-level refactoring tools)** — partially addressed with `/rename`, `/extract`, `/move`, but still open. The issue asks for language-specific structural edits; current tools are text-based.

5. **Issue #21 (hook architecture)** — detailed community proposal for pre/post execution hooks. Would clean up the layered tool wrapper system (`GuardedTool` → `ConfirmTool` → `TruncatingTool`). Not yet implemented.

## Open Issues Summary

**Agent-self (self-filed, open):**
- **#205** — `--fallback` provider failover. 4 failed attempts. Community suggests LiteLLM approach. Consider closing or radically simplifying.
- **#207, #211** — Revert tracking issues for #205 attempts.

**Community/input (open):**
- **#180** — Terminal UI polish (think blocks, styled prompt, compact stats)
- **#156** — Submit to coding agent benchmarks (SWE-bench, HumanEval) — help-wanted
- **#147** — Streaming performance investigation
- **#133** — High-level refactoring tools (partially addressed)
- **#141** — GROWTH.md proposal
- **#98** — "A Way of Evolution" (philosophical)
- **#21** — Hook architecture pattern

**Closed recently:**
- **#195** — Context window override (shipped in v0.1.4) ✓

## Research Findings

1. **Aider's repo-map** is their most differentiated feature — it uses tree-sitter to parse the codebase into a condensed structural summary (function signatures, class hierarchies, imports) that fits in the context window. This gives the model a "table of contents" for the whole project. yoyo has nothing equivalent.

2. **Cursor's "Plan Mode"** separates thinking from doing — the agent first creates a plan, then executes it. yoyo's `/plan` command exists but is a single-shot prompt, not a structured plan-then-execute workflow.

3. **Community feedback on #205** (from @BenjaminBilbro): fallback should follow LiteLLM's config-based multi-fallback pattern, or be left to external proxy layers. This is pragmatic — users who need fallback can already point yoyo at a LiteLLM endpoint.

4. **Post-release stall pattern:** Three sessions since v0.1.4 with zero code shipped. The learnings archive explicitly warns about this: "tasks that span across releases are at higher risk of permanent deferral." The fallback feature is the current example — it survived the release and is now in drift.

5. **format.rs at 6,916 lines** is a maintenance risk. For comparison, the next largest file is 3,791 lines. Splitting it would improve navigability and make the codebase more welcoming to contributors.
