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

### Sub-agent spend

When the session has delegated work with `sub_agent` or `explore_agent`,
`/cost` lists that spend on its own line, and the first line says it is the
session's own spend only:

```
  Session cost (own, excluding sub-agents): $0.500
    4.2k in / 2.1k out
  sub-agents: 2 runs, $0.050 (priced at each child model's rates)
    2.0k in / 400 out
  Total including sub-agents: $0.550
```

If any delegated run could not be priced, the line says `cost unknown` (with
its tokens) and the total is reported as unknown — it is never shown as `$0`.
With no delegation, `/cost` looks exactly as above. The per-turn breakdown,
`/tokens`, `--output-format json` and the `--cost-warn` tally still count the
session's own spend only (#1073).

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

Costs are estimated based on published pricing for all major providers.

**Error direction.** Where a vendor's price varies (a promotion, a context tier,
a time-of-day rate, a regional endpoint), what decides the rate is the variation's
*selector*: the date, the endpoint, the prompt size, the account tier. When the
selector reaches yoyo (yoyo has a clock, knows the endpoint it calls, and sees each
prompt's size), the table models the variation. Only when the selector never reaches
yoyo does it use the higher rate instead, so an estimate is meant to read high, not
low. Date-keyed rows are listed under Google below. Two places do not meet
that yet, and both read low: DeepSeek is priced at its off-peak rate (see below);
and the session cost line for a tiered model (Haiku 5.5, Gemini 3.1 Pro, Grok 4.x)
uses the base rate, where only the per-turn breakdown applies the tier. (Until
Day 241, Grok 4.7 on xAI's US endpoint was a third; it now bills that
endpoint's rate, see xAI below.)

### Anthropic

Fleet models (Fable 5, Fable 5.1, Opus 5, Opus 5.5, Opus 4.8, Sonnet 5, Haiku 4.5)
read their pricing directly from yoagent's model presets at runtime, so their
rows below are informational — the preset is the source of truth. Until Day 222
(#1003) Opus 5.5 and Fable 5.1 were matched by their older sibling's preset and
priced as Opus 5 ($5 / $25) and Fable 5 ($1 cache read); each now gets its own.
Sonnet 5.5 has no yoagent preset either; until Day 224 it was priced as Sonnet 5
($0.20 cache read). yoyo now carries its own arm (Sonnet 5's window and limits,
$0.10 cache read, from the models.dev catalogue).

| Model | Input | Cache Write | Cache Read | Output |
|-------|-------|-------------|------------|--------|
| Fable 5 | $10/MTok | $12.50/MTok | $1/MTok | $50/MTok |
| Fable 5.1 | $10/MTok | $12.50/MTok | $0.25/MTok | $50/MTok |
| Opus 5.5 | $4/MTok | $5/MTok | $0.20/MTok | $20/MTok |
| Opus 4.8 | $5/MTok | $6.25/MTok | $0.50/MTok | $25/MTok |
| Sonnet 5 | $2/MTok | $2.50/MTok | $0.20/MTok | $10/MTok |
| Sonnet 5.5 | $2/MTok | $2.50/MTok | $0.10/MTok | $10/MTok |
| Opus 4.5/4.6 | $5/MTok | $6.25/MTok | $0.50/MTok | $25/MTok |
| Opus 4/4.1 | $15/MTok | $18.75/MTok | $1.50/MTok | $75/MTok |
| Sonnet | $3/MTok | $3.75/MTok | $0.30/MTok | $15/MTok |
| Haiku 4.5 | $1/MTok | $1.25/MTok | $0.10/MTok | $5/MTok |
| Haiku 5.5 | $0.10/MTok | $0.125/MTok | $0.01/MTok | $0.50/MTok |
| Haiku 3.5 | $0.80/MTok | $1/MTok | $0.08/MTok | $4/MTok |

Haiku 5.5 is not yet a yoagent preset, so yoyo carries its own (1M context,
128K max output, prices from Anthropic's pricing page). Anthropic charges it
more above 100,000 prompt tokens ($0.50 in, $0.625 cache write, $0.05 cache
read, $2.50 out). Since Day 222 the **per-turn breakdown** in `/cost` prices each
turn whose own prompt (input + cache tokens) is over 100,000 at those tier
rates, marks it with `*`, and adds a footnote; a prompt of exactly 100,000 stays
at the base rate. The tier is per request, so it is applied only where yoyo has
one request's usage. The **session cost** line, `/tokens`, the per-response
`↳` line and the `--verbose` totals do not apply the tier and still use
the base rates, so they **understate** a Haiku 5.5 session with very long
prompts. When the two disagree, the per-turn total is the closer one (it covers
only turns still in the conversation, so after `/compact` it can be lower). A
`[model_pricing]` override for the model replaces its pricing wholesale and no
tier applies. Gemini 3.1 Pro (see Google below) is tiered the same way.

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
| Gemini 2.5 Pro TTS (`gemini-2.5-pro-preview-tts`) | $1/MTok | $20/MTok |
| Gemini 3.1 Pro Preview (`gemini-3.1-pro-preview`, `-customtools`) | $2/MTok | $12/MTok |
| Gemini 3 Pro Image (`gemini-3-pro-image`) | $2/MTok | $120/MTok |
| Gemini 2.5 Flash | $0.30/MTok | $2.50/MTok |
| Gemini 2.5 Flash-Lite | $0.10/MTok | $0.40/MTok |
| Gemini 2.5 Flash Image (`gemini-2.5-flash-image`) | $0.30/MTok | $30/MTok |
| Gemini 2.5 Flash TTS (`gemini-2.5-flash-preview-tts`) | $0.50/MTok | $10/MTok |
| Gemini 2.0 Flash | $0.10/MTok | $0.40/MTok |
| Gemini 3 Flash Preview (`gemini-3-flash-preview`) | $0.50/MTok | $3/MTok |
| Gemini 3.1 Flash-Lite (`gemini-3.1-flash-lite`) | $0.25/MTok | $1.50/MTok |
| Gemini 3.5 Flash-Lite (`gemini-3.5-flash-lite`) | $0.30/MTok | $2.50/MTok |
| Gemini 3.6 Flash (`gemini-3.6-flash`) | $1.50/MTok | $7.50/MTok |
| Gemini 3.8 Flash (`gemini-3.8-flash`, also `gemini-3.7-flash`) | $1.50/MTok | $7.50/MTok |
| Gemini 4 Argon (`gemini-4-argon`) | $4/MTok | $20/MTok |
| Gemini 3.5 Flash (`gemini-3.5-flash`, retired) | $1.50/MTok | $9/MTok |

The rates in the table for Gemini 3.6 Flash, 3.8 Flash (and `gemini-3.7-flash`) and 4 Argon are
Google's regular rates. Until each introductory window ends, yoyo bills the introductory rate
instead, keyed on the date (since Day 242; before that `/cost` read about twice the bill):

| Model | Introductory input / cache read / output | Through | Regular from |
|-------|------------------------------------------|---------|--------------|
| `gemini-4-argon` | $2 / $0.10 / $10 | 2026-11-30 | 2026-12-01 00:00 UTC |
| `gemini-3.8-flash`, `gemini-3.7-flash`, `gemini-3.6-flash` | $0.75 / $0.075 / $3.75 | 2026-12-31 | 2027-01-01 00:00 UTC |

Google names no time zone, so the switch is at the earliest reading (midnight UTC); if Google
means a later zone, those hours read high. A cost is priced when it is shown, so a session from
before the switch summed after it also reads high. Google's deprecations page
says `gemini-3.7-flash` requests go to 3.8 Flash, so it bills 3.8's rate.
`gemini-3.5-flash` requests go to 3.6 Flash, but Google doesn't say at which model's price, so it
keeps its own $9 output, the higher of the two. The shut-down previews `gemini-3.1-flash-lite-preview`,
`gemini-3.1-flash-image-preview` and `gemini-3-pro-image-preview` bill the replacement Google
names for each. The 3.1 image, TTS and live
ids and `gemini-3.5-live-translate-preview` have exact prices too (image or audio output rates).
Other `gemini-3*` flash ids no Google page documents are still estimated at $0.15 / $0.60, and
other `gemini-3*` pro ids (for example `gemini-3-pro-preview`) at $1.25 / $10. Gemini 3.1 Pro (both ids) is billed at
$2 / $0.20 cache read / $12; Google charges $4 / $0.40 / $18 for a prompt over 200K tokens, and
since Day 239 the per-turn breakdown prices each such turn at those rates and marks it `*`, as for
Haiku 5.5 above (a prompt of exactly 200K stays at the base rate). The session cost line still
uses the base rate, so for those sessions it reads low; the per-turn total is the closer one.
Gemini 3 Pro Image is billed at its image output rate ($120); its text output is $12.

### DeepSeek

| Model | Input | Output |
|-------|-------|--------|
| DeepSeek Chat/V3 | $0.27/MTok | $1.10/MTok |
| DeepSeek Reasoner/R1 | $0.55/MTok | $2.19/MTok |
| DeepSeek Flash (V4.1-Flash, incl. the `deepseek-v4-flash` legacy alias) | $0.15/MTok | $0.60/MTok |

Two things to read these numbers with. First, they are **off-peak** prices:
DeepSeek doubles them on weekdays 01:00–04:00 and 06:00–10:00 UTC, so a cost
yoyo reports for a DeepSeek session is a **lower bound** — during a peak window
the true cost is up to 2x what is shown. Second, this table is a set of
**hardcoded literals**: when a provider changes its prices, nothing changes
here automatically. A drift alarm exists but is opt-in, because it fetches
models.dev over the network: `cargo test --bin yoyo audit_table_against_models_dev
-- --ignored` lists every row that disagrees. Open drift is tracked in #1012. `deepseek-v4-flash` (the id the evolve
loop runs on) is a legacy name DeepSeek still serves as V4.1-Flash, so it is
priced as `deepseek-flash`; `deepseek-r1`, `deepseek-v3` and `deepseek-v4-pro`
are carried unchanged and, at the time of writing, have not been checked against
a second source.

### Mistral

| Model | Input | Output |
|-------|-------|--------|
| Mistral Large | $0.50/MTok | $1.50/MTok |
| Mistral Small (`mistral-small-latest`, `mistral-small-2603`) | $0.15/MTok | $0.60/MTok |
| Mistral Small (other ids, e.g. `mistral-small-2506`) | $0.10/MTok | $0.30/MTok |
| Codestral | $0.30/MTok | $0.90/MTok |

### xAI (Grok)

| Model | Input | Cache Read | Output |
|-------|-------|------------|--------|
| Grok 4.3, Grok 4.20 (`grok-4.20-0309-reasoning`, `-non-reasoning`, `grok-4.20-multi-agent-0309`) | $1.25/MTok | $0.20/MTok | $2.50/MTok |
| Grok 4.5 | $2/MTok | $0.30/MTok | $6/MTok |
| Grok 4.6 (global endpoint; 1.1x on `us.api.x.ai`) | $2/MTok | $0.50/MTok | $6/MTok |
| Grok 3 | $3/MTok | — | $15/MTok |
| Grok 3 Mini | $0.30/MTok | — | $0.50/MTok |
| Grok 2 | $2/MTok | — | $10/MTok |

The Grok 4.x rows are from xAI's models page (2026-10-27) and match models.dev;
they are the rates of xAI's **global** endpoint, `https://api.x.ai/v1`, which is
yoyo's default for `provider = "xai"`. xAI's **US regional endpoint**,
`https://us.api.x.ai/v1`, serves only Grok 4.6 and Grok 4.7 and bills every
token 10% more, long-context and cached rates included (xAI's regional endpoints
page, 2026-10-27). yoyo picks the rate from the endpoint it actually calls: when
`base_url` (or `--base-url`) has the host `us.api.x.ai`, Grok 4.6 and Grok 4.7
are billed at 1.1x ($2.20 / $0.55 / $6.60), otherwise at the base rate. A
`[model_pricing]` entry for either id is used as written, never scaled.
Above a 200,000-token prompt xAI doubles every rate; the
per-turn breakdown in `/cost` applies that tier, the same way it does for Gemini
3.1 Pro, and the session cost line does not. Only the exact ids above are
priced this way: xAI's aliases (`grok-4.3-latest`, `grok-4.20-reasoning`, ...)
and other `grok-4*` ids are still estimated at $3 / $15. Grok 4.7 is priced by
yoagent's preset ($2 / $0.50 / $6), with the same US-endpoint premium applied.

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

## Spending warning

Set a budget in US dollars and yoyo prints one stderr warning when this session's estimated spend crosses it. It is a warning only; nothing is stopped. Sources, highest priority first: `--cost-warn <usd>`, the `YOYO_COST_WARN_USD` environment variable, and `cost_warn_usd` in `.yoyo.toml`. It is off when none is set.

The value must be a positive number. An invalid one (`abc`, `-1`, `0`, `nan`) prints a warning at startup naming where it came from, before any tokens are spent. If no other source gives a usable value, the cost warning is off for that run. An empty `YOYO_COST_WARN_USD` counts as unset.

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
