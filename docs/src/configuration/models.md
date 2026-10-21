# Models & Providers

yoyo supports **13 providers** out of the box — from Anthropic and OpenAI to local models via Ollama.

## Default model

The default model is `claude-opus-5-5` (Anthropic). You can change it at startup or mid-session.

## Changing the model

**At startup:**
```bash
yoyo --model claude-sonnet-4-20250514
yoyo --model gpt-4o --provider openai
yoyo --model llama3.2 --provider ollama
```

**During a session:**
```
/model claude-sonnet-4-20250514
/model list
/model list openai
```

> **Note:** Switching models with `/model` preserves your conversation history — you can change models mid-task without losing context. Use `/model list` to see all available models grouped by provider, or `/model info <name>` to see pricing, context window, and provider details for any model.

## Providers

Use `--provider <name>` to select a provider. Each provider has a default model and an environment variable for its API key.

> **Tip:** If you run `yoyo` without any API key configured, an interactive setup wizard will walk you through choosing a provider and entering your key. You can also save the config to `.yoyo.toml` directly from the wizard.

| Provider | Default Model | API Key Env Var |
|----------|--------------|-----------------|
| `anthropic` (default) | `claude-opus-5-5` | `ANTHROPIC_API_KEY` |
| `openai` | `gpt-6-sol` | `OPENAI_API_KEY` |
| `google` | `gemini-2.0-flash` | `GEMINI_API_KEY` or `GOOGLE_API_KEY` |
| `openrouter` | `anthropic/claude-sonnet-4-20250514` | `OPENROUTER_API_KEY` |
| `ollama` | `llama3.2` | *(none — local)* |
| `xai` | `grok-3` | `XAI_API_KEY` |
| `groq` | `llama-3.3-70b-versatile` | `GROQ_API_KEY` |
| `deepseek` | `deepseek-chat` | `DEEPSEEK_API_KEY` |
| `mistral` | `mistral-large-latest` | `MISTRAL_API_KEY` |
| `cerebras` | `llama-3.3-70b` | `CEREBRAS_API_KEY` |
| `zai` | `glm-4-plus` | `ZAI_API_KEY` |
| `minimax` | `MiniMax-M2.7` | `MINIMAX_API_KEY` |
| `custom` | `claude-opus-5-5` | *(none — bring your own)* |

> **Note:** the table shows each provider's *default* model — others are
> available. For example, MiniMax's flagship `MiniMax-M3` can be selected via
> `--model MiniMax-M3` or picked in the setup wizard.

### Examples

```bash
# OpenAI
OPENAI_API_KEY=sk-... yoyo --provider openai

# Google Gemini
GOOGLE_API_KEY=... yoyo --provider google --model gemini-2.5-pro

# Local with Ollama (no API key needed)
yoyo --provider ollama --model llama3.2

# Custom endpoint (OpenAI-compatible API)
yoyo --provider custom --base-url http://localhost:8080/v1 --model my-model
```

You can also set these in `.yoyo.toml`:
```toml
provider = "openai"
model = "gpt-4o"
base_url = "https://api.openai.com/v1"
```

## Route sub-agents to a cheaper model

A sub-agent (`sub_agent` or `explore_agent`) inherits the session's model by default. If
your exploration and bulk-delegation work does not need your main model, point it at a
cheaper one:

```toml
model = "claude-opus-4-6"
sub_agent_model = "claude-haiku-4-5"
```

This exists for **cost routing, not capability** — it is purely a cost lever, and the
child is otherwise unchanged. Only the model id moves.

- **Absent (the default) means the child runs on your session's model** — nothing changes
  for anyone who does not set it.
- The child is dispatched against **your session's provider and API key**, so write the
  bare model id from that provider. A `provider/model` value does not switch providers.
- The failure report a parent sees on a dead sub-agent names the model the child actually
  ran on, so the cheaper route is visible rather than implied.

## Cost estimation

Cost estimation is built in for many providers:

| Model Family | Input (per MTok) | Output (per MTok) |
|-------------|------------------|--------------------|
| Opus 4.5/4.6 | $5.00 | $25.00 |
| Opus 4/4.1 | $15.00 | $75.00 |
| Sonnet | $3.00 | $15.00 |
| Haiku 4.5 | $1.00 | $5.00 |
| Haiku 5.5 | $0.10 (prompts ≤100K tokens; $0.50 above) | $0.50 ($2.50 above) |
| Haiku 3.5 | $0.80 | $4.00 |

Cost estimates are also available for OpenAI, Google, DeepSeek, Mistral, xAI, Groq, ZAI and more.
On `provider = "openai"`, the exact ids `gpt-6-astra`, `gpt-6-sol`, `gpt-6-luna` and `gpt-6.1-sol` use
yoagent's own preset: a 1,050,000-token window, 64K default `max_tokens` (128K maximum), and its price,
including the higher whole-request rate above 272K prompt tokens (`gpt-6.1-sol` has its own
price, not `gpt-6-sol`'s). Other ids keep the generic OpenAI defaults.

The same holds for four more exact ids on their own provider: `gemini-4-argon` and `gemini-3.8-flash`
(`google`: 1,048,576-token window, 65,536 `max_tokens`, which is also the declared output maximum),
`grok-4.7` (`xai`: 500,000-token window, 64K default `max_tokens`, and the whole-request rate at
200K prompt tokens and above) and `mistral-large-4` (`mistral`: 1,000,000-token window, 32K default
`max_tokens`). xAI and Mistral publish no output maximum, so yoyo does not warn about a higher
`max_tokens` there. Gemini 4 Argon, Gemini 3.8 Flash and Mistral Large 4 are priced at the vendor's
**regular** rate, so while an introductory or launch price is running `/cost` can read up to twice
the actual bill. Near-miss ids (`gemini-3.8-flash-lite`, `grok-4.7-mini`, `mistral-large-latest`)
keep the generic defaults.

### Models with no preset run on guessed limits

On the OpenAI-compatible providers `openai`, `openrouter`, `xai`, `groq`, `mistral`, `cerebras`,
`github` and `zai`, a model id yoyo has no preset for runs on a guessed 128,000-token context window
and `max_tokens = 4096`. That cap covers reasoning plus answer on every turn, so a newer model
(for example a GPT release newer than yoyo's presets) can be cut short. yoyo now says so once per model on stderr, even in
quiet or piped runs (stdout is unchanged):

```text
warning: no preset is known for openai model 'gpt-6.9-test', so yoyo is using guessed limits: ...
```

If the model allows more, set the real limits yourself, which also silences the warning:

```toml
# .yoyo.toml
max_tokens = 64000
context_window = 400000
```

or pass `--max-tokens` / `--context-window`. Setting `max_tokens` is what silences it; setting only
`context_window` keeps the warning about the output cap. `ollama` and `custom` endpoints use the same
defaults but are not warned about, since you configure those servers yourself; `minimax` defaults to
a 1M window with `max_tokens = 4096` and is not warned about either.

## Context window

yoyo assumes a 200,000-token context window (the standard for Claude models). When usage exceeds 80% of this, auto-compaction kicks in. See [Context Management](../features/context.md).
