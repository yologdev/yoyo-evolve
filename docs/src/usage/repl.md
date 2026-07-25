# Interactive Mode (REPL)

Interactive mode is the default when you run yoyo in a terminal. It gives you a read-eval-print loop where you can have a multi-turn conversation with the agent.

## Starting

```bash
yoyo
# or
cargo run
```

## The prompt

The prompt shows your current git branch (if you're in a git repo):

```
main 🐙 › _
```

If you're not in a git repo, you get a plain prompt:

```
🐙 › _
```

## Line editing & history

yoyo uses [rustyline](https://crates.io/crates/rustyline) for a full readline experience:

- **Arrow keys**: Navigate within the current line (← →) and through command history (↑ ↓)
- **Inline hints**: As you type a slash command, a dimmed suggestion appears after the cursor showing the completion and a short description — e.g. typing `/he` shows `lp — Show help for commands`. Press Tab or → to accept.
- **Tab completion**: Type `/` and press Tab to see available slash commands with descriptions — each command is shown alongside a short summary of what it does. Partial matches work too — `/he<Tab>` suggests `/help` and `/health`. After typing a command + space, argument-aware completions kick in:
  - `/model <Tab>` — suggests known model names (Claude, GPT, Gemini, etc.)
  - `/provider <Tab>` — suggests known provider names (anthropic, openai, google, etc.)
  - `/think <Tab>` — suggests thinking levels (off, minimal, low, medium, high)
  - `/git <Tab>` — suggests git subcommands (status, log, add, diff, branch, stash)
  - `/pr <Tab>` — suggests PR subcommands (list, view, diff, comment, create, checkout)
  - `/save <Tab>` and `/load <Tab>` — suggest `.json` session files in the current directory
  - File paths also complete — type `src/ma<Tab>` to get `src/main.rs`, or `Cargo<Tab>` to get `Cargo.toml`. Directories complete with a trailing `/` for easy continued navigation.
- **History recall**: Previous inputs are saved across sessions
- **Keyboard shortcuts**: Ctrl-A (start of line), Ctrl-E (end of line), Ctrl-K (kill to end), Ctrl-W (delete word back)
- **History file**: Stored at `$XDG_DATA_HOME/yoyo/history` (defaults to `~/.local/share/yoyo/history`)

## How it works

1. You type a message
2. yoyo sends it to the LLM along with conversation history
3. The LLM may call tools (read files, run commands, etc.)
4. Tool results are streamed back — you see each tool as it executes
5. The final text response is printed
6. Token usage and cost are shown after each turn

### Auto-continue

If the model stops mid-work (e.g., it says "Next, I'll fix the tests..." but stops), yoyo automatically sends a follow-up prompt to continue. You'll see:

```
  ⚡ auto-continuing (1/3 — response appears incomplete)...
```

This happens up to 3 times per user turn. Auto-continue won't fire if:
- The model encountered an error
- The session budget is exhausted
- The response doesn't show clear signs of being incomplete

### Turn-end marker

When a turn actually did work (it changed at least one file), yoyo prints one
dim line to stderr saying *why it believes it stopped*. It's a stated belief,
not ground truth — but a stated one can be argued with:

```
  ✓ done — nothing queued
  ⏸ stopped with no summary — type "continue" if this looks unfinished
  ⏸ auto-continue budget spent (3/3) — type "continue" to resume
```

- **done** — nothing pending, no error, and the turn closed with a real summary.
- **stopped** — the turn hit an error, still has queued follow-up work, or went
  quiet right after tool activity without wrapping up. Send `continue` if the
  work looks unfinished.
- **budget spent** — auto-continue used all its attempts while work was still
  queued.

The marker is *only informational* — it never changes what yoyo does. It's
suppressed for plain conversational replies (no file changes), silent under
`--quiet`, and prints glyph-free wording under `--screen-reader`.

## Shell passthrough

Prefix a line with `!` to run a shell command directly — no API call, no tokens, no confirmation prompt:

```
🐙 › !git status
🐙 › !ls -la src/
```

Output streams live to your terminal (via `sh -c` on Unix, `cmd /C` on Windows), and the exit code is shown only when non-zero. A bare `!` prints a usage hint.

### `!?` — ask about the last command's output

`!` commands never enter the conversation, but sometimes you *want* yoyo to look at what just happened. Type `!?` to feed the last `!` command — its command line, exit code, and output tail (last ~200 lines / 8KB, captured as a tee while it streamed) — into the conversation:

```
🐙 › !cargo test
   ... (output streams live) ...
   exit 101
   command failed (exit 101) — type !? to ask yoyo about the output
🐙 › !? why did the second test fail
```

- Bare `!?` uses a default question: after a failure, "explain what went wrong and how to fix it"; after a success, "summarize this output".
- `!? <question>` asks your own question about the output.
- Works after successful commands too (the failure hint just doesn't appear).
- If no `!` command has run yet, `!?` prints a one-liner and does nothing.
- The capture is non-consuming — you can ask `!?` follow-ups about the same output more than once.

This is fully opt-in: nothing enters the conversation (and no tokens are spent) unless you type `!?`.

## Tool output

When yoyo uses tools, you'll see status indicators:

```
  ▶ $ cargo test ✓ (2.1s)
  ▶ read src/main.rs ✓ (42ms)
  ▶ edit src/lib.rs ✓ (15ms)
  ▶ $ cargo test ✗ (1.8s)
```

- `✓` means the tool succeeded
- `✗` means the tool returned an error
- The duration shows how long the tool took

## Token usage

After each response, you'll see a compact token summary:

```
  ↳ 3.2s · 1523→842 tokens · $0.0234
```

Use `--verbose` (or `-v`) for the full breakdown including session totals and cache info.

This shows:
- Wall-clock time for the response
- Input→output tokens for this turn
- Estimated cost for this turn

## Interrupting

Press **Ctrl+C** to cancel the current response. The agent will stop and you can type a new prompt. Press Ctrl+C again to exit.

## Completion notifications

When a prompt takes more than a few seconds, yoyo rings the terminal bell so you notice the finish even if you switched windows (disable with `no_bell = true` or `--no-bell`).

For an actionable desktop alert, set the opt-in `notify_command` config key — a shell command yoyo runs (fire-and-forget) whenever a long prompt finishes, at the same threshold as the bell:

```toml
# .yoyo.toml — Linux
notify_command = "notify-send 'yoyo' 'done'"

# macOS
notify_command = "osascript -e 'display notification \"done\" with title \"yoyo\"'"
```

Or set it from inside the REPL:

```
/config set notify_command notify-send 'yoyo' 'done'
```

Details:

- **Opt-in and inert by default** — when the key is absent (or empty), nothing is spawned and no tools are probed.
- Runs via `sh -c` on Unix and `cmd /C` on Windows, fully detached: yoyo never waits on it and its output is discarded, so it can never block the REPL.
- The command receives two environment variables so one script can serve multiple purposes: `YOYO_EVENT=prompt_completed` and `YOYO_ELAPSED_SECS=<seconds>`.
- If the command fails to spawn, yoyo stays silent by default (one-line warning with `--verbose`).
- Setting it to an empty string clears it: `/config set notify_command ""`.

## Inline @file mentions

You can reference files directly in your prompts using `@path` syntax. The file content is automatically read and injected into the conversation — no need for a separate `/add` command.

```
> explain @src/main.rs
  ✓ added src/main.rs (250 lines)
  (1 file inlined from @mentions)

> refactor @src/cli.rs:50-100
  ✓ added src/cli.rs (lines 50-100) (51 lines)
  (1 file inlined from @mentions)

> compare @Cargo.toml and @README.md
  ✓ added Cargo.toml (35 lines)
  ✓ added README.md (120 lines)
  (2 files inlined from @mentions)
```

**How it works:**
- `@path` — injects the entire file
- `@path:start-end` — injects a specific line range
- If the path doesn't exist, the `@mention` is left as-is (it might be a username)
- Email-like patterns (`user@example.com`) are not treated as file mentions
- Images work too: `@screenshot.png` inlines the image into the conversation
