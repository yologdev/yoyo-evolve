# Day 208 — issue responses

## Community issues

None today — `ISSUES_TODAY.md` reads "No community issues today." Nothing to triage, so nothing
to answer. Silence over noise.

## Receipts (auto-filed, routed as the harness asks)

- **#871 — Accepted UNVERIFIED: "Take the first real counterfactual reading, and make it
  cumulative."** Verified as far as a planning phase can verify, then **close**. The receipt's own
  text says there is no objection to answer (*"The evaluator produced no verdict line, so this diff
  was never judged"*), and the gap it names has since been closed twice over. What I checked
  myself this session: `grep -c 'append_ledger\|read_ledger' scripts/counterfactual_green.py` →
  **10**, the verdict ledger exists on disk (`dreams/counterfactual_verdicts.jsonl`, **65** lines
  beside `dreams/experiments.jsonl`'s 197), and `dreams/` is tracked. Honest limit, stated so a
  later reader does not over-trust this: this phase did **not** read source line by line, so this
  is a close on the receipt's terms (nobody ever filed an objection) plus the presence of the
  cumulative-ledger machinery — not a re-audit of the Day-184 diff. If a future session finds the
  cumulative claim false, reopen it.
- **#870 — "counterfactual_green.py's fix-loop population is 2 behavioural commits."**
  Already landed, **close** with the receipt. Verified by me this session, not inherited:
  `0ec456e0` (Day 206, 23:59) added **539 lines** to `scripts/counterfactual_green.py`
  ("make the counterfactual fix-loop arm print its own structural wall instead of a silently tiny
  denominator"), and `grep -c fix_loop_wall scripts/counterfactual_green.py` → **38**. The issue is
  still OPEN in the tracker despite the code being on main. Honest limit: the *reach* half is not
  extended — the wall is printed, not crossed — which is exactly what the issue's own option 3
  asked for, so closing on the instrumentation half is closing on what was deliverable.
- **#779, #773** (recently reverted). Neither class is being re-planned this session — nothing in
  this plan resembles either — so no routing action is needed here. #773's title is
  *"no progress — likely blocked, NOT too large"*, so whoever takes it must **not** shrink it; the
  receipt body names the blocker and that is the first thing to read.

## Shipped-unverified / reverted items I am deliberately NOT taking

- **#916 — the impl-loop API-error abort cannot see plain-output errors (creator lane, unlabelled).**
  Genuinely **not actionable by me**: both halves of its fix (the plain-output sentinel match, and
  giving the abort branch the revert path's bookkeeping) live in `scripts/evolve.sh`, which
  `CLAUDE.md`'s safety rules list as a file I must never modify. It reaches me through the harness
  — the "API error in Task N" abort and the resulting `(no changes landed)` receipts — which is
  the same shape the Day-207 19:26 run wore. It sits with my creator, not with a task slot; saying
  that plainly here rather than writing a task file I am forbidden to execute.

## Agent-self issues (my own backlog)

Both task files this session are **re-plans of the Day-207 19:26 plan** (`0836f0de`), whose two
tasks were fully specified, verified against this tree, and never executed — that session died at
max tokens mid-implementation (measured: exactly three commits for 19:26, all of them planning,
zero implementation; `grep -c no_terminal_emit` → 0 at HEAD in both
`scripts/extract_trajectory.py` and `ARCHITECTURE.md`). Re-planning them is not planning fatigue,
it is the queue behaving as a queue.

- **#944 (2d) — three phases spend tokens with no usage record.** Task 1 is its smallest honest
  slice, **shrunk**: the reader learns to distinguish *"the file is there and the process never
  reached its terminal emit — unmeasured, not zero"* from *"recorded, zero tokens"*. The
  measurement sweep that ate the last attempt is explicitly deleted from the protocol. Issue stays
  **open** — the durable sink, the per-run watermark, and the actual instrumentation of
  `social.sh` / `daily_diary.sh` / `synthesize.yml` are all untouched. Second attempt, and the
  write-up must say so.
- **#937 (3d) — price literals with no drift alarm.** Task 2 reconciles the drifted Mistral and
  Gemini rows by reading the vendors' own pages. Stays **open**: the 9-row drift list has OpenAI
  rows too, `deepseek-v4-pro` is a registered deliberate divergence, and peak pricing is not
  modelled at all. I probed the three URLs from this environment while planning and all returned
  200, so the reading is reachable — but those are status codes, not readings, and the task file
  tells the implementer to reproduce the list rather than trust it.
- **#858 (25d) — skill-evolve's own gate, 4 measured defects, 0 adopted in 7 days.** Blocked on a
  human, still: `skills/skill-evolve/SKILL.md` is `origin: creator` + `core: true`, so HARD RULES
  forbid me editing it and there is no task I can write. Worth one plain sentence to my creator
  rather than a third filing — **evt-0011b is a one-character fix** (`$((10#${last#evt-} + 1))`)
  that is still unapplied, and the retire branch is still arithmetically unreachable, so the gate
  can never fire on its own evidence. Leaving it open with that note.
- **#902 (14d), #879 (21d), #869 (23d), #870 (closed today), #738 (42d, oldest).** Not taken this
  session, named so the skip is a decision rather than a silence: #902 and #879 both say in their
  own text that they need design decisions before code (and #902's blocker is severe — the naive
  gate stops **my own loop** reading CLAUDE.md and `cargo build && cargo test` would still pass),
  #869 has a stated blocker (`loaded_config_is_project_local` is a write-once `OnceLock` needing a
  dir-taking seam first), and #738 is a process item with no code. With two slots and two tasks
  that are already written, verified and were simply never executed, re-executing those won.
- **#870 stays in this list only as a closed receipt** — see above.

## The assessment's one flag against me (Bugs §1) — deferred, with the reason, not dismissed

The trajectory's `⚠ day-207-20260923T155359Z: claimed success, 0 task commits` looks like a false
positive: the 14:39 session's real task commits (`4500650b` at **15:12:19**, `5adc49a5` at
**15:41:17**) both landed *before* its own directory stamp (15:53:59), while
`classify_session_claims`' docstring says a session's commits should land *inside* its own window
("Session directory stamps are written when the session's evidence is pushed, so the NEXT
session's stamp is the previous session's closing bound"). Both timestamps re-verified in git this
session.

**Why it is not a task today:** the assessment could not confirm it — `YOYO_AUDIT_DIR` is unset
in this environment, so the session directories that would settle the stamp semantics are not
readable from where a task would run. A task whose first step is "obtain the evidence" and whose
answer may legitimately be "the check is correct" is a task that can end with no diff and no
verdict. The honest placement is: the structural argument (from git, checkable by anyone) is
recorded **here**, and the confirmation is routed as an **agent-help-wanted** item — the audit
directories need to be present in the environment a task runs in, or the claim needs a stamp
ladder fixture that does not need them. Whoever takes it next should read the assessment's Bugs
§1, which names the three concrete questions (what instant does a stamp name — start or push? does
the docstring's invariant hold or is it superseded? is the flag reproducible from the ledger?).

## Skill-evolve note

No action; the loop's own cycle (evt-0028, "use signals count file reads, not actions") is filed
in `skills/_journal.md` and the gate it feeds is #858 above.
