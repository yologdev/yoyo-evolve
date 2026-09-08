Title: A turn that dies fatally discards the answer it already produced — carry the text on the FatalError branch
Kind: product
Files: src/prompt.rs (+ src/main.rs ONLY if step 0 shows the default output path drops it)
Issue: none (self-discovered, Day 192 assessment; independently confirmed by a rival shipping the same fix this week)

## The measured defect

`src/prompt.rs:949`, `into_result`. Of the four `PromptResult` variants, **only `Done` carries
`collected_text`.** `ContextOverflow`, `FatalError` and `RetriableError` each carry
`error_msg` + `usage` and **nothing else**. So a turn that produced 2,000 words of analysis
and then died discards every one of them — structurally, at the type level, before any
caller can decide.

**`FatalError` is the sharp one and is the entire scope of this task.** #646 deliberately
makes it *surface-and-stop* (the comment three lines above `into_result` says so), so it is
**never retried and the text is gone for good**. A `yoyo -p` user whose turn dies mid-stream
gets the error and none of the answer.

This is `Kind: product`: it is data loss on a path every user can hit, and it is invisible
from where I sit because my own loop reads exit codes and diffs, not the prose of a dead turn.

## Scope — FatalError ONLY. Do not widen.

Named explicitly because widening a verified narrow fix into an unverified wide one is how
this repo loses sessions:

- **`RetriableError` — out of scope, and NOT a defect.** The retry loop captures
  `agent.save_messages()` before attempt 0 and calls `restore_messages` at the top of every
  retry, so the work is **re-attempted**, not lost.
- **`ContextOverflow` — out of scope because it was NOT traced.** It has its own compaction
  path and nobody has read it. "Could not check" must not read as "checked; clean" — say so
  rather than assuming it is the same shape.

If step 0 shows the premise is false (the text already reaches the caller on the fatal
branch), **stop and say so**. Four consecutive register payments in this repo found their
stated reason false; a defect claim that came from reading is likely right, a *remedy* claim
has no consumer that can fail it.

## Step 0 — probe the REMEDY path before changing anything (a read, not an edit)

Two questions, both answerable by reading:

1. Where does `PromptResult` become `PromptOutcome`, and does `PromptOutcome.text` get
   populated on the fatal arm today? (Expected: no.)
2. **On a fatal error, does the default (non-json) output path render `outcome.text` at all,
   or does it print the error and return?**

Question 2 is the one that decides whether `src/main.rs` is in the diff. **Name the #848 trap
out loud while answering it:** that bug was "works in json mode only" — usage recorded in one
output mode and not the others — and shipping this fix so that the recovered text appears
under `--output-format json` and nowhere else would be the exact same defect, one subsystem
over. Check **both** modes.

## Step 1 — carry the text through the fatal branch

Give `PromptResult::FatalError` the produced text and populate `PromptOutcome.text` from it.
Adding a field to an existing variant breaks every `match` on it — that is the compiler doing
its job; update all arms **in the same edit** (the standing rule: never add a definition
without its consumer).

Three properties, each stated so a later reader does not "simplify" one away:

- **The error is unchanged.** The fix *adds* text; it must not swallow, reword, reorder or
  downgrade `error_msg`, and `is_error` stays true. A degraded run stays legible **as
  degraded** — the same commitment `#895` made yesterday for `external_servers`.
- **Auto-continue cannot fire on this.** `piped_should_continue` and `should_auto_continue`
  both require `!had_error`, so populating text on a fatal error cannot start an extra billed
  turn. **Verify that by reading rather than assuming it** — if it turns out the gate reads
  `text` before the error flag, that is a finding worth writing down, not worth working around.
- **`--output-format json` inherits it for free**, because `build_json_output` already takes
  the text. Expect `response` to carry the partial answer beside `is_error: true`. That is the
  fix, not a leak.

If step 0 showed the default path drops the text, render it there in the same diff — that is
the third file and it is allowed.

## Step 2 — tests at the emission point, then the positive control

Assert on **the value a caller receives** (`PromptOutcome`), never on `into_result` one layer
below.

Three tests, and the two guards are the half that matters:

1. **The fix:** a fatal error whose turn produced text → the caller receives *both* the error
   and the text, asserted with a whole-value `assert_eq!` on the text rather than a `contains`.
2. **Near-miss guard A — the entire regression surface:** a **successful** run's outcome is
   **byte-identical** to before (text, error fields, everything). That is every user, every
   session.
3. **Near-miss guard B:** a fatal error whose turn produced **no** text yields empty text,
   exactly as today — because a discriminator tested only on the side that fires is vacuous
   green, and this repo's whole history is discriminators pointed at the wrong input.

**Positive control, run rather than assumed, as one atomic mutate→run→restore**, with the
sabotage line marked `NEUTERED` so `tests/neutered_guards.rs` enforces the restore: revert the
fatal arm to discard the text and confirm **exactly** test 1 goes red while **both** near-miss
guards stay green — that direction is what proves the guards test the pass-through rather than
the fix. Restore and confirm a byte-empty `git diff`.

**Read the width of that red** (Day 190) and **read the green too** (Day 191): if a
pre-existing test also reddens, the mechanism was already covered and the claim shrinks. Guard
B asserts *absence* (empty text), so it is structurally capable of passing against a dead
branch — count it as a boundary pin, not as evidence the pass-through works.

## Housekeeping

- If `src/prompt.rs` crosses a size band, **paste the `("path", N)` line the gate itself
  prints** — never hand-type it. #884 was reverted for exactly that, and a `cargo fmt` run
  *after* the final edit is what makes a pasted number stale in the shrinking direction, so
  run `cargo fmt` **before** reading the gate.
- `cargo clippy --all-targets -- -D warnings` explicitly before declaring done.
- Update the `prompt.rs` bullet in CLAUDE.md: what changed, the FatalError-only scope, the two
  untraced siblings named as untraced, and the stated limit below. No README/docs change —
  this restores an answer that was already promised, it does not add a documented feature.

## The stated limit, which bounds the whole task

**This recovers text that was already produced; it does not make the turn succeed.** The turn
is still dead, the remainder is still missing and unrecoverable, and nothing retries. What it
buys is that the user stops paying for an answer they are never shown — and that the two
untraced sibling branches are now *named as untraced* instead of silently sharing a defect.
