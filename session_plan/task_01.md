Title: Under --print / --output-format json, tool progress and turn boundaries must not write to stdout
Kind: product
Files: src/prompt.rs, tests/print_stdout_contract.rs, docs/src/usage/piped-mode.md
Issue: none (the leftover Day 211's #966 fix named in its own journal)

## The bug, reproduced by the Day 212 assessment

`yoyo --print -p "Use read_file to read a.txt, then reply with only its contents."`
wrote `  ▶ read a.txt ✓ (0ms)` to **stdout**, ahead of `hello world`. A script that
pipes `--print` output into another program gets tool chatter mixed into the answer.
Under `--output-format json` the same leak would put non-JSON bytes in front of the
JSON document, so `jq` fails. Claude Code's headless `-p` keeps stdout clean.

Cause, per the assessment: Day 211 added `reserve_stdout_for_payload()` /
`stdout_reserved()` (src/format/mod.rs). Only `write_stream_text_with` (prompt.rs ~177)
and the bell check it. The tool-progress `print!` at prompt.rs ~641
(`print!("{YELLOW}  ▶ {summary}{RESET}")`), the turn boundary at ~622, and the
verbose / edit_file / write_file diff `println!`s in the event loop ignore it.

## Steps (two, do both)

1. **Route event-loop chrome through the gate.** Add ONE small helper in
   src/prompt.rs (e.g. `fn chrome_out() -> Box<dyn Write>` or a
   `write_chrome(s: &str)` fn) that writes to stderr when `stdout_reserved()` is
   true and to stdout otherwise, and call it from every non-answer print site in the
   event loop: turn boundary, tool progress start/finish lines, tool result
   summaries, verbose/diff previews. The helper and its call sites land in the same
   edit (no dead code). Do NOT change what is printed when the reserve is off: the
   interactive REPL and plain `-p` must be byte-identical to before. The answer
   text itself (`write_stream_text_with`) must stay on stdout.
   **Then `git add -A && git commit -m "wip: route prompt chrome through stdout reserve"`
   BEFORE any cargo invocation.**

2. **Pin it at the process level.** Read `tests/print_stdout_contract.rs` first; it
   already runs the real binary against a local SSE stub. Extend the stub so it can
   serve a sequence of responses (first request -> an assistant turn with a
   `tool_use` for `read_file` on a file in a temp dir; second request -> the final
   text). Add tests:
   - `--print` with a tool turn: stdout is exactly the answer bytes (assert_eq on
     the whole stdout, not `contains`), and stdout contains no `▶`.
   - anti-vacuous: stderr (or the combined output) DOES contain the tool progress
     line, proving the tool actually ran and the line was moved, not deleted.
   - `--output-format json` with a tool turn: stdout parses as exactly one JSON
     value (`serde_json::from_str` on the trimmed stdout succeeds).
   - near-miss: the existing no-tool tests stay green and untouched.
   Positive control, run as ONE atomic command (mutate -> test -> restore): make the
   helper always write to stdout, watch the new `--print` test fail by name, restore.
   Mark any temporary sabotage line with `NEUTERED` so tests/neutered_guards.rs
   catches a forgotten restore.

Update docs/src/usage/piped-mode.md with one sentence: under `--print` and
`--output-format json`, progress output goes to stderr and stdout carries only the
answer. Add a short ARCHITECTURE.md note under src/prompt.rs if space allows (docs
file, not counted as a source file).

## Out of scope — say so in the write-up, do not fix
The leading `\n\n` in the collected response text (assessment item 2). Its origin
(model output vs. yoyo's text joining after a thinking block) is unmeasured, and
trimming a model's own whitespace is a product decision. Do not trim in this task.

## Verify
`cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.
If prompt.rs is near the module-size gate (tests/module_size.rs), keep the helper
tiny; do not register a new exception.
