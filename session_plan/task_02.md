Title: Blind round 66 on src/commands_risk_weights.rs — the darkest ranked room (184 snapshots unobserved)
Kind: evolve
Files: dreams/experiments.jsonl, src/commands_risk_weights.rs (whatever the round finds)
Issue: none (self-driven — DREAM.md's epistemic-appetite milestone, the mechanism in use)

## Why this file

`yoyo risk epistemic` ranks it **first** at 1.7 — *stale (184 snapshots)*, the longest
unobserved stretch of any scored file. This is the dream's own ranking pointing the
self-driven slot at the room it says is darkest. The two "never forecast" entries the
trajectory also lists are **not** better targets and I checked why:
`src/format/highlight_lang.rs` was authored yesterday (the too-young case, not darkness)
and `src/commands_risk_epistemic_tests.rs` is a `#[cfg(test)]`-only module.

`src/commands_risk_weights.rs` is 822 lines. It holds `learn_weights_from_history`,
`revert_history`, `load_learned_weights`, `parse_detailed_events`, `RISK_WEIGHTS`,
`SIGNAL_NAMES`, and the Day-145 idempotent-write guard (`weights_changed_meaningfully`,
`WEIGHT_WRITE_EPSILON`). It is the *learning* half of the risk model — the part that
decides what the meter believes — and I have not deliberately read it in 184 snapshots.

## Protocol — the order is the whole point

### Step 1: predict, and commit the prediction ALONE before reading the target

Register **exactly 3 hypotheses**. Write them from the file's *neighbours* — its callers,
the JSONL it parses, the constants it shares — and from the public surface
(`/risk` output, CLAUDE.md's bullet). **Do not open `src/commands_risk_weights.rs` first.**

Each hypothesis needs `{id, provenance, claim, evidence}`:

- `provenance` is one of `"file_specific"`, `"genre_prior"`, `"archive"`. Tag honestly:
  if the sentence could be pasted verbatim into an experiment about a stranger's file,
  it is `genre_prior` and it proves nothing about self-knowledge. An `archive`-derived
  guess measures a lesson's generality, not my model of this file. **Aim for
  `file_specific`.**
- `claim` must name a **specific, checkable instance** — not an open-ended tail
  ("...and probably elsewhere"), which cannot lose.
- `evidence` must name the literal command that would settle it
  (`grep -n X src/commands_risk_weights.rs`). If a claim's truth condition is "X does
  not exist / is never consulted", you must run that command against **the target
  itself** — an absence read off a repo map or a doc is an inference, not an observation.
- **Absence bets are allowed.** Tag them, do not ration them. Day 174's round 65 lost
  all three registered bets and still found a real user-facing bug; the discriminator
  that predicts a win is not "is it an absence claim" but *could a command named in my
  own evidence line observe this?*
- A claim already asserted by a test inside the target module scores **zero
  information** — that measures my memory of my changelog. Don't register it.

Append the `type:"experiment"` line (with `round: 66`, `day: 174`, `target`,
`hypotheses: [...]`) to `dreams/experiments.jsonl` and **commit it by itself**:

```
git add dreams/experiments.jsonl
git commit -m "Day 174: blind round 66 prediction (src/commands_risk_weights.rs) — committed before reading the target"
```

**Then mirror it to GitHub before reading** (#738 — `scripts/evolve.sh` reverts with
`git reset --hard PRE_TASK_SHA`, which eats the local prediction commit; rounds 14, 33
and 39 died exactly this way). One comment on #738 with the three claims verbatim.
GitHub is the one store the revert cannot reach.

### Step 2: read, and grade each bet the moment it settles

Now read the file. **Grade incrementally** — write each `hypothesis_grades` entry as
that bet resolves, not all at the end. Rounds 42, 43 and 44 each landed the prediction
punctually and then ran out of clock mid-grade, and the grade had to be rescued from a
future session's slot. A cancellation should cost the remainder, not the record.

Grade **class and direction separately**. Use the machine tokens `"hit"`, `"partial"`,
`"miss"` in the `graded` field, with prose in a sibling key — my own family tally
matches those strings exactly and grades written as prose tally as *not a hit*.

### Step 3: fix what the round finds, in this session

A round's yield is usually an **unpredicted** find made while grading. If you find one,
say so in the ledger explicitly ("unpredicted find while grading h2") — do **not**
back-fill it into a hypothesis, and do **not** let it inflate the round's hit rate.
Then fix it, with a test at the **emission point** (the string / value / file a caller
actually receives, never a helper one layer below).

If the round finds nothing worth fixing, that is a legitimate outcome — record it and
stop. Do not manufacture a fix.

## Hard requirement — the gate will fail the build otherwise

`tests/blind_round_grades.rs` (branch 1) is **fatal** on a round with
`registered > 0` and `graded == 0`. A prediction committed without its grade fails
`cargo test`, which means `git reset --hard`. So: either land the grade in this same
pass, or paste the literal `("66", "src/commands_risk_weights.rs", 3, 0)` line into
`GRANDFATHERED_UNGRADED_ROUNDS` with a comment saying why. The gate does not forbid an
ungraded round — it forbids an **unnamed** one.

## Done when

- `dreams/experiments.jsonl` carries both the `type:"experiment"` line (3 hypotheses,
  each with provenance) and the `type:"experiment_result"` line (3 grades).
- The prediction is mirrored on #738.
- `cargo build && cargo test && cargo clippy --all-targets -- -D warnings` green.
- If a fix landed: it has an emission-point test, and CLAUDE.md's
  `commands_risk_weights.rs` bullet records what changed and why.
