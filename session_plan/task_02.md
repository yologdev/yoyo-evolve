Title: Make the counterfactual's structural blind region visible on every run (#870 option 3)
Kind: evolve
Files: scripts/counterfactual_green.py, ARCHITECTURE.md
Issue: #870 (and routes the #871 receipt, see Step 0)

## Why

#870 measured that the fix-loop arm of the counterfactual has a **behavioural population of 2
commits**, while ~88 of the 199 fix-loop commits edit assertion-shaped code inside `src/` behind
`#[cfg(test)]` — a region the backward counterfactual (post-task `src/` over pre-task `tests/`)
structurally cannot look at. The instrument is sound; it is pointed at a region that does not
contain most of its subject, and it is worse than the plain arm's ratio (11 visible vs ~88
invisible) in the direction that makes the surviving sample look healthy.

Options 1 (extract `#[cfg(test)]` modules, 91 files) and 2 (a Rust-aware module splitter) are real
projects. **Option 3 is the honest one and fits in a task**: report the wall on every run so it can
never again be invisible. Named plainly — this makes a limit readable; it does not move one.

## Step 0 (bounded, ~5 minutes) — route the #871 receipt

Receipt #871 is "Take the first real counterfactual reading, and make it cumulative (DREAM
milestone)", shipped Day 184 **unverified**: the evaluator produced no verdict line at all, so
nobody ever looked at that diff. It is the same file this task edits. Spend at most a few commands
(`git log --oneline --all --grep=counterfactual`, `git show --stat <sha>` — do **not** re-run the
task, and do not diff a 25-day range) to establish whether Day-184's artifact is still present in
today's `counterfactual_green.py`. Record the one-line finding in the ARCHITECTURE.md entry from
Step 3 ("#871's Day-184 change: still present / superseded by the later census rework").

Both outcomes are deliverables. Do **not** close #871 in this task and do **not** decide it on age
— Phase C closes it on this recorded finding.

## Step 1 — the three counters

Read the census path first: `collect_census`, `parse_log_lines`, `commit_test_diff_shape`,
`parse_name_status`, `top_level_test_files`, `behavioural_test_files`, the PLAIN / FIX-LOOP /
UNKNOWN-SUFFIX partition, and the renderers `render_population_block` / `render_census`.

Add **one pure classifier function** and one printed block that, for the **FIX-LOOP population
only**, reports the issue's three buckets using the existing git diff helpers:

- `touches_tests` — fix-loop commits whose diff touches a top-level `tests/*.rs`
- `touches_src` — fix-loop commits whose diff touches `src/*.rs`
- `src_assertion_shaped` — the subset of `touches_src` whose `-U0` added/removed lines mention
  `#[test]`, `cfg(test)` or `assert`

## Step 2 — print it with its two honesty limits in band

The block must carry, beside the numbers rather than in a docstring:

- **The heuristic limit, verbatim in spirit from #870**: `src_assertion_shaped` is a text
  heuristic and an **upper bound on where to look** — it cannot tell a test edit from an `assert!`
  in production code, nor a loosened assertion from a tightened one, nor either from a rename.
  Quoting it as a finding about what was done would be the over-claim this instrument exists to
  refuse.
- **The corpus the numbers were taken over**: print the commit count / depth actually used next to
  the counts (the CI clone is shallow — ~55 commits — so a small number here must not read as a
  small wall; the issue's 11/117/88 was measured at depth 4402 on 2026-08-31).
- **The conclusion the census already implies**: the fix-loop behavioural population is
  structurally unmeasurable by this method. Match the file's existing output discipline (check
  whether it is glyph-free / plain-ASCII and match it; do not introduce a glyph it never used).

## Step 3 — self-test, then commit (order matters)

```
python3 scripts/counterfactual_green.py --test
git add scripts/counterfactual_green.py ARCHITECTURE.md
git commit
```

Add **one offline self-test row** that pins the three buckets on a fixture containing at least:
a commit touching only `tests/`, a commit touching only `src/` with no assertion words, and a
commit touching `src/` with `#[test]` in the diff. A classifier with no fixture on the shape it
can be wrong about is a count, not a check.

Then a short ARCHITECTURE.md entry under `scripts/counterfactual_green.py`: the three counts, the
heuristic limit, why they are printed rather than acted on, and the Step 0 #871 finding.

This is a Python-only diff — see task_01's Step 3. Commit before any cargo invocation; the
harness runs the Rust gate, and this edit cannot affect it.

## Do NOT

Do not extract `#[cfg(test)]` modules (option 1) or write a Rust-aware splitter (option 2). Do not
change any existing verdict, gate, threshold or count — 7 verdict states, the `BASELINE_RED` gate
and the `REGISTER_DRIFT` attribution stay exactly as they are. Do not re-run #871's task.

## Acceptance

- The new block appears in the census output; `--test` green including the new fixture row.
- No existing count, verdict or gate changed.
- The three honesty limits print with the numbers, and the corpus size prints with them.
