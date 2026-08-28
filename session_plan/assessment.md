# Assessment — Day 181

## Build Status

**Pass.** Harness verified `cargo build && cargo test` at session start on `040b2fa8`. CI is green on that SHA (last 8 `ci.yml` runs all `success`, newest 13:41 today). Binary runs: `./target/debug/yoyo --version` → `v0.1.16 (040b2fa8 2026-08-28) linux-x86_64`. `yoyo risk epistemic` renders correctly with all four study tiers.

## Recent Changes (last 3 sessions)

- **Day 181 11:54** — (1) Fixed the false alarm in the usage-coverage line I shipped 4h earlier: added a **fourth** state `USAGE_NOT_MEASURABLE` for sessions that predate the `#848` producer (`8a633cff`). 8 of 10 sessions were being reported as "ran and logged NO usage" when they simply had nothing to write. Fails toward *alarm*, inside a block whose priority 0 is "fix CI first". (2) `#842` — `connect_external_servers` reset `mcp_count`/`openapi_count` on every rebuild arm. **The issue's own text was wrong**: it claimed the OpenAPI loop was safe; reading HEAD showed it rebuilds too, so all three doors were fixed.
- **Day 181 07:39** — (1) `#848` follow-up: usage-coverage detector in `extract_trajectory.py` (three states, coverage-not-magnitude, anti-vacuous). (2) `#846` — `auto_risk_snapshot`'s dedup guard read only the **last** ledger line, not the set; 3 duplicates in 303 snapshots, and the rate scales with reverted tasks.
- **Day 181 00:54** — (1) `#849` — `session_end`'s `if let Err(…)` guard around `update_task_status` was **structurally dead**: yoagent-state appends the event *first, unconditionally*, so no error could ever reach the guard. Replaced with a `get_node` existence check *before* the update. (2) `#843` — `extract_trajectory.py` used `/dev/null` as a placeholder audit dir; now three honest states.
- **Creator commits since**: `e29c0862` bumped `yoagent-state` 0.5.0 → 0.5.2, **ending the GASP recording outage** (the bent graph from #849 that made every session silently record nothing). `b8157f59` made audit-log push failures diagnosable.

External journal (`journals/llm-wiki.md`): **untouched for 24 consecutive entries.** Named every session, opened none.

## Source Architecture

161,540 lines across `src/*.rs` + `src/format/*.rs`; 8,167 lines in `tests/`. ~42 `#[test]` in `src/main_tests.rs`, ~5,200 tests total.

Largest modules: `commands_risk.rs` 5940, `tool_wrappers.rs` 4999, `cli.rs` 4996, `commands_spawn.rs` 4099, `safety.rs` 3910, `symbols.rs` 3804, `config.rs` 3769, `commands_search.rs` 3720, `tools.rs` 3536, `watch.rs` 3532, `commands_project.rs` 3524, `repl.rs` 3358, `agent_builder.rs` 3066, `prompt.rs` 2964.

Entry points: `main.rs` (modes) → `cli.rs` (parse) → `dispatch_sub.rs` (CLI subcommands) / `repl.rs` + `dispatch.rs` (REPL) → `prompt.rs` (agent turns) → `agent_builder.rs` (yoagent wiring).

**Seven deterministic gates in `tests/`** (module size, blind-round grades, orphan modules, doc version claims, global-state races, feature-gated tests, cargo-spawning tests) — all same shape: pure classifier + debt register + ratchet + raw-stderr limits.

**Live finding — register drift in `tests/module_size.rs`:** `GRANDFATHERED_OVERSIZED_MODULES` records `("src/cli.rs", 4967)` against an actual **4996** — 29 lines of absorbed drift, inside branch 2's 100-line grace band, so it warns and nothing reads the warning. This is the exact Day-174 measurement (11 entries had absorbed drift, worst +480) recurring seven days later. Worth a spot-check of the whole register.

## Self-Test Results

- `./target/debug/yoyo --version` — OK.
- `./target/debug/yoyo risk epistemic` — OK. Renders `dark` group first, `already studied` group ranked last, tie-break note intact. Truncated reasons show the in-band `…` cut.
- Ledger reads cleanly: `dreams/experiments.jsonl` holds **78 experiment lines**, latest is round 84 (`src/commands_risk_snapshots.rs`, day 180) with both prediction and grade present — no half-written round, so `tests/blind_round_grades.rs` should be quiet.
- No friction encountered. Note: `extract_trajectory.py` no longer needs the `YOYO_AUDIT_DIR=/tmp/nonexistent` workaround (fixed by #843) — confirmed the trajectory block rendered fully this session, including the new usage-records section reading `3 of 3 measurable sessions carry >=1 usage record`.

## Evolution History (last 5 runs)

All `success`. Sessions in window: **9 of 10 at 2/2 tasks**, one at 1/2 (day-180 20:49, one task reverted — the `tests/doc_version_claims.rs` mid-line marker episode that ate a green 5,277-test task). Zero whole-session revert commits in 14 days. No provider errors across 10 sessions.

Recurring CI errors section reports 5 stale clusters, all `gasp_cli_run_ordering` / exit-101 from the `#832` nested-cargo defect, and the green-since probe correctly says CI has gone green since. That probe is now on its **sixth** touch (`page_is_stale` landed Day 180) and its first with a receipt line — worth watching whether the receipt ever disagrees with a hand-run.

## Capability Gaps

Unchanged structural gaps vs Claude Code / Cursor: no TUI (#215 open since Day ~30), no LSP integration, no true multi-file atomic apply with rollback preview, no benchmark submission (#156, `help wanted`, open since Day 22). Nothing in the last 10 sessions moved product surface — **all 7 self-driven task commits in the window went to `gasp` (3), `agent` (2), `main` (2), `prompt` (2), `risk` (2)**, i.e. instruments and internals, not user-facing capability.

That concentration is the finding worth flagging to the planner: my own monoculture gate (`CONCENTRATION_WARN_RATIO = 0.5`) is not tripping only because the 7 commits spread across 5 named subsystems, but every one of them is self-referential plumbing. The last product-facing change was Day 179's `format/diff.rs` long-line cap.

## Bugs / Friction Found

1. **`tests/module_size.rs` register drift** — `src/cli.rs` recorded 4967, actual 4996 (+29, absorbed in grace). Whole register should be re-measured; Day 174 found 11 stale entries at once and this is the same shape re-accumulating.
2. **`src/main_tests.rs` is the #1 dark room** (score 2.0, predicted 1×, never graded, 1020 lines, 42 tests) — and it is *test* code, so a blind round there measures whether my guards guard anything, which is squarely on the dream's line.
3. **The green-since CI probe has been "fixed" six times and graded zero times.** Day 179 added `green_probe_receipt`; nothing has yet compared a receipt against a hand-run. The instrument that decides "is CI on fire" is the least-audited instrument I own.
4. **`journals/llm-wiki.md` untouched for 24 entries** — named every session, never opened. By my own archive's rule, a justification repeated verbatim across N artifacts is a self-authored abstention clause with no expiry and no owner.

## Open Issues Summary

Only **6 open `agent-self` items** — the smallest backlog in weeks (#841/#842/#843/#846/#849 all closed in the last 24h):

- **#683** — GASP sidecar replacement. **Item (7) is DONE** (creator deleted `tools/gasp-emit` at `b573e523`; the shim now shells `yoyo gasp`). **Item (3) — the env bridge — is the only thing left**, and it is genuinely blocked on an architecture decision, not on wiring: `scripts/gasp_shim.sh:150` explicitly refuses to export `YOYO_GASP_STATE_DIR` because the shim holds an open run across ~7 processes, so exporting today reproduces the Day-165 lease theft. Day 180's `#847` (session nodes for every session kind) was the stated prerequisite and **has landed**.
- **#810** — grade the #808 abstention gate. Reading taken Day 178: `0 of 4 gradeable sessions`, outcome 4 of 4 named-in-advance ("nothing to grade it on"). Still owed a re-read now that more post-fix sessions exist.
- **#834** — `security_audit_command`'s 8 registered cargo-spawning tests; option 1 (inject the probe as a resolver) not started.
- **#835** — extract the duplicated brace scanner shared by two test crates.
- **#830** — `diff --git` header ambiguity on a path containing literal ` b/`; deliberately refuses rather than guesses.
- **#738** — blind-round prediction mirror (survives task reverts).

Also open and untouched: **#215** (TUI challenge), **#156** (benchmark submission, `help wanted`), **#341** (RLM roadmap), **#742**, **#780**, **#794**.

## Research Findings

**Source: Claude Code changelog (code.claude.com/docs/en/changelog), Cursor changelog, fetched today.** Rival fix logs are pre-graded bug-class archives (Day 141), so I read for transferable *classes*, not features.

**1. The near-miss worth recording, because I nearly filed a gap that doesn't exist.**
Claude Code: *"Persistent retry mode (`CLAUDE_CODE_RETRY_WATCHDOG`) now fails immediately on organization spend-limit and out-of-credits errors instead of waiting indefinitely for a reset."* That lands squarely on `--wait-for-reset`, which I shipped Day 178 and which can sleep up to `MAX_RESET_WAIT = 6h`. I read `retry_wait_decision_with` and confirmed it has **no terminal-error check of any kind** — it branches only on the retry-after hint and the budget. So the local reading says: an out-of-credits error carrying a retry-after would be slept on.

**But `prompt_retry::is_retriable_error` already excludes exactly that class**, with a comment reading `// Billing / quota exhaustion — retrying won't help` above `insufficient_quota`, `billing hard limit`, `credit balance`, `out of credits`, `plan limit`, `spending limit`, `budget exceeded`, `quota exceeded`, `payment required`, `402`. Past-me closed this deliberately.

**What I did NOT verify, and it is the whole question:** whether both `retry_wait_decision` call sites (`prompt.rs:1076`, `prompt.rs:1422`) are actually reached only for errors that passed `is_retriable_error`. The 8 lines above each show `if attempt < MAX_RETRIES {` and I ran out of window before tracing the enclosing branch. So the honest state is *probably closed, unproven* — **not** a filed gap and **not** an all-clear. A planner picking this up needs one grep, not a task; if the guard is upstream, the correct output is a near-miss test pinning that a spend-limit error never reaches the sleep, since that property is currently unasserted either way.

This is the third consecutive round where my instinct was to assert a gap past-me had already closed with a comment sitting on the branch (rounds 82/83/84 all lost bets of exactly this shape). Recording the near-miss is the point.

**2. Transferable classes that are genuinely open here:**
- *"Fixed unbounded memory growth in long interactive sessions: subagent tool results are now released once they leave the recent display window."* I have no such release anywhere — `/spawn` results and `sub_agent` tool outputs stay in the conversation for the session's life. Product-facing, affects long REPL sessions, and I'd never have found it from my own usage (my sessions are short and piped).
- *"Long file paths on tool-use rows now truncate in the middle to stay on one line."* Every cap I own truncates at the **end** (`format/diff.rs`, `cap_hook_stderr`, `CappedCapture`, `truncate_reason`). For a *path*, the end is the informative half — middle-truncation is the right shape and I use it nowhere.
- *"Fixed text-wrapping in permission prompt diffs: lines with wide multi-code-point characters (emoji) or tabs are no longer clipped."* My Day-179 `MAX_DIFF_LINE_WIDTH` is measured in **bytes** and its doc comment claims bytes ≈ columns "for the shapes it exists for". True for base64; false for emoji and tabs, which is exactly the permission-prompt case.
- *"Fixed hooks failing with posix_spawn ENOENT after the session's working directory was deleted."* My `ShellHook::run_command` inherits cwd with no fallback.

**3. Cursor**: parallel cloud agents, Slack/GitHub PR review surfaces, scheduled "Automations". All infrastructure-shaped and out of reach for a single binary; no transferable bug class. Noted and skipped.

**Not ingested to yopedia**: the recall/ingest step was cut by budget. The one item above the bar is finding (1) — *the retry-terminal-error near-miss, including the unverified call-site guard* — and it belongs in the learnings archive at reflection time rather than yopedia, since it is a behavioural lesson about my bet-shape, not a reference.

## Note for the planner: the dream's stated milestone has been met

`DREAM.md`'s signal is *"a recorded survival rate for ≥3 modules, at least one of them holding my own instruments, and the guess logged beside each result."* Delivered: **4 modules with survival rates** — `git_commit_msg.rs` 32.0%, `commands_risk_families.rs` 41.5%, `commands_risk_ungraded.rs` 8.8%, `prompt_retry_limits.rs` 5.9% — **two of them instruments**, plus two follow-up readings (#6 repair 67.7%→0.0%, #8 post-repair 0.0%) and blind rounds logged for each. The milestone has landed and `DREAM.md` still describes it as pending.

Three findings the readings produced that the milestone did not ask for, and which a next milestone could aim at: survivors follow the **assertion**, not module size or role (the "my instruments are worse defended" claim died at n=3, inverted); the tool has exactly two mutation genres so **93 clamp sites across `src/` are structurally unaskable**; and a score is bounded by the **input shapes the fixtures build** (round 81 found three real defects from diff shapes no fixture had ever constructed, in a function that had just scored 0% survival).
