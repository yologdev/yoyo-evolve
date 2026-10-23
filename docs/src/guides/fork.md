# Grow Your Own Agent

Fork yoyo-evolve, edit two files, and run your own self-evolving coding agent on GitHub Actions.

## What You Get

A coding agent that:
- Runs on GitHub Actions every ~8 hours
- Reads its own source code, picks improvements, implements them
- Writes a journal of its evolution
- Responds to community issues in its own voice
- Gets smarter over time through a persistent memory system

## Quick Start

### 1. Fork the repo

Fork [yologdev/yoyo-evolve](https://github.com/yologdev/yoyo-evolve) on GitHub.

### 2. Edit your agent's identity

**`IDENTITY.md`** — your agent's constitution: name, mission, goals, and rules.

**`PERSONALITY.md`** — your agent's voice: how it writes, speaks, and expresses itself.

These are the only files you *need* to edit. Everything else auto-detects.

If you register in the yoyo family Address Book, also update **`LINEAGE.md`** so your agent has prompt-visible generation, parent, and branch-point context.

### 3. Choose your provider

yoyo supports 13+ providers out of the box. Pick the one that fits your budget and preferences:

| Provider | Env Var | Default Model | Notes |
|----------|---------|---------------|-------|
| `anthropic` | `ANTHROPIC_API_KEY` | `claude-opus-5-5` | Default. Best overall quality. |
| `openai` | `OPENAI_API_KEY` | `gpt-6-sol` | GPT-6 presets (sol/astra/luna) and older GPT ids |
| `google` | `GEMINI_API_KEY` or `GOOGLE_API_KEY` | `gemini-3.8-flash` | Gemini models |
| `openrouter` | `OPENROUTER_API_KEY` | `anthropic/claude-sonnet-4-20250514` | Multi-provider gateway |
| `deepseek` | `DEEPSEEK_API_KEY` | `deepseek-chat` | Very cost-effective |
| `groq` | `GROQ_API_KEY` | `llama-3.3-70b-versatile` | Fast inference |
| `mistral` | `MISTRAL_API_KEY` | `mistral-large-latest` | Mistral and Codestral models |
| `xai` | `XAI_API_KEY` | `grok-3` | Grok models |
| `ollama` | *(none — local)* | `llama3.2` | Free, runs on your hardware |

For the full list of providers and models, see [Models & Providers](../configuration/models.md).

> **Tip:** Anthropic is the default and what yoyo itself uses to evolve. If you're unsure, start there. If cost is a concern, DeepSeek and Groq offer strong results at a fraction of the price. Ollama is free but requires local hardware.

### 4. Create a GitHub App

Your agent needs a GitHub App to commit code and interact with issues.

1. Go to **Settings > Developer settings > GitHub Apps > New GitHub App**
2. Give it your agent's name
3. Set permissions:
   - **Repository > Contents**: Read and write
   - **Repository > Issues**: Read and write
   - **Repository > Discussions**: Read and write (optional, for social features)
4. Install it on your forked repo
5. Note the **App ID**, **Private Key** (generate one), and **Installation ID**
   - Installation ID: visit `https://github.com/settings/installations` and click your app — the ID is in the URL

### 5. Set repo secrets

In your fork, go to **Settings > Secrets and variables > Actions** and add:

| Secret | Description |
|--------|-------------|
| *Provider API key* | API key for your chosen provider (see table in step 3) |
| `APP_ID` | GitHub App ID |
| `APP_PRIVATE_KEY` | GitHub App private key (PEM) |
| `APP_INSTALLATION_ID` | GitHub App installation ID |

Set the API key secret matching your chosen provider. For example, if using Anthropic, add `ANTHROPIC_API_KEY`. If using OpenAI, add `OPENAI_API_KEY`. If using DeepSeek, add `DEEPSEEK_API_KEY`, and so on.

### 6. Enable the Evolution workflow

Go to **Actions** in your fork and enable the **Evolution** workflow. Your agent will start evolving on its next scheduled run, or trigger it manually with **Run workflow**.

## What Each File Does

| File | Purpose |
|------|---------|
| `IDENTITY.md` | Agent's constitution — name, mission, goals, rules |
| `PERSONALITY.md` | Agent's voice — writing style, personality traits |
| `LINEAGE.md` | Agent's family-tree generation, root ancestor, parent, and branch point |
| `ECONOMICS.md` | What money/sponsorship means to the agent |
| `journals/JOURNAL.md` | Chronological log of evolution sessions (auto-maintained) |
| `DAY_COUNT` | Tracks the agent's current evolution day |
| `memory/` | Persistent learning system (auto-maintained) |
| `SPONSORS.md` | Sponsor recognition (auto-maintained) |

## Costs

Costs vary by provider and model:

- **Anthropic Claude Opus** — ~$3-8 per session (~$10-25/day at 3 sessions/day)
- **Anthropic Claude Sonnet** — ~$1-3 per session, good balance of quality and cost
- **DeepSeek** — significantly cheaper, strong coding performance
- **Groq** — fast and affordable for smaller models
- **Ollama** — free (runs locally), but requires capable hardware

The default schedule runs ~3 sessions per day (8-hour gap between runs). To reduce costs, switch to a cheaper provider/model or reduce session frequency.

## Customization

### Change the provider and model

Set `PROVIDER` and `MODEL` environment variables in `.github/workflows/evolve.yml`:

```yaml
env:
  PROVIDER: openai
  MODEL: gpt-4o
```

Or set just `MODEL` to use a different model within the default provider (Anthropic):

```yaml
env:
  MODEL: claude-sonnet-4-6
```

You can also edit the default directly in `scripts/evolve.sh`.

### Change session frequency

Edit the cron schedule in `.github/workflows/evolve.yml`. The default `0 * * * *` (every hour) is gated by an 8-hour gap in the script, so the agent runs ~3 times/day.

### Add custom skills

Create markdown files with YAML frontmatter in the `skills/` directory. The agent loads them automatically via `--skills ./skills`.

### Sponsor system

The sponsor system auto-detects your GitHub Sponsors. No configuration needed — just set up GitHub Sponsors on your account.

## The `/update` Command

The yoyo binary's `/update` command checks for releases from `yologdev/yoyo-evolve`, not your fork. This is expected behavior. As a fork maintainer, rebuild from source after pulling changes:

```bash
cargo build --release
```

In the future, an evolve portal will provide guided setup including custom update targets.

## Optional: Dashboard Notifications

If you have a dashboard repo that accepts repository dispatch events, set a repo variable:

```bash
gh variable set DASHBOARD_REPO --body "your-user/your-dashboard" --repo your-user/your-fork
```

And add the `DASHBOARD_TOKEN` secret with a token that can dispatch to that repo.
