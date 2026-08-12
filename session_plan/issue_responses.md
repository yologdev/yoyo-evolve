# Issue responses — Day 165 (21:06)

## #683 — Replace the GASP sidecar with yoagent's gasp feature (@yuanhao, agent-input)

**Defer — and not for lack of a slot.** The checklist's own next step says so: item (3) is
marked *"THE NEXT STEP, and it is operator-lane, not yoyo's"*, and it is bound to (7) by the
ordering constraint at the top of the issue — the env bridge and the sidecar retirement are one
change, and wiring the bridge alone destroys the sidecar's session record via the 600s lease
steal. Item (10) (make in-process recording observable) is explicitly *"land with (3)/(7)"*,
so it is not separable either.

Everything on that list that is mine is done and verified by him: the default-off `gasp`
feature (`cae59418`), the `--features gasp` CI (`9dcccf48`), the recording seam over all four
`prompt.rs` agent-start sites, and — this morning — moving `redact_secrets` into
`src/safety.rs` so the default test run actually compiles it, plus wiring it into
`prompt_budget::write_audit_entry`, which is the log I *do* publish.

No comment this session. I have nothing new to say, the last three comments on that thread are
pointer corrections rather than progress, and I'd rather post when I have a run to show than
add a fourth. Silence over noise.

## #740 — /revisit list cannot tell a corrupt .yoyo/revisit.json from an empty one

**Implementing** — task_02. Re-read the code this session before planning it rather than
trusting my own issue text: `load_revisit_list` really does `unwrap_or_default()` a parse
failure, and `add`/`remove` really do write a fresh one-entry list over the damaged file under
a green success message. That second half is data loss, so it's the part the fix is aimed at:
refuse to write, name the file, let the human decide.

## #741 — /revisit add stores a placeholder title and never checks the issue exists

**Defer, deliberately.** Same file as #740, and I want one bug per diff — my last three
reverts were all "did more than the one thing." It stays open with the evidence already in it.

## #738 — Blind-round prediction mirror (agent-self)

**In use, not a task.** Round 42's prediction goes there as a comment before the first read
(task_01, step 1). It exists because the harness has eaten rounds 14, 33 and 39 mid-flight;
GitHub is the one store my own revert can't rewrite.

## Reverted-task receipts (#739, #737, #731)

Not acting on these as written. #737 and #731 are blind rounds that died to over-scope, and
the correct response is the one baked into task_01: fix at most one thing, ≤25 lines, file the
rest. #739 (naming destroyed rounds in the ledger) stays parked — the mirror on #738 is the
cheaper version of the same guarantee, and it's already running.
