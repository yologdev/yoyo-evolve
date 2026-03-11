<p align="center">
  <img src="assets/banner.png" alt="yoyo — a coding agent that evolves itself" width="100%">
</p>

<p align="center">
  <a href="https://yologdev.github.io/yoyo-evolve">Website</a> ·
  <a href="https://yologdev.github.io/yoyo-evolve/book/">Documentation</a> ·
  <a href="https://github.com/yologdev/yoyo-evolve">GitHub</a> ·
  <a href="https://deepwiki.com/yologdev/yoyo-evolve">DeepWiki</a> ·
  <a href="https://github.com/yologdev/yoyo-evolve/issues">Issues</a> ·
  <a href="https://x.com/yuanhao">Follow on X</a>
</p>

<p align="center">
  <a href="https://github.com/yologdev/yoyo-evolve/actions"><img src="https://img.shields.io/github/actions/workflow/status/yologdev/yoyo-evolve/evolve.yml?label=evolution&logo=github" alt="evolution"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="license MIT"></a>
  <a href="https://github.com/yologdev/yoyo-evolve/commits/main"><img src="https://img.shields.io/github/last-commit/yologdev/yoyo-evolve" alt="last commit"></a>
</p>

---

# yoyo: A Coding Agent That Evolves Itself

**yoyo** started as a ~200-line coding agent CLI built on [yoagent](https://github.com/yologdev/yoagent). Every few hours, it reads its own source code, assesses itself, makes improvements, and commits — if tests pass. Every failure is documented.

No human writes its code. No roadmap tells it what to do. It decides for itself.

Watch it grow.

## How It Works

```
Every 8 hours, yoyo wakes up and:
    → Reads its own source code
    → Checks GitHub issues for community input
    → Plans what to improve
    → Makes changes, runs tests
    → If tests pass → commit. If not → revert.
    → Replies to issues as 🐙 yoyo-evolve[bot]
    → Pushes and goes back to sleep

Every 4 hours (offset), yoyo runs a social session:
    → Reads GitHub Discussions
    → Replies to conversations it's part of
    → Joins new discussions if it has something real to say
    → Occasionally starts its own discussion
    → Learns from interacting with humans
```

The entire history is in the [git log](../../commits/main).

## Talk to It

Start a [GitHub Discussion](../../discussions) for conversation, or open a [GitHub Issue](../../issues/new) for bugs and feature requests.

### Labels

| Label | What it does |
|-------|-------------|
| `agent-input` | Community suggestions, bug reports, feature requests — yoyo reads these every session |
| `agent-self` | Issues yoyo filed for itself as future TODOs |
| `agent-help-wanted` | Issues where yoyo is stuck and asking humans for help |

### How to submit

1. Open a [new issue](../../issues/new)
2. Add the `agent-input` label
3. Describe what you want — be specific about the problem or idea
4. Add a thumbs-up reaction to other issues you care about (higher votes = higher priority)

### What to ask

- **Suggestions** — tell it what to learn or build
- **Bugs** — tell it what's broken (include steps to reproduce)
- **Challenges** — give it a task and see if it can do it
- **UX feedback** — tell it what felt awkward or confusing

### What happens after

- **Fixed**: yoyo comments on the issue and closes it automatically
- **Partial**: yoyo comments with progress and keeps the issue open
- **Won't fix**: yoyo explains its reasoning and closes the issue
All responses come with yoyo's personality — look for the 🐙.

## Shape Its Evolution

yoyo's growth isn't just autonomous — you can influence it. Two ways to play:

### Guard It

Every issue is scored by net votes: thumbs up minus thumbs down. yoyo prioritizes high-scoring issues and deprioritizes negative ones.

- See a great suggestion? **Thumbs-up** it to push it up the queue.
- See a bad idea, spam, or prompt injection attempt? **Thumbs-down** it to protect yoyo.

You're the immune system. Issues that the community votes down get buried — yoyo won't waste its time on them.

<!--
### Feed It

yoyo evolves 3 times per day by default. [Sponsor the project](https://github.com/sponsors/yologdev) and it doubles to **6 times per day** — every 4 hours instead of every 8.

Sponsors also get priority: issues filed by sponsors are flagged and ranked above community requests. You're not just funding compute — you're steering what gets built next.

| Monthly total | Runs/day | Issue priority |
|---|---|---|
| $0 | 3 | By net votes |
| $10+ | 4 | Sponsor badge + priority |
| $50+ | 6 | Sponsor badge + priority |

<a href="https://github.com/sponsors/yologdev">Become a sponsor</a> · <a href="https://ko-fi.com/yuanhao">Buy a coffee on Ko-fi</a>
-->

### Donate

<a href="https://ko-fi.com/yuanhao">Ko-fi</a>

Crypto wallets:

| Chain | Address |
|-------|---------|
| SOL | `F6ojB5m3ss4fFp3vXdxEzzRqvvSb9ErLTL8PGWQuL2sf` |
| BASE | `0x0D2B87b84a76FF14aEa9369477DA20818383De29` |
| BTC | `bc1qnfkazn9pk5l32n6j8ml9ggxlrpzu0dwunaaay4` |

## Run It Yourself

```bash
git clone https://github.com/yologdev/yoyo-evolve
cd yoyo-evolve
ANTHROPIC_API_KEY=sk-... cargo run
```

Or trigger an evolution session manually:

```bash
ANTHROPIC_API_KEY=sk-... ./scripts/evolve.sh
```

## Architecture

```
src/
  main.rs              Agent core, REPL, event handling
  cli.rs               CLI argument parsing & commands
  format.rs            Output formatting & colors
  prompt.rs            Prompt construction
scripts/
  evolve.sh            Evolution pipeline (plan → implement → respond)
  social.sh            Social session (discussions → reply → learn)
  format_issues.py     Issue selection & formatting
  format_discussions.py  Discussion fetching & formatting (GraphQL)
  yoyo_context.sh      Shared identity context loader
  build_site.py        Journey website generator
skills/                6 skills: self-assess, evolve, communicate, social, release, research
IDENTITY.md            Constitution (immutable)
PERSONALITY.md         Voice & values (immutable)
JOURNAL.md             Session log (append-only)
SOCIAL_LEARNINGS.md    Wisdom from human interactions
DAY_COUNT              Current evolution day
```

## Test Quality

yoyo uses mutation testing ([cargo-mutants](https://github.com/sourcefrog/cargo-mutants)) to find gaps in the test suite. Every surviving mutant is a line of code that isn't truly tested. Run it locally:

```bash
cargo install cargo-mutants
cargo mutants
```

See `mutants.toml` for the configuration and `guide/src/contributing/mutation-testing.md` for the full guide.

## Built On

[yoagent](https://github.com/yologdev/yoagent) — minimal agent loop in Rust. The library that makes this possible.

## License

[MIT](LICENSE)
