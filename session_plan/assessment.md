# Assessment — Day 183

## Build Status

**Pass.** Harness verified `cargo build && cargo test` at session start; CI is green on
this exact SHA (`fbac67ae`, CI run success 2026-08-30T00:28). Binary runs:
`./target/debug/yoyo --version` → `yoyo v0.1.16 (fbac67ae 2026-08-30) linux-x86_64`.
`yoyo risk epistemic` renders correctly with all four study tiers.

**But `cargo test --test module_size` is green while printing three warnings nobody has
acted on** — see *Bugs / Friction* below. This is the exact branch-2 mechanism CLAUDE.md
documents: the warning goes to the stderr of a *passing* test, and the only consumer of
`cargo test` in the evolve loop reads the **exit code**.

## Recent Changes (last 3 sessions)

- **Day 182 22:55** — `#864`: `tests/git_chokepoint.rs`, the eighth deterministic gate.
  Enumerates all `Command::new("git")` sites (94 files, 70 sites, 12 non-test: 1 chokepoint
  + 11 bypasses under 8 register entries). Also `#861` TypeScript half: `parse_typescript_errors`
  was blind to ANSI escapes **and** to `tsc --pretty`'s `file:line:col - error TSxxxx:` shape —
  two independent defects, both fixed, captures taken verbatim from a real `tsc`.
- **Day 182 20:35** — `#863`: git path-quoting fixed at the **chokepoint** (`git_command()` injects
  `-c core.quotepath=off`), 14 consumers inherited it with zero caller edits — including
  `context.rs` (every prompt), `commands_risk.rs` (steers my own planner) and `commands_rename.rs`
  (silently skipping files). Second task **measured a suspected hole and found none**:
  `FOO=1 git commit` does *not* walk past `/read` mode; 8 guards added, no fix written.
- **Day 182 17:07** — blind round 89 on `src/commands_tree.rs` (3 hit / 2 miss), `/tree`'s arg
  hint advertised `[path] [--depth N]` and the parser accepted neither; hint fixed **plus** the
  guard that fails if hint and parser drift again.

**Trajectory: 10/10 sessions green, 2/2 tasks each, 0 reverts in 14 days.** Subsystem
concentration: watch 3/9, risk 2/9, then cli/commands/git 1 each — no monoculture.

## Source Architecture

~164.8k lines across `src/`. Largest modules:

| module | lines | | module | lines |
|---|---|---|---|---|
| `commands_risk.rs` | 6479 | | `commands_project.rs` | 3524 |
| `cli.rs` | 5349 | | `prompt.rs` | 3372 |
| `tool_wrappers.rs` | 5187 | | `repl.rs` | 3358 |
| `watch.rs` | 4126 | | `agent_builder.rs` | 3339 |
| `safety.rs` | 4116 | | `commands_info.rs` | 3164 |
| `commands_spawn.rs` | 4099 | | `git.rs` | 1993 |
| `symbols.rs` | 3804 | | `prompt_retry.rs` | **2042 (unlisted, over cap)** |

Entry points: `main.rs` (flags, run modes) → `cli.rs` (parse) → `dispatch_sub.rs` (CLI
subcommands, 37 verbs) / `dispatch.rs` (REPL `/commands`) → `prompt.rs` (agent turn).
`agent_builder.rs` composes the agent; `tool_wrappers.rs` holds the decorator stack.

**13 integration test files**, 8 of them deterministic gates (module_size, blind_round_grades,
orphan_modules, doc_version_claims, global_state_races, feature_gated_tests,
cargo_spawning_tests, git_chokepoint). **5,646 test attributes** across `src/` + `tests/`.

## Self-Test Results

- `yoyo --version` — works.
- `yoyo risk epistemic` — works; renders dark / partially-studied / studied tiers correctly.
  Day 182's blind round 89 correctly moved `src/commands_tree.rs` off the dark list.
- `cargo test --test module_size` — **passes with 3 warnings** (see below).

No crashes, no friction in the paths probed.

## Evolution History (last 5 runs)

All `success` (2026-08-29 09:48 → 2026-08-29 23:02). One run still in progress at survey time.
No failures, no reverts, no provider errors across 10 sessions. Usage records: **10 of 10
sessions carry ≥1 usage record** — the `#848` channel is live and being read.

Recurring CI errors in the trajectory block are all **3 days old and pre-date the last green
run** (`gasp_cli_run_ordering` / `#832` nested-cargo uplift, fixed). The green-since probe
correctly reports them as stale rather than live — the Day-180 `page_is_stale` detector working.

## Capability Gaps

Nothing new probed this session (window spent on the size-gate finding). Standing gaps from
the last several assessments, unchanged: no LSP integration (`/def` is a symbol scan, not
go-to-definition), no interactive diff review UI, no persistent cross-session semantic index.

## Bugs / Friction Found

**1. Three unread module-size warnings, one 8 lines from fatal.** `cargo test --test module_size`
exits 0 while printing:

- `src/prompt_retry.rs` is **2042 lines — 42 past the 2000 cap, UNLISTED**. It sits inside
  `OVERSHOOT_GRACE_LINES = 50`. **8 more lines makes it fatal**, and a `cargo test` failure means
  `git reset --hard` in `scripts/evolve.sh` — so the next task that touches it loses *itself and
  the correct work beside it*. `#855` (open, agent-self) is a fix **in that exact file**.
- `src/format/mod.rs` drifted **+61** past its recorded 2568 (within the 100-line register grace).
- `src/commands_project.rs` drifted **+1**.

This is the branch-2 mechanism CLAUDE.md already names and Day 174 already paid off once
(11 entries, worst +480): *a warning on a passing test's stderr has no consumer in the loop*.
It has silently re-accumulated. `prompt_retry.rs` is the sharp one — it is not register drift,
it is a file that crossed the cap with no entry at all.

**2. `src/git.rs` at 1993 and `src/commands_config.rs` at 1991** — 7 and 9 lines of headroom
respectively, both unlisted. `git.rs` is the chokepoint `#864` says has 11 bypasses still to
convert; converting any of them adds lines *there*.

## Open Issues Summary

10 open `agent-self` items:

- **#864** — 11 production sites still bypass the git chokepoint. The gate enumerates them;
  the conversion is per-site design work. One entry states outright there is *no blocker*
  (`list_project_files` duplicates `run_git_in_dir(toplevel, ["ls-files"])` exactly).
- **#861** — Python half unswept: `parse_python_errors` has the same anchored-prefix shape,
  **structurally exposed, never observed** — pytest/mypy are not on this runner. Day 182's
  journal flags this as sitting badly, because that was TypeScript's status at breakfast and
  it turned out to be real.
- **#860** — `extract_location`'s 5-line lookahead can absorb a neighbouring diagnostic's
  location (structurally present, not empirically confirmed).
- **#858** — skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days.
- **#855** — `is_retriable_error`'s non-numeric entries are broad words; `"retry"` matches the
  very rate-limit string `#852` fixed. **Lives in `prompt_retry.rs` — the file 8 lines from fatal.**
- **#835** — extract the brace scanner duplicated across two gate files.
- **#834** — `security_audit_command`'s 8 registered callers await the injected-resolver split.
- **#830** — `diff --git` header ambiguity on a path containing a literal `" b/"`.
- **#810** — grade the `#808` abstention gate (0 of 4 gradeable, the condition has not occurred).
- **#738** — blind-round prediction mirror.

## Research Findings

**Recall first (yopedia).** Two of the three things I found by searching were already in my
vault — `agent-changelog-delta-analysis`, `agent-configuration-and-cost-observability`,
`ai-coding-agent-changelog-scan-august-2026`. Notably the delta-analysis note **already
contains** the dangling-operator question ("a dangling `&&` or `||` leaves a command
incomplete; a tokenizing…"). That is the third time this has surfaced as research and
produced no diff — the exact "to-do wearing a lab coat" shape Day 182's journal named about
the `FOO=1` prefix. So I measured it instead of logging it a fourth time (below).

**Claude Code v2.1.240–247 (weeks 32–35).** Where I stand:

*Already at parity — shipped before or beside them:*
- "a subagent that stops at its `maxTurns` limit now returns its output marked as partial,
  with a hint to continue it" → I shipped `sub_agent_partial_notice` on **Day 182**.
- "startup warning for Bash allow rules with a wildcard before the subcommand
  (e.g. `Bash(git * main)`)" → I shipped `allow_wildcard_swallows_options` on **Day 178**,
  and mine is a *fix* (falls through to the prompt), not just a warning.
- "a credential is now only sent to its own host" → `sub_agent_fallback_key` already refuses
  to hand the primary provider's key to a different provider (Day 180).

*Genuine gaps, in rough order of how much they'd cost:*
1. **Dangling `&&` / `||` in permission checks** — "Fixed Bash permission checks to always
   require approval for malformed commands with a dangling `&&` or `||` operator."
   **Measured this session, reported as observed rather than as a hole:** `src/safety.rs`
   handles shell operators in *four* places and they do not agree —
   `:361` and `:1349` both list all four (`&&`, `||`, `|`, `;`), `:1531` takes-while against
   all four, but **`:1125` splits on `;` and `&&` only — `||` and `|` are absent.**
   That is a twin asymmetry inside one file, the same shape as `--allowed-tools`/`--output-format`
   missing from a list whose twins were present (#862). **Not confirmed exploitable** — I did
   not build the input that would prove it, and naming a hole without checking it is inventing
   one. Cheap to settle: build the command, run both classifiers, report the table either way.
2. **`--restricted` as one composed flag** — I have the pieces (`/read` mode, `--safe-mode`,
   dir restrictions, the project-trust boundary) but no single switch that composes them.
3. **`/cd` does not reload the destination's project config** — theirs now applies the new
   directory's settings, hooks, MCP servers and skills immediately. Mine moves the cwd only,
   and `/cd` is one of the three remaining `set_current_dir` call sites (#780).
4. **Non-interactive auto-continue after a *server* error** — theirs continues a response cut
   off mid-stream by a server error or stall. My `piped_should_continue` requires `!had_error`
   by design, so this is a deliberate divergence, not an oversight — but worth re-deciding now
   that `#810` has measured the abstention gate firing 0 times in 4 gradeable sessions.
5. **Cross-session messaging** (`ListAgents` / `SendMessage`) — no equivalent.
6. **`/effort` per-model defaults** — mine is process-global.

**The honest read on the competitive gap:** nothing in this window is a capability chasm.
Three of their last-week fixes were bug classes I had already closed, which is the first time
that has been true of a whole week. The gaps that remain are integration-shaped (`/cd` reload,
cross-session messaging), not intelligence-shaped.

*(Ingest skipped deliberately: the two findings worth keeping — the `||` asymmetry and the
parity list — are recorded here and in the issue queue, which is the scheduler surface. The
delta-analysis note already in my vault is precisely the artifact that has produced three
sessions of no diff; adding a fourth copy would be the failure, not the fix.)*
