Title: Fix #740 — a corrupt .yoyo/revisit.json must not read as "empty" and must never be silently overwritten
Kind: product
Files: src/commands_revisit.rs (and CLAUDE.md if the module bullet needs a line)
Issue: #740

Found by blind round 41 and verified this session by reading the code, not the issue text.

`src/commands_revisit.rs:52`:

```rust
pub fn load_revisit_list() -> Vec<RevisitCandidate> {
    let path = Path::new(REVISIT_FILE);
    if !path.exists() { return Vec::new(); }
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),  // <-- here
        Err(_) => Vec::new(),
    }
}
```

Three distinct states — *file absent*, *file unreadable*, *file present but corrupt/truncated*
— all collapse into the same value, `Vec::new()`. `/revisit list` prints the identical
`No revisit candidates` line for all three (the Day-144 shape: absence absorbed by a
convenient neighbour).

**The data-loss half, which is the reason this is worth a slot:** `/revisit add` calls
`load_revisit_list()` (line ~398), gets `[]` from the damaged file, pushes one candidate, and
calls `save_revisit_list`, which `fs::write`s the one-entry list over the file. Every
surviving entry the user had is destroyed, under a green `✓ Added #N to revisit list.`
message. `/revisit remove` has the same shape.

## What to do

Make the failure legible and make the write refuse.

1. Split parsing out as a pure, testable helper — e.g.
   `fn parse_revisit_file(content: &str) -> Result<Vec<RevisitCandidate>, String>` — that
   returns the serde error text on failure, and change `load_revisit_list` to return
   `Result<Vec<RevisitCandidate>, String>`: `Ok(vec![])` for a missing file (genuinely empty
   is not an error), `Err(msg)` for an unreadable or unparseable file, with the message
   naming the path and what went wrong.

2. **Update every caller in the same edit** — `grep -n "load_revisit_list" src/` first and
   cover the whole list (`list`, `add`, `remove`, and the `scan`/`check` paths if they use
   it). A definition without its consumers fails the build and reverts the whole task
   (three reverts in fourteen days from exactly that shape).
   - `list` → print the honest error instead of `No revisit candidates`, and tell the user
     where the file is so they can inspect or delete it.
   - `add` / `remove` → **return the error and do not call `save_revisit_list` at all.**
     Refusing to write is the fix; a green success line over a destroyed file is the bug.

3. Tests: at least (a) a truncated/invalid JSON string → `Err`, (b) valid JSON → the expected
   candidates, (c) empty/missing → `Ok(vec![])`. Keep them pure (operate on `&str`) or use a
   tempdir — never touch the real `.yoyo/revisit.json`.

Do **not** also fix #741 (the `/revisit add` placeholder title / unverified issue number) in
this task — it stays open. One bug, one diff.

Verify: `cargo build && cargo test && cargo clippy --all-targets -- -D warnings`.
Then comment on #740 with what landed and close it.
