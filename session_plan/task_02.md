Title: Release v0.1.5 — fallback fix, Bedrock, /map, inline hints
Files: Cargo.toml, CHANGELOG.md
Issue: none

## Context

v0.1.4 shipped on Day 28. Since then, Days 29-32 accumulated significant changes:

**Added:**
- `/map` command — structural repo map with ast-grep backend and regex fallback (Day 29)
- AWS Bedrock provider — full end-to-end support with BedrockConverseStream (Day 30)
- REPL inline command hints — type `/he` and see dimmed `lp — Show help` (Day 30)
- `--fallback` provider failover — auto-switch to backup provider on API failure (Day 31)

**Improved:**
- Hook system extracted into `src/hooks.rs` — Hook trait, HookRegistry, AuditHook, ShellHook (Day 31)
- Config loading consolidated — single `load_config_file()` eliminates 3 redundant reads (Day 31)

**Fixed:**
- Permission prompt hidden behind spinner (Issue #224) — stop spinner before prompting (Day 30)
- MiniMax stream duplication — exclude "stream ended" from auto-retry (Issue #222) (Day 30)
- `write_file` empty content — validation + confirmation prompt for empty writes (Issues #218, #219) (Day 30)
- **`--fallback` in piped mode** — fallback retry now works in piped and --prompt modes, with proper non-zero exit codes on failure (Day 32, Issue #230) ← from Task 1

## What to do

1. **Verify all gates pass:**
   ```bash
   cargo build 2>&1 | tail -1
   cargo test 2>&1 | tail -1
   cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
   cargo fmt -- --check && echo PASS
   ```

2. **Update version in Cargo.toml:** Change `version = "0.1.4"` to `version = "0.1.5"`

3. **Write CHANGELOG.md entry:** Add `## [0.1.5] — 2026-04-01` section at the top (below the header, above `[0.1.4]`). Use the changes listed above, organized into Added/Improved/Fixed sections following the existing format.

4. **Check README.md** is still accurate — the new features (Bedrock, /map, --fallback) should be mentioned if they're significant enough. At minimum verify the existing feature list doesn't claim things that aren't true.

5. **Commit:** `git add Cargo.toml CHANGELOG.md && git commit -m "v0.1.5: fallback fix, Bedrock, /map, inline hints"`

6. **Tag:** `git tag v0.1.5`

7. **Do NOT run `cargo publish`** — the CI release workflow handles that on tag push.

## Important notes

- This task depends on Task 1 shipping successfully. If Task 1 was reverted, remove the "fallback in piped mode" fix from the changelog and still release with the other accumulated changes.
- Follow the existing CHANGELOG format exactly (see v0.1.4 entry for style).
- Don't modify any source files — this is metadata only.
