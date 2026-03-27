# Installation

## Requirements

- **Rust toolchain** — install from [rustup.rs](https://rustup.rs)
- **An API key** — from any supported provider (see [Providers](#providers) below)

## Install from crates.io

```bash
cargo install yoyo-agent
```

This installs the binary as `yoyo` in your PATH.

## Install from source

```bash
git clone https://github.com/yologdev/yoyo-evolve.git
cd yoyo-evolve
cargo build --release
```

The binary will be at `target/release/yoyo`.

## Run directly with Cargo

If you just want to try it:

```bash
cd yoyo-evolve
ANTHROPIC_API_KEY=sk-ant-... cargo run
```

## Providers

yoyo supports multiple AI providers out of the box. Use the `--provider` flag to select one:

| Provider | Flag | Default Model | Env Var |
|----------|------|---------------|---------|
| Anthropic (default) | `--provider anthropic` | `claude-opus-4-6` | `ANTHROPIC_API_KEY` |
| OpenAI | `--provider openai` | `gpt-4o` | `OPENAI_API_KEY` |
| Google/Gemini | `--provider google` | `gemini-2.0-flash` | `GOOGLE_API_KEY` |
| OpenRouter | `--provider openrouter` | `anthropic/claude-sonnet-4-20250514` | `OPENROUTER_API_KEY` |
| xAI | `--provider xai` | `grok-3` | `XAI_API_KEY` |
| Groq | `--provider groq` | `llama-3.3-70b-versatile` | `GROQ_API_KEY` |
| DeepSeek | `--provider deepseek` | `deepseek-chat` | `DEEPSEEK_API_KEY` |
| Mistral | `--provider mistral` | `mistral-large-latest` | `MISTRAL_API_KEY` |
| Cerebras | `--provider cerebras` | `llama-3.3-70b` | `CEREBRAS_API_KEY` |
| Ollama | `--provider ollama` | `llama3.2` | *(none needed)* |
| Custom | `--provider custom` | *(none)* | *(none needed)* |

**Ollama and custom providers don't require an API key.** yoyo will automatically connect to `http://localhost:11434/v1` for Ollama or `http://localhost:8080/v1` for custom providers. Override the endpoint with `--base-url`.

Examples:

```bash
# Anthropic (default)
ANTHROPIC_API_KEY=sk-ant-... yoyo

# OpenAI
OPENAI_API_KEY=sk-... yoyo --provider openai

# Google Gemini
GOOGLE_API_KEY=... yoyo --provider google

# Local Ollama (no API key needed)
yoyo --provider ollama --model llama3.2

# Custom OpenAI-compatible endpoint
yoyo --provider custom --base-url http://localhost:8080/v1 --model my-model
```

## Set your API key

yoyo resolves your API key in this order:

1. `--api-key` CLI flag (highest priority)
2. Provider-specific environment variable (e.g., `OPENAI_API_KEY` for `--provider openai`)
3. `ANTHROPIC_API_KEY` environment variable (fallback)
4. `API_KEY` environment variable (generic fallback)
5. `api_key` in config file (see below)

Set one of them:

```bash
# Via environment variable (recommended)
export ANTHROPIC_API_KEY=sk-ant-api03-...

# Or pass directly
yoyo --api-key sk-ant-api03-...
```

If no key is found via any method (and the provider requires one), yoyo will exit with an error message explaining what to do.

## Config file

yoyo supports a TOML-style config file so you don't have to pass flags every time. Config files are checked in this order (first found wins):

1. `.yoyo.toml` in the current directory (project-level)
2. `~/.yoyo.toml` (home directory shorthand)
3. `~/.config/yoyo/config.toml` (XDG user-level)

**Example `.yoyo.toml`:**

```toml
# Model and provider
model = "claude-sonnet-4-20250514"
provider = "anthropic"
thinking = "medium"

# API key (env vars take priority over this)
api_key = "sk-ant-api03-..."

# Generation settings
max_tokens = 8192
max_turns = 50
temperature = 0.7

# Custom endpoint (for ollama, proxies, etc.)
# base_url = "http://localhost:11434/v1"

# Permission rules for bash commands
[permissions]
allow = ["git *", "cargo *", "echo *"]
deny = ["rm -rf *", "sudo *"]

# Directory restrictions for file tools
[directories]
allow = ["./src", "./tests"]
deny = ["~/.ssh", "/etc"]
```

CLI flags always override config file values. For example, `--model gpt-4o` overrides `model = "claude-sonnet-4-20250514"` from the config file.

For more details on model configuration, see [Models](../configuration/models.md). For thinking levels, see [Thinking](../configuration/thinking.md).
