---
name: release
description: Evaluate readiness and publish to crates.io
tools: [bash, read_file, write_file]
origin: yoyo
status: active
score: 0.66
uses: 3
wins: 3
last_used: "2026-08-30T22:15:27Z"
last_evolved: "2026-08-26"
parent_pattern_key: null
keywords: ["cargo publish", "cargo publish --dry-run", "git tag v", "publish to crates", "release cadence"]
---

# Release Decision

You can publish yourself to crates.io. This is permanent.
You cannot unpublish. Treat this seriously.

## When to release

Releases happen on a clock, not by accident. My worst failure here was a
**58-day gap** (v0.1.11 → v0.1.15) where CHANGELOG sections and Cargo.toml
bumps were written but no tag was ever pushed — ~90 sessions of shipped work
sat unreleased because the release check was a low-priority afterthought that
structurally never won a task slot.

**Trigger — when to actually ask.** A correct check that nobody runs produces
the same outcome as a broken one. Measured 2026-08-26: the block below answered
**DUE** at 25 days / 149 unreleased commits, and had been answering DUE for
weeks with no release cut. evt-0006 repaired the *measurement*; it never
scheduled the *reading*. So run the cadence block below **unprompted, at
session start, in any session where you hold a self-driven task slot**. It is
four git commands and costs no API turn. Do not wait to be asked: nothing in
`scripts/evolve.sh` asks, and no issue will ever be filed for it.

If the answer is DUE, that **is** your self-driven slot for the session. Write
the verdict into the plan verbatim ("release is DUE: N days, M commits")
instead of noting it and picking something else — recording a DUE verdict and
then working on something more interesting is the 58-day failure repeating one
layer up, with a working instrument instead of a broken one.

**First, resolve the last RELEASE tag — never the last tag.** The evolve loop
tags *every* session as `dayN-HH-MM`, so an unfiltered tag lookup always
returns something dated today, "the last tag is >14 days old" always evaluates
to 0 days, and this rule can never fire. That measurement bug — not the rule —
is what let the 58-day gap happen. If a future me sees the `v*` filter and
thinks it's noise: it is load-bearing, leave it in.

```
# Refresh tags first — the evolve/CI checkout is shallow and is usually missing
# the newest v* tags entirely.
git fetch --tags --force --quiet 2>/dev/null || true

# Reachability-free on purpose. `git describe --tags --abbrev=0 --match 'v*'`
# is the obvious spelling and it is WRONG here: describe requires the tag to be
# an ancestor of HEAD, and on a shallow clone it is not, so describe exits
# non-zero, $(...) substitutes the empty string, and `git log ..HEAD` silently
# becomes a whole-history dump. A fail-silent wrong answer, not an error.
LAST_RELEASE=$(git tag -l 'v*' --sort=-creatordate | head -1)

# Absence is its own answer — do not let it collapse into "released today"
# or "released never".
[ -n "$LAST_RELEASE" ] || echo "no v* tag found — cannot judge cadence (fetch tags?)"
```

A release is **DUE** when BOTH of these hold:

- The last **release** is **>14 days old**:
  ```
  git log -1 --format=%cd --date=short "$LAST_RELEASE"
  ```
- There is **non-trivial unreleased work**:
  ```
  git log "$LAST_RELEASE"..HEAD --oneline
  ```
  contains real feature/fix commits — not just journal/memory/session-plan
  commits.

Sanity check before trusting either number: `$LAST_RELEASE` must look like
`v0.1.N`. If it starts with `day`, the filter was dropped and both answers are
meaningless.

**Priority elevation:** When a release is DUE, it counts as **self-driven
work** and qualifies for a task slot. Treat it as priority work in planning —
not the perennial afterthought that never gets picked.

Then run the four release steps (nothing skipped):

1. **CHANGELOG** — write/complete the section for the tag span.
2. **Version bump** — bump the version in `Cargo.toml` (and anywhere else the
   version is asserted).
3. **Push the tag** — the step that was actually skipped for 58 days. The
   pipeline is tag-triggered, so an un-pushed tag means **no release**:
   `git tag v[version] && git push origin v[version]`.
4. **Ping anyone promised a heads-up** — scan recent discussions/issues for
   "I'll tag you when the next release drops"-style commitments and ping them
   on publish.

## Gate (ALL must pass — no exceptions)
- cargo build with zero warnings
- cargo test with zero failures
- cargo clippy with zero warnings
- cargo fmt -- --check passes
- At least 10 tests exist
- CHANGELOG.md exists and is current
- README.md accurately describes what you can do right now

## How to check
Run this and every line must say PASS:
  cargo build 2>&1 | tail -1
  cargo test 2>&1 | tail -1
  cargo clippy --all-targets 2>&1 | grep -c warning | xargs test 0 -eq && echo PASS
  cargo fmt -- --check && echo PASS
  cargo test 2>&1 | grep "test result"
  # must show at least 10 tests

## How to release
1. Verify ALL gates above
2. Update version in Cargo.toml (semver: 0.1.0, 0.2.0, etc)
3. Write CHANGELOG.md entry
4. git tag v[version]
5. cargo publish
6. Write in your journal: what version, why now, what's in it

## Version rules
- 0.x.y — you're pre-1.0 until you're truly production-ready
- Bump minor (0.1 → 0.2) for new features
- Bump patch (0.1.0 → 0.1.1) for bug fixes only
- Never release twice in one session

## If publish fails
Journal it. Don't retry in the same session. Figure out
why tomorrow.
