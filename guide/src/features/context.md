# Context Management

Claude models have a finite context window (200,000 tokens). As your conversation grows, it fills up. yoyo helps you manage this.

## Checking context usage

Use `/tokens` to see how full your context window is:

```
/tokens
```

Output:
```
  Context window:
    messages:    24
    context:     85.2k / 200.0k tokens
    ████████░░░░░░░░░░░░ 43%

  Session totals:
    input:       120.5k tokens
    output:      45.2k tokens
    cache read:  30.0k tokens
    cache write: 15.0k tokens
    est. cost:   $0.892
```

When the context window exceeds 75%, you'll see a warning:

```
    ⚠ Context is getting full. Consider /clear or /compact.
```

## Manual compaction

Use `/compact` to compress the conversation:

```
/compact
```

This summarizes older messages while preserving recent context. You'll see:

```
  compacted: 24 → 8 messages, ~85.2k → ~32.1k tokens
```

## Auto-compaction

When the context window exceeds **80%** capacity, yoyo automatically compacts the conversation. You'll see:

```
  ⚡ auto-compacted: 30 → 10 messages, ~165.0k → ~62.0k tokens
```

This happens transparently after each prompt response. You don't need to do anything — yoyo handles it.

## Clearing the conversation

If you want to start completely fresh:

```
/clear
```

This removes all messages and resets the conversation. Unlike `/compact`, nothing is preserved.

## Tips

- For long sessions, use `/tokens` periodically to monitor usage
- If you notice the agent losing track of earlier context, try `/compact`
- Starting a new task? Use `/clear` to avoid confusing the agent with unrelated history
