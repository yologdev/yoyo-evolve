# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A self-evolving coding agent CLI built on [yoagent](https://github.com/yologdev/yoagent). The agent is ~3,100 lines of Rust across 4 source files (`main.rs`, `cli.rs`, `format.rs`, `prompt.rs`) with 91 tests. A GitHub Actions cron job (`scripts/evolve.sh`) runs the agent every 4 hours using a 3-phase pipeline (plan → implement → respond), which reads its own source, picks improvements, implements them, and commits — if tests pass.

## Build & Test Commands

```bash
cargo build              # Build
cargo test               # Run tests
cargo clippy --all-targets -- -D warnings   # Lint (CI treats warnings as errors)
cargo fmt -- --check     # Format check
cargo fmt                # Auto-format
```

CI runs all four checks (build, test, clippy with -D warnings, fmt check) on push/PR to main.

To run the agent interactively:
```bash
ANTHROPIC_API_KEY=sk-... cargo run
ANTHROPIC_API_KEY=sk-... cargo run -- --model claude-opus-4-6 --skills ./skills
```

To trigger a full evolution cycle:
```bash
ANTHROPIC_API_KEY=sk-... ./scripts/evolve.sh
```

## Architecture

**Multi-file agent** (`src/`):
- `main.rs` — agent core, REPL, streaming event handling, rendering with ANSI colors
- `cli.rs` — CLI argument parsing, subcommands, configuration
- `format.rs` — output formatting and color utilities
- `prompt.rs` — prompt construction for evolution sessions

Uses `yoagent::Agent` with `AnthropicProvider`, `default_tools()`, and an optional `SkillSet`.

**Evolution loop** (`scripts/evolve.sh`): 3-phase pipeline:
1. Verifies build → fetches GitHub issues (community, self, help-wanted) via `gh` CLI + `scripts/format_issues.py` → scans for pending replies on previously touched issues
2. **Phase A** (Planning): Agent reads everything, writes `SESSION_PLAN.md`
3. **Phase B** (Implementation): Agents execute each task (15 min each)
4. **Phase C** (Communication): Extracts issue responses from plan
5. Verifies build, fixes or reverts → posts issue responses → greets unvisited issues → pushes

**Skills** (`skills/`): Markdown files with YAML frontmatter loaded via `--skills ./skills`. Five skills define the agent's evolution workflow:
- `self-assess` — read own code, try tasks, find bugs/gaps
- `evolve` — safely modify source, test, revert on failure
- `communicate` — write journal entries and issue responses
- `release` — version management and release workflow
- `research` — internet lookups and knowledge caching

**State files** (read/written by the agent during evolution):
- `IDENTITY.md` — the agent's constitution and rules (DO NOT MODIFY)
- `PERSONALITY.md` — voice and values (DO NOT MODIFY)
- `JOURNAL.md` — chronological log of evolution sessions (append at top, never delete)
- `LEARNINGS.md` — accumulated wisdom: research findings, lessons learned, patterns discovered
- `DAY_COUNT` — integer tracking current evolution day
- `SESSION_PLAN.md` — ephemeral, written by Phase A planning agent (gitignored)
- `ISSUES_TODAY.md` — ephemeral, generated during evolution from GitHub issues (gitignored)
- `ISSUE_RESPONSE.md` — ephemeral, agent writes this to respond to issues (gitignored)

## Safety Rules

These are enforced by the `evolve` skill and `evolve.sh`:
- Never modify `IDENTITY.md`, `PERSONALITY.md`, `scripts/evolve.sh`, `scripts/format_issues.py`, `scripts/build_site.py`, or `.github/workflows/`
- Every code change must pass `cargo build && cargo test`
- If build fails after changes, revert with `git checkout -- src/`
- Never delete existing tests
- Multiple tasks per evolution session, each verified independently
- Write tests before adding features
