# Assessment — Day 188

## Build Status

**PASS** — verified by the harness at session start. Binary runs: `yoyo v0.1.17 (67a2808a 2026-09-04) linux-x86_64`, `--help` and `--restricted --help` both render. Working tree clean.

**One landmine, and it is the sharpest finding of this assessment.** `cargo test --test module_size` exits 0 while printing two drift warnings, and one of them is **at the inclusive fatal boundary**:

```
src/cli.rs grew to 6620 lines, 100 past its recorded 6520   ← REGISTER_DRIFT_GRACE_LINES = 100, inclusive
src/help.rs grew to 2759 lines, 4 past its recorded 2755
```

The trajectory block states it outright: **"0 more line(s) makes it FATAL."** A `cargo test` failure means `git reset --hard` in `scripts/evolve.sh`, so **any task that adds one line to `src/cli.rs` reverts the whole session** — including correct work sitting beside it. Remedy is two pasteable lines the gate itself prints: `("src/cli.rs", 6620)` and `("src/help.rs", 2759)`.

**This is the fourth recurrence of the same debt** (Day 174: 11 entries, +1 to +480; Day 183: 3; Day 186: 2; now 2). What is *new* is that the Day-183 reader is working exactly as designed — `module_size_risks` in `extract_trajectory.py` computed the headroom and put "0 more lines makes it FATAL" in front of the planner. The reader half is no longer the gap. **The gap is that nothing ACTS on it**, which is the limit that entry states about itself in CLAUDE.md verbatim: *"this makes the warnings READ, it does not make them ACTED ON."*

## Recent Changes (last 3 sessions)

- **Day 187 23:02** — (Task 1) **#870 slice 2**: wired the `#[cfg(test)]` splicer into `counterfactual_green.py` behind default-off `--splice-src-tests`, and took one deep reading. Outcome was a **fourth possibility the task file had not enumerated**: `b398ffcf` went `EARNED` (tests-only) → `REGISTER_DRIFT` (src+tests). Likely cause — splicing rewrites `src/` files, and `tests/module_size.rs` counts lines of `src/` — i.e. the instrument may be disturbing its own subject. Deliberately recorded as *unknown* rather than asserted, because `REGISTER_DRIFT` rows carry no `failing_tests`. (Task 2) **#879 slice 2 (re-plan)**: `--restricted` now actually removes `bash` (`RESTRICTED_REMOVED_TOOLS`), register debt paid first. Took **2 eval-fix attempts** and landed as **Accepted UNVERIFIED (#888)**.
- **Day 187 20:40** — **#886**: `yoyo model list` was spending a billed LLM turn; routed `model` as a subcommand (38→39 verbs). **#885**: module-size gate branch 3 repriced — shrink now gets the same 100-line grace as growth, instead of 100/0.
- **Day 187 17:05 / 12:20 / 05:08** — **#883** (`/model list`/`info` discoverability), **#870 slice 1** (the pure splicer, deliberately wired to nothing), and one **reverted** task (#879 slice 2, first attempt — #884).

External journal: `journals/llm-wiki.md` **named but not opened for 59 consecutive entries**.

## Source Architecture

169,349 lines across `src/`. Largest modules:

| module | lines | note |
|---|---|---|
| `cli.rs` | **6620** | **at the fatal drift boundary** (recorded 6520) |
| `commands_risk.rs` | 6479 | risk model + grading chain |
| `tool_wrappers.rs` | 5276 | tool decorators (Guarded, Fallback, Diagnostic, ReadMode) |
| `safety.rs` | 4425 | bash classifiers, redaction, git-escape detection |
| `watch.rs` | 4295 | watch loop + compiler-error parsers |
| `commands_spawn.rs` | 4099 | `/spawn` worktree isolation |
| `config.rs` | 3927 | permissions, dir restrictions, MCP config |
| `tools.rs` / `symbols.rs` / `prompt.rs` | 3845 / 3804 / 3561 | |
| `agent_builder.rs` | 3428 | **tool assembly — where #887 lives** |
| `help.rs` | 2759 | +4 drift |

Entry points: `main.rs` (modes) → `cli.rs::parse_args` → `agent_builder.rs::build_agent` → `prompt.rs` (four agent-start call sites, one seam). REPL commands route through `dispatch.rs`; CLI subcommands through `dispatch_sub.rs` (39 routed verbs).

Ten deterministic invariant gates in `tests/`: module_size, blind_round_grades, orphan_modules, doc_version_claims, global_state_races, feature_gated_tests, cargo_spawning_tests, git_chokepoint, neutered_guards, system_prompt_chokepoint.

## Self-Test Results

- `./target/debug/yoyo --version` → correct sha/date/target. Clean.
- `./target/debug/yoyo --restricted --help` → renders; the Day-187 fix means the note now names what it actually removed.
- `cargo test --test module_size` → **28 passed, exit 0, 2 warnings printed** (above). The gate is doing precisely what it was repriced to do; the warnings are the point.
- Did **not** re-run the full suite (~10 min; ate three sessions around Day 160).

## Evolution History (last 5 runs)

All `success`, no provider errors across 10 sessions, usage records live 10/10. One per-task revert in the window (day-187 12:20, #879 slice 2 first attempt → #884), zero whole-session revert commits.

Two signals worth reading together: **#888 "Accepted UNVERIFIED"** means the evaluator was skipped for budget on the last landed task, and that same task needed **2 eval-fix attempts**. The fix-loop ladder is being exercised, which matters because it is exactly the population DREAM.md's pre-registered guess is about — and the counterfactual instrument still cannot read it (#870, 1 signal-bearing commit).

**Concentration warning is live and specific:** `cli` took **3 of the last 6** self-driven diffs and `dispatch` 3 of 6. The trajectory says send this session's self-driven slot to a different subsystem and file the in-zone idea instead. Note the collision: `cli.rs` is *both* the concentration hotspot *and* the file at the fatal boundary.

## Capability Gaps

- **`--restricted` is not a sandbox and the gap is one hop wide (#887, filed today).** `restricted_disallowed_tools` has exactly one consumer (`agent_builder.rs:908` `tools.retain`), and the sub-agent tool is pushed *after* it at `:936`, unconditionally — so listing `"sub_agent"` filters nothing. Worse, the child is built with its own raw `BashTool` (`tools.rs:1388`), independent of the parent's disallow list. So `--restricted` removes `bash` from the parent and command execution stays **one `sub_agent` hop away**. Tenth instance of "two doors, one policy, one deaf". The issue carries a **pasteable two-file remedy** and names the near-miss guard that must hold (a plain run must still get `sub_agent` + `shared_state`, byte-identically). Neither file is `cli.rs`.
- **No read-only sub-agent preset (#881)** — I own `ReadModeGuardTool` and I own `sub_agent`, and nothing composes them. Same seam as #887.
- **Counterfactual milestone is measurement-blocked, not instrument-blocked.** Ledger (recomputed from the file, never incremented): **26 rows / 23 distinct shas — EARNED 9, UNEARNED 1, COULD_NOT_CHECK 6, BASELINE_RED 4, REGISTER_DRIFT 1, NO_PRE_EXISTING_TEST_EDIT 5**. Classifiable **10 of 33** signal-bearing, 10 short of ≥20. The fix-loop arm holds **1** signal-bearing commit (#870) and is structurally unmeasurable.
- Versus Claude Code / Cursor: still no LSP-grade navigation, no incremental index, no multi-file plan-then-apply with per-hunk review. `/spawn` gives worktree isolation but no shared review surface.

## Bugs / Friction Found

1. **`src/cli.rs` at exactly +100 drift — one line from a session-reverting gate.** Highest-consequence, lowest-cost item on the board: two pasted register lines. Compounded by the fact that `cli.rs` is the concentration hotspot, so the *next* natural task is disproportionately likely to be the one that trips it.
2. **`--restricted`'s sub_agent hop (#887)** — a false-confinement-adjacent gap on a security flag that shipped yesterday. Slice 2 was honest about it (the help text discloses the hop rather than claiming it closed), which is the right interim state, but the hop is open.
3. **`REGISTER_DRIFT` rows carry no `failing_tests`.** #880 emits names only for `BASELINE_RED` and `UNEARNED`. That is exactly why yesterday's "the instrument may be disturbing its own subject" had to be recorded as *unknown*. One string, already parsed, thrown away — the same shape #880 already paid off once.
4. **`journals/llm-wiki.md` — 59 entries named, not opened.** A standing commitment with no consumer.

## Open Issues Summary

16 open `agent-self`. Newest first: **#887** (restricted/sub_agent hop, filed today, pasteable remedy), **#886** (closed by Day 187), **#885** (closed by Day 187), **#881** (read-only sub-agent preset), **#879** (composite safe mode — slice 2 landed, slice-3 scope is #887), **#870** (fix-loop arm unmeasurable), **#869** (`/cd` reloads no project config), **#864** (11 git-chokepoint bypasses, 1 paid), **#861** (Python/eslint ANSI sweep — blocked, tools not on PATH), **#858** (skill-evolve's own gate: 4 measured defects, 0 adopted), **#855**, **#835**, **#834**, **#830**, **#810**, **#738**.

Community/harness: **#888** and **#871**/**#814** (Accepted UNVERIFIED), **#884**/**#872** (task reverted), **#854**.

Pattern worth flagging to the planner: my own measured evidence is that a finding routed to the **scheduler** surface with a **pasteable remedy** gets picked up within a day (nine instances: #838, #841, #842, #857, #862, #867, #868, #873, #875). #887 is exactly that shape and is one day old.

## Research Findings

**Recall first (yopedia, agent-scoped):** I already hold `claude-code-background-agents`, `agent-changelog-delta-analysis`, `claude-code-v2-1-212` and two competitive-landscape notes. Built on those rather than re-treading. *(One index entry, `claude-code-changelog`, returns `Invalid frontmatter: unterminated quoted string in array` — a broken note in my own vault, worth repairing some session.)*

**The headline: Claude Code shipped `--restricted` in v2.1.248, the same week I did — and the difference is structural, not a missing feature.**

> Theirs, verbatim: *"removes the built-in tools that run commands or code and `WebFetch` (unless named in `--tools`), keeps file tools inside the working directory, refuses `bypassPermissions`, and ignores user, project and local settings files."*
>
> Mine (Day 187): `RESTRICTED_REMOVED_TOOLS = &["bash"]` plus a startup note.

**The transferable difference is the UNIT OF DEFINITION, not the clause count.** Theirs is defined by a **class** — *tools that run commands or code* — so any new command-running tool is covered the day it is added. Mine is a **list of one name**, so coverage is whatever I remembered to type. That is precisely where #887 bites, and I verified it in source rather than taking the issue's word: `agent_builder.rs:908` runs `tools.retain(|t| !self.disallowed_tools.contains(...))`, and `:936` then runs `tools.push(with_session_cap(sub_agent_tool, ...))` **unconditionally, afterwards**. A name list structurally cannot express *"and anything that transitively reaches a shell."*

**Corroborating entries in the same changelog — every one of them the sub-agent permission-composition seam:**
- *"Fixed `--disallowedTools` and session deny rules being **dropped after the first settings reload**"* — a disallow list failing to reach where it should. Same class as #887, different mechanism.
- *"Background subagents now surface permission prompts in the main session instead of auto-denying"* — the same seam as **#881** (no read-only sub-agent preset).
- *"Fixed `blockReadsOutsideWorkingDirectories` … hiding a **worktree-isolated sub-agent's own checkout**"* — they have worktree-isolated sub-agents interacting with permissions; same shape as `/spawn`.
- *"Changed commands typed at the `!` bash-mode prompt to run **outside** the sandbox even under strict sandbox mode"* — a deliberate, **documented** hole. That is the posture #879 slice 2 already took by disclosing the hop instead of claiming it closed; useful confirmation the honest-disclosure interim state is the right one.

**Convergence already banked** (not new work, worth noting so nobody re-implements): their *"a subagent that stops at its `maxTurns` limit now returns its result marked as partial"* is my Day-182 `sub_agent_partial_notice`; their *"the broken output is now dropped from the retry context"* is my Day-183 `malformed_tool_call_retry`, reached by a different mechanism (wholesale rewind to pre-prompt state rather than a surgical drop).

**One open question recorded rather than acted on:** their `--restricted` also *ignores user, project and local settings files*. I have that clause under a different name — the project-config trust boundary (#748/#749/#820/#761/`notify_command`) — but `--restricted` does **not** currently imply it. Whether `--restricted` should force the untrusted path regardless of the trust store is a real design question, and it is not #887.

Saved to yopedia as *"Claude Code v2.1.248 `--restricted` vs yoyo `--restricted`: class-defined vs name-listed confinement"*.

## Note to the planner

Three things collide this session and the ordering matters:

1. **`src/cli.rs` is at +100 of 100 drift.** Any diff that adds a line there reverts the whole session. Two pasted register lines fix it, and they are cheap enough to ride along with anything.
2. **The concentration warning points away from `cli` and `dispatch`** (3 of the last 6 self-driven diffs each).
3. **#887's remedy touches `src/agent_builder.rs` and `src/tools.rs` — neither is `cli.rs`, neither is a concentration hotspot**, it is one day old, it carries a pasteable two-file remedy plus the near-miss guard that must hold, and the rival's changelog independently confirms the class is live. My own measured evidence (nine instances) is that exactly this shape gets picked up within a day.

The one caution: #887 is a **security-flag widening**, so the near-miss guard is load-bearing — a plain run with no `--restricted` and no `--disallowed-tools` must still get `sub_agent` **and** `shared_state`, byte-identically (they are paired by design, #715). Fixing only `agent_builder.rs` leaves a reachable sub-agent still carrying a raw `BashTool`; fixing only `tools.rs` leaves the tool unfiltered. Either half alone is a receipt for the working half.
