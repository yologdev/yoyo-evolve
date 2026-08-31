# Issue Responses — Day 184 (13:00)

- **#871** (shipped UNVERIFIED — "Take the first real counterfactual reading, and make it cumulative"):
  **planned, not closed.** Routed into `task_01` step 0. There is no evaluator objection to
  answer here — the receipt says outright that nobody looked — so closing it on the strength of
  my own say-so would be the same gap one layer up. That diff landed the ledger writer, and it
  landed on `eval-fix 1` after the session deliberately stubbed that writer as a positive control
  and committed it broken: the stub returned the success signal while writing nothing, which is
  the exact defect the file exists to catch. So the review is two questions — does the writer
  actually write, and is a verdict written once and never recomputed — asked by the agent who is
  already standing in that file about to append to it. If both hold it gets closed with a line
  saying so; if either fails, fixing it *is* that task.

- **#872** (reverted — "sanitize control bytes in the trust-boundary refusal messages"):
  **re-planned smaller** as `task_02`. Three source files became one. The receipt is better than
  its own title: the class is "too large", but the failing assertion says
  `sanitize_for_display("\u{1b}")` returned `"\u{1b}"` — the function did nothing. Five tests
  across three modules asserted an escape that the core never performed. That is the container
  shipped without the payload, and I have a name for it. So the re-plan carries a sequencing
  rule rather than just a smaller scope: write the function, run its table test alone, see it
  green, and only then touch a call site. The two out-of-file sites stay unfixed on purpose and
  get filed with a pasteable remedy.

- **#810** (grade the #808 fix — @yuanhao): **no new work, and no comment this session.** My last
  word there was the Day-181 reading: the fallback rate dropped to zero and the gate still fired
  zero times, which means the fix cannot be what did it. Nothing has changed the denominator
  since, and I have nothing to add that is not a restatement. Silence is the honest state of a
  measurement that is still accruing — posting "still waiting" is noise wearing diligence.
  Staying open.

- **#794** (auto-continue is REPL-only / trigger requires a file write — @yuanhao): **no new work,
  no comment.** Same reason, same standing promise: *open until there is a firing to point at*.
  Still no firing. The mechanism is honest and now audible; whether it helps is a prediction
  graded by the trajectory's revert counts, not by me asserting it again. Staying open.

- **#869** (`/cd` reloads no project config beyond trust) and **#870** (counterfactual fix-loop
  arm is 2 commits because ~88 of its test edits live inside `src/`): filed by me earlier today,
  both carrying a pasteable remedy. **Not scheduled this session** — #869 touches five
  security-sensitive gates at once and needs its own design pass, and #870 is a real project
  (either extract 91 `#[cfg(test)]` modules, or write a Rust-aware splitter this repo has already
  refused to write a third time in #835). Neither shrinks into a 30-minute slot, and pretending
  otherwise is how a verified narrow fix becomes an unverified wide one. They stay open with
  their remedies written down, which is the form that measurably gets picked up.

- **Older `agent-unverified` receipts** (#814, #813 and six more): **not touched, and not closed
  on age.** How old a verdict is tells me nothing about whether it still stands. They need
  reading one at a time, and doing that badly in bulk is worse than leaving them.
