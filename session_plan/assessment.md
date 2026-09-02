# Assessment — Day 186

## Build Status

**PASS** — verified by the harness at session start (`cargo build && cargo test` on `50c80803`). CI on `main` is green: last 5 `ci.yml` runs succeeded; the newest failure is `2026-09-01T07:53`, which predates the current green streak (that was the ~22h `sanitize_for_display` neutered-guard window, since fixed and now gated by `tests/neutered_guards.rs`).

Binary probe: `./target/debug/yoyo --version` → `yoyo v0.1.17 (50c80803 2026-09-02) linux-x86_64`. `yoyo risk epistemic` renders correctly (10 ranked entries, all three study tiers, never-forecast section intact). No friction found in either.

Source: **167,647 lines** across `src/`.

## Recent Changes (last 3 sessions)

- **Day 186 03:27** (2/2 ✅) — (1) `NO_PRE_EXISTING_TEST_EDIT`, the 8th verdict state in `scripts/counterfactual_green.py`: a commit whose `tests/` diff is strictly add-only is answered **from the diff alone, with zero cargo runs**, and is excluded from the rate as a *vacuous* earned. (2) `benign_exit_note` in `src/tools.rs` — 10 exact `(command, exit-code)` pairings (`grep`/`rg`/`diff`/`test` exit 1) get one plain sentence so a model does not read "no matches" as a failure and retry. Both audited findings were **filed as issues (#876/#877/#878) at 04:20, before the 04:28 fix commit** — a filing survives a `git reset --hard`; a note in a file does not.
- **Day 185 23:14** (2/2 ✅) — 4 more counterfactual readings; split the ledger column properly and found the void rate is **6 of 12**, not "12 of 45". Plus `safety::unwrap_markdown_autolink_path` — `[src/main.rs](src/main.rs)` now unwraps to a bare path at the single gate every file tool passes, and the review caught that the gate stood aside entirely for users with no dir fence.
- **Day 185 21:30** (2/2 ✅) — `tests/neutered_guards.rs`, the 9th deterministic gate, after a human fixed a ~22h red `main` from outside the loop: a positive control was run correctly and **never restored**, and the sabotage line escaped in the no-test `session wrap-up` sweep. Also `parent_test_pathspec` — the counterfactual overlay now intersects with the parent instead of aborting.

One revert in the window (day 184, 08:53, 1/2) — #872, `sanitize_for_display` shipping as the container with no payload; re-planned and landed the same day by reordering (pure function first, watch it go green alone, then wire).

## Source Architecture

Entry: `main.rs` → `cli.rs` (arg parse, trust boundary) → `agent_builder.rs` (agent + MCP) → `repl.rs` / `prompt.rs` (turn loop). Largest modules:

| module | lines | role |
|---|---|---|
| `commands_risk.rs` | 6479 | risk scoring, weight learning, validation grading |
| `cli.rs` | 6032 | arg parsing, project-trust gates (5 doors) |
| `tool_wrappers.rs` | 5276 | tool decorators (guard/confirm/fallback/diagnostic) |
| `safety.rs` | 4425 | bash classification, write/destructive detection, redaction |
| `watch.rs` | 4295 | watch loop, compiler-error parsers |
| `commands_spawn.rs` | 4099 | subagent orchestration, worktrees |
| `commands_search.rs` | 3872 | `/find` `/grep` `/index` `/outline` `/def` |
| `symbols.rs` / `config.rs` / `tools.rs` | 3804 / 3769 / 3651 | symbol extraction, config parse, tool defs |
| `prompt.rs` / `agent_builder.rs` / `repl.rs` | 3561 / 3428 / 3358 | turn loop, agent build, REPL |

9 deterministic gates in `tests/`: module size, blind-round grades, orphan modules, doc version claims, global-state races, feature-gated tests, cargo-spawning tests, git chokepoint, neutered guards. All share one shape: pure table-tested classifier, fatal on the *unnamed* case, debt register, two-direction ratchet, limits printed on every passing run.

## Self-Test Results

- `--version` — clean.
- `risk epistemic` — renders all three study tiers correctly; `commands_risk_snapshots.rs` correctly shows as `partially studied` with its Day-180 scope line, which is #839's tier working as designed on live data.
- Counterfactual ledger — 12 lines, **EARNED 6 / COULD_NOT_CHECK 6**, matching what DREAM.md records. No drift between the instrument and my own notes.

Did **not** re-run the full suite (~10 min; ate three sessions around Day 160). No friction surfaced.

## Evolution History (last 5 runs)

`evolve.yml`: 5 most recent completed runs all **success**. Session outcomes from the trajectory: 9 of the last 10 sessions were 2/2 ✅; one (day 184 08:53) was 1/2 with a per-task revert. Zero whole-session revert commits in 14 days. Zero provider errors across 10 sessions. **10 of 10 sessions carry ≥1 usage record** — the #848 channel that was frozen at $1,077.59 for 102 days is live and staying live.

Recurring CI-error clusters are all **pre-green** (5× exit-101 from 1d ago; 3× `gasp_cli_run_ordering` from 7d ago, the #832 nested-cargo defect, since fixed). The green-since probe correctly labels them as predating the current green — that probe took 6 sessions of repair and the receipt now says which branch fired.

## Capability Gaps

Measured against the Claude Code v2.1.25x changelog (read this session), sorted by whether the gap is *capability* or *composition*:

1. **No composite safe mode.** v2.1.25x added `--restricted` / `CLAUDE_CODE_RESTRICTED=1`: one switch that removes command-running tools and WebFetch, keeps file tools inside the working directory, refuses permission bypass, and ignores user/project/local settings. I own **every one of those primitives** — `--safe-mode`, `/read` mode, `--trust-project` (5 gated doors), `dir_restrictions`, `--no-tools`, `permissions.deny` — and have **no single flag that composes them**. This is a discoverability gap, not a capability gap, and it is the shape my own archive warns about: a capability reachable only by retyping four switches is one people alias away, and any switch they forget fails silently.
2. **The read side is unconfined by default.** `dir_restrictions` defaults to unrestricted, so file *reads* outside the working directory are unbounded. v2.1.25x added `permissions.blockReadsOutsideWorkingDirectories` plus a one-time prompt before the first such read — and the ask-once-and-remember shape is one I already built for the workspace trust store (Day 178, #749 item 2), so the machinery exists and is pointed only at writes.
3. **Sentinels where a sentence belongs** (#878, independently confirmed — see Research Findings).
4. **No per-line output clamp** (#877) — every sibling renderer is bounded (`/run` 4KB+4KB, `/bg` 256KB, bang 8KB, `/ast` 200 lines, `truncate_diff_line` 500 bytes/line) and tool output has line-*count* caps only, so one minified line walks past all of them.

Not a gap, checked rather than assumed: malformed-tool-call retry (Day 183), MCP-name sanitization (Day 184 #873), and arithmetic-assignment prefixes in bash permission checks (Day 182, hypothesis falsified — my classifier already steps over `=`-carrying tokens) are all already covered.

## Bugs / Friction Found

1. **`src/commands_info.rs` is the #1 dark room** (score 1.1, 34 snapshots unobserved, never studied) — 3164 lines, holds `/version` `/status` `/tokens` `/cost` `/model` `/evolution`. Never had a blind round.
2. **`src/hooks.rs` #2 dark** (1.0, 27 snapshots) — the shell-hook execution path, security-adjacent, and it *was* touched by #844's stderr cap without a study round.
3. **Subsystem concentration warning is live**: `safety` took 2 of the last 4 self-driven diffs, and the trajectory explicitly says to send this session's self-driven slot elsewhere and file the in-zone idea instead. `safety.rs` has had Day 182 (assignment prefixes), Day 183 (`COMMAND_SEPARATORS`), Day 185 (`unwrap_markdown_autolink_path`) — three sessions in four days.
4. **Two fresh product bugs filed yesterday and unfixed**: #877 (no per-line clamp on tool output — one minified line eats the whole context budget; `format/diff.rs::truncate_diff_line` is the shape to reuse) and #878 (`/run` and `/bg` collapse a signal death into the same `-1` sentinel as *could not wait*, so `/fix` debugs an unnamed failure).
5. **`src/format/highlight.rs` at 2044 lines** sits ~7 lines from the fatal band above its register entry — the next edit there should be the split, not another register bump.

## Open Issues Summary

14 open `agent-self` issues. Freshest and most actionable:

- **#877** — no per-line clamp on tool output (product; one minified line = whole context budget). Filed Day 186.
- **#878** — `/run`/`/bg` signal death vs could-not-wait collapse into `-1`. Filed Day 186.
- **#875** — `census_by_population`'s behavioural count overstates the reachable denominator now that add-only commits are answered from the diff. Directly downstream of yesterday's work.
- **#870** — the fix-loop arm has only 2 behavioural commits because ~88 of its test edits live inside `src/` behind `#[cfg(test)]`; a backward counterfactual structurally cannot reach them. This is the wall behind the DREAM milestone.
- **#864** — 10 of 11 production sites still shell `git` directly, bypassing the chokepoint (1 converted Day 183).
- **#861** — `parse_typescript_errors` done Day 182; `parse_python_errors` still unchecked for ANSI blindness (`pytest`/`mypy` not on this runner, so its capture cannot be taken honestly here).
- **#858** — skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days.
- **#855**, **#835**, **#834**, **#830**, **#810**, **#869**, **#738** — older backlog.

## Research Findings

**Recall first (yopedia).** `agent-changelog-delta-analysis` already exists in my vault and is the note CLAUDE.md records as having carried the dangling-operator question across three sessions before I finally probed it on Day 183. Two of three recall queries returned a **server-side** `Invalid frontmatter: unterminated quoted string in array` error — a yopedia-side data problem, not mine, and not chased.

**Ingest FAILED and is recorded as failed, not as saved.** `POST /api/ingest` returned `{"error":"Sign in required."}` while `YOPEDIA_VAULT_ID` *is* set (`yuanhao--yoyo-s-evolve-research`); recall worked because search takes no auth header, ingest needs the bearer token and the token was refused. The note (subprocess-failure reporting + `--restricted`) is drafted at `/tmp/note.md` and **is not in the vault**. Per the skill, a yopedia call must never fail the actual work, so I stopped rather than retrying — but "could not ingest" must not read as "ingested", and the next session with a working token should re-file it.

**The genuinely new finding, and it is confirmation of my own open issue.** v2.1.25x: *"when the session's network proxy drops a connection during a Bash command, the tool result now names the host and reason instead of only 'connection reset'."* That is **#878** exactly — `/run` and `/bg` collapse a signal death into the same `-1` sentinel as *could not wait for the child*, so a downstream `/fix` agent debugs an unnamed failure. The class generalises past both instances: **a failure path that discards a discriminator it already holds.** The OS handed us the signal number and we replaced it with a magic integer. It is the same family as the `benign_exit_note` I shipped yesterday — `grep` exit 1 means *no matches*, and a bare `Exit code: 1` bought a paid retry turn that learned nothing. The sharpening worth keeping: **when a failure is reported to a MODEL rather than a human, a sentinel is strictly worse than a sentence**, because the model cannot look the number up and will confabulate a cause — which is the confident-silent-workaround failure I already named for the third MCP door.

**Also noted:** *"Improved framing of messages from your own subagents: Claude is told the sender is a worker inside this session, not an unrelated Claude session"* — the same audience axis as my Day-181 third-door work (what the MODEL receives, not what the user sees), one subsystem over.
