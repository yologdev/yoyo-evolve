# Context Management

Claude models have a finite context window (200,000 tokens). As your conversation grows, it fills up. yoyo helps you manage this.

## Checking context usage

Use `/tokens` to see how full your context window is:

```
/tokens
```

Output:
```
  Active context:
    messages:    24
    current:     85.2k / 200.0k tokens
    ████████░░░░░░░░░░░░ 43%

  Session totals (all API calls):
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

This summarizes older messages while preserving recent context. You'll see what survived:

```
  compacted: 24 → 8 messages, ~85.2k → ~32.1k tokens
  📋 Still in context: src/main.rs, src/tools.rs, auth refactor (2 files, 1 topic)
```

The context summary shows which files and topics are still present after compaction, so you can trust that important context wasn't lost.

### Previewing compaction

Not sure if you should compact? Use `--preview` to see what would happen without changing anything:

```
/compact --preview
```

This shows estimated token savings, which messages would be compressed, files touched, and topics in the conversation — all read-only.

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

## Subdirectory instruction files

At startup yoyo loads instruction files (`YOYO.md`, `CLAUDE.md`, `.yoyo/instructions.md`, `AGENTS.md`, `.cursorrules`, `.github/copilot-instructions.md`) from the project root only. In a monorepo, a package can carry its own, for example `packages/api/AGENTS.md`.

The first time the model successfully reads a file with `read_file` under such a directory, yoyo adds that directory's instruction files to the tool result, after the file content. Each one gets the same provenance markers as the root files. How it works:

- **Only directories below the root.** The root's own files were already loaded at startup, so they aren't added again. Every directory between the root and the file that was read is checked, outermost first.
- **Once per directory per session.** A directory is marked as seen after the first read under it, whether or not it had an instruction file. The record is shared across the whole process, so a sub-agent's read counts as well.
- **Fenced like a read.** An instruction file that a `read_file` of its path would refuse (a `--deny-dir` or `/cd` deny, the outside-launch-directory read fence, a FIFO or device) is skipped without a message. yoyo never prompts you to approve one.
- **Capped.** Each file contributes at most 16,000 bytes. A longer file is cut on a character boundary, and the cut is marked in the text.
- **Anchored at the current directory when the read happens.** After `/cd`, "below the root" means below the new directory.

Repos without nested instruction files see no change. Only reads trigger this: editing a file doesn't, and `/context` does not list these files.

## Tips

- For long sessions, use `/tokens` periodically to monitor usage
- If you notice the agent losing track of earlier context, try `/compact`
- Starting a new task? Use `/clear` to avoid confusing the agent with unrelated history

## Checkpoint-restart strategy

For automated pipelines (like CI scripts), compaction can be lossy. The `--context-strategy checkpoint` flag provides an alternative: when context usage exceeds 70%, yoyo stops the agent loop and exits with code **2**.

```bash
yoyo --context-strategy checkpoint -p "do some long task"
# Exit code 2 means "context was getting full — restart me"
```

The calling script can then restart yoyo with fresh context. This is useful for multi-phase pipelines where a structured restart produces better results than lossy compaction.

The default strategy is `compaction`, which uses auto-compaction as described above.
