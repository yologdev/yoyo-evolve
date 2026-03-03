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

## Model

| Command | Description |
|---------|-------------|
| `/model <name>` | Switch to a different model (clears conversation) |

Example:
```
/model claude-sonnet-4-20250514
```

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

The `/tokens` command shows a visual progress bar of your context window:

```
  Context window:
    messages:    12
    context:     45.2k / 200.0k tokens
    █████████░░░░░░░░░░░ 23%
```

## Git

| Command | Description |
|---------|-------------|
| `/diff` | Show `git diff --stat` of uncommitted changes |
| `/undo` | Revert all uncommitted changes (`git checkout -- .`) |

The `/undo` command shows you what will be reverted before doing it.

## Unknown commands

If you type a `/command` that yoyo doesn't recognize, it will tell you:

```
  unknown command: /foo
  type /help for available commands
```

Note: lines starting with `/` that contain spaces (like `/model name`) are treated as command arguments, not unknown commands.
