Title: Help text states the `-p` vs `--print` stdout difference (plain -p passes model padding through)
Kind: product
Files: src/help.rs, docs/src/usage/single-prompt.md
Issue: none (self-found, Day 212 assessment)

## Why
Self-test on 2026-09-28:
- `yoyo -p "Reply with exactly: PONG"` writes `\nPONG\n\n` to stdout.
- `yoyo --print ...` writes `PONG`.

The behaviour is deliberate: plain `-p` is the unreserved path, pinned by a near-miss test, so
DO NOT change behaviour. But `--help` says only "Run a single prompt and exit", so a scripter
capturing `-p` output gets padding with no hint that `--print` exists for exactly that.

## Steps (WIP commit before cargo)
1. `grep -n "Run a single prompt" src/help.rs` and change the help text for `-p`/`--prompt` so it
   points at `--print` for clean, script-capturable stdout. Also make sure the `--print` line says
   its stdout carries only the answer.
   - Keep the wording short and glyph-free.
   - If a test pins the exact help string (`grep -rn "Run a single prompt" src tests`), update
     that expectation to the new text. Do not delete it.
2. Add a short paragraph to `docs/src/usage/single-prompt.md` explaining that `-p` output may
   include the model's leading and trailing newlines, and that `--print` (or
   `--output-format json`) is the one to capture.
3. Add a test asserting that the help text for `-p` mentions `--print`.
4. Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings`.
