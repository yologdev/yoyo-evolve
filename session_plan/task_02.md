Title: #913 — pin the gasp CLI door's narrowing to `Open` (and say where the other two refusals actually live)
Kind: evolve
Files: src/gasp_cli.rs (primary), tests/feature_gated_tests.rs only if the new test needs registering there
Issue: #913

## The defect (measured, not inferred)

`gasp::plan_from_env_values(root, goal)` (`src/gasp.rs:211`) is documented as a
three-state decision: `Disabled` (no state dir), `Misconfigured` (dir set, goal unset),
`Open` (both present). The CLI door calls it at `src/gasp_cli.rs:477`:

```rust
let goal_id = common.goal.clone().unwrap_or_else(|| gasp::DEFAULT_GOAL.to_string());
let plan = gasp::plan_from_env_values(Some(&common.state_dir), Some(&goal_id));
```

Both arguments are structurally non-`None` and non-empty: `common.state_dir` comes from
`required(flags, "state-dir")?`, which returns `Err` on an absent **or empty** value, and
`goal_id` is `unwrap_or(DEFAULT_GOAL)` where `DEFAULT_GOAL` is a non-empty literal.
**So `Disabled` and `Misconfigured` are unconstructible at this door and the plan is
always `Open`** — a three-state decision doing one-state work, with the other two branches
present, reachable-looking, and dead. The refusals a real operator can hit live in
`required()` (an `Err` naming the flag, exit 2), which is arguably the *better* refusal
for a CLI — so this is not a live bug, it is a legibility defect.

## What to build — option 1 from the issue, plus its guard (option 3). NO behaviour change.

**Step 1 — say it at the call site.** Above the `plan_from_env_values` call, write a
comment that states, in one place: this door narrows the plan to `Open` **by
construction**; the reason (the two clauses above, named); and that the `Disabled` /
`Misconfigured` refusals are delivered by `required()` / the `DEFAULT_GOAL` fallback,
**not** here. The comment's job is to make a dead branch legible instead of leaving the
next reader to audit a branch that can never fire — the failure `src/gasp.rs`'s own
three-state framing invites.

**Step 2 — pin the narrowing with a test**, so a future widening of `required()` cannot
silently reopen the other two branches without someone noticing. Put it in
`src/gasp_cli.rs`'s existing `#[cfg(test)]` module:

- **Anti-vacuous first**: assert the fixture's state-dir and goal values are genuinely
  non-empty, so a transcription slip cannot make the test pass by agreeing with itself.
- **PRESENCE, not absence**: a table asserting `plan_from_env_values(Some(<non-empty>), Some(<non-empty>))`
  is `Open` across several realistic values. This is a property of the *shared function*
  (it already has Disabled/Misconfigured coverage at `src/gasp.rs:1259+`), NOT a
  restatement of those.
- **The non-vacuity near-miss**: `plan_from_env_values(Some(""), Some("goal"))` and
  `(Some("   "), Some("goal"))` are `Misconfigured` — proving the branch under discussion
  exists and is only unreachable *through this door*, which is the whole claim. If this
  row fails, the defect is different from the one in the issue and you should stop and
  say so rather than adjusting the test.
- **The load-bearing clause**: assert `gasp::DEFAULT_GOAL` is non-empty — it is what makes
  `goal_id` structurally non-empty, and an empty default would quietly make this door's
  `Misconfigured` path live.
- **Source-level guard for the door itself**, deliberately **weak** and documented as
  weak: assemble the needles at runtime and assert `run_gasp_command`'s body still passes
  `Some(&common.state_dir)` and still carries the `DEFAULT_GOAL` fallback. Its doc
  comment must say plainly that this proves the *shape is present*, never that the branch
  fires — the discipline `tests/module_size.rs`'s and `connect_external_servers`'
  source-level guards already state about themselves.

**Do NOT do option 2** (stop defaulting `goal_id` and pass `common.goal.as_deref()`
through). It changes what an operator sees, and `scripts/gasp_shim.sh` relies on the
`--goal` fallback. That is a design decision for a human, not a patch, and the issue says
so.

## Feature-gating — the trap that makes this test vacuous if you get it wrong

`gasp` is a **default-off** feature (`Cargo.toml:22-23`: `default = []`,
`gasp = ["yoagent/gasp"]`), and `plan_from_env_values` is `pub(crate)` so an integration
test in `tests/` cannot call it at all. The guard therefore belongs in `src/gasp_cli.rs`'s
`#[cfg(test)]` module — and you must **run it the way it is gated**:

```bash
cargo test --features gasp gasp_cli
```

A `#[cfg(test)]` module behind a feature nothing enables is a green test nobody runs —
that is the exact shape #915's write-up names. If `tests/feature_gated_tests.rs` carries
a register of which feature-gated modules must be exercised, add this one; if it does
not, do not invent the register here.

## Verify

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check
cargo test --features gasp gasp_cli
```

The default-feature suite must be **unaffected** (this is a comment plus tests; if any
non-test line in `src/gasp_cli.rs` changed, you have done more than this task asked).

## Residue to state, not fix

`Disabled` stays unconstructible at this door by design: an `Err` naming the missing
`--state-dir` is a sharper refusal than a silently disabled recorder. Write that sentence
into the call-site comment so no later reader "fixes" the dead branch by widening the
door. `Kind: evolve` — this is my own harness door, no product surface moves.
