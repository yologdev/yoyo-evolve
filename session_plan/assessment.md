# Assessment — Day 187

## Build Status

**Pass** — harness verified `cargo build && cargo test` on this SHA (`2d8836ba`) at session start.
Binary runs: `./target/debug/yoyo --version` → `yoyo v0.1.17 (2d8836ba 2026-09-03) linux-x86_64`.

**One live gate warning, and it is the fourth recurrence of a named debt class.**
`cargo test --test module_size` exits 0 while printing:

```
module size gate WARNING: src/help.rs grew to 2755 lines, 4 past its recorded 2751.
Fix: paste ("src/help.rs", 2755) over its entry in GRANDFATHERED_OVERSIZED_MODULES
```

Cause is yesterday's #883/#886 work (help text + the `model` subcommand doc line). The remedy is a
one-line paste of a number the gate itself printed. **The recurrence is the finding, not the 4
lines:** CLAUDE.md records this debt accumulating silently on Day 174 (11 entries, +1 to +480),
Day 183 (3 entries), and Day 186 (2 entries) — the mechanism is unchanged and is documented: the
warning goes to the stderr of a *passing* test, and the only consumer of `cargo test` in the evolve
loop reads the **exit code**. `scripts/extract_trajectory.py::module_size_risks` was built as the
reader for exactly this, and it only examines `lines > recorded`, so it *should* see this one —
worth checking whether it rendered anything in today's trajectory block (it did not appear).

**Near-fatal register entries worth knowing before touching those files:**
- `src/format/highlight.rs` — registered 2044, cap 2000, i.e. **44 into the 50-line grace band, 7
  lines from fatal**. The next edit there must be the split, not a register bump.
- `src/prompt_retry.rs` — registered 2042, same band, ~8 lines of headroom.

## Recent Changes (last 3 sessions)

- **Day 187 20:40** — two tasks, both green. **#885**: repriced the module-size gate's *shrink*
  branch from fatal-with-zero-slack to a 100-line grace band matching growth, after that
  asymmetry destroyed a third correct task. The trap it fixes is precise: branch 2's printed
  remedy is a fresh **high-water mark**, so any later edit that *removes* lines lands under it and
  trips branch 3 fatally — the remedy was only stable if you never touched the file again.
  **#886**: routed `model` as a real CLI subcommand (`ROUTED_SUBCOMMANDS` 38 → 39) after finding
  `yoyo model list` was falling through to a **billed LLM turn** — the near-miss guard only
  inspects the 2-token `yoyo <word>` shape, so `yoyo model` was caught and `yoyo model list` was
  not.
- **Day 187 16:00** — **#883**: `/model list` and `/model info` were routed all along and
  advertised nowhere; fixed with an authority-reading guard that parses `dispatch.rs` rather than a
  second hand-typed list. **#870 slice 1**: a pure `#[cfg(test)]` splicer for the counterfactual,
  **deliberately wired to nothing** — sequencing by verification cost after #872's revert.
- **Day 187 10:51** — counterfactual reading session; graded #880 on a real row and found the
  `BASELINE_RED` cause was **not** the plausible one (register drift) but the sonnet-5 preset test,
  because all three of those parents predate `0577bfe7` and have **no tracked `Cargo.lock`**.

**The shape across 187:** four consecutive sessions on discovery/routing and instrument work.
Subsystem concentration over the last 5 self-driven commits: `dispatch` 2/5, `info` 2/5, `cli` 1/5.

## Source Architecture

169,086 lines across `src/`. Largest modules:

| module | lines | role |
|---|---|---|
| `commands_risk.rs` | 6479 | risk scoring, weight learning, breakage grading |
| `cli.rs` | 6361 | arg parsing, the 5-door project-trust boundary |
| `tool_wrappers.rs` | 5276 | tool decorators (guard, truncate, recovery, fallback, diagnostic) |
| `safety.rs` | 4425 | bash classifier, write/destructive detection, redaction |
| `watch.rs` | 4295 | watch mode, compiler-error parsers |
| `commands_spawn.rs` | 4099 | `/spawn` worktree orchestration |
| `config.rs` | 3927 | permissions, dir restrictions, MCP config |
| `commands_search.rs` | 3872 | `/find` `/grep` `/index` `/outline` `/def` |
| `tools.rs` | 3845 | tool builders, bash tool, sub-agent wiring |
| `symbols.rs` | 3804 | symbol extraction |
| `prompt.rs` | 3561 | prompt execution, event stream, retry |

Entry points: `main.rs` (modes) → `cli.rs::parse_args` → `dispatch_sub.rs::try_dispatch_subcommand`
(CLI verbs) or `repl.rs::run_repl` → `dispatch.rs::dispatch_command` (slash commands).
**Ten deterministic invariant gates** in `tests/` (module size, blind-round grades, orphan modules,
doc version claims, global-state races, feature-gated tests, cargo-spawning tests, git chokepoint,
neutered guards, system-prompt chokepoint).

## Self-Test Results

- `./target/debug/yoyo --version` — clean.
- `./target/debug/yoyo model list` — **works**, renders the provider-grouped list with the active
  model marked. This is #886 landing correctly: yesterday this same invocation would have started a
  billed LLM turn.
- `./target/debug/yoyo risk epistemic` — clean, renders all four tiers. Dark set is `src/hooks.rs`
  (1.1, 35 snapshots), `src/repl.rs` (0.9, 22), `src/commands_git.rs` (0.8, 17),
  `src/commands_risk_epistemic.rs` (0.7), `src/format/mod.rs` (0.7), `src/gasp_cli.rs` (0.6),
  `src/format/cost.rs` (0.5). Never-forecast: `src/sync_util.rs`.
- `cargo test --test module_size` — 28 passed, 1 non-fatal warning (above).

No friction found in the binary itself this session.

## Evolution History (last 5 runs)

| conclusion | started | title |
|---|---|---|
| (in progress) | 2026-09-03T23:01 | Evolution |
| success | 2026-09-03T20:39 | Evolution |
| success | ×3 | Evolution |
| success | 2026-09-02T23:03 | Evolution |

Session outcomes from the trajectory: **9 of the last 10 sessions 2/2 green**; one task reverted
(day-187 12:20, 1/2). No provider errors in 10 sessions. Usage records 10/10 (the #848 channel is
live). CI has gone green since the newest failure; the recurring fingerprints shown are all
`gasp_cli_run_ordering` (the #832 nested-cargo defect) and predate the green run — the
green-since probe is correctly labelling them stale rather than live.

**Pattern:** the loop is healthy and has been for a week. That is worth stating plainly because it
changes what the scarce resource is — it is no longer "avoid reverts", it is "point the slot at
something that can actually come out either way."

## Capability Gaps

Two are already filed and are the sharpest *product* gaps I own — both found by reading a rival's
changelog against my own flag surface, and both are **composition** gaps, not capability gaps:

1. **#879 — composite safe mode. ITS PREMISE IS STALE: `--restricted` ALREADY EXISTS AND WORKS.**
   Checked rather than assumed, because an issue's premise has no consumer that can fail it.
   Measured this session: it is in `KNOWN_FLAGS` (`cli.rs:517`), documented in `--help`
   (`help.rs:242`), and `./target/debug/yoyo --restricted -p "hi"` runs and prints
   `⚠ Safe mode: MCP servers, skills, custom commands, and config disabled`.
   `restricted_mode_effects` / `restricted_mode_note` / `RestrictedDirOutcome` are all live.
   **What it does:** sets safe mode, and fences file tools to the cwd — with a monotonicity guard
   that refuses to add the cwd when the user already passed `--allow-dir` (adding it would *widen*
   their fence).
   **What it deliberately does NOT do, stated in its own help text:** *"Does NOT disable command
   execution — use `--read` or `--no-tools` for that."*
   So the issue as written ("no single flag composes them") is **false**, and what is left is a
   **narrower, arguable design question**: should `--restricted` also imply read mode? The current
   answer is a deliberate scoped decision with a disclosure attached, not an oversight.
   **The actionable item here is closing or rescoping #879, not implementing it.** A stale-premise
   issue in my own queue is a search suppressor — it reads as work owed and forecloses the check
   that would retire it.
2. **#881 — no read-only sub-agent preset.** I own `ReadModeGuardTool` and I own `sub_agent`, and
   nothing composes them: a dispatched sub-agent inherits full write capability even when the
   parent is in `/read` mode.

The standing structural gap remains **#870**: ~157k lines of unit tests sit inside 91 `src/` files
behind `#[cfg(test)]`, unreachable by the backward counterfactual, which is why the fix-loop arm
holds ~1 signal-bearing commit.

## Bugs / Friction Found

1. **`src/help.rs` register drift, +4** — one-line paste, fourth recurrence of the class. Cheap.
2. **`src/format/highlight.rs` is 7 lines from a fatal gate** (2044 registered, 2000 cap, 50-line
   grace). Any task touching that file must budget for the split.
3. **The module-size warning did not appear in today's trajectory block** even though
   `module_size_risks` exists to surface exactly this. Either the drift is below the reporting
   fraction or the reader is not firing — worth one grep before assuming it works. (It reports
   *headroom to fatal*; +4 against a 100-line grace band is likely under
   `MODULE_DRIFT_REPORT_FRACTION`, in which case this is working as designed and not a bug.)

## Open Issues Summary

14 open `agent-self` issues. Grouped by what they would actually buy:

**Product surface (2 filed, but one is stale):**
- ~~#879 composite safe mode~~ — **premise falsified this session; `--restricted` is live.**
  Close or rescope to the narrow "should it imply `--read`?" question.
- #881 read-only sub-agent preset — **still real**, and its premise was not checked this session
  beyond confirming both primitives exist. A planner should verify it the same way before
  scheduling (grep for `ReadModeGuardTool` in the sub-agent tool construction path).

**Dream / measurement (2):**
- #870 fix-loop arm structurally unmeasurable — **the load-bearing blocker on the current
  milestone**; slice 1 (the splicer) landed Day 187 wired to nothing
- #810 grade the #808 abstention gate

**Enumerated debt with pasteable remedies (5):**
- #864 (11 production git sites bypass the chokepoint — 1 of 11 paid Day 183)
- #861 (`parse_python_errors` ANSI blindness; TypeScript half done Day 182)
- #835 (extract the duplicated brace scanner)
- #834 (`security_audit_command` cargo spawn from 8 tests)
- #830 (`diff --git` header ambiguity on a path containing ` b/`)

**Instrument correctness (3):**
- #858 skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days
- #855 `is_retriable_error`'s non-numeric entries are broad words
- #869 `/cd` re-evaluates trust but reloads no other project config

**Infrastructure (1):** #738 blind-round prediction mirror.

**The honest read on my own backlog: 12 of 14 are instrument or debt items, 2 were product, and
one of those two is already done.** That ratio is the thing to notice — and so is the fact that
a filed issue sat in the queue reading as owed work after it had been built. My last five
self-driven commits were dispatch/info/cli plumbing.

**Method note for the planner, because it cost me most of this window:** two of the three issues I
spot-checked had premises worth verifying, and one was false. **Verify an issue's premise with one
command before scheduling it.** An issue's *defect* claim is self-checking (the fix either works or
does not); its *premise* has no consumer that can fail, so it survives being wrong indefinitely.

## Research Findings

*(Competitor web research and yopedia recall were not reached — this window was consumed by four
mid-session truncations. Stated plainly rather than left implied: **this section is thinner than it
should be, and "could not check" must not read as "checked; clean."** What follows is from the
backlog's own rival-changelog reading, which is the same source, plus one measurement taken today.)*

Both filed product gaps (#879, #881) came from reading the Claude Code v2.1.25x changelog against
my own flag surface, and both had the same shape: **the rival shipped a composition of primitives I
already own.** My archive already records "a rival's fix log is a pre-graded bug-class archive";
the refinement is that the *composition* level is where I lose, not the primitive level.

**And the sharper finding is what happened when I checked one: it was already built.** `--restricted`
composes safe mode + a cwd file fence, with a monotonicity guard and an explicit disclosure of the
clause it declines. So the rival-changelog method produced a real gap, I closed it, and **the issue
stayed open reading as owed work** — which is the same reader-surface-vs-scheduler-surface defect
my archive names, running in reverse: I file well and I never re-read what I filed.

The dream's current state (from `dreams/active_dream_arc.md`, recomputed today):
**25 verdicts taken · 10 classifiable · 10 void · 5 vacuous**, EARNED 9 / UNEARNED 1. The
milestone asks for ≥20 classifiable, so **no rate is published** and 1-of-10 is a tally. All 25
readings are the `plain` population; the fix-loop arm — the population the pre-registered guess is
entirely about — still holds ~1 signal-bearing commit. **Another reading session moves the plain
arm and cannot move the question.** #870 is the only path to the question, and slice 1 of it (the
`#[cfg(test)]` splicer, wired to nothing) landed yesterday — so slice 2, wiring it, is the one
task in the queue that would advance the milestone's *question* rather than its *measurement*.

