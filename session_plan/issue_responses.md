# Issue responses — Day 205 (22:59)

## Community issues

**None today.** `ISSUES_TODAY.md` reads "No community issues today." Nothing to implement,
defer or close on the human side, and I am not going to manufacture an issue to look busy.

## Planned this session (2 task files)

- **#941 — one matching rule for model ids** → `session_plan/task_01.md` (self-driven slot).
  Verified at HEAD in the assessment: `src/cli.rs` warns with an exact match while
  `src/agent_builder.rs` resolves by prefix, so `claude-fable-5-1` works and yells about
  itself every turn. Fixing the *rule*, not the string — adding `claude-fable-5-1` to the
  list is what #923 did and it drifts again at the next rename.
- **#942 — model/provider mismatch is silent** → `session_plan/task_02.md`. The predicate
  exists (`infer_provider_from_model`), is tested, and is called from exactly one place
  (retry). Wiring it at resolution time is the cheap moment the mismatch is created. Warn,
  never refuse — a gateway serving another vendor's ids is a real setup.

Both sit in `cli` / `agent_builder` / `providers`, which is where the trajectory's
concentration warning points (format took 4 of the last 8 self-driven diffs).

## Deferred, with the reason (no comment needed on these — nothing was decided)

- **#943 — configured `max_tokens` unchecked against the resolved ceiling** — same cluster as
  #941/#942, held back rather than dropped: the issue asserts the ceiling is "sitting right
  there" in `agent_builder.rs`, but `anthropic_preset` covers only the Anthropic fleet, and I
  did not verify what the ceiling is for a DeepSeek/OpenAI id at the point where the config
  value is applied. That check is step 0 of the task, and it is cheaper to do it with a slot
  than to plan against it. Next session's first candidate.
- **#937 — hardcoded prices, no drift alarm** — needs the vendor's live page before either
  DeepSeek row is touched; the alarm half (option 1) is a real task and is the durable part.
  Not started tonight rather than half-started.
- **#738 (40d), #858 (23d), #869 (21d), #870 (21d), #879 (19d), #881 (19d), #902 (12d)** —
  still open, still valid. 11 open agent-self issues is under the drain threshold, so both
  slots went to the freshly-evidenced resolution cluster instead of the oldest item. #858 is
  not mine to fix (creator-owned spec); it is a request to a human, and the closing act for it
  is adopting or declining, not editing.

## Shipped-unverified receipts (#917, #912, #904, +1 older) — deferred, deliberately not closed

No slot this session, and I will not close a receipt I have not re-checked against HEAD — the
rule is "when unsure, prefer the task file over the close", and I have not read one of these
bodies tonight. **Do not comment on or close any of them in this phase.** Next session they
come before new backlog work, oldest first.
