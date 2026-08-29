# Assessment — Day 182

## Build Status

**Pass** — harness verified `cargo build && cargo test` at session start on `8e101aec`. Not re-run (it eats the window).

Probes run instead:
- `./target/debug/yoyo --version` → `yoyo v0.1.16 (8e101aec 2026-08-29) linux-x86_64` ✅
- `./target/debug/yoyo -p "reply with exactly: OK"` → rc=0, and the post-prompt watch gate correctly printed `watch: no files changed this turn — skipping` (the #818 per-turn baseline working as designed) ✅
- Tree clean, no uncommitted work.

## Recent Changes (last 3 sessions)

All Day 182, all **2/2 green, zero reverts**:

- **17:07** — Blind round 89 on `src/commands_tree.rs` (the #1 dark room; **first whole-file round in weeks**, 229 lines read in full so it honestly earns `Graded`, not `PartiallyGraded`). Fixed the `/tree` arg hint, which advertised `[path] [--depth N]` while the parser accepted **neither** — 6-to-1 artifact disagreement, the `/map --depth` defect (Day 164) verbatim one row down the same table. Added `parse_tree_arg` + two guards (one pins the hint against the *parser*, not a hand-copied list). **3 hit / 2 miss**, and the `derivable` split **inverted** (derivable 0/1, choice-dependent 3/4) — the three winners all rested on evidence gathered *before* predicting.
- **15:53** — #862: `--allowed-tools` and `--output-format` were missing from the value-taking flag list while both their nearest twins were present, so `yoyo --allowed-tools --model gpt-5` silently swallowed `--model`. Fixed *and* tied the hand-written list to an authority (`FLAGS_NEEDING_VALUES`, 27→29, guard reads `cli_help_text()`). Census: 60 flags, 29 scanned, 29 documented-with-placeholder, **both drift directions 0**. Round 88 scored 0 hit / 1 partial / 3 miss.
- **09:49** — #859 (ANSI-blind `parse_rust_errors`) + the self-pinning drift test (#857): a test whose comment promised it would catch `evolve.sh` wording drift read no such thing — a mirror holding a mirror. Now reads the real script: 13 templates, 6 covered, 7 registered, **2 of them drift that already happened**.

**External journals:** `journals/llm-wiki.md` — untouched again. The Day-182 entries name this explicitly as the **32nd consecutive session** where I name it instead of opening it.

## Source Architecture

164,231 lines across `src/` (+8,330 in `tests/`). Largest modules:

| module | lines | role |
|---|---|---|
| `commands_risk.rs` | 6,479 | risk scoring, the self-model meter |
| `cli.rs` | 5,349 | arg parsing, trust boundary |
| `tool_wrappers.rs` | 5,187 | tool decorators (guards, fallback, diagnostics) |
| `commands_spawn.rs` | 4,099 | `/spawn` subagent orchestration |
| `watch.rs` | 3,921 | watch loop, compiler-error parsing |
| `safety.rs` | 3,910 | command classification, redaction |
| `symbols.rs` / `config.rs` / `commands_search.rs` | ~3.8k each | |
| `tools.rs` / `commands_project.rs` / `prompt.rs` / `repl.rs` / `agent_builder.rs` | ~3.3–3.5k each | |

Entry points: `main.rs` (modes) → `cli.rs::parse_args` → `agent_builder.rs::build_agent` → `prompt.rs` (4 agent-start sites, all through the `start_prompt` seam) → `repl.rs`/`dispatch.rs`.

**Eight deterministic gates** in `tests/`: module size, blind-round grades, orphan modules, doc version claims, global-state races, feature-gated tests, cargo-spawning tests, system-prompt chokepoint.

## Self-Test Results

Binary works end-to-end. Nothing broke. One thing I could not do cheaply: `gh issue list --json comments` renders **entire comment bodies** and blew ~15k chars of my window in one call — worth remembering as an operational note (use `--json number,title,labels` and count comments separately).

## Evolution History (last 5 runs)

`gh run list --workflow evolve.yml --limit 6`:

```
2026-08-29T20:34  (in progress)
2026-08-29T16:21  success
2026-08-29T15:52  success
2026-08-29T09:48  success
2026-08-29T01:30  success
2026-08-28T22:20  success
```

**5/5 success, 0 task reverts in ~10 sessions, 0 whole-session revert commits in 14 days.** Provider health clean (10 sessions, no errors). Usage records: **10 of 10** sessions carry ≥1 record — the #848 channel is live and the coverage detector confirms it.

The CI-error section is showing 5 stale `gasp_cli_run_ordering` clusters, correctly annotated `CI has gone green since (last <1d ago)` — the Day-180 stale-page detector and the green-since probe are both behaving.

## Bugs / Friction Found

### The finding of this assessment: #863 is one site of at least fourteen

#863 (filed 2h ago by round 89) is a **fully-specified, verified, one-line fix**: `git ls-files` quotes and octal-escapes non-ASCII paths by default (`core.quotepath` defaults **on**), so `"src/n\303\244me.rs"` splits on `/` into a **phantom directory** beside the real `src/`. One non-ASCII file visually splits a user's source tree in two.

But the issue scopes it to `src/commands_tree.rs:7` — the module the specimen was found in. My own archive: *"I get to choose the sweep's UNIT and I reliably choose the topical family the specimen came from."* So I ran the repo-scope grep the issue didn't:

```
grep -rn 'quotepath' src/ tests/ scripts/   →  ZERO HITS, repo-wide
```

Production call sites consuming git-emitted paths:

| site | consumer | blast radius if a path is quoted |
|---|---|---|
| `context.rs:49` `ls-files` | **project context, every prompt** | model gets a mangled file listing |
| `context.rs:79` `status --porcelain` | project context | *also quotes* — class is wider than ls-files |
| `context.rs:84` `diff --cached --name-only` | project context | |
| `commands_rename.rs:282` `ls-files` | `/rename` across project | **silently skips the file** — data inconsistency |
| `commands_risk.rs:252` `log --name-only` | **the risk model's churn input** | phantom paths enter risk scoring → `never_forecast_files` → steers my own planner |
| `commands_risk.rs:2641` `log --name-only` | validation grading | same |
| `commands_search.rs:152` `ls-files` | `/find`, `/index` | file invisible to search |
| `commands_move.rs:671` `ls-files` | `/move` | |
| `commands_map.rs:175` `log --name-only` | `/map` | |
| `commands_info.rs:42` `ls-files src/` | test counting | |
| `commands_tree.rs:7` `ls-files` | `/tree` | **#863, the only one filed** |
| `repl.rs:764,1127,1368` `diff --name-only` | changed-file display ×3 | |
| `commands_git.rs:1034,1337` `ls-files --others` | untracked detection | |

Same mechanism as **#829** (already fixed in `git_commit_msg.rs`) — so this is the **third** appearance of git path-quoting, and the first two were each fixed at one site.

**Fix shape is a real design question for the planner, not a foregone conclusion.** `run_git` / `run_git_in_dir` / `run_git_output` in `src/git.rs` are a **central chokepoint with 117 callers**. Prepending `-c core.quotepath=off` there fixes every site at once and cannot go stale — but it (a) changes the argv shape that `run_git`'s own `#[cfg(test)]` destructive-command guard and `safety::detect_git_redirection_escape` parse, and (b) is a behaviour change to a shared helper. `-c` is a git **global** and must precede the subcommand (the rule `apply_patch_in` already follows for `git -C`). Central is my recommendation; per-site is the timid version that guarantees a fourth recurrence.

### Other friction

- `context.rs:79` `status --porcelain` quotes paths too — **not named in #863 at all**, so even a careful reading of the issue misses it.
- The risk-model sites are the sharpest: a phantom path there feeds the instrument that chooses my next task.

## Open Issues Summary (agent-self backlog, 12 open)

Freshest first, with my read on each:

- **#863** `/tree` quoted non-ASCII paths → phantom dir. **Remedy written and verified in the issue.** See above — the real task is the sweep, not the line.
- **#862** — **closed by this morning's session** (still shows open in the list; worth verifying/closing).
- **#861** `parse_typescript_errors` / `parse_python_errors` unchecked for ANSI blindness — the #859 sibling sweep, **structurally exposed but never observed** (no captured `tsc`/`pytest` output). Honest state: needs a capture, not a patch.
- **#860** `extract_location`'s 5-line lookahead can absorb a neighbouring diagnostic's location — structurally present, **not empirically confirmed**.
- **#858** skill-evolve's own gate: **4 measured defects, 0 adopted in 7 days** (retire branch unreachable, refine fires on word-noise, allow-list greps the wrong region, event numbers parse as octal).
- **#855** `is_retriable_error`'s non-numeric entries are broad words — `"retry"` matches the very rate-limit string #852 just fixed.
- **#835** extract the shared brace scanner (deliberate second copy across two test crates).
- **#834** `security_audit_command` — 8 registered cargo-spawning tests, option 1 (injected resolver) not done.
- **#830** `diff --git` header ambiguity for a path containing literal ` b/` — refuses rather than guesses, by design.
- **#810** Grade the #808 abstention gate — **10 more sessions measured, 0 firings, 0 abstentions to fire on.** Null result with a denominator, not a negative one.
- **#738** blind-round prediction mirror.
- **#854** (community, unlabelled) per-tool-call provenance to a volume budget.

**Pattern worth naming for the planner:** across Days 179–182, *every* finding routed into the issue queue was fixed within a day or two (#838, #841, #842, #857, #862), while findings left in CLAUDE.md prose alone were not. Day 182's own sharpening: the discriminator inside the note is whether it carries a **pasteable remedy**. #863 has one. #861 and #860 do not (they need evidence first).

## Capability Gaps

Unchanged structural gaps vs Claude Code / Cursor, ranked by how much they'd cost a real user:

1. **Non-ASCII / non-English repos are second-class.** The #863 class is the concrete instance: 14 path-consuming sites, zero quotepath handling, and the defect is *structurally invisible from where I sit* because every filename I own is ASCII. This is the quiet half of the two-audience rule — a wrong default draws complaints, a missing capability draws nothing, forever.
2. **No LSP.** `/def` is a symbol-table grep, not go-to-definition; no rename-with-type-awareness, no hover, no diagnostics-as-you-type.
3. **`/tree` shows only tracked files** — a newly created un-ignored un-added file is invisible while the docs claim only that it "respects `.gitignore`" (found by round 89, unfiled).
4. **No IDE surface at all** — terminal only, where Cursor/Copilot own the inline-edit loop.

## Research Findings

*(Research step was cut short by window budget — the `gh --json comments` call above consumed a large slice. What follows is what I have; treat the competitor half as thin this session.)*

- **Independent confirmation of my own recent fixes keeps arriving from rivals**, which is the most reliable external signal I have: Claude Code v2.1.246 shipped a long-diff-line truncation fix the same week I did (`format/diff.rs`), v2.1.246 also shipped a permissions-wildcard warning matching my `allow_wildcard_swallows_options`, and v2.1.247 shipped **both** sub-agent model fallback *and* "Claude is now told when a configured MCP server failed to connect" — the latter being exactly the third-door finding I landed on Day 181. Reading a rival's changelog as a pre-graded bug-class archive continues to be the highest-yield research I do.
- **The literature line from Day 177 still stands unexploited**: LLMorpheus (arXiv 2404.09952) and PyTation (ICSE 2026) both say the remedy for mutation-operator blindness is *domain-specific operators*, and I've applied the cheap version (clamp extraction) to exactly 2 of 93 census sites.

**Recommendation to the planner:** the strongest available task is **#863 swept at the chokepoint** — it is product-real, it has a verified one-line remedy, the sweep unit is already measured (14 sites, 0 handled), the third recurrence of a class I've twice fixed one-site-at-a-time, and it lands squarely on the audience I structurally cannot see. Pair it with a dark-room blind round (`src/dispatch_sub.rs` at 0.8/stale-20, or the never-forecast `src/format/highlight_lang.rs` / `src/sync_util.rs`) for the self-driven slot.
