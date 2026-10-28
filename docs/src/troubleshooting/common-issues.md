# Common Issues

## Quick fix: Safe mode

If something is broken and you can't start yoyo normally (e.g., an MCP server crashes, a skill
causes errors, or a config setting is wrong), start yoyo in safe mode:

```bash
yoyo --safe-mode
```

This disables all customizations — MCP servers, skills, custom commands, permissions, and
auto-context — giving you a clean baseline session with only builtin tools. You'll see:

```
⚠ Safe mode: MCP servers, skills, custom commands, and config disabled
```

Project-authored text is kept out of the system prompt too: instruction files
(`CLAUDE.md`, `YOYO.md`, `.yoyo/instructions.md`), `.yoyo/memory.json`, and the
project goal file `.yoyo/goal.md`. If a goal file exists, yoyo says so on stderr
rather than dropping it silently:

```
⚠ goal file .yoyo/goal.md not loaded into the prompt — --safe-mode skips project-authored text (/goal show still reads it)
```

Use safe mode to:
- **Isolate problems**: determine whether an issue is caused by your config or yoyo itself
- **Recover from broken MCP servers**: if a server crashes on startup, safe mode skips it
- **Debug skill issues**: if a skill causes unexpected behavior, safe mode runs without skills
- **Work around config errors**: if `.yoyo.toml` has a typo or bad permission rule, safe mode ignores it

Once you've identified the problem, fix the offending config/skill/server and restart normally.

## "error: config file is malformed"

yoyo found a syntax problem its lenient config reader can't fix for you: a quoted value with no closing quote (`model = "claude-haiku-5-5`) or a `[section` header with no `]`. The message names each `file:line`.

Starting a session (the REPL, `yoyo -p`, piped input, `yoyo review`) **refuses** with this error and **exit code 2**, before any API call. Read as written, the value would keep its stray quote and be sent anyway (a model id with a stray `"` 404s), and nobody writes an unterminated quote on purpose. git, cargo and Python's `tomllib` refuse the same file.

To get going: fix the named lines, or move the file aside (`mv .yoyo.toml .yoyo.toml.bak`); yoyo then uses the next config file it finds, or its defaults.

The inspection commands (`yoyo config get`, `yoyo config show`, `yoyo doctor`, `yoyo model`, `yoyo permissions`) and `yoyo setup` still run, with a `warning: config file is malformed` line instead, so you can look at what yoyo read while you fix it. The detector skips the inside of multi-line strings (`"""` / `'''`), so their contents never trigger it.

## "No API key found"

```
error: No API key found.
Set ANTHROPIC_API_KEY or API_KEY environment variable.
```

**Fix:** Set your Anthropic API key:
```bash
export ANTHROPIC_API_KEY=sk-ant-api03-...
```

yoyo checks `ANTHROPIC_API_KEY` first, then `API_KEY`. At least one must be set and non-empty.

## "No input on stdin"

```
No input on stdin.
```

This happens when you pipe empty input to yoyo:
```bash
echo "" | yoyo
```

**Fix:** Make sure your piped input contains actual content.

## "is not a regular file (FIFO)"

```
pipe.txt is not a regular file (FIFO): reading it could block forever or never end; read_file only reads regular files
```

The `read_file` tool only reads regular files (symlinks to regular files are fine). It refuses FIFOs (named pipes), character and block devices (such as `/dev/null` or a tty) and sockets, because reading one can block forever or never end, which used to hang a `-p` run until it was killed. This applies with or without `--allow-dir`/`--deny-dir`. Directories and missing paths get the tool's own error, as before.

The same check covers two other readers. At startup, a project instruction file (`YOYO.md`, `CLAUDE.md`, `.yoyo/instructions.md`, `AGENTS.md`, `.cursorrules`, `.github/copilot-instructions.md`) that is not a regular file is skipped with a `project instruction file … not loaded` warning, so the session still starts. `/add` refuses one with `could not read …: … is not a regular file (FIFO)`. The goal files are covered too: a non-regular `.yoyo/goal.md` is skipped with a `goal file … not loaded into the prompt` warning, `.yoyo/goal_verify.md` with `verify command … not loaded`, and a non-regular `.yoyo/memory.json` with `project memories not loaded`; each run continues as if the file were absent. A non-regular `.yoyo.toml` is skipped with `warning: config file skipped - .yoyo.toml is not a regular file (FIFO)`, and yoyo goes on to load `~/.yoyo.toml` (or the XDG config) instead; `/cd` into a directory holding one reports that its `.yoyo.toml` could not be read. `yoyo config get` prints the same `config file skipped` warning and reports the key as unset; `yoyo config show` prints `Failed to read config file .yoyo.toml: … is not a regular file (FIFO)` on stderr and exits 1; and `/config set` (or saving an "always allow" choice) refuses with an error naming the file instead of reading from or writing into the pipe.

**Fix:** If you meant to read what flows through a pipe or device, capture it to a regular file first (for example `cat pipe.txt > out.txt` in another shell) and read that.

## Model errors

```
  error: [API error message]
```

This appears when the Anthropic API returns an error. Common causes:

- **Invalid API key** — check your key is correct and active
- **Rate limiting** — you're sending too many requests; wait and retry
- **Model unavailable** — the model you specified doesn't exist or you don't have access
- **A model id from another vendor's family** — `--provider` and `--model` resolve
  independently, so `--model claude-opus-5` with `provider = "deepseek"` sends an Anthropic
  id to `https://api.deepseek.com/v1`, and the 401 from the wrong endpoint is the only clue
  you get. yoyo warns once at startup when it sees this, naming the model, the resolved
  provider, and the endpoint requests will actually go to. Fix it by passing
  `--provider anthropic`, or pass `--base-url` if the endpoint is deliberate — a proxy or
  gateway serving another vendor's ids is a supported setup and stays silent.

**Automatic retry:** yoyo automatically retries transient errors (rate limits, server errors, network issues) with exponential backoff — up to 3 retries with 1s, 2s, 4s delays. You'll see a dim message like `⚡ retrying (attempt 2/4, waiting 2s)...` when this happens. Auth errors (401, 403) and invalid requests (400) are shown immediately without retrying.

**Tool error auto-recovery:** When a tool execution fails during a natural-language prompt, yoyo automatically retries the prompt with error context appended (up to 2 times). This lets the agent self-correct — for example, retrying a failed file read with a corrected path. You'll see `⚡ auto-retrying after tool error...` when this kicks in.

**Rate limits with a known reset time:** when a provider's error names *when* the limit
resets (e.g. `retry after Some(14454000)ms`, or an HTTP `retry-after:` header), yoyo reads
it instead of guessing. A short reset is waited out. A long one — more than ~2 minutes, or
more than the session has left — stops **on purpose**, with a message naming when the limit
reopens, rather than burning the remaining attempts on ≤90s sleeps against the same refusal.

If you'd rather leave the terminal open and wait the limit out, pass `--wait-for-reset`:

```bash
yoyo --wait-for-reset
```

That raises the ceiling to 6 hours (still capped by your session budget, if one is set), and
prints a line telling you how long it's about to wait. It is **off by default** — a process
that can silently sleep for hours is not a safe default — and it only makes the *running*
process wait; it does not resume or reschedule anything.

Use `/retry` to manually re-send the last prompt after a non-transient error is resolved.

## Context window full

```
    ⚠ Context is getting full. Consider /clear or /compact.
```

Your conversation is approaching the 200,000-token context limit.

**Fix:** Use `/compact` to compress the conversation, or `/clear` to start fresh.

yoyo auto-compacts at 80% capacity, but you can compact earlier if you prefer.

**Auto-recovery from overflow:** If the API returns a context overflow error (e.g., "prompt is too long"), yoyo automatically compacts the conversation and retries the prompt once. You'll see:
```
  ⚡ context overflow detected — auto-compacting and retrying...
```
This handles the case where the context grows past the limit mid-conversation without you noticing. If the retry also fails, yoyo suggests using `/compact` manually.

## "warning: Failed to load skills"

```
warning: Failed to load skills: [error]
```

The `--skills` directory couldn't be read. yoyo continues without skills.

**Fix:** Check that the path exists and contains valid skill files.

## "unknown command: /foo"

```
  unknown command: /foo
  type /help for available commands
```

You typed a command yoyo doesn't recognize. If it's a typo, yoyo will suggest the closest match:

```
  unknown command: /hlep
  did you mean /help?
  type /help for available commands
```

**Fix:** Check the suggestion, or type `/help` to see all available commands.

## "not in a git repository"

```
  error: not in a git repository
```

You used `/diff` or `/undo` outside a git repo.

**Fix:** Navigate to a directory that's inside a git repository before starting yoyo.

## Ctrl+C behavior

- **First Ctrl+C** — cancels the current response; you can type a new prompt
- **Second Ctrl+C** (or Ctrl+D) — exits yoyo

If a tool execution is hanging, Ctrl+C will abort it.

On Linux and macOS, stdio MCP servers run in their own process group, so the first Ctrl+C (cancel this response) does not reach them and their tools stay available afterwards. When yoyo exits (`/quit`, end of input, the end of a `-p` or piped run, an error exit, or SIGTERM/SIGHUP/SIGQUIT), it closes every connected server by killing its process group, so a server that ignores a closed stdin is stopped too. Exit waits at most 2 seconds for this; a server still busy with a call that never returned is left running past that. A crash (a panic) or a SIGKILL of yoyo itself skips the close, and then a server that ignores a closed stdin can keep running after yoyo is gone.

## Background jobs started by the bash tool

When a bash command finishes, yoyo keeps reading its output for up to 2 more seconds, then stops. Anything the command started in the background (`server &`, `(sleep 30 &)`) keeps running, and yoyo does not wait for it. Before this fix, a background job still writing to the tool's output kept `yoyo -p` from exiting until the job ended.

If such a job writes to that output after those 2 seconds, the write fails and the job will usually die of `SIGPIPE`. So to keep a server running, send its output somewhere else: `npm run dev >dev.log 2>&1 &`.

## Session file errors

```
  error saving: [error]
  error reading yoyo-session.json: [error]
  error parsing: [error]
```

Session save/load failed. Common causes:

- **Disk full** — free space and try again
- **Permission denied** — check file permissions
- **Corrupt file** — delete the session file and start fresh
