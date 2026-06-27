Title: Log warnings instead of silently discarding risk weight/validation write errors
Files: src/commands_risk.rs
Issue: none (self-discovered bug from assessment)

## What

The assessment found 5 `let _ =` instances in `commands_risk.rs` that silently discard
IO errors in production paths. The most critical is line 360: `let _ = std::fs::write(weights_path, json_str)`
which silently drops the entire risk weight file. If this fails, weights are lost with zero diagnostic.

## Implementation

1. **Line 357** (`let _ = std::fs::create_dir_all(parent)`) — Replace with:
   ```rust
   if let Err(e) = std::fs::create_dir_all(parent) {
       eprintln!("  {DIM}(warning: could not create risk weights dir: {e}){RESET}");
       return; // or early-return from the block
   }
   ```

2. **Line 360** (`let _ = std::fs::write(weights_path, json_str)`) — Replace with:
   ```rust
   if let Err(e) = std::fs::write(weights_path, json_str) {
       eprintln!("  {DIM}(warning: could not save risk weights: {e}){RESET}");
   }
   ```

3. **Line 1570** (`let _ = std::fs::create_dir_all(parent)`) — Same pattern, log warning.

4. **Line 1579** (`let _ = writeln!(file, "{json_str}")`) — Same pattern, log warning.

5. **Line 3496** (`let _ = coupling.len()`) — This one is in a test; check context. If it's
   intentionally discarding a value to avoid unused warnings, leave it. If it's masking an
   error, fix it.

These are all best-effort operations so the function should NOT return an error — just
log to stderr so operators can diagnose failures. Use the existing `{DIM}` formatting
constant for consistency with other warning messages.

## Tests

No new tests needed — these are logging improvements. Verify with `cargo build && cargo test && cargo clippy --all-targets -- -D warnings`.
