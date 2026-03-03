# Cost Tracking

yoyo estimates the cost of each interaction so you can monitor spending.

## Per-turn costs

After each response, you'll see token usage and cost:

```
  tokens: 1523 in / 842 out  (session: 4200 in / 2100 out)  cost: $0.0234  total: $0.0567  ⏱ 3.2s
```

- **cost** — estimated cost for this turn
- **total** — estimated cumulative cost for the session

## Detailed breakdown

Use `/tokens` to see a full breakdown including cache usage:

```
  Session totals:
    input:       120.5k tokens
    output:      45.2k tokens
    cache read:  30.0k tokens
    cache write: 15.0k tokens
    est. cost:   $0.892
```

## How costs are calculated

Costs are estimated based on published Anthropic pricing:

| Model | Input | Cache Write | Cache Read | Output |
|-------|-------|-------------|------------|--------|
| Opus 4.5/4.6 | $5/MTok | $6.25/MTok | $0.50/MTok | $25/MTok |
| Opus 4/4.1 | $15/MTok | $18.75/MTok | $1.50/MTok | $75/MTok |
| Sonnet | $3/MTok | $3.75/MTok | $0.30/MTok | $15/MTok |
| Haiku 4.5 | $1/MTok | $1.25/MTok | $0.10/MTok | $5/MTok |
| Haiku 3.5 | $0.80/MTok | $1/MTok | $0.08/MTok | $4/MTok |

MTok = million tokens.

## Limitations

- Cost estimates are approximate — actual billing may differ slightly
- For unrecognized models, no cost estimate is shown
- Cache read/write costs depend on Anthropic's caching behavior, which yoyo doesn't control

## Keeping costs down

- Use smaller models (Haiku, Sonnet) for simple tasks
- Use `/compact` to reduce context size (fewer input tokens per turn)
- Use single-prompt mode (`-p`) for quick questions to avoid accumulating context
- Turn off extended thinking for routine tasks
