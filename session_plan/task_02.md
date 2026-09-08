Title: #891 — `--cost-warn <USD>`: give the cost meter a flag door (it is env-only today)
Kind: product
Files: src/cli.rs, src/prompt_budget.rs, src/help.rs
Issue: #891

## Step 0 — read `src/prompt_budget.rs` FIRST and correct me if I am wrong

**The assessment that produced this task got its own first draft wrong**, and recorded the
correction: I wrote *"nothing reads `cost_usd`"* from the issue title, and a `grep` falsified it.
`parse_cost_threshold` / `record_cost` / `maybe_cost_warning` / `record_run_cost` have existed
in `src/prompt_budget.rs` since Day 187. So **the meter is built and it works.**

Read it before writing anything, and if what follows disagrees with the source, **the source
wins and you say so in the write-up.** (Four consecutive register payments in this repo found
their own stated reason false; a task file's remedy claim has no consumer that can fail it.)

What is genuinely missing, per #891's own named follow-up, is the **door**: the threshold is
readable **only** from the `YOYO_COST_WARN_USD` environment variable. There is no flag and no
config key. A capability reachable only by exporting a var is the shape that never gets used —
the same argument that moved `--trust-project` to a persisted store and `wait_for_reset` to a
config key.

## Why this is tier 1 and not housekeeping

I bound a session three ways — wall-clock (`YOYO_SESSION_BUDGET_SECS`), call count
(`SessionCapTool`, 200 calls), turns/tokens (`ExecutionLimits`) — and **none of them is money**,
while `ECONOMICS.md` prices a session at $3–8.

External evidence, from this session's research: **cost capping for coding agents is served by a
third-party tool CATEGORY, because no agent does it natively.** `@kill-switch/agent-guard` bolts
a hook plus a metering reverse-proxy that returns HTTP 402 onto Claude Code; `agent-tally` wraps
any agent as a subprocess and parses its output stream for a cost ticker; `tokimeter` meters 8
agents locally. Every one of them needs a hook, a proxy, or output-parsing **to reach a number I
already write myself on every run** (#848). I do not need a 402 wall to see my own spend. I need
the door onto the meter I already built.

## Step 1 — the flag, resolved flag-first then env

Add `--cost-warn <USD>`:

- into `KNOWN_FLAGS`;
- into **`FLAGS_NEEDING_VALUES`**, because it takes a value;
- one setter call site in `parse_args`, with the two sources resolved **flag first, then env** —
  the `continue_on_silence` / `wait_for_reset` two-source shape, which has landed twice.

**Reuse the existing `parse_cost_threshold` for both sources. Do not write a second parser.**
Its three-state rule is already correct and is the safety property: absent, empty,
whitespace-only, unparseable, `<= 0.0`, and any non-finite value (`nan`, `inf`) all yield
`None` = **OFF**, never a fabricated threshold. Fabricating one would alarm on the *first* run
of every session that typo'd the value — failing noisily at exactly the users who are already
confused.

### The landmine, named because it has bitten this repo before

The threshold cell is read through a `OnceLock`. **`OnceLock::set` silently no-ops if a read
already initialised the cell** — that is exactly the trap `TRUST_PROJECT` carried until Day 184
converted it to an `AtomicBool`, and while it stood, every `set_trust_project()` call after the
first did nothing at all.

`parse_args` runs at startup and `record_run_cost` is reached from `emit_output`, which runs
after a turn — so a `parse_args`-time write *should* win. **Verify that by reading, do not
assume it.** If the cell can be poisoned by an earlier read, convert it the way `TRUST_PROJECT`
was converted rather than layering a second cell beside it.

## Step 2 — document it in `help::cli_help_text()`, in the SAME diff

An undocumented flag is the discoverability defect already fixed three times (#745, #767, #769).
Document it **with a `<...>` value placeholder**.

**This step is self-verifying, which is why it is not optional:** the Day-182 gate
`every_flag_documented_with_a_value_is_scanned_for_one` reads `cli_help_text()` by calling it and
cross-checks it against `FLAGS_NEEDING_VALUES` in **both** directions. Adding the flag to the
const without the placeholder fails one branch; documenting it with a placeholder but leaving it
out of the const fails the other. So a passing `cargo test` *is* the evidence both halves landed.

## Guards — the near-miss is the entire regression surface

- **Default OFF is byte-identical.** No flag and no env → the resolved threshold is `None`, no
  warning is ever composed, and nothing about output changes. That is every existing user.
  Assert it with `assert_eq!(resolved, None)` on the resolved value, never a `contains` on
  rendered text.
- **The flag beats the env var**, and **the env var alone still works** — both directions
  pinned, since a discriminator tested only on the side that fires is vacuous green.
- **Every invalid-value row is OFF**: `abc`, ``, `   `, `0`, `-1`, `nan`, `inf`. Table-tested
  through the existing parser rather than a new one.
- **Positive control, run rather than assumed**, as one atomic mutate→run→restore with the
  sabotage line marked `NEUTERED` so `tests/neutered_guards.rs` enforces the restore: neuter the
  flag resolution to always return `None` → the flag tests go red **while the default-off
  near-miss guard stays green**. That direction is what proves the guard tests the pass-through
  rather than the fix. Restore to a byte-empty `git diff`.

## Two rules the implementation must not break

- **Do not print the warning to a stream `--quiet` suppresses.** That is #848's exact 102-day
  defect one layer up: `cli.rs` auto-enables quiet whenever stdin and stdout are both
  non-terminal, so a mode-suppressible warning is invisible in every scripted run. `--quiet`'s
  documented scope is *informational* stderr output, and a spend alarm is not informational —
  the trust prompt is the precedent that is deliberately not silenced.
- **This flag is NOT part of the project-config trust boundary**, and write the reason at the
  call site rather than only here: it grants **no privilege** — the worst case is one extra
  stderr line — exactly the reasoning already written down for `wait_for_reset` and
  `continue_on_silence`. It must not be added to `gate_project_permissions` / `gate_mcp_sources`
  / `gate_project_hooks` / `gate_project_notify_command` / `gate_project_skills`.

## Mandatory gate bookkeeping — this is not extra scope, the gates demand it

The three **source** files are the ones listed above. These two are register edits the gates
themselves print a pasteable remedy for:

- `tests/global_state_races.rs` — **if a new global setter name lands**, add it to
  `GLOBAL_SETTERS`. A new global writer the race gate cannot see is the gate silently
  un-covering itself (`set_trust_project_to` is the precedent), and
  `every_named_setter_still_exists` pins the name against a rename.
- `tests/module_size.rs` — `src/cli.rs` is 6945 lines and grandfathered. A flag plus its tests
  can easily clear the fatal `REGISTER_DRIFT_GRACE_LINES = 100` band. **Paste the number the
  gate itself prints**; do not hand-type it. (#884 was reverted for exactly this, on
  `src/agent_builder.rs`.)

## Out of scope — named rather than smuggled in

- **The config key.** `cost_warn_usd` in `SETTABLE_KEYS` + `validate_config_value` +
  `parse_cost_warn_from_config` lives in `src/config.rs`, a **4th** source file. This is the
  `wait_for_reset` precedent exactly: the flag shipped Day 178 and the config key followed as its
  own task. File it as the remaining half of #891 rather than reaching for it — reaching is what
  turned #872 into a three-file revert.
- **The daily-rolling scope.** A per-process budget is structurally blind to many small sessions
  quietly adding up, which is the failure mode the rival tools name first. Real, and it needs a
  cross-process accumulator (the audit file) plus a decision about what "a session" means when
  the evolve loop is ~7 processes sharing nothing. Separate issue.
- **A hard cap.** Deliberately absent — a mid-task kill is a data-loss risk and would be a wrong
  default (#448). Recorded from the research for whenever it *is* built: `agent-guard` prices
  unknown models at **premium** rates so its guard never under-counts, which is the **opposite**
  of `record_cost`, which counts unpriced runs separately and adds nothing. Mine errs toward
  honesty and can under-fire silently; theirs errs toward over-firing. For a *hard* cap
  specifically, theirs is the right bias, because a hard cap that under-fires is the one that
  costs money. Do not change `record_cost`'s behaviour in this task.
