Title: `-p` stdout stops leading with blank lines (the third door of the Day-212 strip)
Kind: product
Files: src/prompt.rs (or wherever the streaming `-p` stdout path writes text), src/main_tests.rs, CHANGELOG.md
Issue: none (self-discovered, Day 215 assessment)

## Why
Day 212 (cc502136) strips the model's leading whitespace-only lines on the reserved-payload doors
(`--print` and `--output-format json`; tests `answer_payload_strips_only_leading_blank_lines` and
`json_response_drops_leading_blank_lines` in src/main_tests.rs). The Day-215 assessment measured the
streaming `-p` door from /tmp with no config and the default model:
`yoyo -p "Reply with exactly: PONG"` -> stdout bytes `\n\n\nPONG\n\n` (od -c). So `x=$(yoyo -p ...)`
gets leading blank lines (command substitution strips trailing newlines, not leading ones).
Two doors fixed, one deaf.

## Steps
1. Reproduce BYTE-FOR-BYTE first (Day-212 lesson): from /tmp, no config, no `--model` flag added:
   `cd /tmp && <repo>/target/debug/yoyo -p "Reply with exactly: PONG" | od -c | head`.
   Do not add your own model/flags. If it does not reproduce under the exact command, write
   "did not reproduce under the exact recorded command" with the od output, and stop — that is
   the deliverable. Then, with `--output-format stream-json`, record which leading `\n` are the
   provider's first text delta and which (if any) yoyo prints itself (e.g. a turn-boundary/
   framing newline written to stdout before the text). Write the finding into the commit message.
2. Fix at the emission point: on the streaming text path to stdout, suppress whitespace-only
   leading text until the first non-whitespace character of the answer arrives (the model's
   leading `\n` may come split across several deltas — buffer only the whitespace prefix, never
   delay real text). If yoyo itself writes a framing newline to stdout before the answer in
   non-interactive `-p` mode, move it to stderr or drop it there. REUSE the Day-212 helper's
   rule (strip only leading whitespace-ONLY lines; keep leading indentation on the first content
   line) — do not write a second, differing copy; if the helper takes a whole string, add a small
   streaming wrapper that delegates its rule to it.
   Interactive REPL output must be byte-identical — gate this on the non-interactive `-p` path
   only, if the code distinguishes them.

## Tests (write first)
- Emission-point test: feed the streaming writer the deltas `["\n", "\n", "PONG"]` and `["\n\nPO", "NG\n"]`
  and assert the bytes written are exactly `PONG` / `PONG\n` (assert_eq!, not contains).
- Near-miss: `["  indented\n", "\nmore"]` is byte-identical (leading indentation and INTERIOR blank lines kept).
- Near-miss: a delta stream that is all whitespace still writes nothing harmful (no panic, no hang).
- Positive control: neuter the strip (marked `NEUTERED`), watch the first test fail by name,
  restore in the same command, watch it pass.

## Docs
CHANGELOG.md Unreleased: one line. If docs/src/usage/single-prompt.md mentions output framing, update it.
Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` before declaring done.
Check ARCHITECTURE.md's entry for the file you touch before editing it.
