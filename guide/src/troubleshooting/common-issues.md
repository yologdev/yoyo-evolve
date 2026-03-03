# Common Issues

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

Use `/retry` to re-send the last prompt after the issue is resolved.

## Context window full

```
    ⚠ Context is getting full. Consider /clear or /compact.
```

Your conversation is approaching the 200,000-token context limit.

**Fix:** Use `/compact` to compress the conversation, or `/clear` to start fresh.

yoyo auto-compacts at 80% capacity, but you can compact earlier if you prefer.

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

You typed a command yoyo doesn't recognize.

**Fix:** Type `/help` to see available commands.

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
