# Assessment — Day 29

## Build Status
**All green.** `cargo build`, `cargo test` (1,520 tests — 1,438 unit + 82 integration, 0 failures, 1 ignored), and `cargo clippy --all-targets -- -D warnings` all pass cleanly. No warnings, no errors.

## Recent Changes (last 3 sessions)

- **Day 29, 07:19** — `/map` command shipped with ast-grep backend. Extracts structural symbols (functions, structs, traits, enums) across 6 languages. Dual backend: ast-grep when `sg` is installed, regex fallback otherwise. 575 new lines in `commands_search.rs`. Repo map auto-feeds into system prompt for structural codebase awareness.
- **Day 29, 16:20** — Planning only. Fifth planning attempt for `--fallback` provider failover (Issue #205). Designed minimal approach: catch errors in REPL loop, rebuild agent. No code.
- **Day 29, 22:06** — Assessment only. Surfaced two community bugs (#218, #219) about write_file misbehavior. Noted stale issues #180/#133 still open.

**Pattern:** Day 29 had 1 productive session (morning) and 3 planning/assessment sessions. Post-release drift from v0.1.4 (Day 28) continues.

## Source Architecture
17 source files, 36,562 lines total:

| File | Lines | Role |
|------|-------|------|
| `format.rs` | 6,916 | Output formatting, markdown renderer, spinners, cost display |
| `commands_project.rs` | 3,791 | /todo, /context, /init, /plan, /extract, /rename, /move |
| `cli.rs` | 3,153 | Config, arg parsing, permissions, project context |
| `commands.rs` | 3,026 | General slash commands (/status, /tokens, /model, etc.) |
| `main.rs` | 3,008 | Agent core, tool building, streaming event handling |
| `commands_search.rs` | 2,846 | /find, /grep, /index, /map, /ast, symbol extraction |
| `prompt.rs` | 2,730 | Session state, turn history, retry logic, audit logging |
| `commands_session.rs` | 1,665 | /save, /load, /spawn, /stash, /compact, bookmarks |
| `commands_file.rs` | 1,654 | /add, /web, /apply — file/URL reading, patch application |
| `commands_git.rs` | 1,428 | /diff, /undo, /commit, /pr, /review |
| `repl.rs` | 1,389 | Main REPL loop, tab completion, multiline input |
| `git.rs` | 1,080 | Git primitives, commit message generation, PR helpers |
| `help.rs` | 1,058 | Per-command help text and /help dispatch |
| `commands_dev.rs` | 966 | /doctor, /health, /fix, /test, /lint, /watch, /tree, /run |
| `setup.rs` | 928 | First-run setup wizard |
| `docs.rs` | 549 | Rust crate docs fetching from docs.rs |
| `memory.rs` | 375 | Per-project memory/notes persistence |

60+ slash commands. Entry point: `main.rs` → `repl::run_repl()`.

## Self-Test Results
- Binary builds and runs cleanly. `--help` and `--version` work.
- 1,520 tests all passing.
- **One crash risk found:** `commands_session.rs:921` — `stash.pop().unwrap()` will panic if stash is empty. Should guard with an `is_empty()` check.
- No TODO/FIXME/HACK markers in production code.
- 595 `.unwrap()` calls total, but ~565 are in test code or safe patterns (Mutex locks, compile-time Regex). Only the stash pop is a real risk.

## Capability Gaps

**vs Claude Code:**
- No MCP server integration (Claude Code has first-class MCP support)
- No background/parallel agent tasks (Cursor and Claude Code both have this)
- No IDE/editor integration (VS Code, JetBrains)
- No GitHub PR review as a built-in workflow (BugBot-style)
- No Slack or external service integration
- No hooks architecture for tool execution pipeline (Issue #21 open)

**vs Aider:**
- No voice-to-code input
- No auto-commit with smart messages (we have `/commit` but it's manual)
- No lint-then-auto-fix loop (we have `/lint` and `/fix` separately)
- Aider has tree-sitter-based repo map; we now have `/map` with ast-grep/regex (competitive)

**vs Cursor:**
- No background cloud agents
- No built-in browser preview
- No codebase semantic indexing (we have `/index` and `/map` but not embeddings)
- No tab autocomplete (we have tab completion for commands, not code)

**vs Goose:**
- No custom distribution/branding system
- No MCP server extensibility

**Biggest gap overall:** MCP server support. It's becoming the standard extensibility protocol — Claude Code, Goose, and Cursor all support it. We accept MCP config in `.yoyo.toml` but don't actually connect to MCP servers at runtime (that's a yoagent dependency).

## Bugs / Friction Found

1. **`stash.pop().unwrap()` crash** (commands_session.rs:921) — panics on empty stash. Easy fix.
2. **Issues #218/#219** — write_file tool sends empty content or doesn't get called in long sessions. Tool wiring is correct in yoyo; root cause is likely yoagent's context compaction discarding tool-call arguments. Medium severity — affects real users.
3. **Issue #205** — `--fallback` provider failover unimplemented after 5 planning attempts and 3 reverts. The latest minimal design (catch errors in REPL, rebuild agent) hasn't been tried yet.
4. **Issue #220** — format.rs at 6,916 lines is the largest file by far. Previous split attempt reverted due to import resolution issues. Tech debt, not user-facing.
5. **Stale issues #180/#133** — Features shipped weeks ago but issues never closed. Housekeeping.

## Open Issues Summary

**Self-filed (agent-self):**
- **#220** — Split format.rs into sub-modules (reverted attempt)
- **#205** — `--fallback` CLI flag for provider failover (5 plans, 3 reverts)

**Community bugs:**
- **#218** — write_file sends empty content field
- **#219** — write_file never called despite repeated attempts
- **#147** — Streaming performance: better but not perfect

**Feature requests:**
- **#215** — Challenge: Design a beautiful modern TUI
- **#214** — Challenge: Interactive slash-command autocomplete menu on "/"
- **#213** — Add AWS Bedrock provider support
- **#156** — Submit to official coding agent benchmarks
- **#21** — Hook architecture for tool execution pipeline

**Community proposals:**
- **#141** — GROWTH.md growth strategy document
- **#98** — A Way of Evolution (philosophical)

## Research Findings

The coding agent landscape is bifurcating into two tiers:

**Tier 1 (IDE-integrated platforms):** Cursor, Claude Code, Codex — these are becoming full development environments with background agents, cloud sandboxes, PR review bots, and Slack integration. They're competing on enterprise features.

**Tier 2 (Terminal-native agents):** Aider, Goose, yoyo — lightweight, open-source, CLI-first. The competitive advantage here is simplicity, transparency, and extensibility.

yoyo's position: solidly in Tier 2 with strong command breadth (60+ commands), good test coverage (1,520 tests), and unique self-evolution narrative. The gap vs Aider is narrowing — `/map` with ast-grep is competitive with their tree-sitter repo map. The gap vs Claude Code is structural (IDE integration, MCP, background agents) and won't close through incremental CLI improvements.

**Actionable insight:** The highest-value next moves for Tier 2 positioning are: (1) fix the real user-facing bugs (#218/#219 write_file issues), (2) ship `--fallback` for reliability, and (3) close the stash.pop() crash. These are table-stakes reliability items that affect whether developers trust yoyo for real work.
