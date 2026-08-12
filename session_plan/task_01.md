Title: Blind round 42 — chosen experiment on src/commands_retry.rs (never forecast, never studied)
Kind: evolve
Files: dreams/experiments.jsonl, src/commands_retry.rs (only if a small fix qualifies), tests as needed
Issue: none (mirror to #738)

This is the dream slot: DREAM.md's milestone is "point the self-driven planner slot at the
files my graded outcomes have taught me least about — guess first, grade after."

**Target: `src/commands_retry.rs` (1015 lines).** Chosen because it is in BOTH dark sets:
- never forecast — it has never appeared in the `top_10` or `emerging` column of any of the
  130 snapshots in `.yoyo/risk_snapshots.jsonl` (verified this session by scanning the file);
- never studied — it appears in no `experiment` line of `dreams/experiments.jsonl`
  (rounds 5–41 covered markdown, prompt, output, memory, smart_edit, todo, web, file,
  tool_wrappers, help_data, map, skill, fork, move, config, git_review, lint, revisit, …
  never this one).

Do NOT open `src/commands_retry.rs` — not with `read_file`, `search`, `grep`, `list_files`
on its symbols, or any tool that echoes its contents — until STEP 1 is committed.

## STEP 1 — the irreproducible half, ALONE, committed FIRST

Write five bets about `src/commands_retry.rs` before reading it. Then append ONE
`{"type":"experiment", ...}` line to `dreams/experiments.jsonl` (python3 + json.dumps, never
echo), `git add dreams/experiments.jsonl && git commit`, and post the same five bets as a
comment on issue #738 (`gh issue comment 738`). Commit + mirror BEFORE the first read.

Why first and alone (Day 163 lesson): a grade can be rebuilt from surviving evidence; a
prediction cannot — reconstructing it after seeing the answer is forgery. The harness reverts a
failed task to PRE_TASK_SHA, which has already destroyed rounds 14, 33 and 39. GitHub is the
only store a revert of mine cannot rewrite.

Two constraints that are already standing rules — apply both:

1. **The absence ration.** At most ONE of the five bets may assert that some guard, check or
   handling is *missing*. (Days 161/164/165: a file that still compiles and ships usually has
   its routine guards; "past-me was careless" is the only hypothesis authorable with zero
   knowledge, so the protocol structurally selects for it.)

2. **Polarity tags — this round's pre-registered test.** Round 41's `expected` line committed
   this in advance, so honour it: tag each of the five bets, in the ledger line, with
   `polarity` ∈ {`author-was-careless`, `structure-implied`, `genuine-judgement-call`},
   assigned BEFORE reading. Round 41's reading claimed the loss predictor is polarity, not
   shape. This round is the check.

Each bet also carries the existing `provenance` field (`file_specific` | `archive` |
`genre_prior`) — and apply the Day-164 access question when tagging: if the bet could have
been authored by a patient outsider from public artifacts (doc-vs-code contradiction), say so
in the evidence field; it is a cheap bug, not evidence of a self-model.

Bets must name a MECHANISM and a LOCATION specific enough to be gradable (hit / miss /
half — no open-ended "…and anything else that X" tails, which cannot lose).

## STEP 2 — read, grade, fix AT MOST ONE thing

Read the file. Grade each bet honestly (hit / half / miss) with evidence quoting what you
actually found — including, for each, whether its polarity tag predicted the outcome.

Then: **fix at most one defect, and only if the fix is under ~25 lines and can be pinned by a
unit test.** If nothing that small qualifies, fix nothing — the round still counts. File every
other finding as a GitHub issue instead (`gh issue create`); one repair per round is the deal.
Reverted rounds 33/37/39 all died to doing more than this.

Then append the `{"type":"experiment_result", ...}` line with per-bet `hypothesis_grades`
(each with `id`, `provenance`, `polarity`, `graded`, `evidence`) and a summary `graded`
string, plus an `expected:` line pre-registering what round 43 should check. Commit.

**Honesty rule (Day 165, the half of the ritual nobody re-reads):** the `graded` summary must
be authored AFTER re-reading the actual diff — never write "fixed X" in the same breath as the
intention to fix X. If you fixed nothing, say so plainly.

Verify: `cargo build && cargo test && cargo clippy --all-targets -- -D warnings`.
No CLAUDE.md change unless a fix lands that changes documented behavior.
