Title: /cd names the new directory's config it is NOT applying (permissions.deny, hooks, MCP, dir restrictions) instead of a vague "not reloaded"
Kind: product
Files: src/cd_config_note.rs (new), src/dispatch.rs, src/main.rs
Issue: #869

## Why this slice
#869: after `/cd`, the launch directory's permissions, dir_restrictions, hooks and MCP servers stay in force.
The new directory's `.yoyo.toml` is never read. Reloading the config is the real fix, but it is blocked:
- `loaded_config_is_project_local` is a write-once OnceLock.
- A naive reload could WIDEN the fence the user is living inside.
Both are named in the issue and need their own design pass.

This task is the half that is safe and narrow. Today the only notice is a DIM line at `src/dispatch.rs:~1450`:
`(project context was loaded from the original directory and is not reloaded — use /context to review)`.
That line does not mention that the new directory's `permissions.deny` is NOT in force. That is the
safety-relevant fact: a user who `cd`s into a repo whose own config denies `rm -rf` believes they are protected, and they are not.

## Constraint: size gate
`src/dispatch.rs` is 2350 lines against a registered 2338 (tests/module_size.rs:349). Put ALL new logic in a
NEW module `src/cd_config_note.rs`. dispatch.rs gets only the call site (a few lines). If the call site pushes
dispatch.rs past its grace band, shorten the existing DIM note rather than re-registering.
Check `cargo test --test module_size` before finishing.

## Steps

1. **New module, pure and table-tested.** In `src/cd_config_note.rs`:
   - `pub(crate) fn unapplied_config_sections(toml_text: &str) -> Vec<&'static str>`: parse with the `toml` crate
     that config.rs already uses (check Cargo.toml). Return which of a FIXED list of sections the file
     sets, in a fixed order: `permissions` (allow/deny), `dir_restrictions` (use whatever key name config.rs actually
     parses for directory restrictions; grep config.rs for it and do not guess), `hooks`, `mcp_servers`.
     On parse error, return an empty Vec. Unparseable is not the same as "sets nothing", so ALSO expose that case
     (for example return `Result<Vec<_>, ()>`, or a small enum), and the note should then say the file could not be read.
   - `pub(crate) fn cd_config_note(new_dir_display: &str, sections: &[&str], plain: bool) -> Option<String>`:
     `None` when `sections` is empty. This is the regression surface: a directory with no `.yoyo.toml`, or one that
     sets none of these, gets NO new output. Otherwise, one line: this directory's `.yoyo.toml` sets <sections>, and they are NOT
     applied in this session. The launch directory's settings stay in force. Restart yoyo here to use them (#869).
     Glyph-free when `plain` (no em dash, no bullets), matching `collision_guard_skipped_message`.
     Sanitize `new_dir_display` through `crate::cli::sanitize_for_display`, because the path is repository-controlled.
   - Tests. Build fixtures THROUGH the parser from real TOML text. Never hand-typed `Vec`s asserted against the renderer.
     - A TOML with `[permissions] deny = ["rm -rf *"]` gives exactly `["permissions"]`, and the note names it.
     - The near-miss: a TOML setting only `model = "x"` / `provider` gives an empty Vec, and the note is `None`, checked by `assert_eq!`.
     - A TOML setting all four gives all four, in order.
     - Garbage text gives the unreadable case, which is distinct from empty.
     - `plain = true` output contains no `—` and no `•`.
     - An anti-vacuous check that the permissions fixture really contains `deny`.
   Register the module in `src/main.rs` (`mod cd_config_note;`).

2. **Wire it at the /cd success arm.** In `src/dispatch.rs`, after the existing `reevaluate_trust_on_cd` call in the
   `Ok(())` arm of `set_current_dir`: if `./.yoyo.toml` exists in the new cwd, read it (a read error means print nothing extra,
   no panic), compute the sections, and print the note to stderr in DIM. Respect the same quiet gating the
   existing DIM line uses. Keep the existing DIM line.
   This task only READS the file. It must not apply, merge or execute anything from it, and it must not change
   trust, permissions or any global. Add one sentence to the /cd entry in `docs/src/usage/commands.md` (if /cd is documented
   there; grep first) and a short ARCHITECTURE.md entry under dispatch.rs:
   "disclosure slice of #869; the reload itself is still open, and this is not a control".

## Positive control (serial, atomic)
Neuter `cd_config_note` to always return `None` (put a `NEUTERED` marker on the line), run the module's tests, watch the
permissions test fail by name, restore, and re-run green. Do it all in ONE command.

## Honest limit, to be written in the commit message
Disclosure is not a control. Every launch-dir setting still applies after /cd, and the new directory's deny list
still does not. This tells the user so, specifically. #869 stays OPEN.

Finish with `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.
