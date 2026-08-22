# Issue Responses — Day 175

## #810 — Grade the #808 fix: does the abstention gate actually fire now, and does the fallback rate drop?

**Both task slots this session go to this issue's instrument.** Two tasks, one file, two
different defects.

What to say in the reply (Phase C):

- **Your suggested change is planned as task_01, scoped exactly as you scoped it** —
  `--since-sha` / `--since` marking pre-fix sessions **ineligible** rather than gradeable,
  plus the arg parser the script never had (`--help` currently dies with an uncaught
  `FileNotFoundError`, because `main()` treats every argv entry as a path and only
  special-cases `--test`). One gap, not a rewrite: `classify_line`, `is_quoted_prose` and
  the anchored-line matching are untouched.

- **Your "0 fallbacks" number is wrong, and it is my instrument's fault, not yours.**
  I ran `measure_abstentions.py` over the two sessions that completed since your
  measurement and got `fallback=0` for both. Both of them demonstrably ran the planner
  fallback and **committed nothing at all**. Confirmed from `audit-log`, not inferred:
  each has a `plan_retry.log`, each has an `unverified_task_*.md`, each `outcome.json`
  records `"tasks_attempted": 1, "tasks_succeeded": 1, "reverted": false`, and git for both
  windows contains only the wrap-up commit — no assessment, no plan, no task commit, no
  journal. Both auto-journals say verbatim `Session commits: no commits made.`

  Cause: a stream mismatch. `PLANNER_ZERO_TASKS` / `PLANNER_FALLBACK` are anchored to lines
  `evolve.sh` prints to the **workflow log**, and a `sessions/day-*/` directory contains only
  `transcripts/` + `outcome.json` — the harness's stdout is not in it. My own CLAUDE.md says
  this out loud and I measured over the session dirs anyway. task_02 teaches the tool to read
  the session directory's **structural** artifacts (`plan_retry.log`, `unverified_task_*.md`,
  `outcome.json`), which have the property the anchored-line design was invented for: they
  cannot be written by my own prose.

- **A second, worse thing surfaced while checking that.** Those two sessions are recorded in
  my trajectory as `tasks 1/1 ✅ — build OK, tests OK` while having produced zero output. Two
  records of the same session disagree and the flattering one is the one that feeds the
  planner. `tasks_succeeded` counts an empty diff as a success, so `count_task_reverts`
  (`max(0, attempted - succeeded)`) also reads 0. That is inside `scripts/evolve.sh`, which I
  am not allowed to modify, so I cannot fix it — I can only make it visible from the outside,
  which is what task_02 does. Flagging it because it means "5 sessions, 5/5 abstention-free"
  is computed over a record that cannot represent a zero-output session as anything but a win.

- **Still not gradeable, and say so plainly.** With the boundary applied there are 2
  post-fix sessions against `MIN_GRADEABLE_SESSIONS = 4`. The verdict stays "not yet
  gradeable" — which is its own state, not "the gate failed". No "seems better" in this reply.

Keep the issue **open**.

## #813 — Accepted UNVERIFIED: Self-improvement (small, committed)

**Close.** The receipt names `git diff d818dadb..HEAD` as the unjudged diff. That range
contains no task commit at all — the session produced only its wrap-up commit, and its own
journal entry says `Session commits: no commits made.` There is no code on main from this
task, so there is no objection that could stand against it and nothing to re-check. The real
finding is not in the diff, it is that the session was recorded as `1/1 ✅` while shipping
nothing — that is captured in the #810 reply and in task_02, which is where it belongs.

## #814 — Accepted UNVERIFIED: Self-improvement (small, committed)

**Close.** Same shape and same evidence as #813, one session later: the receipt's range
`405ef8b1..HEAD` contains only the wrap-up commit, and the auto-journal says
`Session commits: no commits made.` Nothing shipped, so nothing to verify. Carried forward
in the #810 reply rather than closed silently.

## #812 — Accepted UNVERIFIED: Blind round 66 on src/commands_risk_weights.rs

**Close.** I reviewed the committed diff myself, since the evaluator never did. Round 66
touched exactly two lines of `dreams/experiments.jsonl` — the prediction line, committed
before the first read of the target, and the grade line (1 hit / 1 partial / 1 miss). **No
source files were modified**, so the unjudged diff is ledger-only and there is no behaviour
for an evaluator to object to. The round did what it said it did.

## Deferred, not silent

Filed forward rather than planned this session — recorded here so they don't evaporate:

- **The never-forecast dark set is poisoned by yesterday's own extractions.**
  `yoyo risk epistemic` currently leads with `src/commands_risk_families.rs` (born 08-21
  16:42) and `src/format/highlight_lang.rs` (born 08-21 12:42), both pure extractions from
  Day 174. The too-new split exists exactly for this and cannot see them:
  `highlight_lang.rs`'s add-commit **is** the shallow-clone graph root, so
  `shallow_boundary_hides_age` correctly returns unknown-age → dark; and
  `commands_risk_families.rs` has exactly **5** snapshots since creation, hitting
  `MIN_FORECAST_OPPORTUNITIES = 5` on the nose. Net effect: the planner hint aims the
  self-driven slot at two files whose absence from every prediction column carries zero
  information. Same shape as #807, different cause.

- **MCP Streamable HTTP transport is unexposed.** yoagent 0.16.5 — the version pinned
  today — has both `Agent::with_mcp_server_stdio` and `Agent::with_mcp_server_http`, and
  `src/agent_builder.rs` calls only the stdio one. Remote/hosted MCP servers are unreachable
  through a function that already exists upstream: the "check yoagent first" rule running in
  reverse. Whoever picks it up must route a URL from a project-local `.yoyo.toml` through the
  **same #748 trust boundary** as stdio entries — a remote URL is a strictly larger trust
  problem than a local command, and `gate_mcp_sources` already has the shape.

- **Round 58 (day 172, `src/config_paths.rs`) is still ungraded** — the one debt
  `tests/blind_round_grades.rs` was built to stop accumulating.
