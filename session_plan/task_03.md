Title: Update gap analysis with accurate stats and recently completed features
Files: CLAUDE_CODE_GAP.md
Issue: none

## Context

CLAUDE_CODE_GAP.md is significantly stale:
- Claims 22,989 lines — actual is 32,292
- Claims 1,039 tests — actual is 1,372 (1,291 unit + 81 integration)
- Claims 50 REPL commands — needs recount
- Claims v0.1.2 — we're at v0.1.3
- Missing features completed since Day 22: `/watch`, `/ast`, `/refactor`, `/todo` (if task 1 lands), `rename_symbol` agent tool, terminal bell, `system_prompt`/`system_file` config keys, git-aware prompts, proactive context compaction, `/apply`, `/stash`, streaming flush improvements, piped mode improvements

## Implementation

### 1. Recount actual stats

Run these to get accurate numbers:
```bash
wc -l src/*.rs                    # total source lines
cargo test 2>&1 | grep "^test result"  # test counts
grep -c "^    \"/" src/commands.rs     # REPL commands (approximate)
```

### 2. Update the stats section at the bottom

Update all numbers:
- Total lines of Rust
- Number of source files (14 now)
- Test count (unit + integration)
- REPL command count
- CLI flag count
- Provider count
- Current published version

### 3. Update feature tables

Mark newly completed items:
- `/watch` — new feature (auto-run tests)
- `/ast` — new feature (structural search)
- `/refactor` — new feature (umbrella command)
- `rename_symbol` as agent tool — new capability
- `/stash` for conversation stash — new feature
- `/apply` for patches — new feature
- Proactive context compaction — improved from "auto-compact at 80%" to "proactive at 70% + auto at 80%"
- Git-aware system prompt — agent always sees branch/dirty state
- Terminal bell notifications — new UX feature
- Config file system_prompt/system_file — new config capability

### 4. Update the Priority Queue section

Remove items that are done, add the remaining genuine gaps:
- TodoRead/TodoWrite equivalent (if task 1 lands, mark as done)
- Real-time subprocess output streaming (still 🟡)
- Full graceful degradation (still 🟡)

### 5. Update "last updated" date

Change to "Day 24 (2026-03-24)"

### No tests needed — this is documentation-only.
