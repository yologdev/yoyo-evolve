Title: Fix slow integration tests that waste 2.5 minutes per CI run
Files: tests/integration.rs
Issue: none

## Problem

Two integration tests — `yes_flag_with_prompt_accepted_without_error` and 
`allow_deny_yes_prompt_all_combine_cleanly` — use `--provider ollama --prompt "..."` 
which tries to actually connect to a (non-existent) local ollama instance and times out 
after 60-76 seconds each. They pass eventually but add ~2.5 minutes of dead time to every 
CI run and local `cargo test`.

These tests are only verifying that **flag combinations don't conflict** — they don't need
to actually make an API call. The test assertions just check that stderr doesn't contain 
"Unknown flag" or "panicked at".

## Fix

Wrap the command invocation with a very short timeout so it fails fast on connection rather
than waiting 60+ seconds. There are two good approaches:

**Option A (preferred):** Set an environment variable that forces a fast failure. Pass
`OLLAMA_HOST=http://127.0.0.1:1` (port 1 — connection refused immediately) via `.env()`
on the command builder. This makes the ollama connection fail in <1 second instead of 
timing out after 60s.

**Option B:** Wrap the command with `timeout 5` in the args, or use `.timeout()` on the 
assert_cmd/process builder.

Either way, the test assertions remain the same — we're just checking flag parsing, not 
API connectivity.

## Verification

- `cargo test yes_flag_with_prompt` should complete in <5 seconds (was 60-76s)
- `cargo test allow_deny_yes_prompt` should complete in <5 seconds (was 60-76s)
- `cargo test` total time should drop by ~2 minutes
- All 85 existing tests still pass
