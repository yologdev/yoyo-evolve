# Safety & Anti-Crash Guarantees

How does a coding agent that edits its own source code avoid breaking itself?

Good question. yoyo has six layers of defense — from the innermost loop
(every single code change) to the outermost (protected files that can never
be touched). Here's how each one works.

## Layer 1: Build-and-test gate on every commit

No code change is ever committed unless it passes:

```bash
cargo build && cargo test
```

This happens inside the evolution session itself. The agent runs the
build and test suite after every edit. If either fails, the change
doesn't get committed — the agent reads the error and tries to fix it.

## Layer 2: CI on every push

Even after the agent commits locally, GitHub Actions runs the full
check suite on every push to `main`:

```
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

Clippy warnings are treated as errors (`-D warnings`), so even subtle
issues like unused variables or redundant clones get caught. If CI
fails, the next evolution session sees the failure and prioritizes
fixing it before doing anything else.

## Layer 3: Automatic revert on build failure

The evolution script (`evolve.sh`) has a post-session verification step.
After all tasks run, it re-checks the build. If it fails:

1. It gives the agent up to 3 attempts to fix the errors automatically
2. If all fix attempts fail, it reverts to the pre-session state:
   ```bash
   git checkout "$SESSION_START_SHA" -- src/
   ```

This means a broken session can never leave `src/` in a worse state
than it started. The revert is surgical — it only touches source files,
preserving journal entries and other non-code changes.

## Layer 4: Tests before features

yoyo's evolve skill requires writing a test *before* adding a feature.
This isn't just a guideline — the planning phase explicitly instructs
each implementation task to "write a test first if possible."

Why this matters: if you write the test first, you know the test
covers the new behavior. If you write the feature first, you might
write a test that only confirms what you already built, missing edge
cases.

## Layer 5: No deleting existing tests

The evolve skill has a hard rule: **never delete existing tests.**
Tests are the agent's immune system. Removing them would let
regressions slip through silently. As of this writing, yoyo has
91+ tests, and that number only goes up.

## Layer 6: Protected files

Some files are simply off-limits. The agent cannot modify:

| File | Why it's protected |
|---|---|
| `IDENTITY.md` | yoyo's constitution — defines who it is and its core rules |
| `PERSONALITY.md` | yoyo's voice and values |
| `scripts/evolve.sh` | The evolution loop itself — if this broke, recovery would be manual |
| `scripts/format_issues.py` | Input sanitization for GitHub issues |
| `scripts/build_site.py` | Website builder |
| `.github/workflows/*` | CI configuration — the safety net that catches everything else |

These files can only be changed by human maintainers. This prevents
a subtle failure mode: the agent "improving" its own safety checks
in a way that weakens them.

## What happens in practice

A typical evolution session:

1. `evolve.sh` verifies the build passes *before* starting
2. The planning agent reads source code, journal, and issues
3. Implementation agents execute tasks, each running build+test after changes
4. Post-session verification re-checks everything
5. If anything broke, automatic fix attempts kick in
6. If fixes fail, revert to pre-session state
7. CI runs on push as a final backstop
8. Next session checks CI status — failures get top priority

The result: yoyo has been evolving autonomously since Day 0, growing
from ~200 lines to ~3,100+ lines, without ever shipping a broken build
to `main`.

## Can it still break?

Theoretically, yes. Safety is defense-in-depth, not a proof of
correctness. Some scenarios the current system *doesn't* catch:

- **Logic bugs that pass tests** — if the test suite doesn't cover
  a behavior, the agent could change it without noticing
- **Performance regressions** — we rely on official leaderboards (SWE-bench, etc.) rather than custom benchmarks
- **Subtle UX regressions** — the agent tests functionality, not
  user experience

These are areas for future improvement. But for the core guarantee —
"the agent won't commit code that doesn't compile or pass tests" —
the six layers above make that extremely unlikely.
