# Changelog

All notable changes to **yoyo-agent** (`cargo install yoyo-agent`) are documented here.

This project is a self-evolving coding agent — every change was planned, implemented, and tested by yoyo itself during automated evolution sessions. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.13] — 2026-05-20

6 sessions spanning Days 79–81. Watch mode learns to parse TypeScript and Python errors alongside Rust, file operation permissions can be persisted to config, and startup now reads instruction files from five AI tools. Lua and Zig join `/map`, `/init` detects existing AI configs, and a steady cleanup arc squashes flaky tests across multiple modules.

### Added

- **Permission persistence for file operations** — `/allow` patterns can be saved to `.yoyo.toml` when the user chooses "always" on a file confirmation prompt (Day 79)
- **Structured Rust compiler error parsing** — watch mode now parses `rustc` error codes into categories (borrow, type, lifetime, import, syntax, unused, test assertion) with category-specific fix hints (Day 79)
- **TypeScript and Python error parsing** — watch mode structured error parsing extended to `tsc`/`eslint` and `pytest`/`mypy` output formats (Day 81)
- **Broader project instruction file support** — startup now reads `AGENTS.md`, `.cursorrules`, and `.github/copilot-instructions.md` alongside `CLAUDE.md` and `YOYO.md` (Day 80)
- **Lua and Zig language support in `/map`** — 17 languages total (Day 80)
- **Smart `/init`** — detects existing AI tool instruction files and notes them in generated config (Day 80)

### Improved

- **36 new unit tests for `session.rs`** — SessionChanges, TurnSnapshot, TurnHistory, format_changes now thoroughly tested (Day 79)
- **Unit tests for `commands_map.rs`** — symbol extraction and repo map formatting (Day 79)

### Fixed

- **Flaky `set_current_dir` tests** — multiple test files migrated from local mutex to `#[serial]` to prevent cross-file race conditions (Days 79–81)

## [0.1.12] — 2026-05-17

8 sessions spanning Days 75–78. A new `--print` mode strips all chrome for scripting and pipelines, `--no-tools` enables pure conversation without any tool access, and `--disallowed-tools` lets you selectively block specific tools. Session restore now shows where you left off, `/compact` accepts arguments for precise control, and the repo map ranks files by relevance instead of alphabet. Five new languages in `/map`, project-type hints for strangers, and a steady arc of test coverage across tool wrappers, dispatch, and tools.

### Added

- **`--print` mode** — raw output only, no banner/cost/chrome, for scripting and pipeline use (Days 76–77)
- **`--no-tools` flag** — suppress all tools including sub_agent, shared_state, and MCP connections for pure conversation mode (Day 78)
- **`--disallowed-tools` flag** — selectively block specific tools by name (Day 76)
- **Session resume summary** — shows last user/assistant messages when restoring with `--continue`, so you know where you left off (Day 78)
- **`/compact` arguments** — `/compact 5` to keep last N exchanges, `/compact all` to compress everything, instead of the old one-size-fits-all default (Day 77)
- **Project-type context hints** — auto-injects development conventions (test commands, lint tools) for detected project types when no instruction file exists (Day 76)
- **`/spawn --bg` flag** — launch sub-agents in background and keep working (Day 76)
- **`/tokens` breakdown** — shows where context tokens went (system prompt, conversation, tool output) (Day 76)
- **5 new languages in `/map`** — C#, PHP, Kotlin, Swift, Scala symbol extraction (Day 77)

### Improved

- **Relevance-ranked repo map** — files ranked by recency and symbol density before truncation, instead of alphabetical cutoff (Day 78)
- **Recovery hints wired to all tools** — `RecoveryHintTool` now wraps all tools, not just during retry; failures immediately suggest concrete alternatives (Day 75)
- **`/retry` carries failure context** — retry includes what went wrong and a suggested recovery path, instead of retrying with fresh hope (Day 75)

### Fixed

- **Auto-watch message leak in `--print` mode** — suppressed chrome output that was bypassing quiet mode (Day 77)
- **`print_usage` and `print_context_usage` leak in `--print`/quiet mode** — properly suppressed with early returns (Day 77)
- **Architect mode test flakiness** — added `#[serial]` to global-state-mutating tests that were racing each other (Day 77)

### Changed (Internal / Architecture)

- **Test coverage expansion** — new tests for `dispatch.rs` (command routing), `tools.rs` (StreamingBashTool, RenameSymbolTool, build_tools), `tool_wrappers.rs` (ToolFailureTracker, truncate_result, AutoCheckTool, TruncatingTool, RecoveryHintTool), `commands_update.rs` (platform detection, version comparison) (Days 75–78)
- **`cli_config.rs` extraction** — constants (VERSION, thresholds) and Config struct separated from `cli.rs` for cleaner imports (Day 75)

## [0.1.11] — 2026-05-13

20 sessions spanning Days 64–74. Prompt caching cuts repeated system prompt costs ~90%, native desktop notifications tell you when long completions finish, clipboard integration via `/copy`, smarter auto-continue for incomplete responses, error-aware `/run`, and a deep consolidation arc that deduplicated 13-way token arithmetic, extracted 6+ modules, and cleaned every re-export middleman out of `prompt.rs`.

### Added

- **`/revisit` command** — scan closed/shelved GitHub issues, check feasibility, and track revisit candidates in `.yoyo/revisit.json` (Day 74)
- **Error-aware `/run`** — when a command fails, shows a colored failure preview and offers to analyze the error with the agent (Day 73)
- **`/doctor` for Java, Ruby, C/C++ projects** — expanded project health checks beyond Rust/Python/Node (Day 73)
- **Output tokens/sec in usage line and `/profile`** — shows generation speed alongside token counts (Day 73)
- **Colored diff preview for `write_file`** — when overwriting an existing file, shows a colored unified diff of the changes (Day 73)
- **Prompt caching via yoagent's CacheConfig** — automatic `cache_control` on system prompt and long tool results, ~90% cost reduction on repeated system prompts; cache hit rate visible in `/cost` and `/tokens` (Day 71)
- **Native desktop notifications** — OS-level notification + terminal bell when completions take >10s; configurable via `notify = false` in config (Day 71)
- **`/copy` command** — clipboard integration detecting `pbcopy`/`xclip`/`wl-copy`/`clip.exe`; subcommands: `/copy last`, `/copy code`, `/copy all` (Day 71)
- **`/changes summary` subcommand** — AI-generated overview of files modified in the current session and why (Day 70)
- **`/model list` command** — browse all known models grouped by provider, with context window sizes (Day 64)
- **Model registry updated** — added GPT-5, GPT-5-mini, GPT-5.5, Grok-4, Grok-4-mini, Gemini 2.5 Flash Lite to provider registry and pricing (Days 64, 70)

### Improved

- **Auto-continue heuristic strengthened** — detects unclosed code fences, numbered lists, and "let me" phrases as incomplete responses; max auto-continues bumped from 3 to 5 (Day 73)
- **Tool recovery with concrete alternative suggestions** — retry prompts now suggest specific alternative tools when one fails (e.g. `edit_file` failing → suggests `write_file`), improving agent self-recovery (Day 70)
- **Banner shows project context at startup** — `📁 Rust project (yoyo-evolve) on main` with auto-detected project type, name, and branch (Day 64)
- **`/evolution` command** — shows CI run status, session stats, and recent evolution history (Day 68)
- **Competitive gap analysis refreshed** — updated scorecard against Claude Code, Cursor, Gemini CLI, Codex, and Aider; identified phase transition from missing features to architectural divergences (Day 67)

### Fixed

- **Silent `.ok()` error swallowing in piped mode and retry path** — errors now properly propagated instead of silently discarded (Day 68)
- **Silent data loss in `save_messages`** — messages no longer silently dropped across provider/model/thinking switches (Day 70)

### Changed (Internal / Architecture)

- **Test coverage expansion** — new tests for `prompt.rs` (stream JSON, PromptOutcome), `prompt_retry.rs` (diagnose_api_error, build_retry_prompt), `tools.rs` (TodoTool, RenameSymbolTool, build_tools), `commands_bg.rs`, `main.rs` (build_json_output, apply_config_flags), and health check coverage (Days 73–74)
- **Command routing extraction** — pure `route_command()` function extracted from `dispatch_command` with full test coverage, enabling deterministic testing of command routing (Day 72)
- **`accumulate_usage` helper** — deduplicated 13-way token-accumulation arithmetic across `prompt.rs` into a single function (Day 66)
- **`finish_prompt_epilogue` helper** — collapsed duplicated 15-line post-prompt epilogue (cost display, bell, context bar) into one call (Day 66)
- **`PostPromptContext` / `ConfigDisplay` structs** — replaced 13- and 14-parameter functions with named structs (Day 66)
- **6 module extractions** continuing the consolidation arc:
  - `conversations.rs` from `repl.rs` — side, quick, and extended conversation handlers (Day 64)
  - `prompt_retry.rs` from `prompt.rs` — retry logic, error classification, backoff (Day 64)
  - `prompt_utils.rs` from `prompt.rs` — search, highlighting, summarization utilities (Day 64)
  - `commands_update.rs` from `commands_dev.rs` — `/update` command (Day 65)
  - `commands_tree.rs` from `commands_dev.rs` — `/tree` command (Day 65)
  - Watch-mode auto-detection moved to `watch.rs` where it belongs (Day 65)
- **`prompt.rs` reduced from ~2,425 to ~1,300 lines** — retry, utility, and re-export middleman cleanup (Days 64, 66–67)
- **`commands_dev.rs` reduced from ~1,693 to ~714 lines** — three command extractions (Day 65)
- **Re-export cleanup** — removed `pub use` middlemen from `prompt.rs`; all dependents now import directly from source modules (Days 66–67)
- **`run_repl` readability** — architect mode and post-prompt handling extracted into named functions; startup banner pulled into its own function (Days 65–66)
- **Test race condition fix** — `set_current_dir` race in parallel tests replaced with parameter-passing approach (Day 64)
- **`tool_wrappers.rs` extraction** — tool decorator types pulled out of `tools.rs` (Day 64)

## [0.1.10] — 2026-05-02

6 sessions spanning Days 61–63. Real-time bash streaming closes the #1 competitive gap, non-interactive code review enables CI pipeline integration, three new skills (synthesis, explore-codebase, x-research) expand the RLM substrate, and the skill ecosystem gains GitHub-based discovery and remote installation.

### Added

- **Real-time bash output streaming** — live stdout/stderr via yoagent's `on_progress` callback replaces buffered output; the single biggest UX gap vs Claude Code, now closed (Day 62)
- **Non-interactive `yoyo review`** — run code review from CLI without entering the REPL, with `--json` output and exit codes for CI pipeline integration (Day 63)
- **`/context files` subcommand** — see which files were touched during the current conversation (Day 62)
- **`/skill search` on GitHub** — discover community-published skills by searching GitHub repositories (Day 61)
- **`/skill install gh:user/repo`** — install skills directly from GitHub repositories (Day 61)
- **synthesis skill** — multi-source research comparison via sub-agent dispatch and SharedState; aggregates 3+ sources or any source >5KB (Day 62)
- **explore-codebase skill** — RLM-style large-codebase comprehension by dispatching sub-agents to explore regions without bloating main context (Day 61)
- **x-research skill** — read X/Twitter via xurl: search posts, fetch threads, read profiles, and read long-form articles (Day 61)

### Improved

- **Auto-retry with tool-specific recovery hints** — retry prompts now include the tool name and specific recovery suggestions (e.g. "check file exists" for read_file, "try a different search pattern" for search), improving agent self-recovery (Day 62)
- **Gap analysis updated** — competitive analysis refreshed to reflect skill ecosystem maturity and recent changes in Claude Code, Aider, and Codex (Day 61)

### Changed (Internal / Architecture)

- **4 module extractions** continuing the consolidation arc:
  - `dispatch_sub.rs` from `dispatch.rs` — CLI subcommand routing, `flag_value`, `FlagValueCheck` (Day 61)
  - `commands_todo.rs` from `commands_project.rs` — `/todo` handling (Day 61)
  - `commands_goal.rs` from `commands_project.rs` — `/goal` handling (Day 61)
  - `commands_ast_grep.rs` from `commands_search.rs` — `/ast` command (Day 63)

## [0.1.9] — 2026-04-29

18 sessions spanning Days 50–60. Dual-model architecture mode, iterative prompt loops, direct positional prompts, ten module extractions, zero production unwraps, SharedState for sub-agents, multi-phase watch, and a long consolidation arc that cleaned house before building new rooms.

### Added

- **`/architect` dual-model mode** — Aider-inspired split where a strong reasoner (e.g. Claude Opus) plans changes and a cheaper model executes them, cutting complex-task costs 60–80%; toggle with `/architect` or `/architect <model>` (Day 59)
- **`/loop` iterative prompt command** — repeat a prompt N times (`/loop 5 "fix warnings"`) or until a command passes (`/loop until-pass cargo test`), automating the fix-test-fix cycle (Day 59)
- **Bare positional prompts** — `yoyo "fix this bug"` now works without `--prompt`; leftover CLI arguments are collected as a prompt, matching Claude Code/Aider/Codex UX (Day 59)
- **`/quick` direct model query** — skip the full agent loop for simple questions; one-turn Q&A with no tool calls (Day 55)
- **`/plan` mode** — sustained read-only toggle that allows search, read, and analysis but blocks modifications and destructive commands (Day 56)
- **`/checkpoint` command** — name a moment in your editing session and jump back later; supports save, restore, list, diff, and delete (Day 53)
- **`/outline <file>` file-scoped view** — symbol outline for a single file instead of the whole project (Day 58)
- **`/config set` and `/config get`** — change and inspect settings mid-session without editing TOML files (Day 56)
- **`/context tokens` breakdown** — system prompt section analysis showing token count per section (personality, project context, skills, etc.) (Day 56)
- **`--quiet` flag** — suppress informational output (config loaded, context files, spinners) for piped/scripted use (Day 57)
- **`/profile` command** — unified session summary in a bordered box showing model, provider, duration, turns, tokens, estimated cost, and color-coded context usage (Day 51)
- **"Did you mean?" fuzzy suggestions** — mistyped slash commands now suggest the closest match using Levenshtein distance with length-adaptive thresholds and unique prefix matching (Day 50)
- **5 more shell subcommands** — `changelog`, `config`, `permissions`, `todo`, and `memories` wired for direct CLI invocation without starting a session (Day 50)
- **`/config edit` subcommand** — opens `.yoyo.toml` or `~/.config/yoyo/config.toml` in `$EDITOR` (Day 50)
- **Proactive context budget warnings** — automatic warnings after each agent turn when context window usage is high (Day 50)
- **`--budget` flag for `/extended`** — set a wall-clock time limit for long-running tasks (Day 53)
- **SharedState for sub-agents** — parent and child agents share a key-value store via `yoagent::SharedState`, enabling artifact passing by reference instead of by copy (Day 58)

### Improved

- **`/watch all` multi-phase mode** — auto-detects both linter and test suite for your project and chains them (e.g. `cargo clippy && cargo test`), stopping at first failure; inspired by Aider's auto-lint-fix loop (Days 57–58)
- **`DispatchContext` struct** — consolidated 20 function arguments into a single named struct for the command dispatch system (Day 58)
- **Build metadata in version output** — `yoyo version` now shows git hash, build date, and platform: `yoyo v0.1.9 (a529e52 2026-04-23) linux-x86_64`; `DAY_COUNT` baked in at compile time so release binaries show the evolution day (Days 54–55)
- **Argument hints for subcommands** — dim inline completions (e.g. `/diff [file] [--stat] [--cached]`) appear when typing subcommands (Day 54)
- **Custom commands visible in `/help`** — `.yoyo/commands/*.md` files now appear in their own help section, and `/help my-command` shows their content (Day 56)
- **RTK dependency check in `/doctor`** — self-diagnostic now reports whether Rust Token Killer is installed (Day 56)
- **`stderr_is_terminal()` gating** — spinners, progress bars, and animated output suppressed when stderr is not a terminal, preventing garbage in piped output (Day 57)
- **Smart `/add` truncation** — files over 500 lines auto-truncate to first 200 + last 100 lines with an omission marker; explicit line ranges bypass truncation (Day 56)
- **Zero production unwraps** — every `.unwrap()` across the entire codebase replaced with explicit error handling or recovery paths; completed across Days 51–55 (Day 55)
- **Poison-proof mutex/rwlock handling** — all `.lock().unwrap()` calls replaced with `lock_or_recover()` helper in `sync_util.rs` that recovers from poisoned mutexes instead of cascading panics; deduplicated from three files into one shared module (Days 52, 58)
- **Exit summary enriched** — session goodbye now includes elapsed time, token count, and estimated cost alongside file changes (Day 53)
- **`/diff --stat` flag** — compact one-line-per-file summary view for diffs (Day 53)
- **Tool output compression** — command-aware filtering collapses `Compiling`/`Downloading` sequences, npm/pip install noise, and consecutive blank lines into compact summaries (Day 50)
- **Live bash output expanded** — increased visible partial output lines from 3 to 6 during command execution, with hidden line count header (Day 51)
- **Analyze-trajectory improvements** — JSON contract for structured sub-agent diagnoses, token-aware chunking for large CI logs, and improved fingerprint clustering that strips timestamps/job names (Days 58–59)

### Changed (Internal / Architecture)

- **10 module extractions** across the consolidation arc:
  - `agent_builder.rs` from `main.rs` — agent construction, MCP collision detection, fallback retry logic (Day 58)
  - `watch.rs` from `prompt.rs` — watch mode, multi-phase auto-fix loop (Day 58)
  - `commands_run.rs` from `commands_dev.rs` — `/loop` and `/run` handlers (Day 59)
  - `safety.rs` from `tools.rs` — bash command safety analysis (Day 54)
  - `session.rs` from `prompt.rs` — session tracking types and change recording (Day 54)
  - `prompt_budget.rs` from `prompt.rs` — wall-clock budget and audit log helpers (Day 54)
  - `sync_util.rs` (new) — shared `lock_or_recover` for poisoned mutex recovery (Day 58)
  - `dispatch.rs` from `repl.rs` — slash command dispatch with `DispatchContext` (Day 55)
  - `help.rs` expansion — 500 lines of help text moved from `cli.rs` (Day 57)
  - `format/output.rs` and `format/diff.rs` from `format/mod.rs` — split 3,092-line file into focused modules (Day 53)
- **`main.rs` reduced from ~2,484 to ~861 lines** — setup, restore, and agent-building logic extracted (Days 57–58)
- **`prompt.rs` reduced from ~3,063 to ~2,174 lines** — watch, session, and budget code extracted (Days 54, 58)
- **yoagent upgraded to 0.8** — one-line version bump, no breaking changes (Day 58)
- **Consolidated watch-fix loop** — 55-line inline copy in `repl.rs` replaced with 9-line call to shared `run_watch_after_prompt` returning `WatchResult` struct (Day 60)
- **LazyLock regex compilation** — 25 regex patterns in `commands_map.rs` now compile once via `LazyLock` instead of per-call (Day 58)

### Fixed

- **UTF-8 safety in refactor commands** — 12 byte-indexing operations in `commands_refactor.rs` replaced with `is_char_boundary()` checks to prevent panics on multi-byte characters; 13 new tests with CJK/emoji strings (Day 53)
- **Home directory hang** — file listing now caps at 10,000 files and skips `node_modules`, `__pycache__`, `venv`, and other common large directories when outside a git repo (Issue #333, Day 55)
- **Missing DAY_COUNT in release builds** — `DAY_COUNT` now baked in at compile time via `build.rs` so release binaries show the correct evolution day (Issue #331, Day 55)
- **Integration tests burning 2.5 min per CI run** — two tests tried to connect to non-existent ollama, timing out with retries; switched to `--print-system-prompt` for instant exit (Day 51)
- **CWD race condition in test suite** — eliminated all `set_current_dir` calls from `commands_config.rs` and `commands_session.rs` tests by extracting `_in(root)` variants that take explicit paths (Day 51)
- **Flaky `build_repo_map_with_regex_backend` test** — fixed CWD race with explicit directory handling (Day 51)

## [0.1.8] — 2026-04-19

Day 50 milestone release — 51 commits spanning Days 36–49. Background processes, colorized blame, proper unified diffs, deep lint subcommands, and 23 shell subcommands wired for direct CLI invocation.

### Added

- **`/bg` background process management** — launch, list, view output, and kill background jobs with persistent tracker (Day 45)
- **`/blame` with colorized output** — git blame with syntax-highlighted annotations (Day 48)
- **`/changelog` command** — view recent evolution history from the terminal (Day 44)
- **`/lint fix`** — auto-fix lint warnings (Day 46)
- **`/lint pedantic`** — extra-strict lint pass (Day 46)
- **`/lint strict`** — deny all warnings during lint (Day 46)
- **`/lint unsafe`** — scan for unsafe code usage (Day 46)
- **23 shell subcommands** — `help`, `version`, `setup`, `init`, `diff`, `commit`, `review`, `blame`, `grep`, `find`, `index`, `lint`, `test`, `doctor`, `map`, `tree`, `run`, `watch`, `status`, `undo`, `docs`, `update`, `pr` — all invocable directly from the shell without entering the REPL (Days 48–49)
- **Per-command bash timeout parameter** — `"timeout": N` (1–600 seconds) for individual bash tool calls (Day 44)
- **Co-authored-by trailer on `/commit`** — automatically credits the AI in git commit metadata (Day 43)

### Improved

- **Proper unified diffs (LCS-based)** — `edit_file` operations now show real unified diffs with context lines instead of walls of red/green (Day 48)
- **Comprehensive categorized help** — all 68+ REPL commands listed with descriptions, organized by category (Day 49)
- **Piped mode gracefully handles slash-command input** — no longer sends `/help` etc. to the model as a real prompt (Day 47)
- **Streaming output for `/run` and `/watch`** — live output rendering instead of buffered display (Day 45)
- **`/status` shows session elapsed time and turn count** — richer session awareness (Day 43)

### Fixed

- **Dead code and unused annotation cleanup** — removed stale `#[allow(dead_code)]` markers and unused code paths (Day 48)
- **Destructive-git-command guard in `run_git()`** — `#[cfg(test)]` guard prevents tests from accidentally committing/reverting in the real repo (Day 45)

## [0.1.7] — 2026-04-05

Patch release with critical bug fixes — UTF-8 crash prevention, Windows build support, and sub-agent security hardening.

### Fixed

- **UTF-8 panic in tool output** — `strip_ansi_codes` and `line_category` no longer crash on multi-byte characters; safe char-boundary checks throughout string processing (Issue #250, Day 36)
- **Windows build** — Unix-only `PermissionsExt` import in `/update` command now behind `#[cfg(unix)]`, allowing cross-platform compilation (Issue #248, Day 36)
- **Sub-agent directory restriction bypass** — sub-agents now inherit parent's directory restrictions via `ArcGuardedTool` wrapper (Day 35)
- **Audit timestamp** — replaced shell `date` call with pure Rust `chrono` for reliable audit logging (Day 35)

### Added

- **`--print-system-prompt` flag** — print the assembled system prompt and exit, for prompt transparency and debugging (Day 35)
- **`/context system` subcommand** — display system prompt broken into sections with line counts, token estimates, and previews (Day 35)
- **Fork-friendly infrastructure** — `scripts/common.sh` auto-detects repo owner/name, workflows parameterized for forks, new fork guide in docs (Day 35)
- **`--provider` typo warning** — warns when provider name looks like a misspelling of a known provider (Day 35)

## [0.1.6] — 2026-04-03

Feature release adding tab completion descriptions, release tooling, smarter context management, and code organization improvements — built across Days 34–35.

### Added

- **Tab completion with descriptions** — slash commands now show descriptions next to names in tab completion for faster command discovery (Issue #214, Day 34)
- **Release changelog extraction** — `scripts/extract_changelog.sh` pulls version sections from CHANGELOG.md; retroactively applied to all existing GitHub releases (Issue #240, Day 34)
- **Autocompact thrash detection** — stops wasting turns after two low-yield compactions and suggests `/clear` instead (Day 34)
- **Context window percentage** — color-coded context usage percentage in post-turn display: green ≤50%, yellow 51–80%, red >80% (Day 34)
- **Watch mode multi-attempt fix loop** — `/watch` now retries up to 3 fix attempts per failure, feeding the latest error output to each retry so the agent can adapt to new errors introduced by previous fixes (Day 35)

### Improved

- **Tool definitions extracted** — moved tool definitions from `main.rs` into `src/tools.rs` (1,088 lines), improving code organization and modularity (Day 34)

## [0.1.5] — 2026-04-01

Feature release adding provider failover reliability, AWS Bedrock support, structural repo mapping, and inline command hints — built across Days 29–32.

### Added

- **Startup update notification** — non-blocking check against GitHub releases on REPL startup; shows a yellow notification when a newer version exists; skipped in piped/prompt modes; disable with `--no-update-check` or `YOYO_NO_UPDATE_CHECK=1` (Day 32)
- **`/map` command** — structural repo map with ast-grep backend and regex fallback, showing file symbols and relationships (Day 29)
- **AWS Bedrock provider** — full end-to-end support with BedrockConverseStream for Claude 3 models via AWS credentials (Day 30)
- **REPL inline command hints** — type `/he` and see dimmed `lp — Show help` suggestions for faster command discovery (Day 30)
- **`--fallback` provider failover** — auto-switch to backup provider on API failure, with configurable provider priority (Day 31)

### Improved

- **Hook system extracted** — Hook trait, HookRegistry, AuditHook, ShellHook consolidated into `src/hooks.rs` for better modularity (Day 31)
- **Config loading consolidated** — single `load_config_file()` eliminates 3 redundant config reads and improves error handling (Day 31)

### Fixed

- **Permission prompt hidden behind spinner** — stop spinner before prompting to prevent UI interference (Issue #224) (Day 30)
- **MiniMax stream duplication** — exclude "stream ended" from auto-retry to prevent infinite loops (Issue #222) (Day 30)
- **`write_file` empty content** — validation + confirmation prompt for empty writes to prevent accidental data loss (Issues #218, #219) (Day 30)
- **`--fallback` in piped mode** — fallback retry now works in piped and --prompt modes, with proper non-zero exit codes on failure (Day 32, Issue #230)

## [0.1.4] — 2026-03-28

Feature release adding agent delegation, interactive questioning, task tracking, context management strategies, and provider resilience — built across Days 24–28.

### Added

- **SubAgentTool** — model can delegate complex subtasks to a fresh agent with its own context window, inheriting the parent's provider/model/key (Day 25)
- **AskUserTool** — model can ask directed questions mid-turn instead of guessing; only available in interactive mode (Day 25)
- **TodoTool** — agent-accessible task tracking during autonomous runs, shared state with `/todo` command (Day 26)
- **`--context-strategy <mode>`** — choose context management: `compaction` (default) or `checkpoint` for checkpoint-restart on overflow (Day 25)
- **Proactive context compaction** — 70% threshold check before prompt attempts to prevent context overflow errors (Day 24)
- **`~/.yoyo.toml` config path** — home directory config file now correctly searched alongside project-level `.yoyo.toml` (Day 27)
- **MiniMax provider** — option 11 in setup wizard via yoagent's `ModelConfig::minimax()` (Day 25)
- **MCP server config** — `--mcp` flag connects to Model Context Protocol servers via stdio transport; configurable in `.yoyo.toml` (Day 25)
- **Audit log** — `--audit` flag / `YOYO_AUDIT=1` env var records tool calls to `.yoyo/audit.jsonl` for debugging and transparency (Day 24)

### Improved

- **Stream error recovery** — auto-retry on transient errors including "overloaded", "stream ended", "unexpected eof", and "broken pipe" (Day 26)
- **`/tokens` display** — clearer context vs cumulative labeling for token usage (Day 25)
- **Bell suppression** — `YOYO_NO_BELL=1` env var suppresses terminal bell in CI/piped environments (Day 24)

### Fixed

- **Flaky todo tests** — isolated global state with `serial_test` crate to prevent test interference (Day 26)
- **`/web` panic** — non-ASCII HTML content no longer causes panics via `from_utf8_lossy` handling (Day 25)
- **Config path mismatch** — `~/.yoyo.toml` is now actually searched as documented (Day 27)

## [0.1.3] — 2026-03-24

Feature release adding file watching, structural search, refactoring tools, and piped-mode improvements — built across Days 22–24.

### Added

- **`/watch <command>`** — auto-run tests after every agent turn that modifies files (Day 23)
- **`/ast <pattern>`** — structural code search via ast-grep integration, graceful fallback when `sg` not installed (Day 24)
- **`/refactor` umbrella** — groups `/extract`, `/rename`, `/move` under one discoverable entry (Day 23)
- **`rename_symbol` agent tool** — model can do project-wide renames in a single tool call (Day 23)
- **Terminal bell notification** — rings `\x07` after operations >3s; disable with `--no-bell` or `YOYO_NO_BELL=1` (Day 23)
- **`system_prompt` and `system_file` keys** in `.yoyo.toml` config (Day 23)
- **Git-aware system prompt** — agent automatically sees current branch and dirty-file status (Day 23)

### Improved

- **Per-turn `/undo`** — undo individual agent turns instead of all-or-nothing (Day 22)
- **Onboarding wizard** — added Cerebras provider, XDG user-level config path option (Day 22)
- **Streaming latency** — tighter flush logic for digit-word and dash-word patterns (Day 23)

### Fixed

- **Suppressed partial tool output in piped/CI mode** — eliminates ~6500 noise lines from CI logs ([#172](https://github.com/yologdev/yoyo-evolve/issues/172))
- **Reduced tool output truncation** from 30K to 15K chars in piped mode — cuts context growth rate to prevent 400 errors ([#173](https://github.com/yologdev/yoyo-evolve/issues/173))

## [0.1.2] — 2026-03-22

Feature release adding per-command help, inline file mentions, new commands, and polished rendering — built across Days 20–22.

### Added

- **Per-command `/help <command>`** — detailed usage, examples, and flags for any slash command (Day 21)
- **`/grep` command** — direct file search from the REPL without an API round-trip (Day 21)
- **`/git stash` subcommand** — `save`, `pop`, `list`, `apply`, `drop` for git stash management (Day 21)
- **Inline `@file` mentions** — `@path` in prompts expands to file contents; supports line ranges `@file:10-20` and image files (Day 21)
- **First-run welcome & setup guide** — detects first run, shows welcome message, guides API key and model configuration (Day 22)
- **Visual section headers** — output hierarchy with section dividers for clearer structure (Day 22)

### Improved

- **Markdown rendering** — lists, italic, blockquotes, and horizontal rules now render properly with ANSI formatting (Day 21)
- **`/diff` with inline colored patches** — diff output shows +/- lines with red/green highlighting (Day 22)
- **Code block streaming** — token-by-token instead of line-buffered; tokens now flow immediately during code output (Day 21)
- **Architecture documentation** — Mermaid diagrams added to mdbook docs (Day 21)
- **`run_git()` helper deduplication** — consolidated repeated git command patterns into shared helper (Day 20)
- **`configure_agent()` provider setup deduplication** — cleaned up provider configuration logic (Day 20)
- **Tool output summaries** — richer context for `read_file`, `edit_file`, `search`, and `bash` tool results (Day 21)

### Fixed

- **Code block streaming buffering** — tokens inside code blocks now flow immediately instead of buffering entire lines (Day 21)
- **Missing transition separator** — added separator between thinking output and text response sections (Day 22)

## [0.1.1] — 2026-03-20

Bug fix release addressing two community-reported issues.

### Fixed

- **Image support broken via `/add`** — images added with `/add photo.png` were base64-encoded but injected as plain text content blocks instead of proper image content blocks, so the model couldn't actually see them. Now `/add` detects image files (JPEG, PNG, GIF, WebP) and sends them as real image blocks the model can interpret. Closes [#138](https://github.com/yologdev/yoyo-evolve/issues/138).
- **Streaming output appeared all at once** — three root causes fixed: (1) spinner stop had a race condition that could prevent the clear sequence from executing, now clears synchronously; (2) thinking tokens went to stdout causing interleaving with text, now routed to stderr; (3) no separator between thinking and text output, now inserts a newline on transition. Also reduced the line-start resolve threshold so common short first tokens flush immediately. Closes [#137](https://github.com/yologdev/yoyo-evolve/issues/137).

## [0.1.0] — 2026-03-19

The initial release. Everything below was built from scratch over 19 days of autonomous evolution, starting from a 200-line CLI example.

### Added

#### Core Agent Loop
- **Streaming text output** — tokens stream to the terminal as they arrive, not after completion
- **Multi-turn conversation** with full history tracking
- **Thinking/reasoning display** — extended thinking shown dimmed below responses
- **Automatic API retry** with exponential backoff (3 retries via yoagent)
- **Rate limit handling** — respects `retry-after` headers on 429 responses
- **Parallel tool execution** via yoagent 0.6's `ToolExecutionStrategy::Parallel`
- **Subagent spawning** — `/spawn` delegates focused tasks to a child agent with scoped context
- **Tool output streaming** — `ToolExecutionUpdate` events shown as they arrive

#### Tools
- `bash` — run shell commands with interactive confirmation
- `read_file` — read files with optional offset/limit
- `write_file` — create or overwrite files with content preview
- `edit_file` — surgical text replacement with colored inline diffs (red/green removed/added lines)
- `search` — regex-powered grep across files
- `list_files` — directory listing with glob filtering

#### REPL & Interactive Features
- **Interactive REPL** with rustyline — arrow keys, Ctrl-A/E/K/W, persistent history (`~/.local/share/yoyo/history`)
- **Tab completion** — slash commands, file paths, and argument-aware suggestions (model values, git subcommands, `/pr` subcommands)
- **Multi-line input** via backslash continuation and fenced code blocks
- **Markdown rendering** — incremental ANSI formatting: headers, bold, italic, code blocks with syntax-labeled headers, horizontal rules
- **Syntax highlighting** — language-aware ANSI coloring for Rust, Python, JS/TS, Go, Shell, C/C++, JSON, YAML, TOML
- **Braille spinner** animation while waiting for AI responses
- **Conversation bookmarks** — `/mark`, `/jump`, `/marks` to name and revisit points in a conversation
- **Conversation search** — `/search` with highlighted matches in results
- **Fuzzy file search** — `/find` with scoring, git-aware file listing, top-10 ranked results
- **Direct shell escape** — `/run <cmd>` and `!<cmd>` execute commands without an API round-trip
- **Elapsed time display** after each response, plus per-tool execution timing (`✓ (1.2s)`)

#### Git Integration
- Git branch display in REPL prompt
- `/diff` — full `git status` plus diff, with file-level insertion/deletion summary
- `/commit` — AI-generated commit messages from staged changes
- `/undo` — revert last commit, including cleanup of untracked files
- `/git` — shortcuts for `status`, `log`, `diff`, `branch`
- `/pr` — full PR workflow: `list`, `view`, `create [--draft]`, `diff`, `comment`, `checkout`
- `/review` — AI-powered code review of staged/unstaged changes against main
- `/changes` — show files modified (written/edited) during the current session

#### Project Tooling
- `/health` — run full build/test/clippy/fmt diagnostic for Rust, Node, Python, Go, and Make projects
- `/fix` — run the check gauntlet and auto-apply fixes for failures
- `/test` — auto-detect project type and run the right test command
- `/lint` — auto-detect project type and run the right linter
- `/init` — scan project structure and generate a starter YOYO.md context file
- `/index` — build a lightweight codebase index: file counts, language breakdown, key files
- `/docs` — quick documentation/API lookup without leaving the REPL
- `/tree` — project structure visualization

#### Session Management
- `/save` and `/load` — persist and restore conversation sessions as JSON
- `--continue/-c` — auto-load the most recent session on startup
- **Auto-save on exit** — sessions saved automatically on clean exit and crash recovery
- **Auto-compaction** at 80% context window usage, plus manual `/compact`
- `/tokens` — visual token usage bar with percentage
- `/cost` — per-model input/output/cache pricing breakdown
- `/status` — show current session state

#### Context & Memory
- **Project context files** — auto-loads YOYO.md, CLAUDE.md, and `.yoyo/instructions.md`
- **Git-aware context** — recently changed files injected into system prompt
- **Codebase indexing** — `/index` summarizes project structure for the agent
- **Project memories** — `/remember`, `/memories`, `/forget` for persistent cross-session notes stored in `.yoyo/memory.json`

#### Configuration
- **Config file support** — `.yoyo.toml` (per-project) and `~/.config/yoyo/config.toml` (global)
- `--model` / `/model` — select or switch models mid-session
- `--provider` / `/provider` — switch between 11 provider backends mid-session (Anthropic, OpenAI, Google, Ollama, z.ai, and more)
- `--thinking` / `/think` — toggle extended thinking level
- `--temperature` — sampling randomness control (0.0–1.0)
- `--max-tokens` — cap response length
- `--max-turns` — limit agent turns per prompt (useful for scripted runs)
- `--system` / `--system-file` — custom system prompts
- `--verbose/-v` — show full tool arguments and result previews
- `--output/-o` — pipe response to a file
- `--api-key` — pass API key directly instead of relying on environment
- `/config` — display all active settings

#### Permission System
- **Interactive tool approval** — confirm prompts for `bash`, `write_file`, and `edit_file` with content/diff preview
- **"Always" option** — persists per-session via `AtomicBool`, so you only approve once
- `--yes/-y` — auto-approve all tool executions
- `--allow` / `--deny` — glob-based allowlist/blocklist for tool patterns
- `--allow-dir` / `--deny-dir` — directory restrictions with canonicalized path checks preventing traversal
- `[permissions]` and `[directories]` config file sections
- Deny-overrides-allow policy

#### Extensibility
- **MCP server support** — `--mcp` connects to MCP servers via stdio transport
- **OpenAPI tool loading** — `--openapi <spec>` registers tools from OpenAPI specifications
- **Skills system** — `--skills <dir>` loads markdown skill files with YAML frontmatter

#### CLI Modes
- **Interactive REPL** — default mode with full feature set
- **Single-shot prompt** — `--prompt/-p "question"` for one-off queries
- **Piped/stdin mode** — reads from stdin when not a TTY, auto-disables colors
- **Color control** — `--no-color` flag, `NO_COLOR` env var, auto-detection for non-TTY

#### Other
- `--help` / `--version` / `/version` — CLI metadata
- `/help` — grouped command reference (Navigation, Git, Project, Session, Config)
- **Ctrl+C handling** — graceful interrupt
- **Unknown flag warnings** — instead of silent ignoring
- **Unambiguous prefix matching** for slash commands (with greedy-match fix)

### Architecture

The codebase evolved from a single 200-line `main.rs` to 12 focused modules (~17,400 lines):

| Module | Lines | Responsibility |
|--------|-------|----------------|
| `main.rs` | ~1,470 | Entry point, tool building, `AgentConfig`, model config |
| `cli.rs` | ~2,360 | CLI argument parsing, config file loading, conversation bookmarks |
| `commands.rs` | ~2,990 | Slash command dispatch and grouped `/help` |
| `commands_git.rs` | ~1,190 | Git commands: `/diff`, `/commit`, `/pr`, `/review`, `/changes` |
| `commands_project.rs` | ~1,950 | Project commands: `/health`, `/fix`, `/test`, `/lint`, `/init`, `/index` |
| `commands_session.rs` | ~465 | Session commands: `/save`, `/load`, `/compact`, `/tokens`, `/cost` |
| `docs.rs` | ~520 | `/docs` crate API lookup |
| `format.rs` | ~3,280 | Output formatting, ANSI colors, markdown rendering, syntax highlighting, cost tracking |
| `git.rs` | ~790 | Git operations: branch detection, diff handling, PR interactions |
| `memory.rs` | ~375 | Project memory system (`.yoyo/memory.json`) |
| `prompt.rs` | ~1,090 | System prompt construction, project context assembly |
| `repl.rs` | ~880 | REPL loop, input handling, tab completion |

### Testing

- **800 tests** (733 unit + 67 integration)
- Integration tests run the actual binary as a subprocess — dogfooding real invocations
- Coverage includes: CLI flag validation, command parsing, error quality, exit codes, output formatting, edge cases (1000-char model names, Unicode emoji in arguments), project type detection, fuzzy scoring, health checks, git operations, session management, markdown rendering, cost calculation, permission logic, and more
- Mutation testing infrastructure via `cargo-mutants` with threshold-based pass/fail

### Documentation

- **mdbook guide** at `docs/book/` covering installation, all CLI flags, every REPL command, multi-line input, models, system prompts, thinking, skills, sessions, context management, git integration, cost tracking, troubleshooting, and permissions
- Landing page at `docs/index.html`
- In-code `/help` with grouped categories

### Evolution Infrastructure

- **3-phase evolution pipeline** (`scripts/evolve.sh`): plan → implement → communicate
- **GitHub issue integration** — reads community issues, self-filed issues, and help-wanted labels
- **Journal** (`journals/JOURNAL.md`) — chronological log of every evolution session
- **Learnings** (`memory/learnings.jsonl`) — self-reflections archive (JSONL, append-only with timestamps and source attribution)
- **Skills** — structured markdown guides for self-assessment, evolution, communication, research, release, and social interaction
- **CI** — build, test, clippy (warnings as errors), fmt check on every push/PR

---

### Development Timeline

| Day | Highlights |
|-----|-----------|
| 0 | Born — 200-line CLI on yoagent |
| 1 | Panic fixes, `--help`/`--version`, multi-line input, `/save`/`/load`, Ctrl+C, git branch prompt, custom system prompts |
| 2 | Tool execution timing, `/compact`, `/undo`, `--thinking`, `--continue`, `--prompt`, auto-compaction, `format_token_count` fix |
| 3 | mdbook documentation, `/model` UX fix |
| 4 | Module split (cli, format, prompt), `--max-tokens`, `/version`, `NO_COLOR`, `--no-color`, `/diff` improvements, `/undo` cleanup |
| 5 | `--verbose`, `/init`, `/context`, YOYO.md/CLAUDE.md project context, `.yoyo.toml` config files, Claude Code gap analysis |
| 6 | `--temperature`, `/health`, `/think`, `--api-key`, `/cost` breakdown, `--max-turns`, partial tool streaming, CLI hardening |
| 7 | `/tree`, `/pr`, project file context in prompt, retry logic, `/search`, `/run` and `!` shell escape, mutation testing setup |
| 8 | Rustyline + tab completion, markdown rendering, file path completion, `/commit`, `/git`, spinner, multi-provider + MCP support |
| 9 | yoagent 0.6.0, `--openapi`, `/fix`, `/git diff`/`branch`, "always" confirm fix, multi-language `/health`, YOYO.md identity, safety docs |
| 10 | Integration tests (subprocess dogfooding), syntax highlighting, `/docs`, git module extraction, docs module extraction, commands module extraction, 49 subprocess tests |
| 11 | Main.rs extraction (3,400→1,800 lines), PR dedup, timing tests |
| 12 | `/test`, `/lint`, search highlighting, `/find`, git-aware context, code block highlighting, `AgentConfig`, `repl.rs` extraction, `/spawn` |
| 13 | `/review`, `/pr create`, `/init` onboarding, smarter `/diff`, main.rs final cleanup (770 lines) |
| 14 | Colored edit diffs, conversation bookmarks (`/mark`, `/jump`), argument-aware tab completion, `/index` codebase indexing |
| 15 | Permission prompts (all tools), project memories (`/remember`, `/memories`, `/forget`), module split (commands→4 files), grouped `/help`, `/provider` |
| 16 | Auto-save sessions on exit, crash recovery, documentation overhaul, CHANGELOG.md |
| 17 | True token-by-token streaming fix, multi-provider cost tracking (7 providers), crates.io package rename, pluralization fix, `/changes` command |
| 18 | z.ai (Zhipu AI) provider support, test backfill for `commands_git` and `commands_project` (1,118 lines of tests) |
| 19 | Published to crates.io as v0.1.0 🎉 |
| 20 | `run_git()` dedup, `configure_agent()` dedup, context overflow auto-recovery, v0.1.1 bug fix release |
| 21 | Per-command `/help <cmd>`, `/grep`, `/git stash`, inline `@file` mentions, markdown rendering (lists, italic, blockquotes), code block streaming fix, tool output summaries, architecture docs |
| 22 | First-run welcome & setup guide, `/diff` inline colored patches, visual section headers, v0.1.2 release |
| 23 | `/watch` auto-test, `/refactor` umbrella, `rename_symbol` tool, terminal bell, `system_prompt`/`system_file` config, git-aware prompt, streaming flush improvements |
| 24 | `/ast` structural search, piped-mode output fixes, v0.1.3 release |

[0.1.11]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.11
[0.1.10]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.10
[0.1.9]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.9
[0.1.8]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.8
[0.1.7]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.7
[0.1.6]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.6
[0.1.5]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.5
[0.1.4]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.4
[0.1.3]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.3
[0.1.2]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.2
[0.1.1]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.1
[0.1.0]: https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.0
