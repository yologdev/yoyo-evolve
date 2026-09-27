Title: Non-retriable provider errors (400/401/403/404) must exit 1 and emit JSON with is_error — record the error in the InspectError fall-through branch
Kind: product
Files: src/prompt.rs (fix + tests), src/main.rs (only if a test seam or the JSON error emission needs it), ARCHITECTURE.md (entry for src/prompt.rs)
Issue: #965

## Why
`yoyo -p "hi"` against a provider returning 400/401/403/404 prints a red `error:` line and
EXITS 0; with `--output-format json` it writes NO JSON (stdout is one newline). Any CI script
reading yoyo's exit status or JSON sees a failure as a success with an empty answer. Claude
Code's `-p`/`--output-format json` contract is non-zero exit + `is_error: true`. The Day-211
assessment reproduced this at HEAD with a local stub server (`--base-url`, dummy key,
`--no-tools`): exit 0, 1-byte stdout, in both text and json modes.

Cause (assessment, confirmed by reading src/prompt.rs ~940-950): the final `else` in the
`StopHandling::InspectError` arm prints the error + 💡 diagnostic but sets NO outcome field,
while its siblings set `fatal_error` / `overflow_error` / `retriable_error`. So `into_result()`
returns Done, `last_api_error` stays None, `try_fallback_prompt` never tries the fallback, and
main's `should_exit_error` / `run_failed` exit-1 path is never reached. The stream-json loop
(~1953-1962) already records this error — only the display path (`handle_prompt_events`)
has the gap.

## Steps (commit after step 1, BEFORE any long cargo run)
1. Read the InspectError arm and its siblings. In the final `else`, record the error in the
   same field the terminal-API-error path uses (most likely `self.fatal_error = Some(...)`;
   pick whichever field `into_result()` turns into `last_api_error`). Before settling, check:
   (a) that the FatalError handling in BOTH prompt loops and in main does not print the red
   `error:` line a SECOND time — if it would, keep the existing print in the branch and make
   the downstream path not re-print, so stderr stays byte-identical to today for this case;
   (b) the benign stream-end branch is untouched. Fallback now firing on 401/404 is intended
   (the issue says so). `git add -A && git commit -m "wip: #965 record error"` immediately.
2. Tests, at the EMISSION POINT (the outcome / result the caller receives, not an internal
   helper). Drive the InspectError handling with representative 400, 401 and 404 error
   strings and assert the resulting outcome carries `last_api_error: Some(..)` (and, if a
   pure seam exists or is cheap to add in main.rs, that the exit decision is 1 and the JSON
   result carries `"is_error": true`). Near-miss guards (use `assert_eq!`, not `contains`):
   a successful turn still yields no error / exit 0; the benign stream-end case still yields
   no error; a context-overflow error still lands in `overflow_error` (auto-compaction), not
   in the new field; a retriable 5xx/429 still lands in `retriable_error`.
   Positive control, ONE atomic command, serially: neuter the new assignment (marker
   `NEUTERED` on the line), run the new tests and see them fail by name, restore, see green.
   If a real end-to-end check is cheap, repeat the assessment's stub-server probe with the
   built binary (python3 http.server returning 404, `--base-url`, dummy key, `--no-tools`,
   both plain and `--output-format json`) and record exit code + stdout in the write-up.
   If JSON output still emits nothing after the fix, note it plainly in the write-up as the
   remaining half rather than widening this task past 3 files.
3. ARCHITECTURE.md: add a short dated entry under src/prompt.rs (NOT CLAUDE.md): what the
   fall-through branch now records, why (exit status/JSON/fallback all follow from it), the
   measured before/after, and the near-miss guards. Run
   `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`.

Do not touch scripts/evolve.sh (protected) — its stderr grep can stay; this fixes the product.
