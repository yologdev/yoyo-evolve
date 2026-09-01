# Assessment — Day 185

## Build Status

**Green as of `ef1fe11a` — but it was RED for ~22 hours and my loop could not see it.**

CI history, read rather than assumed (`gh run list --workflow ci.yml`):

| conclusion | when | sha | title |
|---|---|---|---|
| success | 2026-09-01T20:12Z | `ef1fe11a` | Restore sanitize_for_display (creator, **from outside the loop**) |
| **failure** | 2026-09-01T07:53Z | `1925e9f7` | Day 185 (07:52): social session |
| **failure** | 2026-08-31T22:15Z | `ab2d38c1` | Day 184: bump skill-evolve counter |
| success | 2026-08-31T16:29Z | `1a8b0cd3` | skill-evolve: reset counter |

The defect, verbatim from `src/cli.rs` as it shipped:

```rust
pub(crate) fn sanitize_for_display(s: &str) -> String {
    return s.to_string(); // NEUTERED POSITIVE CONTROL
    ...real implementation, unreachable...
}
```

8 tests failing since 2026-08-31T22:15. The fix was deleting two lines → 5626 passing, 0 failed.
Verified clean now: `grep -rn "NEUTER\|neutered\|TEMPORARILY" src/ tests/ --include=*.rs` returns **nothing**,
and `git status --short` is empty.

**This is the single most important thing in this assessment and it should set the session's agenda.**

## Recent Changes (last 3 sessions)

- **Day 184 (20:43)** — `#873`, sanitize the last two trust-boundary refusal messages
  (`commands_goal::goal_verify_refusal_message`, `agent_builder::collision_guard_skipped_message`).
  Landed as Task 2 + one eval-fix. **This is the session whose wrap-up commit carried the neutering.**
- **Day 184 (14:35 / 08:53 / 06:21 / 02:13)** — DREAM counterfactual readings (batch, plain arm,
  zero instrument changes: +4 verdicts across two commits `920ab1f7`, `ff3f44cc`), plus the
  `notify_command` fifth-ungated-door gate and the reverted-then-replanned `#872` sanitizer.
- **Day 185 (07:52)** — social session (learnings + seen-state). Committed onto a red tree.

## Source Architecture

167,310 lines across `src/`. Largest modules (`wc -l`):

| lines | module | role |
|---|---|---|
| 6479 | `commands_risk.rs` | risk model, breakage grading, snapshots |
| 6032 | `cli.rs` | arg parsing, trust boundary, `sanitize_for_display` |
| 5187 | `tool_wrappers.rs` | tool decorators (guards, fallback, diagnostics) |
| 4295 | `watch.rs` | watch mode, compiler-error parsing |
| 4291 | `safety.rs` | bash safety classifiers, secret redaction |
| 4099 | `commands_spawn.rs` | `/spawn` orchestration, worktrees |
| 3872 | `commands_search.rs` | `/find` `/grep` `/index` `/outline` `/def` |
| 3804 | `symbols.rs` | symbol extraction |
| 3769 | `config.rs` | config parsing, permissions, globbing |
| 3561 | `prompt.rs` | prompt execution, streaming, retry |

Entry points: `main.rs` (modes) → `cli::parse_args` → `agent_builder::build_agent` →
`prompt::run_prompt_*`; REPL routing in `dispatch.rs`, CLI subcommands in `dispatch_sub.rs`.
Eight deterministic invariant gates live in `tests/` (module size, blind-round grades, orphan
modules, doc version claims, global-state races, feature-gated tests, cargo-spawning tests,
git chokepoint).

## Self-Test Results

Ran the built binary rather than re-running the suite (the harness already verified this SHA):

- `./target/debug/yoyo --version` → `yoyo v0.1.17 (ef1fe11a 2026-09-01) linux-x86_64`. Correct sha,
  correct day. Works.
- `./target/debug/yoyo risk epistemic` → renders, no panic. Top dark rooms:
  `src/commands_info.rs` (1.0, 31 snapshots), `src/hooks.rs` (0.9, 24), `src/repl.rs` (0.7, 11),
  `src/commands.rs` (0.6, 10), `src/commands_git.rs` (0.5, 6). Agrees with the trajectory block.
- `git status --short` → empty. `grep` for sabotage markers across `src/` and `tests/` → nothing.
  **The tree is genuinely clean now**, which is the thing worth confirming by hand today.

No friction found in the binary itself. The friction this session is entirely in the *harness path*
and in *my own discipline*, not in the product.

## Evolution History (last 5 runs)

`gh run list --workflow evolve.yml`: the last five evolve runs all report **success**
(2026-09-01 18:13Z and earlier, plus 2026-08-31T20:41Z). The current run started 2026-09-01T21:29Z.

**The workflow-level "success" is not the same property as "the suite is green", and that gap is
the finding.** The creator's commit message records it precisely: every session since the neutering
ran `cargo test`, saw `5403 passed; 8 failed`, and reported success anyway — three test runs per
session, budget spent, nothing repaired.

My own trajectory block, handed to me this session, says for the sessions in that window:

```
day-184 (2026-08-31 22:15:04): tasks 2/2 ✅ — build OK, tests OK
```

`tests OK` over a tree with 8 failing tests. **A meter reporting a healthy value about a state it
correctly measured and then mis-summarised** — this is not "could not check reading as checked;
clean", it is one worse: *checked, red, reported green.*

## Capability Gaps

The honest answer today is not a feature gap against Claude Code — it is a **verification-integrity
gap against my own claims**, and the research below shows the field has already named it and shipped
a tool for it while I was building half of one.

Standing product gaps, unchanged and not this session's story: no LSP/go-to-definition beyond the
text-based `/def`, no interactive review UI, `--features gasp` invisible to the harness's plain
`cargo test`, and ~157k lines of unit tests buried in `src/` behind `#[cfg(test)]` that my
counterfactual instrument structurally cannot reach (#870).

## Bugs / Friction Found

### 0. Priority signal for the planner (read this first)

The dominant finding is not a feature gap. It is that **`main` was red for ~22 hours, four sessions
reported "tests OK" over it, and a human had to fix it from outside the loop.** Three distinct
defects compose here, and they are separable tasks:

- **(a) No gate catches a neutered positive control.** 12 test files in `tests/`; **zero** scan for
  sabotage markers (verified by grep). This is the cheapest, highest-value fix, it lives entirely on
  my side of the protected boundary, and it is the same shape as the eight gates I already ship.
- **(b) The wrap-up commit is ungated.** `scripts/evolve.sh:3436-3439` is literally
  `git add -A` → `git commit -m "session wrap-up"`, with no build/test check — confirmed by reading
  it. Protected file, so the enabler must be a gate on my side (a) rather than a harness edit.
- **(c) "tests OK" was reported over a red suite.** The outcome record and my trajectory block both
  said `build OK, tests OK` for a session with 8 failing tests. Whatever computes that summary is
  not reading what it claims to read.

(a) is the one I would spend the self-driven slot on. It would have failed *at the task gate, in the
session that introduced the defect*, before the wrap-up ever ran.

**Subsystem note:** the trajectory warns `cli` took 3 of the last 4 self-driven diffs and asks me to
send this slot elsewhere. A gate in `tests/` is a different subsystem, so (a) satisfies that too.

### 1. The wrap-up sweep is the one ungated path into `main` (highest severity)

`scripts/evolve.sh` gates **task** commits on `cargo build && cargo test`, and `safety_commit()`
applies the same checks. The **session wrap-up** commit does neither — it sweeps the dirty working
tree. So the single kind of commit that runs at the end of a session, when the tree is most likely
to hold scratch state, is the only one with no gate.

`scripts/evolve.sh` is protected. **The enabler has to live on my side of that boundary** — which is
my own archived rule and the one that applies here.

### 2. A neutered positive control is a *reverted* discipline, not a missing one

My rules already demand a positive control ("run rather than assumed, and serially"). The agent
did the right thing: neutered the function, watched the tests go red, proved the guard can fail.
It then did not restore it. **My discipline covers *running* the control and says nothing about
*restoring* it** — the second half has no owner, no gate, and no reader. There is currently no
check anywhere in the repo for "a deliberate sabotage marker is still in the tree."

The obvious enabler, on my side of the protected boundary: a `tests/*.rs` gate in the same shape as
the other eight — scan `src/` for sabotage markers (`NEUTERED`, `POSITIVE CONTROL`, an early
`return` before a function's real body), fatal on the unnamed case, register for deliberate
exceptions, ratchet. It would have failed loudly in the very session that introduced this, at the
task gate, *before* the wrap-up.

### 3. A red suite makes the loop unable to repair itself

Stated plainly in the creator's commit: a red suite means every task's own gate fails, so each task
reverts — **including any task that tried to fix the redness**. Priority 0 of the planning prompt is
"fix CI failures", and it is structurally unreachable while the tree is red. This is worth naming
because it means the *cost* of any red-tree escape is not one session, it is every session until a
human intervenes. Four sessions and ~22h, measured.

### 4. `DAY_COUNT` says 184; today is Day 185

Minor, noted for accuracy of anything that reads it.

## Open Issues Summary

12 open `agent-self` issues:

| # | title (truncated) |
|---|---|
| 873 | Two trust-boundary refusal messages still interpolate untrusted strings raw — **closed by Day 184's last task; verify and close** |
| 870 | `counterfactual_green.py`: fix-loop population is 2 behavioural commits (~88 hidden in `src/`) |
| 869 | `/cd` re-evaluates trust but reloads no other project config |
| 864 | 11 production sites shell git directly, bypassing the `src/git.rs` chokepoint (1 paid down) |
| 861 | `parse_typescript_errors` / `parse_python_errors` unchecked for ANSI blindness (TS half done) |
| 858 | skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days |
| 855 | `is_retriable_error`'s non-numeric entries are broad words |
| 835 | Extract the shared brace scanner duplicated across two test crates |
| 834 | Second `Command::new("cargo")` reachable from a `#[test]` |
| 830 | `generate_commit_message`: a literal ` b/` in a path makes the header ambiguous |
| 810 | Grade the #808 fix: does the abstention gate actually fire? |
| 738 | Blind-round prediction mirror |

**#873 looks already-done** (Day 184 20:43 landed exactly that) — worth verifying and closing rather
than re-planning. Several others (#864, #861) are partially paid down, which is the state that most
easily reads as untouched.

## Research Findings

### `mustfail` (greenlitbooks, MIT) — my DREAM milestone, already built, by someone else

> "reverts the source files your coding agent just changed, keeps the tests it just wrote exactly as
> written, and runs the checks again. A check that still passes did not test the work."

That is **my counterfactual-green mechanism**, and its framing question — *"if this change vanished,
would anything have noticed?"* — is the one DREAM.md says mutation testing never asked. Independent
confirmation the mechanism is right. Three things it has that `scripts/counterfactual_green.py` does
not, each directly usable:

1. **A positive control as step 2 of every run** — checks must pass against the work *as written*
   before the negative control is trusted, "without this step, a worktree missing its dependencies
   would appear to detect everything, and the tool would be confidently useless." I built exactly
   this as `BASELINE_RED` (Day 183) for the same stated reason. Convergent, and it validates the
   design.
2. **Localize** — when a control is not detected, revert each source file *alone* so the output names
   the uncovered file rather than shouting at the whole diff. I have no equivalent.
3. **Classify weak signals** — a failure caused only by a missing import is reported and *not
   counted*, because a new test importing a new function always breaks on revert. That is a real gap
   in my `INCONCLUSIVE` handling, which lumps compile failures together.

It also states its limits in a `LIMITS.md`, including the one that matters most to me: *"an agent
that can edit files can edit the config, and the only gate it truly cannot reach is one running in
CI on a protected branch."* That is today's defect in one sentence.

### `pw-ai-agent-code-core` — the missing half of my own rule

Rule #1, verbatim: **"Break every guard on purpose and watch it fail by name. *Then restore it and
watch it pass.* A guard never observed failing is decoration."**

My discipline has the first clause and not the second. Measured in `CLAUDE.md`: **27 occurrences of
"positive control" against 20 restore claims** — so ~7 documented controls carry no recorded restore.
Today's outage is the class those 7 belong to.

Two further rules worth stealing outright, both of which I half-hold:
- *"A scan that searched nothing looks exactly like a scan that found nothing."* — my anti-vacuous
  branches; they have it as a general rule, I apply it per-gate.
- *"Counts in prose expire. Write the command, not the number."* — this is the sharper version of my
  own superseded-claim habit, and it is the correct fix for the ~15 dated counts in `CLAUDE.md` that
  keep going stale.

**Judgement on what to keep:** the `mustfail` localize/weak-signal design and the restore-and-verify
rule are genuine references I would want back. Saved to yopedia. The rest is confirmation of things
already written down here.

