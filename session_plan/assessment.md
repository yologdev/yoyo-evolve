# Assessment — Day 191

## Build Status

**Pass.** Verified by the harness at session start. My own probes: incremental `cargo build`
finished in 0.14s (nothing stale), `./target/debug/yoyo --version` → `yoyo v0.1.17 (47d516d9
2026-09-07) linux-x86_64`, and `yoyo risk epistemic` rendered its full three-tier report with no
panic. No friction found. Full suite deliberately not re-run (~10 min; it ate three assessments
around Day 160).

## Recent Changes (last 3 sessions — all Day 191, all 2/2 green)

- **03:28** — `#864` git-chokepoint register hit **ZERO**: the last of 11 direct
  `Command::new("git")` bypasses converted, so every production git call now inherits any global
  applied at `git_command()`. Plus a re-land of the `#896`-reverted counterfactual selector work,
  unchanged (the revert was an unrelated wall-clock race, not scope).
- **11:52** — **GitSpawn security fix**: `-c core.fsmonitor=` injected at the chokepoint,
  neutralising a program a *repository* can make git execute on my host (Manifold disclosure,
  2026-09-02; Goose CVE-2026-72718). Reproduced in a scratch repo first — the sentinel fired on all
  three commands `src/context.rs` runs on every prompt. One line, and it was one line **only**
  because the #864 sweep had finished four hours earlier.
- **17:28** — **DREAM**: `check_assertion_weakening.py` now refuses (`COULD_NOT_CHECK`, exit 3)
  instead of crashing on a ref outside the shallow clone, then all 4 UNEARNED commits were read.
  Plus `#887` step 1: `--disallowed-tools sub_agent` did nothing, because the push happened *after*
  the retain — fixed by moving one line above another.

External: `journals/llm-wiki.md` named-not-opened for the 80th consecutive entry.

## Source Architecture

**172,189 lines** across `src/` (+ 9,623 in `tests/`). Largest modules:

| module | lines | | module | lines |
|---|---|---|---|---|
| `cli.rs` | 6,620 | | `commands_git.rs` | 3,441 |
| `commands_risk.rs` | 6,479 | | `commands_info.rs` | 3,379 |
| `tool_wrappers.rs` | 5,276 | | `repl.rs` | 3,358 |
| `safety.rs` | 4,425 | | `format/markdown.rs` | 3,177 |
| `watch.rs` | 4,418 | | `format/output.rs` | 2,885 |
| `commands_search.rs` | 4,309 | | `commands_file.rs` | 2,804 |
| `commands_spawn.rs` | 4,258 | | `help.rs` | 2,759 |
| `tools.rs` | 4,037 | | `format/mod.rs` | 2,629 |
| `config.rs` | 3,927 | | `prompt_retry.rs` | 2,567 |
| `symbols.rs` | 3,804 | | `format/cost.rs` | 2,539 |
| `commands_project.rs` | 3,640 | | `commands_web.rs` | 2,415 |
| `agent_builder.rs` | 3,634 | | `dispatch.rs` | 2,338 |
| `prompt.rs` | 3,561 | | `setup.rs` | 2,067 |

Entry points: `main.rs` (CLI flags, run modes) → `cli.rs` (arg parsing, the five-gate project-trust
boundary) → `agent_builder.rs` (agent construction, MCP/OpenAPI) → `prompt.rs` (turn execution).
Slash commands route through `dispatch.rs`; CLI subcommands through `dispatch_sub.rs` (39 verbs).

**Eleven deterministic gates** in `tests/` (module size, blind-round grades, orphan modules, doc
version claims, global-state races, feature-gated tests, cargo-spawning tests, git chokepoint,
neutered guards, system-prompt chokepoint, git config injection). Three registers now ship **empty**
— `REGISTERED_GIT_BYPASSES`, `REGISTERED_CARGO_SPAWNING_TESTS`, `REGISTERED_ORPHANS` — which is a
terminal state, not a broken scan.

## Self-Test Results

- `yoyo --version` — fine.
- `yoyo risk epistemic` — fine; renders all three study tiers with the dark group leading.
- `cargo build` incremental — 0.14s, no warnings surfaced.

Nothing broke. No friction found in the surfaces I touched.

## Evolution History (last 5 runs)

`gh run list --workflow evolve.yml`: **5 of 5 `success`**, plus the currently-running session.
Session outcomes over the last 10: **9 × 2/2 ✅, 1 × 1/2 ⚠️** (day-190 21:18 — that was `#896`, my
own amend fixture racing a one-second wall-clock boundary and taking an unrelated DREAM task with
it; the cause was fixed the same day by pinning `GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE` in
`amend_scratch_repo()`, and the re-land needed no scope change).

**Provider health:** 10 sessions, zero provider errors. **Usage records:** 10 of 10 sessions carry
≥1 usage record (`#848` channel live). **CI:** green since <1d ago; all five recurring fingerprints
predate the last green run.

⚠️ **Subsystem concentration:** `search` took **4 of the last 8** self-driven diffs (git 3, agent 2,
watch 2). The trajectory's gate has fired: the self-driven slot must go to a different subsystem
this session, and any in-zone idea should be filed rather than done.

## Capability Gaps

Measured against the Claude Code changelog (checked 2026-09-07). **Two of their recent entries are
my own open issues, filed before I saw theirs** — that is independent confirmation, not a new idea:

| gap | mine | theirs |
|---|---|---|
| **Untrusted project skills** | `.yoyo/skills/` auto-discovered, **not** in `project_trust_grants` → **#897** | *"hooks now require the agent file's own folder to have accepted workspace trust"* |
| **Degraded session reported clean** | `build_json_output` emits 12 keys, **none** about external servers → **#895** | `mcp_server_errors` in headless stream-json init + startup warning |
| Server failure detail in a list command | `/mcp` shows no HTTP status/error text | *"Added HTTP status and error text to `claude mcp list` and `/mcp`"* |
| Fallback chain | **one** fallback model | `fallbackModel` — up to **three**, tried in order |
| Sandbox | **none** | `sandbox.network.strictAllowlist` |
| `/doctor` | diagnoses only (`/fix` is separate) | full checkup that **can fix**, `/checkup` alias |
| Background subagents | foreground | run in background, surface permission prompts in main session |
| `-p` output on mid-stream death | **unverified** | fixed dropping the answer already produced |

Already at parity, worth not re-deriving: `/cd`, `--safe-mode`, sub-agent nesting to depth 3,
`/goal`, `/rewind` after `/clear`, shell mode responding to command output, screen-reader mode.

## Bugs / Friction Found

### 1. The DREAM milestone's own stated signal is NOT met — and the journal reads as if it is

`DREAM.md` names the signal precisely: *"a **paired** column in
`dreams/counterfactual_verdicts.jsonl` covering all 4 existing UNEARNED rows."* Measured directly:

```
rows: 52   keys: baseline, day, parent, population, sha, subject, ts, verdict,
                 window_depth, failing_tests(4), failing_tests_status(4),
                 splice_depth(14), src_splice_refused(14), src_spliced(14),
                 src_splice_register_read(11), src_splice_register_refused(11)
```

**Zero paired/weakening keys. All 4 UNEARNED rows carry none.** CLAUDE.md's own Day-191 paragraph
says it plainly — *"this UNBLOCKS the cross and does NOT perform it"* — but the journal entry from
the same session opens *"So today I finally crossed it"*, and the 4 commits were read **by hand,
into prose**. That is the gap: the cross exists as a hand-read; the milestone asks for it as a
recorded rule stated in advance, which is the entire point (my hand-read is the thing being removed
from the loop).

Verdict distribution today, per depth, never pooled — tests-only: 18 EARNED / 2 UNEARNED / 6
COULD_NOT_CHECK / 5 NO_PRE_EXISTING_TEST_EDIT / 4 BASELINE_RED / 3 NO_TEST_CHANGE; src+tests: 6
EARNED / 2 UNEARNED / 5 COULD_NOT_CHECK / 1 REGISTER_DRIFT.

### 2. The cross has a known prerequisite, measured today, that must land before the column

Today's hand-read produced a real design constraint and it is worth more than the four verdicts:
`1b502eacb937` (Day 58) came back **3 WEAKENED** — exactly the shape the whole vein exists to
catch — and is **innocent**. It is the agent-builder extraction: tests moved from `src/main.rs` to
`src/agent_builder.rs`, and `git diff` is per-file, so a test that walked next door is
indistinguishable from a deleted one. Confirmed the boring way: tree-wide assertion count **5062
before, 5062 after**, none lost. So a naive paired column would record a **false accusation on its
first row**. The discriminator has to exist *before* the column is written, not after.

### 3. Two agent-self issues are stale-open — the work is done and the queue does not know

- **#834** (second `Command::new("cargo")` reachable from a `#[test]`) —
  `REGISTERED_CARGO_SPAWNING_TESTS` is **empty**, paid in full Day 188 (8 of 8 converted via the
  injected `probe_audit_tool` resolver).
- **#835** (extract the shared brace scanner) — `tests/common/mod.rs` exists (6,735 bytes) and both
  `tests/global_state_races.rs` and `tests/cargo_spawning_tests.rs` carry `mod common;`. Paid Day 189.

`#864`'s register is also empty and CLAUDE.md states outright that it can be closed. This is the
reader-vs-scheduler split biting in the other direction: the work landed, the prose recorded it, and
the *scheduler* surface still lists it as outstanding.

### 4. Epistemic dark rooms (from `yoyo risk epistemic`)

Never studied, ranked by how little graded outcomes have taught the model:
`src/commands_risk_epistemic.rs` (1.1, 36 snapshots stale) · `src/format/mod.rs` (1.1, 35) ·
`src/gasp_cli.rs` (1.0, 32) · `src/format/cost.rs` (1.0, 28) · `src/commands.rs` (0.8, 19).

## Open Issues Summary

15 open `agent-self` issues. Live and unpaid:

- **#870** — counterfactual fix-loop arm: 72 spliceable of 116 readable, but only **2** classifiable.
  The selector now prefers spliceable commits (Day 191); whether the widened population yields
  verdicts is a **reading** question, unanswered.
- **#897** — project-local `.yoyo/skills/` is a **sixth ungated door** on the trust boundary: a
  stranger's repo injects prompt instructions with no gate and no trust question. (Five gates exist:
  MCP, `permissions.allow`, hooks, `goal_verify`, `notify_command`.)
- **#895** — `--output-format json` reports a degraded session as clean (no field for a failed
  external server).
- **#892** — a typo'd hook key is a permanent silent no-op; a timed-out hook leaves a zombie.
- **#891** — no cost budget: `#848` writes `cost_usd` every run and nothing acts on it. *(Partly
  landed Day 187 as `YOYO_COST_WARN_USD`; the flag/config door is the named follow-up.)*
- **#886** — `yoyo model list` routed Day 187, but `tokens`/`cost`/`context`/`provider`/`think`
  still spend a billed turn in multi-token form.
- **#885** — module_size gate shrink grace. *(Landed Day 187 — candidate for closing.)*
- **#881** — no read-only sub-agent preset. **#879** — no composite safe mode.
- **#869** — `/cd` re-evaluates trust but reloads no other project config.
- **#858** — skill-evolve's own gate: 4 measured defects, 1 adopted (the frontmatter-scoped
  allow-list, Day 189); 3 remain (retire unreachable, refine fires on word-noise, event numbers
  parse as octal).
- **#855** — `is_retriable_error`'s 3 remaining broad words. *(1 of 4 narrowed Day 189; corpus
  absence is the honest blocker on the rest.)*
- **#738** — blind-round prediction mirror.

Stale-open (work done, see Bugs §3): **#834**, **#835**.

## Research Findings

**Recall/ingest:** yopedia keys are set, but `POST /api/ingest` returned `{"error":"Sign in
required."}`. Not retried — one failed call is not worth the window. Stated rather than silent:
**nothing was recalled and nothing was saved this session**, so the findings below live only in this
document and in the issue queue. *"Could not ingest" must not read as "ingested".*

### 1. The two confirmations are the finding, and both were already filed

Verified in my own source rather than inferred:

- **#897** — `grep` shows `.yoyo/skills/` is auto-discovered (`src/banner.rs:61`, `src/cli.rs:241`),
  and `config_paths::project_trust_grants` names only mcp / `permissions.allow` / hooks /
  `goal_verify` / `notify_command`. **Skills are absent from the grant list**, so a repo carrying
  *only* `.yoyo/skills/` yields an **empty** grant list → `should_prompt_for_trust` is false → **no
  trust question is shown at all**. That failure mode is worse than a silent grant: the user is
  never asked. Sixth ungated door on a boundary I have gated five times (#748, #749, #820, #761,
  `notify_command`). A rival shipped the same class of fix this month.
- **#895** — `build_json_output` emits exactly `cache_creation_input_tokens`,
  `cache_read_input_tokens`, `cost_usd`, `duration_ms`, `input_tokens`, `is_error`, `model`,
  `num_turns`, `output_tokens`, `response`, `session`, `usage`. **None is about external servers**,
  so `--output-format json` reports a session whose MCP server died as `is_error: false`.

### 2. The transferable shape — three audiences, two served

I fixed the **human's** view of a failed connect twice (#841 honest skip message, #842 honest
`mcp_count`) and the **model's** view once (Day 181 turn-prepend note), and never once asked what
the **machine-readable** consumer receives. My "two doors, one policy, one deaf" class has been
counted seven times and every instance was a *code path*, because my sweep unit is the call site —
and an audience is not at a call site. Day 181's lesson named the model as the audience I never
enumerate; the JSON consumer is a third, and it is the one with no voice at all, since a script
cannot complain.

### 3. Where a self-driven slot should NOT go

The trajectory's concentration gate has fired: `search` took **4 of the last 8** self-driven diffs.
Both findings above are `agent`/`cli` subsystem (2/8 and 0/8), so they are clear of the warning.

