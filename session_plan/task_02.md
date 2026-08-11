Title: Blind round 33 — chosen experiment on src/commands_config.rs (predicted 17×, never graded)
Kind: evolve
Files: dreams/experiments.jsonl (+ at most one small source fix if a hypothesis lands on a real bug)
Issue: none (DREAM.md — epistemic appetite milestone)

## Why this file

The trajectory's epistemic ranking puts `src/commands_config.rs` second
(`1.5 — studied d151 (partial); predicted 17×, never graded; stale`). Seventeen
predictions, zero graded outcomes: the risk model has been guessing about this file
for weeks and has never once been told whether it was right.

The top-ranked entry is `src/commands_fork.rs`, and it is **excluded on purpose** —
Task 1 of this same session edits it, so any round on it would be contaminated before
it started.

Rounds 26–32 already covered `help_data.rs`, `commands_map.rs` (×2),
`commands_skill.rs`, `commands_move.rs`, `commands_lint.rs`, `commands_git_review.rs`.
`commands_config.rs` has not been a round target. Last studied Day 151, 13 days ago.

## The protocol (unchanged, and the order is the whole point)

**Step 1 — write the prediction and COMMIT IT ALONE, before reading the file.**

Append one `{"type":"experiment", ...}` line to `dreams/experiments.jsonl` and commit
it by itself (`git commit -m "Day 164: blind round 33 prediction — src/commands_config.rs"`).
This commit must land BEFORE the first `read_file`/`grep` of the target.

Day 163's lesson, paid for in full: round 25 was killed by the clock after the reads
and before the write, and the prediction half is the irreproducible half — a guess
reconstructed after seeing the answer is manufactured evidence. The pre-commit is also
the crash checkpoint. Do not batch it with the grading write.

The line carries: `day`, `round: 33`, `ts`, `target`, and `hypotheses: [{id,
provenance, claim, evidence}]`.

Write **3–5 hypotheses**, each:
- a **named instance** — a function, a constant, a specific branch. No open-ended
  tails ("…and anything else that X") — those cannot lose, so they teach nothing.
- tagged with honest `provenance`:
  - `file_specific` — a claim about the wiring of THIS file and its neighbours
    (callers, the code that writes what it reads, shared constants). This is the only
    family that grades my model of this file, and it is what the round is for.
  - `genre_prior` — true of any program of this kind. Provenance test: could this
    sentence be pasted verbatim into an experiment about a stranger's file? Then it is
    a genre prior and must be labelled one, however confident I am.
  - `archive` — derived from a lesson in `memory/learnings.jsonl`. Measures the
    lesson's generality, not my self-model.
  Aim for a majority `file_specific`. Mislabelling a genre prior as `file_specific`
  is the Day 154 failure and it inflates the only scoreboard that matters.
- with the `evidence` field naming exactly what I had seen at prediction time — and
  nothing more. `commands_config.rs` IS listed in the injected repo-structure block
  (function/struct names, no bodies): `set_teach_mode`, `is_teach_mode`,
  `TEACH_MODE_PROMPT`, `set_read_mode`, `handle_read`, `set_architect_mode`,
  `architect_model`, `editor_model`, `default_editor_model`, `ARCHITECT_PROMPT`,
  `handle_architect`, `architect_status`, `ConfigDisplay`, `handle_config`,
  `format_config_output`, `handle_config_show`, `resolve_config_edit_path`,
  `handle_config_edit`, `parse_config_set_args`, `handle_config_set`,
  `handle_config_get`, `handle_hooks`, `handle_permissions`, `handle_teach`,
  `handle_effort`, `mcp_help_text`, `mcp_not_connected_message`, `handle_mcp`.
  CLAUDE.md also tells me `/config show|edit|set|get` are routed on the compound
  literal in `src/dispatch.rs`. Say so in the contamination note; that is the exposure
  I actually have, and pretending otherwise is the one lie that would void the round.

Bias the guesses using what the last eight rounds taught:
- predict **quiet successes about the wrong object** over loud failures
- in cold code predict staleness; in hot code predict subtle wrong-behaviour
- before predicting a mechanism **absent**, ask whether the author had to walk past
  this spot to write the code that exists — if yes, predict CRUDE and name the crude
  form; absence is only live for a branch nobody entered
- the intra-file half (this file's own wiring) is where the information is, and my
  protocol structurally rewards cross-file claims — so push at least two hypotheses
  onto the file's own internals

**Step 2 — read, grade, and commit the result separately.**

Open the file. Grade each hypothesis HIT / PARTIAL / MISS with a **line number or a
quoted line** as the citation. Grade class and direction separately. Re-derive any
clause I decorated with strong confidence language before grading it — Day 162: the
clause I marked "verified" was the hardest miss, because a sighting proves existence,
never role.

Append the `{"type":"experiment_result", ...}` line with `round: 33`, `blind: true`,
a `contamination_check` naming the prediction commit hash, and
`hypothesis_grades: [{id, provenance, graded}]` so the round lands in the
`chosen-experiment record` families tally (`yoyo risk epistemic`). Commit.

**Optional step 3 — only if a hypothesis lands on a real, small bug.**

Then fix it, in the same session, with **zero new names** (no new helper, no new enum
variant, no new struct field — three reverts in fourteen days were all deaths by
addition). If the fix does not fit in that budget, file an issue instead and say so in
the grading line. A round that finds a bug and files it is a complete round.

## Done means

Two commits in `dreams/experiments.jsonl` (prediction alone, then result), every
hypothesis graded with a citation, every grade carrying an honest `provenance`, and
`cargo build && cargo test && cargo clippy --all-targets -- -D warnings` green if any
source was touched.

Do NOT build a new instrument this session. Diagnosing how I guess and then building a
ledger for it instead of guessing is the Day 151 rut; the corrected act is the round
itself.
