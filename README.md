<p align="center">
  <img src="assets/banner.png" alt="yoyo — a coding agent that evolves itself" width="100%">
</p>

<p align="center">
  <a href="JOURNAL.md">Journal</a> ·
  <a href="https://yologdev.github.io/yoyo-evolve">Website</a> ·
  <a href="https://github.com/yologdev/yoyo-evolve">GitHub</a> ·
  <a href="https://deepwiki.com/yologdev/yoyo-evolve">DeepWiki</a> ·
  <a href="https://github.com/yologdev/yoyo-evolve/issues">Issues</a>
</p>

<p align="center">
  <a href="https://github.com/yologdev/yoyo-evolve/actions"><img src="https://img.shields.io/github/actions/workflow/status/yologdev/yoyo-evolve/evolve.yml?label=evolution&logo=github" alt="evolution"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="license MIT"></a>
  <a href="https://github.com/yologdev/yoyo-evolve/commits/main"><img src="https://img.shields.io/github/last-commit/yologdev/yoyo-evolve" alt="last commit"></a>
</p>

---

# yoyo: A Coding Agent That Evolves Itself

**yoyo** started as a ~200-line coding agent CLI built on [yoagent](https://github.com/yologdev/yoagent). Every day, it reads its own source code, assesses itself, makes improvements, and commits — if tests pass. Every failure is documented.

No human writes its code. No roadmap tells it what to do. It decides for itself.

Watch it grow.

## How It Works

```
GitHub Actions (daily 9am UTC)
    → Verify build passes
    → Fetch community issues (label: agent-input)
    → Agent reads: IDENTITY.md, src/main.rs, JOURNAL.md, issues
    → Self-assessment: find bugs, gaps, friction
    → Implement improvements (as many as it can)
    → cargo build && cargo test after each change
    → Pass → commit. Fail → revert.
    → Write journal entry
    → Push
```

The entire history is in the [git log](../../commits/main). The journal is in [JOURNAL.md](JOURNAL.md).

## Talk to It

Open a [GitHub issue](../../issues/new) with the `agent-input` label and yoyo will read it during its next session.

- **Suggestions** — tell it what to learn
- **Bugs** — tell it what's broken
- **Challenges** — give it a task and see if it can do it

Issues with more thumbs-up reactions get prioritized. The agent responds in its own voice.

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
src/main.rs              The entire agent (~470 lines of Rust)
scripts/evolve.sh        Daily evolution pipeline
scripts/build_site.py    Journey website generator
skills/                  Skill definitions (self-assess, evolve, communicate)
IDENTITY.md              Agent constitution (immutable)
JOURNAL.md               Daily session log (append-only)
DAY_COUNT                Current evolution day
```

## Built On

[yoagent](https://github.com/yologdev/yoagent) — minimal agent loop in Rust. The library that makes this possible.

## License

[MIT](LICENSE)
