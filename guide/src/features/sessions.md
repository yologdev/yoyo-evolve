# Session Persistence

yoyo can save and load conversations, letting you resume where you left off.

## Manual save/load

**Save the current conversation:**
```
/save
```
This writes to `yoyo-session.json` in the current directory.

**Save to a custom path:**
```
/save my-session.json
```

**Load a conversation:**
```
/load
/load my-session.json
```

## Auto-save with --continue

The `--continue` (or `-c`) flag enables automatic session management:

```bash
yoyo --continue
yoyo -c
```

When `--continue` is used:
1. **On startup**, yoyo looks for `yoyo-session.json` and restores the conversation if found
2. **On exit**, yoyo automatically saves the conversation back to `yoyo-session.json`

This means you can close yoyo, come back later, and pick up right where you left off:

```bash
$ yoyo -c
  resumed session: 8 messages from yoyo-session.json

main > what were we working on?
```

## Session format

Sessions are stored as JSON files containing the conversation message history. The format is determined by the yoagent library.

## Error handling

- If no previous session exists when using `--continue`, yoyo prints a message and starts fresh
- If a session file is corrupt or can't be parsed, yoyo warns you and starts fresh
- Save errors are reported but don't crash yoyo
