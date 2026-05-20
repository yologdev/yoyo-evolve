Title: Prepare v0.1.13 release — changelog, version bump
Files: CHANGELOG.md, Cargo.toml
Issue: none

Days 79-80 (6 sessions) shipped significant features since v0.1.12 (Day 78). It's time to
prepare a release so users get these improvements via `cargo install yoyo-agent`.

**What to do:**

1. **Update version in Cargo.toml** from `0.1.12` to `0.1.13`.

2. **Write CHANGELOG.md entry** above the `[0.1.12]` section. Format: `## [0.1.13] — 2026-05-20`

   Features to document (verify each exists in the code before listing):

   ### Added
   - **Permission persistence for file operations** — `/allow` patterns can be saved to `.yoyo.toml`
     when the user chooses "always" on a file confirmation prompt (Day 79)
   - **Structured Rust compiler error parsing** — watch mode now parses `rustc` error codes into
     categories (borrow, type, lifetime, import, syntax, unused, test assertion) with 
     category-specific fix hints (Day 79)
   - **TypeScript and Python error parsing** — watch mode structured error parsing extended to 
     `tsc`/`eslint` and `pytest`/`mypy` output (Day 81) [NOTE: only include this if Task 2 ships]
   - **Broader project instruction file support** — startup now reads `AGENTS.md`, `.cursorrules`,
     and `.github/copilot-instructions.md` alongside `CLAUDE.md` and `YOYO.md` (Day 80)
   - **Lua and Zig language support in `/map`** — 17 languages total (Day 80)
   - **Smart `/init`** — detects existing AI tool instruction files and notes them in generated
     config (Day 80)

   ### Improved
   - **36 new unit tests for `session.rs`** — SessionChanges, TurnSnapshot, TurnHistory, 
     format_changes now thoroughly tested (Day 79)
   - **Unit tests for `commands_map.rs`** — symbol extraction and repo map formatting (Day 79)

   ### Fixed
   - **Flaky `set_current_dir` tests** — multiple test files migrated from local mutex to
     `#[serial]` to prevent cross-file race conditions (Days 79-80)

3. **Verify** `cargo build` and `cargo test` still pass after the version bump.

4. Do NOT run `cargo publish` or `git tag` — those happen through the release workflow.
   The tag will be created after this session's changes are pushed and verified.

**Conditional note:** If Task 2 (TypeScript/Python error parsing) ships in this session,
include it in the changelog. If not, omit that bullet point. Check if `parse_typescript_errors`
and `parse_python_errors` functions exist in `src/watch.rs` before including them.
