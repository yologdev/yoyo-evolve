# Architecture

This page explains the *reasoning* behind yoyo's internal design — why the codebase is shaped the way it is, what trade-offs were made, and what invariants contributors should understand before changing things. For a machine-generated dependency graph, see [DeepWiki](https://deepwiki.com/yologdev/yoyo-evolve).

## Why 13 modules instead of 3?

yoyo started as a single 200-line file. By Day 10 it was a single 3,400-line `main.rs`. That file was split over Days 10–15 into the current structure, not because someone sat down and designed thirteen modules, but because the code kept telling us where the seams were.

The split follows a simple heuristic: **if two chunks of code change for different reasons, they belong in different files.** Adding a new `/git` subcommand shouldn't force you to scroll past the markdown renderer. Fixing a cost-calculation bug shouldn't put you in the same file as the CLI argument parser.

The current modules, from smallest to largest:

| Module | Lines | Role |
|--------|------:|------|
| `memory.rs` | ~375 | Project-specific `.yoyo/memory.json` persistence |
| `docs.rs` | ~550 | Fetching and parsing docs.rs HTML |
| `help.rs` | ~840 | Per-command help text and `/help` handler |
| `git.rs` | ~1,080 | Low-level git operations (branch, commit, diff) |
| `commands_git.rs` | ~1,130 | `/commit`, `/diff`, `/undo`, `/pr`, `/review` handlers |
| `repl.rs` | ~1,270 | Readline loop, tab completion, multi-line input |
| `commands_session.rs` | ~1,340 | `/save`, `/load`, `/export`, `/spawn`, `/mark`, `/jump` |
| `main.rs` | ~1,560 | Entry point, agent construction, tool wiring |
| `prompt.rs` | ~1,870 | Agent execution, streaming event loop, retry logic |
| `cli.rs` | ~2,520 | Argument parsing, config files, provider selection |
| `commands.rs` | ~2,910 | Core command dispatch, re-exports sub-modules |
| `commands_project.rs` | ~3,660 | `/add`, `/fix`, `/test`, `/lint`, `/tree`, `/find`, `/web`, `/plan` |
| `format.rs` | ~4,700 | Colors, markdown rendering, cost calc, spinner, diffs |

Thirteen modules is a lot for ~24k lines. The alternative — three or four large files — would be easier to navigate in a directory listing but harder to work in. When a module is under 1,500 lines, you can hold its entire API in your head. When it's 4,700 lines (like `format.rs`), you start wanting to split it further — and that's a fair instinct, discussed below.

## The layered design and why it matters

The modules form five rough layers, and the key invariant is: **dependencies only point downward.**

```
  ┌─────────────────────────────────────────────────┐
  │  Entry          main.rs                         │
  ├─────────────────────────────────────────────────┤
  │  REPL           repl.rs                         │
  ├─────────────────────────────────────────────────┤
  │  Commands       commands.rs                     │
  │                 commands_git.rs                  │
  │                 commands_project.rs              │
  │                 commands_session.rs              │
  │                 help.rs                          │
  ├─────────────────────────────────────────────────┤
  │  Engine         prompt.rs       format.rs       │
  ├─────────────────────────────────────────────────┤
  │  Utilities      git.rs   memory.rs   docs.rs    │
  └─────────────────────────────────────────────────┘
```

**Entry layer.** `main.rs` parses CLI args (via `cli.rs`), builds the agent, wires up tools with permission checks, and hands control to either `repl.rs` (interactive) or `prompt.rs` (single-prompt / piped mode). It owns the `AgentConfig` struct and the `build_agent()` / `configure_agent()` functions. It also defines `StreamingBashTool`, a custom replacement for yoagent's default `BashTool` that reads subprocess stdout/stderr line-by-line via `tokio::io::AsyncBufReadExt` and emits periodic `ToolExecutionUpdate` events through the `on_update` callback. This means when a user runs `cargo build` or `npm install`, partial output appears in real-time instead of after the command finishes. The reasoning: agent construction is complex (provider selection, tool wiring, MCP/OpenAPI setup, permission configuration) and shouldn't be tangled with either the REPL loop or command handlers.

**REPL layer.** `repl.rs` owns the readline loop, tab completion, multi-line input detection, and the big `match` block that dispatches `/` commands. It depends on nearly everything below it because it's the traffic cop — but nothing depends on it. This is intentional: piped mode and single-prompt mode bypass the REPL entirely and go straight to `prompt.rs`.

**Command layer.** `commands.rs` is the hub — it re-exports handlers from three sub-modules (`commands_git.rs`, `commands_project.rs`, `commands_session.rs`) and `help.rs`. The sub-module split follows *domain*, not *size*: git-workflow commands in one file, project-workflow commands in another, session-management commands in a third. This means adding a new `/git stash pop` subcommand only touches `commands_git.rs`, even though `commands_project.rs` is three times larger. The split is by reason-to-change, not by line count.

**Engine layer.** `prompt.rs` and `format.rs` are the two largest modules by complexity. `prompt.rs` runs the agent, processes the streaming event channel, handles retries on transient errors, and manages context overflow (auto-compaction). `format.rs` handles everything the user *sees*: ANSI colors, the incremental `MarkdownRenderer`, cost calculations for seven providers, the terminal spinner, diff formatting, and dozens of small display utilities. These two modules sit at the same layer because they collaborate tightly — `prompt.rs` feeds events to `format.rs`'s renderer — but neither depends on commands or the REPL.

**Utility layer.** `git.rs`, `memory.rs`, and `docs.rs` are leaf modules with no upward dependencies. They wrap external systems (git CLI, filesystem JSON, docs.rs HTTP) behind clean Rust APIs. Any module above can call into them, but they never call up. This makes them easy to test in isolation — and they are: `git.rs` has 41 tests, `memory.rs` has 14, `docs.rs` has 23.

The layering isn't enforced by the compiler — Rust's module system doesn't prevent circular `use crate::` imports at the module level. It's enforced by convention and by the fact that violations immediately feel wrong: if `git.rs` needed to call a command handler, that would be a sign the abstraction is leaking.

## Why format.rs is the largest file

At ~4,700 lines with 256 tests, `format.rs` is twice the size of any other module. This isn't accidental — it's the consequence of a design choice: **all terminal presentation logic lives in one place.**

The module contains:

- **Color system** — the `Color` wrapper that respects `NO_COLOR`, all ANSI color constants
- **MarkdownRenderer** — incremental streaming renderer that turns text deltas into ANSI-colored output with syntax highlighting, handling code blocks, headers, bold/italic, lists, and inline code as tokens arrive
- **Cost calculations** — pricing tables for seven providers, input/output/cache cost breakdowns
- **Spinner** — background activity indicator for API roundtrips
- **Display utilities** — `pluralize`, `truncate`, `context_bar`, `format_duration`, `format_token_count`, `format_edit_diff`, `format_tool_summary`, and more

The alternative would be splitting into `color.rs`, `renderer.rs`, `cost.rs`, etc. That's probably the right move eventually. But today, having all presentation in one file has a benefit: when you change how something looks, you only need to look in one place. The `MarkdownRenderer` uses the color system, cost formatting uses the color system, the spinner uses the color system — they're coupled by the shared presentation layer, and co-location makes that coupling visible rather than hiding it across five small files.

The 256 tests are the reason this works at ~4,700 lines. Every public function has test coverage. The `MarkdownRenderer` alone has tests for every markdown construct it handles. If those tests didn't exist, the file would be unmaintainable at this size.

## Why cli.rs is so large

`cli.rs` (~2,520 lines) handles three jobs that sound simple but aren't:

1. **Argument parsing** — yoyo doesn't use `clap` or `structopt`. Arguments are parsed by hand from `std::env::args`. This was a deliberate choice: the CLI has unusual needs (multi-value `--mcp` flags, `--provider` with fallback chains, config file merging) that are easier to handle with custom parsing than with a framework's escape hatches. The trade-off is more code in `cli.rs`, but zero macro magic and full control over error messages.

2. **Config file merging** — `.yoyo.toml` and `YOYO.md` settings merge with CLI flags and environment variables, with a clear precedence chain. This merging logic accounts for hundreds of lines.

3. **Provider configuration** — selecting the right API key, endpoint, and default model for each of eight providers, including fallback behavior when keys aren't set.

The 92 tests in `cli.rs` verify the parsing of every flag and every merge scenario. Adding a new CLI flag means adding it in one place and adding a test.

## The command dispatch pattern

Every `/command` follows the same pattern:

1. User types `/foo bar baz` in the REPL
2. `repl.rs` matches on `"/foo"` and calls `commands::handle_foo(args, agent, ...)`
3. The handler does its work, possibly calling into utility modules
4. If it needs the LLM, it calls `prompt::run_prompt()` with a constructed input

This pattern is enforced by convention, not by a trait. Early versions tried a `Command` trait with `execute()`, but it added ceremony without value — every command has different arguments, different return types, and different needs (some need the agent, some don't, some are async, some aren't). A simple function per command turned out to be the right abstraction level.

The `commands.rs` hub re-exports all handlers so the REPL only needs `use crate::commands::*`. The sub-modules (`commands_git`, `commands_project`, `commands_session`) group by domain. When you run `/commit`, the REPL calls `handle_commit()`, which is defined in `commands_git.rs` and re-exported through `commands.rs`.

## Why prompt.rs handles retries internally

`prompt.rs` encapsulates the entire agent interaction lifecycle: sending the prompt, receiving streaming events, rendering output, and handling errors. Retry logic lives here — not in the REPL or in `main.rs` — because retries need access to the event stream state.

Three kinds of retries happen:

- **Tool failures** — if a tool execution fails, the error is sent back to the LLM as context and it retries (up to 2 times). This happens inside the agent's own loop.
- **Transient API errors** (429, 5xx) — retried with exponential backoff. The REPL doesn't need to know this happened.
- **Context overflow** — when the conversation exceeds the context window, `prompt.rs` triggers auto-compaction (asking the LLM to summarize the conversation so far) and retries with the compressed context.

Keeping this in `prompt.rs` means the REPL's contract is simple: call `run_prompt()`, get back a `PromptOutcome` with the response text, token usage, and any unrecoverable errors. The REPL never has to think about retries, backoff, or context management.

## The streaming renderer design

yoyo streams LLM output token-by-token. The `MarkdownRenderer` in `format.rs` is an incremental state machine that receives text deltas (often partial words or half a markdown construct) and emits ANSI-colored output. This is architecturally significant because:

- **It can't buffer entire lines.** If it did, the output would appear in chunks instead of flowing. An early version had this bug — it was technically correct but felt broken. (Day 17 fix.)
- **It must track state across deltas.** When a delta contains `` ` `` and the next delta contains `rs`, the renderer must know it's inside a code block header. The state machine tracks: are we in a code block? What language? Are we in bold? Italic? A header? A list item?
- **It must handle malformed markdown gracefully.** LLMs sometimes emit unclosed code blocks, nested formatting that doesn't resolve, or markdown-like syntax that isn't actually markdown. The renderer must produce reasonable output regardless.

The alternative — buffering the entire response and rendering it at the end — would be simpler but would make the tool feel unresponsive. Streaming is a UX requirement that imposes real architectural cost.

## Invariants contributors should know

**No upward dependencies from utilities.** `git.rs`, `memory.rs`, and `docs.rs` must never `use crate::commands` or `use crate::repl`. If you find yourself wanting to, the abstraction boundary is wrong.

**`format.rs` is the only module that writes ANSI escape codes.** Other modules call `format::Color`, `format::DIM`, etc. — they don't hardcode escape sequences. This is enforced by convention and makes `NO_COLOR` support work globally.

**Every command handler is a standalone function.** No command state persists between invocations (except through the `Agent`'s conversation history and `SessionChanges`). This makes commands testable in isolation.

**Tests live next to the code they test.** Each module has a `#[cfg(test)] mod tests` block at the bottom. The project has ~1,000 tests total. Integration tests live in `tests/integration.rs` and test the CLI binary as a black box.

**The agent is the only LLM dependency.** yoyo delegates all LLM interaction to the `yoagent` crate. `prompt.rs` receives `AgentEvent`s through a channel — it never constructs HTTP requests or parses API responses directly. This means swapping the LLM backend (or the entire agent framework) would only require changes to `main.rs` (construction) and `prompt.rs` (event handling).

## Trade-offs and known debt

**`format.rs` should probably be split.** The `MarkdownRenderer`, cost tables, and color utilities are three distinct concerns sharing a file. The blocker isn't technical — it's that all three are coupled through the color system, and splitting would require deciding where `Color` lives.

**Hand-rolled CLI parsing is a maintenance burden.** Every new flag requires manual parsing code, help text updates, and config file support. A framework like `clap` would reduce this at the cost of a dependency and less control over error messages. The current approach works because flags don't change often.

**`commands.rs` as a hub creates a wide dependency surface.** Because it re-exports everything, changing any command sub-module can trigger recompilation of anything that imports `commands::*`. In a larger project this would matter for build times. At ~24k lines, it doesn't yet.

**No trait abstraction for commands.** This is fine at the current scale but means there's no compile-time guarantee that all commands follow the same pattern. A new contributor might put command logic directly in `repl.rs` instead of in a handler function. Code review catches this, not the type system.
