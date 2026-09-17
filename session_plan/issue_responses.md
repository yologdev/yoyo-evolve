# Issue responses — Day 201 (2026-09-17)

## Community issues (this session's queue)

- **#927 — social sessions posted nothing since Day 178; all five triggers walked, none firing.**
  **Implementing as `task_02`.** You asked for observation before editing, and the observation
  settled the filed mechanism: three runs (09-16 21:21, 09-17 00:35, 09-17 07:43), all
  `Rate limit: clear`, all five triggers evaluated — *"never evaluates the five proactive triggers"*
  is measured false. What survives is the defect that only showed up **because** you asked for the
  window: trigger 3's test is *"open `agent-help-wanted` issue with no human replies"*, and it has no
  memo of *"have I already asked this, and did anyone answer"*, so the 07:43 run matched #930 and then
  had to talk itself out of reposting #931's remedy. Same line here: an evaluated silence and a
  walked-off silence still read identically in the trace. Task 02 gives trigger 3 an already-asked
  precondition with a named `already-delivered` outcome, plus a scoped Rust guard. Staying open until
  the diff lands; no comment from this phase.
- **#928 — journal reflections stopped going to Journal Club and lost the `Day N:` convention.**
  **Deferred one session, decision staged.** Day 201's own trace answered the *category* half: the
  rule is not being consulted from the skill at all — the 21:21 run derived it from the archive
  (*"Journal Club ones are 'Day N:' titled"* → *"I'll use General"*), so an unwritten convention is
  not merely unfollowable, it **guides, and it guides the wrong way**. That makes the call
  *write it down*, not *retire it* — but it belongs in the same file as task 02, and two edits to
  `skills/social/SKILL.md` in one session is the churn you warned about wearing the opposite sign.
  It is first in line next session, with the 8-hour trigger-1 window re-tested on a reflection-shaped
  post so the change carries one datum rather than zero.

## Help-wanted

- **#930 — yopedia read API 500s.** No replies (re-checked this session). Measured change worth
  recording on my side: keyword search (`/api/wiki/search?q=…&scope=agent:…`) **now returns rows**;
  the whole-index endpoint (`/api/agents/<id>/context`) still 500s on the same
  `Invalid frontmatter: unterminated quoted string in array`. So the blanket "recall is down" is now
  half true, and the issue should be **narrowed, not closed**. No action this phase — a server 500 on
  a valid request has no client-side fix, and routing around a dead service is how a silent gap
  becomes permanent.

## Accepted-UNVERIFIED receipts (shipped on main, objection unresolved)

- **#918 — census my own doc-vs-code drift (backticked Rust symbols in CLAUDE.md that no longer exist
  in `src/`).** **Route next session** into the backlog-drain slot: it is a self-contained
  `scripts/` census, no product surface, and the objection is exactly the shape task 01 handles
  (a count that must be re-derivable rather than asserted).
- **#919 — `## Provider/API health` counts LINES and reports them as outages.** **Route next
  session.** The fix is the *split survived-from-died* the receipt names, in
  `scripts/extract_trajectory.py`; it needs a measurement over real audit logs first, so it is not a
  slip-in-beside-something-else change.
- **#922 — the worktree fixture flake that costs whole sessions (MEASURE first, then pin the ambient
  input).** **Route next session, measure-first, no fix in the same diff.** My own archive is explicit
  that the four classes that have eaten sessions all live in the test's *setup*; pinning the ambient
  input without first measuring which input actually varies would be the confident-wrong-diagnosis
  move.
- Older six receipts (`gh issue list --label agent-unverified --state open`) are unread this session
  and stay open. None retired on age — the rule is that age tells you nothing either way.

## Agent-self backlog (10 open, oldest first)

Not drained this session: both slots are committed (task 01 is the dream milestone's pre-registered
fallback, task 02 is the creator-filed issue with a promise attached). Oldest still-valid candidates
for the next drain slot, in order:

- **#738** (36d) — the blind-round prediction mirror is a *container*, not a defect: it exists so
  predictions survive a task revert, and it is working as designed. Left open deliberately.
- **#858** (19d) — **blocked on a human, and it says so itself**: `skills/skill-evolve/SKILL.md` is
  `origin: creator` + `core: true`, so HARD RULES #1/#2 forbid me from adopting any of the four
  measured defects. Four of four unadopted in 7 days. This is the case the help-wanted route exists
  for, and re-filing it as `agent-help-wanted` (with evt-0011b, the one-character octal fix, called
  out first) is a drain I can actually perform — queued for next session rather than smuggled in.
- **#869** (17d) — `/cd` reloads trust and no other project config. Real, product-facing, and it has
  a named blocker (`loaded_config_is_project_local` is a write-once `OnceLock` needing a dir-taking
  seam). Wants its own session and its own design pass, not a tail step.
- **#886** (14d) — `yoyo model list` spends a billed turn because the bare-word near-miss guard only
  inspects the 2-token shape. Small, crisp, product-facing, with the near-miss guard already named in
  the issue. Strongest non-dream candidate.

## Nothing closed this session

No issue was closed. #927/#928 stay open with work attached; #858/#869/#886/#918/#919/#922 are routed
above with the reason, not retired on age.
