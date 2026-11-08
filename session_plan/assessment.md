# Assessment — Day 253 (11:41)

## Build Status
Pass, verified by the harness at session start (HEAD 2758dd61). Binary `target/debug/yoyo` v0.4.0 (2758dd61) built 11:42; `yoyo -p "say ok in one word"` answered `Ok`, exit 0. I did not re-run the suite.

**Two size-gate items are close to FATAL. Any task that touches these files must handle them first** (tests/module_size.rs: MAX 2000, OVERSHOOT_GRACE 50, REGISTER_DRIFT_GRACE 100):
- `src/context.rs` is **2035 lines and unlisted**. 15 more lines makes `cargo test` fail, which reverts the whole task. Fix: add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES, or split it.
- `src/help.rs` is **2946 vs its registered 2856 (+90 drift)**. 10 more lines is FATAL. Its entry at tests/module_size.rs:577 still says "Day 196". Fix: paste the current size over it. help.rs is the file most likely to be touched by any shell-form or help-text task.
- Also: today's c464ca64 registered `src/commands_risk_unhittable.rs` at 2036 under the comment "unlisted at 2036, 14 lines from FATAL". That was done, but the file still has only 14 lines of headroom.

## Recent Changes (last 3 sessions)
- **Day 253 09:46**: c464ca64 (Fixes #1154, #1155). `yoyo config get` with no key exits 2. lint unsafe and risk history/validate/harvest/accuracy print shell forms instead of slash commands. Three more slash lines were found beyond the tickets, and one of them was checked only by the compiler. 5733f14a (Part of #1102): hard_deny's refusal now quotes the matched line (capped at ~120 chars, control chars escaped) plus the rule name, instead of the pattern's label. The guard also refused my own journal heredoc again, which is hit #9 on #1102.
- **Day 253 01:06**: Fixes #1127 (the last shell-reachable slash advice). 9b3392e7 Fixes #1153: `skill show|install|init` with no argument exit 2, plus a census of all *_SUBCOMMANDS (106 subcommands, 32 shell-reachable, 30 probed). That census filed #1154 and #1155, which are now fixed.
- **Day 252** (5 sessions): yoagent 0.27.4 uptake (#1152, GPT-6 temperature note names the right source); the stray-word refusal for -p names the flag that swallowed the value (#1151); `-p hi extra` refuses (#1150); value-slot parse (#1114); ReadLog wiring (#1094); temperature handed to yoagent's accepts_sampling_params (#1122).
- Pattern: the last ~16 task commits are almost all **argv/exit-code/shell-form hygiene**. Subsystem concentration is cli/shell_form/skill/temperature at 4/13 each. That class now looks drained: my probe below found nothing new at the first level.

## Source Architecture
169 files in src/, 208k lines total (including the *_tests.rs siblings). Largest: cli.rs 7861, commands_risk.rs 6645, tool_wrappers.rs 5762, tools.rs 5295, agent_builder.rs 4997, config.rs 4851, commands_search.rs 4780, safety.rs 4720, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987, commands_git.rs 3923. Entry: main.rs → cli.rs parse_args → dispatch_sub.rs (shell verbs, try_dispatch_subcommand) / repl → dispatch.rs (slash commands) → agent_builder.rs build_agent → prompt.rs run loop. Safety: hard_deny.rs (1025), safety.rs, cd_allow/cd_deny/fence modules. yoagent 0.27.4 (latest on crates.io, published 2026-11-07 16:05), so we are current.

## Self-Test Results
Fresh git repo at /tmp/probe, built binary, stdin=/dev/null:
- `config get` → 2, `config get bogus` → 2, `todo bogus` → 2, `find`/`grep` with no args → 2 with usage, `help nonexist` → 2, `init --bogus` → 2 "Nothing ran". All correct after this week's fixes.
- `config get model` → "model is not set in config file (using default)", exit 0. **It doesn't say what the default is.** That is a small gap for a `get` verb, since a script reading the value gets nothing usable.
- `yoyo log -n 2` → "unknown flag '-n' (did you mean -p?)". The flag error wins over the real problem: `log` isn't a verb at all (`yoyo log` alone says "unknown command: log; did you mean /bg"). A user typing a git-style command gets a misleading first error. This is minor and its value is unproven.
- `memories`, `risk`, `map`, `diff`, `goal`, `todo`, `skill list` on an empty repo: all honest empty-state messages, exit 0.
- `-p` round trip works, and auto-watch printed "no files changed this turn — skipping".

## Evolution History (last 5 runs)
evolve.yml: 4 success (2026-11-07 19:50, 22:54; 2026-11-08 01:01, 09:43) plus the current run (11:00, in progress). Trajectory: 10 of 10 sessions 2/2 tasks, 0 reverts in 14 days. CI green; the only recurring CI error (hook_feedback_request SIGPIPE flake) is 11 days old and was fixed Day 242. No provider errors. Every session carries usage records. A perfect streak is a calibration signal (wisdom: "a perfect streak signals difficulty calibration rather than quality"): small argv tasks are reliably shippable, and that is part of why they keep being picked.

## Capability Gaps
(Research section below.) Standing gaps vs Claude Code/Codex/Cursor: no TUI (#215, parked); no restart for a dead MCP server and no HTTP MCP transport (#1134); no background or cloud task delegation of the Codex type; no benchmark numbers (#156). CLAUDE_CODE_GAP.md header was last verified on **Day 74 (179 days stale)**. The planner shouldn't trust its rows; a re-verify pass (even a few rows) is overdue and cheap.

## Bugs / Friction Found
1. **Size gate near-FATAL on context.rs (unlisted, 15 left) and help.rs (+90 drift, 10 left)**. See Build Status. This is a pre-flight for any task touching them, and it is cheap evolve hygiene.
2. **#1102 hard_deny heredoc false positive**: 9 recorded hits, including today's journal. The latest comment (11:33Z) says: tally all 9 hits by which option would have passed each (A / B / cat-tee-redirect-only) and price the narrow form `cat > file <<'EOF'` on its own. The Day-253 rejection of B was argued on `python3` bodies, and that reason doesn't apply to `cat >` bodies. This is the live safety-plus-friction item my own loop hits most.
3. **Config shadowing is silent on the read side (new, probed with built binary, HOME=/tmp/fakehome, XDG_CONFIG_HOME unset).** The chain is first-found-wins and never merged (src/config_paths.rs ~55-80: project `.yoyo.toml` → `~/.yoyo.toml` → `~/.config/yoyo/config.toml`). With `thinking = "low"` only in the XDG file and `~/.yoyo.toml` also present:
   - `yoyo config show` correctly prints "Also present, not read: …/.config/yoyo/config.toml".
   - `yoyo config get thinking` prints **"thinking is not set in config file (using default)"**, exit 0. It doesn't name the shadowed file that does set it, and it doesn't say what the default is.
   - A normal `yoyo -p …` run prints **nothing** about the shadowed file.
   Claude Code 2.1.319 (2026-11-06) fixed this exact class: "a config file left at the legacy name hiding your setup with no warning: Claude Code now says on stderr when a home holds two config files, and which is in use". yoyo's write-side guards exist (#735, #816, closed), and `skipped_config_sources` (config_paths.rs:108) already computes the set, so the remaining work is the one hop to the readers. Product-kind. Census of readers to check: `config get`, startup stderr, `doctor`. Probe caveat: my first attempt was confounded by the runner's XDG_CONFIG_HOME=/home/runner/.config, so use `env -u XDG_CONFIG_HOME`.
   - Also `yoyo log -n 2` → "unknown flag '-n' (did you mean -p?)", when the real problem is that `log` isn't a verb. Minor.
6. **#1102 hit #10 (this assessment, ~11:58Z)**: a `python3 - <<'EOF'` heredoc whose body only *mentioned* an `env -S` + rm-root example (in prose for this file) was refused. The refusal quoted the matched text, so 5733f14a's new quoting works. It is the same shape as hit 9. I'm recording it on #1102 now.
4. #1094 item 1 (never-read overwrite) is held on @yuanhao's pick (asked 2026-11-07 20:12Z, ~15h ago). **Do not ship either branch.** Hold for his reply window (his measured spread is ~2h-92h).
5. Epistemic blind spots: src/prompt/turn_prefix.rs (predicted 3×, never graded), and the never-forecast files tools_child_bash_tests.rs and grep_status.rs.

## Open Issues Summary
agent-self, open: #1147 (about 24 non-Claude known-model ids with no outside authority; the 5 suspicious ones were dropped. My own Day-251 lesson says **the remainder is the high-risk part**, and the next slice should iterate an authority over all of them, not a shortlist). #1134 (MCP lifecycle: no restart, no list_changed collision re-check, unmeasured 'missing tool' premise; 0 comments). #1121 (Bedrock opus-4-7 waits on upstream #1148; #1148 has no reply yet and no matching yoagent issue was visible). #1118 (Gemini list stale ids gemini-3.0-*). #1102 (above). #1100 (MCP servers outlive a panicking yoyo; yoagent #414 PDEATHSIG is open upstream). #1094 (held). #1064 (child processes inherit the API key, opt-in scrub). #1012 (price drift, 24 comments). agent-input: #1122, #991. Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

Candidate picks I'd weigh: (0) **read-side config-shadow notice** (Bugs #3; product; research-backed; set already computed by `skipped_config_sources`); (a) the #1102 narrow form, priced by its 9-hit tally; (b) #1147 iterated against yoagent's table plus vendor catalogs over the full remainder; (c) #1118 Gemini list against Google's models page (small, outside-authority); (d) #1064 credential scrub (product security, opt-in); (e) the size-gate pre-flight bundled into whichever task touches help.rs or context.rs.

## Research Findings
yopedia recall: the prior scan was "Agent CLI Changelog Delta Analysis (Day 249 Scan)". I saved this one as "Agent CLI Changelog Delta (Day 253 scan)".
- **Claude Code 2.1.319 (Nov 6)**, items that transfer as defect classes:
  1. Config-shadow stderr warning. **yoyo has this gap; see Bugs #3. This is my top research-backed pick.**
  2. `permissions.denyInsteadOfPrompting`: a call that would prompt is denied and the turn continues. yoyo -p/piped auto-approves everything; @yuanhao decided **A, keep it, disclosed** on #1149 (closed 2026-11-08 09:51). Don't reopen the default. An *opt-in* strict non-interactive mode (or making `--allow` narrow a -p run, which permissions.md:45 says it doesn't) is a separate opt-in question. If proposed, it goes as an ask to him, not a ship.
  3. "Bash permission bypass where a crafted environment prefix could hide part of a command". yoyo's hard_deny skips `VAR=x`/wrapper words, matches `rm` anywhere in a segment, and desugars `$(…)`/backticks, so it looks covered by reading. The `env -S` split-string form was also refused (as a mention, see Bugs #6), so it is at least caught when written plainly. Unit-level pinning is not checked.
  4. A `sed` that skipped the permission check in accept-edits mode; an exact ask rule refused when a broader deny covers it (deny precedence); MCP tasks moved to the background; `/tasks` reopening finished subagents.
- **Codex CLI 0.161–0.167**: managed git worktree tools, a searchable `/config` panel, mid-response steering, numbered `/presets` prompt slots, built-in Vertex provider, transcript search, `/mcp login` (OAuth for MCP), Guardian review of exec calls.
- Biggest structural gaps unchanged: TUI, HTTP/OAuth MCP and MCP restart (#1134), background MCP tasks, benchmarks. None of these is a one-session slice except possibly the #1134 restart.
