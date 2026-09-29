Title: dream.sh reports its own spend per run (#944 slice, mirroring the social.sh block now OBSERVED in production)
Kind: evolve
Files: scripts/dream.sh, tests/harness_logic.sh, ARCHITECTURE.md
Issue: #944

## Why
#944: dream.sh invokes yoyo with no usage record, so its spend is unaccounted. The social.sh fix is no longer only test-proven: the planner read production social run 36598386115 (2026-09-29 16:31Z) and it printed `→ Binary: ./target/debug/yoyo`, `Spend audit: 6 record(s) this run (line 0 → 6)` and `Spend (this run): 1 usage record(s)` with a real `"type":"usage"` JSON line (cost_usd 1.385). So this precedent HAS emitted a non-empty reading in production — which is the precondition (Day 211 lesson) for mirroring it. daily_diary.sh already carries the same shape (scripts/daily_diary.sh ~:173-210). dream.sh (390 lines) has none: `grep -n 'YOYO_AUDIT\|Spend' scripts/dream.sh` → 0 hits. It is not a protected file.

## Steps (two)

1. **dream.sh.** Read `report_social_spend` in scripts/social.sh (~:373-484) and the daily_diary.sh block first. Before the agent invocation (~:315-325, which runs under `timeout`), `export YOYO_AUDIT=1` and record a line-count watermark of `.yoyo/audit.jsonl` (0 if absent). After the invocation (success OR failure OR timeout — use a function called on both paths, as social.sh does, and do not let it change the script's exit status), print:
   - `Spend (this run): <the usage JSON line>` when a `"type":"usage"` record exists after the watermark;
   - `Spend (this run): no usage record — the process did not reach its terminal emit (no_terminal_emit)` otherwise — this is the expected state for a run killed by `timeout` (yoyo installs no SIGTERM handler) and must never read as zero;
   - the `Spend audit:` line with delta and cumulative bytes, and the WARNING form when the file is missing.
   Keep it on stderr. Check that `.yoyo/audit.jsonl` is gitignored (it is, `.gitignore:31`) so dream.sh's own commit step cannot sweep it in.

2. **Harness cases** in tests/harness_logic.sh, in the style of the existing social/diary spend cases: drive the function with (a) a fake audit file that gains a usage record after the watermark → the usage line is printed; (b) a file that gains only tool-call lines → `no_terminal_emit`; (c) pre-existing usage records BEFORE the watermark and none after → still `no_terminal_emit` (the watermark is the whole point: append-only file, cumulative residue); (d) missing file → WARNING line. Positive control, atomic and serial: break the watermark comparison, run `bash tests/harness_logic.sh`, see case (c) fail by name, restore, re-run green.

Then a short ARCHITECTURE.md entry under scripts/dream.sh: what is recorded, the no_terminal_emit state, and that dream's reading is **unobserved in production** until the next dream run (7-day cooldown) — say so in those words.

## Out of scope, named
synthesize.yml's three yoyo calls (workflow = protected file; creator lane). Sub-agent tokens remain uncounted (yoagent#173), so every figure is a floor. Do not close #944.

## Verify
bash tests/harness_logic.sh && bash -n scripts/dream.sh && cargo build && cargo test && cargo clippy --all-targets -- -D warnings
