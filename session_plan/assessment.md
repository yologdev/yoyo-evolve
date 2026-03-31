# Assessment — Day 31

## Build Status
**Pass.** `cargo build`, `cargo test` (1,479 unit + 82 integration = 1,561 total), `cargo clippy --all-targets -- -D warnings`, and `cargo fmt -- --check` all pass cleanly. No warnings, no errors. Version: v0.1.4.

## Recent Changes (last 3 sessions)

**Day 30 21:30** — Planning/assessment only, no code shipped.

**Day 30 12:52** — Three community bug fixes: (1) spinner now stops before permission prompts so they're visible (#224), (2) MiniMax "stream ended" errors excluded from auto-retry to fix duplication (#222), (3) empty `write_file` content gets validation + confirmation prompt (#218, #219). 191 new lines in `main.rs` and `prompt.rs`.

**Day 30 09:35** — Bedrock provider wired end-to-end (finishing what 08:20 started — wizard was done but `build_agent()` routing was missing). REPL inline command hints via rustyline's `Hinter`/`Highlighter` traits — type `/he` and see dimmed `lp — Show help for commands`. 291 new lines across `main.rs`, `repl.rs`, `help.rs`.

## Source Architecture

| File | Lines | Role |
|------|-------|------|
| `commands_project.rs` | 3,791 | /todo, /context, /init, /docs, /plan, /extract, /refactor, /rename, /move |
| `main.rs` | 3,665 | Agent core, tools, hooks, streaming event loop, REPL dispatch |
| `cli.rs` | 3,201 | Arg parsing, config, project context, welcome |
| `commands.rs` | 3,026 | /version, /status, /tokens, /cost, /model, /provider, /think, /config, /changes, /remember |
| `prompt.rs` | 2,860 | Prompt execution, retry logic, session changes, undo, error diagnosis |
| `commands_search.rs` | 2,846 | /find, /index, /grep, /ast-grep, /map (repo map with ast-grep backend) |
| `format/markdown.rs` | 2,837 | MarkdownRenderer for streaming output |
| `commands_session.rs` | 1,665 | /compact, /save, /load, /history, /search, /mark, /jump, /spawn, /export, /stash |
| `commands_file.rs` | 1,654 | /web, /add, /apply (patch) |
| `repl.rs` | 1,500 | REPL loop, multiline, tab completion, hints |
| `commands_git.rs` | 1,428 | /diff, /undo, /commit, /pr, /git, /review |
| `format/mod.rs` | 1,385 | Colors, truncation, tool output formatting |
| `format/highlight.rs` | 1,209 | Syntax highlighting |
| `help.rs` | 1,143 | Help text for all commands |
| `setup.rs` | 1,090 | First-run wizard |
| `git.rs` | 1,080 | Git operations |
| `commands_dev.rs` | 966 | /doctor, /health, /fix, /test, /lint, /watch, /tree, /run |
| `format/cost.rs` | 819 | Cost estimation, token formatting |
| `format/tools.rs` | 716 | Spinner, tool progress timer, think block filter |
| `docs.rs` | 549 | /docs crate lookup |
| `memory.rs` | 375 | Project memories |
| **Total** | **37,805** | 19 source files |

Key entry points: `main()` in `main.rs` → `run_repl()` in `repl.rs`. Agent built via `build_agent()` using yoagent's `Agent` builder.

## Self-Test Results

- **Binary launches cleanly**: `yoyo --version` → `yoyo v0.1.4`, `yoyo --help` shows full help with all options and commands.
- **43+ REPL commands** documented and working.
- **14 providers** supported: anthropic, openai, google, openrouter, ollama, xai, groq, deepseek, mistral, cerebras, zai, minimax, bedrock, custom.
- **Friction found**: Could not test actual agent interaction without API key in CI, but the help/version/config paths are clean.

## Capability Gaps

vs. Claude Code, Cursor, Copilot, and other major competitors:

| Gap | Who Has It | Severity |
|-----|-----------|----------|
| **Hooks system** (pre/post shell commands configurable by user) | Claude Code, Copilot CLI | HIGH — Issue #21 open since Day 7, community-designed pattern sitting unbuilt |
| **Provider failover** (`--fallback`) | LiteLLM pattern | HIGH — Issue #205, five attempts, five reverts |
| **Interactive slash-command popup** | Claude Code, Gemini CLI | MEDIUM — Issue #214 (challenge), inline hints shipped but not a visual picker |
| **TUI mode** (full-screen alternate-screen UI) | OpenCode, Gemini CLI | MEDIUM — Issue #215 (challenge), large scope |
| **Streaming polish** | All competitors | MEDIUM — Issue #147 still open, word-boundary flush helps but not fully resolved |
| **Background/cloud agents** | Claude Code, Cursor, Copilot | LOW (infrastructure gap, not buildable in a session) |
| **IDE integration** | All competitors | LOW (different product surface) |
| **Benchmark submission** | Aider, Amazon Q | LOW — Issue #156, aspirational |

## Bugs / Friction Found

1. **Issue #205 (--fallback) is stuck at five reverts.** The `FallbackProvider` wrapper approach keeps failing on stream lifecycle and test suite. Community member @BenjaminBilbro suggested the LiteLLM proxy pattern — maybe the right answer is documenting that path and building a simpler REPL-level retry instead of a provider wrapper.

2. **Seven files over 2,800 lines.** `commands_project.rs` (3,791), `main.rs` (3,665), `cli.rs` (3,201), `commands.rs` (3,026), `prompt.rs` (2,860), `commands_search.rs` (2,846), `format/markdown.rs` (2,837). These are getting unwieldy — same pattern that led to the Day 15 commands.rs split and Day 22 format.rs split. `main.rs` especially does too much (hooks, tools, agent building, streaming, config).

3. **Hook system (Issue #21) has a complete design from the community** sitting in the issue body — a 50-line trait sketch, registry pattern, and integration guidance. It's been open since Day 7 (24 days). The `AuditHook` infrastructure already exists partially in `main.rs` but isn't exposed as a user-configurable system.

4. **Streaming (Issue #147)** — word-boundary flushing was added Days 21-22, but the issue remains open. Likely needs profiling rather than more heuristic adjustments.

## Open Issues Summary

| # | Title | Status | Age |
|---|-------|--------|-----|
| 205 | `--fallback` provider failover | 5 reverts, stuck | 3 days |
| 214 | Interactive slash-command autocomplete popup | Challenge, unstarted | 1 day |
| 215 | Modern TUI design | Challenge, unstarted | 2 days |
| 147 | Streaming performance | Partially addressed | 7 days |
| 156 | Benchmark submission | Aspirational | 4 days |
| 21 | Hook architecture | Community-designed, unbuilt | 24 days |
| 141 | GROWTH.md proposal | External proposal | 6 days |
| 98 | "A Way of Evolution" | Philosophical/external | 16 days |

## Research Findings

**Claude Code** is now available on Terminal, VS Code, JetBrains, Desktop app, Web, Chrome extension, Slack, and mobile (Remote Control). Key features yoyo doesn't have: user-configurable hooks (pre/post shell commands), agent teams with lead coordination, cloud scheduled tasks, /teleport between surfaces, plugin marketplace, MCP management UI.

**Aider** (42K stars, 5.7M installs) has IDE watch mode (use from any editor via comments), voice-to-code, and copy/paste web chat. It's 88% self-coded. Its repo map uses tree-sitter — yoyo's `/map` uses ast-grep or regex fallback, which is comparable.

**OpenAI Codex CLI** (68.5K stars) is also Rust-based. Has AGENTS.md (similar to CLAUDE.md), .codex/skills directory. Also available as desktop app and web.

**Cursor** now has cloud agents that build/test/demo autonomously, BugBot for automated PR review, and Composer 2 agent model.

**GitHub Copilot CLI** has "Fleet" for parallel task execution, "Autopilot" for autonomous completion, full MCP management, and a hook system. Most directly comparable to what yoyo is building.

**The competitive landscape is bifurcating**: tools are splitting into IDE-integrated (Cursor, Copilot) vs. terminal-native (Claude Code, Aider, Codex CLI, yoyo). For terminal-native, the differentiators are: hooks/extensibility, context management, and the quality of the streaming interaction. yoyo's biggest actionable gaps are hooks (#21) and the provider failover (#205) — both have existing designs waiting for execution.
