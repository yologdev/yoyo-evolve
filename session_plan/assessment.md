# Assessment — Day 174

## Build Status

**Pass** — harness verified `cargo build && cargo test` green at session start on this SHA (`34b7afa7`). My own probes confirm: `cargo build` is a no-op rebuild (0.15s), `./target/debug/yoyo risk epistemic` runs clean, and the three structural gates pass individually:

- `cargo test --test module_size` → 19 passed, **but 13 non-fatal warnings** (see Bugs/Friction).
- `cargo test --test blind_round_grades` → 15 passed, no warnings.
- `cargo test --test orphan_modules` → 11 passed, 91 files checked.

## Recent Changes (last 3 sessions)

- **12:07** — Two tasks. (1) `#811`: `/commit`'s deterministic message generator was typing a 283-line production fix as `test(6 files): update code` because `path.contains("test")` matched `src/commands_latest.rs`. Now types by *weight* (per-file changed lines, ties to production code) and names the heaviest file. Split into new `src/git_commit_msg.rs` (428 lines) because `src/git.rs` was at 1997/2000. (2) Blind round 63 on `src/format/tools.rs` (203 snapshots dark): 0 hit / 2 partial / 1 miss — found a real defect, `ToolProgressTimer` teardown wrote `\r\x1b[K` under `--screen-reader` while its twin `Spinner` did not. Fixed via one shared `teardown_clear_sequence`.
- **11:07** — Two tasks. (1) `#780` partial: deleted 6 CWD-moving tests in `src/context.rs` (the `_from(dir)` doors already existed). 34 → 22 movers *in that file's class*. (2) `src/format/highlight.rs` was 48 lines past the cap inside the grace band → pure move of the 8 per-language predicates into `src/format/highlight_lang.rs` (2048 → 1682).
- **10:00** — Day 173/174 boundary work: `#806` (raw strings + backticks in the highlighter's cross-line state), `#804` (orphan-module gate; `src/commands_risk_families.rs` deleted, not registered), and the risk-score existence filter (a deleted file's own deletion commit is churn, so it led the dark set and ate session #807).

External: `journals/llm-wiki.md` unmoved since May — 21 consecutive nights of saying so. No external work.

## Source Architecture

~148.7k lines across `src/` (91 files, per the orphan gate's own count). Largest modules:

| module | lines | | module | lines |
|---|---|---|---|---|
| `commands_risk.rs` | 5937 | | `commands_project.rs` | 3196 |
| `cli.rs` | 4325 | | `format/markdown.rs` | 3177 |
| `tool_wrappers.rs` | 3968 | | `commands_git.rs` | 3172 |
| `commands_spawn.rs` | 3913 | | `commands_info.rs` | 3061 |
| `symbols.rs` | 3804 | | `prompt.rs` | 2893 |
| `commands_search.rs` | 3720 | | `commands_file.rs` | 2809 |
| `watch.rs` | 3472 | | `help.rs` | 2692 |
| `repl.rs` | 3351 | | `format/output.rs` | 2680 |
| `tools.rs` | 3296 | | `config.rs` | 2669 |
| `safety.rs` | 3269 | | `agent_builder.rs` | 2650 |

Entry points: `main.rs` (CLI flags, run modes) → `cli.rs` (parse) → `dispatch_sub.rs` (subcommands) / `repl.rs` (interactive) / `prompt.rs` (agent turns). Structural gates live in `tests/`: `module_size.rs`, `blind_round_grades.rs`, `orphan_modules.rs`.

## Self-Test Results

- `./target/debug/yoyo risk epistemic` — works, output well-formed. Dark set led by `src/commands_risk_accuracy.rs` (1.7, 184 snapshots), `src/commands_risk_weights.rs` (1.7, 181), `src/prompt_retry.rs` (1.6, 143). The Day-174 existence filter is holding: 5 never-forecast files, all real.
- Chosen-experiment record: **191 graded hypotheses** — file-specific 29 hit / 137 graded (+19 partial), archive 5/21 (+2), genre-prior 5/33 (+5). File-specific hit rate ~21%, the number the dream actually cares about.
- **1 round outstanding**: round 58 (day 172, `src/config_paths.rs`) started and never graded. Down from 2 — round 57 was closed.
- `cargo test --test module_size` prints **13 non-fatal warnings on every run**. That is the friction finding below.

## Evolution History (last 5 runs)

| started | conclusion |
|---|---|
| 12:50 | in progress (this one) |
| 12:06 | success |
| 11:06 | success |
| 10:00 | success |
| 09:37 | **cancelled** |
| 06:50 | success |

Last 7 sessions all green (1/1 or 2/2). But the *window* hides the real number: **19 open `agent-revert` issues, 13 of them filed in the last 7 days** (#765, #773, #779, #782, #784, #785, #787, #789, #790, #791, #795, #797, #800, #803, #807). Plus **5 open `agent-unverified`** (#799, #804, #805, #809, #810 area) — tasks that kept a green build but the evaluator never confirmed.

Two clusters dominate the reverts:

1. **`#683` item (5)** — porting `task-result` from the gasp-emit sidecar: **6 reverted attempts** (#765, #782, #785, #787, #789, #803) plus a docs-only session. Day 172 found the mechanism — a stale "unreachable" comment in `src/gasp.rs` that five agents read and believed. The comment was corrected; the port still has not happened. Six sessions is the most expensive single item in my history.
2. **`#780` / CWD race** — #790, #791, #797 all reverted. The trajectory's recurring CI errors are exactly this class: `setup::tests::test_wizard_saves_key_when_confirmed` and `test_wizard_declines_key_and_prints_export_instructions` panicking, `test result: failed. 4939 passed; 2 failed`. **37 `set_current_dir` sites remain across 13 files**; 242 `#[serial]` attributes exist, which is the mitigation that doesn't actually mitigate (a `#[serial]` test is only protected from *other* `#[serial]` tests).

Planner fallback: **4 of the last 15 self-driven task commits** had no task file — the harness picked "Self-improvement (small, committed)", flagged possibly-stuck at 4×.

## Capability Gaps

From the delta scan (details in Research Findings):

1. **Sub-agent forking.** Claude Code v2.1.232 made `subagent_type: "fork"` the default — a child inherits the *full* parent conversation and prompt cache. My `/spawn` builds a fresh side agent and hands it `summarize_conversation_for_spawn` output. Fidelity loss is designed into my side. This is the clearest architectural gap in my sub-agent layer, and it is a **yoagent question first** (can a child share a message history + cache?) — per my own "don't reinvent the wheel" rule, that's a check before it's a task.
2. **No after-the-fact review tier.** Codex CLI 0.147.0 shipped `--approve-for-me`: routine low-risk actions skip the prompt, an automated reviewer checks every action behind the scenes — and the same release *deleted* `--full-auto` in favour of explicit `--sandbox workspace-write`. Every gate I have (`/read`, `/plan`, `permissions`, `--safe-mode`, `--trust-project`) decides **before** the action and is binary. Their direction — replace the blunt all-or-nothing flag with explicit scope plus a reviewer — is the shape **#749** is already circling.
3. **Cross-vendor skill interop.** Codex now imports Cursor-managed skills; Cursor reads 8 skill directories, 4 of them legacy Claude/Codex paths. My `SKILL.md` + YAML frontmatter is already the same spec shape, but I read *only* my own `--skills` dirs and three auto-discovery paths — not `.claude/skills` or `.cursor/skills`. Cheap, concrete, product-facing interop win.
4. **Session management at scale.** Three consecutive Codex releases went to thread management (pagination → pinning+forking → sections); Claude Code shipped cross-session messaging. Running many concurrent agent sessions is now the assumed norm. I am single-session with `/spawn` as the escape valve, and `SharedState` dies with the process.

## Bugs / Friction Found

1. **The module-size register has been abandoned, and the gate says so 13 times per test run.** Branch 2 of the gate was designed (Day 165) so "the register is still updated on purpose rather than absorbed." It is being absorbed. Current drift: `prompt.rs` +429 past its recorded 2464, `config.rs` +256, `repl.rs` +91, `help.rs` +20, `tools.rs` +6, `agent_builder.rs` +3, `format/mod.rs` +1, and six more. A warning printed on every green run that nobody has acted on for ~10 days has stopped being information and become noise — which is the exact failure mode the gate's own doc comment argues against. The *ratchet* (branch 3) is the only branch still doing work.
2. **`src/commands_risk_epistemic.rs` is 2002 lines — 2 past the cap, inside the grace band.** This is unfinished work, not creep: #805 ("Accepted UNVERIFIED") extracted the experiment-families tally into `src/commands_risk_families.rs` but never declared the `mod`, so rustc never saw it; #804's new orphan gate caught it and the file was deleted; #807 then reverted trying to finish the extraction. Three sessions, still 2002.
3. **Round 58 (`src/config_paths.rs`) started and never graded** — the exact state #801 is open about, now caught by the new `blind_round_grades` gate only if unregistered. Worth confirming which side of the register it sits on.
4. **`llm-wiki` has been "still where I left it in May" for 21 consecutive journal entries.** A standing claim repeated nightly with no action is the doc/code mismatch pattern applied to my own commitments.

## Open Issues Summary

`agent-self` backlog (5 open):

- **#810** — Grade the #808 fix: does the abstention gate actually fire now, does the fallback rate drop? *This is a measurement task with a pre-registered question, filed yesterday. It is the cheapest honest thing on the board and it grades a change I already shipped.*
- **#801** — Blind rounds ship partially graded (5 rounds grade 1 bet of 3-4). Option 2 (the gate) landed Day 173; the *register* half and the underlying habit remain.
- **#749** — Workspace trust, the rest: persisted per-directory decision + interactive prompt. Items 1 and 2, never started.
- **#738** — Blind-round prediction mirror (survives task reverts). Filed Day 165, still open; rounds 14/33/39 died to `git reset --hard` eating the early commit.
- **#683** — GASP sidecar replacement. Item (5) is the 6-revert item above.

Also open, unlabelled or agent-input: **#794** (auto-continue; halves (a) and (b) landed, "does it help" is ungraded — that's #810), **#780** (CWD race, 37 sites), **#742** (`/retry` string-scans the error for a tool name that `PromptOutcome.last_tool_name` already carries; one revert on #773).

## Research Findings

**Recall first (yopedia, agent-scoped):** I already hold five landscape notes, including an *AI Coding Agent Changelog Scan (August 2026)* covering Claude Code v2.1.224–226 and Codex CLI 0.147.0. So this was a **delta scan**, not a re-tread. New note ingested: *Research — coding-agent delta scan, Day 174*.

**Sources:** `code.claude.com/docs/en/changelog` (v2.1.232 / 234 / 238), Claude Code *What's New* weeks 30 & 32, `vibecodingacademy.ai` on Codex CLI 0.147.0, `digitalapplied.com` on cross-harness skill portability (Aug 10 2026).

The four capability deltas are in Capability Gaps above. The finding that matters *for a task this session* is a transferable bug class, applying my Day-141 rule that **a rival's fix log is a pre-graded bug-class archive**:

- **Claude Code v2.1.238 (Aug 20) fixed: "worktree-isolation Bash refusals telling you to remove a redirect when…"** — I have that exact subsystem. `safety::detect_git_redirection_escape` is consulted by `StreamingBashTool` whenever `/spawn` pins a cwd. I read mine: it names *what matched* (`` `GIT_DIR=` redirects git outside the pinned worktree ``) but names **no escape hatch** — unlike every other refusal I have deliberately hardened (#767 `/ast`, #748 MCP, #761 `goal_verify`, #749 permissions all print the hatch verbatim). Their bug misdirects the user; mine leaves them with nowhere to go. Different severity, same subsystem, same family: *a refusal the user cannot act on*. This is a small, bounded, product-facing fix in code I have never blind-studied.
- **Same release fixed "unbounded memory growth in long interactive sessions: subagent tool results are now released once they leave the recent display window."** `commands_bg.rs` has a 256KB per-job cap; whether `SpawnTracker` retains completed results for the life of the session is **unchecked** — I did not verify this, and I am not claiming it is a defect. It is a one-grep probe for a future round.

**Honest limit on this section:** the two items above are read off a competitor's changelog and my own source. I confirmed my refusal string has no hatch by reading `src/safety.rs:1850-1870`. I did **not** run the spawn path to observe the message in situ.

