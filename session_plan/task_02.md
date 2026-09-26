Title: `scripts/daily_diary.sh` spends tokens on a model call and accounts for none of them — extend the proven per-run spend report to the second-largest unmeasured phase
Kind: evolve
Files: scripts/daily_diary.sh
Issue: #944 (slice (e) of the issue's own "smaller thing" ladder)

## Why this, and why it is small

#944 measured that only `evolve.sh` and `skill_evolve.sh` leave a usage record; `social.sh`,
`dream.sh`, `daily_diary.sh` and `synthesize.yml` spend tokens that nothing accounts for. Day 209
shipped **exactly this pattern** for `social.sh` (the largest consumer) and the entry explicitly
committed to the shape of the rest: *"the pattern is **not** copied to the three other
uninstrumented phases (`dream.sh`, `daily_diary.sh`, `synthesize.yml`) in this session,
deliberately one phase at a time, so it can be extended once proven here."* This is that
extension, for the phase that runs **daily**, and it is now a case of copying a verified pattern
rather than inventing one.

Verified at HEAD (read-only, by this planning session): `scripts/daily_diary.sh` is 166 lines,
runs `yoyo` exactly once at the end, and the invocation's output goes **straight to stdout** with
no capture at all:

```
"$YOYO_BIN" --model "${MODEL:-claude-opus-4-6}" --max-turns 1 < "$PROMPT_FILE"
rm -f "$PROMPT_FILE"
```

So there is nothing to read: no `tee`, no `AGENT_LOG`, and no `YOYO_AUDIT` export. The spend
exists only in a workflow log nobody counts. The file is **not** protected, and no Rust is
involved.

## What to do (mirror `scripts/social.sh`'s Day-209 block; read it first, do not re-derive it)

1. **Read `scripts/social.sh`'s accounting block** (`export YOYO_AUDIT=1`, `AUDIT_BEFORE`,
   `report_social_spend()`) and copy its *shape*, not its variable names.
2. In `scripts/daily_diary.sh`: `export YOYO_AUDIT=1` immediately before the invocation, capture
   the run's output to a temp file, re-emit **stdout byte-identical**, and read the usage line out
   of the captured file **before** it is deleted.
3. Print, on **stderr** (the diary text on stdout is consumed downstream — keeping stdout pure is
   what makes this additive):
   - `Spend (this run): …` when the usage line is present, matched by **the shape the binary
     actually emits** (leading `↳` plus a duration), read out of `format_usage_line`'s compact
     branch rather than hand-written from memory;
   - `Spend (this run): no usage record — the process did not reach its terminal emit
     (no_terminal_emit)` when it is absent — **reuse that wording**, never a `0`;
   - the per-run **audit delta** beside the cumulative byte size, **separately labelled**
     (`N record(s) this run (line A → B); S bytes cumulative in .yoyo/audit.jsonl`) — two numbers
     answering two different questions, never merged. A bare line count answers "what has this
     repo ever spent"; the delta answers "what did this run spend".
4. **Fail-soft is the contract**: the script runs under `set -euo pipefail`, so every command whose
   no-match case is normal needs `|| true` and a numeric fallback for `wc`. A missing audit file,
   an absent usage line, an empty `grep`, an unreadable temp path: warn and continue. The diary
   must still be produced in every one of those cases, with the same exit status as before.
5. **Do not touch** `scripts/evolve.sh`, `scripts/build_site.py`, `scripts/format_issues.py` or
   `.github/workflows/` (protected). The durable sink for this spend lives in a workflow file and
   is therefore a human's call; that residue stays in #944 and is named, not smuggled.

## Verification (the precedent's exact method — a stub binary, never a real diary run)

- `bash -n scripts/daily_diary.sh` clean.
- Extract the accounting block **with `sed` from the shipped file** rather than transcribing it, so
  the harness exercises the real code, and run each case in its own `mktemp -d` with a **stub**
  `YOYO_BIN` (the script honours `YOYO_BIN`, so no real model call and no credentials are needed).
- Run all of it as **one atomic command** and paste the literal output for each case:
  (a) usage line present; (b) usage line absent with a non-zero exit → prints the honest
  `no_terminal_emit` **and still reports the audit delta**; (c) no `.yoyo/audit.jsonl` at all → a
  warning, not a failure; (d) a persistent checkout with the audit file pre-loaded → the watermark
  is what makes the delta right; (e) **the near-miss that matters**: a degenerate run with
  `YOYO_AUDIT` unset, no audit file and no usage line prints both honest absences, exits **0**, and
  leaves **stdout byte-identical to the pre-change script** — assert `diff` on captured stdout,
  which is the whole regression surface for the daily post.

## Done when

The literal outputs above are pasted in the session summary, stdout is provably unchanged, the
script is still fail-soft, and ARCHITECTURE.md's `scripts/daily_diary.sh` note (create the entry if
the file has none — new history goes to ARCHITECTURE.md, never CLAUDE.md) records the measured
before/after and the stated residue: **this is a per-run report, not a durable sink**, and the
sub-agent token floor (yologdev/yoagent#173) means any total here is a **floor**.
