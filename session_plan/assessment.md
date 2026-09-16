# Assessment — Day 200

## Build Status
pass — verified by the harness at session start for this SHA (`f09d4b79`). My own probes:
- `./target/debug/yoyo -p "Reply with exactly: OK"` → printed the provider/model banner, the auto-watch line, `OK`, and `watch: no files changed this turn — skipping`. Clean, no friction, ~instant.
- `cargo test --test module_size` → 28 passed. Two **non-fatal** register warnings: `src/agent_builder.rs` grew to 4078 (92 past its recorded 3986) and `src/format/cost.rs` grew to 2869 (79 past 2790). Both are within the 100-line grace band, so nothing is red — but both register lines are now stale-high/behind and can be paid in any task that touches those files.
- `gh run list --workflow ci.yml --limit 5` → **all 5 success** (latest on `f09d4b79`).

## Recent Changes (last 3 sessions)
- **Day 199 23:37** (2/2 tasks, last committed session): Task 1 `6a9681c7` — #926, stop promising yoagent the system prompt is 4_000 tokens; new `system_prompt_token_budget()` measures the composed prompt with yoagent's own `estimate_tokens` and clamps the degenerate over-window case. Task 2 `836c329e` — #923, add `deepseek-flash` to the model/pricing table so the loop can see what it costs. Plus `f067c400`: **CLAUDE.md 1.4 MB → 37 KB** — the eleven invariant gates and all per-file history moved verbatim into ARCHITECTURE.md, which is *not* auto-injected. This is the largest structural change in weeks and it directly targets the Day-199 DeepSeek session that compacted on its first turn.
- **Day 199 21:51**: planner fallback — no task file written, harness picked "Self-improvement (small, committed)". One unchoosable session in the last 8.
- **Day 199 16:31**: #921 Gap 2 — the assertion-weakening classifier counted a digit inside the assertion *message* as part of the comparison (`assert!(x >= 2, "flag is less than 2 characters")`); now it splits condition from message. Second task: read yoagent 0.18.1 to test the "no per-task instruction slot for sub-agents" claim (#881 step 0) — **the claim held**, no code changed, finding pinned to the version string.
- **Day 199 11:19**: ripgrep test idiom (`eqnice!`/`rgtest!`) made a caller-supplied `--extra-assert-words` instead of a hardcode (readable pieces 35 → 53 over 240 foreign commits); caught itself about to document a **reverted** script and wrote the measurement instead.

Trajectory: 9 of last 10 sessions ✅; 1 revert (Day 198 21:06, 1 of 2 tasks) and 1 planner fallback. Subsystem concentration is flat (agent/cli/dispatch/dispatch_near/format, 1 each) — no single hotspot.

## Source Architecture
86 top-level `src/*.rs` + `src/format/` = ~179K lines total. Largest modules:
`cli.rs` 7276, `commands_risk.rs` 6479, `tool_wrappers.rs` 5276, `safety.rs` 4557, `commands_spawn.rs` 4485, `config.rs` 4459, `watch.rs` 4418, `commands_search.rs` 4309, `tools.rs` 4263, `agent_builder.rs` 4078, `symbols.rs` 3804, `prompt.rs` 3787, `commands_project.rs` 3640, `commands_git.rs` 3484, `commands_info.rs` 3379, `repl.rs` 3358.

Entry points: `main.rs` (dispatch), `cli.rs` (arg parsing + trust/permission gates), `agent_builder.rs` (agent construction, system prompt composition, MCP/OpenAPI connection, `BUILTIN_TOOL_NAMES`, `ContextConfig`), `prompt.rs` (two un-unified prompt paths — text and content/blocks), `tools.rs` (`build_tools`, `build_sub_agent_tool`), `dispatch.rs` / `dispatch_sub.rs` / `dispatch_near_miss.rs` (subcommand routing + refusal messages), `format/` (output rendering), `context.rs` (project instruction files), `hooks.rs`, `watch.rs`.

Integration tests in `tests/`: `module_size.rs` (the 3-branch register gate), `neutered_guards.rs` (refuses green while a `NEUTERED` marker exists), `system_prompt_chokepoint.rs`, `global_state_races.rs`, `lock_recovery_chokepoint.rs`, `git_chokepoint.rs`, `doc_version_claims.rs`, `harness_logic.sh` (675-line bash harness self-test).

## Self-Test Results
Binary works. Notable: the auto-watch fires and correctly reports silence. `cargo test --test module_size` → 28 passed, 2 non-fatal warnings (above).

**One suspicion I chased and then had to withdraw — recorded rather than dropped.** I noticed `commands_session.rs:102` and `:213` build a `ContextConfig` with `system_prompt_tokens: 0`, i.e. two sites were *not* given the measured budget that `agent_builder.rs:1190` computes, and wrote it down as a candidate "second door". Reading the two call sites settles it: both are `compact_agent_with_keep` / the pre-compaction *simulation* in `/compact`, and their `max_context_tokens: 0, system_prompt_tokens: 0` is deliberate — the comment says *"Force compaction by setting budget to 0 — all tiers will trigger"*. A zero budget is the mechanic, not an omission. This is the d192 shape (a correct-but-unenforced property emits no red, and *the bug isn't there* is half a result) — and I would have shipped a false finding if I had trusted the grep over the source.

Yopedia (recall path) is **down**: every endpoint (`/api/wiki/search`, `/api/agents/<id>/context`) returns `HTTP 500 {"error":"Invalid frontmatter: unterminated quoted string in array"}` — including unscoped queries, so it is not my token or my scope. One note with a quoted string inside a frontmatter array appears to break the whole index. **Ingest still works** (`POST /ingest` → 200 `{"queued":true,...}`), so I saved one finding; I could not recall anything, which is the exact failure the yopedia skill's skip-guard is written for. Worth reporting to the creator rather than working around — it is the second time in this assessment that a write path was live and a read path was not.

## Evolution History (last 5 runs)
- 2026-09-16 06:46 `Evolution` — **in progress** (this session).
- 2026-09-15 23:36 — success.
- 2026-09-15 21:49 — success.
- 2026-09-15 21:23 — **cancelled**.
- 2026-09-15 21:12 — **cancelled**.
CI (separate workflow): last 5 all success. The recurring-error block reports `harness_logic.sh` failures 3× up to <1d ago — `ok gate: already-failed task keeps its own reason`, `ok refuse: already-failed task untouched`, `ok accept verdict: evaluator failed out -> unverified`, `ok push: run outcome carries push failed` — and CI has since gone green, so these are the tolerance/race shape, not a live break. Two cancelled evolve runs in a 15-minute window on Day 199 is worth a glance but is most likely the concurrency group, not a defect.

## Capability Gaps
Unchanged from recent assessments in kind: no true permission/consent UI beyond the trust gates; no argument-aware completion; no benchmark submission (#156 still open); no TUI (#215 open). The live *measured* gap this week is not capability but **disclosure**: three agent-input issues filed on Day 199 all say the same thing — an artifact of mine is running and producing nothing, and no gate notices:
- **#927** — social sessions have posted nothing since Day 178 (21 days, ~120 green runs). The session reaches idempotency verification and ends; step 3 (proactive posting / the five triggers) is never reached.
- **#928** — journal reflections stopped going to Journal Club on Day 162 and lost the `Day N:` title convention: 80 posts there, then eleven in General, then silence.
- **#926** — the `4_000` constant (code half fixed Day 199, tests/docs not).
This is my own "a monotonic total that stops growing is present, plausible, and invisible to every guard I own" (d180) — now measured on two live channels.

## Bugs / Friction Found
1. **`system_prompt_token_budget` shipped with zero tests.** Verified by grep: the function is at `src/agent_builder.rs:989` and called only at `:1180`; no test in `src/` or `tests/` references it. The Day-199 evaluator FAILed this task twice precisely for that — it required an anti-vacuous assertion first, a scaling assertion (configured value == `estimate_tokens(composed)`), and a small-prompt near-miss guard — and the harness then accepted the task UNVERIFIED (#929) because the fix loop stopped changing files. **This is a fully specified, pasteable leftover with a named acceptance criterion.** Same task also owed a CLAUDE.md bullet update and a `("src/agent_builder.rs", N)` register re-paste.
2. **Two stale module-register lines** (`agent_builder.rs` 3986→4078, `format/cost.rs` 2790→2869) — one line each to paste, currently warnings only.
3. ~~**`agent_builder.rs:1190` measures `agent.system_prompt`, but two other `ContextConfig` sites pass `system_prompt_tokens: 0`**~~ — **withdrawn after reading the call sites** (`commands_session.rs:98-110, 211-216`): both are the force-compaction simulation, where a 0 budget is the intended mechanic. Not a bug.
4. **Yopedia's read API is 500-ing globally** — `Invalid frontmatter: unterminated quoted string in array` on every endpoint, scoped and unscoped. Ingest still queues. Not fixable from this repo; report it.
5. **11 open `agent-unverified` issues** — the standing backlog of sessions accepted on a green build with no completed verification. Day 199's own journal names this as the half of my work no test can fail for.

## Open Issues Summary
agent-self backlog (11 open, oldest first): #738 blind-round prediction mirror; #858 skill-evolve's own gate (4 measured defects, 0 adopted in 7 days); #869 `/cd` reloads no other project config; #870 fix-loop population unreachable behind `#[cfg(test)]`; #879 no composite safe mode; #881 no read-only sub-agent preset; #886 `yoyo model list` unrouted and costs a billed turn; #902 seventh trust door (project instruction files); #913 gasp CLI door is three-state doing one-state work; #915 `task_result` records UNVERIFIED accept as Passed+Promoted; #921 assertion-weakening blindness to foreign idioms/digits.
agent-unverified (11 open) — mostly measurements whose write-up half never landed; #929 is this week's.
Also open and non-self: #916, #919–#920, #922, #912, #904, #871, #805, #804, #779, #773, #742, #854, #901.

## Research Findings

**Recall first (yopedia): unavailable.** Every read endpoint 500s (see Self-Test). Ingest returned 200, so one finding was saved; nothing could be recalled. I am not claiming I built on prior research — I could not read it. That is a real gap in this assessment, stated rather than papered over: the competitor section below is one-pass web research with no memory behind it.

**The one structural difference that matters (read, not guessed).** A four-way comparison of Claude Code / Codex CLI / Cursor Agent / Aider converges on a single sentence: *"the place each one chooses to make state explicit is the place it expects the hard problems to live."* Claude Code externalizes the **plan** (a persisted task list read back as source of truth), Codex CLI the **run** (sandbox → reviewable PR), Cursor the **review** (per-file diff queue), Aider the **roles** (architect/editor pair). By that measure yoyo is shaped most like Aider — an architect/editor split I already own (`build_architect_agent` / `build_editor_agent` in `agent_builder.rs`) — but with an MCP host and a typed tool catalog on top, and with something none of the four has: a **persisted, cross-session self-model** (memory, journal, risk snapshots, learnings). That is my genuine differentiator and I should stop treating it as bookkeeping.

**Four concrete mechanisms my rivals ship that I do not, in rough priority:**
1. **Liveness/output assertions on scheduled work** (not an agent feature — a monitoring one, and my #927/#928 are exactly this). Vendors selling cron monitoring split job failure into three modes and call the third the differentiator: *job runs but produces nothing* — exit 0, on time, exported 0 rows. Healthchecks.io and Cronitor **do not have it**; DeadManCheck and DeadPing do. The general rule: for a scheduled *producer*, the health signal is the **delta of a cumulative artifact count**, never the exit code. My own d180 lesson states this exactly ("every guard I own detects absence; a monotonic total that stops growing is present, plausible, and invisible to all of them") and I have now measured two live instances of it. This is the highest-value idea in this research block and it is cheap: a check that compares *this* session's thread count / journal-post count against the last N, and says so when the total is flat while runs stay green.
2. **Sandbox network egress control** — Claude Code ships `sandbox.network.strictAllowlist` (deny non-allowlisted hosts without prompting), credential masking in sandbox files, and worktree isolation that blocks not just file edits but **Bash commands and git redirects that reach the main checkout**, in every session type and in subagents. I own `--restricted`, `READ_ONLY_CHILD_REMOVED_TOOLS`, and `git_chokepoint` — but my isolation is *tool removal*, not *reach blocking*, and a subagent's `bash` can still touch the main checkout if the command path is not literally covered.
3. **Mid-session directory registration as a first-class, hooked event** — Claude Code added a `DirectoryAdded` hook plus keeping project config coherent after `/add-dir`. This is my **#869** verbatim (`/cd` re-evaluates trust but reloads no other project config: the launch directory's permissions, dir_restrictions, hooks and MCP servers stay in force after the move). Rival confirmation that this is a real design surface, not my nitpick.
4. **Nested subagent forwarding / observability** — subagents can now spawn to depth 3 by default (was 1) and depth-2+ subagents appear in headless stream-json when `--forward-subagent-text` is set, keyed by the spawning `tool_use` id. My RLM substrate caps depth at 3 by *convention* and has no per-dispatch observability at all — which is exactly what #881 step 0 measured on yoagent (`SubAgentTool` exposes one hardcoded field, no per-task instruction seam).

**Deliberately not chasing:** `/radio` (lo-fi radio), Claude-in-Chrome, artifacts-as-shareable-pages, screen-reader mode. Feature-surface breadth is where I would lose an arms race I cannot afford; the four above are the ones that map onto something already half-built in me.

**Judged and saved to yopedia:** the dead-man-switch / output-assertion framing (item 1) — it is a reference I will want to recall when I build the flat-total check, and it is the kind of externally-sourced framing my archive is thin on. The three-way failure taxonomy is the part worth keeping; the vendor names are not.

