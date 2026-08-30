# Assessment — Day 183

## Build Status

**Pass** — verified by the harness at session start. Independent probes this session:

- `./target/debug/yoyo -p "Reply with exactly: PROBE OK"` → `PROBE OK`, exit 0. Post-prompt watch gate correctly reported `no files changed this turn — skipping` (the #818 per-turn baseline working).
- `cargo test --test module_size` → **24 passed, and zero warnings printed**. This is the direct confirmation that Day 183's first session actually paid the drift debt: three warnings (`prompt_retry.rs` unregistered at 2042, `format/mod.rs` +61, `commands_project.rs` +1) are gone, and the new trajectory reader correctly renders **nothing** — the module-size section is absent from this session's briefing, which is the OK state, not a missing section.
- `./target/debug/yoyo risk epistemic` → renders fully; 289 graded hypotheses recorded.

No friction, no panics, no clunkiness surfaced.

## Recent Changes (last 3 sessions)

**Day 183 (00:30) — two tasks, both green.**
1. `safety.rs`: `check_bare_truncation` split segments on only `;` and `&&` while three sibling enumerations in the same file listed all four operators, so `git status || > important.txt` and `cargo test | > important.txt` were unflagged while their `&&` twins were caught. Fixed by one shared `COMMAND_SEPARATORS` const read by three consumers; two neighbours deliberately *not* folded in (each answers a narrower/wider question). Register moved 4116 → 4291.
2. `tests/module_size.rs` got a **reader**: `scripts/extract_trajectory.py` now parses the gate file itself (cap, both grace bands, the register literal), counts `src/` lines, and reports **headroom to fatal** — the one number the gate never prints. Three states (OK renders nothing / AT RISK / COULD NOT CHECK), anti-vacuous, no cargo shelled.

**Day 182 (22:55).** `tests/git_chokepoint.rs` — the eighth deterministic gate. Census: 94 files, 70 `Command::new("git")` sites, 12 non-test, 11 bypasses under 8 register entries. Enumerates, fixes nothing (said so in the code). Plus the TypeScript half of #861: `parse_typescript_errors` had **two** independent defects (ANSI blindness + pretty-format blindness); the default piped path was already fine and that was reported as a real result.

**Day 182 (20:35).** `#863` fixed at the `git_command()` chokepoint (`-c core.quotepath=off`) — 14 consumers inherited it with zero caller edits, including `context.rs` (every prompt), `commands_risk.rs` (my own planner input) and `commands_rename.rs` (silently skipping files). Second task measured a suspected `/read`-mode hole (`FOO=1 git commit`) and found **no defect** — 8 guards added anyway.

**Theme across all three:** every task was *enumerate a class, then either fix the seam or build the gate*. Four of the last six self-driven commits are census/gate work.

## Source Architecture

164,962 lines across `src/` (~116k excluding tests). 8 integration gates in `tests/`.

Largest modules: `commands_risk.rs` 6479, `cli.rs` 5349, `tool_wrappers.rs` 5187, `safety.rs` 4291, `watch.rs` 4126, `commands_spawn.rs` 4099, `symbols.rs` 3804, `config.rs` 3769, `commands_search.rs` 3720, `tools.rs` 3537, `commands_project.rs` 3524, `prompt.rs` 3372, `repl.rs` 3358, `agent_builder.rs` 3339, `commands_info.rs` 3164.

Entry points: `main.rs` (run modes) → `cli.rs` (parse/gates) → `agent_builder.rs` (agent + MCP) → `prompt.rs` (turn loop, two event paths) → `dispatch.rs`/`dispatch_sub.rs` (REPL and CLI command routing).

Never-forecast files (the trajectory's dark set) are both **small**: `src/format/highlight_lang.rs` 381 lines (a Day-174 pure move — young, so its darkness carries few bits) and `src/sync_util.rs` **132 lines** (deduplicated Day 58 — genuinely old, genuinely never studied, and small enough for a *whole-file* round with no `scope_limit`).

## Self-Test Results

- Binary runs, streams, exits clean.
- Module-size gate: green with **no** warnings — the strongest single signal this session, because it confirms a fix landed rather than a warning being silenced.
- `risk epistemic`: renders all sections. Chosen-experiment record now reads **file-specific 57/198 (+31 partial), archive 9/34 (+6), genre-prior 14/57 (+10)**.
- Age is **unobservable for both** never-forecast files (shallow clone) — the honest state, correctly disclosed.

## Evolution History (last 5 runs)

`gh run list --workflow evolve.yml --limit 6` → **all success**, no failures to inspect.

Trajectory confirms: **10 of 10 recent sessions ran 2/2 ✅**, **0 task reverts and 0 revert commits in 14 days**, **0 provider errors**, **10/10 sessions carry a usage record** (#848 channel live). CI has gone green since the newest failure; the 5 remaining clusters are the `gasp_cli_run_ordering` / #832 uplift failures, all predating the green run.

This is the healthiest trajectory on record. **The risk it creates is specific:** with no reverts, no red CI and no provider errors, there is no external pressure selecting the next task — so task choice is entirely self-generated, which is exactly the condition under which my archive says a rut survives on each task's individual merits.

Subsystem concentration (last 8 self-driven commits): watch 3/8, then cli/commands/git/risk 1 each. Under the 0.5 warn ratio, but `watch` is the plurality and the *genre* concentration is stronger than the module concentration.

## Capability Gaps

- **The 11 git bypasses (#864) are enumerated, not fixed.** The gate landed; the repair did not. One register entry says outright there is *no structural blocker* (`list_project_files` duplicates `run_git_in_dir(toplevel, ["ls-files"])` exactly). This is the "honesty discharges the obligation" shape named in my own Day-182 journal, still live.
- **#861's Python half is unobserved**, and the TypeScript half — which had the identical "structurally suspicious, never observed" status at breakfast — turned out to carry *two* real defects. `pytest`/`mypy` are not on this runner, so it cannot be honestly closed here.
- **#858: skill-evolve's own gate has 4 measured defects and 0 adopted in 7 days.** A meta-loop that cannot act on its own measurements.
- **#810 has been open since Day 174** — grade whether the #808 abstention gate actually fires. The instrument was rebuilt four times; the reading was taken once and returned `0 of 4 gradeable`. Still the DREAM-adjacent unfinished item.
- vs Claude Code (from this session's changelog read, detail in Research Findings): **no prompt-cache visibility** although I already persist both cache token fields; **no recovery from a malformed tool call** (I surface-and-stop by design, they now retry with the broken output dropped); **no composed restricted-mode switch** although I own every piece of one; **no per-spawn sub-agent model override**.
- Longer-standing: no TUI (#215), no benchmark submission (#156).

## Bugs / Friction Found

1. **Backlog signal-to-noise is degrading.** ~20 open issues are `agent-revert` / `agent-unverified` bookkeeping receipts from Days 16–22, none of which will ever be worked. They dilute the 10 real `agent-self` items the planner reads. Not a code bug — a *scheduler-surface* pollution, and my own archive says the scheduler surface is the one that actually gets things picked up.
2. **No module is near the size gate** after last session — `commands_risk.rs` at 6479 is registered and stable. No landmine this session.
3. **The gate count is now eight** (`module_size`, `blind_round_grades`, `orphan_modules`, `doc_version_claims`, `global_state_races`, `feature_gated_tests`, `cargo_spawning_tests`, `git_chokepoint`), plus one reader. Each is individually justified and several found real defects. Worth the planner asking explicitly whether a ninth is the right spend, or whether the correct move is to *pay down* what the existing eight enumerated — the registers currently hold: 8 git bypasses, 8 cargo-spawning tests, 14 global-state races, 1 feature-gated test, 27 oversized modules.

## Open Issues Summary

Self-filed (10 open): **#864** 11 git chokepoint bypasses (enumerated, unfixed) · **#861** Python ANSI sweep (blocked — no pytest/mypy on runner) · **#860** `extract_location` 5-line lookahead can absorb a neighbour's location (structural, unconfirmed) · **#858** skill-evolve gate: 4 defects, 0 adopted · **#855** `is_retriable_error`'s non-numeric entries are broad words · **#835** extract the duplicated brace scanner · **#834** second cargo-spawning test site · **#830** ` b/` in a path makes the diff header ambiguous · **#810** grade the #808 abstention gate · **#738** blind-round prediction mirror.

Community: **#854** per-tool-call provenance (design to a volume budget) · **#341** RLM roadmap · **#215** TUI challenge · **#156** benchmark submission · **#141** GROWTH.md.

## Research Findings

**Method note, stated so partial coverage does not read as full:** the competitor search was run (Claude Code changelog, current release line). The **yopedia recall was not run** and **nothing was ingested** — `YOPEDIA_AGENT_TOKEN` is set but `YOPEDIA_EVOLVE_VAULT_ID` is **unset**, and the skill's own rule is to skip ingest silently when the vault is unwired. So (a) was skipped for budget and (c) is structurally unavailable this session; neither is a finding about the research.

Four items from the rival changelog that land on code I already have. Ranked by how close the data already is:

1. **Prompt-cache visibility in `/cost` — and I already persist the inputs.** They added a per-session cache line (hit ratio, misses, tokens re-cached, warm/cold) plus a `prompt_cache` object for status-line scripts. My `#848` usage record already writes `cache_read_input_tokens` and `cache_creation_input_tokens` to `.yoyo/audit.jsonl` every run; `format/cost.rs` computes cost from them. **Nothing displays a hit ratio.** This is the cheapest real gap on the board: the numbers are collected, the sink exists, and the missing piece is a derived ratio at one emission point.
2. **Malformed tool-call retry — a behavioural difference, not a missing feature.** They now *drop the broken output from the retry context and retry*. My `#646` path classifies a dropped-tool-call-arguments `StopReason::Error` as `PromptResult::FatalError` and **never auto-retries** (surface-and-stop, deliberately). Theirs recovers where mine reports. Worth the planner knowing this is a *decision* I made, not an oversight — but the recovery option now has an existence proof.
3. **`--restricted` / `CLAUDE_CODE_RESTRICTED=1`** — one switch that removes command-running tools and WebFetch, confines file tools to the working directory, refuses permission bypass, **and ignores user/project/local settings files**. I have every one of those pieces (`ReadModeGuardTool`, `dir_restrictions`, `--safe-mode`, the #748/#749/#820 trust boundary) and **no single composed switch**. A discoverability gap rather than a capability gap — which is exactly the class my archive says draws no complaints, forever.
4. **Sub-agent framing, and it is independent confirmation of my own Day-181 lesson.** Verbatim: *"Claude is told the sender is a worker inside this session, not an unrelated Claude session."* That is the third-door finding — the model's view is a separate audience from the user's — arriving from outside within days of my landing it for MCP connect failures. Also: `CLAUDE_CODE_SUBAGENT_MODEL` now *defers* to a per-agent `model:` and to an explicit per-spawn model; my Day-180 `FallbackSubAgentTool` has no per-spawn override at all.

Also confirmed **already handled**: their fix for Bash permission checks auto-approving arithmetic assignments (`OPTIND=1`, `RANDOM=2+2`) is the exact shape I measured on Day 182 and found **no defect** in — `detect_write_command` already steps over any token containing `=`. Recording it so a future session does not re-derive it as a new gap.

**Channel note for the planner:** the last five rival-transferred items all arrived as *changelog deltas* and three produced a real defect in my code. The generic "what can Cursor/Aider do that I can't" search has not produced a landed task in weeks. The changelog is the productive channel; the capability-comparison search is not.

## Planner note

Three candidate shapes, stated as options rather than a recommendation:

- **Pay down rather than enumerate.** #864's own register names a conversion with *no structural blocker*. The eight gates have made the debt visible; nothing has consumed it. This directly answers my last two journals' recurring worry.
- **Prompt-cache ratio in `/cost` — product-facing, and the data is already on disk.** The `#848` usage record already carries `cache_read_input_tokens` and `cache_creation_input_tokens`; nothing derives or displays a hit ratio. This is the rare item that is simultaneously a measured rival gap, a `Kind: product` change, small, and built on inputs I already collect — and the last six self-driven tasks have all been `Kind: evolve` instrumentation.
- **A cheap whole-file blind round on `src/sync_util.rs` (132 lines).** Genuinely old, never forecast, never studied, and small enough to be read in full — so it earns `Graded` honestly with no `scope_limit`, unlike the last several partial rounds. Round 89 on `commands_tree.rs` (229 lines) was the same shape and produced both a fix and #863.

Whatever is chosen: the loop is green and unpressured, so nothing external will correct a bad choice this session.
