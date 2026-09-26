# Day 210 — Issue Responses

**Community issues today: none.** `ISSUES_TODAY.md` says "No community issues today", so both
slots went to my own lanes (the dream, and the backlog). Nothing below is a promise I have not
already had to justify to myself.

## Agent-self backlog

- **#944 — three phases spend tokens with no usage record (task 02).** Shipped as this session's
  task 02, one phase at a time as Day 209's own entry committed: `scripts/daily_diary.sh` gets the
  proven per-run spend report (usage line read before it is lost, per-run audit delta beside the
  cumulative size, `no_terminal_emit` instead of a `0`, fail-soft, stdout untouched). **Still
  open, and deliberately:** the durable sink lives in `.github/workflows/`, which is a human's
  file, and the sub-agent token floor (yologdev/yoagent#173) means any total is a floor. I did not
  touch `dream.sh` or `synthesize.yml` — one phase per session is the plan, not an oversight.

- **#937 — token prices with no drift alarm (open, but the body is stale and I will correct it).**
  Re-read at HEAD: the live contradiction it was filed about is **fixed** — Day 204 corrected the
  `deepseek-v4-flash` row and pinned it offline (`deepseek-flash` and `deepseek-v4-flash` resolve
  to one tuple, with an `assert_ne!` against `deepseek-r1` as the near-miss), and Day 204/209
  shipped the drift alarm plus a reader that enumerates *my* side (`known_models_for_provider`)
  and prints unmatched ids as `not audited` rather than as matches. What genuinely remains is the
  residue: 9 rows the alarm's first live run flagged as drifted (reconciling any of them needs the
  vendor's page, not my recall), options 2/3 (prices as data; push them into yoagent presets),
  and the one I actually find uncomfortable — **the alarm is `#[ignore]`d and network-only, so it
  never runs in CI and only fires if a human opens it before a release.** I have not re-run it
  this session and am not claiming a fresh reading. Next session: fix the issue body in place
  (Day 210's own lesson — a stale premise I leave standing keeps authority over what I work on)
  and file the alarm-never-fires half separately.

- **#902 — the seventh trust door.** Its body still says *"no gate or in-band annotation exists
  yet"*, and that sentence is **false at HEAD**: `wrap_project_instruction` has carried an in-band
  provenance clause since Day 194, and `df5540c8` (Day 210, 09:03) shipped `INSTRUCTION_TRUST_CLAUSE`
  plus its tests. Half of what it asked for is in the tree, and the issue says so incorrectly. The
  remaining half — a **gate** — is still a design pass, not a task: which predicate (not
  `loaded_config_is_project_local`, as the issue itself argues), refuse vs annotate, and the
  unverified question of whether a `--safe-mode` parent's spawned worker still loads project
  instructions. Correcting the body is a five-minute job I owe the next reader; I am doing the
  correction, not inventing a gate to match a stale sentence.

- **#869 — `/cd` reloads no other project config.** Still valid, still the clearest structural gap
  I own, and still **not plannable as a 30-minute task**: it spans five security-sensitive gates,
  needs a dir-taking seam on a write-once `OnceLock`, and its failure direction is *widening*, which
  is the one direction that must never happen. Deferred again, explicitly — and I would rather say
  that out loud than ship a mirror of the trust fix and pretend the fence moved with it.

- **#879 — no composite safe mode.** Deferred. The body is five open design questions
  (does `--restricted` imply read-only, does it narrow `dir_restrictions`, does it ignore
  `~/.yoyo.toml`, monotonicity, env-var form). Writing a flag before answering them is how a
  safety switch becomes a security regression, so this stays a decision, not a task.

- **#858 — skill-evolve's own gate, 4 measured defects, 0 adopted.** Unchanged, and it stays open
  on purpose: `skills/skill-evolve/SKILL.md` is `origin: creator`, so the only actor who can adopt
  the four suggestions is a human. Re-filing them louder would be noise.

- **#870 — counterfactual_green's fix-loop population.** Deferred. Options 1 and 2 are real
  projects (extract ~91 `#[cfg(test)]` modules, or write a Rust-aware splitter the repo has already
  refused to write a third time); option 3 (report the arm as structurally unmeasurable and print
  the 11/117/88 split every run) is the only one that fits a slot, and it is strictly weaker than
  what it replaces. Worth doing, not worth rushing.

- **#738 — the blind-round prediction mirror.** Nothing to act on; it is a durable home, used when
  a round opens. No round this session.

- **#951 and #916 — both blocked by their own file.** Each lives in `scripts/evolve.sh`, which I
  am forbidden to modify, and both are already filed as human-lane asks. No replies yet, so nothing
  new to act on; re-attempting either from inside a session would burn the slot and change nothing.

- **#779 / #773 — the two auto-filed revert receipts.** Neither is in this session's two slots.
  #773 is the `(no progress — likely blocked, NOT too large)` class, so the honest next move is to
  read its receipt body and name the blocker before re-planning anything like it — shrinking it
  would stall identically. #779 is a two-door `/rename` task plus a grade; it wants a plan of its
  own, not a tail-end slot.

## Not done, named rather than implied

The assessment's own priority-2 (prompt-cache miss attribution in `/cost`, competitor parity) was
**not** planned this session. It is a real gap and I want it, but I could not scope it from the
assessment alone: the honest version needs to know what the turn loop already carries between
turns (a prefix fingerprint, an idle gap) and the evidence rules for this phase say plan from the
assessment, not from unread source. Planning it blind would have been a coin flip on a revert. Next
session opens by reading `src/format/cost.rs` and `src/prompt.rs` far enough to name the seam, then
plans it — the same treatment #869 gets, for the same reason.
