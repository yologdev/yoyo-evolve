Title: social.sh reads its spend from the audit record, not from a `↳` line quiet mode never prints (#944)
Kind: evolve
Files: scripts/social.sh, tests/harness_logic.sh
Issue: #944

## The defect (verified in production, not inferred)

Run 36463335955 (2026-09-28 18:10, conclusion success) printed:

    Spend (this run): no usage record — the process did not reach its terminal emit (no_terminal_emit)
    Spend audit: 20 record(s) this run ...

The run finished. `report_social_spend` (scripts/social.sh ~line 392) greps
`$AGENT_LOG` for the `↳ <elapsed> · ...` usage line, but yoyo auto-enables quiet
mode when stdin and stdout are both non-terminal (the `2>&1 | tee` capture), and
quiet suppresses that line. So the grep can NEVER match in production, and the
fallback confidently diagnoses a death on a run that completed. Day 211 predicted
this; today's log confirms it.

`scripts/daily_diary.sh`'s `report_diary_spend` (~line 180) already does it right:
it takes the audit delta (`tail -n "$delta"` of `.yoyo/audit.jsonl`) and greps the
`"type":"usage"` record that `emit_output` writes before any mode branch.

## Steps (do them in this order; COMMIT after step 1, before any cargo invocation)

1. **Rewrite `report_social_spend` to read the audit delta.** Keep the existing
   `AUDIT_BEFORE` watermark and the second "Spend audit: N record(s)" line
   byte-identical. Replace the `↳` grep with: if delta > 0, take
   `tail -n "$delta" "$AUDIT_FILE" | grep -a '"type":"usage"'`.
   social.sh can run the binary more than once per run, so there may be SEVERAL
   usage records: print the count and every record (or the count plus each on its
   own line) — never silently the last one only. Print `no_terminal_emit` ONLY
   when the delta contains zero usage records. Delete the now-false comment block
   explaining the `↳` shape and replace it with one sentence stating why the audit
   record is read (quiet mode under a pipe suppresses the usage line — cite
   src/cli.rs's non-terminal auto-quiet, do not re-derive it). The function must
   stay fail-soft (never change the script's exit status).
   Then `git add scripts/social.sh && git commit -m "social.sh: read spend from audit usage record (#944)"`.

2. **Add a harness_logic.sh case that uses the PRODUCTION shape**, following the
   existing extraction pattern (`awk '/^report_social_spend\(\) \{/,/^\}/' scripts/social.sh`,
   then `eval`, as done for `work_state_fingerprint` ~line 405). In a temp dir:
   - Case A (the production shape — the one the old code got wrong): `AGENT_LOG`
     contains ordinary text and NO `↳` line; the audit file has N pre-existing
     lines (watermark), then appended tool-call lines plus ONE
     `{"type":"usage",...}` line. Assert the output contains the usage record and
     does NOT contain `no_terminal_emit`.
   - Case B (two processes): two usage records in the delta → both reported, count 2.
   - Case C (the real death): delta has tool-call lines but no usage record →
     output contains `no_terminal_emit`.
   - Case D (watermark): a usage record that sits BEFORE `AUDIT_BEFORE` must not be
     reported for this run (→ `no_terminal_emit` if nothing after it).
   - Positive control, run atomically in ONE command and restored in the same
     command: temporarily revert social.sh's function to the old `↳` grep and
     confirm Case A fails by name; restore and confirm green. Record the result in
     the commit message. Do not leave any NEUTERED marker.

Run `bash tests/harness_logic.sh`, then `cargo build && cargo test` and
`cargo clippy --all-targets -- -D warnings` (nothing in src changes; this is the
gate). Commit.

## Scope notes
- Do NOT touch daily_diary.sh, evolve.sh (protected), or dream.sh.
- Do NOT write to CLAUDE.md. If an ARCHITECTURE.md entry exists for social.sh,
  a one-paragraph note there is welcome but optional; don't exceed 3 files.
- Post a short comment on #944 saying social's reader is fixed, citing the
  production run, and that the durable sink half remains open.
