Title: #902 slice — the spawned worker's `load_project_context()` is a SECOND door around the parent's `--safe-mode`/`--restricted` decision. MEASURE it, then route the parent's answer in.
Kind: product
Files: src/commands_spawn.rs (the direct `load_project_context()` call sites, reported at ~:810 and ~:961), tests/module_size.rs (register line, pasted from what the gate prints), ARCHITECTURE.md (write-up — NOT CLAUDE.md)
Issue: #902

## The gap, quoted from my own issue (which named it and could not settle it)

> "**Measured caveat worth its own line:** `commands_spawn.rs:810` and `:961` call
> `load_project_context()` **directly**, outside `cli.rs`'s safe-mode branch. Whether a
> `--safe-mode` parent's spawned worker still loads project instructions was **not
> verified** and should be part of the design pass."

That is the whole task: **settle it, then close the door** if it is open. `cli.rs`
loads project context only when `!safe_mode` (`safe_mode = --restricted || --safe-mode`,
`src/cli.rs`, the two-line branch the issue quotes at `:2443-2449`). Two call sites
outside that branch mean the first thing a confined session does — spawn a worker —
may re-load the very text the confinement was meant to keep out. This is the
"two doors, one policy, one deaf" class, and it is the *same* policy (project-authored
text entering the prompt) that the whole of #902 is about.

## Step 1 — MEASURE, and write down which branch fired

Do not assume the door is open. Read both call sites and establish:
- which process runs them (the parent building the child's prompt, or the child);
- what `load_project_context()` returns there (the full context, or only the
  instruction-file half);
- whether any existing predicate (`cli::is_restricted`, the `--safe-mode` flag, the
  trust store) is already consulted on that path.

If cheap, corroborate by running the binary with and without `--safe-mode` from a
directory carrying one instruction file, and report the observed difference.

**Both outcomes are valid deliverables and must be reported as measured, not inferred:**
if the worker already inherits the parent's decision, the task becomes the pinning
test plus the ARCHITECTURE.md line saying so — **do not invent a change to justify the
task**, and say plainly in the write-up that the reading came out clean.

## Step 2 — route the parent's decision through the ONE predicate that already exists

If the measurement shows the door is open: the two call sites must consult the
**existing** safe-mode predicate that `cli.rs` already uses — read it, do not
re-spell it (a second copy of the rule is the defect wearing a fix's clothes). Prefer
passing the decision **explicitly** (a `bool` parameter or a pure predicate called at
the call site) over re-reading a process-global deep in a spawn path, and read the
value where the flags are still in scope.

Bounds:
- **Normal spawns stay byte-identical.** Every user who has not passed `--safe-mode`
  or `--restricted` must see exactly the context they see today — that is the whole
  regression surface and it is pinned with whole-string `assert_eq!`, not a `contains`.
- This **narrows only**. A spawned worker may never gain context its parent suppressed;
  no path may ever lift a fence.
- Do not change what `--safe-mode` *is* and do not touch the sixth/remaining design
  questions in #902 (which predicate for instruction files, refuse vs annotate in band,
  `project_trust_grants`). This fixes **reach**, not policy.

## Step 3 — test at the seam, anti-vacuous first

Keep it pure or source-level; `commands_spawn` spawns real processes, so a weak
source-level guard with the reason written in its doc comment is the accepted shape
(the `fetch_mcp_tool_names_retrying` / `connect_external_servers` precedent).
Assert in this order:
1. **anti-vacuous**: the fixture/diff genuinely carries a project-context load, so the
   guard cannot pass by there being nothing to gate;
2. confined → the load is skipped/refused, named;
3. **near-miss, the direction that matters**: unconfined → the same call site loads
   exactly what it loaded before, asserted as byte-identity.
If the assertion could be satisfied by a dead branch, it is an absence assertion — pair
it with a presence assertion (Day-191 rule).

## Step 4 — write it down and keep the gates green

- **ARCHITECTURE.md**, in the `commands_spawn.rs` entry: what was measured, which
  branch fired, the fix or the clean reading, the test names, and the stated limit
  (this is one door — it says nothing about the other `load_project_context()` callers
  or about the remaining #902 design questions, which stay open).
- `tests/module_size.rs`: `src/commands_spawn.rs` is grandfathered — **paste the number
  the gate itself printed**, after running `cargo fmt`, rather than hand-typing it.
- Run `cargo fmt`, then `cargo build && cargo test`, then explicitly
  `cargo clippy --all-targets -- -D warnings`. Do not delete or weaken an existing test.

`Kind: product` — a user running yoyo in a repo they do not trust is the person this
serves.
