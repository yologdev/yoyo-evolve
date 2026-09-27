Title: Per-edit auto-check skips a cargo check command when the edited file cannot affect it (#961)
Kind: product
Files: src/tool_wrappers.rs, (new) src/auto_check_scope.rs if tool_wrappers.rs cannot grow, src/main.rs (mod line only if the new file is added)
Issue: #961

## Problem (measured by the creator, run 35909185710, Day 207)
`AutoCheckTool::execute` (`src/tool_wrappers.rs` ~650) runs `commands[0]` after EVERY successful edit. Under
`auto_watch` that command is `cargo clippy --all-targets -- -D warnings && cargo test`, not "lint/check" as the
comment claims. A task that edited only a `.py` and a `.md` file spent 2–4 minutes per tool call on it and hit the
1800s timeout with its work finished but uncommitted. The full suite still runs after the turn
(`run_watch_after_prompt`), so skipping the per-edit check changes latency only, not the final verdict.

## ORDER OF EVENTS
Commit after the first edit and before any cargo run (`git commit -m "wip: #961"`), then again at every green checkpoint.

## Design (product-safe; the rule must hold for non-Rust projects)
Add one pure helper, e.g. `fn edit_can_affect_check(edited_path: &str, check_cmd: &str) -> bool`:
- If `check_cmd` does not invoke `cargo` (whitespace-token match on `cargo`, not a substring, so `mycargo-lint`
  doesn't count), return **true**. Every non-Cargo user keeps today's behaviour byte-identically. This is the
  regression surface.
- If it does invoke cargo, return true only when the path could change a cargo result: extension `rs`, or file name
  `Cargo.toml`, `Cargo.lock`, `build.rs`, `rust-toolchain`, `rust-toolchain.toml`, or anything under a `.cargo/` directory.
  Everything else (`.md`, `.py`, `.json`, `.sh`, ...) returns false.
- Unknown or empty path: return true (fail toward running the check, never toward silently skipping).
In `AutoCheckTool::execute`, find where the edited path is available (the tool's params: `path` for
write_file/edit_file). When the helper returns false, return the inner result unchanged and run no command.
Fix the misleading comment so it describes what `commands[0]` actually is.

Module size: `tests/module_size.rs` registers `("src/tool_wrappers.rs", 5574)` and the file is already ~5579.
Read that file's rule. If growth is not allowed, put the helper and its table test in a new small
`src/auto_check_scope.rs` (with its `mod` line), and keep the net change to tool_wrappers.rs at about zero.

## Tests (required, named, at the emission point)
- Table test for the helper. `.md`, `.py`, `scripts/x.sh` under a cargo command give false. `src/a.rs`,
  `Cargo.toml`, `build.rs`, `.cargo/config.toml` give true. A non-cargo command (`npm test`, `pytest`) gives true
  for `.md` too (near-miss guard). An empty path gives true. `mycargo-lint` gives true for `.md` (token, not substring).
- If `AutoCheckTool` can be constructed in a test with a command whose effect is observable (e.g. a command that
  touches a marker file in a tempdir), assert: a `.md` edit leaves the marker absent, and a `.rs` edit creates it.
  If that seam does not exist, say so in the ARCHITECTURE.md entry. Do not fake it with a struct literal answer key (Day-210 lesson).
- Positive control, as ONE atomic command: neuter the helper to always return true, run the table test, confirm it
  fails by name, then restore. Put a `NEUTERED` marker on the line while it is neutered.

## Docs
ARCHITECTURE.md entry under `src/tool_wrappers.rs`. If docs/src mentions per-edit auto-check (grep `auto_watch`
under docs/src), add one sentence saying non-code edits skip the per-edit cargo check.
Finish with `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.
