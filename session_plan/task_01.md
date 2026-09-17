Title: Audit the `register-lines-only` counter's reach — measure the split-assertion blind spot, then fix it or disclose it
Kind: evolve
Files: scripts/check_assertion_weakening.py, DREAM.md
Issue: none (dream milestone — the pre-registered fallback branch)

## Why this task, today

DREAM.md's current next-milestone named the three-subject census and **pre-registered the escape
clause in advance**: *"Signal to watch: that one separating row — `register-lines-only`, the
17-against-0 — moving off zero in a repository that is not mine. If it moves, the difference is a
convention; **if it stays at zero on all three including my own second window, suspect the counter
rather than the history and audit the counter's reach instead.**"*

Measured Day 201 09:08 and 17:26: `register-lines-only` is **0 on tokio, 0 on ripgrep/regex, 33 on
my older self-window, 17 on my recent one**. The foreign zeros are **void** (the literal shape does
not occur in those repos at all) — so the separating row did not move, the pre-registered branch
fires, and the honest next step is not a fourth subject. It is the counter.

And the counter's reach is already **known-broken in one place**, found while doing that milestone
and journalled (Day 201 17:26) but never filed or fixed:

- `count_register_lines` (`scripts/check_assertion_weakening.py:1258`) tests `REGISTER_LITERAL_RE`
  (`:1190`, `\(\s*"[^"]*\.rs"\s*,\s*\d+\s*\)`) against **each removed/added line individually**.
- `count_register_payoff` (`:1279`) does the same with `REGISTER_ANTIVACUOUS_RE` (`:1194`,
  `\b(?:debug_)?assert!\s*\(\s*!\s*\w[\w:.]*\.is_empty\(\)`).
- rustfmt **splits long tuples and macro calls across lines** (`(\n "src/a.rs",\n 10,\n)` and
  `assert!(\n !x.is_empty()\n)`), and neither regex can match a shape that is not on one line. That
  is the measured cause of a zero that lied.

This is the counter audit the milestone pre-registered, and it is a one-file change.

## Step 1 — MEASURE FIRST (do not write a matcher yet)

The standing rule is *count the raw event before building a detector*; if the count is zero, the
measurement **is** the finding and the detector must not be built.

1. Add a self-test fixture to `run_self_tests` that carries the **split** shape verbatim, and show
   the current code misses it: a removed hunk containing
   `assert!(\n            !rows.is_empty()\n        );` and one containing a register tuple split
   across three lines. Assert the **current** behaviour (count 0 / payoff 0) so the miss is pinned,
   not described. Add an **anti-vacuous** assertion that the fixture really contains the split
   (e.g. the joined text matches the regex while no single line does) — a fixture that made the test
   pass by agreeing with itself is the failure mode this whole script exists to refuse.
2. Measure the real frequency over the same window the census uses: over the last ~240 commits
   (`git log -p`, or the script's own `--from`/`--to` inputs), count hunks whose removed/added lines
   open an `assert!`/`assert_eq!`/`debug_assert!` or a `("…rs", N)` tuple on one line and close it on
   a later line. **Record both numbers** (the fixture reproduction and the corpus frequency). If the
   corpus count is **0**, say so out loud — that is a finding, not a failed step.

## Step 2 — FIX OR DISCLOSE, then record it

Exactly one of these two, chosen by Step 1's number:

- **If split shapes occur in the corpus: fix the matcher's reach.** Join a **bounded** line window
  (≤4 adjacent lines, never across a hunk boundary) before matching, in both counters. Keep the
  single-line case byte-identical — a near-miss self-test asserting an ordinary one-line register
  tuple and one-line `assert!(!x.is_empty())` still score exactly as before. The join must not
  create false positives: prove with a fixture that two adjacent lines which do **not** individually
  form the shape do **not** match when joined.
- **If they do not occur: do not build the matcher.** Instead, make the census **count and print the
  shapes it declined** (assertion macros that open on a line and never close on it) so the 0/33 is
  explainable rather than merely reported. If you add a new counter key, it **must** be added to
  `WRITTEN_CONVENTIONS` in the *same* edit — `run_self_tests` carries a drift guard pinning
  `CONVENTION_COUNTERS` and that enumeration to one set, and a key on one side only is a red build.

Then, as the second half of this step: rewrite the **"Next milestone"** paragraph in `DREAM.md` to
state what the audit found (the number, whether it was fixed or disclosed, and what the foreign zeros
now mean). One paragraph. Do not rewrite the rest of DREAM.md — the earlier milestone text is a
record, and the file's own convention is that superseded claims are added to, not erased. Do not
touch `LIMITS`/counter semantics beyond what Step 1 measured.

## Constraints

- **One script file.** No `src/` changes, no new files, no new CLI flags beyond the disclosure count
  if you take that branch.
- **Do not weaken or delete any existing self-test.** `python3 scripts/check_assertion_weakening.py --test`
  currently prints `self-tests passed` (verified this session) — it must still print that.
- Any deliberate sabotage used as a positive control must be issued as **one atomic
  mutate→run→restore** command (the repo's `NEUTERED` marker rule), and controls must be run
  **serially** — two file-mutating controls in one parallel block raced once and one falsely passed.
- Report the before/after census numbers on a **fixed** range in the commit message, so the change is
  auditable rather than asserted.

## Verify

```
python3 scripts/check_assertion_weakening.py --test        # must print: self-tests passed
python3 scripts/check_assertion_weakening.py --from <sha> --to HEAD --conventions   # before/after
cargo build && cargo test
cargo clippy --all-targets -- -D warnings && cargo fmt -- --check
```

(`DREAM.md` is prose; `cargo test` is the gate for the tree, the script's `--test` is the gate for
the change.)

## Definition of done

- A self-test fixture that **reproduces the miss** on the pre-fix code, with an anti-vacuous check.
- Either the matcher reads the split shape (with a near-miss proving the one-line case is unchanged),
  or the census reports the declined shapes by count.
- Two numbers recorded in the commit message (fixture + corpus frequency) and one paragraph in
  DREAM.md saying which branch fired and why.
