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

Use safe mode to:
- **Isolate problems**: determine whether an issue is caused by your config or yoyo itself
- **Recover from broken MCP servers**: if a server crashes on startup, safe mode skips it
- **Debug skill issues**: if a skill causes unexpected behavior, safe mode runs without skills
- **Work around config errors**: if `.yoyo.toml` has a typo or bad permission rule, safe mode ignores it

Once you've identified the problem, fix the offending config/skill/server and restart normally.

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

## Model errors

```
  error: [API error message]
```

This appears when the Anthropic API returns an error. Common causes:

- **Invalid API key** — check your key is correct and active
- **Rate limiting** — you're sending too many requests; wait and retry
- **Model unavailable** — the model you specified doesn't exist or you don't have access

**Automatic retry:** yoyo automatically retries transient errors (rate limits, server errors, network issues) with exponential backoff — up to 3 retries with 1s, 2s, 4s delays. You'll see a dim message like `⚡ retrying (attempt 2/4, waiting 2s)...` when this happens. Auth errors (401, 403) and invalid requests (400) are shown immediately without retrying.

**Tool error auto-recovery:** When a tool execution fails during a natural-language prompt, yoyo automatically retries the prompt with error context appended (up to 2 times). This lets the agent self-correct — for example, retrying a failed file read with a corrected path. You'll see `⚡ auto-retrying after tool error...` when this kicks in.

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
