Title: --print and --output-format json emit the answer exactly once on stdout (#966 + its json sibling)
Kind: product
Files: src/prompt.rs, src/main.rs, src/format/mod.rs (only if an existing switch lives there)
Issue: #966

## The defect (reproduced by the assessment against a local Anthropic-SSE stub)
- `yoyo --no-tools --max-turns 1 --base-url <stub> -p hi --print` writes `\n{"answer": 42}\n\n{"answer": 42}`.
  The streaming renderer prints the text, and then `emit_output` (main.rs ~294) prints `response.text` again.
- `--output-format json` has the SAME leak, and #966 does not mention it: the streamed text lands on stdout
  before the JSON envelope, so `| jq` fails with "Extra data". Fixing only `--print` is the
  "two doors, one deaf" shape again. Both modes must be fixed in this diff.
- `-o <file>` is already correct. Do not change it.

## Steps (two, in this order)
1. **Commit before any cargo invocation once the edit is written** (the Day-209 ordering lesson).
   Find the switch. In print mode main.rs only calls `disable_color()`. Read how `is_quiet()` / `enable_quiet()`
   in src/format/mod.rs are set and what they already suppress. If quiet mode already silences streamed text,
   reuse it. Otherwise add ONE process-global "stdout is reserved for the final payload" flag (an AtomicBool
   with a setter and a getter, beside the existing quiet/plain switches). Set it in main.rs for
   `print_mode || json_output`, in the same place `disable_color()` is called, so that piped mode is covered too.
   Gate every streamed-text stdout write in src/prompt.rs on that flag. The assessment names the sites
   `print!` at ~858 (text delta), ~878 (think flush at agent end), ~1120 and ~1183 (markdown flush), plus the
   trailing `println!` when `state.in_text` is set. Grep `print!\|println!` in prompt.rs so none is missed.
   Suppress these writes; do not reroute them to stderr (stderr already carries the UI chrome).
   Leave `emit_output` as the single writer of the payload.
2. Tests at the emission point, in the same diff (a definition must land with its consumer):
   - Flag set: the streaming path writes nothing to stdout. If prompt.rs writes through `print!` directly, factor
     the decision into a pure `fn stream_text_to_stdout(reserved: bool) -> bool`, or equivalent, and table-test it.
     Also add a source-level guard that counts the gated sites, so a fifth ungated `print!` in the renderer
     fails the test. State in its doc comment that it proves the guard is present, not that it fires.
   - Near-miss guard: with the flag unset (plain `-p` without `--print`, and the REPL), streaming is unchanged.
     Assert byte-identical results where you can.
   - Both modes: one test for print_mode and one for json_output. Neither may pass because of the other.
   - Global-state hygiene: if tests toggle the global, use a `_with(flag)` parameter seam rather than
     mutating the process global (tests/global_state_races.rs names this remedy).

Manual check after the build, which should reuse the assessment's approach: a ~20-line python SSE stub on
127.0.0.1, `--base-url`, `--no-tools --max-turns 1`. With `--print`, stdout must equal the response text
exactly once with no leading blank line. With json, stdout must be a single JSON object (`python3 -m json.tool` accepts it).
Record the before and after bytes in the commit message. (Task 2 turns this into a committed test.)

Docs: add one line to docs/src/usage/single-prompt.md or piped-mode.md, if either documents --print or json,
saying that stdout carries only the final payload in those modes. Add a CHANGELOG.md line under Unreleased.
Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt -- --check` before declaring done.
Check tests/module_size.rs's register if prompt.rs or main.rs grows past its entry.
