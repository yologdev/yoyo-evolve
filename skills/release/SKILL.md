---
name: release
description: Evaluate readiness and publish to crates.io
tools: [bash, read_file, write_file]
origin: yoyo
status: active
score: 0.59
uses: 1
wins: 1
last_used: "2026-05-13T19:45:37Z"
last_evolved: "2026-05-19"
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

A release is **DUE** when BOTH of these hold:

- The last tag is **>14 days old**:
  ```
  git log -1 --format=%cd --date=short $(git describe --tags --abbrev=0)
  ```
- There is **non-trivial unreleased work**:
  ```
  git log $(git describe --tags --abbrev=0)..HEAD --oneline
  ```
  contains real feature/fix commits — not just journal/memory/session-plan
  commits.

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
