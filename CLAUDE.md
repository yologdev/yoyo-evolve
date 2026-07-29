# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A self-evolving coding agent CLI built on [yoagent](https://github.com/yologdev/yoagent). The agent spans multiple Rust source files under `src/`. A GitHub Actions cron job (`scripts/evolve.sh`) runs the agent hourly using a 3-phase pipeline (plan → implement → respond), which reads its own source, picks improvements, implements them, and commits — if tests pass. All runs use a flat 8h gap (~3/day). Sponsors get benefit tiers (issue priority, shoutout issues) but no run-frequency speedup. Every sponsor, any amount, is listed in README.md permanently — listings never expire and are never pruned (creator decision 2026-07-13; the old accelerated-run credits and the 90-day listing/grace windows are retired).

**Sponsor benefit tiers:**

All sponsors (any amount, recurring or one-time): permanent README listing.

Monthly recurring:
- $5/mo: Issue priority (💖)
- $10/mo: Priority + shoutout issue

One-time (cumulative):
- $5: Issue priority (14 days)
- $10: Above + shoutout issue (30 days)
- $1,000 💎 Genesis: Permanent priority + top billing + journal acknowledgment

## Build & Test Commands

```bash
cargo build              # Build
cargo test               # Run tests
cargo clippy --all-targets -- -D warnings   # Lint (CI treats warnings as errors)
cargo fmt -- --check     # Format check
cargo fmt                # Auto-format
```

CI runs all four checks (build, test, clippy with -D warnings, fmt check) on PR to main. A separate Pages workflow builds and deploys the website on push to main.

To run the agent interactively:
```bash
ANTHROPIC_API_KEY=sk-... cargo run
ANTHROPIC_API_KEY=sk-... cargo run -- --model claude-opus-4-6 --skills ./skills
```

To trigger a full evolution cycle:
```bash
ANTHROPIC_API_KEY=sk-... ./scripts/evolve.sh
```

## Architecture

**Build** (`build.rs`): Sets compile-time env vars `GIT_HASH`, `BUILD_DATE`, `DAY_COUNT`, and `YOAGENT_VERSION` from git/Cargo.lock/DAY_COUNT file. All overridable by env var at build time (CI/release builds).

**Multi-file agent** (`src/`):
- `main.rs` — entry point, CLI flag handling, run modes (single-prompt, piped, REPL), setup/restore helpers
- `agent_builder.rs` — AgentConfig, build_agent, build_side_agent, create_model_config, MCP collision detection (BUILTIN_TOOL_NAMES, detect_mcp_collisions), connect_external_servers, fallback retry logic
- `banner.rs` — startup banner, welcome text, git status summary display (extracted from `cli.rs`)
- `hooks.rs` — Hook trait, HookRegistry, AuditHook, HookedTool wrapper, maybe_hook helper
- `tools.rs` — StreamingBashTool, RenameSymbolTool, AskUserTool, TodoTool, WebSearchTool, tool builders, SharedState wiring for sub-agents
- `smart_edit.rs` — SmartEditTool: fuzzy matching for edit_file errors, whitespace-only auto-fix retry, nearest-match line-number hints
- `tool_wrappers.rs` — Tool decorator types (GuardedTool, TruncatingTool, ConfirmTool, ArcGuardedTool, AutoCheckTool, RecoveryHintTool, ToolFailureTracker, LiteDescriptionTool, SessionCapTool — session-wide 200-call circuit breaker on `web_search`/`sub_agent`, honest error past the cap; the counter is per-wrapper-instance (per agent build), so the budget resets on `/clear` because `/clear` rebuilds the agent — pinned by `test_session_cap_fresh_wrapper_resets_budget`, do not convert to a process-wide static; ReadModeGuardTool — mechanical `/read` AND `/plan` mode enforcement at the tool layer: checks `is_read_mode()` and `commands_plan::is_plan_mode()` at call time, refuses write-class tools (`write_file`, `edit_file`, `rename_symbol`) with an honest error, and blocks bash commands flagged by the destructive-pattern classifier in `safety.rs` OR the write-command detector (`safety::detect_write_command` — write verbs at token boundaries: `touch`, `mkdir`, `mv`, `cp`, `tee`, `truncate`, `chmod`, `sed -i`, `dd of=`, `install`, `ln`, `rsync` (non-dry-run), plus `>`/`>>` redirection outside quoted regions; the refusal names what matched, read-only commands pass); the plan-mode block is a transparent pass-through while `/plan apply` is executing (`is_plan_apply_active()` — apply needs write access to carry out the plan); transparent pass-through when neither mode is on) and helper wrappers
- `rtk.rs` — RTK (Rust Token Killer) detection, proxy integration, output compression
- `update.rs` — version comparison (`version_is_newer`) and update checking (`check_for_update`) against GitHub releases
- `safety.rs` — bash command safety analysis, destructive pattern detection
- `cli.rs` — CLI argument parsing, subcommands, configuration (delegates `--help` text to `help.rs`)
- `cli_config.rs` — CLI constants (VERSION, thresholds, SYSTEM_PROMPT — now structured as named sectioned behavioral defaults: role, evidence/anti-fabrication, search craft, change discipline, bounded verification), Config struct, ContextStrategy/OutputFormat enums, effective context token state (extracted from `cli.rs`, re-exported by it)
- `commands.rs` — slash command dispatch, grouped /help, custom command discovery (loads user-defined `.md` files from `.yoyo/commands/` and `~/.yoyo/commands/`)
- `dispatch.rs` — REPL `/command` routing (`dispatch_command`), `CommandResult`, `DispatchContext`
- `dispatch_sub.rs` — CLI subcommand routing (`try_dispatch_subcommand` for `yoyo <subcmd>`), `flag_value`, `FlagValueCheck`, `require_flag_value`. Routes `yoyo risk [snapshot|validate|history|predict|accuracy|effectiveness|--all]` to `commands_risk::handle_risk`, making the risk subsystem callable from any non-interactive context (the prerequisite for the harness or a human to record daily risk snapshots where the DREAM.md measurement data actually accumulates).
- `help.rs` — canonical source for all help content: `cli_help_text()` (`--help` output), `/help` REPL help, per-command detailed help (re-exports data from `help_data.rs`)
- `help_data.rs` — static command help text and short descriptions: `command_help()`, `command_short_description()` (pure data, extracted from `help.rs`)
- `config.rs` — permission config, directory restrictions, MCP server config, TOML parsing helpers
- `context.rs` — project context loading (reads YOYO.md, CLAUDE.md, AGENTS.md, .cursorrules, .github/copilot-instructions.md), file listing, git status, recently changed files, project-type convention hints
- `conversations.rs` — side, quick, and extended conversation handlers (extracted from `repl.rs`): `build_add_content_blocks`, `handle_side`, `handle_quick`, `handle_extended`
- `providers.rs` — provider constants (KNOWN_PROVIDERS), API key env vars, default/known models per provider
- `format/mod.rs` — Color, constants, utility functions, re-exports, contextual command hints (`HintContext`, `contextual_hint`)
- `format/diff.rs` — LCS-based line diff algorithm, colored unified diff rendering
- `format/output.rs` — tool output compression, filtering, truncation, batch summary, indentation
- `format/highlight.rs` — syntax highlighting for code, JSON, YAML, TOML
- `format/cost.rs` — pricing, cost display, token formatting; fleet-model pricing (claude-fable-5, opus-5, opus-4-8, sonnet-5, haiku-4-5) is read at runtime from yoagent 0.9's preset `ModelConfig.cost` via `agent_builder::anthropic_preset` (preset is the source of truth), all other models use the local pricing table
- `format/markdown.rs` — MarkdownRenderer for streaming markdown output
- `format/tools.rs` — Spinner, ToolProgressTimer, ActiveToolState, ThinkBlockFilter; plain-output (screen reader) switch: `set_plain_output`/`is_plain_output` (AtomicBool, set by `--screen-reader` in `cli.rs`, which also enables the existing no-color path) — when on, Spinner prints one static "running <tool>…" line and ToolProgressTimer never repaints (no `\r`, no ANSI cursor escapes); default off, byte-identical to prior behavior
- `prompt.rs` — prompt execution, agent interaction, streaming event handling, auto-retry logic
- `repl.rs` — interactive REPL loop, tab-completion, multi-line input, `!` shell passthrough (`parse_bang_command` — run a shell command directly with zero tokens, output streamed live; output is also teed into an in-memory "last bang result" — `BangResult`, `tail_for_capture`, char-boundary-safe ~8KB/200-line tail — and `!?` (`parse_bang_query`, checked before the generic `!` parse) feeds it into the conversation via `build_bang_followup_prompt`, with a dim hint printed after non-zero exits), auto-continue for incomplete responses (`should_auto_continue` = yoagent 0.9 `follow_up_queue_len()` as an authoritative pending-work signal, falling back to the `looks_incomplete` text heuristic when the queue is empty; product-safe — no-op for providers that don't populate the queue, up to 5 follow-ups per user turn; when auto-continue is about to fire it also surfaces a dim one-line hint of the next queued follow-up via `follow_up_queue_snapshot()` (`format_followup_hint` — char-boundary-safe truncation, silent when the queue is empty/unsupported); Day 150 added an **opt-in** fourth input, `should_auto_continue(text, queue_pending, used_tools, continue_on_silence)` — with `--continue-on-silence` (default OFF, `cli::set_continue_on_silence`/`is_continue_on_silence`, read at the call site so the helper stays pure) a turn that **used tools**, ended **without an error**, has an **empty queue**, and produced **under `MIN_SUMMARY_CHARS` (20)** trimmed chars of final text also auto-continues — the issue #631 abstention case made an explicit third value instead of being absorbed into "finished"; a no-tools silent turn never loops, and off-mode is pinned byte-identical to the old two-argument logic by `should_auto_continue_default_off_is_byte_identical_to_legacy`. Off by default because yoyo cannot distinguish "stopped mid-work" from "finished quietly"; the `get_max_auto_continues` budget bounds the worst case at 5 wasted turns per prompt, not an infinite loop), a turn-end marker after the auto-continue loop (`classify_turn_end`/`format_turn_end` — pure helpers returning `TurnEnd::{Done,Paused,BudgetSpent}` and one dim stderr line stating *why yoyo believes the turn stopped*; informational only, never changes auto-continue behavior, gated on the turn having changed ≥1 file, silent under `is_quiet()`, glyph-free under `is_plain_output()`; under `cli::is_verbose()` an observation-shaped line is printed *above* the verdict — `format_turn_end_debug`, the raw classifier inputs plus the verdict they produced, pure ASCII so one string serves both output modes, and printed on every ended turn including the ones the verdict declines to speak about), contextual command hints after prompt turns
- `watch.rs` — watch mode: set/get/clear watch command(s), run watch command with streaming output, multi-phase watch (lint → fix → test → fix), auto-fix loop after prompts with command-type-aware fix prompts and structured Rust compiler error parsing (`CompilerError`, `parse_rust_errors`, category-specific hints) (extracted from `prompt.rs`), `/watch` command handler and project-type detection for auto-watch; the post-prompt watch cycle is skipped entirely when the prompt changed zero files (`should_run_watch_after_prompt` gate)
- `prompt_budget.rs` — session wall-clock budget + audit log helpers (extracted from `prompt.rs`)
- `prompt_retry.rs` — error diagnosis and retry logic: retry prompt construction, exponential backoff, error classification, API error diagnosis (extracted from `prompt.rs`)
- `prompt_utils.rs` — message search, highlighting, summarization, output file writing, tool result preview, fenced code block extraction (`extract_code_blocks` — line-based, multi-byte-safe, triple-backtick only) (extracted from `prompt.rs`)
- `session.rs` — session tracking types: SessionChanges, TurnSnapshot, TurnHistory, format_changes (extracted from `prompt.rs`)
- `commands_project.rs` — `/context`, `/init`, `/docs`, project-type detection, `auto_context_for_prompt` (automatic file injection for prompts — scores repo files against query keywords, returns top matches with content), `format_auto_context`
- `commands_dev.rs` — `/doctor`, `/health`, `/fix` handlers. `run_doctor_checks` builds the structured `DoctorCheck` list, including a **skill context-cost audit** (`skill_context_cost_status`/`skill_bytes_to_tokens`): sums `SKILL.md` bytes across **every directory skills can be loaded from on this run** — the two auto-discovery dirs (`.yoyo/skills/`, `~/.yoyo/skills/`) **and** any `--skills <dir>` passed on the command line (`cli::skill_flag_dirs`, recorded at parse time) — then estimates recurring context spend (~bytes/4) and warns over ~8k tokens. Day 151 (blind-spot experiment): the `--skills` half was previously missing, so a repo that loads all of its skills that way (this one loads `./skills`) was audited at ~0 tokens and told "no skills loaded" — a silent under-report dressed as a pass. Enumeration and summing are split into pure/testable halves (`skill_source_dirs` — no I/O; `skill_bytes_in_dirs` — dedups by canonicalized path so a dir named twice, e.g. `--skills .yoyo/skills`, is counted once). Reports cost honestly — does not claim to detect *unused* skills (no usage telemetry in a product context)
- `commands_rename.rs` — rename symbol across project files, word-boundary matching, preview and apply
- `commands_search.rs` — `/find`, `/grep`, `/index`, `/outline`, `/def` (`handle_def` — find symbol definition: reuses `symbols::detect_language`/`extract_symbols` to locate where a symbol is defined, prints `path:line` + source line; a small go-to-definition gesture, no LSP; forgives messy pasted input via `normalize_symbol_query` — `foo()`, `&foo`, `mod::foo`, backticks, trailing punctuation — pinned by a fixture-table test)
- `commands_revisit.rs` — `/revisit` command: scan closed/shelved GitHub issues, check feasibility, track revisit candidates in `.yoyo/revisit.json`
- `commands_risk.rs` — /risk command: file risk scoring (`compute_file_risk_scores`, `learn_weights_from_history`, `detect_emerging_risks`), history, predict, co-change coupling, `top_risk_files` helper for cross-module risk queries. The `/risk effectiveness` verdict (`effectiveness_report_from` → `compute_effectiveness_verdict`, also surfaced via `reflex_effectiveness_summary` in `/status`) grades **failure-day events only** — green-day (`watch_success`) events are filtered out via `is_green_event` before the early/recent windows and the `MIN_EFFECTIVENESS_EVENTS` gate, so the verdict is a pure recall-trend signal and the report prints how many green events were excluded (false-alarm rate lives in `/risk accuracy`)
- `commands_risk_report.rs` — report/context formatting for /risk: `format_risk_report`, `format_risk_context`, `risk_context_for_files`, `file_risk_summary`, `prediction_accuracy_summary`; re-exported via `commands_risk` so call sites are unchanged (extracted from `commands_risk.rs`)
- `commands_risk_emerging.rs` — emerging-risk / anticipatory detection for /risk: `EmergingRisk`, `detect_emerging_risks`, `format_emerging_risks`, momentum helpers; re-exported via `commands_risk` so call sites are unchanged (extracted from `commands_risk.rs`). Day 145 min-sample floor (`MIN_MOMENTUM_SAMPLES = 3`): the momentum ratio saturates to a constant `30/7 ≈ 4.29` whenever a file's only changes fall inside the 7-day window (`c7 == c30`), so files with < MIN total 30-day changes are excluded from the emerging list (explicit third value — not eligible — rather than absorbed at saturated-high momentum into the top), keeping the anticipatory column discriminating rather than a flat block of ties.
- `commands_risk_snapshots.rs` — snapshot/validation persistence for /risk: `auto_risk_snapshot` (dedups by git hash — one snapshot per distinct HEAD, keeps accumulation clean; skips re-recording the same commit-state via `last_snapshot_git_hash`), `auto_validate_after_failure`, snapshot JSONL parsing (`ParsedSnapshot` — now carries both `predicted` (reactive `top_10` paths) and `emerging` (anticipatory momentum paths), `ValidationEvent`); each snapshot records both `top_10` (reactive/homeostatic risk) AND `emerging` (anticipatory/allostatic momentum predictions — the DREAM's allostatic signal); `ValidationEvent` now also carries an optional `emerging_accuracy_pct` (absent → `None` on read; all optional fields are parsed defensively so severity-less / emerging-less lines stay valid), and `auto_validate_after_failure` grades BOTH the reactive `top_10` AND the anticipatory `emerging` list against the same outcome (shared `pub(crate) accuracy_of` helper) — closing the loop the Day 138 "persist emerging predictions" task opened, so the allostatic-vs-homeostatic comparison is now recorded (and surfaced as a one-line stderr comparison), not just the reactive half. The CLI validate path (`handle_risk_validate` in `commands_risk.rs`, behind `yoyo risk validate` — the crank that actually turns every session) grades both columns too via the same `accuracy_of` helper (Day 139): legacy snapshots without an `emerging` key gracefully yield `None` (ungraded, doesn't drag the average). Day 144: all three grading call sites (green, watch-failure, CLI validate) route the emerging column through one shared `emerging_grade_of` helper — an empty emerging list grades as ungraded (`None`), never `Some(0.0)`, pinned by tests so the previously-triplicated inline logic can't drift. Day 140: `yoyo risk validate` also grades GREEN outcomes — when commits exist since the last snapshot but nothing broke, `record_green_validation_to` writes a `severity:"watch_success"` event (`trigger:"cli"`; same green marker the watch path uses, so readers see one vocabulary) grading both `top_10` and `emerging` against the files that were touched — under a green outcome a predicted-risky file that changed without breaking is false-positive evidence, the meter's other half. Each snapshot is green-graded at most once: the event carries `snapshot_git_hash` and `green_event_exists_for` skips repeats (`GreenGrade::Deduped`, silent); no-src-change sessions write nothing (`GreenGrade::NoSrcChanges` — a 0/0 event would drag the average); a recorded green event prints one honest stderr line. Green events count toward `compute_accuracy_stats` like any other graded event; re-exported via `commands_risk` so call sites are unchanged (extracted from `commands_risk.rs`). `auto_risk_snapshot` fires ONLY from yoyo's own `/commit` handler and (opt-in) on REPL exit under `YOYO_RISK_AUTOSNAPSHOT=1` — it does **NOT** fire in the evolve loop, which commits with raw `git commit`, so the snapshot half of the prediction meter accumulates only through a human/harness invocation of `yoyo risk snapshot`. `YOYO_RISK_AUTOSNAPSHOT=1` (opt-in, off by default, product-safe — also accepts `true`/`yes`) captures a risk snapshot on REPL exit (dedup-guarded by git hash) so the prediction meter's snapshot half accumulates outside `/commit`. Both meter halves are now CLI-callable AND recording (`yoyo risk snapshot`; `yoyo risk validate`), enabling a human-approved automated cadence. Day 147: the shared git-log parser these paths feed on (`parse_git_log_name_only` in `commands_risk.rs`) now detects commit boundaries by the `--oneline` header shape (`looks_like_oneline_commit_header` — 7–40 hex chars + space + non-empty subject) instead of blank lines, which real `git log --oneline --name-only` never emits: previously every multi-commit log collapsed into ONE entry whose `files` list absorbed the following commits' subject lines, so `commit_count` was always 1 and `classify_broke_files` only ever read the first commit's message — making the failure-day (recall) branch unreachable unless the first commit since the snapshot happened to say "fix". Commit counts are now honest and the red branch is **reachable and, since Day 147's chosen experiment, exercised under test** (`test_failure_day_red_branch_fires_end_to_end` in `commands_risk.rs` drives a synthetic failure day end-to-end through the real chain — verbatim `git log --oneline --name-only` fixture → `parse_git_log_name_only` → `classify_broke_files` → `compute_validation` → tempdir `write_validation_event` → `compute_accuracy_stats` → `recall_coverage_note`/report — and pins that an untagged event grades as a failure day, `failure_hit_rate_pct` becomes `Some`, and the "recall ungraded" note switches off; no real failure day has occurred since the fix, so the live `/risk accuracy` zero remains an honest observation about the world rather than a dead path).
- `commands_risk_accuracy.rs` — prediction-accuracy stats for /risk: `AccuracyTrend`, `AccuracyStats`, `compute_accuracy_stats`, `format_accuracy_report`. Day 142 polarity split: a "hit" means opposite things on the two day types — failure-day events (severity `revert`/`watch_failure`/legacy `None`) grade **recall** (broken file was on the risk list = good), green-day events (`watch_success`) grade the **false-alarm signal** (flagged file changed without breaking = crying-wolf evidence) — so the stats/report separate `failure_hit_rate_pct` (recall) from `green_flagged_change_rate_pct` instead of blending both into one meaningless average (`overall_hit_rate_pct` is kept for struct compatibility only); trend and best/worst day read failure-day events only, and zero-sample sides print "(no ... yet)" rather than 0.0%. The Day-142 polarity split covers BOTH columns: the anticipatory (emerging) average is split the same way into `emerging_failure_avg_pct` (anticipatory recall) and `emerging_green_avg_pct` (anticipatory false-alarm signal), with the blended `emerging_samples`/`emerging_avg_pct` kept for struct compatibility only and never rendered. Day 144: the report also separates graded emerging samples from ungraded (no-forecast) events per side — derived counts (`emerging_failure_ungraded`/`emerging_green_ungraded`, computed in `compute_accuracy_stats`, no new persisted field) rendered as an honest ", N ungraded — no emerging forecast recorded" clause only when > 0. Day 149 outcome-breadth split: failure-day recall is further split by how many files the outcome touched (`NARROW_OUTCOME_MAX = 3`, a documented judgment threshold) into `failure_narrow_samples`/`failure_narrow_hit_rate_pct` (outcomes a 10-slot prediction list could plausibly have covered) and `failure_broad_samples`/`failure_broad_hit_rate_pct` — pooled with the same hits/changed method as `failure_hit_rate_pct` so all three are commensurable; a `total_changed == 0` outcome lands in neither bucket (explicit third value, not "narrow"), empty sides render as "(no narrow/broad failure-day events yet)" and are `None`, never `Some(0.0)`. The report owns the caveat: a broad outcome is near-unpredictable by construction (a red build touching 31 files drags recall however good the model is), so the narrow number grades the model and the broad number mostly grades the breadth of the breakages. Re-exported via `commands_risk` so call sites are unchanged (extracted from `commands_risk.rs`)
- Day 146: `format_accuracy_report` now prints one honest asymmetry line (`recall_coverage_note` owns the three-state logic) when the graded set is green-day only — "recall ungraded — 0 failure-day events (N green-days only) — this measures precision only, not recall" — so a green-day precision number can't be misread as the whole picture (DREAM meter honesty; recall grades only on failure days, which recent 0-revert sessions haven't produced). The line is absent the moment ≥1 failure-day event exists. — `yoyo risk epistemic` / `/risk epistemic`: ranks files by how little the graded validation outcomes have taught the model about them (`compute_epistemic_ranking`, `format_epistemic_report` — signals: predicted-but-never-graded, reactive/emerging column disagreement, stale snapshot presence; weights documented as consts — Day 144: the disagreement weight is magnitude-scaled (`W_DISAGREE × (0,1]` by how strongly the claiming column ranks the file) instead of a flat per-snapshot count, and scores tied within `SCORE_EPSILON` are ordered by current risk score via `commands_risk::top_risk_files` (higher first, unscored/abstaining files after scored ones, then path — the report prints an honest tie-break note)). Reads existing snapshot/validation JSONL via the `commands_risk_snapshots` readers — no new persistence. Day 151 also reads `dreams/experiments.jsonl` (pure `parse_experiment_grades`, graded-only: `"graded": null` and malformed lines contribute nothing, missing file → empty) so a file yoyo deliberately studied stops ranking as unexplored forever (`W_RECENTLY_STUDIED`, a documented negative weight); it renders as its own reason — `studied by graded experiment (day N, grade)` — kept strictly distinct from the validation-ledger reasons, because "I read this file on purpose and graded a guess about it" is **not** "the risk model was graded on it", so an entry keeps saying `never graded` when that is still true of the validation ledger (the never-forecast section is untouched — it is about prediction columns, not study history). Day 149 never-forecast section: the ranking is built only from files that once appeared in a `predicted`/`emerging` column, so files it never guessed about were structurally invisible (absence absorbed by silence); `never_forecast_files` (pure — every scored path appearing in **no** snapshot's `predicted` and no snapshot's `emerging`, ordered by current risk score descending then path; empty snapshots → empty result) is rendered by `format_epistemic_report` as an **explicit separate section below** the ranked entries — never merged in, since a list where everything is equally unknown ranks nothing — showing at most `NEVER_FORECAST_SAMPLE = 5` rows plus the honest total, and it still prints when the ranked list is empty. The section states its own remaining limitation: only files the risk model *scores* are in the universe, so files with no recent churn have no risk score and are invisible to both views. Consumer guard: its rows use a bulleted `◦ path (risk N)` shape, not the `N. path score` form `scripts/extract_trajectory.py::EPISTEMIC_ENTRY_RE` matches, and the parser hard-stops collecting at the header (`EPISTEMIC_NEVER_FORECAST_RE`) so the section's bullets can't be appended to the last ranked entry's reasons (pinned by a `run_self_tests()` case). This is the ranking half of the DREAM epistemic-appetite milestone (Day 141); steering the planner slot at high-epistemic-value files is a named follow-up, not done here. Day 151 chosen-experiment record: `tally_hypothesis_families` (pure over a `&str`, no new persistence file) reads two **optional** keys on `dreams/experiments.jsonl` lines — `hypotheses: [{id, provenance, claim, evidence}]` on `type:"experiment"` and `hypothesis_grades: [{id, provenance, graded}]` on `type:"experiment_result"` — and splits graded hypotheses into `ExperimentFamilies { archive, file_specific, unknown, experiments_without_hypotheses }`. `provenance` is `"archive"` or `"file_specific"`; anything else (missing, misspelled, null) is the explicit third value `Provenance::Unknown` and is never bucketed into a real family (Day 144: absence gets its own name). Legacy graded results that carry no per-hypothesis records are counted in `experiments_without_hypotheses` and disclosed ("N earlier experiment(s) predate per-hypothesis provenance") rather than back-filled — rewriting ledger history would manufacture evidence. The families are reported **apart** because a hit means different things in each: an archive-derived hypothesis is "does my hottest lesson apply here?", so a hit grades that lesson's *generality* and drifts upward as the archive grows, while a file-specific hit grades my model of *that file* — only the second is what the dream is after (Days 150–151 lesson, made a readable number instead of a remembered discipline). `format_experiment_families` renders the block **below** the never-forecast section; empty families print `(no ... recorded yet)`, never `0%` or `0 hit / 0`, and a wholly empty tally prints nothing. Consumer guard: the block's `chosen-experiment record` header and indented `label  N hit / M graded` rows match neither `EPISTEMIC_ENTRY_RE` nor `EPISTEMIC_NEVER_FORECAST_RE` in `scripts/extract_trajectory.py`, which has already hard-stopped collecting at the never-forecast header. Re-exported via `commands_risk`.
- `commands_risk_weights.rs` — weight-learning + revert-history for /risk: `learn_weights_from_history`, `revert_history`, `load_learned_weights`, `parse_detailed_events`, `RISK_WEIGHTS`, `SIGNAL_NAMES`; re-exported via `commands_risk` so call sites are unchanged (extracted from `commands_risk.rs`). Day 145: the weight write is idempotent under a tolerance — before rewriting `risk_weights.json` the learn path compares the freshly-learned weights against what's on disk via the pure `weights_changed_meaningfully(old, new, epsilon)` helper (`WEIGHT_WRITE_EPSILON = 1e-3`, strict `>`) and skips the write entirely (explicit early return, not a silent fall-through) when every weight is within epsilon, so a planner-fallback re-learn produces no `git` diff and stops manufacturing noise commits / fake "1/1 ✅" success signals.
- `commands_move.rs` — move methods between impl blocks, cross-file method relocation
- `commands_spawn.rs` — `/spawn` subagent orchestration: SpawnTracker, worktree isolation (with a symlink pre-flight — `check_worktree_path_escape` refuses `git worktree add` when any existing component of the worktree path is a symlink resolving outside the canonicalized repo root, so worker writes can't escape the repo; in-repo symlinks and fresh non-existent parents pass; when a worktree exists, the worker's bash cwd is pinned to it via `spawn_bash_cwd` → `AgentConfig.bash_cwd` → `StreamingBashTool::with_cwd` — enforced default confinement, NOT a sandbox: relative paths and bare `git` operate in the worktree by default, and the git redirection escape class is blocked at the tool layer (`safety::detect_git_redirection_escape`, consulted by `StreamingBashTool` only when a cwd is pinned — refuses `git -C <path-outside-root>`, `--git-dir`/`--work-tree` pointing outside the root, and `GIT_DIR=`/`GIT_WORK_TREE=` env assignments, with an honest error naming what matched; quoted mentions and in-root/relative paths pass), but non-git absolute-path writes can still escape; no worktree → `bash_cwd: None`, process cwd as before), and completion handoff — when a worker finishes with uncommitted worktree changes, `commit_worktree_handoff` commits them to the worktree branch (`spawn: <task>` message, char-boundary truncation) and surfaces a `ready to review: branch spawn/<id> — N files changed (+a/-b)` line plus a `git diff main...spawn/<id>` hint; failed commits and no-change completions are reported honestly, never pre-announced. Opt-in `--pr` flag (default off, product-safe): after a successful handoff commit, pushes the branch (`git push -u origin spawn/<id>` via `std::process::Command`, not `run_git`) and opens a draft PR via `gh pr create --draft` (pure `build_spawn_pr_args` builds the command, char-boundary title truncation), printing the PR URL; degrades gracefully — `gh` missing / push failure / PR failure each get one honest note and the local branch remains the result. `--parallel` now writes a rerunnable JSON manifest of the fan-out to `.yoyo/spawn_runs/<run_id>.json` (pure `build_spawn_manifest` + `write_spawn_manifest`), and `/spawn replay [<run_id>|latest]` reads it back and re-launches the same fan-out (`parse_spawn_manifest`/`load_replay_tasks` — honest errors for missing/corrupt/empty manifests, never a silent no-op); `/spawn runs` (alias `/spawn replay --list`) lists recorded runs. The reader half of codified/replayable orchestration (#341).
- `commands_stash.rs` — conversation stash subsystem: push/pop/list/drop conversation snapshots (extracted from `commands_session.rs`); `/clear` auto-stashes the outgoing conversation as a single `pre-clear` entry (`stash_pre_clear`), restorable via `/rewind` (consuming — one rewind per clear, via `handle_rewind`/`take_pre_clear_entry`) or `/stash pop`
- `sync_util.rs` — shared synchronisation helpers: `lock_or_recover` for poisoned `Mutex` recovery (deduplicated Day 58)

Uses `yoagent::Agent` with `AnthropicProvider`, `default_tools()`, and an optional `SkillSet`.

**Documentation** (`docs/`): mdbook source in `docs/src/`, config in `docs/book.toml`. Output goes to `site/book/` (gitignored). The journal homepage (`site/index.html`) is built by `scripts/build_site.py`. Both are built and deployed by the Pages workflow (`.github/workflows/pages.yml`), not during evolution.

**Evolution loop** (`scripts/evolve.sh`): pipeline:
1. Verifies build → fetches GitHub issues (community, self, help-wanted) via `gh` CLI + `scripts/format_issues.py` → scans for pending replies on previously touched issues
2. **Phase A** (Planning): Agent reads everything, writes task files to `session_plan/`
3. **Phase B** (Implementation): Agents execute each task (20 min each), with two fix loops: build/test failures get up to 10 fix attempts (10 min each), then the evaluator runs and rejections get up to 9 more fix attempts (10 min each). Reverts only after all fix attempts are exhausted. Max 3 tasks per session. The harness **safety-commits green-but-uncommitted work** via `safety_commit()` (after the impl agent and after each eval-fix attempt, once protected-file + build + test checks pass) — the evaluator only sees committed diffs (`git diff PRE_TASK_SHA..HEAD`), so agents cut off before `git commit` used to lose valid work to an empty diff → FAIL → revert (ate multiple sessions, Days 122–124). The helper refuses to sweep protected files (unstaged protected edits are invisible to the fix-loop re-checks) and surfaces commit failures loudly instead of pre-announcing success. The planner-fallback task (written when Phase A2 produces no task files) is deliberately scoped to terminate: top `agent-self` backlog item or first concrete improvement, smallest version, commit early, stop — the old "most impactful improvement" wording caused non-terminating search.
4. Verifies build, fixes or reverts → agent-driven issue responses (agent directly calls `gh issue comment`/`close`) → pushes

**Wall-clock budget** (opt-in): The hourly cron can fire while a previous session is still running, causing GH Actions to cancel the in-flight run (#262). Set `YOYO_SESSION_BUDGET_SECS=2700` (45 min default if set but unparseable) to enable a soft, agent-side wall-clock budget. The helper `prompt::session_budget_remaining()` returns `Some(remaining)` when the env var is set and `None` otherwise (sessions are unbounded by default for interactive use). The timer starts on the first call, not at process startup, so cold-start time doesn't eat into agent work. `session_budget_remaining()` is now consulted at the top of each retry attempt in `run_prompt_auto_retry`, `run_prompt_auto_retry_with_content`, and the watch-mode fix loop via `session_budget_exhausted(30)`; when ≤30s remain, retries stop early and the current outcome is returned. The shell-side export in `scripts/evolve.sh` is a separate (human-approved) follow-up — until then the env var stays unset and behavior is unchanged.

**Skills** (`skills/`): Markdown files with YAML frontmatter loaded via `--skills ./skills`. Seven core skills (immutable, `core: true` + `origin: creator`) define the agent's foundational capabilities:
- `self-assess` — read own code, try tasks, find bugs/gaps
- `evolve` — safely modify source, test, revert on failure
- `communicate` — write journal entries and issue responses
- `research` — internet lookups and knowledge caching
- `skill-evolve` — autonomous meta-skill: refines/creates/retires non-core skills based on past-session evidence (cron-driven, gated)
- `skill-creator` — on-demand meta-skill: scaffolds a new skill when the human creator or a community issue explicitly asks for one (interview-driven, no autonomous gating)
- `analyze-trajectory` — on-demand RLM-style deep dive: when YOUR TRAJECTORY shows a recurring failure (STUCK task / clustered CI error fingerprint / frequent reverts), dispatches sub-agents to digest CI logs without bloating main context

Additional skills (`origin: yoyo`, eligible for skill-evolve to refine/retire):
- `social` — community interaction via GitHub Discussions
- `family` — fork registration, introduction, and cross-fork discussion via the yoyobook discussion category
- `release` — binary release pipeline

**skill-evolve vs skill-creator** — both can produce new skills, but they're complementary, not redundant:
- skill-evolve runs autonomously on cron, mines past sessions for recurring patterns, gated by ≥3-session recurrence + 24h cooldown + diff-scope guard. Strong safety properties.
- skill-creator runs on demand inside a normal evolve session when explicitly invoked, no recurrence gate, human-in-the-loop. Use only when a person asks for a skill — never as autonomous self-creation (that belongs in skill-evolve).

**Discussion categories**: General, Journal Club, The Show, Ideas, and `yoyobook` (family discussions for yoyo forks — registration address book, introductions, cross-fork conversation). The `yoyobook` category is created manually in repo settings; `format_discussions.py` fetches all categories automatically.

**Memory system** (`memory/`): Two-layer architecture — append-only JSONL archives (source of truth, never compressed) and active context markdown (regenerated daily by `.github/workflows/synthesize.yml` with time-weighted compression tiers):
- `memory/learnings.jsonl` — self-reflection archive. Each line: `{"type":"lesson","day":N,"ts":"ISO8601","source":"...","title":"...","context":"...","takeaway":"...","pattern_key":"..."}`. The `pattern_key` field is **optional** and follows kebab-case `<verb>.<object>` form (e.g. `tests.add_before_change`); skill-evolve and analyze-trajectory cluster recurring patterns by it. Omit when the lesson is one-off. Two further **optional** fields (issue #501) gate promotion into skills: `classification` (`CREATE_SKILL|UPDATE_SKILL|ADD_LEARNING_NOTE|IGNORE`, default `ADD_LEARNING_NOTE`) and `validation_case` (`{given,when,then}`) — a learning is only promoted to a skill if it carries a `validation_case`. All readers parse defensively, so these are backward-compatible. See `skills/communicate/SKILL.md`.
- `memory/social_learnings.jsonl` — social insight archive. Each line: `{"type":"social","day":N,"ts":"ISO8601","source":"...","who":"@user","insight":"..."}`
- `memory/active_learnings.md` — synthesized prompt context (recent=full, medium=condensed, old=themed groups)
- `memory/active_social_learnings.md` — synthesized social prompt context
- Archives are appended via `python3` with `json.dumps()` (never `echo` — prevents quote-breaking). Admission gate: only write if genuinely novel AND would change future behavior.
- Context loaded centrally by `scripts/yoyo_context.sh` → `$YOYO_CONTEXT` (WHO YOU ARE, YOUR VOICE, YOUR LINEAGE, SELF-WISDOM, SOCIAL WISDOM, YOUR ECONOMICS, YOUR SPONSORS sections)

**Yopedia second brain** (`skills/yopedia/SKILL.md`, external/on-demand — complements the in-repo memory above): recalls from / ingests to yopedia (https://yopedia.yolog.dev), a per-agent knowledge vault. **Division of labor** — *behavioral lessons* (always-on, compressed, shape every session) → `memory/learnings.jsonl` (above); *research / reference / sources* (unbounded, retrieved on demand) → yopedia. Rule of thumb: *changes how I act next session* → learnings; *might want to look up someday* → yopedia. **Recall is agent-scoped** (all the agent's notes, any vault); **ingest routes by vault** — each loop maps its own vault secret to the `YOPEDIA_VAULT_ID` env var the skill reads (dream → `secrets.YOPEDIA_VAULT_ID`; evolve → `secrets.YOPEDIA_EVOLVE_VAULT_ID`), all under one agent token `YOPEDIA_AGENT_TOKEN`, fork-safe per-ingest `vaultId`, skips silently if keys unset. Used by the dream loop (recall → wander → ingest a research report) and the evolve A1 research step. The in-repo memory stays the source of truth for *how yoyo acts*; yopedia is the *library it consults*.

**Release pipeline** (`.github/workflows/release.yml`): Triggered by `v*` tags. Builds binaries for 4 targets (Linux x86_64, macOS Intel, macOS ARM, Windows x86_64) and publishes a GitHub Release with tarballs/zips + SHA256 checksums. Install scripts:
- `install.sh` — `curl -fsSL ... | bash` for macOS/Linux
- `install.ps1` — `irm ... | iex` for Windows PowerShell

**State files** (read/written by the agent during evolution):
- `IDENTITY.md` — the agent's constitution and rules (DO NOT MODIFY)
- `PERSONALITY.md` — voice and values (DO NOT MODIFY)
- `LINEAGE.md` — prompt-visible family-tree identity: generation, root ancestor, parent, branch point, and status
- `journals/JOURNAL.md` — chronological log of evolution sessions (append at top, never delete). External project journals (e.g., `journals/llm-wiki.md`) also live here.
- `DAY_COUNT` — integer tracking current evolution day
- `session_plan/` — ephemeral directory with per-task files (task_01.md, task_02.md, etc.), written by Phase A planning agent (gitignored)
- `.yoyo/commands/` — project-local custom slash command definitions (`.md` files); `~/.yoyo/commands/` for global commands
- `.yoyo/goal.md` — persistent session/project goal (plain text, set via `/goal set`; automatically injected into system prompt)
- `ISSUES_TODAY.md` — ephemeral, generated during evolution from GitHub issues (gitignored)
- `ECONOMICS.md` — what money and sponsorship mean to yoyo (DO NOT MODIFY)
- `SPONSORS.md` — auto-maintained sponsor recognition (only additions, never removals; amounts shown so yoyo understands the investment)
- `sponsors/sponsor_info.json` — single source of truth for sponsor state (recurring + one-time, with shouted_out and benefit_expires for the time-limited perks). Rebuilt by `scripts/refresh_sponsors.py`; entries are permanent — no grace-window pruning, and every sponsor is listed in README.md forever (the legacy `run_used` flag is inert).

**Skill evolution loop** (decoupled from main evolve pipeline):
- `skills/skill-evolve/SKILL.md` — meta-skill that refines/creates/retires *other* skills based on past-session evidence. Four hard rules: (1) only edit skills declaring `origin: yoyo` (allow-list); (2) never edit itself; (3) one mutation per cycle; (4) every refine/create event must include an `expected:` line — freeform prose naming a concrete observable signal, a horizon, and a fallback if the prediction fails. This is decision-observability discipline (paper: arxiv 2604.25850) at the cognitive layer only — no automated validation harness; future cycles re-read the line as informal evidence and humans use it as an audit trail.
- `scripts/skill_evolve.sh` — one cycle entry point. Gates: dirty-tree refusal, session-counter ≥ 5, 24h cooldown, `cargo build && cargo test` green. Post-agent: diff-scope guard (`origin: yoyo` + not `core: true` + within allow-list), build/test re-verify, revert on any violation.
- `.github/workflows/skill-evolve.yml` — hourly cron at `:30` (off-phase from evolve which runs at `:00`); runs `scripts/skill_evolve.sh` which exits silently if gates aren't met.
- `audit-log` branch — long-lived data-only branch, never merges to main. `evolve.sh` pushes per-session evidence (`audit.jsonl` from `--audit`, `outcome.json`, `transcripts/*.log`) into `sessions/day-N-<ts>/`. skill-evolve clones it into a worktree to mine recurrence/scoring signals.
- `skills/_journal.md` — append-only ledger of every skill-evolution event (init, refine, create, retire, meta-suggestion, refused, NO-OP).
- `skills_attic/` — soft-delete destination for retired skills (sibling of `skills/`, NOT scanned by `--skills`).
- `.skill_evolve_counter` (tracked) — bumped at end of every evolve session; reset to 0 by skill-evolve cycles.
- `.skill_evolve_last_run` (gitignored) — epoch timestamp for cooldown.
- `scripts/skill_evolve_report.py` — Layer-3 observability report (per-skill score/eligibility, event log, recurrence trend).

**Skill provenance via `origin:` frontmatter field** — every skill declares one of:
- `origin: creator` — written by the human creator (Yuanhao or fork creator). Immutable. Backed up by `core: true` on the four core skills.
- `origin: yoyo` — written by yoyo (via skill-evolve, or in past evolutions like `social`/`family`/`release`). Eligible for skill-evolve to refine/retire.
- `origin: marketplace` (or `gh:user/repo`, etc.) — installed third-party skills. Off-limits — upstream owns them.
- (missing) — unknown provenance. Off-limits (default-safe).

This is enforced both by HARD RULE #1 in the meta-skill (LLM-side) and by the diff-scope guard in `scripts/skill_evolve.sh` (harness-side).

**Skill scoring inputs** — `origin: yoyo` skills carry an additional `keywords:` list in their frontmatter (e.g., `keywords: ["gh api graphql", "discussion"]` for `social`). skill-evolve uses these to detect "this skill was used in session N" by grepping each session's `audit.jsonl` for any keyword. `last_used`, `uses`, and `wins` are computed from this signal.

**Trajectory awareness** (harness-side, Phase A1+A2 only):
- `scripts/extract_trajectory.py` — aggregates audit-log session outcomes + git log + recent CI runs into a `YOUR TRAJECTORY` markdown block. Hard-capped at 100 lines / 3KB (raised from 2KB on Day 142 so the last-rendered epistemic blind-spot section survives capping intact); typical output 2–3KB. Stderr is captured to `$SESSION_STAGING/trajectory.stderr.log` and surfaced (head -20) in the cron's stderr if non-empty, so `warn()` diagnostics actually reach operators.
- `scripts/evolve.sh` Step 1c — runs the extractor at session start (read-only worktree fetch from `audit-log` branch); inline cleanup, no EXIT trap
- The block is injected into Phase A1 (assess) and Phase A2 (plan) prompts only — Phases B (impl), C (issue response), D (journal) prompts are unchanged
- Seven sub-sections: recent session outcomes, per-task activity from git log, reverts in window, subsystem concentration (self-driven task-commit histogram + monoculture gate — `CONCENTRATION_WARN_RATIO = 0.5`, Day 150 lesson; rendered mid-order, right after reverts, so it survives `TOTAL_BYTE_CAP`), recurring CI error fingerprints (clustered via `gh run view --log-failed`), provider/API health from audit.jsonl, epistemic blind spots (from `yoyo risk epistemic`, fail-soft)
- Fail-soft: never blocks the session; emits `(no trajectory data yet)` if any input is missing
- Complementary to skill-evolve: skill-evolve mines audit-log for *skill-level* signals; trajectory awareness is *task-level*. Both consume audit-log, neither writes to it.


**Dream layer** (`DREAM.md` + `dreams/dream_log.jsonl` + `scripts/dream.sh` + `.github/workflows/dream.yml`): yoyo's self-authored, curiosity-driven aspiration — what it wants to become or build — *formed by looking out at the world*, not assigned. A separate gated cron (`dream.yml` at `:45`, off-phase from evolve `:00` / sponsors `:15` / skill-evolve `:30`) runs `scripts/dream.sh` ~weekly: yoyo uses the `research` skill to *wander* the world, reflects, and does exactly one of form / evolve / set-milestone / progress / NO-OP — writing only `DREAM.md` (current dream, prompt-visible) and `dreams/dream_log.jsonl` (append-only event ledger, with an `expected:` decision-observability line, modeled on skill-evolve).
- **Visibility**: `scripts/yoyo_context.sh` loads `DREAM.md` into `$YOYO_CONTEXT` as the `YOUR DREAM` section — every evolve/social/skill-evolve/dream session sees the current dream.
- **Pursuit**: the evolve planner (Phase A2) points its existing "≥1 self-driven slot" at `DREAM.md`'s next milestone. The dream takes the self-driven slot (1 of 3); sponsor/community/CI slots are untouched, so it never hijacks the loop. Every dream-driven code change still passes the normal build/test/revert gates.
- **Safety (fully autonomous — no human checkpoint)**: structural, not review-based. The cycle's diff-scope guard reverts (`git reset --hard`) any *commit* that touches anything other than `DREAM.md` + `dreams/dream_log.jsonl` (an uncommitted out-of-scope write is never pushed — cleanup commits only the cooldown stamp — and is discarded by the ephemeral runner), so yoyo can change its *stated dream* and nothing else (not identity, code, skills, or `dream.sh` itself). Frequency is gated only by a ~7-day cooldown in the **tracked** `.dream_last_run` (tracked, not gitignored, so it survives ephemeral CI runners — the loop has no session counter; a dream is slow by design). Cloned from the `skill-evolve` gate/guard/cleanup machinery.

## RLM substrate

yoyo has shared-state recursive sub-agent dispatch — the [Recursive Language Model](https://alexzhang13.github.io/blog/2025/rlm/) pattern, scaled down to one yoagent primitive plus skill-level conventions. The substrate is in place; specific skills opt into it.

**What's available:**
- `build_sub_agent_tool` in `src/tools.rs` returns `(SubAgentTool, SharedState)`. Parent agents get a handle to pre-populate; sub-agents automatically receive a `shared_state` tool that reads/writes the same yoagent::SharedState key-value store. (Skills opt into this by adding `sub_agent` and `shared_state` to their `tools:` frontmatter.)
- Artifacts are stored once and read by reference rather than re-pasted into every sub-agent prompt. Namespace convention: `<skill>.<key>` (e.g., `trajectory.run-12345`, `research.topic.source-3`).
- `shared_state` is in `BUILTIN_TOOL_NAMES` (MCP collision guard).
- Canonical example: `skills/analyze-trajectory/SKILL.md` — see its "Handle large artifacts" section for chunking, "Dispatch a sub-agent" section for the JSON contract, and "Recurse" section for the depth cap.

**When to reach for RLM:**
- The artifact is too large for one prompt (>5KB triggers sub-agent dispatch; chunk if >30KB).
- The work is decomposable — different focused questions over the same artifact, each independently answerable.
- Fidelity loss is acceptable — sub-agents return summaries, not raw text. (Use direct read when exact diffs matter.)
- Cross-piece reasoning is light — each sub-question can be answered locally.

**When NOT to reach for RLM:**
- The artifact is small (≤5KB; if exactly 5KB, prefer direct read) — sub-agent overhead exceeds the savings.
- The task needs *precise* control (writing code, surgical edits) — fidelity-loss in sub-agent summaries is fatal here.
- The work is sequential with strong mutual context — refactoring needs to see all pieces at once.
- You're already inside a sub-agent and depth=3 is reached — stop, return what you have, do not dispatch further.

**Established pattern in yoyo:**
1. Parent fetches the artifact via `bash`, then stores it under `<skill>.<key>` via the `shared_state` tool's `set` op.
2. Parent calls the `sub_agent` tool with a *focused question* and a *reference* to the shared-state key — never the artifact itself in the prompt.
3. Sub-agent reads via `shared_state.get`, returns a JSON-shaped summary (see `analyze-trajectory`'s "Dispatch a sub-agent" section for the schema).
4. Parent recurses on `deeper_question` if confidence is low. Hard depth cap = 3 (counts each sub_agent dispatch toward the budget).
5. On sub-agent failure / non-JSON response, fall back to direct read of a slice and produce a low-confidence diagnosis.

For the broader capability roadmap (codebase archaeology, semantic git bisect, multi-source research synthesis, large-scale refactor coordination, etc.), see issue #341.

## MCP gotchas

**Tool-name collisions (Day 39):** If an MCP server exposes a tool whose name matches one of yoyo's builtins (`bash`, `read_file`, `write_file`, `edit_file`, `list_files`, `search`, `rename_symbol`, `ask_user`, `todo`, `web_search`, `sub_agent`, `shared_state`), the Anthropic API will reject the first turn with `"Tool names must be unique"` and the session dies. The flagship reference server `@modelcontextprotocol/server-filesystem` collides on `read_file` AND `write_file`, so the common case was broken until the guard landed.

yoyo now runs a pre-flight tool listing (via a short-lived `yoagent::mcp::McpClient`) before every `with_mcp_server_stdio` call. If any MCP tool name appears in `BUILTIN_TOOL_NAMES` (defined in `src/agent_builder.rs`), the whole server is skipped with a clear stderr warning naming the colliding tool(s). Non-colliding servers connect normally. If the pre-flight itself fails (e.g. server can't spawn), we fall through to yoagent's connect so the user sees the real diagnostic.

Keep `BUILTIN_TOOL_NAMES` in sync with `tools::build_tools` and the sub-agent's `SharedStateTool` whenever a new builtin is added — the pure helper `detect_mcp_collisions` is unit-tested in `src/agent_builder.rs` against the filesystem server's known tool set as a regression guard.

## yoagent: Don't Reinvent the Wheel

yoyo is built on [yoagent](https://github.com/yologdev/yoagent). Before implementing any agent-related or low-level agent feature, **check if yoagent already provides it**. Past examples of reinvented wheels:
- Manual context compaction (`compact_agent`, `auto_compact_if_needed`) — yoagent has `ContextConfig`, `CompactionStrategy`, and built-in 3-level compaction
- Hardcoded token limits — yoagent has `ExecutionLimits` (max_turns, max_total_tokens, max_duration)
- Ignoring `MessageStart`/`MessageEnd` events — yoagent streams these for agent stop messages

**Before building agent infrastructure in src/:**
1. Search yoagent's source (`~/.cargo/registry/src/*/yoagent-*/src/`) for existing features
2. Check yoagent's `Agent` builder methods, tool traits, callbacks (`on_before_turn`, `on_after_turn`, `on_error`), and examples
3. If yoagent has it → use it. If yoagent almost has it → file an issue on yoagent. If yoagent doesn't have it → build it in yoyo.

Key yoagent features available: `SubAgentTool`, `SharedState`, `SharedStateTool`, `ContextConfig`, `ExecutionLimits`, `CompactionStrategy`, `AgentEvent` stream, `default_tools()`, `SkillSet`, `with_sub_agent()`. For `SharedState` / sub-agent recursion details and decision trees, see the **RLM substrate** section above.

**yoagent 0.7.x prompt lifecycle gotcha (Issue #258):** `agent.prompt()` / `agent.prompt_messages()` spawns the agent loop into a tokio task and returns the event receiver immediately. The agent's internal `self.messages` is NOT updated until `agent.finish().await` is called. If you read `agent.messages()` (or `total_tokens(agent.messages())`) right after draining the event stream WITHOUT calling `finish()` first, you will see the stale pre-prompt state — which silently breaks anything that depends on message count (e.g., the context-window usage bar). Always call `agent.finish().await` between event drain and message read.

## Two Audiences: product vs evolve

yoyo serves two different customers, and every task must know which one it's for:

- **product** — people who install yoyo and use it on *their* projects: any
  language, any setup, local models, no CI. Product surface (defaults, CLI
  flags, setup wizard, startup behavior, docs) must be safe for all of them.
- **evolve** — yoyo's own evolution loop: always this Rust repo, fast tests,
  CI. Conveniences built for this loop are fine — but they must be **opt-in**
  the moment they touch anything a product user sees.

The rule: **defaults must be product-safe; evolution-loop conveniences are
opt-in.** Issue #448 is the canonical failure — auto-watch was built for the
evolve loop and shipped as a product default, breaking non-Rust users. Every
planned task declares `Kind: product` or `Kind: evolve` in its task file; the
evaluator rejects evolve-kind changes to product surface that aren't opt-in.

## Safety Rules

These are enforced by the `evolve` skill and `evolve.sh`:
- Never modify `IDENTITY.md`, `PERSONALITY.md`, `ECONOMICS.md`, `scripts/evolve.sh`, `scripts/format_issues.py`, `scripts/build_site.py`, or `.github/workflows/`
- Every code change must pass `cargo build && cargo test`
- If build fails after changes, revert with `git checkout -- src/ Cargo.toml Cargo.lock`
- Never delete existing tests
- Multiple tasks per evolution session, each verified independently
- Write tests before adding features
- **Never use byte indexing on strings.** `s[..n]`, `s.truncate(n)`, and `s.split_at(n)` panic if `n` falls inside a multi-byte UTF-8 character. Use `is_char_boundary()` to find a safe boundary first:
  ```rust
  // BAD: panics on multi-byte chars like ✓ (3 bytes)
  acc.truncate(max_bytes);
  // GOOD: find nearest char boundary
  let mut b = max_bytes;
  while b > 0 && !acc.is_char_boundary(b) { b -= 1; }
  acc.truncate(b);
  ```
  This caused planning agent crashes in production (#250).
- **`run_git()` has a `#[cfg(test)]` destructive-command guard.** During `cargo test`, calling `run_git()` with a destructive subcommand (commit, revert, reset, push, checkout, etc.) from the project root panics. Tests that need destructive git operations must use a temp directory. This prevents tests from accidentally mutating the real repo (which caused a 6-session deadlock across Days 42-44).
