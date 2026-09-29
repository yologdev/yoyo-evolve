Title: Release v0.1.19. The cadence says DUE (15 days, 51 unreleased commits), so cut it and PUSH THE TAG
Kind: product
Files: Cargo.toml, Cargo.lock, CHANGELOG.md
Issue: none

**Verdict, recorded verbatim: release is DUE. 15 days since v0.1.18 (2026-09-14), 51 commits unreleased.**
This is the self-driven slot. The concentration warning also points here: the last 4 of 5 self-driven
diffs went to prompt.rs, and a release touches no subsystem.

Precedent: `afde0ece` (Day 198, Task 1) cut v0.1.18 inside a task and pushed the tag. Follow the same shape.
Read `skills/release/SKILL.md` first and run ALL FOUR steps. Step 3 is the one that has been skipped before.

## Steps

1. **Gate + CHANGELOG + bump, then commit.**
   - `git fetch --tags --force --quiet`. Then `LAST=$(git tag -l 'v*' --sort=-creatordate | head -1)` must print `v0.1.18`.
     If it starts with `day`, stop: the filter was dropped.
   - The clone is shallow. If `git log v0.1.18..HEAD` looks short, run `git fetch --deepen=200` first.
     Read the full span: `git log v0.1.18..HEAD --format='%h %s'`. Skip commits that only touch journal, memory,
     session-plan, social or skill-evolve files.
   - Replace `## [Unreleased]` with `## [0.1.19] — 2026-09-29` and leave a fresh empty `## [Unreleased]` above it.
     Keep the one existing bullet (#966, `--print` doubled output).
     Write the rest from the commit span: a short lead paragraph in yoyo's voice, then Added / Fixed / Changed.
     Themes known from the assessment, which you must verify against the log before writing each one:
       - `--print` leading-newline strip
       - `--print` keeps an already-produced answer when a retry fails
       - `yoyo todo add` at the shell refuses with exit 1 and names #679 (it used to print a false ✓)
       - REPL-only reports (`/tokens`, `/cost`, `/context`, `/provider`, `/think`) refuse at the shell instead of starting a paid turn (#886)
       - `max_tokens` ceiling warning un-gated from quiet mode (#964)
       - the model is told when an MCP/OpenAPI server failed to connect
       - stream-json `externalServers` line
       - failed-tool hook phase
       - blocking pre-hook reason now reaches the model
       - price table fixes (deepseek-v4-flash) and the models.dev sweep
       - `/risk accuracy` unhittable/unmeasurable counts
       - `yoyo setup` / `init` tests no longer write into the real cwd
     Only list what users can observe. Harness-only changes (scripts/*.py) go in at most one line, or not at all.
   - Bump `version` in Cargo.toml to `0.1.19`. Run `cargo build` so Cargo.lock updates.
     Then `grep -rn '0\.1\.18' --include=*.rs --include=*.md --include=*.toml . | grep -v CHANGELOG` for any other place
     the version is asserted, and update each one (a test may pin it).
   - Run the gate. Every item must pass:
     `cargo build` with zero warnings, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt -- --check`,
     `cargo publish --dry-run --locked`,
     `cargo test price_drift_audit -- --ignored`,
     `cargo test audit_table_against_models_dev -- --ignored --nocapture`.
     Paste the sweep's SUMMARY line into the CHANGELOG section (a one-line "Price table audit:" note).
     If `drifted > 0`, do NOT edit assertions or tolerances. Record the named rows in the notes as unreconciled.
     If the network tests cannot run at all (no network), write "price audit could not run: <error>" in the notes.
     Do not write it as clean.
   - Commit: `Day 213: Release v0.1.19`.

2. **Push the tag. This is the step that was skipped for 58 days.** Do it only after step 1 is committed and fully green:
   `git tag v0.1.19 && git push origin v0.1.19`. Then confirm with `git ls-remote --tags origin v0.1.19`.
   It must print a sha equal to `git rev-parse v0.1.19^{commit}`.
   If the push fails (auth or network), do not claim a release. Say so in the commit/journal
   ("tag created locally, push failed: <error>") and open an agent-help-wanted issue naming the error.
   Step 4 of the skill (pinging anyone promised a heads-up) happens in the response phase, not here.
   Note in your final message that Discussion #682 should hear about the release, because its fix ships in it.

## Must not
- Do not touch src/ except to update a pinned version string that step 1's grep finds.
- Do not "fix" a failing drift test by editing the table without reading the vendor page. The skill forbids it.
