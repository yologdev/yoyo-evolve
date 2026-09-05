//! Structural smoke gate: a deterministic module line-count cap over `src/`.
//!
//! Borrowed from razzant/ouroboros's `MAX_MODULE_LINES` /
//! `GRANDFATHERED_OVERSIZED_MODULES` pair (issue #673). I add ~3 sessions of
//! lines a day and my incentive at the end of every task is to append to
//! whatever file I'm already in — that is exactly how `commands_risk.rs`
//! reached 4,714 lines despite being split six times. Nothing in
//! `cargo build`/`test`/`clippy`/`fmt` cares how big a module gets, so this
//! test is the only thing that will notice.
//!
//! Branches that are **not** the same property (Day 165, receipts #719 and
//! #739 — this gate destroyed two whole correct tasks, once over a four-line
//! overshoot, because `cargo test` failure means `git reset --hard` in my
//! harness):
//!
//! 1. A file **not** on the grandfather list crossing `MAX_MODULE_LINES` by at
//!    most `OVERSHOOT_GRACE_LINES` → **warning, not fatal** (Day 166, #762).
//!    Incremental creep past the cap is not the event this rule was written
//!    for, and killing the whole task teaches nothing the warning cannot.
//!    Over the cap by **more** than the grace band → **fatal**: a module
//!    blowing 50+ lines past the cap in one task *is* the design event. That
//!    is the actual invariant, and it is what stays lethal.
//! 2. A file **on** the list growing past its recorded ceiling → depends on
//!    *how far* (Day 174; split by `REGISTER_DRIFT_GRACE_LINES`, exactly as
//!    Day 166 split branch 1). Up to 100 lines of drift → **warning, not
//!    fatal**: growth of an already-capped module is information, not an
//!    emergency, and a four-line overshoot does not deserve a whole-task
//!    revert. More than 100 → **fatal**, so the remedy (one pasted register
//!    line) lands in front of `scripts/evolve.sh`'s fix loop, which is the
//!    only reader this branch has ever had.
//!
//!    **Superseded claim, recorded rather than erased** (my own Day-165 rule):
//!    this doc used to say the warning meant "the debt register still gets
//!    updated on purpose rather than absorbed". That was false for eight days.
//!    Measured on Day 174: **11 register entries had absorbed drift**, between
//!    +1 and +480 lines, worst `src/cli.rs` (recorded 3845, actual 4325). The
//!    mechanism is my own "a capability is real only where something consumes
//!    it" lesson landing on my own gate — the warning goes to the stderr of a
//!    *passing* test, and the only consumer of `cargo test` in the evolve loop
//!    reads the **exit code**. Nothing read it, so nothing acted on it. The
//!    damage is to branch 3: a stale-high entry does not break the ratchet, it
//!    *loosens* it — `cli.rs` could have shed 480 lines with the ratchet never
//!    firing. Drift under 100 lines is still absorbable, and that is a
//!    deliberate tradeoff, not an oversight.
//! 3. A file on the list sitting **below** its recorded ceiling → **fatal**.
//!    This is the ratchet: an exception list only pays itself down if
//!    improving is also a failure, otherwise a shrunk file keeps silent
//!    headroom nobody decided to grant. Fatal on purpose, and it is the cheap
//!    direction — the fix is the smaller number, printed verbatim in the
//!    message. (Same for a listed file that shrank under the cap entirely, or
//!    vanished: its entry must be deleted.)
//!
//! Both warnings (branch 1's grace band and branch 2a) are written straight to
//! `std::io::stderr()` through one shared helper rather than through
//! `eprintln!`, because libtest captures the macros and swallows output from
//! *passing* tests — and a silent gate teaches nothing at all, which is worse
//! than a fatal one.
//!
//! `Kind: evolve` — this governs my own repo's growth discipline; no product
//! surface changes.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Maximum lines allowed in a single `src/` module.
///
/// Chosen at 2,000 because 24 of my 79 modules were already over it on Day
/// 157 (largest 4,714) — low enough to bite, high enough that the debt
/// register is readable. Raising this number is a deliberate edit; never
/// hardcode a different limit elsewhere.
const MAX_MODULE_LINES: usize = 2_000;

/// A module over `MAX_MODULE_LINES` by at most this many lines warns instead
/// of failing.
///
/// Judgment threshold, not a measurement. The gate exists to stop a module
/// going oversized as a *design event*; it was instead killing whole correct
/// tasks over incremental overshoot (#739 died to a **four-line** overshoot,
/// #719 to the same shape). A fix that adds a handful of lines to an
/// already-large module is not the event this rule was written for, and
/// reverting the task teaches nothing the warning cannot.
///
/// The accepted tradeoff: a module may creep to `MAX_MODULE_LINES + 50` lines
/// without stopping a task. What keeps that visible rather than free is the
/// register — the moment a file is added to `GRANDFATHERED_OVERSIZED_MODULES`
/// the ratchet (branch 3) makes every later shrink a failure, so headroom can
/// never be granted silently.
const OVERSHOOT_GRACE_LINES: usize = 50;

/// A **grandfathered** module that grew past its recorded ceiling by at most
/// this many lines warns (branch 2a); beyond it, fatal (branch 2b).
///
/// Judgment threshold, not a measurement — nothing measured says 100 is the
/// right number. The argument is the shape of the two failure modes: a single
/// task rarely adds 100 lines to an already-oversized module, so a crossing is
/// a real event worth stopping for, while the +1/+2/+3 creep that made up most
/// of the Day-174 drift list stays a warning and never reverts a correct task
/// (which is the whole reason branch 2 was made non-fatal on Day 165).
///
/// Why it needed a fatal side at all: from Day 166 to Day 174 the warning ran
/// with **no reader**. It goes to the stderr of a *passing* test, and the only
/// consumer of `cargo test` in my evolve loop reads the exit code — so eleven
/// register entries silently absorbed between +1 and +480 lines. Stale-high
/// entries do not break branch 3's ratchet, they *loosen* it: `src/cli.rs`
/// recorded 3845 against an actual 4325 could have shed 480 lines with the
/// ratchet never firing, which is the exact silent headroom branch 3 exists to
/// prevent. Making large drift fatal hands the warning the one reader that
/// already exists — `scripts/evolve.sh`'s fix loop — and the remedy is a
/// single pasted line, which is precisely what that loop handles well.
///
/// The accepted tradeoff, stated rather than papered over: drift under 100
/// lines is still absorbable, and that is deliberate. This is not a crusade
/// against every +1.
const REGISTER_DRIFT_GRACE_LINES: usize = 100;

/// Modules already over `MAX_MODULE_LINES` when the gate was installed
/// (Day 157). Each number is the file's **recorded size**: growing past it
/// warns or fails depending on how far (branch 2, split Day 174), sitting
/// under it fails (branch 3, the ratchet), so the entry tracks reality in
/// both directions and the register only shrinks.
///
/// **Day 174 sweep.** Eleven of these entries were stale-high — the branch-2
/// warning had run for eight days with no reader, absorbing between +1 and
/// +480 lines per entry. Each `// Day 174: +N absorbed` note below records the
/// drift that entry had accumulated by then; they are paid off in one pass,
/// and the same commit makes drift past `REGISTER_DRIFT_GRACE_LINES` fatal so
/// the next large one cannot be absorbed the same way.
const GRANDFATHERED_OVERSIZED_MODULES: &[(&str, usize)] = &[
    // Day 181 (#846): 2047 — the snapshot dedup moved from a tail read to a set
    // read, plus its emission-point tests. Registered rather than left in the
    // grace band because 2047 leaves 3 lines before the fatal branch, i.e. the
    // next task touching this file would be reverted by the overshoot rather
    // than by anything it did. Registering names the debt and hands it to the
    // branch-3 ratchet; the split (the ledger *readers* here are a separate
    // concern from the *writers*) is real follow-up work, not done here.
    ("src/commands_risk_snapshots.rs", 2047),
    // Day 163 (#715): +4 lines — parent-side SharedStateTool so the documented RLM
    // store-then-reference step is executable.
    // Day 174: +3 absorbed since Day 166 — the warning branch had no reader.
    ("src/agent_builder.rs", 3506),
    // Day 164 (#728): +98 lines — `/skill install`'s destination becomes a third
    // auto-discovery source, so an explicitly installed skill actually loads.
    // The two near-identical per-directory blocks were collapsed into one loop
    // over a pure `auto_discovery_sources` list first (that dedup is why this is
    // +98 and not more); the rest is the new source, its doc comments, and three
    // tests pinning the precedence order (installed < global < project).
    // Day 174: +480 absorbed since Day 166 — the single worst entry on the
    // register, and the concrete damage: cli.rs could have shed 480 lines and
    // branch 3's ratchet would never have fired. Nothing read the warning.
    // Day 178 (#749 item 1): +231 lines — persisted per-directory workspace trust
    // (`--trust-project-always`), its two announcement builders, and their tests.
    // Acknowledged, not absorbed: this crossed the 100-line drift grace band, which
    // is the gate working as designed.
    // Day 184: 5604 -> 5813 (+209), the fifth gate on the project-config trust
    // boundary — `gate_project_notify_command` + `project_notify_refusal_message`
    // and their table tests. A project-local `notify_command` is arbitrary shell
    // (`sh -c`), so it is executable code by the same test #820 applied to hooks.
    ("src/cli.rs", 6620),
    // Day 162 (#698): +12 lines — SUPPORTED_IMAGE_FORMATS single source of truth
    // (bmp removed; API only accepts png/jpg/jpeg/gif/webp) plus regression tests
    // pinning the extension↔MIME agreement. Tests must live in this module.
    // Day 162 (#699): +117 lines — /apply cascade detects a tree mutated by a
    // failed --3way (which writes conflict markers on merge conflict), stops
    // before running -C1/--recount against the dirty state, and reports the
    // conflicted files honestly instead of "all strategies failed".
    // Day 162 fmt: +7 lines — `cargo fmt` reflowed the #699 code after the
    // ceiling was recorded at pre-fmt size. No new code; formatter wrapping only.
    // Day 162 (#697): +38 lines — handle_add now returns the successfully-added
    // paths alongside its results (so /add related-file suggestions derive from
    // actual adds, not an input re-parse), plus the regression test pinning that
    // failed adds and URL args never leak into that list.
    // Day 162 (#704): +65 lines — @mention read failures on EXISTING files now
    // warn on stderr (mention_read_warning helper, both Err arms) instead of
    // silently re-emitting the raw @path; plus tests pinning the warning string,
    // free-form-mention silence, and the unreadable-file behavior.
    // Day 174: +2 absorbed since Day 166.
    ("src/commands_file.rs", 2804),
    ("src/commands_git.rs", 3172),
    // Day 174: +25 absorbed since Day 166.
    // +103 (Day 179, #832): the `/evolution` cargo shell-out was split into a
    // thin wrapper + injected resolvers so no `#[test]` spawns `cargo`, plus a
    // source-level guard pinning that. Acknowledged, not absorbed.
    ("src/commands_info.rs", 3379),
    // Day 163 (#726): -59 lines — emerging-risk prompt injection removed
    // (map, annotation, helper, and the test pinning them); see #724.
    // Day 174: +3 absorbed since Day 166.
    // Day 183: +1 absorbed since Day 179 (#837's auto-context scoring change).
    // Paid off rather than left as a third unread warning.
    ("src/commands_project.rs", 3640),
    // Day 162 (#708): +40 lines — classify_broke_files now filters to `src/`
    // (the risk model's whole universe), plus its unit test and the updated
    // end-to-end fixture assertions.
    // Day 163: +280 lines — classify_broke_files gained two-tier corroboration
    // (a lone `Fix #710` delivery commit no longer grades as a failure day),
    // which is mostly doc comment + a fixture-table test + a second verbatim
    // git-log capture for the corroborated red-branch proof.
    // Day 163 (#717): +131 lines — uncorroborated-repair third value
    // (has_uncorroborated_repair_evidence + its green-branch call site) plus the
    // three fixture tests pinning flagged / green / corroborated windows.
    // Day 163 (corroboration): +184 lines — is_mechanical_commit (harness
    // bookkeeping commits are not a second opinion) plus three fixture tests
    // pinning the cargo-fmt window as ungraded, the real-corroboration window
    // as red, and the harness vocabulary the filter is keyed to.
    // Day 174: +91 lines — the risk-score universe is now filtered to paths that
    // exist on disk (`scorable_paths` + its table test). A file's deletion commit
    // is itself churn, so a deleted file was earning a fresh score plus a
    // guaranteed never-forecast status and leading the list that steers the
    // planner. Recorded deliberately rather than absorbed.
    ("src/commands_risk.rs", 6479),
    // Day 162 (#707): +68 lines — format_project_index no longer byte-slices a
    // path tail (live panic on any non-ASCII path >50 bytes) and measures its
    // column in chars; 62 of the 68 lines are the two regression tests, one of
    // which asserts the fixture is genuinely boundary-violating so it can't
    // drift back to ASCII-safe like the old test did.
    // Day 163 (#706): +118 lines — parse_grep_args gained a `--` end-of-flags
    // terminator and honest loser-branches (a value-taking flag with no usable
    // value is now a literal pattern token instead of being silently swallowed),
    // plus the fixture tests covering both the new paths and the untouched ones.
    ("src/commands_search.rs", 3872),
    // Day 163 (#716): +99 lines — spawn_dir_restrictions confines a spawn
    // worker's file tools to its worktree (bash_cwd only pinned bash), plus
    // three regression tests covering no-worktree passthrough, the confined
    // common case, and widening a human-set allow list while preserving deny.
    ("src/commands_spawn.rs", 4252),
    // Day 162 (#692): +108 lines — extract_last_assistant_text now stops at the
    // newest turn's boundary (no stale-turn fallback) plus the regression tests
    // pinning that a text-less newest turn yields None, not an older turn's text.
    ("src/commands_web.rs", 2415),
    // Day 164 (#732): +105 lines — TOML basic-string escaping on the write
    // side, matching unescaping (and a lone-quote panic fix) on the read side,
    // plus the round-trip tests that pin writer and reader as one promise.
    // Day 174: +256 absorbed since Day 166.
    ("src/config.rs", 3927), // Day 186: +158 for the chained-command allow guard and its tests.
    // Day 165: 2307 -> 2296. Not a shrink I made this session — the entry was
    // stale-high, and branch 3 (below-ceiling is fatal) is what finally said so.
    // Day 184: 2321 -> 2337 (+16). NOT this task's diff — `git diff src/dispatch.rs`
    // is empty; this is pre-existing drift the gate had been warning about, from the
    // `/cd` trust re-evaluation call site (`reevaluate_trust_on_cd` +
    // `trust_changed_on_cd_message`, ~:1423). Paid here rather than absorbed: Day 174
    // measured 11 entries carrying +1 to +480 because this branch absorbed drift for
    // eight days, and a warning nobody pays is how the third accumulation starts.
    ("src/dispatch.rs", 2338),
    // Day 187 (#886): 2000 -> 2160, i.e. sitting EXACTLY at the cap and
    // un-registered, then +160 for the `yoyo model` route — the pure
    // `parse_model_subcommand` / `model_refusal_message` pair, the dispatch arm,
    // and their emission-point tests. `yoyo model list` was spending a billed LLM
    // turn with write-capable tools to answer a question `handle_model_list`
    // answers deterministically, so the route is the fix and this is its cost.
    // REGISTERED rather than SPLIT, and the reason is the gate's own Day-183
    // precedent (`src/prompt_retry_limits.rs`): registering is the remedy this
    // gate prints, while the better fix is a split — deliberately not done here
    // because a half-landed pure move is a build failure and a reverted session,
    // and this file is the CLI dispatcher every subcommand route passes through.
    // The next task that has to grow this file should split it, not bump this.
    ("src/dispatch_sub.rs", 2160),
    ("src/format/cost.rs", 2539), // Day 183: +133 for the cache-ratio provenance guards (denominator pinned to upstream, NaN contract, emission-point tie + near-miss).
    // Day 183 (#865): 1763 -> 2044, i.e. 44 past the cap and inside the 50-line
    // grace band, for Python triple-quoted strings carried across lines (the
    // `TripleQuote` open/close branch plus 36 emission-point tests, most of them
    // the near-miss guards that pin every other language byte-identical).
    // REGISTERED RATHER THAN SPLIT, and the distinction matters: registering is
    // this gate's own stated remedy for a grace-band overshoot, while the better
    // fix is a split (precedent: `src/format/highlight_lang.rs`, carved out of
    // this same file on Day 174 for this same reason). Deliberately not done in
    // this pass because a half-landed pure move is a build failure and a reverted
    // session, while a register edit cannot half-land. The file now sits 7 lines
    // from fatal, so the *next* task here is the split, not another entry.
    ("src/format/highlight.rs", 2044),
    // Day 162 (#661): +228 lines — bounded inline-marker carry across streaming
    // deltas (split `**bo` + `ld**` pairs now render bold) plus the
    // chunking-independence and carry-safety regression tests.
    // Day 174: +17 absorbed since Day 166.
    ("src/format/markdown.rs", 3177),
    // Day 174: +1 absorbed since Day 166.
    // Day 177: 2456 → 2568 (+112, past the 100-line grace band, so the gate
    // demanded this line rather than absorbing it). The growth is the #780-class
    // race paydown: two pure `*_with`-style cores lifted out of `print_usage` /
    // `print_context_usage` / `contextual_hint` with their doc comments, one
    // source-level wrapper pin, and three test bodies that grew because they
    // went from asserting nothing to asserting both directions.
    // Day 183: 2568 -> 2629, +61 absorbed since Day 177 (the `color_enabled`
    // cfg(test) pin and the three pure-core splits that paid the shared-global
    // register down). Inside REGISTER_DRIFT_GRACE_LINES, so it warned rather than
    // failed — and warned unread for six days, which is the finding this task fixes.
    ("src/format/mod.rs", 2629),
    // Day 162 (#665): +27 lines — the test-output filter is now gated on tool
    // provenance, so read_file results stop being eaten. Signature recorded
    // retroactively during Day 162 reflection: the raise itself shipped
    // unattributed in commit 6e446f09.
    // Day 164: +45 lines — provenance corroboration gate for filter_test_output
    // (a `✓` glyph is a shape, a runner summary is provenance) + its regression tests.
    ("src/format/output.rs", 2885),
    ("src/help.rs", 2759), // Day 188: +4 paid off (was 2755 at Day 187).
    // Day 161 (#662 half 1): +9 lines — run_prompt_auto_retry now breaks out of
    // the retry loop (with one dim stderr line) on deterministic tool refusals
    // instead of burning MAX_AUTO_RETRIES on an identical answer.
    // Day 162 (#662 half 2): +9 lines — the same block mirrored verbatim into
    // run_prompt_auto_retry_with_content, so both retry drivers stop on
    // deterministic refusals.
    // Day 162 (#686): +68 lines — REFUSAL_NOTICE_MARKER + the pure
    // `refusal_notice` builder that makes the harness's grep contract
    // mechanical, plus the test that pins the emitted bytes.
    // Day 165 (#683 step 2): +34 lines — the two agent-start seams
    // (`start_prompt` / `start_prompt_messages`) that route all four prompt
    // call sites through one place so GASP recording is on for all of them or
    // none. Raised on purpose: the seam belongs beside the call sites it
    // replaces, and splitting four one-line calls into another module would
    // hide the enumeration this task exists to make checkable.
    // Day 174: +429 absorbed since Day 166 — second-worst on the register, and
    // 429 lines of ratchet slack the warning branch never got anyone to close.
    ("src/prompt.rs", 3561),
    // Day 183: first entry for this file — 2042, i.e. 42 past MAX_MODULE_LINES and
    // so inside OVERSHOOT_GRACE_LINES, which is why it warned instead of failing.
    // Registering it is this gate's OWN stated remedy for a grace-band overshoot
    // ("split it, or add (...) with a reason"), not a way around it: the entry
    // converts silent headroom into a ratcheted ceiling that branch 3 defends.
    // The better fix is a SPLIT — precedent `src/prompt_retry_limits.rs`, carved
    // out of this very file on Day 177 for this very reason — deliberately not
    // done here: a half-landed pure move is a build failure and a reverted
    // session, while a register edit cannot half-land. It was 8 lines from fatal
    // with #855 (open, agent-self) queued against this exact file.
    ("src/prompt_retry.rs", 2486),
    // Day 162 (#689): +14 lines — double Ctrl+C at the idle REPL prompt now
    // exits (consecutive-flag `ctrl_c_armed`, dim hint on first press).
    // Day 174: +91 absorbed since Day 166.
    ("src/repl.rs", 3358),
    // Day 174: raised 3269 -> 3490 for `git_redirection_refusal_message` +
    // `classify_redirection_reason` (~76 lines) and their emission-point tests
    // (~146) — the worktree-confinement refusal now names the accepted
    // alternatives, branching so the env class is never offered a hatch that
    // would also be refused. The message belongs beside the detector whose
    // reason string it classifies; splitting them would create the second
    // matcher this deliberately avoids.
    ("src/safety.rs", 4425),
    // Day 175 (#816): first entry for this file — 1882 → 2067, +185. The setup
    // wizard becomes the second and third consumer of the #735 shadow/demotion
    // guard family (which had exactly one: `/config set`), so both of its save
    // arms now say when the file they just wrote is shadowed, or has silently
    // demoted the config yoyo was reading. ~60 of the +185 are the two arms and
    // the one shared pure writer; ~125 are tests, including the emission-point
    // one that pins the string a user actually reads out of the wizard's writer.
    // Registered rather than trimmed on purpose: the only way to land under the
    // cap here was to delete the tests and the "why" comment, i.e. trade the
    // legibility of the fix for a line count. A split is the real answer and is
    // NOT free — this module's ~1200 lines of tests reach ~20 private helpers
    // through `use super::*`, so moving them out means making all of those
    // `pub(crate)` first. Filed as the follow-up rather than half-done here.
    ("src/setup.rs", 2067),
    ("src/symbols.rs", 3804),
    // Day 161 (#662 half 1): +10 lines — pub REFUSAL_STEM_* consts that the
    // wrapper messages and prompt_retry::is_deterministic_tool_error share.
    // Day 162 (#709): raised 3748 -> 3894 for the Arc flavour of the mode
    // guard (two constructors) + three Arc-path enforcement tests. The tests
    // are ~120 of those lines; the guard itself is one type, not a second copy.
    // Day 163 (#710): raised 3894 -> 3964. The production change is a 4-line
    // short-circuit in RecoveryHintTool; the ~70 lines are tests the task
    // required — the helper's per-stem cases plus the both-sides wrapper
    // discriminator (verbatim message + counter unmoved vs hint + bump).
    // Day 164: +4 lines — the #665 fixture gained the runner summary line a real
    // runner emits; without it the fixture asserted the ✓-shape-only collapse.
    // Day 186: 5187 -> 5276, pasted verbatim from what the gate itself printed
    // (+89, inside REGISTER_DRIFT_GRACE_LINES = 100 and so a warning, not fatal).
    // Deliberately registered rather than split: a half-landed pure move is a
    // build failure and a reverted session, while a register edit cannot
    // half-land — the same reasoning that registered src/prompt_retry.rs on
    // Day 183. The drift was surfaced by scripts/extract_trajectory.py's
    // module-size section, which exists precisely so this is acted on rather
    // than accumulating a third time (Day 183: the recurrence is the finding).
    ("src/tool_wrappers.rs", 5276),
    // Day 162 (#709): raised 3245 -> 3264 to wrap the sub-agent tool list in
    // the mode guard, plus the comment stating what is enforced and what is not.
    // Day 163 (#714): raised 3264 -> 3290 — RenameSymbolTool now carries the
    // session's DirectoryRestrictions (struct + constructor + denied-file
    // reporting in execute), so rename_symbol stops writing across --deny'd
    // directories. The rename logic itself lives in src/commands_rename.rs.
    // Day 174: 3290 -> 3299. Only +3 of that is this task (the inline refusal
    // string became a call to safety::git_redirection_refusal_message); the
    // other +6 predates it and was already showing as unrecorded growth.
    ("src/tools.rs", 4037),
    // Day 163 (#726): -58 lines — emerging-risk annotation removed from
    // build_watch_fix_prompt, with its own test; see #724.
    ("src/watch.rs", 4295),
];

/// A way the size gate can be violated. Five distinct values on purpose —
/// "a new file crept just past the cap", "a new file blew way past the cap",
/// "a known-big file got bigger", "a known-big file's recorded size is
/// stale-high", and "the debt register lists a file that no longer belongs"
/// are different problems with different fixes, and collapsing them into one
/// string would hide which one happened. Two of them are non-fatal; see
/// `is_fatal`.
#[derive(Debug, PartialEq, Eq)]
enum SizeViolation {
    /// A module not on the grandfather list crossed the cap by more than
    /// `OVERSHOOT_GRACE_LINES` — the design event this gate exists for.
    OverCap { path: String, lines: usize },
    /// A module not on the grandfather list crossed the cap, but by at most
    /// `OVERSHOOT_GRACE_LINES`. Creep, not a design event: warn and move on.
    OverCapWithinGrace { path: String, lines: usize },
    /// A grandfathered module grew past its recorded ceiling, but by at most
    /// `REGISTER_DRIFT_GRACE_LINES`. Creep, not a design event: warn.
    GrewPastCeiling {
        path: String,
        lines: usize,
        ceiling: usize,
    },
    /// A grandfathered module grew more than `REGISTER_DRIFT_GRACE_LINES` past
    /// its recorded ceiling. Fatal (Day 174): the warning branch had no reader
    /// for eight days and absorbed up to +480 lines, and a stale-high entry is
    /// slack in branch 3's ratchet.
    GrewFarPastCeiling {
        path: String,
        lines: usize,
        ceiling: usize,
    },
    /// A grandfathered module is smaller than its recorded ceiling by at most
    /// `REGISTER_DRIFT_GRACE_LINES` (but still over the cap) — creep down, not
    /// a design event: warn (Day 187, #885). The mirror of `GrewPastCeiling`.
    ShrankWithinGrace {
        path: String,
        lines: usize,
        ceiling: usize,
    },
    /// A grandfathered module is smaller than its recorded ceiling by more
    /// than `REGISTER_DRIFT_GRACE_LINES` (but still over the cap) — the entry
    /// grants headroom nobody decided to give.
    StaleCeiling {
        path: String,
        lines: usize,
        ceiling: usize,
    },
    /// A grandfathered module dropped back under the cap (or vanished) —
    /// its entry should be removed so the register keeps shrinking.
    StaleGrandfatherEntry { path: String, lines: Option<usize> },
}

impl SizeViolation {
    /// Whether this violation should fail the test run.
    ///
    /// Two kinds are non-fatal, and both are the same judgment: *incremental*
    /// growth is information, not an emergency.
    ///
    /// - `GrewPastCeiling` — a grandfathered module got bigger. That is the
    ///   branch which cost me two whole tasks (#719, #739): a correct fix
    ///   reverted because a file I had *already* signed off as oversized got
    ///   four lines bigger.
    /// - `OverCapWithinGrace` — an unlisted module crept at most
    ///   `OVERSHOOT_GRACE_LINES` past the cap (Day 166, #762). Same shape,
    ///   same price paid, one branch over.
    ///
    /// Every other kind stays fatal. `OverCap` — now meaning *past the cap by
    /// more than the grace band* — is the real invariant, and both
    /// stale-register kinds are the ratchet: if improving a file is not also a
    /// failure, the register never pays itself down. They are also the *cheap*
    /// direction — each message states the exact edit verbatim.
    fn is_fatal(&self) -> bool {
        match self {
            SizeViolation::OverCap { .. } => true,
            SizeViolation::OverCapWithinGrace { .. } => false,
            SizeViolation::GrewPastCeiling { .. } => false,
            SizeViolation::GrewFarPastCeiling { .. } => true,
            SizeViolation::ShrankWithinGrace { .. } => false,
            SizeViolation::StaleCeiling { .. } => true,
            SizeViolation::StaleGrandfatherEntry { .. } => true,
        }
    }

    fn message(&self) -> String {
        match self {
            SizeViolation::OverCap { path, lines } => format!(
                "{path} is {lines} lines, {} past the {MAX_MODULE_LINES}-line module cap — \
                 more than the {OVERSHOOT_GRACE_LINES}-line grace band, so this one is fatal.\n     \
                 Fix: split it. If growth is genuinely intended, add \
                 (\"{path}\", {lines}) to GRANDFATHERED_OVERSIZED_MODULES with a reason \
                 in the commit message.",
                lines.saturating_sub(MAX_MODULE_LINES),
            ),
            SizeViolation::OverCapWithinGrace { path, lines } => format!(
                "{path} is {lines} lines, {} past the {MAX_MODULE_LINES}-line module cap.\n     \
                 Not fatal — within the {OVERSHOOT_GRACE_LINES}-line grace band, and creep past \
                 the cap is information, not the design event this gate exists for.\n     \
                 Fix: split it, or add (\"{path}\", {lines}) to \
                 GRANDFATHERED_OVERSIZED_MODULES with a reason in the commit message.",
                lines.saturating_sub(MAX_MODULE_LINES),
            ),
            SizeViolation::GrewPastCeiling {
                path,
                lines,
                ceiling,
            } => format!(
                "{path} grew to {lines} lines, {} past its recorded {ceiling}.\n     \
                 Not fatal — growth of an already-capped module is information, not an \
                 emergency.\n     Fix: paste (\"{path}\", {lines}) over its entry in \
                 GRANDFATHERED_OVERSIZED_MODULES (and say why in the commit message), or \
                 move the new code to a smaller module.",
                lines.saturating_sub(*ceiling),
            ),
            SizeViolation::GrewFarPastCeiling {
                path,
                lines,
                ceiling,
            } => format!(
                "{path} grew to {lines} lines, {} past its recorded {ceiling} — more than the \
                 {REGISTER_DRIFT_GRACE_LINES}-line register-drift grace band, so this one is \
                 fatal.\n     Fix: paste (\"{path}\", {lines}) over its entry in \
                 GRANDFATHERED_OVERSIZED_MODULES (and say why in the commit message), or \
                 move the new code to a smaller module.",
                lines.saturating_sub(*ceiling),
            ),
            SizeViolation::ShrankWithinGrace {
                path,
                lines,
                ceiling,
            } => format!(
                "{path} is down to {lines} lines from its recorded {ceiling} — {} lines of \
                 headroom nobody decided to grant.\n     \
                 Not fatal — within the {REGISTER_DRIFT_GRACE_LINES}-line register-drift grace \
                 band, and shrinking a file is the direction this gate wants.\n     \
                 Fix: paste (\"{path}\", {lines}) over its entry in \
                 GRANDFATHERED_OVERSIZED_MODULES.",
                ceiling.saturating_sub(*lines),
            ),
            SizeViolation::StaleCeiling {
                path,
                lines,
                ceiling,
            } => format!(
                "{path} is {lines} lines but its entry still records {ceiling} — {} lines of \
                 headroom nobody decided to grant, more than the \
                 {REGISTER_DRIFT_GRACE_LINES}-line register-drift grace band, so this one is \
                 fatal.\n     \
                 Fix: paste (\"{path}\", {lines}) over its entry in \
                 GRANDFATHERED_OVERSIZED_MODULES. Fatal on purpose: the register only \
                 ratchets down if a large shrink is also a failure.",
                ceiling.saturating_sub(*lines),
            ),
            SizeViolation::StaleGrandfatherEntry { path, lines } => match lines {
                Some(n) => format!(
                    "{path} is down to {n} lines, under the {MAX_MODULE_LINES}-line cap. \
                     Nice.\n     Fix: delete its entry from GRANDFATHERED_OVERSIZED_MODULES — \
                     the debt register only shrinks."
                ),
                None => format!(
                    "{path} is listed in GRANDFATHERED_OVERSIZED_MODULES but no longer exists.\n     \
                     Fix: delete its entry."
                ),
            },
        }
    }
}

/// The three outcomes for a module that is **not** on the grandfather list.
///
/// Pure, so the reprice (#762) is table-testable without touching real files:
/// the decision lives here, the I/O stays at the call site.
#[derive(Debug, PartialEq, Eq)]
enum UnlistedVerdict {
    /// At or under the cap — silent, byte-identical to the pre-#762 pass.
    Ok,
    /// Over the cap by at most `grace` lines — warn, run stays green.
    Grace,
    /// Over the cap by more than `grace` lines — fatal, the design event.
    Fatal,
}

/// Classify an unlisted module's line count against the cap and the grace band.
///
/// The grace band is inclusive: exactly `grace` lines over is still a warning,
/// `grace + 1` is fatal. A threshold has to land somewhere, and the side that
/// costs a whole task is the side that gets the benefit of the doubt.
fn classify_unlisted(lines: usize, max_lines: usize, grace: usize) -> UnlistedVerdict {
    if lines <= max_lines {
        UnlistedVerdict::Ok
    } else if lines - max_lines <= grace {
        UnlistedVerdict::Grace
    } else {
        UnlistedVerdict::Fatal
    }
}

/// The three outcomes for a module that **is** on the grandfather list, once
/// it is known to still be over `MAX_MODULE_LINES`.
///
/// Same split as `classify_unlisted` one branch over (Day 174): pure, so the
/// reprice is table-testable without touching real files, and all filesystem
/// I/O stays at the single call site.
#[derive(Debug, PartialEq, Eq)]
enum ListedVerdict {
    /// At or below the recorded ceiling — not a growth case at all. Branch 3
    /// (the ratchet) owns everything under the recorded number; this
    /// classifier deliberately says nothing about it.
    NotGrowth,
    /// Grew past the ceiling by at most `grace` lines — warn, run stays green.
    /// Byte-identical to the pre-Day-174 behaviour of the whole branch.
    Grace,
    /// Grew past the ceiling by more than `grace` lines — fatal, so the fix
    /// loop reads what eight days of unread warnings could not deliver.
    Fatal,
}

/// Classify a grandfathered module's line count against its recorded ceiling.
///
/// The grace band is inclusive, like `classify_unlisted`'s: exactly `grace`
/// lines of drift is still a warning, `grace + 1` is fatal. A threshold has to
/// land somewhere, and the side that costs a whole task gets the benefit of
/// the doubt.
fn classify_listed(lines: usize, recorded: usize, grace: usize) -> ListedVerdict {
    if lines <= recorded {
        ListedVerdict::NotGrowth
    } else if lines - recorded <= grace {
        ListedVerdict::Grace
    } else {
        ListedVerdict::Fatal
    }
}

/// The three outcomes for a grandfathered module that sits **at or below** its
/// recorded ceiling — branch 3, the ratchet (Day 187, #885).
///
/// The mirror of `ListedVerdict`, and it exists because the two directions had
/// wildly different prices for 22 days: growth got a 100-line grace band on Day
/// 174 while *any* shrink — even one line — stayed fatal. Nobody wrote that
/// asymmetry down as a decision; branch 2 got a grace band and branch 3 simply
/// never did. It traps a fix loop, because branch 2's printed remedy is a
/// **high-water mark**: paste `("src/cli.rs", 6557)`, then let any later edit
/// remove a line, and the run lands under the freshly-pasted ceiling with zero
/// slack. That is the "a guard that reads the world AFTER its own action"
/// lesson mirrored onto the *remedy*.
///
/// The decision lives here; the I/O stays at the single call site.
#[derive(Debug, PartialEq, Eq)]
enum ShrinkVerdict {
    /// At or above the recorded ceiling — not a shrink case at all. Branch 2
    /// owns everything above the recorded number; this classifier deliberately
    /// says nothing about it.
    NotShrink,
    /// Shrank below the ceiling by at most `grace` lines — warn, run stays
    /// green. Creep down, and the direction this gate wants.
    Grace,
    /// Shrank by more than `grace` lines — fatal. This is the ratchet, and it
    /// survives: a large shrink is real headroom nobody granted, and the
    /// remedy is one pasted line.
    Fatal,
}

/// Classify a grandfathered module that is at or below its recorded ceiling.
///
/// The boundary is inclusive and mirrors `classify_listed` exactly: a shrink of
/// `grace` lines warns, `grace + 1` is fatal.
fn classify_shrink(lines: usize, recorded: usize, grace: usize) -> ShrinkVerdict {
    if lines >= recorded {
        ShrinkVerdict::NotShrink
    } else if recorded - lines <= grace {
        ShrinkVerdict::Grace
    } else {
        ShrinkVerdict::Fatal
    }
}

/// Pure checker: given every module's line count and the grandfather list,
/// report every violation. No I/O, so it is testable against synthetic input
/// rather than only against whatever `src/` happens to look like today.
fn check_module_sizes(
    files: &[(String, usize)],
    max_lines: usize,
    grandfathered: &[(&str, usize)],
) -> Vec<SizeViolation> {
    let mut violations = Vec::new();

    for (path, lines) in files {
        match grandfathered.iter().find(|(g, _)| g == path) {
            Some((_, ceiling)) => {
                if *lines <= max_lines {
                    violations.push(SizeViolation::StaleGrandfatherEntry {
                        path: path.clone(),
                        lines: Some(*lines),
                    });
                } else {
                    match classify_listed(*lines, *ceiling, REGISTER_DRIFT_GRACE_LINES) {
                        // Branch 2a: creep past the recorded ceiling — warn.
                        ListedVerdict::Grace => violations.push(SizeViolation::GrewPastCeiling {
                            path: path.clone(),
                            lines: *lines,
                            ceiling: *ceiling,
                        }),
                        // Branch 2b (Day 174): large drift — fatal, so the fix
                        // loop becomes the reader the warning never had.
                        ListedVerdict::Fatal => {
                            violations.push(SizeViolation::GrewFarPastCeiling {
                                path: path.clone(),
                                lines: *lines,
                                ceiling: *ceiling,
                            })
                        }
                        // Branch 3 (Day 187, #885): at the ceiling is clean;
                        // below it splits by how far, exactly as growth does.
                        //
                        // Three cases stay FATAL and must NOT be loosened by a
                        // later "simplification":
                        //   1. dropping under MAX_MODULE_LINES entirely — the
                        //      debt is *paid* and the remedy is DELETE the
                        //      entry, a different edit from updating a number
                        //      (handled above, before this match);
                        //   2. a listed file that has vanished (handled below);
                        //   3. a shrink past the grace band — the ratchet.
                        ListedVerdict::NotGrowth => {
                            match classify_shrink(*lines, *ceiling, REGISTER_DRIFT_GRACE_LINES) {
                                ShrinkVerdict::NotShrink => {}
                                ShrinkVerdict::Grace => {
                                    violations.push(SizeViolation::ShrankWithinGrace {
                                        path: path.clone(),
                                        lines: *lines,
                                        ceiling: *ceiling,
                                    })
                                }
                                ShrinkVerdict::Fatal => {
                                    violations.push(SizeViolation::StaleCeiling {
                                        path: path.clone(),
                                        lines: *lines,
                                        ceiling: *ceiling,
                                    })
                                }
                            }
                        }
                    }
                }
            }
            None => match classify_unlisted(*lines, max_lines, OVERSHOOT_GRACE_LINES) {
                UnlistedVerdict::Ok => {}
                UnlistedVerdict::Grace => violations.push(SizeViolation::OverCapWithinGrace {
                    path: path.clone(),
                    lines: *lines,
                }),
                UnlistedVerdict::Fatal => violations.push(SizeViolation::OverCap {
                    path: path.clone(),
                    lines: *lines,
                }),
            },
        }
    }

    // A listed file that no longer exists on disk is its own third value —
    // not silently ignored, because a rename would otherwise retire a
    // ceiling without anyone deciding to.
    for (path, _) in grandfathered {
        if !files.iter().any(|(p, _)| p == path) {
            violations.push(SizeViolation::StaleGrandfatherEntry {
                path: (*path).to_string(),
                lines: None,
            });
        }
    }

    violations
}

/// Recursively collect `*.rs` files under `dir`, returning paths relative to
/// `root` with forward slashes (so the grandfather list reads the same on
/// every platform).
fn collect_rs_files(dir: &Path, root: &Path, out: &mut Vec<(String, usize)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => panic!("cannot read {}: {e}", dir.display()),
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        if path.is_dir() {
            collect_rs_files(&path, root, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, content.lines().count()));
        }
    }
}

/// Write the non-fatal half of the report to stderr.
///
/// One helper for **both** warning branches (an unlisted module inside the
/// grace band, and a grandfathered module that grew) so the two can never
/// drift apart in shape or in visibility.
///
/// Written to the raw stderr handle on purpose: libtest's capture hook only
/// intercepts the `print!`/`eprint!` macro family, and it discards captured
/// output from tests that PASS — which is exactly the case these branches
/// create. Going through the handle keeps the warnings visible in a plain
/// `cargo test` run, so "non-fatal" doesn't quietly become "silent".
fn write_warnings(warnings: &[&SizeViolation]) {
    if warnings.is_empty() {
        return;
    }
    let mut err = std::io::stderr();
    for w in warnings {
        let _ = writeln!(err, "\nmodule size gate WARNING: {}", w.message());
    }
    let _ = writeln!(
        err,
        "     ({} non-fatal size warning(s). Not failing the run — see \
         tests/module_size.rs for why.)\n",
        warnings.len()
    );
    let _ = err.flush();
}

#[test]
fn src_modules_respect_the_size_gate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &root, &mut files);

    assert!(
        files.len() > 10,
        "module walk found only {} files — the walk is broken, not the repo",
        files.len()
    );

    let violations = check_module_sizes(&files, MAX_MODULE_LINES, GRANDFATHERED_OVERSIZED_MODULES);
    let (fatal, warnings): (Vec<&SizeViolation>, Vec<&SizeViolation>) =
        violations.iter().partition(|v| v.is_fatal());

    write_warnings(&warnings);

    if !fatal.is_empty() {
        let report = fatal
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "module size gate failed ({} violation(s)):\n{report}\n\n\
             This gate lives in tests/module_size.rs. It is deliberate: growth has to be \
             acknowledged, not absorbed.",
            fatal.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(v: &[(&str, usize)]) -> Vec<(String, usize)> {
        v.iter().map(|(p, n)| ((*p).to_string(), *n)).collect()
    }

    #[test]
    fn small_unlisted_module_passes() {
        let v = check_module_sizes(&files(&[("src/a.rs", 100)]), 200, &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn unlisted_module_over_cap_is_a_violation() {
        // Day 166 (#762): still a violation, but a *reported* one rather than
        // a fatal one — 1 line over is inside the grace band.
        let v = check_module_sizes(&files(&[("src/a.rs", 201)]), 200, &[]);
        assert_eq!(
            v,
            vec![SizeViolation::OverCapWithinGrace {
                path: "src/a.rs".to_string(),
                lines: 201
            }]
        );
        assert!(!v[0].is_fatal(), "one line over must not revert a task");
    }

    #[test]
    fn unlisted_module_far_over_cap_is_fatal() {
        // Past the grace band, so the design event the gate was written for.
        let v = check_module_sizes(&files(&[("src/a.rs", 400)]), 200, &[]);
        assert_eq!(
            v,
            vec![SizeViolation::OverCap {
                path: "src/a.rs".to_string(),
                lines: 400
            }]
        );
        assert!(v[0].is_fatal());
    }

    #[test]
    fn classify_unlisted_covers_the_grace_band_and_both_near_misses() {
        // The whole reprice in one table. 200 = cap, 50 = grace band.
        for (lines, want) in [
            (0, UnlistedVerdict::Ok),
            (199, UnlistedVerdict::Ok),
            // at the cap → silent, byte-identical to before #762
            (200, UnlistedVerdict::Ok),
            // one line over → warning, NOT a failure (#739 died to four)
            (201, UnlistedVerdict::Grace),
            // exactly the grace band → still a warning (inclusive boundary)
            (250, UnlistedVerdict::Grace),
            // the near-miss on the other side → fatal
            (251, UnlistedVerdict::Fatal),
            (5_000, UnlistedVerdict::Fatal),
        ] {
            assert_eq!(
                classify_unlisted(lines, 200, 50),
                want,
                "{lines} lines against cap 200, grace 50"
            );
        }
    }

    #[test]
    fn grace_band_uses_the_real_const_at_the_real_cap() {
        // The table above drives synthetic numbers; this one pins that the
        // shipped constants are what the checker actually applies.
        assert_eq!(
            classify_unlisted(
                MAX_MODULE_LINES + OVERSHOOT_GRACE_LINES,
                MAX_MODULE_LINES,
                OVERSHOOT_GRACE_LINES
            ),
            UnlistedVerdict::Grace
        );
        assert_eq!(
            classify_unlisted(
                MAX_MODULE_LINES + OVERSHOOT_GRACE_LINES + 1,
                MAX_MODULE_LINES,
                OVERSHOOT_GRACE_LINES
            ),
            UnlistedVerdict::Fatal
        );
    }

    #[test]
    fn grace_band_warning_names_the_overshoot_and_the_paste_in_entry() {
        // Same actionability as branch 2's warning: file, count, cap,
        // overshoot, and the literal register line to paste.
        let m = SizeViolation::OverCapWithinGrace {
            path: "src/a.rs".to_string(),
            lines: 2004,
        }
        .message();
        assert!(m.contains("src/a.rs"), "{m}");
        assert!(m.contains("2004"), "{m}");
        assert!(m.contains(&MAX_MODULE_LINES.to_string()), "{m}");
        assert!(m.contains("4 past"), "{m}");
        assert!(m.contains("(\"src/a.rs\", 2004)"), "{m}");
        assert!(m.contains("Not fatal"), "{m}");
    }

    #[test]
    fn over_cap_message_names_the_grace_band_it_exceeded() {
        // A reader must be able to tell WHICH of the two branches they hit.
        let m = SizeViolation::OverCap {
            path: "src/a.rs".to_string(),
            lines: 2100,
        }
        .message();
        assert!(m.contains("100 past"), "{m}");
        assert!(m.contains(&OVERSHOOT_GRACE_LINES.to_string()), "{m}");
        assert!(m.contains("grace band"), "{m}");
    }

    #[test]
    fn exactly_at_cap_passes() {
        let v = check_module_sizes(&files(&[("src/a.rs", 200)]), 200, &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn grandfathered_module_at_its_ceiling_passes() {
        let v = check_module_sizes(&files(&[("src/a.rs", 500)]), 200, &[("src/a.rs", 500)]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn grandfathered_module_far_below_its_ceiling_is_a_stale_ceiling() {
        // Was `grandfathered_module_may_shrink_while_still_over_cap`, which
        // asserted this passes. Day 165 flipped it: a shrink that leaves the
        // recorded number untouched is silent headroom, so it is fatal and the
        // entry must be rewritten. That is the ratchet.
        //
        // Day 187 (#885) repriced it a second time and this fixture moved with
        // it: the shrink is now 200 lines, past REGISTER_DRIFT_GRACE_LINES, so
        // it still exercises the fatal half. It used to be 500 -> 400, a
        // 100-line shrink, which is now the *grace* band — that case did not
        // disappear, it moved to the sibling test below. The assertion is
        // unchanged and the ratchet direction still stays fatal.
        let v = check_module_sizes(&files(&[("src/a.rs", 300)]), 200, &[("src/a.rs", 500)]);
        assert_eq!(
            v,
            vec![SizeViolation::StaleCeiling {
                path: "src/a.rs".to_string(),
                lines: 300,
                ceiling: 500
            }]
        );
        assert!(v[0].is_fatal(), "the ratchet direction must stay fatal");
    }

    #[test]
    fn grandfathered_module_shrinking_within_grace_warns_rather_than_reverting() {
        // Day 187 (#885). The exact fixture the test above used to carry: a
        // 100-line shrink, at the inclusive boundary. Fatal for 22 days, and
        // the branch that destroyed #884 — a `cargo test` failure means
        // `git reset --hard`, so its real price was the whole session beside it.
        let v = check_module_sizes(&files(&[("src/a.rs", 400)]), 200, &[("src/a.rs", 500)]);
        assert_eq!(
            v,
            vec![SizeViolation::ShrankWithinGrace {
                path: "src/a.rs".to_string(),
                lines: 400,
                ceiling: 500
            }]
        );
        assert!(!v[0].is_fatal(), "a 100-line shrink must not revert a task");

        // The remedy has to be pasteable, exactly as branch 2's is — the whole
        // point of a warning nobody can act on is that nobody acts on it.
        let msg = v[0].message();
        assert!(msg.contains("(\"src/a.rs\", 400)"), "{msg}");
        assert!(msg.contains("Not fatal"), "{msg}");
    }

    #[test]
    fn shrink_grace_boundary_is_inclusive_on_both_sides() {
        // A discriminator tested only on the side that fires is vacuous green,
        // so both sides of the boundary are pinned at the emission point.
        let grace = check_module_sizes(
            &files(&[("src/a.rs", 500 - REGISTER_DRIFT_GRACE_LINES)]),
            200,
            &[("src/a.rs", 500)],
        );
        assert!(
            !grace[0].is_fatal(),
            "a shrink of exactly {REGISTER_DRIFT_GRACE_LINES} must warn: {grace:?}"
        );

        let fatal = check_module_sizes(
            &files(&[("src/a.rs", 500 - REGISTER_DRIFT_GRACE_LINES - 1)]),
            200,
            &[("src/a.rs", 500)],
        );
        assert!(
            fatal[0].is_fatal(),
            "one line past the band must stay fatal: {fatal:?}"
        );
    }

    #[test]
    fn classify_shrink_covers_the_band_and_both_near_misses() {
        // Mirrors `classify_listed_covers_the_drift_band_and_both_near_misses`
        // rather than inventing a second shape for the same question.
        let cases = [
            // At or above the ceiling is branch 2's business, not this one.
            (500, ShrinkVerdict::NotShrink),
            (501, ShrinkVerdict::NotShrink),
            (5_000, ShrinkVerdict::NotShrink),
            // Below it, up to and including the band: warn.
            (499, ShrinkVerdict::Grace),
            (400, ShrinkVerdict::Grace),
            // Past the band: fatal.
            (399, ShrinkVerdict::Fatal),
            (0, ShrinkVerdict::Fatal),
        ];
        for (lines, want) in cases {
            assert_eq!(
                classify_shrink(lines, 500, 100),
                want,
                "classify_shrink({lines}, 500, 100)"
            );
        }
    }

    #[test]
    fn shrink_grace_does_not_loosen_the_three_cases_that_stay_fatal() {
        // The near-miss guards, and they are the half that matters: #885 gave
        // *one* direction a grace band and must not have widened the others.

        // 1. Dropped under the cap entirely — the debt is paid and the remedy
        //    is DELETE the entry, a different edit from updating a number.
        let paid = check_module_sizes(&files(&[("src/a.rs", 150)]), 200, &[("src/a.rs", 500)]);
        assert_eq!(
            paid,
            vec![SizeViolation::StaleGrandfatherEntry {
                path: "src/a.rs".to_string(),
                lines: Some(150)
            }]
        );
        assert!(paid[0].is_fatal(), "a fully paid debt must still be fatal");

        // 2. A listed file that has vanished.
        let gone = check_module_sizes(&files(&[]), 200, &[("src/a.rs", 500)]);
        assert_eq!(
            gone,
            vec![SizeViolation::StaleGrandfatherEntry {
                path: "src/a.rs".to_string(),
                lines: None
            }]
        );
        assert!(gone[0].is_fatal(), "a vanished entry must still be fatal");

        // 3. Growth is byte-identical: the band and its near miss both hold.
        let grew = check_module_sizes(&files(&[("src/a.rs", 600)]), 200, &[("src/a.rs", 500)]);
        assert!(
            !grew[0].is_fatal(),
            "growth of 100 must still warn: {grew:?}"
        );
        let grew_far = check_module_sizes(&files(&[("src/a.rs", 601)]), 200, &[("src/a.rs", 500)]);
        assert!(
            grew_far[0].is_fatal(),
            "growth of 101 must still be fatal: {grew_far:?}"
        );

        // And sitting exactly on the ceiling still says nothing at all.
        let at = check_module_sizes(&files(&[("src/a.rs", 500)]), 200, &[("src/a.rs", 500)]);
        assert!(at.is_empty(), "{at:?}");
    }

    #[test]
    fn only_incremental_growth_is_non_fatal() {
        // Was `only_growth_of_a_grandfathered_module_is_non_fatal`. Day 166
        // (#762) added the second non-fatal kind: an unlisted module inside
        // the grace band. Both are the same judgment — creep is information —
        // and both are the branch that cost me #719 and #739.
        let grew = SizeViolation::GrewPastCeiling {
            path: "src/a.rs".to_string(),
            lines: 501,
            ceiling: 500,
        };
        assert!(!grew.is_fatal());

        let crept = SizeViolation::OverCapWithinGrace {
            path: "src/a.rs".to_string(),
            lines: 201,
        };
        assert!(!crept.is_fatal());

        for fatal in [
            SizeViolation::OverCap {
                path: "src/a.rs".to_string(),
                lines: 201,
            },
            // Day 174: large register drift joined the fatal side.
            SizeViolation::GrewFarPastCeiling {
                path: "src/a.rs".to_string(),
                lines: 900,
                ceiling: 500,
            },
            SizeViolation::StaleCeiling {
                path: "src/a.rs".to_string(),
                lines: 400,
                ceiling: 500,
            },
            SizeViolation::StaleGrandfatherEntry {
                path: "src/a.rs".to_string(),
                lines: Some(150),
            },
            SizeViolation::StaleGrandfatherEntry {
                path: "src/a.rs".to_string(),
                lines: None,
            },
        ] {
            assert!(fatal.is_fatal(), "{fatal:?} must stay fatal");
        }
    }

    #[test]
    fn classify_listed_covers_the_drift_band_and_both_near_misses() {
        // The Day-174 reprice in one table. 500 = recorded, 100 = grace band.
        for (lines, want) in [
            // below and at the recorded number are branch 3's business, not
            // this classifier's — it must not call either one "growth".
            (0, ListedVerdict::NotGrowth),
            (499, ListedVerdict::NotGrowth),
            (500, ListedVerdict::NotGrowth),
            // one line of drift → warning, exactly as before Day 174
            (501, ListedVerdict::Grace),
            // exactly the grace band → still a warning (inclusive boundary)
            (600, ListedVerdict::Grace),
            // the near-miss on the other side → fatal
            (601, ListedVerdict::Fatal),
            (5_000, ListedVerdict::Fatal),
        ] {
            assert_eq!(
                classify_listed(lines, 500, 100),
                want,
                "{lines} lines against recorded 500, grace 100"
            );
        }
    }

    #[test]
    fn drift_band_uses_the_real_const_at_a_real_recorded_size() {
        // The table drives synthetic numbers; this pins that the shipped
        // constant is what the checker actually applies. `recorded + 100`
        // warns, `recorded + 101` is fatal.
        let recorded = 2_464;
        assert_eq!(
            classify_listed(
                recorded + REGISTER_DRIFT_GRACE_LINES,
                recorded,
                REGISTER_DRIFT_GRACE_LINES
            ),
            ListedVerdict::Grace
        );
        assert_eq!(
            classify_listed(
                recorded + REGISTER_DRIFT_GRACE_LINES + 1,
                recorded,
                REGISTER_DRIFT_GRACE_LINES
            ),
            ListedVerdict::Fatal
        );
    }

    #[test]
    fn large_register_drift_is_fatal_end_to_end() {
        // Proven against a FABRICATED (file, recorded) pair — never by really
        // growing a module in src/, same discipline tests/orphan_modules.rs
        // uses for its fatal branch. These numbers are src/cli.rs's real Day-174
        // drift (3845 recorded, 4325 actual, +480), which is the instance that
        // motivated the reprice.
        let v = check_module_sizes(&files(&[("src/a.rs", 4325)]), 2_000, &[("src/a.rs", 3845)]);
        assert_eq!(
            v,
            vec![SizeViolation::GrewFarPastCeiling {
                path: "src/a.rs".to_string(),
                lines: 4325,
                ceiling: 3845
            }]
        );
        assert!(v[0].is_fatal(), "480 lines of drift must stop the run");
    }

    #[test]
    fn small_register_drift_stays_a_warning_end_to_end() {
        // The near-miss that must still pass through: the +1/+2/+3 creep that
        // made up most of the Day-174 list never reverts a task.
        let v = check_module_sizes(
            &files(&[("src/a.rs", 2_003)]),
            2_000,
            &[("src/a.rs", 2_000)],
        );
        assert_eq!(
            v,
            vec![SizeViolation::GrewPastCeiling {
                path: "src/a.rs".to_string(),
                lines: 2_003,
                ceiling: 2_000
            }]
        );
        assert!(!v[0].is_fatal(), "three lines of creep must stay non-fatal");
    }

    #[test]
    fn far_past_ceiling_message_names_the_drift_band_it_exceeded() {
        // A reader must be able to tell WHICH of the two branch-2 halves they
        // hit, and must get the literal register line to paste.
        let m = SizeViolation::GrewFarPastCeiling {
            path: "src/a.rs".to_string(),
            lines: 4325,
            ceiling: 3845,
        }
        .message();
        assert!(m.contains("src/a.rs"), "{m}");
        assert!(m.contains("4325"), "{m}");
        assert!(m.contains("3845"), "{m}");
        assert!(m.contains("480 past"), "{m}");
        assert!(m.contains("(\"src/a.rs\", 4325)"), "{m}");
        assert!(m.contains(&REGISTER_DRIFT_GRACE_LINES.to_string()), "{m}");
        assert!(m.contains("grace band"), "{m}");
    }

    #[test]
    fn growth_warning_names_the_overshoot_and_the_paste_in_entry() {
        // A warning nobody can act on is just noise, so pin the four things a
        // reader needs: the file, the recorded number, the current number, the
        // overshoot, and the literal entry to paste back.
        let m = SizeViolation::GrewPastCeiling {
            path: "src/a.rs".to_string(),
            lines: 2310,
            ceiling: 2306,
        }
        .message();
        assert!(m.contains("src/a.rs"), "{m}");
        assert!(m.contains("2310"), "{m}");
        assert!(m.contains("2306"), "{m}");
        assert!(m.contains("4 past"), "{m}");
        assert!(m.contains("(\"src/a.rs\", 2310)"), "{m}");
    }

    #[test]
    fn stale_ceiling_message_states_the_smaller_number_verbatim() {
        let m = SizeViolation::StaleCeiling {
            path: "src/a.rs".to_string(),
            lines: 2296,
            ceiling: 2307,
        }
        .message();
        assert!(m.contains("(\"src/a.rs\", 2296)"), "{m}");
        assert!(m.contains("11 lines of headroom"), "{m}");
    }

    #[test]
    fn grandfathered_module_growing_by_one_line_is_a_violation() {
        let v = check_module_sizes(&files(&[("src/a.rs", 501)]), 200, &[("src/a.rs", 500)]);
        assert_eq!(
            v,
            vec![SizeViolation::GrewPastCeiling {
                path: "src/a.rs".to_string(),
                lines: 501,
                ceiling: 500
            }]
        );
    }

    #[test]
    fn grandfathered_module_back_under_cap_must_be_delisted() {
        let v = check_module_sizes(&files(&[("src/a.rs", 150)]), 200, &[("src/a.rs", 500)]);
        assert_eq!(
            v,
            vec![SizeViolation::StaleGrandfatherEntry {
                path: "src/a.rs".to_string(),
                lines: Some(150)
            }]
        );
    }

    #[test]
    fn listed_module_that_no_longer_exists_is_a_stale_entry() {
        let v = check_module_sizes(&files(&[("src/b.rs", 10)]), 200, &[("src/a.rs", 500)]);
        assert_eq!(
            v,
            vec![SizeViolation::StaleGrandfatherEntry {
                path: "src/a.rs".to_string(),
                lines: None
            }]
        );
    }

    #[test]
    fn empty_input_yields_no_violations() {
        let v = check_module_sizes(&[], 200, &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn violation_messages_name_the_file_and_the_fix() {
        let v = SizeViolation::OverCap {
            path: "src/a.rs".to_string(),
            lines: 9000,
        };
        let m = v.message();
        assert!(m.contains("src/a.rs"), "{m}");
        assert!(m.contains("9000"), "{m}");
        assert!(m.contains("GRANDFATHERED_OVERSIZED_MODULES"), "{m}");
    }
}
