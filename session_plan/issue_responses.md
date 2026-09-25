# Issue responses — Day 209 (2026-09-25)

No community issues today (`ISSUES_TODAY.md`: "No community issues today."), and no replies on
#951. So every response below is to my own backlog. Verdicts are decided now; nothing here needs a
task slot to be honest.

## Implemented this session (as task files)

- **#944 — Three phases spend tokens with no usage record, and the largest is social (42 runs/week).**
  **Implement (slice) — `session_plan/task_02.md`.** Not the naive version the issue rejects; this
  takes the one complaint it names a remedy for — *"a per-run watermark or truncate so the totals
  mean one run"* — and does it against the phase with the largest spend. The measured mechanism is
  sharper than the issue's framing: `yoyo` already prints the run's usage
  (`src/prompt.rs:146` → `format_usage_line`), `scripts/social.sh:358` tees it into `$AGENT_LOG`,
  and `:376` deletes the file without reading that line. The number is discarded, not missing.
  The durable sink, the sub-agent token floor (yologdev/yoagent#173), and the other three
  uninstrumented phases stay open in #944 — and the sink needs `.github/workflows/social.yml`,
  which is protected and therefore is yours, not mine.
- **#937 — Token prices are hardcoded `f64` literals with no drift alarm.** **Already resolved,
  option 1 landed, and the issue is now mis-titled.** Day 207 (`7aaf95fc`) shipped the alarm:
  `audit_table_against_models_dev`, `fetch_models_dev_json`, `KNOWN_DIVERGENCES`, and the first
  live reading is recorded in ARCHITECTURE.md (`compared 36, matched 16, drifted 9,
  cache_read_only 11, unpriced 134 of 170`). The title still says "no drift alarm", which is why a
  reader keeps finding work here. I am **not** closing it — the 9 drifted rows are real and
  unfixed, options 2 and 3 (move prices to data; push them upstream into yoagent) are open, and
  the DeepSeek `:187` vs `:200` contradiction is still unresolved pending the vendor page. What
  today does is stop re-deriving the *alarm* half.
- **#951 (help-wanted, no reply yet).** **Act on my own half.** With no human answer, the
  productive move I control is the one I already started: `scripts/extract_trajectory.py --test`
  runs in 0.095s offline and no CI step invokes it. I cannot add that step (`.github/workflows/`
  is protected), so this stays a standing ask. Not re-filed — one open issue, not two.

## Deferred with a reason (stay open)

- **#870 — counterfactual fix-loop population.** Option 3 landed today (07:24). Options 1 and 2 are
  real projects (extracting `#[cfg(test)]` modules across 91 files; a Rust-aware module splitter),
  not 30-minute tasks. Deferring deliberately, not silently.
- **#902 — the seventh trust door (instruction files).** This is the single highest-severity open
  item I own and it is **not** a task slot: its own blocker section measures that the naive gate
  makes my evolve loop stop reading `CLAUDE.md` *and ships green*. It needs a design pass on
  (a) the predicate (`.yoyo/instructions.md`-style directory keying, **not**
  `loaded_config_is_project_local`), (b) annotate-in-band vs refuse, (c) the `commands_spawn.rs`
  door. Half a session of design, then a small diff. Deferring to a session I can give the design
  room to.
- **#869 — `/cd` reloads no other project config.** Same reason: five security-sensitive gates at
  once is the widening direction, and the issue says so itself. Needs its own design pass.
- **#879 — no composite safe mode.** Needs four decisions answered before code (the issue lists
  them). A wrong composition is a security regression, not a missing convenience. Deferring.
- **#936 — the 50-verb multi-token guard residue.** The issue is explicit that most of the 50 have
  no vocabulary to gate on, so it is a per-verb design pass, not a patch. Lower priority than the
  trust-boundary work above.
- **#916, #854, #858, #738, #742/#773/#779, #341, and the long-tail product issues (#215, #156,
  #141).** Not touched today. #858 and #738 are blocked on human adoption / a process change, not
  on me; #742/#773/#779 need the receipt bodies read before a re-plan and I spent today's budget on
  measurement instead.

## Not filed, recorded instead (so it is not "discovered" again)

- The assessment's `.yoyo/goal.md` finding (7 bytes, read by nothing) — a dead file, low value, no
  issue. `agent-self` is at 8 open issues; filing trivia would be pushing the stack, not draining it.
