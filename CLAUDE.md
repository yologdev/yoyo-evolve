# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A self-evolving coding agent CLI built on [yoagent](https://github.com/yologdev/yoagent). The agent spans multiple Rust source files under `src/`. A GitHub Actions cron job (`scripts/evolve.sh`) runs the agent every 8 hours using a 3-phase pipeline (plan → implement → respond), which reads its own source, picks improvements, implements them, and commits — if tests pass.

## Build & Test Commands

```bash
cargo build              # Build
cargo test               # Run tests
cargo clippy --all-targets -- -D warnings   # Lint (CI treats warnings as errors)
cargo fmt -- --check     # Format check
cargo fmt                # Auto-format
```

CI runs all four checks (build, test, clippy with -D warnings, fmt check) on PR to main. A separate Pages workflow builds and deploys the website on push to main.

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

**Documentation** (`docs/`): mdbook source in `docs/src/`, config in `docs/book.toml`. Output goes to `site/book/` (gitignored). The journal homepage (`site/index.html`) is built by `scripts/build_site.py`. Both are built and deployed by the Pages workflow (`.github/workflows/pages.yml`), not during evolution.

**Evolution loop** (`scripts/evolve.sh`): 3-phase pipeline:
1. Verifies build → fetches GitHub issues (community, self, help-wanted) via `gh` CLI + `scripts/format_issues.py` → scans for pending replies on previously touched issues
2. **Phase A** (Planning): Agent reads everything, writes `SESSION_PLAN.md`
3. **Phase B** (Implementation): Agents execute each task (15 min each)
4. **Phase C** (Communication): Extracts issue responses from plan
5. Verifies build, fixes or reverts → posts issue responses → pushes

**Skills** (`skills/`): Markdown files with YAML frontmatter loaded via `--skills ./skills`. Four core skills (immutable) define the agent's evolution workflow:
- `self-assess` — read own code, try tasks, find bugs/gaps
- `evolve` — safely modify source, test, revert on failure
- `communicate` — write journal entries and issue responses
- `research` — internet lookups and knowledge caching

**Memory system** (`memory/`): Two-layer architecture — append-only JSONL archives (source of truth, never compressed) and active context markdown (regenerated daily by `.github/workflows/synthesize.yml` with time-weighted compression tiers):
- `memory/learnings.jsonl` — self-reflection archive. Each line: `{"type":"lesson","day":N,"ts":"ISO8601","source":"...","title":"...","context":"...","takeaway":"..."}`
- `memory/social_learnings.jsonl` — social insight archive. Each line: `{"type":"social","day":N,"ts":"ISO8601","source":"...","who":"@user","insight":"..."}`
- `memory/active_learnings.md` — synthesized prompt context (recent=full, medium=condensed, old=themed groups)
- `memory/active_social_learnings.md` — synthesized social prompt context
- Archives are appended via `python3` with `json.dumps()` (never `echo` — prevents quote-breaking). Admission gate: only write if genuinely novel AND would change future behavior.
- Context loaded centrally by `scripts/yoyo_context.sh` → `$YOYO_CONTEXT` (WHO YOU ARE, YOUR VOICE, SELF-WISDOM, SOCIAL WISDOM sections)

**State files** (read/written by the agent during evolution):
- `IDENTITY.md` — the agent's constitution and rules (DO NOT MODIFY)
- `PERSONALITY.md` — voice and values (DO NOT MODIFY)
- `JOURNAL.md` — chronological log of evolution sessions (append at top, never delete)
- `DAY_COUNT` — integer tracking current evolution day
- `SESSION_PLAN.md` — ephemeral, written by Phase A planning agent (gitignored)
- `ISSUES_TODAY.md` — ephemeral, generated during evolution from GitHub issues (gitignored)
- `ISSUE_RESPONSE.md` — ephemeral, agent writes this to respond to issues (gitignored)

## Safety Rules

These are enforced by the `evolve` skill and `evolve.sh`:
- Never modify `IDENTITY.md`, `PERSONALITY.md`, `scripts/evolve.sh`, `scripts/format_issues.py`, `scripts/build_site.py`, or `.github/workflows/`
- Every code change must pass `cargo build && cargo test`
- If build fails after changes, revert with `git checkout -- src/ Cargo.toml Cargo.lock`
- Never delete existing tests
- Multiple tasks per evolution session, each verified independently
- Write tests before adding features
