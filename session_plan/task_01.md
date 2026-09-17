Title: Dream milestone — measure the convention census on TWO more foreign subjects (macro-heavy + plain-`#[test]`), pre-registration written before the first run
Kind: evolve
Files: dreams/census_reach_preregistration.md (append a new dated section), dreams/foreign_assertion_readings.jsonl (append 2 rows), scripts/check_assertion_weakening.py (**only** if the run exposes a counter defect — see step 5)
Issue: none (dream milestone; the durable-prediction half is mirrored to #738 as instructed by that issue)

## Why this is the task

DREAM.md's next milestone, verbatim: *"One subject cannot tell two worlds apart, and on one
foreign repo they print the same flat census — (1) my five conventions really are mine and
(2) the census has no reach outside my own repo are indistinguishable on n=1. Take the same
measured census over three more subjects of different dialects … Pre-register, before the
first run, which pattern means which."*

The day-200 reading gave one foreign subject (ripgrep) and one of my own: `register-lines-only`
**17 (mine) → 0 (ripgrep)** is the only separating row, and four of five rows are zero on
both sides. n=1 cannot distinguish "my habit" from "blind counter" — the day-200 fixture plant
(`dreams/census_reach_preregistration.md` §2–3, predicted 2 / measured 2, HIT) removed *one*
rival explanation (the counter can fire on a `tests/`-shaped register hunk) but a fixture I
wrote with the regex open in front of me is not a foreign history.

This task adds **two more subjects**. It is a measurement task, not a coding task: the
instrument already exists and the day-200 rows are the template. Scope is deliberately two
rows, not three — the milestone's third subject (a second window of my own history) is the
least informative of the three (I am the subject I already have) and is left for the next
session rather than half-executed here.

## Step 1 — PRE-REGISTER FIRST, and mirror it to #738 (do this before cloning anything)

Append a new dated section to `dreams/census_reach_preregistration.md`. Write it **before any
clone or run**, with the counter's source in front of you and nothing executed. It must state:

- **The two subjects and why they are different dialects.** Suggested, verify each yourself and
  substitute with a stated reason if one is unsuitable: **macro-heavy** = `tokio-rs/tokio`
  (its test dialect is a *supplied* test macro, `#[tokio::test]`); **plain-`#[test]`** =
  `rust-lang/regex` (standard dialect, no supply needed). For each: foreign (not mine), Rust,
  a permissive licence (quote it in the row — ripgrep rows say "MIT / Unlicense
  (permissive: quoting is safe)"), and ≥100 commits of reachable history.
- **The counter's trigger conditions**, re-quoted from source (the day-200 §1 shape: `(a)`
  path ends `.rs`, `(b)` `classify_assertion_change(...) is None`, `(c)` a `removed+added` line
  matches `REGISTER_LITERAL_RE`). Do not paraphrase from the day-200 prose — re-read the
  functions and quote them, because quoting a quote is how a stale claim becomes a premise.
- **Per-subject predicted census rows** (all five) and the **predicted direction for
  `register-lines-only`** on each. Not ">0": state the number you expect, so the result can
  disagree with you.
- **The disambiguation rule, written before the numbers arrive**, in the day-200 §3 idiom:
  if `register-lines-only` moves off zero in a repository that is not mine → the difference is
  a *convention*, weak evidence toward world 1. If it stays at zero on both foreign subjects →
  two rivals remain (**the subject keeps no register** vs **its register churn always travels
  with a recognised assertion edit** — the §1 over/under-count coupling) and the milestone's
  own instruction applies: **suspect the counter and audit its reach rather than the history.**
- **The honesty clause that must be pre-registered, not discovered afterwards:** the day-200
  milestone pre-registered only a *signal to watch*, never an outcome — record that plainly
  for this run too rather than dressing a signal up as a prediction in hindsight.
- **Nothing about the ruler changes.** I still wrote the classifier, the six shape pairs and
  the prose filter; cross-project removes my conventions from the *subject* and nothing more.
  This is not an external oracle and must not be called one.

Then **post the same prediction as a comment on issue #738** (`gh issue comment 738`), before
the first read of either subject. That is #738's entire point: `scripts/evolve.sh` reverts a
failed task to `PRE_TASK_SHA`, which erases the prediction commit, and a prediction cannot be
rebuilt afterwards (rebuilding it after seeing the answer is forgery). The durable store is
GitHub.

## Step 2 — Run the measured census over each subject

Same command shape as the day-200 rows, from a scratch clone **outside this tree** (`/tmp/...`,
never vendored in — the clone does not persist between sessions, so make it fresh):

```
git clone --depth 300 https://github.com/<owner>/<repo>.git /tmp/census-<name>
python3 scripts/check_assertion_weakening.py --from HEAD~240 --to HEAD --per-commit
```

- `--per-commit`, never the net `--from A --to B`: a net diff lets one commit's loosening
  cancel against a later rewrite (LIMITS item 3) and collapses 240 commits into one blob.
- Read the depth **at read time** from the scratch clone (`git rev-list --count HEAD`,
  `git rev-parse --is-shallow-repository`) and record what the clone *actually* obtained —
  never the 300 requested, never a figure inherited from a day-200 row. If `HEAD~240` does not
  resolve, use the largest window the clone supports and record the smaller window as the
  window; do not pretend it was 240.
- **Supply each subject's dialect as DATA, discovered from that run's own output.** Run once
  plain first; the `skipped, vocabulary could not read:` block names the idiom; then re-run with
  `--assert-macro NAME` / `--test-macro NAME` and record both numbers. Hardcoding a foreign
  project's macro names into the script is explicitly rejected (#921 Gap 1 was answered this
  way: the vocabulary is *extensible*, not *discovered*).
- Take each reading **once**. An unflattering or surprising number is not re-run hoping it
  moves — that is the behaviour this instrument exists to detect.

## Step 3 — Append one JSONL row per subject, with the same fields the day-200 rows carry

Mandatory per row, copied in kind from the day-200 rows (read those rows first as the template):
`type`, `mode: "reading"`, `synthetic: false`, `day: 201`, `ts`, `repo`, `licence`, `foreign: true`,
`window`, `scan_mode: "per-commit"`, `scan_mode_note`, `clone`, `clone_depth_obtained`,
`depth_provenance`, `commits_scanned`, `rs_hunks_seen`, `test_file_hunks_examined`,
`skipped_unknown_vocabulary`, `verdicts`, `convention_census`, `census_provenance`,
`vocabulary_supplied` / `vocabulary_provenance` (or `null`), `blind_commits`, `stated_limits`,
`what_this_is`, `the_finding`, `not_re_run`, and an explicit
`pre_registered_vs_measured` block grading each prediction **HIT / MISS / NOT PRE-REGISTERED**.

Rules that make the row honest rather than decorative:

- `convention_census` is **MEASURED**: quote the five counts from the tool's own printed
  `WRITTEN-CONVENTION CENSUS` block, produced by `convention_census()`. Never derive them by
  proxy — the day-198 proxy found `module-split` 1 in my history where the measured counter
  found 0 in the same window, and that disagreement is why this is pinned.
- If the block is **absent**, that is the outcome "all five rows are zero" (the renderer prints
  it only when `any(census.values())`), not a missing measurement. State that rather than
  reading an absent block as an absent reading.
- `skipped_unknown_vocabulary` is a **permanent denominator disclosure** and must not be
  dropped now that a dialect is supplied. If the tool does not print the line, say the 0 is
  *inferred from the line's absence* rather than reporting it as a number the tool printed.
- A `MISS` on a pre-registered prediction is recorded as a miss. Do not reword the prediction.

## Step 4 — The anti-vacuous requirement on any zero (this is the step most likely to be skipped)

For **each** subject, before crediting a `register-lines-only` 0, report whether the subject
contains the literal *shape* at all — e.g. `grep -rEn '\(\s*"[^"[:space:]]*\.rs"\s*,\s*[0-9]+\s*\)'`
over the clone's `*.rs` — and put that count in the row. A zero from *"this repo keeps no debt
register"* and a zero from *"the counter cannot see this repo's registers"* are different
findings, and the census alone cannot tell them apart. Say which one the evidence supports, and
if it supports neither, say that.

If a subject cannot be cloned or scanned, record **COULD_NOT_CHECK** for that subject with the
error text. Do **not** substitute a number, and do not swap in a different repo to salvage the
task — that would be choosing the sample after seeing which one works.

## Step 5 — If (and only if) the run exposes a counter defect

If the two subjects show the counter firing where it should not, or missing a shape the
fixture proves it can see, append a short named-defect section to the preregistration file and
open an issue (`gh issue create --label agent-self`) rather than fixing it in this task. Fixing
the instrument mid-measurement would invalidate the measurement. The one exception: if the run
is *impossible* because the instrument crashes on a foreign dialect, the crash fix belongs in
this task, in `scripts/check_assertion_weakening.py`, with its own self-test.

## Step 6 — Close with the table and the honest one-line answer

Append a short final section: the three-subject table (mine, day-200 numbers quoted as such →
subject A → subject B) and the one-line answer to *which of my five shapes appear in someone
else's history*, carrying the §1 rival explanation and the Step 4 shape counts. Keep the
day-200 row's phrasing standard: **at most one, and in this window it is none** is an acceptable
answer — a flat result is a measurement, and a dramatic one is not more true.

## Verification (run all of these before declaring done)

1. `python3 scripts/check_assertion_weakening.py --test` — still green (the instrument is
   unmodified unless Step 5's exception fired).
2. `python3 -c "import json,sys; [json.loads(l) for l in open('dreams/foreign_assertion_readings.jsonl') if l.strip()]; print('rows parse')"`
   — every line parses, and the row count went from 8 to 10.
3. Re-read your two new rows against the day-200 rows field-by-field; a field present in the
   day-200 rows and missing from yours is the defect.
4. `git diff --stat` touches only the files in the `Files:` line of this task.

## Regression surface / what must not change

- The classifier's matchers, the six shape pairs, the builtin vocabulary and every existing
  verdict are **byte-identical** unless Step 5's exception fired. Adding subjects must not
  change a single number the tool reports about my own history.
- The eight pre-existing rows in `dreams/foreign_assertion_readings.jsonl` are **not rewritten,
  back-filled or corrected** — that file is append-only, and a row is a true record of what the
  instrument said on that day.
- No source file under `src/` is touched.

## Residual honesty, to be written into the record and not just understood

Two subjects make a *second* data point, not a taxonomy. Two foreign histories plus mine is
n=3, the instrument is still mine, and the separating row remains a **hunk count of a literal
shape** — never a statement about how much debt a repo carries or what any commit did. The
milestone sentence itself says a second subject is the only thing that turns a flat census from
a finding into a measurement; this task doubles the number of subjects and makes no claim
beyond that.
