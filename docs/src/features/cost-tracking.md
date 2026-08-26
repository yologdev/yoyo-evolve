# Cost Tracking

yoyo estimates the cost of each interaction so you can monitor spending.

## Per-turn costs

After each response, you'll see a compact token summary:

```
  ↳ 3.2s · 1523→842 tokens · $0.0234
```

With `--verbose` (or `-v`), you get the full breakdown:

```
  tokens: 1523 in / 842 out  [cache: 1000 read, 500 write]  (session: 4200 in / 2100 out)  cost: $0.0234  total: $0.0567  ⏱ 3.2s
```

- **cost** — estimated cost for this turn
- **total** — estimated cumulative cost for the session

## Quick cost check

Use `/cost` for a quick overview with a breakdown by cost category:

```
  Session cost: $0.0567
    4.2k in / 2.1k out
    cache: 1.0k read / 500 write

    Breakdown:
      input:       $0.0126
      output:      $0.0315
      cache write: $0.0031
      cache read:  $0.0005
```

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

## Supported models

Costs are estimated based on published pricing for all major providers:

### Anthropic

Fleet models (Fable 5, Opus 4.8, Sonnet 5, Haiku 4.5) read their pricing
directly from yoagent's model presets at runtime, so their rows below are
informational — the preset is the source of truth:

| Model | Input | Cache Write | Cache Read | Output |
|-------|-------|-------------|------------|--------|
| Fable 5 | $10/MTok | $12.50/MTok | $1/MTok | $50/MTok |
| Opus 4.8 | $5/MTok | $6.25/MTok | $0.50/MTok | $25/MTok |
| Sonnet 5 | $3/MTok | $3.75/MTok | $0.30/MTok | $15/MTok |
| Opus 4.5/4.6 | $5/MTok | $6.25/MTok | $0.50/MTok | $25/MTok |
| Opus 4/4.1 | $15/MTok | $18.75/MTok | $1.50/MTok | $75/MTok |
| Sonnet | $3/MTok | $3.75/MTok | $0.30/MTok | $15/MTok |
| Haiku 4.5 | $1/MTok | $1.25/MTok | $0.10/MTok | $5/MTok |
| Haiku 3.5 | $0.80/MTok | $1/MTok | $0.08/MTok | $4/MTok |

### OpenAI

| Model | Input | Output |
|-------|-------|--------|
| GPT-4.1 | $2/MTok | $8/MTok |
| GPT-4.1 Mini | $0.40/MTok | $1.60/MTok |
| GPT-4.1 Nano | $0.10/MTok | $0.40/MTok |
| GPT-4o | $2.50/MTok | $10/MTok |
| GPT-4o Mini | $0.15/MTok | $0.60/MTok |
| o3 | $2/MTok | $8/MTok |
| o3-mini | $1.10/MTok | $4.40/MTok |
| o4-mini | $1.10/MTok | $4.40/MTok |

### Google

| Model | Input | Output |
|-------|-------|--------|
| Gemini 2.5 Pro | $1.25/MTok | $10/MTok |
| Gemini 2.5 Flash | $0.15/MTok | $0.60/MTok |
| Gemini 2.0 Flash | $0.10/MTok | $0.40/MTok |

### DeepSeek

| Model | Input | Output |
|-------|-------|--------|
| DeepSeek Chat/V3 | $0.27/MTok | $1.10/MTok |
| DeepSeek Reasoner/R1 | $0.55/MTok | $2.19/MTok |

### Mistral

| Model | Input | Output |
|-------|-------|--------|
| Mistral Large | $2/MTok | $6/MTok |
| Mistral Small | $0.10/MTok | $0.30/MTok |
| Codestral | $0.30/MTok | $0.90/MTok |

### xAI (Grok)

| Model | Input | Output |
|-------|-------|--------|
| Grok 3 | $3/MTok | $15/MTok |
| Grok 3 Mini | $0.30/MTok | $0.50/MTok |
| Grok 2 | $2/MTok | $10/MTok |

### Groq (hosted models)

| Model | Input | Output |
|-------|-------|--------|
| Llama 3.3 70B | $0.59/MTok | $0.79/MTok |
| Llama 3.1 8B | $0.05/MTok | $0.08/MTok |
| Mixtral 8x7B | $0.24/MTok | $0.24/MTok |
| Gemma2 9B | $0.20/MTok | $0.20/MTok |

MTok = million tokens.

### OpenRouter

Models accessed through OpenRouter (e.g., `anthropic/claude-sonnet-4-20250514`) are automatically recognized — the provider prefix is stripped before matching.

## Custom pricing

The built-in table is list price. If your real rate differs — a negotiated
contract, a volume discount, a proxy or gateway that re-prices, a self-hosted
model whose marginal cost is electricity, or any OpenAI-compatible endpoint
yoyo has no entry for — you can say so in your config file:

```toml
[model_pricing."my-model-id"]
input = 1.50    # USD per million input tokens
output = 7.00   # USD per million output tokens
```

**Precedence: your override > the built-in preset > the built-in table.** Your
number always wins for the model id you name.

Rules:

- **Model ids match exactly.** Pricing `gpt-4o` does not re-price
  `gpt-4o-mini`, and does not re-price `openai/gpt-4o`. Write the id exactly as
  yoyo reports it in `/model`.
- **Both rates are required.** An entry missing `input` or `output`, or
  carrying a non-number or a negative number, is ignored entirely and named on
  startup — yoyo will not apply half a price, because the resulting number
  would belong to no source at all.
- **Zero is a valid price.** A self-hosted model whose marginal cost per
  million tokens really is zero reports `$0.0000` rather than reporting
  nothing.
- **Overrides cover input and output only.** A model you price yourself is
  costed with cache-write and cache-read rates of zero; cache-token rates are
  not configurable today.

This makes the number **correctable, not correct**: an override is a claim you
made, and yoyo has no way to verify it against your actual invoice.

`[model_pricing]` is a table, not a scalar, so it is not settable through
`/config set` — edit your config file directly (`/config edit`).

## Limitations

- Cost estimates are approximate — actual billing may differ slightly
- For unrecognized models, no cost estimate is shown
- Cache read/write costs only apply to Anthropic models; other providers show zero cache costs
- Pricing may change — check your provider's pricing page for the latest rates, or pin your own rate with `[model_pricing]` above

## Keeping costs down

- Use smaller models (Haiku, Sonnet, GPT-4.1 Mini, Gemini Flash) for simple tasks
- Use `/compact` to reduce context size (fewer input tokens per turn)
- Use single-prompt mode (`-p`) for quick questions to avoid accumulating context
- Turn off extended thinking for routine tasks
