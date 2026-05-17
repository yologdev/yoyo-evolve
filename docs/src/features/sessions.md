# Session Persistence

yoyo can save and load conversations, letting you resume where you left off.

## Auto-save on exit

yoyo **automatically saves your conversation** to `.yoyo/last-session.json` every time you exit the REPL — whether via `/quit`, `/exit`, `Ctrl-D`, or even unexpected termination. No flags needed.

If a previous session is detected on startup, yoyo prints a hint:

```
  💡 Previous session found. Use --continue or /load .yoyo/last-session.json to resume.
```

## Resuming with --continue

The `--continue` (or `-c`) flag restores the last auto-saved session:

```bash
yoyo --continue
yoyo -c
```

When `--continue` is used:
1. **On startup**, yoyo loads from `.yoyo/last-session.json` (preferred) or `yoyo-session.json` (legacy fallback)
2. **On exit**, the conversation is auto-saved as usual

```bash
$ yoyo -c
  📋 resumed session (8 messages, 5 tool calls)
  last prompt: "Can you fix the test failures in commands_map.rs?"
  last reply:  "I found 3 failing tests. The issue was..."

main > what were we working on?
```

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
/load .yoyo/last-session.json
```

## Session format

Sessions are stored as JSON files containing the conversation message history. The format is determined by the yoagent library.

## Error handling

- If no previous session exists when using `--continue`, yoyo prints a message and starts fresh
- If a session file is corrupt or can't be parsed, yoyo warns you and starts fresh
- Empty conversations (no messages exchanged) are not auto-saved
- Save errors are reported but don't crash yoyo
