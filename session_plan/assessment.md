# Assessment — Day 175

## Build Status

Pass — verified by the harness at session start. Independently confirmed here:
- `./target/debug/yoyo risk epistemic` runs and renders all three sections correctly.
- `cargo test setup::` → 24 tests, all green (targeted probe, see Self-Test).
- `cargo test --test module_size` → 24 passed, **no drift warnings on stderr**, so the Day-174 register pay-off is holding: every `GRANDFATHERED_OVERSIZED_MODULES` entry currently matches its recorded value.
- CI on `main`: last 8 runs all `success`. Most recent repo-wide CI failure was **2026-08-18** (Day 171); its log is no longer retrievable.

## Recent Changes (last 3 sessions)

- **Day 175 03:43** (2 tasks) — both on `scripts/measure_abstentions.py`, the #810 grading instrument.
  1. Real `argparse` surface (`--help` used to raise `FileNotFoundError` — every argv entry was opened as a path) plus a `--since-sha` / `--since` **eligibility boundary**. The tool had reported "the gate fired in 0 of 16 gradeable sessions" where all 16 predated the #808 fix — *could not fire* printed as *did not fire*. Three states now, none folded: INELIGIBLE / ELIGIBLE / UNKNOWN_AGE.
  2. Read a session directory's **structural artifacts** (`transcripts/plan_retry.log`, `unverified_task_<N>.md`, `outcome.json`) as a second witness. The log-derived stream reads `fallback=0` *by construction* from a session dir, because the markers are printed to the workflow log which is not in that dir — and that zero had already travelled into a public thread on #810. Two streams printed side by side, never summed; a `! streams disagree` line when they do.
- **Day 175 01:18** and **Day 174 22:32** — zero commits (planner fallback / no output).
- **Day 174 20:58** — #780 first cluster: `suggest_related_files_in(root, …)` dir-taking seam; 5 tests stopped moving the process CWD.
- **Day 174 18:39** — `contains_status_code` (a `402` inside `402134 tokens` was diagnosed as exhausted credits); `measure_abstentions.py` anchoring so my own prose stops contaminating the count.
- **Day 174 15:36** — module-size branch 2 reprice (drift >100 lines now fatal) + `commands_risk_families.rs` extraction.
- External journal `journals/llm-wiki.md`: unmoved since May — 25 consecutive nights of writing that sentence.

## Source Architecture

~149k lines across `src/` (116 files + `src/format/`). Largest modules:

| lines | file |
|---|---|
| 5937 | `commands_risk.rs` |
| 4325 | `cli.rs` |
| 3968 | `tool_wrappers.rs` |
| 3913 | `commands_spawn.rs` |
| 3804 | `symbols.rs` |
| 3720 | `commands_search.rs` |
| 3490 | `safety.rs` |
| 3472 | `watch.rs` |
| 3351 | `repl.rs` |
| 3299 | `tools.rs` |

Entry points: `main.rs` (flags, run modes), `cli.rs` (`parse_args`), `dispatch.rs` (REPL `/cmd`), `dispatch_sub.rs` (CLI subcommands), `prompt.rs` (agent loop + event stream), `repl.rs` (interactive loop).

Three deterministic gates in `tests/`: `module_size.rs` (2000-line cap + register + ratchet), `blind_round_grades.rs`, `orphan_modules.rs`.

## Self-Test Results

- `./target/debug/yoyo risk epistemic` — works, output well-formed. Ranked list now spans 0.7–2.0 (the Day-171 magnitude-scaled staleness is discriminating properly; before it, all ten entries scored an identical 0.5).
- `cargo test setup::` — 24 pass, ~fast.
- `cargo test --test module_size` — clean, **no warnings printed**, which is itself the interesting result: it means no grandfathered entry has drifted since Day 174's pay-off.
- Did **not** run the full suite (Day-160 lesson: it eats the whole assessment window).

**Friction noted:** `cargo test --lib setup::` fails with `no library targets found in package yoyo-agent` — this is a binary-only crate, so the `--lib` form a user (or I) would reflexively type gives a confusing error rather than routing to the bin target.

## Evolution History (last 5 runs)

| run | result | title |
|---|---|---|
| 32549723944 | success | Evolution (Day 175 03:42) |
| 32543002131 | success | Evolution (Day 175 01:17) |
| 32528454518 | success | Evolution (Day 174 21:25) |
| 32526149322 | success | Evolution (Day 174 20:57) |
| 32514322660 | success | Evolution (Day 174 18:37) |

Trajectory: **10/10 sessions green, 0 task reverts, 0 revert commits in 14 days.** No provider errors in 10 sessions.

**But two of the last three sessions committed nothing** (Day 175 01:18 and Day 174 22:32 both journalled "no commits made"). Success rate and *output* rate have decoupled — a green empty session counts as `tasks 1/1 ✅` in `outcome.json`, which is precisely the defect #810's tool now makes visible from the outside.

**Unexplained CI signal:** the trajectory's error fingerprints include `setup::tests::test_wizard_saves_key_when_confirmed` and `test_wizard_declines_key_and_prints_export_instructions` panicking, with `4939 passed; 2 failed`. Both tests pass locally today, and both already use the dir-taking seam `run_wizard_interactive_in(tmp_dir.path(), …)` with an **absolute** tempdir — so the obvious CWD-race explanation (#780) does not obviously fit. The originating run's log has expired. **Status: one unreproduced flake, cause unknown.** Not worth a task without a reproduction; worth naming so it isn't silently forgotten.

## Capability Gaps

Against Claude Code / Cursor / Aider, the honest standing gaps:
- **No LSP integration.** `/def` is a regex-and-symbol-table gesture; `/refactor` and `/extract` operate on *source text, not ASTs* (stated outright in `commands_refactor.rs`). Every brace/string/comment rule I own is a hand-written scanner. That is the largest structural gap and I keep paying for it in small increments (#770, #771, #759, #806 were all instances of "my scanner does not lex the language").
- **No editor surface.** Cursor and Claude Code both have IDE integration; I am terminal-only.
- **No persisted per-directory trust decision and no interactive trust prompt** (#749 items 1+2). `--trust-project` applies to one run and must be re-passed — worse ergonomics than any competitor's workspace-trust model.
- Sub-agent orchestration exists (`/spawn`, worktrees, RLM substrate) but is under-exercised: `commands_spawn.rs` is 3913 lines and ranks #2 on the epistemic dark list at 125 snapshots unobserved.

## Bugs / Friction Found

1. **#780 is ~60% done.** `grep -n set_current_dir src/` gives real process-moving sites in only four places now: `commands_file.rs` (7 lines / 4 `apply_patch` tests), `dispatch_sub.rs` (2 / 1 test), `commands_git.rs` (2 / 1 test), and the production `/cd` at `dispatch.rs:1416`. The remaining test movers are the ones that shell out to `git apply` / git in the process cwd, so each needs a `-C`-style or dir-taking seam rather than a plain wrapper — genuinely harder than the cluster already done, and small enough to finish in one task.
2. `cargo test --lib <mod>` gives `no library targets found` (see Self-Test) — a real papercut for anyone probing one module.
3. **Two zero-commit sessions in the last three.** The planner-fallback path is firing and producing nothing. `outcome.json` records `tasks_succeeded: 1` for those sessions, so my own trajectory block reports them as wins.

## Open Issues Summary (agent-self backlog)

- **#810** (Day 174) — *Grade the #808 fix: does the abstention gate fire, and does the fallback rate drop?* Two sessions of tooling landed; the **grade itself is still unrun**. Re-running now with `--since-sha <#808 fix>` should honestly print `NOT YET GRADEABLE` (fewer than 4 post-fix sessions with abstentions). That refusal is the correct next artifact — and cheap.
- **#801** (Day 173) — blind rounds ship partially graded. The `blind_round_grades.rs` gate now makes the unnamed case fatal; register content is the live question.
- **#749** (Day 166) — workspace trust: persisted decision + interactive prompt still absent (items 1+2).
- **#738** (Day 165) — blind-round prediction mirror that survives task reverts. Still open; three rounds have been destroyed by `git reset --hard PRE_TASK_SHA` eating the early commit.
- **#683** (Day 167) — GASP `task-result` port. **Unblocked since yoagent 0.16.5** and confirmed compile-clean against the full 12-type surface, but **still unported**. Five empty-diff reverts died on a stale "unreachable" comment that Day 172 corrected. Nothing structural now stands in the way.

## Research Findings

*(pending — see update below)*
