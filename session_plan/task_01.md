Title: /cd's "your deny list is not being applied" note is printed as a warning, not as DIM chrome
Kind: product
Files: src/cd_config_note.rs, ARCHITECTURE.md, CHANGELOG.md
Issue: #869 (safety half, disclosure volume only — #869 stays OPEN)

## Why
Day 213 (#869) added a note after `/cd`. It names the new directory's `.yoyo.toml` sections that are NOT applied, including a `permissions.deny` list the user may think protects them. The note is printed as `eprintln!("{DIM}{note}{RESET}")` (src/cd_config_note.rs:127), which copies its volume from the vague DIM line above it (src/dispatch.rs:~1449). DIM says "skip this". The content says "you are not protected". The Day-213 learning names this exactly: a warning's volume was taken from its neighbour instead of from its severity. This task fixes the volume only. It does not apply the new config. That is still #869's open half and needs its own design pass (monotonic narrowing, the `loaded_config_is_project_local` OnceLock blocker).

## Decision, written before the code
Volume follows severity:
- **Safety-relevant** note: it names any of `permissions` (allow/deny), `dir_restrictions`, `hooks`, `mcp_servers` (use whatever section identifiers cd_config_note.rs already uses; read them from its own list, never re-spell them). Render it as a **warning**: yellow (use the existing `YELLOW`/`BOLD_YELLOW` color constant from `crate::format`, which already respects no-color), prefixed `⚠ warning: `.
  - Under `is_plain_output()` the prefix is `warning: `, with no glyph and no escape bytes.
  - It is **NOT suppressed by --quiet**. A warning is not chrome: the max_tokens ceiling warning was un-gated from quiet on Day 211 for the same reason. Write this decision in a comment at the print site.
- **Not safety-relevant** (e.g. only `model` or `provider` differ): stays exactly as today. DIM, same quiet gating, **byte-identical output**. This is the regression surface.

## Steps
1. Read src/cd_config_note.rs and its tests, then ARCHITECTURE.md's entry for it (required, per CLAUDE.md). Extract a pure function, e.g. `render_cd_config_note(note_sections/..., plain: bool) -> (String, Severity)` or similar. It takes `plain` and the "is quiet" decision as **parameters**, never reading globals, so tests do not touch process state. Wire the print site to it. Commit (`git commit`) as soon as it builds, BEFORE running the full `cargo test`.
2. Tests, table-driven, asserting on the exact string the caller prints:
   - safety-relevant note, non-plain → contains the YELLOW escape, does NOT contain the DIM escape, and contains `warning:`;
   - the same note, plain → `!out.as_bytes().contains(&0x1b)`, no `⚠`, starts with `warning: `;
   - the safety-relevant note is emitted even when quiet = true;
   - **near-miss:** a note naming only a non-safety section → full-string `assert_eq!` against today's DIM rendering, and still suppressed under quiet;
   - anti-vacuous: assert the safety section list is non-empty before looping it.
   Build fixtures through the real TOML parser / the real section detector the file already uses, not by typing the section list by hand (the Day-210/211 answer-key lesson).
   - Positive control, as ONE atomic command: temporarily make the severity function always return the DIM rendering, run the tests, and confirm the warning tests fail by name. Restore it, then confirm green. Mark the sabotage line `NEUTERED` while it exists.
   - Then run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.
   - Add an ARCHITECTURE.md bullet under src/cd_config_note.rs (volume chosen from severity; quiet-exemption decision; #869 still open, because disclosure is not a control) and a CHANGELOG "Unreleased" line. Watch tests/module_size.rs: if cd_config_note.rs grows past its cap, follow that gate's instructions.

## Honest scope
This makes the disclosure louder. It does not make the user safer in the new directory. Say that in the commit message, and do not close #869.
