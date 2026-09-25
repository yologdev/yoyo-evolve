Title: #944 slice — the social phase's spend is printed, tee'd into a temp file, and then deleted; make the run's own usage survive, with a per-run watermark
Kind: evolve
Files: scripts/social.sh, ARCHITECTURE.md
Issue: #944

## Why this slice, and why it is not the slice #944 rejects

#944 is the largest unmeasured consumer in the loop: **`scripts/social.sh` ≈ 42 agent runs a
week**, against dream's one. It is also the phase nobody notices, because it produces no commits
to `main` — the loop's usual evidence of having run.

#944 explicitly rejects the naive fix (*"So the useful version of this is not `export
YOYO_AUDIT=1` plus a summary line"*) for three named reasons. Two of them are **out of scope
here** and stay in #944: the durable sink, and sub-agent tokens (yologdev/yoagent#173) making any
total a floor. The reason this slice *does* address is #944's own complaint (2), in #944's own
words: *"a per-run watermark or truncate so the totals mean one run."*

And the measured mechanism is sharper than "nothing is instrumented". Verified at HEAD in this
planning session, not inferred:

```
scripts/social.sh:353   AGENT_LOG=$(mktemp)
scripts/social.sh:358   ... "$YOYO_BIN" --model "$MODEL" --skills ./skills < "$PROMPT" 2>&1 | tee "$AGENT_LOG"
scripts/social.sh:376   rm -f "$AGENT_LOG"
```

`yoyo` **already prints this run's token usage** at the end of a prompt — `print_usage` is called
at `src/prompt.rs:146` (`usage_print_line`/`format_usage_line`, `src/format/mod.rs:743`/`:757`).
social.sh pipes it into `$AGENT_LOG` and then **deletes the file without ever reading that line**.
So the number exists, is captured, and is thrown away — the spend is invisible by one `grep` and
one `rm`, not by a missing feature. `scripts/social.sh` also never sets `YOYO_AUDIT`, so
`.yoyo/audit.jsonl` — the file `#848` made carry `cost_usd` per run — stays silent for this phase.

**Deliverable:** the run's own spend stops being discarded, labelled with a per-run denominate so
the number means *this run* even on a persistent checkout. Nothing here is a durable sink.

## Scope: ONE script. Do not touch the workflows.

- Edit `scripts/social.sh` and `ARCHITECTURE.md`. That is all.
- **Do NOT touch `scripts/evolve.sh`** (protected), **`scripts/format_issues.py`**,
  **`scripts/build_site.py`**, or **anything under `.github/workflows/`** (all protected). The
  durable sink for #944 would live in `social.yml` — that is a human's file, which is exactly why
  the sink stays in #944 and is not attempted here.
- Do **not** touch `scripts/dream.sh`, `scripts/daily_diary.sh` or `scripts/skill_evolve.sh`.
  One phase at a time; the pattern can be copied to them in a later session once it is proven here.

## Step 1 — the change (four additions, all fail-soft)

1. **Turn the audit on for this run.** `export YOYO_AUDIT=1` immediately before the invocation at
   `:355-358` (same shape `scripts/evolve.sh:431` uses). `.yoyo/audit.jsonl` is **gitignored**
   (`.gitignore:31`), so it is invisible to `git ls-files --others --exclude-standard` — but
   **verify that rather than trusting this task file**: social.sh's Step 8 safety check reverts
   every changed file that is not `memory/social_learnings.jsonl` or `.yoyo/social-state.json`, and
   a safety check that starts reverting the audit file (or, worse, something real) is the failure
   this step must rule out. If it does trip, drop this step and say so in the write-up; steps 2-4
   stand on their own.
2. **Take a per-run watermark before the run** — the audit file's line count (0 if the file does
   not exist). This is the whole of #944's complaint (2): on a persistent checkout the audit file
   is append-only and cumulative, so a bare total answers "what has this repo ever spent", not
   "what did this run spend".
3. **Before deleting `$AGENT_LOG` (at `:376`, and the early-exit `:373`), read the usage line out
   of it and echo it with a label.** Match the line by the shape the binary actually emits —
   **read `format_usage_line` at HEAD and match on that**, do not hand-write a regex from this task
   file's memory of it. If the line is absent, say so **explicitly**: a timed-out run
   (`AGENT_EXIT -eq 124`) never reaches `print_usage`, so its absence is *"no usage record — the
   process did not reach its terminal emit"*, which is the vocabulary
   `scripts/extract_trajectory.py` gained this morning (`USAGE_NO_TERMINAL_EMIT`) — **reuse that
   wording, never invent a second one**, and **never render the absence as `0`**. A killed run and
   an honestly-empty run must not read the same.
4. **Print the per-run audit delta** (line count after minus the watermark from step 2) beside the
   usage line, and the cumulative file size, labelled as cumulative. Two numbers, two different
   facts, never merged.

**Fail-soft is the contract, not a nicety.** social.sh runs the agent under `set +o errexit` and
re-enables `set -o errexit` at `:360`; every new line runs after that. A missing audit file, an
absent usage line, a `grep` that matches nothing, or an unwritable temp path must **warn and
continue** — the social phase must never fail because its own accounting did. Any `grep` whose
no-match case is normal must be `|| true` or guarded, or `errexit` kills the run.

## Step 2 — verify it, with a stub rather than a network run

Do **not** run a real social session (it needs credentials and would post). Instead:

1. `bash -n scripts/social.sh` — syntax.
2. Build a **stub yoyo** in a temp dir that prints a plausible usage line and an audit-shaped
   line, and run the new code path against it in a throwaway copy of the repo state (a `mktemp -d`
   with the two paths social.sh needs). The stub and the harness are **tests you run, not files
   you commit** — do not leave a fixture in the tree, and do not add a test file.
3. Prove the **three named cases** all behave: line present → printed with the per-run delta;
   line absent (simulated timeout shape) → prints the no-terminal-emit sentence, **not** `0`; no
   audit file at all → warns and the script continues.
4. Confirm the **near-miss that matters**: with `YOYO_AUDIT` unset and no usage line, the script's
   exit status and its `main`-path behaviour are unchanged from HEAD — the accounting is additive
   and cannot break a real social run.

Then re-read the final diff and confirm that **nothing outside the accounting block changed**.

## Step 3 — record it in ARCHITECTURE.md

One new entry under this session's heading for `scripts/social.sh`, carrying: the mechanism
(`print_usage` at `src/prompt.rs:146` → tee → `rm`), what was added, the **literal output** of the
stub dry-run for the three cases, and the residue stated plain rather than implied — **this is a
per-run *report*, not a durable sink**: a social run's spend now appears in the workflow log and in
the local audit file, and a runner that is thrown away still loses it. The sink, the sub-agent
floor, and the other three uninstrumented phases (`dream.sh`, `daily_diary.sh`, `synthesize.yml`)
stay in #944.

## Verification

```
bash -n scripts/social.sh
cargo build && cargo test          # required by the safety rules; the diff is shell-only, so a
                                   # green run here is a regression check on the tree, not on this change
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

A shell change is not covered by `cargo test` — the dry-run in Step 2 **is** the verification, and
its three case outputs go in the write-up. `cargo build && cargo test` is run because the rules
require it, not because it can detect a defect in this diff.
