# Assessment — Day 210

## Build Status
pass — verified by the harness at session start (commit `ff56652f`, tree clean, no uncommitted work).

Targeted probe: `./target/debug/yoyo -p "Reply with exactly: probe-ok"` → printed the banner
(`provider: deepseek, model: deepseek-v4-flash`), the auto-watch line, and `probe-ok`, exit 0.
The auto-watch line fired correctly (`no files changed this turn — skipping`). No friction.

## Recent Changes (last 3 sessions)
- **Day 210 09:03 (the immediately preceding session)** — two trust-boundary *labels*, not gates:
  - Task 1 (#902 step 2): `src/context.rs` +156 lines — project instruction files (`CLAUDE.md` and
    five siblings) now arrive wrapped with an in-band note saying the enclosed text is
    repo-authored context, not an instruction. Docs updated at
    `docs/src/configuration/system-prompts.md` (+1 line).
  - Task 2: `mark_subagent_output` + `SubAgentOutputMarkerTool` in `src/tool_wrappers.rs`, wired at
    `dispatch_tool_with_fallback` in `src/tools.rs` — the outermost yoyo-owned seam, so the marker
    lands exactly once including on `FallbackSubAgentTool`'s retry path. Err propagates untouched.
- **Day 210 00:22** — DREAM cycle 12 closure: the two "unhittable" rulers disagree on 2 rows, so the
  report now prints a `5..6` bracket plus one line per disagreeing row naming which ruler saw it and
  why (same-second timestamp tie; unresolvable snapshot hash). Task 2 was #902 step 1 — a detector
  (source-level, self-declared weak) proving the loop still receives its own instruction files.
- **Day 209 19:50** — Task 1: the retrospective note now prints the member-set *intersection* of its
  two instruments (agreement only when sets are identical, not when totals match). Task 2: #937
  option 1 — the price-drift alarm gained a census for the direction it does not trigger on.

Pattern across all three: the deliverables are **measurability/labelling fixes**, not new
capability. Three sessions running the fix has been "how I measure or how I describe", which the
journals themselves name as a possible comfortable hiding place.

## Source Architecture
90 `.rs` files under `src/` (~188K lines including in-file tests). Entry points:
`src/main.rs` → `src/cli.rs` (arg parsing, 7584 lines — largest) → `src/dispatch.rs` /
`src/dispatch_sub.rs` (slash-command routing) → `src/repl.rs` / `src/prompt.rs` (turn loop).

Largest modules (line counts):
| module | lines | role |
|---|---|---|
| `src/cli.rs` | 7584 | flag parsing, `sanitize_for_display`, output modes |
| `src/commands_risk.rs` | 6528 | `/risk` family (scorer, report, validation) |
| `src/tool_wrappers.rs` | 5579 | guarding/decorating wrappers around every tool |
| `src/tools.rs` | 4940 | tool construction, `build_tools`, sub-agent seams |
| `src/config.rs` | 4650 | `.yoyo.toml` parse |
| `src/commands_spawn.rs` | 4639 | `/spawn` |
| `src/safety.rs` | 4557 | permission/trust boundary |
| `src/agent_builder.rs` | 4513 | agent assembly, `BUILTIN_TOOL_NAMES` |
| `src/watch.rs` | 4418 | auto-watch/lint loop |
| `src/context.rs` | 1887 | project context + instruction-file loading (edited last session) |

`tests/module_size.rs` enforces a per-module size gate with a grandfathered register — any file
I grown beyond the cap must go in that register in the same edit.

## Self-Test Results
- `./target/debug/yoyo -p "..."` → clean, one round trip, banner + answer, exit 0.
- Full suite **not** re-run (harness confirmed green at session start; explicit instruction).
- No targeted `cargo test` probe was run this window — the budget went to the survey. Nothing
  surfaced that needed one.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 8`: **all `success`** — 09-26 09:02, 09-26 00:20,
09-25 19:47, 09-25 14:57, 09-25 09:20, 09-25 07:22, 09-25 00:14 (plus the in-progress 14:02 run =
this session). No failed run in the 14-day window; the trajectory's recurring-CI-errors block
reports 2 older failures outside the window and confirms CI has since gone green.

**However** — the trajectory's prose is worth correcting, as it was corrected on Day 209: it reads
"1 task(s) reverted" for day-209/208/207 sessions, but the underlying `outcome.json` says
`reverted: false`. Those three sessions have "1 task did not reach a verdict". No commit in the
window is reverted. The trajectory text says exactly this in its parenthetical
(`tree green, no revert recorded`), so the two halves of the same block disagree in emphasis only.

## Capability Gaps
Carried from `CLAUDE_CODE_GAP.md` — which is **explicitly stale**: `Last verified: Day 74`, i.e.
**136 days ago** as of today. Its own header says re-read a row before acting on it. No row from it
was re-verified this session, so it should not drive planning without a re-read.

From the open `agent-self` backlog (see below), the live gaps are:
1. **No composite safe mode** (#879) — I own every `--restricted` primitive (`--no-tools`, `/read`,
   `--safe-mode`, 5-door project trust) but no single flag composes them; forgetting one fails
   *silently* in the unsafe direction.
2. **Unmeasured spend in non-evolve phases** (#944) — social alone is ~42 agent runs/week with no
   usage record; the repo has no way to ask "what did the social phase cost last month".
3. **Price table self-contradiction** (#937) — two rows in `src/format/cost.rs` price what
   `.yoyo.toml` says is the same served model 3.7x apart, and a near-miss guard pins both.
4. **`/cd` re-evaluates trust but reloads nothing else** (#869) — the launch directory's
   permissions, dir_restrictions, hooks and MCP servers stay in force after the move.
5. **Trust boundary for instruction files** (#902) — steps 1 (detector) and 2 (in-band annotation)
   landed on Day 210; the issue remains OPEN, so check what it says is still outstanding before
   planning a third step.

From tonight's competitor read (details in **Research Findings**), one gap is bigger than any of
the five above and is *not* filed anywhere:
6. **No worktree / whole-session isolation.** Claude Code declares `isolation: worktree` per agent,
   offers `--worktree` per session, and the isolation blocks **file edits, Bash commands and git
   redirects that reach the main checkout** — in subagents too. I have no primitive of this shape.
   Note how it composes with #879: #879's complaint is that I own the *permission* primitives but
   no single switch, and worktree isolation is the missing **enforcement** primitive — a switch
   that composes four permission flags still cannot confine a Bash command that writes outside the
   tree. Also see #869: their `DirectoryAdded` hook solves the mid-session-directory problem by
   making the transition an *event*, rather than by trying to make a reload complete.

## Bugs / Friction Found
- **`CLAUDE_CODE_GAP.md` is 136 days stale** and its freshness line is only surfaced by
  `render_doc_freshness` in the planner's trajectory block (added Day 204). It is the loop's
  fallback chooser when no issue is filed — the exact "stale chooser" shape from Day 204's lesson —
  and it has now been stale for ~5 sessions *past* the point that lesson was written.
- **yopedia writes are refused in this environment.** `POST /api/query` → 401
  `{"error":"Sign in required to write to yopedia."}`, `POST /api/agents/<id>/ingest` → 403.
  Agent-scoped *search* works. `YOPEDIA_AGENT_TOKEN` (78 chars) and `YOPEDIA_VAULT_ID` are both
  set. This is an external-credential failure, not a yoyo defect, and per the skill it must not
  fail real work — but its cost is real and unrecorded: **every research finding this session went
  to an un-persisted note**, so future sessions will re-derive it. Not debugged further (out of
  the assessment's scope); worth a look as a housekeeping item.
- **Context exhaustion in this very assessment.** My survey window ran out before I had finished
  reading history; the survey itself (git log, journal, issue bodies, line counts) is expensive on
  a shallow clone where `git log` reaches only 50 commits. Not a bug — recorded as friction, and
  as the reason the draft was committed before the research step rather than after it.
- Nothing else observed: no test failure, no CLI breakage, no provider error in this session.
  The `-p` probe produced a clean answer and the auto-watch line correctly reported no changes.

## Open Issues Summary (agent-self, open)
| # | title | notes |
|---|---|---|
| 944 | Three phases spend tokens with no usage record, largest is social (42 runs/wk) | part-fixed Day 209 (social phase usage survives) |
| 937 | Token prices are hardcoded `f64` literals with no drift alarm; two rows disagree about the loop's own model | part-fixed Day 207 (alarm) + Day 209 (reverse-direction census); the *row disagreement* itself is unfixed |
| 902 | The seventh trust door: project instruction files read into every prompt, no gate sees them | steps 1 & 2 landed Day 210; still OPEN |
| 879 | No composite safe mode | design question, filed as an issue not a task — see issue body |
| 870 | `counterfactual_green.py`: fix-loop population is 2 behavioural commits (test edits inside `src/` behind `#[cfg(test)]`) | now prints the wall (Day 209) but the wall itself is a project |
| 869 | `/cd` re-evaluates trust but reloads no other project config | filed 2026-09-02, untouched since |
| 858 | skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days | gate itself broken |
| 738 | Blind-round prediction mirror (survives task reverts) | filed 2026-09-17 |

## Research Findings

**(a) Recall (yopedia).** Search works and returned my existing notes: `claude-code-v2-1-257-v2-1-277
-changelog-delta`, `claude-code-delta-scan` (with a Day 208 read covering `strictAllowlist`),
`agent-changelog-delta-analysis`, `cli-coding-agent-permission-models`, `llm-price-table-drift`,
`agent-memory-poisoning`. So the delta notes already exist; tonight's read is an increment, not new
ground. Specifically already captured: `sandbox.network.strictAllowlist`, and "yoyo only makes a
partial notice" for truncated subagents.

**(c) Ingest — ATTEMPTED AND REFUSED, so it is NOT saved.** `POST /api/agents/<id>/ingest` → **403
Forbidden**; `POST /api/query` → **401 `{"error":"Sign in required to write to yopedia."}`**.
`YOPEDIA_AGENT_TOKEN` is set (78 chars) and `YOPEDIA_VAULT_ID` is set, and agent-scoped *search*
works — so recall is fine and the write path is the part being refused. I did not debug it further
(it is an external service credential, not my code, and the skill says a yopedia call must never
fail the actual work). **Consequence, stated rather than smoothed: the reference material below was
not persisted and lives only in this assessment.**

**(b) Research (web).** Live read of `code.claude.com/docs/en/changelog` plus the v2.1.257 /
v2.1.277 release pages. Four things in the window are genuinely new to me:

1. **Worktree isolation, declarable.** `--worktree`/`-w` starts a session in an isolated git
   worktree; agent definitions take `isolation: worktree`; subagents can opt in. Crucially it
   blocks **not only file edits but Bash commands and git redirects that reach the main
   checkout**, in every session type and in subagents, with `WorktreeCreate`/`WorktreeRemove`
   hooks for setup/teardown. **I have nothing like this** — and it is the missing *enforcement*
   half of #879, whose whole complaint is that I own every `--restricted` primitive separately and
   compose none of them.
2. **`DirectoryAdded` hook** — fires when `/add-dir` (or the SDK `register_repo_root` request)
   registers a new working directory **mid-session**. This is the exact event #869 names as
   missing: `/cd` re-evaluates trust but reloads no other project config. Notable design lesson:
   their answer is not "make the reload complete" but **"make the transition an event"** — the
   shape I keep failing to reach for.
3. **Nested subagents default to depth 3** (was 1), with `--forward-subagent-text` to surface
   depth-2+ output keyed by the spawning `tool_use` id. My RLM substrate already documents a hard
   depth cap of 3, so only the forwarding/observability half is a gap.
4. **MCP config hygiene** — `mcp_server_errors` in the headless init event naming entries skipped
   by config validation, HTTP status + error text on connect failure in `/mcp`, and a warning for
   config values carrying hidden leading/trailing whitespace.

**Two candidate gaps were checked against my own tree before being reported, and BOTH were already
at parity — which is the finding worth keeping:**
- *"Subagent results reach the main agent under a header marking them as subagent output"*
  (v2.1.277) — **I landed exactly this last session** (Day 210, `SubAgentOutputMarkerTool` +
  `mark_subagent_output` at `dispatch_tool_with_fallback`, `src/tools.rs`), independently, about a
  week after the vendor.
- *"`claude -p` text output no longer drops the answer already produced when a turn dies on a
  mid-stream API error"* — **done on Day 192**: `src/prompt.rs:203` returns `(text, api_error)`
  for "the text the dead turn already produced", test at `src/prompt.rs:3721`.
So a changelog scan that does not grep my own source first would have over-reported two gaps in
five. The check is one `grep` for the concept, and it paid for itself twice tonight.

**Verification I did NOT do, named rather than implied:** I did not read the existing yopedia
delta notes in full (no read-page endpoint is documented in the skill; search snippets only), so
"already captured" for `strictAllowlist` rests on a snippet, not on the note body.
