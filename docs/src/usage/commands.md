# REPL Commands

All commands start with `/`. Type `/help` inside yoyo to see the full list.

## Navigation

| Command | Description |
|---------|-------------|
| `/quit`, `/exit` | Exit yoyo |
| `/help` | Show available commands |
| `/help <command>` | Show detailed help for a specific command |

## Conversation

| Command | Description |
|---------|-------------|
| `/clear` | Clear conversation history and start fresh |
| `/compact` | Compress conversation to save context space (see [Context Management](../features/context.md)) |
| `/retry` | Re-send your last input — useful when a response gets cut off or you want to try again |
| `/history` | Show a summary of all messages in the conversation |
| `/search <query>` | Search conversation history for messages containing the query (case-insensitive) |
| `/mark <name>` | Bookmark the current conversation state |
| `/jump <name>` | Restore conversation to a bookmark (discards messages after it) |
| `/marks` | List all saved bookmarks |

### Conversation bookmarks

The `/mark` and `/jump` commands let you bookmark points in your conversation and return to them later. This is useful when exploring different approaches — bookmark a good state, try something, and jump back if it doesn't work out.

```
> /mark before-refactor
  ✓ bookmark 'before-refactor' saved (12 messages)

> ... try something risky ...

> /jump before-refactor
  ✓ jumped to bookmark 'before-refactor' (12 messages)

> /marks
  Saved bookmarks:
    • before-refactor
```

Bookmarks are stored in memory for the current session. Overwriting a bookmark with the same name updates it. Jumping to a bookmark restores the conversation to exactly that point — any messages added after the bookmark are discarded.

## Model, Provider & Thinking

| Command | Description |
|---------|-------------|
| `/model <name>` | Switch to a different model (preserves conversation) |
| `/provider <name>` | Switch provider and reset model to the provider's default |
| `/think [level]` | Show or change thinking level: `off`, `minimal`, `low`, `medium`, `high` |

Examples:
```
/model claude-sonnet-4-20250514
/provider openai
/provider google
/think high
/think off
```

The `/model` command preserves conversation when switching models. The `/provider` command switches to a different API provider (e.g., `anthropic`, `openai`, `google`, `openrouter`, `ollama`, `xai`, `groq`, `deepseek`, `mistral`, `cerebras`, `custom`) and automatically sets the model to the provider's default. Use `/provider` without arguments to see the current provider and available options. The `/think` command adjusts the thinking level.

## Session

| Command | Description |
|---------|-------------|
| `/save [path]` | Save conversation to a file (default: `yoyo-session.json`) |
| `/load [path]` | Load conversation from a file (default: `yoyo-session.json`) |

See [Session Persistence](../features/sessions.md) for details.

## Information

| Command | Description |
|---------|-------------|
| `/status` | Show current model, git branch, working directory, and session token totals |
| `/tokens` | Show detailed token usage: context window fill level, session totals, and estimated cost |
| `/cost` | Show estimated session cost |
| `/config` | Show all current settings |
| `/version` | Show yoyo version |

The `/tokens` command shows a visual progress bar of your context window:

```
  Context window:
    messages:    12
    context:     45.2k / 200.0k tokens
    █████████░░░░░░░░░░░ 23%
```

## Documentation

| Command | Description |
|---------|-------------|
| `/docs <crate>` | Look up docs.rs documentation for a Rust crate |
| `/docs <crate> <item>` | Look up a specific module/item within a crate |

The `/docs` command fetches the docs.rs page for a given crate and shows a quick summary — confirming the crate exists, displaying its description, and listing the crate's API items (modules, structs, traits, enums, functions, macros). No tokens used, no AI involved.

Each category is capped at 10 items with a "+N more" suffix for large crates.

```
/docs serde
  ✓ serde
  📦 https://docs.rs/serde/latest/serde/
  📝 A generic serialization/deserialization framework

  Modules: de, ser
  Traits: Deserialize, Deserializer, Serialize, Serializer
  Macros: forward_to_deserialize_any

/docs tokio task
  ✓ tokio::task
  📦 https://docs.rs/tokio/latest/tokio/task/
  📝 Asynchronous green-threads...
```

## Shell

| Command | Description |
|---------|-------------|
| `/run <cmd>` | Run a shell command directly — no AI, no tokens used |
| `!<cmd>` | Shortcut for `/run` |
| `/web <url>` | Fetch a web page and display clean readable text content |

The `/run` command (or `!` shortcut) lets you execute shell commands without going through the AI model. Useful for quick checks (e.g., `!git log --oneline -5`) without burning API tokens.

```
/run ls -la src/
/run cargo test
/run git status
```

### `/web` — Fetch and read web pages

The `/web` command fetches a URL and extracts readable text content, stripping away HTML tags, scripts, styles, and navigation. This is useful for quickly pulling in documentation, error explanations, API references, or any web content without getting raw HTML.

```
/web https://doc.rust-lang.org/book/ch01-01-installation.html
/web docs.rs/serde
/web https://stackoverflow.com/questions/12345
```

Features:
- **Auto-prepends `https://`** if you omit the protocol — `/web docs.rs/serde` works
- **Strips noise** — removes `<script>`, `<style>`, `<nav>`, `<footer>`, `<header>`, and `<svg>` blocks
- **Converts structure** — headings become prominent, list items get bullets, block elements get newlines
- **Decodes entities** — `&amp;`, `&lt;`, `&gt;`, `&#NNN;`, `&nbsp;`, etc.
- **Truncates** — caps output at ~5,000 characters to keep it readable
- **No AI tokens used** — pure curl + text extraction

## Subagent & Planning

| Command | Description |
|---------|-------------|
| `/plan <task>` | Create a step-by-step plan for a task without executing anything (architect mode) |
| `/spawn <task>` | Spawn a subagent with a fresh context to handle a task |

### `/plan` — Architect mode

The `/plan` command asks the AI to create a detailed, structured plan for a task **without executing any tools**. This is the "architect mode" equivalent — you see exactly what the agent intends to do before it does anything.

```
> /plan add caching to the database layer

  📋 Planning: add caching to the database layer

  ## Files to examine
  - src/db.rs — current database implementation
  - src/config.rs — configuration for cache TTL

  ## Files to modify
  - src/db.rs — add cache layer
  - src/cache.rs — new file for cache implementation
  - tests/cache_test.rs — new tests

  ## Step-by-step approach
  1. Read src/db.rs to understand current query patterns
  2. Create src/cache.rs with an LRU cache struct
  3. Wrap database queries with cache lookups
  4. Add cache invalidation on writes
  5. Add configuration for cache size and TTL

  ## Tests to write
  - Cache hit returns cached value
  - Cache miss falls through to database
  - Write invalidates relevant cache entries

  ## Potential risks
  - Cache invalidation on complex queries
  - Memory pressure with large result sets

  ## Verification
  - Run existing tests to ensure no regressions
  - Run new cache tests
  - Benchmark query latency before/after

  💡 Review the plan above. Say "go ahead" to execute it, or refine it.
```

After reviewing the plan, you can:
- Say **"go ahead"** to have the agent execute the plan
- Ask the agent to **refine** specific parts ("make the cache configurable")
- **Modify** the approach ("use Redis instead of in-memory")
- Say **"no"** or change direction entirely

This is especially useful for:
- **Large refactors** where you want to understand the scope before committing
- **Unfamiliar codebases** where you want the agent to map things out first
- **Trust and transparency** — see the full plan before any files are modified
- **Teaching moments** — the plan itself teaches you about the codebase structure

### `/spawn` — Subagent

The `/spawn` command creates a fresh AI agent with its own independent context window, sends it your task, runs it to completion, and injects the result back into your main conversation.

This is useful for tasks that would consume a lot of context in your main session — reading large files, multi-step analysis, exploring unfamiliar code — without polluting your primary conversation history.

```
/spawn read all files in src/ and summarize the architecture
/spawn find all TODO comments in the codebase and list them
/spawn analyze the test coverage and suggest gaps
```

The subagent has access to the same tools (bash, file operations, etc.) and uses the same model. Its token usage counts toward your session total, but its context is completely separate from your main conversation. When it finishes, a summary of the task and result is injected into your main conversation so you have awareness of what was done.

## Git

| Command | Description |
|---------|-------------|
| `/git status` | Show working tree status (`git status --short`) — quick shortcut |
| `/git log [n]` | Show last n commits (default: 5) via `git log --oneline` |
| `/git add <path>` | Stage files for commit |
| `/git stash` | Stash uncommitted changes |
| `/git stash pop` | Restore stashed changes |
| `/git stash list` | List all stash entries with colored output |
| `/git stash show [n]` | Show diff of stash entry (default: latest) |
| `/git stash drop [n]` | Drop a stash entry (default: latest) |
| `/commit [msg]` | Commit staged changes — generates a conventional commit message if no msg provided |
| `/diff` | Show colored file summary, change stats, and full diff of uncommitted changes |
| `/undo` | Revert all uncommitted changes (`git checkout -- .` and `git clean -fd`) |
| `/pr [number]` | List open PRs (`gh pr list`), or view a specific PR (`gh pr view <number>`) |
| `/pr create [--draft]` | Create a PR with an AI-generated title and description |
| `/pr <number> diff` | Show the diff of a PR (`gh pr diff <number>`) |
| `/pr <number> comment <text>` | Add a comment to a PR (`gh pr comment <number>`) |
| `/pr <number> checkout` | Checkout a PR branch locally (`gh pr checkout <number>`) |
| `/health` | Run project health checks — auto-detects project type, reports pass/fail with timing |
| `/test` | Auto-detect and run project tests — shows output with timing |
| `/lint` | Auto-detect and run project linter — shows output with timing |
| `/fix` | Auto-fix build/lint errors — runs health checks, sends failures to the AI agent for fixing |

The `/git` command is a convenience wrapper for common git operations without burning AI tokens or using `/run git ...`. For example:

```
/git status          # instead of /run git status --short
/git log 10          # instead of /run git log --oneline -10
/git add src/main.rs # stage a file
/git stash           # stash changes
/git stash pop       # restore stash
/git stash list      # see all stash entries
/git stash show 1    # view diff of stash@{1}
/git stash drop 0    # drop the latest stash
```

The `/commit` command helps you commit staged changes quickly:
- `/commit` (no arguments): reads your staged diff, generates a conventional commit message (e.g., `feat(main): add changes`), and asks for confirmation — press `y` to accept, `n` to cancel, or `e` to edit
- `/commit fix: typo in README`: commits directly with your provided message
- If nothing is staged, it reminds you to `git add` first

The `/undo` command shows you what will be reverted before doing it.

The `/pr` command is a quick wrapper around the [GitHub CLI](https://cli.github.com):

- `/pr` — list the 10 most recent open pull requests
- `/pr create` — create a PR with an AI-generated title and description from your branch's diff and commits
- `/pr create --draft` — same, but as a draft PR
- `/pr 42` — view details of PR #42
- `/pr 42 diff` — show the diff for PR #42
- `/pr 42 comment looks good!` — add a comment to PR #42
- `/pr 42 checkout` — checkout PR #42's branch locally

For merging or closing PRs, use `/run gh pr ...` or ask the agent directly — it has full bash access.

The `/health` command auto-detects your project type by looking for marker files and runs the appropriate checks:

- **Rust** (`Cargo.toml`): `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt --check`
- **Node.js** (`package.json`): `npm test`, `npx eslint .`
- **Python** (`pyproject.toml`, `setup.py`, `setup.cfg`): `pytest`, `flake8`, `mypy`
- **Go** (`go.mod`): `go build`, `go test`, `go vet`
- **Makefile** (`Makefile`): `make test`

If no recognized project type is found, it shows a helpful message listing the marker files it looked for.

The `/test` command is a focused shortcut that only runs the test suite for your project (e.g., `cargo test`, `npm test`, `python -m pytest`, `go test ./...`, `make test`). It auto-detects the project type the same way `/health` does, but runs just the tests — with full output and timing. This is handy for a quick test run without the full suite of lint/build checks that `/health` performs.

The `/lint` command is similar to `/test` but runs only the linter for your project. It auto-detects the project type and runs the appropriate linter:

- **Rust**: `cargo clippy --all-targets -- -D warnings`
- **Node.js**: `npx eslint .`
- **Python**: `ruff check .`
- **Go**: `golangci-lint run`

No AI involvement — just runs the linter and shows the output with timing. For auto-fixing lint errors with AI help, use `/fix` instead.

The `/fix` command goes one step further than `/health` — it runs the same health checks, but when any check fails, it sends the full error output to the AI agent with a prompt to fix the issues. The AI reads the relevant files, understands the errors, and applies fixes using its tools. After fixing, it re-runs the checks to verify. This is particularly useful for quickly resolving lint warnings, format issues, or build errors.

```
/fix
  Detected project: Rust (Cargo)
  Running health checks...
  ✓ build: ok
  ✗ clippy: FAIL
  ✓ fmt: ok

  Sending 1 failure(s) to AI for fixing...
```

## Code Review

| Command | Description |
|---------|-------------|
| `/review` | AI-powered review of staged changes (falls back to unstaged if nothing staged) |
| `/review <path>` | AI-powered review of a specific file |

The `/review` command sends your code to the AI for a thorough review covering:

1. **Bugs** — logic errors, off-by-one errors, null handling, race conditions
2. **Security** — injection vulnerabilities, unsafe operations, credential exposure
3. **Style** — naming, idiomatic patterns, unnecessary complexity, dead code
4. **Performance** — obvious inefficiencies, unnecessary allocations
5. **Suggestions** — improvements, missing error handling, better approaches

```
/review              # review staged changes (or unstaged if nothing staged)
/review src/main.rs  # review a specific file
/review Cargo.toml   # review any file
```

This is one of the most common workflows for developers using coding agents — getting a second pair of eyes on your changes before committing.

## Refactoring

| Command | Description |
|---------|-------------|
| `/rename <old> <new>` | Cross-file symbol renaming with word-boundary matching |
| `/extract <symbol> <source> <target>` | Move a symbol (fn, struct, enum, trait, type, const, static) between files |

### `/rename` — Cross-file symbol renaming

The `/rename` command does a smart find-and-replace across all git-tracked files, respecting word boundaries (renaming `foo` won't change `foobar` or `my_foo`). Shows a preview of all matches, then asks for confirmation.

```
/rename my_func new_func
/rename OldStruct NewStruct
```

### `/extract` — Move symbols between files

The `/extract` command moves a top-level item (function, struct, enum, impl, trait, type alias, const, or static) from one file to another. It uses brace-depth tracking to find the full block, including doc comments and attributes above the declaration.

```
/extract my_func src/lib.rs src/utils.rs
/extract MyStruct src/main.rs src/types.rs
/extract MyTrait src/old.rs src/new.rs
/extract MyResult src/lib.rs src/errors.rs
/extract MAX_SIZE src/config.rs src/constants.rs
```

The command shows a preview of the block to be moved and asks for confirmation before making changes. If the target file doesn't exist, it's created. If the symbol is public, yoyo notes that you may need to add a `use` import in the source file.

## Project Context

| Command | Description |
|---------|-------------|
| `/add <path>` | Add file contents into the conversation — the AI sees them immediately |
| `/context` | Show which project context files are loaded (YOYO.md is primary; CLAUDE.md supported for compatibility) |
| `/find <pattern>` | Fuzzy-search project files by name — respects `.gitignore`, ranked by relevance |
| `/grep <pattern> [path]` | Search file contents directly — no AI, no tokens, instant results |
| `/index` | Build a lightweight index of all project source files — shows path, line count, and first-line summary |
| `/init` | Scan the project and generate a YOYO.md context file with detected build commands, key files, and project structure |
| `/tree [depth]` | Show project directory tree (default depth: 3, respects `.gitignore`) |

### `/add` — Inject file contents into conversation

The `/add` command reads files and injects their contents directly into the conversation as a user message. The AI sees the file immediately without needing to call `read_file` — similar to Claude Code's `@file` feature.

```
/add src/main.rs
  ✓ added src/main.rs (850 lines)
  (1 file added to conversation)

/add src/main.rs:1-50
  ✓ added src/main.rs (lines 1-50) (50 lines)
  (1 file added to conversation)

/add src/*.rs
  ✓ added src/cli.rs (400 lines)
  ✓ added src/commands.rs (3000 lines)
  ✓ added src/main.rs (850 lines)
  (3 files added to conversation)

/add Cargo.toml README.md
  ✓ added Cargo.toml (28 lines)
  ✓ added README.md (50 lines)
  (2 files added to conversation)
```

Features:
- **Line ranges** — `/add path:start-end` injects only the specified lines
- **Glob patterns** — `/add src/*.rs` expands to all matching files
- **Multiple files** — `/add file1 file2` adds both in one message
- **Syntax highlighting** — content is wrapped in fenced code blocks with language detection
- **No AI tokens used for reading** — the file is read locally and injected directly

This is the fastest way to give the AI context about specific files without waiting for it to call tools.

The `/find` command does fuzzy substring matching across all tracked files in your project (via `git ls-files`, falling back to a directory walk if not in a git repo). Results are ranked by relevance — filename matches score higher than directory matches, and matches at the start of the filename rank highest.

```
/find main
  3 files matching 'main':
    src/main.rs
    site/book/index.html
    scripts/main_helper.sh

/find .toml
  2 files matching '.toml':
    Cargo.toml
    docs/book.toml
```

### `/grep` — Search file contents directly

The `/grep` command searches file contents without using the AI — no tokens, no API call, instant results. This is one of the fastest ways to find code in your project.

```
/grep TODO
  src/main.rs:42: // TODO: handle edge case
  src/cli.rs:15: // TODO: add validation
  
  2 matches

/grep "fn main" src/
  src/main.rs:10: fn main() {
  
  1 match

/grep -s MyStruct src/lib.rs
  src/lib.rs:5: pub struct MyStruct {
  src/lib.rs:20: impl MyStruct {
  
  2 matches
```

Features:
- **Case-insensitive by default** — use `-s` or `--case` for case-sensitive search
- **Git-aware** — uses `git grep` in git repos (faster, respects `.gitignore`), falls back to `grep -rn`
- **Colored output** — filenames in green, line numbers in cyan, matches highlighted in yellow
- **Truncated results** — shows up to 50 matches with a "narrow your search" hint
- **Optional path** — `/grep pattern src/` restricts search to a specific file or directory

The `/tree` command uses `git ls-files` to show tracked files in a visual tree structure, automatically respecting your `.gitignore`. You can specify a depth limit:

```
/tree        # default: 3 levels deep
/tree 1      # just top-level directories and their files
/tree 5      # deeper view
```

Example output:
```
src/
  cli.rs
  format.rs
  main.rs
  prompt.rs
Cargo.toml
README.md
```

### `/index` — Codebase indexing

The `/index` command builds a lightweight in-memory index of your project's source files. For each text file tracked by git (or found via directory walk), it shows:

- **Path** — the file path relative to the project root
- **Lines** — the total line count
- **Summary** — the first meaningful line (skipping blank lines), which is typically a doc comment, module declaration, or import statement

Binary files (images, fonts, archives, etc.) are automatically skipped.

```
/index
  Building project index...
  Path                Lines  Summary
  ──────────────────  ─────  ────────────────────────────────────────
  Cargo.toml             18  [package]
  src/cli.rs            400  //! CLI argument parsing and configuration.
  src/commands.rs      4500  //! REPL command handlers for yoyo.
  src/main.rs           850  //! yoyo — a coding agent that evolves itself.
  README.md              50  # yoyo

  5 files, 5818 total lines
```

This gives you a quick bird's-eye view of the entire codebase without needing to run `find`, `list_files`, or `wc -l` manually.

## Project Onboarding with `/init`

The `/init` command scans your project and generates a `YOYO.md` context file automatically. It:

1. **Detects the project type** — Rust, Node.js, Python, Go, or Makefile-based projects
2. **Finds the project name** — from `Cargo.toml`, `package.json`, `README.md` title, or directory name
3. **Lists important files** — README, config files, CI configs, lock files, etc.
4. **Lists key directories** — `src/`, `tests/`, `docs/`, `scripts/`, etc.
5. **Generates build commands** — `cargo build`, `npm test`, `go test ./...`, etc. based on project type

```
/init
  Scanning project...
  Detected: Rust
  ✓ Created YOYO.md (32 lines) — edit it to add project context.
```

If `YOYO.md` or `CLAUDE.md` already exists, `/init` won't overwrite it. The generated file is a starting point — edit it to add your project's specific conventions and instructions.

## Project Memory

| Command | Description |
|---------|-------------|
| `/remember <note>` | Save a project-specific note that persists across sessions |
| `/memories` | List all saved project memories |
| `/forget <number>` | Remove a memory by its number |

Project memories let you teach yoyo things about your project that it should always know — build quirks, team conventions, infrastructure requirements. Memories are stored in `.yoyo/memory.json` in your project root and are automatically injected into the system prompt at the start of every session.

### Example workflow

```
> /remember this project uses sqlx for database access
  ✓ Remembered: "this project uses sqlx for database access" (1 total memories)

> /remember tests require docker running
  ✓ Remembered: "tests require docker running" (2 total memories)

> /memories
  Project memories (2):
    [0] this project uses sqlx for database access (2026-03-15 08:32)
    [1] tests require docker running (2026-03-15 08:33)

> /forget 0
  ✓ Forgot: "this project uses sqlx for database access" (1 memories remaining)
```

Use `/remember` any time you find yourself repeating the same instruction to the agent. The memory will be there next time you start a session in this project directory.

## Unknown commands

If you type a `/command` that yoyo doesn't recognize, it will tell you:

```
  unknown command: /foo
  type /help for available commands
```

Note: lines starting with `/` that contain spaces (like `/model name`) are treated as command arguments, not unknown commands.
