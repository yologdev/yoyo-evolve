# Models & Providers

yoyo supports **12 providers** out of the box — from Anthropic and OpenAI to local models via Ollama.

## Default model

The default model is `claude-opus-4-6` (Anthropic). You can change it at startup or mid-session.

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
```

> **Note:** Switching models with `/model` preserves your conversation history — you can change models mid-task without losing context.

## Providers

Use `--provider <name>` to select a provider. Each provider has a default model and an environment variable for its API key.

> **Tip:** If you run `yoyo` without any API key configured, an interactive setup wizard will walk you through choosing a provider and entering your key. You can also save the config to `.yoyo.toml` directly from the wizard.

| Provider | Default Model | API Key Env Var |
|----------|--------------|-----------------|
| `anthropic` (default) | `claude-opus-4-6` | `ANTHROPIC_API_KEY` |
| `openai` | `gpt-4o` | `OPENAI_API_KEY` |
| `google` | `gemini-2.0-flash` | `GOOGLE_API_KEY` |
| `openrouter` | `anthropic/claude-sonnet-4-20250514` | `OPENROUTER_API_KEY` |
| `ollama` | `llama3.2` | *(none — local)* |
| `xai` | `grok-3` | `XAI_API_KEY` |
| `groq` | `llama-3.3-70b-versatile` | `GROQ_API_KEY` |
| `deepseek` | `deepseek-chat` | `DEEPSEEK_API_KEY` |
| `mistral` | `mistral-large-latest` | `MISTRAL_API_KEY` |
| `cerebras` | `llama-3.3-70b` | `CEREBRAS_API_KEY` |
| `zai` | `glm-4-plus` | `ZAI_API_KEY` |
| `custom` | `claude-opus-4-6` | *(none — bring your own)* |

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

## Cost estimation

Cost estimation is built in for many providers:

| Model Family | Input (per MTok) | Output (per MTok) |
|-------------|------------------|--------------------|
| Opus 4.5/4.6 | $5.00 | $25.00 |
| Opus 4/4.1 | $15.00 | $75.00 |
| Sonnet | $3.00 | $15.00 |
| Haiku 4.5 | $1.00 | $5.00 |
| Haiku 3.5 | $0.80 | $4.00 |

Cost estimates are also available for OpenAI, Google, DeepSeek, Mistral, xAI, Groq, ZAI and more.

## Context window

yoyo assumes a 200,000-token context window (the standard for Claude models). When usage exceeds 80% of this, auto-compaction kicks in. See [Context Management](../features/context.md).
