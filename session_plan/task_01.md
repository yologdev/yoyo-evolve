Title: Persist the git reading of unhittable surprises that write_validation_event computes and throws away (DREAM milestone)
Kind: evolve
Files: src/commands_risk_snapshots.rs, src/commands_risk.rs, tests/module_size.rs
Issue: none (DREAM.md "next milestone")

## Why
Every validation event since day 209 records `unhittable_surprises` from the LEDGER join only. The ledger cannot see a file born in the same session (its first-scored row is written at the *next* snapshot, after the verdict), so the last two live rows read "unhittable 0, unmeasurable N" for `src/prompt/stream_external_servers.rs` (snapshot `bf8beaf6`) and `src/cd_config_note.rs` (snapshot `45fb1800`) — files that did not exist at their snapshot hash. The git check that CAN see this is already computed at the call site: `count_unhittable_surprises_with_git` returns an `UnhittableCount` carrying `git_born_after` and `git_unmeasured` (defined in src/commands_risk_unhittable.rs ~:76/:99, set ~:359-360). `write_validation_event` (src/commands_risk_snapshots.rs ~:487, call site ~:859) saves only the ledger count. The fast sense exists and has no voice. This task gives it one.

## Steps (two — do both in one pass)

1. **Writer.** In `write_validation_event`, add the two git fields to the persisted JSON row as new keys `git_born_after` and `git_unmeasured`, passed from the `UnhittableCount` that the call site already has (do NOT recompute; do not touch src/commands_risk_unhittable.rs — it sits at 1999 lines, one under the 2000 cap and not grandfathered, so any line added there is a size-gate warning). The function already carries `#[allow(clippy::too_many_arguments)]`; prefer passing the `UnhittableCount` (or a small struct) over two more loose args if that is cleaner. If the git leg could not run for an event, WRITE NOTHING for those keys (absent), never 0 — "could not check" must not read as "checked; zero".

2. **Reader.** Where `/risk accuracy` parses validation rows (src/commands_risk.rs ~:1073 area, `parse_surprise_rows` or its neighbour), parse the two keys as `Option<u32>` and print them beside the existing unhittable/unmeasurable figures, e.g. `unhittable 0 (ledger) / 1 born after snapshot (git), 0 git-unmeasured`. Rows written before this change have no keys → render as `git: not recorded`, distinct from `git: 0`. Pin that three-way distinction (absent / 0 / ≥1).

## Tests (required — a missing test leaves the tree exactly as green)
- **Round-trip through the REAL writer into the REAL parser** (Day 211 lesson: a hand-typed JSON row is a struct literal in another costume). Write an event with `git_born_after = Some(2)`, `git_unmeasured = Some(1)` into a tempdir ledger via `write_validation_event`, read it back with the real reader, assert the exact values and the exact rendered line with `assert_eq!`.
- Near-miss: an event written with the git leg absent parses as `None` and renders `not recorded`, not `0`.
- Back-compat: an OLD row (the one place a typed row is legitimate — it represents history the writer no longer produces; say so in a comment) parses without error and renders `not recorded`.
- **Positive control, atomic and serial:** in ONE command, neuter the writer's insert of `git_born_after` (mark the line `// NEUTERED POSITIVE CONTROL`), run `cargo test commands_risk`, confirm the round-trip test fails BY NAME, restore with `git checkout -- src/commands_risk_snapshots.rs` or an equivalent sed, re-run green. Report both halves.

## Size gate
src/commands_risk_snapshots.rs is 2127 vs its registered 2099 (+28, grace 100); src/commands_risk.rs is registered at 6526. After your edit, run `cargo test --test module_size` and paste whatever lines the gate prints for these files over their entries in tests/module_size.rs (`("src/commands_risk_snapshots.rs", N),`). If you move tests elsewhere to save room, say so; do not create a new file just to dodge the gate.

## Report, as values
In the commit message / ARCHITECTURE.md entry under src/commands_risk_snapshots.rs: what shipped, that PAST rows stay "not recorded" (this does not retrofit the ten live rows — that retrospective reading already exists in the retrospective unhittable note), and the signal to watch: the next watch event whose surprise list holds a file created in that same session should read `git_born_after ≥ 1`. That is NOT observable this session — say "unobserved in production" in those words.

## Verify
cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check
