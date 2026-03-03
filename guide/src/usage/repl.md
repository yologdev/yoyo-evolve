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
main > _
```

If you're not in a git repo, you get a plain prompt:

```
> _
```

## How it works

1. You type a message
2. yoyo sends it to the LLM along with conversation history
3. The LLM may call tools (read files, run commands, etc.)
4. Tool results are streamed back — you see each tool as it executes
5. The final text response is printed
6. Token usage and cost are shown after each turn

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

After each response, you'll see token usage:

```
  tokens: 1523 in / 842 out  (session: 4200 in / 2100 out)  cost: $0.0234  total: $0.0567  ⏱ 3.2s
```

This shows:
- Input/output tokens for this turn
- Session totals
- Estimated cost for this turn and session total
- Wall-clock time for the response

## Interrupting

Press **Ctrl+C** to cancel the current response. The agent will stop and you can type a new prompt. Press Ctrl+C again to exit.
