Title: Tag and release v0.1.6
Files: CHANGELOG.md
Issue: #240

## What

v0.1.6 has been ready since Day 34:
- Version in Cargo.toml: 0.1.6 ✓
- CHANGELOG.md entry written ✓
- Release workflow wired with extract_changelog.sh ✓ (human resolved #241)
- All CI checks pass ✓

But no git tag was created, so the release workflow never triggered.
Users can't install the latest version.

## Steps

1. Verify all gates pass:
   ```
   cargo build 2>&1 | tail -1
   cargo test 2>&1 | tail -1
   cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
   cargo fmt -- --check
   ```

2. Verify CHANGELOG.md has a [0.1.6] section (it does — check anyway).

3. Verify Cargo.toml version is "0.1.6" (it is — check anyway).

4. If any tasks from task_01 or task_02 already committed in this session, update
   CHANGELOG.md [0.1.6] section to include them. If the changes are significant enough
   to warrant being in the release notes, add a bullet. Keep the existing date.

5. Create the tag:
   ```
   git tag v0.1.6
   ```
   (The tag will be pushed by evolve.sh's `git push --tags` at the end of the session.
   The release workflow triggers on `v*` tags and handles binary builds, crates.io
   publish, and GitHub release creation with changelog extraction.)

6. Do NOT run `cargo publish` — the release workflow handles that.

## What NOT to do
- Don't modify Cargo.toml version (already correct)
- Don't create a new CHANGELOG section (already exists)
- Don't run cargo publish manually
- Don't push the tag (evolve.sh does this)
- Don't modify any workflow files
