Title: Prepare v0.1.3 release — bump version and update CHANGELOG
Files: Cargo.toml, CHANGELOG.md
Issue: none

## Context

Since v0.1.2 (Day 22), significant features have been added across Days 22-24:
- `/watch` command — auto-run tests after agent edits
- `/ast` command — ast-grep structural search integration
- `/refactor` umbrella command — groups /extract, /rename, /move
- `rename_symbol` agent tool — project-wide rename via the model
- Terminal bell notification — `\x07` after long operations (with --no-bell flag)
- `system_prompt` / `system_file` config keys in `.yoyo.toml`
- Git-aware system prompt — agent knows current branch and dirty state
- Per-turn `/undo` — undo by turn instead of all-or-nothing
- Onboarding wizard improvements — Cerebras provider, XDG config paths
- Streaming contract tests — 10 tests pinning buffering behavior
- Suppress partial tool output in piped mode (Task 1 of this session)
- Reduced tool output truncation in piped mode (Task 2 of this session)

## Implementation

### 1. Bump version in Cargo.toml

Change `version = "0.1.2"` to `version = "0.1.3"` in `Cargo.toml`.

### 2. Update CHANGELOG.md

Add a new `## [0.1.3] — 2026-03-24` section at the top (after the header), with:

**Added:**
- `/watch <command>` — auto-run tests after every agent turn that modifies files (Day 23)
- `/ast <pattern>` — structural code search via ast-grep integration, graceful fallback when `sg` not installed (Day 24)
- `/refactor` umbrella — groups `/extract`, `/rename`, `/move` under one discoverable entry (Day 23)
- `rename_symbol` agent tool — model can do project-wide renames in a single tool call (Day 23)
- Terminal bell notification — rings `\x07` after operations > 3s; disable with `--no-bell` or `YOYO_NO_BELL=1` (Day 23)
- `system_prompt` and `system_file` keys in `.yoyo.toml` config (Day 23)
- Git-aware system prompt — agent automatically sees current branch and dirty-file status (Day 23)

**Improved:**
- Per-turn `/undo` — undo individual agent turns instead of all-or-nothing (Day 22)
- Onboarding wizard — added Cerebras provider, XDG user-level config path option (Day 22)
- Streaming latency — tighter flush logic for digit-word and dash-word patterns (Day 23)

**Fixed:**
- Suppressed partial tool output in piped/CI mode — eliminates ~6500 noise lines from CI logs (#172)
- Reduced tool output truncation from 30K to 15K chars in piped mode — cuts context growth rate to prevent 400 errors (#173)

### 3. Run cargo publish dry-run

After updating:
```bash
cargo publish --dry-run
```

Verify no warnings.

### 4. Update README.md stats if needed

Check current line count, test count, command count and update the README's stats section if they've changed significantly.

## Verification

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
cargo publish --dry-run 2>&1 | tail -5
```
