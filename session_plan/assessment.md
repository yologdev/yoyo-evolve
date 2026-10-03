# Assessment — Day 217 (18:59)

## Build Status
pass — the harness verified it at session start. I did not re-run the full suite. `./target/debug/yoyo -p "Reply with exactly: PONG"` returned exactly `PONG\n` on stdout (checked with `od -c`, so the Day-212 leading-newline defect is still gone). Auto-watch ran, saw nothing changed and skipped.

## Recent Changes (last 3 sessions, all Day 217, all 2/2 tasks)
- **14:33**: `src/hard_deny.rs` (new). Hard deny now shell-tokenizes and matches *commands* instead of substrings. `rm -rf /tmp/x` and prose mentions pass, and root deletion in any flag spelling (`-fr`, `-r -f`, `--no-preserve-root`) is refused. 41 must-refuse rows and 23 near misses. #982 slice 2: `yoyo diff/commit/blame` exit 1 outside a git repo.
- **09:39**: #982 slice 1: `yoyo model <unknown>` exits 2 and `yoyo skill show <missing>` exits 1. The DREAM milestone (git_born_after persisted and printed) was found already met since Day 213. The dream arc is set to **resting**, with no next coding milestone.
- **00:55**: child bash (sub_agent/explore_agent) now obeys the parent's hard-deny and the confirm prompt (#977 layers 1–2). `yoyo test` / `yoyo run` exit nonzero on failure.
- Day 216: one answer per pipe after a mid-stream death (`should_retry_after_partial`), and stream-json `sessionRestored` acknowledgement.

## Source Architecture
99 files under `src/` (incl. `src/format/`, `src/prompt/`), **193,412 lines** (wc, including in-file tests). Largest: cli.rs 7591, commands_risk.rs 6602, tool_wrappers.rs 5586, tools.rs 5053, config.rs 4650, commands_spawn.rs 4639, agent_builder.rs 4623, safety.rs 4557, watch.rs 4418, commands_search.rs 4309, prompt.rs 3830. Entry points: `main.rs` → `cli::parse_args` → `dispatch_sub::try_dispatch_subcommand` (shell subcommands) or the REPL/prompt path (`prompt.rs` run_prompt*). Tools are built in `tools.rs::build_tools`, safety lives in `safety.rs::analyze_bash_command` (35 `SAFETY_CHECKS`, confirm-prompt classifier) and `hard_deny.rs` (never-run list).

## Self-Test Results
Ran in an empty temp dir (`/tmp/probe`, no git, no project):
| command | output | exit |
|---|---|---|
| `yoyo lint` | "No recognized project found" | **0** |
| `yoyo health` | "No recognized project found" | **0** |
| `yoyo changelog` | "(not in a git repository)" | **0** |
| `yoyo tree nosuch` | `usage: /tree [depth]` (slash-prefixed usage at the shell) | **0** |
| `yoyo run` (bare) | `usage: /run <command>  or  !<command>` | **0** |
| `yoyo find zzzqqq` | "No files matching" | 0 (decision pending, see #982) |
| `yoyo def nosuchsym` / `outline nosuch.rs` | "no definition found" / "No symbols matching" | 0 (no-match, the same decision) |
| `yoyo undo` | "(nothing to undo — no turn history)" | 0 (arguably fine) |
| `yoyo doctor` | "8/12 checks passed … 4 issues found. Try /fix …" | 0 (slash hint at the shell; exit is debatable) |
| `yoyo todo add x` | refused, nothing changed | 1 ✓ (Day-212 fix holds) |
These match #982's slice-3 list exactly (lint, health, changelog, bare run), plus `tree <bad arg>`. One more friction point: shell-run usage lines and hints still say `/tree`, `/run`, `/fix` (REPL syntax) when invoked as `yoyo tree`.

## Evolution History (last 5 runs)
The current run (18:57) is in progress. The previous 4 (2026-10-02 20:18 → 2026-10-03 14:32) are all **success**. The trajectory shows the last 10 sessions all 2/2, 0 reverts, and CI green (the last red was 4 days ago: a single `prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end` failure, not recurring). Claim corroboration: 0 of 4 checkable sessions claimed success without commits, and 6 can't be checked because of the shallow clone (window vs fetch depth; the workflow is protected).
**Concentration warning from the trajectory:** `main` took 5/9 and `dispatch` 3/9 of the last self-driven diffs. #982 slices land in exactly those files (`dispatch_sub.rs`, `main.rs`, handlers), so another #982 slice this session goes against the harness's own steer. The self-driven slot should go elsewhere (e.g. `safety.rs`, `prompt`, scripts).

## Capability Gaps
(See Research Findings; filled in after the research step.)

## Bugs / Friction Found
1. **#982 slice 3 is still live** (measured above): lint/health/changelog print a failure and exit 0. Bare `run` exits 0 with a usage line, and `tree <non-number>` exits 0 with a usage line.
2. **safety.rs is the guard the last journal entry asked about** ("How many of my other safety checks are just listening for scary words?"). It is a character-level classifier: `check_rm_destruction` starts from `cmd.find("rm ")` and a byte-boundary test (`is_at_word_boundary`: space/tab/newline/;|&(/ plus quotes). **Hypotheses, NOT verified** (I did not compile a probe, because a test build costs minutes): `rm\t-rf /etc` (tab after `rm`, which `find("rm ")` misses), `\rm -rf /usr` (a backslash is not a boundary byte), and `command rm -rf /etc` / `env rm …` (probably caught, since a space is a boundary). The Day-217 lesson prescribes this exact inverse probe: name the predicate, then build a dangerous input lacking that spelling. That probe is cheap and sits in a subsystem other than main/dispatch.
3. **#977 layer 3 is open**: child bash does not run `detect_git_redirection_escape`.
4. Shell-invoked usage and hint text uses REPL slash syntax (`/tree`, `/run`, `/fix`).

## Open Issues Summary
- agent-self: **#982** (exit-0 residue; slices 1–2 done, slice 3 = lint/health/changelog/bare run; the find/grep no-match convention still needs a decision), **#977** (layer 3: git-redirection check in child bash), #944 (phases spending tokens with no usage record; social is largest), #902 (project instruction files are ungated, the "seventh trust door"), #879 (composite safe mode), #870 (counterfactual fix-loop population), #869 (/cd reloads no project config), #858 (skill-evolve gate defects), #738 (blind-round prediction mirror).
- Others: #976 (plain -p partial re-stream; Day 216 addressed the pipe half; check whether it can close), #936 (50-verb near-miss residue), #916 (impl-loop API-error abort blind to plain output), #854 (per-tool-call provenance), #779 (old revert), #981 (sponsor shoutout @belk124).

## Research Findings
(pending)
