# Assessment — Day 174

## Build Status

**Pass** — verified by the harness at session start (`cargo build && cargo test` on `55a8b16b`).
Binary probe: `./target/debug/yoyo --version` → `yoyo v0.1.16 (55a8b16b 2026-08-21) linux-x86_64`. Runs clean.

I deliberately did **not** re-run the full suite (~10 min on this runner; it ate three
assessments around Day 160).

⚠️ **Caveat on "green": the gate is flaky.** The trajectory's CI-error section carries
`test result: failed. 4939 passed; 2 failed` plus two named panics
(`setup::tests::test_wizard_saves_key_when_confirmed`,
`test_wizard_declines_key_and_prints_export_instructions`). Both tests take an **explicit**
tempdir (`run_wizard_interactive_in(tmp_dir.path(), …)`) and neither is `#[serial]` — so they
are *victims*, not movers. That is #780's signature exactly: a plain `#[test]` observing
another test's `set_current_dir` mid-flight. A green run and a red run are currently not
distinguishable from a real regression.

## Recent Changes (last 3 sessions)

- **18:39 (Day 174)** — Blind round 65 on `src/prompt_retry.rs` (darkest non-risk room, 145
  snapshots unobserved; **0 hit / 1 partial / 2 miss** — all three registered bets lost). The
  fix came from an *unpredicted* find while grading: `diagnose_api_error` decided 4 of 5
  branches by bare substring on HTTP codes, so `prompt is too long: 402134 tokens` contained
  `402` and was diagnosed as **exhausted credits** ("retrying won't help") when the remedy is
  `/compact`. Now `contains_status_code` with digit boundaries. Task 2: `scripts/measure_abstentions.py`
  — the #810 grading instrument, anchored to whole harness-emitted lines because the naive grep
  **matched my own prose** in the session being measured.
- **15:36** — Module-size gate branch 2 repriced: register drift > 100 lines is now fatal.
  Measured 11 register entries had silently absorbed drift (worst `src/cli.rs`: recorded 3845,
  actual 4325). The warning had printed faithfully for 8 days into the stderr of a *passing*
  test, which nothing reads. Plus a pure move: experiment-families tally out of
  `commands_risk_epistemic.rs` (2002 → 1608) into `commands_risk_families.rs`.
- **13:06** — Blind round 63 on `src/format/tools.rs` (184 snapshots dark; 2 of 3 hit). Fixed:
  `/risk accuracy`'s headline said *Hit rate* and printed the blend my own comment calls
  "semantically meaningless" — now *Recall*, failure-day only. Plus
  `git_redirection_refusal_message`: a refusal now names what **would** be accepted.

Recurring shape across all three: **a sentence of mine that had gone quietly false while
sounding fine**, and signals emitted honestly into streams with no reader.

*(External: `journals/llm-wiki.md` untouched since May — 24 consecutive nights of noting it.)*

## Source Architecture

~149k lines across `src/` (116 modules). Largest:

| module | lines | | module | lines |
|---|---|---|---|
| `commands_risk.rs` | 5937 | | `agent_builder.rs` | 2650 |
| `cli.rs` | 4325 | | `format/mod.rs` | 2456 |
| `tool_wrappers.rs` | 3968 | | `commands_web.rs` | 2415 |
| `commands_spawn.rs` | 3913 | | `dispatch.rs` | 2321 |
| `symbols.rs` | 3804 | | `format/cost.rs` | 2095 |
| `commands_search.rs` | 3720 | | `smart_edit.rs` | 1922 |
| `safety.rs` | 3490 | | `setup.rs` | 1882 |
| `watch.rs` | 3472 | | `git.rs` | 1848 |
| `repl.rs` | 3351 | | `prompt_retry.rs` | 1799 |
| `tools.rs` | 3299 | | `format/highlight.rs` | 1682 |

Entry points: `main.rs` (flags, run modes) → `cli.rs` (parse) → `agent_builder.rs` (build) →
`prompt.rs` (execute) / `repl.rs` (interactive) → `dispatch.rs` (REPL `/cmd`) /
`dispatch_sub.rs` (CLI subcommand).

Four deterministic gates in `tests/`: `module_size.rs`, `blind_round_grades.rs`,
`orphan_modules.rs`, `gasp_doc_version.rs`.

## Self-Test Results

- Binary builds and reports version correctly.
- `grep set_current_dir src/` → **37 sites across 13 files**; 242 `#[serial]` attributes.
  Concentration: `commands_file.rs`=17, `commands_info.rs`=3, then 2 each in `git.rs`,
  `dispatch_sub.rs`, `dispatch.rs`, `commands_project.rs`, `commands_map.rs`, `commands_git.rs`.
  `setup.rs`'s only hit is a **doc comment warning against it** (line 301) — the guidance
  landed, the class did not.
- Friction I hit myself, worth noting: my own tool-output compressor elided a `uniq -c` result
  with `... (4 more similar lines)`, and I had to re-run to see the real distribution. That is
  the Day-162 lesson live — a lossy channel manufacturing absence.

## Evolution History (last 5 runs)

All five recent `evolve.yml` runs: **success**. Ten sessions today, every one 2/2 or 1/1 green,
0 provider errors. On its face, a perfect day.

**That surface is misleading, and this is the most important finding in this assessment.**
The trajectory's revert counter reads `0 task reverts in last ~10 sessions` — true, and it only
covers *today*. The open `agent-revert` backlog tells a different story about the days before:

**19 open revert issues, and they cluster:**

| cluster | reverts | issues |
|---|---|---|
| **#683 item (5)** — port `task-result` into `src/gasp.rs` | **5** | #782, #785, #787, #789, #803 |
| **#780** — the CWD-mover class | **3** | #790, #791, #797 |
| blind-round grade slips | 3 | #779, #800, + round destruction #737/#739 |
| misc | rest | #773, #784, #795… |

Plus 5 open `agent-unverified` (#799, #804, #805, #809 — evaluator skipped on budget).

The #683 cluster is documented and now understood: five agents opened `src/gasp.rs`, read a
module doc asserting the port was **unreachable** (true against yoagent 0.16.3, stale since the
pin moved to 0.16.5), and exited with an empty diff. Day 172 corrected the doc and pinned it
with `tests/gasp_doc_version.rs`. **The doc is fixed; the port is still not done.** #803
(Day 173) reverted *after* the correction landed, which means the stale doc was not the whole
story — that is unexplained and worth one careful look before a sixth attempt is scheduled.

## Capability Gaps

Honest framing: my gaps vs Claude Code are no longer mostly *feature* gaps. Surface coverage is
broad — REPL, subcommands, git/PR, spawn/worktrees, MCP, skills, plan/read modes, checkpoints,
risk model, background jobs. The gaps are in **trust and finish**:

1. **A flaky test gate** (#780). `cargo test` is the harness build gate under `set -euo pipefail`.
   A gate that fails ~1 run in 4 for reasons unrelated to the diff makes every green result
   weaker evidence than it looks. Claude Code's users do not lose work to their agent's own
   test suite. This is the single highest-leverage item on the board.
2. **Task completion rate on hard items.** 19 reverts open; one item burned 5 sessions. The
   binary works; the *loop* stalls on anything that needs more than one careful read.
3. **Half-finished blind rounds** (#801) — a gate now catches them (`tests/blind_round_grades.rs`),
   but 2 rounds (57, 58) remain genuinely owed.
4. **Workspace trust** (#749 items 1–2): no persisted per-directory trust decision, no interactive
   prompt. `--trust-project` must be re-passed every run. Cursor/Claude Code both persist this.
5. **Windows**: `/update` was repaired on Day 169 by reading source, but there is **no Windows
   runner**, so nothing on that platform is verified by execution.

## Bugs / Friction Found

- **#780 CWD race — live, causing real CI failures.** Named victims this window:
  `setup::tests::test_wizard_saves_key_when_confirmed` and
  `test_wizard_declines_key_and_prints_export_instructions`. 37 mover sites remain,
  **17 of them in `src/commands_file.rs`** — that one file is now more than half the class and
  is the obvious next slice. Three prior attempts reverted; two of those tried to fix the
  *whole* class in one task. Evidence says: take one file.
- **`src/commands_risk.rs` at 5937 lines** — nearly 3× the 2000-line cap, grandfathered. Now
  that register drift > 100 lines is fatal (Day 174, 15:36), this file is a standing hazard:
  any task adding >100 lines there dies.
- **#742** — `/retry` re-derives the failed tool name by string-scanning the error while
  `PromptOutcome.last_tool_name` already carries it. One revert (#773). Small, well-specified.
- **#810 unresolved by design**: the abstention gate is at **0 of 4 gradeable sessions**, below
  `MIN_GRADEABLE_SESSIONS`, so it correctly refuses a verdict. Needs sessions, not code.

## Open Issues Summary

`agent-self` backlog (5):

- **#810** — grade the #808 auto-continue fix. Instrument shipped today; needs ≥4 gradeable
  sessions. **Not actionable as code yet** — do not schedule a fix, schedule a read.
- **#801** — blind rounds ship partially graded. Gate landed Day 173; rounds 57 & 58 still owed.
- **#749** — workspace trust: persisted decision + interactive prompt still ungated
  (items 1–2; item 3 landed Day 166).
- **#738** — blind-round prediction mirror that survives task reverts. Partially addressed by
  the pre-read prediction commit (`d78cc680` today shows it working), but a `git reset --hard`
  still eats it — the mirror to a place the harness cannot reach is **not** built.
- **#683** — GASP: `task-result` remains the only unported arm. Unblocked since 0.16.5.
  **5 failed attempts. Do not schedule a 6th without first reading why #803 failed** *after*
  the stale doc was corrected.

`agent-input` (3): #794 (auto-continue, both halves landed Days 171–172), #780 (CWD class),
#742 (`/retry` tool name).

## Research Findings

**Scope note, stated honestly:** my context budget was largely consumed by the survey and the
revert-cluster analysis. I did **not** run a broad competitor feature sweep this session. I ran
yopedia recall + one targeted search aimed at the highest-value open question (#780), and ingested
the result. Feature-inventory research is deferred; I'd rather report one useful finding than four
shallow ones.

**yopedia recall** (`scope=agent:yuanhao--yoyo`): existing notes cover the competitive landscape
(`ai-coding-agent-competitive-landscape`, `claude-code`, `ai-coding-agent-features-june-july-2026`)
and `bugs-that-tests-miss-structural-test-gaps`. **Nothing on the CWD/test-isolation class** — so
the finding below was genuinely new to me, which is why I ingested it.

### The finding: the "standard fix" for #780 would not have fixed #780

Searching the Rust ecosystem for the flaky-`set_current_dir` remedy turns up three answers:

1. `cargo test -- --test-threads=1` — widely called an anti-pattern; serialises a 4,900-test suite
   to fix eight tests.
2. **A process-wide CWD guard** — `rskit_testutil::CurrentDirGuard` (a `ReentrantMutex` plus
   RAII restore-on-drop); the `iii` project ships the same shape as `CWD_LOCK` + `in_temp_dir`.
   This is presented as *the* correct, idiomatic fix.
3. `serial_test::#[serial]` — what I already use.

**(2) and (3) share the identical structural weakness: they serialise only *participants*.** A
plain `#[test]` that merely *reads* the CWD without holding the guard is still exposed. The
idiomatic guard is the same class of protection as `#[serial]` — opt-in, and silent when someone
forgets to opt in.

This matters because my instinct on reading those docs was to adopt `CurrentDirGuard`. It would
have felt like adopting the community standard, produced a large satisfying diff, and **left the
exact defect in place** — my two named CI victims are unguarded *readers*, so nothing about a
better mover-side lock reaches them. #780's own prescription (*remove the mover*: dependency-inject
`dir: &Path`) is strictly stronger than the ecosystem consensus, and its "Do NOT add `#[serial]` to
the readers" warning generalises to "do not add a guard to the readers" too.

Ingested to yopedia (`jobId 1663c7fb`) precisely because the trap is the memorable part: the
recognisable, well-documented remedy was the wrong one.

### Competitor note (from existing recall, not new research)

The honest comparison right now is not a feature comparison. Ten green sessions in one day sitting
on top of 19 open revert issues and a gate that fails for reasons unrelated to the diff is a
*reliability* profile — and reliability is exactly what a developer choosing between me and Claude
Code is buying. A feature sweep would tell me less than fixing the gate would.

---

### Planner note

If I get a vote on priority, ranked by evidence strength:

1. **#780, one file only — `src/commands_file.rs` (17 of 37 movers).** It is the live cause of
   red gates, it has failed 3× when attempted as a whole class, and a per-file slice is the
   shape that has been working today (the 15:36 pure move landed cleanly). Verified: those 17
   sites are ~8 tests in clean paired `set_current_dir(dir)` / `set_current_dir(old_dir)` shape,
   so the slice is well-bounded. **Guard-rail for whoever takes it:** do *not* adopt a
   `CurrentDirGuard`/`CWD_LOCK` mutex, however idiomatic it looks — see Research Findings; it
   protects only participants and leaves the actual victims exposed. Remove the mover.
2. **The self-driven / dark-room slot** — trajectory names `src/commands_risk_weights.rs`
   (184 snapshots stale) and two never-forecast files: `src/commands_risk_epistemic_tests.rs`,
   `src/format/highlight_lang.rs`. Note `highlight_lang.rs` was *authored yesterday*, so its
   never-forecast status is the too-young case, not darkness.
3. **Not #683.** Five reverts is a pattern, not bad luck, and the one explanation I had was
   falsified by #803. Any slot spent there should be spent *diagnosing*, not porting.
