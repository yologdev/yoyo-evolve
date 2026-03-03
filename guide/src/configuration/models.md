# Models

yoyo uses Anthropic's Claude models via the Anthropic API.

## Default model

The default model is `claude-opus-4-6`.

## Changing the model

**At startup:**
```bash
yoyo --model claude-sonnet-4-20250514
yoyo --model claude-haiku-4-5-20250514
```

**During a session:**
```
/model claude-sonnet-4-20250514
```

> **Note:** Switching models with `/model` clears the conversation history. This is because different models may handle context differently.

## Supported models

yoyo works with any Anthropic model. Cost estimation is built in for these model families:

| Model Family | Input (per MTok) | Output (per MTok) |
|-------------|------------------|--------------------|
| Opus 4.5/4.6 | $5.00 | $25.00 |
| Opus 4/4.1 | $15.00 | $75.00 |
| Sonnet | $3.00 | $15.00 |
| Haiku 4.5 | $1.00 | $5.00 |
| Haiku 3.5 | $0.80 | $4.00 |

For unrecognized models, yoyo still works — you just won't see cost estimates.

## Context window

yoyo assumes a 200,000-token context window (the standard for Claude models). When usage exceeds 80% of this, auto-compaction kicks in. See [Context Management](../features/context.md).
