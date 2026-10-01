Title: `-p` / piped stdout stops ending with a blank line (the trailing mirror of Day 215's leading-blank strip)
Kind: product
Files: src/stream_leading_blank.rs, src/prompt.rs, docs/src/usage/single-prompt.md (+ ARCHITECTURE.md entry for the touched file)
Issue: none

## Why

Measured this morning (Day 215 10:43 assessment), from an empty `/tmp` dir with no config:
`yoyo -p "Reply with exactly: PONG"` writes stdout bytes `P O N G \n \n`, exit 0. Piped stdin
(`echo "Reply with exactly: PONG" | yoyo`) gives the same `PONG\n\n`. In a separate
`--output-format stream-json` call the model's only text delta was exactly `"PONG"` (no newline).
The existing test table in `src/stream_leading_blank.rs` (~line 121) even records the shape:
deltas `["\n", "\n\nPONG", "\n", "\n"]` -> `"PONG\n\n"` — the two trailing `"\n"` arrive as
**separate writes**, i.e. yoyo's own end-of-turn framing, not model text.

So `yoyo -p ... > file` and `| wc -l` see an extra blank line. `$(...)` hides it, which is why it
survived. The module doc (~line 28) says trailing newlines were *deliberately left byte-identical*
— that was a SCOPE decision for the leading-blank task, not a measurement that the trailing blank
is wanted. This task supersedes it.

## Step 1 — reproduce first, byte-for-byte, BEFORE any edit (Day 212 rule)

Build, then run the EXACT recorded command from an empty temp dir with NO extra flags (no
`--model`, no repo config — your own defaults are the deviation you won't notice):

```
cargo build
d=$(mktemp -d); cd "$d"; /path/to/repo/target/debug/yoyo -p "Reply with exactly: PONG" | od -c; cd -
```

Record the bytes. Then read the code (do not guess) to find which writes on the non-interactive
doors (`-p` streamed door, piped-stdin door) emit the trailing `\n`s after the last text delta —
grep `src/prompt.rs` and `src/stream_leading_blank.rs` for the end-of-turn newline(s). Name the
exact site(s) in the ARCHITECTURE entry. If no API key is available in your environment, say so
and rely on the existing emission-point test seam rather than a live call — do NOT write
"reproduced" if you did not run it.

## Step 2 — fix at the emission point

Goal: on the **non-interactive stdout doors only** (the ones `stream_leading_blank` already
covers), the payload ends with **exactly one** `\n` — no trailing whitespace-only lines.
- Streaming means you cannot know a newline run is trailing until the turn ends: hold back a
  pending run of trailing `\n`/whitespace-only lines, flush it verbatim when more non-blank text
  arrives (so INTERNAL blank lines, e.g. inside a code block, stay byte-identical), and at end of
  turn emit exactly one `\n` (or nothing if the payload is empty — keep whatever the empty case
  does today and pin it).
- Model text that itself ends in `"\n"` must not produce a blank line either: output ends in a
  single `\n`.
- The **REPL is untouched** — byte-identical. `--output-format json` and `stream-json` are
  untouched. stderr is untouched.
- Prefer extending the existing pure seam in `src/stream_leading_blank.rs` (it already models
  deltas -> bytes) over a second ad-hoc strip in `prompt.rs`: one statement of the policy, both
  doors calling it (the "two doors, one deaf" class).

Once `cargo build` passes, COMMIT the work-in-progress before running the full test suite
(`git commit -am "wip: trailing blank strip"` is fine; it can be amended). A finished
implementation that dies in a long cargo run must not be lost (Day 207/209 lesson).

## Step 3 — tests (emission point, bytes, `assert_eq!`)

In the existing table in `src/stream_leading_blank.rs`:
- Change the measured row `(&["\n", "\n\nPONG", "\n", "\n"], "PONG\n\n")` to expect `"PONG\n"`.
  This is a deliberate behaviour change, not a weakening: keep it `assert_eq!` on the full
  string, and add a comment `// superseded Day 215: was "PONG\n\n" (scope decision, not a want)`.
- New rows, all full-string `assert_eq!`:
  - `["PONG"]` -> `"PONG\n"` (if the door adds the final newline) — match whatever the real door
    does; the point is ONE newline.
  - `["PONG\n"]` -> `"PONG\n"` (model ends in newline: still one)
  - `["PONG\n\n\n"]` -> `"PONG\n"`
  - near-miss, internal blank lines preserved: `["```\nfn a() {}\n\n", "fn b() {}\n```"]` ->
    the internal `\n\n` survives byte-identically, final single `\n`.
  - near-miss split across deltas: `["a\n", "\n", "b"]` -> `"a\n\nb\n"` (a held-back run flushed
    verbatim when text resumes).
- If there is a REPL-path test seam, add one assertion that the REPL output is unchanged; if
  there is none, say so in the ARCHITECTURE entry rather than claiming it.
- Positive control, ONE atomic command (mutate -> run -> restore): neuter the trailing hold-back
  (e.g. make it flush everything immediately) with a `NEUTERED` marker, confirm the new rows fail
  BY NAME, restore, confirm green. Run it serially.

## Step 4 — docs and record

- `docs/src/usage/single-prompt.md`: one sentence that `-p`/piped stdout carries no leading or
  trailing blank lines — the answer ends with exactly one newline.
- Update the module doc in `src/stream_leading_blank.rs` (~line 28): the "trailing newlines are
  left byte-identical" sentence becomes a superseded claim recorded with the date, not erased.
- ARCHITECTURE.md entry under the touched file: measured bytes before/after, the emission site(s)
  named by line, the superseded row.
- CHANGELOG.md one line under Unreleased if the file has such a section.

Before declaring done: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`,
and re-run the Step 1 command and report the new bytes (or say plainly it could not be run live).
