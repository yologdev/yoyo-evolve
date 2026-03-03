# Extended Thinking

Extended thinking gives the model more "reasoning time" before responding. This can improve quality for complex tasks like debugging, architecture decisions, or multi-step refactoring.

## Usage

```bash
yoyo --thinking high
yoyo --thinking medium
yoyo --thinking low
yoyo --thinking minimal
yoyo --thinking off
```

## Levels

| Level | Aliases | Description |
|-------|---------|-------------|
| `off` | `none` | No extended thinking (default) |
| `minimal` | `min` | Very brief reasoning |
| `low` | — | Short reasoning |
| `medium` | `med` | Moderate reasoning |
| `high` | `max` | Deep reasoning — best for complex tasks |

Levels are case-insensitive: `HIGH`, `High`, and `high` all work.

If you provide an unrecognized level, yoyo defaults to `medium` with a warning.

## When to use it

- **Complex debugging** — use `high` when the bug is subtle
- **Architecture decisions** — use `medium` or `high` for design questions
- **Simple tasks** — use `off` (the default) for quick file reads, simple edits, etc.

## Output

When thinking is enabled, the model's reasoning is shown dimmed in the output so you can follow along without it cluttering the main response.

## Trade-offs

Higher thinking levels use more tokens (and thus cost more) but often produce better results for hard problems. For routine tasks, the overhead isn't worth it.
