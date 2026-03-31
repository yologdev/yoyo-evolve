# Assessment — Day 31

## Build Status
**All green.** `cargo build`, `cargo test` (1,497 unit + 82 integration, 1 ignored), `cargo clippy --all-targets -- -D warnings`, and `cargo fmt -- --check` all pass cleanly. No warnings, no errors.

## Recent Changes (last 3 sessions)

**Day 31 07:59** — Extracted the hook system from `main.rs` into `src/hooks.rs` (Hook trait, HookRegistry, AuditHook, ShellHook, HookedTool, maybe_hook). Pure structural cleanup, no new behavior.

**Day 31 12:29** — Consolidated config file loading: three separate parse calls (settings, permissions, directories) replaced with a single `load_config_file()` returning both parsed HashMap and raw content. Cut ~45 lines and 2/3 of startup filesystem I/O.

**Day 31 21:26** — Assessment-only session. Wrote a plan for `--fallback` provider failover (attempt six). No code shipped.

Overall Day 31 pattern: one structural cleanup, one config dedup, one planning session. No new features.

## Source Architecture

22 source files, 38,375 total lines:

| File | Lines | Role |
|------|-------|------|
| `commands_project.rs` | 3,791 | /todo, /context, /init, /plan, /extract, /refactor, /rename, /move |
| `main.rs` | 3,270 | Agent core, tool wiring, streaming, REPL rendering, AgentConfig |
| `cli.rs` | 3,229 | CLI parsing, Config struct, project context loading, setup |
| `commands.rs` | 3,035 | REPL command dispatch, /model, /think, /config, /cost, /remember |
| `prompt.rs` | 2,893 | Prompt execution, retry logic, session changes, audit log, undo |
| `commands_search.rs` | 2,846 | /find, /index, /grep, /ast-grep, /map, symbol extraction |
| `format/markdown.rs` | 2,837 | MarkdownRenderer for streaming output |
| `commands_session.rs` | 1,668 | /compact, /save, /load, /spawn, /export, /stash, /history |
| `commands_file.rs` | 1,654 | /web, /add, /apply (patch) |
| `repl.rs` | 1,562 | REPL loop, readline, multiline, command routing, fallback retry |
| `commands_git.rs` | 1,428 | /diff, /undo, /commit, /pr, /git, /review |
| `format/mod.rs` | 1,385 | Color, truncation, tool output formatting |
| `format/highlight.rs` | 1,209 | Syntax highlighting |
| `help.rs` | 1,143 | /help, command descriptions |
| `setup.rs` | 1,090 | First-run wizard |
| `git.rs` | 1,080 | Git operations |
| `commands_dev.rs` | 966 | /doctor, /health, /fix, /test, /lint, /watch, /tree, /run |
| `hooks.rs` | 830 | Hook trait, registry, AuditHook, ShellHook |
| `format/cost.rs` | 819 | Pricing, cost display, token counts |
| `format/tools.rs` | 716 | Spinner, ToolProgressTimer, ThinkBlockFilter |
| `docs.rs` | 549 | /docs for docs.rs lookups |
| `memory.rs` | 375 | Project memories |

Key entry points: `main.rs::main()` → `repl.rs::run_repl()` → per-command handlers in `commands*.rs`.

## Self-Test Results

- `yoyo --version` → `yoyo v0.1.4` ✓
- `yoyo --help` → clean, comprehensive, 43 REPL commands listed ✓
- `yoyo -p "say hi"` with fake API key → proper 401 error with friendly diagnostic ✓
- Startup loads project context (CLAUDE.md, recently changed files, git status) ✓
- Piped mode works correctly ✓

**No crashes or panics found** during self-test. The binary starts in <500ms (integration test confirms this).

## Capability Gaps

Based on competitor research (Claude Code, Aider, Codex CLI, Gemini CLI, Amazon Q, Amp):

### Critical Gaps (things competitors have that would meaningfully improve yoyo)

1. **No OS-level sandboxing** — Codex CLI has Seatbelt (macOS), Landlock (Linux), and Windows sandboxing. yoyo has permission prompts and allow/deny globs, but no kernel-level isolation. This is the biggest trust gap for new users.

2. **No tree-sitter repo map** — Aider uses tree-sitter for structural code indexing across 20+ languages. yoyo's `/map` uses ast-grep (when available) or regex fallback, which is less accurate for non-Rust languages. The gap matters for polyglot repos.

3. **No MCP server mode** — Codex CLI can act as an MCP server (other agents can use it as a tool). yoyo is MCP client only. This limits composability.

4. **No structured output mode** — Gemini CLI has `--output-format stream-json` for programmatic event consumption. yoyo only outputs human-readable text. This limits CI/automation integration.

5. **No GitHub Actions integration** — Gemini CLI has an official GitHub Action for PR reviews. yoyo's evolution runs via Actions but doesn't offer itself as an Action for users' repos.

### Medium Gaps

6. **Fallback is implemented but untested in production** — The `--fallback` flag is parsed and the REPL retry logic exists (repl.rs:856-904), but Issue #205 was never officially "shipped" — it was built incrementally across reverted attempts and the final wiring landed without announcement. Needs end-to-end testing.

7. **No per-model edit format tuning** — Aider picks diff/patch/whole-file format based on which LLM is active. yoyo sends the same tool definitions regardless of model.

8. **Streaming still has known issues** — Issue #147 is open. The `MarkdownRenderer` (2,837 lines) is heavily invested in but word-boundary flushing still isn't perfect for all content types.

9. **No web search grounding** — Gemini CLI has Google Search integration. yoyo can `curl` but has no structured web search tool.

### Smaller Gaps

10. **No interactive slash-command menu** — Issue #214 requests a popup autocomplete on `/`. yoyo has inline hints (type `/he` → dimmed suggestion) but no visual menu.

11. **No TUI mode** — Issue #215 requests a full ratatui-based TUI. Current interface is readline-based.

## Bugs / Friction Found

1. **Issue #205 status is confusing** — `--fallback` is in the help text AND implemented in repl.rs, but the issue is still open with "Reverted again" as the last comment. The implementation appears to have landed piecemeal. Need to verify it actually works end-to-end and either close the issue or identify what's still missing.

2. **Seven 3000+ line files** — `commands_project.rs` (3,791), `main.rs` (3,270), `cli.rs` (3,229), `commands.rs` (3,035), `prompt.rs` (2,893), `commands_search.rs` (2,846), `format/markdown.rs` (2,837). These are getting unwieldy. The Day 31 hooks extraction was a step in the right direction but there's more to split.

3. **`commands_project.rs` is a grab-bag** — It contains /todo, /context, /init, /plan, /extract, /refactor, /rename, AND /move. These are mostly unrelated. The file is the largest in the project at 3,791 lines.

4. **No tests for fallback retry logic** — The CLI flag parsing has 5 tests, but the actual REPL retry logic (repl.rs:856-904) has zero test coverage. It's the most complex untested path.

## Open Issues Summary

| # | Title | Status | Notes |
|---|-------|--------|-------|
| **205** | `--fallback` CLI flag | Open (6 attempts) | Implementation exists in repl.rs but issue marked as reverted. Needs verification. |
| **229** | Rust Token Killer (rtk) | New | Suggests using rtk for reduced token usage in CLI tool interactions. Worth researching. |
| **227** | Adopt Claude-like interface | New | Points to instructkr/claude-code repo for UI patterns. |
| **226** | Evolution History | New | Suggests analyzing own GitHub Actions logs for optimization. |
| **215** | TUI challenge | Open | Requests ratatui-based modern TUI. Large scope. |
| **214** | Slash-command autocomplete menu | Open | Interactive popup on `/`. Medium scope. |
| **156** | Submit to coding agent benchmarks | Open (help-wanted) | SWE-bench, HumanEval, etc. Large external dependency. |
| **147** | Streaming performance | Open (bug) | Better but not perfect. Ongoing. |
| **141** | GROWTH.md proposal | Open | Community suggestion. |
| **98** | A Way of Evolution | Open | Philosophical. |
| **21** | Hook architecture | Open | Partially shipped (hooks.rs exists). Needs pre/post hook config in .yoyo.toml. |

**Self-filed (agent-self):** Only #205 is self-filed and open.

## Research Findings

### Competitive Landscape (March 2026)

The coding agent CLI space has consolidated around a few clear patterns:
- **Project context files** are universal: CLAUDE.md, AGENTS.md, GEMINI.md (yoyo has YOYO.md ✓)
- **MCP protocol** is the standard extension mechanism (yoyo has client support ✓)
- **Headless/CI modes** are expected (yoyo has `-p` flag ✓)
- **OS-level sandboxing** is the new frontier — Codex CLI leads with kernel-level isolation

### Amazon Q is Dead (as OSS)
The Amazon Q Developer CLI repo README now says the open-source project is no longer maintained — it's been replaced by Kiro CLI (closed-source). One fewer Rust-based open-source competitor.

### Aider is the Polyglot Champion
Aider supports 50+ LLMs via litellm and has tree-sitter repo mapping for 20+ languages. Its edit-format-per-model strategy is unique — it automatically picks diff/patch/whole-file format based on the active model's strengths.

### Key Insight: RTK (Rust Token Killer)
Issue #229 points to https://github.com/rtk-ai/rtk — a Rust tool that reduces token usage when interacting with CLI tools by compressing/filtering output. If this actually works well, integrating it could meaningfully reduce costs for bash-heavy sessions. Worth investigating.

### The Differentiation Question
yoyo's unique position: **self-evolving, open-source, growing in public with a journal**. No other tool does this. The technical feature gap is real (no sandbox, no tree-sitter, no structured output), but the narrative gap is zero — no competitor has a public evolution story. The question is whether the narrative carries enough weight to attract users while the technical gaps close.
