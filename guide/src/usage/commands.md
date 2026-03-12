# REPL Commands

All commands start with `/`. Type `/help` inside yoyo to see the full list.

## Navigation

| Command | Description |
|---------|-------------|
| `/quit`, `/exit` | Exit yoyo |
| `/help` | Show available commands |

## Conversation

| Command | Description |
|---------|-------------|
| `/clear` | Clear conversation history and start fresh |
| `/compact` | Compress conversation to save context space (see [Context Management](../features/context.md)) |
| `/retry` | Re-send your last input — useful when a response gets cut off or you want to try again |
| `/history` | Show a summary of all messages in the conversation |
| `/search <query>` | Search conversation history for messages containing the query (case-insensitive) |

## Model & Thinking

| Command | Description |
|---------|-------------|
| `/model <name>` | Switch to a different model (preserves conversation) |
| `/think [level]` | Show or change thinking level: `off`, `minimal`, `low`, `medium`, `high` |

Examples:
```
/model claude-sonnet-4-20250514
/think high
/think off
```

The `/think` command preserves your conversation history — only the thinking level changes. The `/model` command also preserves conversation.

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

The `/run` command (or `!` shortcut) lets you execute shell commands without going through the AI model. Useful for quick checks (e.g., `!git log --oneline -5`) without burning API tokens.

```
/run ls -la src/
/run cargo test
/run git status
```

## Subagent

| Command | Description |
|---------|-------------|
| `/spawn <task>` | Spawn a subagent with a fresh context to handle a task |

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
| `/commit [msg]` | Commit staged changes — generates a conventional commit message if no msg provided |
| `/diff` | Show `git status --short` and `git diff --stat` of uncommitted changes |
| `/undo` | Revert all uncommitted changes (`git checkout -- .` and `git clean -fd`) |
| `/pr [number]` | List open PRs (`gh pr list`), or view a specific PR (`gh pr view <number>`) |
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
```

The `/commit` command helps you commit staged changes quickly:
- `/commit` (no arguments): reads your staged diff, generates a conventional commit message (e.g., `feat(main): add changes`), and asks for confirmation — press `y` to accept, `n` to cancel, or `e` to edit
- `/commit fix: typo in README`: commits directly with your provided message
- If nothing is staged, it reminds you to `git add` first

The `/undo` command shows you what will be reverted before doing it.

The `/pr` command is a quick wrapper around the [GitHub CLI](https://cli.github.com):

- `/pr` — list the 10 most recent open pull requests
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

## Project Context

| Command | Description |
|---------|-------------|
| `/context` | Show which project context files are loaded (YOYO.md is primary; CLAUDE.md supported for compatibility) |
| `/init` | Create a starter YOYO.md project context file |
| `/tree [depth]` | Show project directory tree (default depth: 3, respects `.gitignore`) |

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

## Unknown commands

If you type a `/command` that yoyo doesn't recognize, it will tell you:

```
  unknown command: /foo
  type /help for available commands
```

Note: lines starting with `/` that contain spaces (like `/model name`) are treated as command arguments, not unknown commands.
