Title: The retrospective unhittable note names the recorded zeros that git contradicts (DREAM milestone, second signal)
Kind: evolve
Files: src/commands_risk_unhittable.rs, src/commands_risk_unhittable_tests.rs, ARCHITECTURE.md
Issue: none (DREAM.md "next milestone": "a retrospective re-read of the 10 live events should turn up at least these 2 rows")

## Why
DREAM.md names two live events whose persisted `unhittable_surprises` reads 0 while the surprise file
did not exist at the snapshot hash: snapshot `bf8beaf6` (src/prompt/stream_external_servers.rs)
and `45fb1800` (src/cd_config_note.rs). The planner checked this session that both hashes RESOLVE
in this clone and that `git cat-file -e <hash>:<path>` FAILS for both. So git can prove both
recorded zeros are wrong. `yoyo risk accuracy` still prints them as
`0 unhittable, 1 unmeasurable, git: not recorded` (rows 2026-09-28 and 2026-09-29).
`yoyo risk` prints the retrospective note ("9 recorded rows state 0 unhittable … the git check
reads 9 of 141 rows"), but the note never names the rows where a RECORDED 0 is contradicted by
the git census. That contradiction is the one the dream is about.

## Steps (2)
0. **Measure first; the null result is a deliverable.** Before editing, run `yoyo risk` and the
   retrospective reader, and write down whether the current output already names those two rows
   (by ts) as "recorded 0, git ≥1". If it already does, the deliverable is only the ARCHITECTURE.md
   line quoting the exact output. Report that verbatim and do not invent a change.
1. **Otherwise, test first, then add one line.** Add a pure helper that returns the ts of every
   row whose persisted `unhittable_surprises == Some(0)` while the retrospective git census
   counts ≥1 born-after for that row. Build it from the data `retrospective_unhittable_at`
   already computes (the `GitCensus` per-row results); do not add a second git pass. Render ONE
   extra line in `retrospective_note`: `recorded 0 contradicted by git: N row(s): <ts>, …`,
   glyph-free under plain, and absent (byte-identical to today's output) when N == 0. Put the
   tests in commands_risk_unhittable_tests.rs, and drive them THROUGH the real reader:
   real ledger lines in, the helper's set out. Do not hand-type the set; see Day 210/211 on
   answer-key fixtures. Include a near-miss: a row that records 0 and whose git reading is also
   0 must NOT be named. Also include a row whose recorded value is absent (`None`, pre-Day-209):
   it must not be named, because "not recorded" is not "recorded 0". Positive control, as one
   atomic command: neuter the helper to return empty (mark it `// NEUTERED`), confirm the new
   test fails by name, restore, rerun green.
2. In ARCHITECTURE.md under commands_risk_unhittable.rs, record the live output of `yoyo risk`
   after the change. Quote the line, and state whether bf8beaf6/45fb1800's rows appear in it.

## Constraints (read before step 1)
- src/commands_risk_unhittable.rs is 2019 lines. Run `cargo test --test module_size` BEFORE
  editing to see its cap and register status. Keep the production addition at 30 lines or fewer.
  If the gate would go fatal, STOP after step 0 and report the null/size blocker in
  ARCHITECTURE.md rather than splitting modules in this task.
- Do not change any existing count, verdict or sentence in the note. This task makes a
  contradiction readable; it does not move a number.
- `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.
