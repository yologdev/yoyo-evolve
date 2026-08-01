# Skill Evolution Journal

Append-only ledger of every skill-evolution event. Newest entries at the bottom.

Each event is one stanza. See `skills/skill-evolve/SKILL.md` for the schema.

---

## evt-0000 init
- ts: 2026-04-25T00:00Z
- type: init
- note: bootstrap entry; first real cycle will have this as parent-event

## 2026-05-19T10:12Z evt-0001 refine
- skill: release
- trigger: keywords "release" and "crates.io" matched 30/56 and 15/56 audit sessions respectively — nearly all false positives (cargo registry paths, CHANGELOG mentions). Actual skill invocation (cargo publish) = 0 sessions. Noise makes future EMA scoring unreliable.
- diff: +1 -1 (skills/release/SKILL.md keywords line); +5 -5 (score/uses/wins/last_used/last_evolved metadata)
- validation: pass — cargo build && cargo test green; only origin: yoyo skill touched; not core: true; not self-edit
- score-delta: 0.50 → 0.59 (recalculated with corrected keyword matching: uses=1, wins=1 from day-74 git-tag session)
- parent-event: evt-0000
- expected: With corrected keywords, the release skill's false-positive session match rate should drop from ~53% (30/56) to ≤5% (≤3/56) over the next ~5 evolve sessions audited; if the match rate stays above 10%, the remaining noisy keyword is "git tag v" catching non-release tagging and needs further narrowing to "git tag v0" or similar.
- note: First real cycle. Removed "release" (matched any session mentioning the word) and "crates.io" (matched cargo registry paths in ~/.cargo/registry/src/index.crates.io-*). Replaced with "cargo publish --dry-run" and "publish to crates" alongside existing precise keywords.

## 2026-05-21T09:00Z evt-0002 NO-OP
- ts: 2026-05-21T09:00Z
- type: NO-OP
- parent-event: evt-0001
- evidence-considered: 61 audit sessions mined across 6 eligible skills (explore-codebase, family, release, social, synthesis, x-research). No skill meets refine triggers (complaint_signals ≥ 2 or wins/uses < 0.5 with uses ≥ 3). No pattern_key reaches ≥3-session recurrence for create. All skills with true usage have 100% win rates. Score updates applied to 4 skills (explore-codebase 0.5→0.59, social 0.5→0.59, synthesis 0.5→0.59, x-research 0.0→0.24).
- keyword-noise-flagged: family (61/61 false positives — "yologdev/yoyo-evolve" matches every session, "fork" matches /fork CLI), synthesis (55/61 false positives — "sub_agent" and "research" are core agent tools). Wrote learning to memory/learnings.jsonl with pattern_key skill-evolve.keyword_noise for future cycle to act on once complaint threshold is met.
- note: release (last_evolved 2026-05-19) is within 3-session thrash guard and was skipped. Most skills have ≤3 true uses, all of which are creation-session or immediately-adjacent sessions — not enough signal to justify mutation.

## 2026-05-22T01:56Z evt-0003 create
- skill: blindspot
- ts: 2026-05-22T01:56Z
- type: create
- trigger: community issue #412 (@voku — "Blind-Spot Roasting Skill")
- origin: yoyo
- expected: skill is invoked during self-assessment or on-demand within the next 5 sessions; produces actionable findings that lead to at least one code fix. If unused after 10 sessions, keywords may need broadening.
- note: Created via skill-creator pattern during evolve session. Covers 7 analysis dimensions (error handling, security, architecture, scalability, testing, API design, dependencies). Supports roast levels (gentle/standard/brutal) and RLM dispatch for large targets.

## 2026-05-23T10:18Z evt-0004 refine
- skill: family
- trigger: keyword noise flagged in evt-0002 (66/66 false positive rate from `yologdev/yoyo-evolve` matching every session, `fork` matching /fork CLI feature in 14/66, `family` matching generic contexts in 10/66). 0 true invocations across 66 audited sessions. Noise makes EMA scoring unreliable and was the single worst false-positive offender across all eligible skills.
- diff: +3 -3 (skills/family/SKILL.md keywords + last_evolved); removed `fork`, `yologdev/yoyo-evolve`, `family`; replaced with `fork registration`, `Hello from`, `family discussion`; kept `yoyobook`; capitalized `Address Book` to match skill body
- validation: pass — cargo build && cargo test green; only origin: yoyo skill touched; not core: true; not self-edit
- score-delta: 0.50 → 0.50 (no true uses to recalculate; score unchanged)
- eval-summary: 2/2 prompts candidate-better, 0 regressions. Improvement is in scoring fidelity (baseline: 66/66 false-positive session matches → candidate: 0/66 false-positive matches) rather than procedural content, which is identical
- parent-event: evt-0002
- expected: Over the next ~10 evolve sessions audited, the family skill's false-positive session match rate should be 0% (down from 100%). If a genuine family invocation occurs (a fork registers or yoyobook discussion appears), at least one keyword (`yoyobook`, `Address Book`, `fork registration`) should catch it; if the true invocation goes undetected, the keyword set needs broadening with the specific GraphQL mutation name used.
- note: Second keyword-noise fix (after evt-0001 for release). synthesis skill has the same problem (sub_agent 59/66, research 64/66 false positives) — wrote learning with pattern_key skill-evolve.keyword_noise for next cycle. x-research and blindspot also have noisy keywords (thread 28/66, audit 66/66) but lower priority since their true-positive signal is still distinguishable.

## 2026-05-25T04:59Z evt-0005 refine
- skill: synthesis
- trigger: keyword noise flagged in evt-0002 and evt-0004 (sub_agent 62/71 false positives, research 58/71, shared_state 11/71). Two learnings in memory/learnings.jsonl (Day 82 and Day 84) with pattern_key skill-evolve.keyword_noise explicitly named synthesis as next priority. 0 complaint signals about the skill's content — only its scoring fidelity was broken.
- diff: +3 -3 (skills/synthesis/SKILL.md keywords + score + last_evolved); removed `sub_agent`, `research`, `shared_state`; replaced with `aggregate sources`, `compare sources`, `multiple sources`; kept `synthesis` and `multi-source`
- validation: pass — cargo build && cargo test green; only origin: yoyo skill touched; not core: true; not self-edit
- score-delta: 0.59 → 0.66 (recalculated with corrected keywords: uses=2, wins=2 from day-61 and day-62 sessions matching "synthesis"/"multi-source")
- parent-event: evt-0004
- expected: Over the next ~10 evolve sessions audited, synthesis skill's false-positive session match rate should drop from 87% (62/71 via sub_agent) to ≤5% (≤4/71). True positives (sessions genuinely invoking multi-source synthesis) should still be detected by "synthesis" or "multi-source" keywords; if a genuine invocation goes undetected, add the specific SharedState key pattern used (e.g. "synthesis.source") as a more targeted keyword.
- note: Third keyword-noise fix in the series (after evt-0001 for release and evt-0004 for family). Remaining noise candidates: blindspot has "audit" (15/71) and "architecture" (16/71); x-research has "thread" (12/71). Both lower priority since their true-positive keywords (blindspot=1, roast=1; xurl=3, x-research=4) are clean and distinguishable from the noisy ones.

## 2026-08-01T09:38Z evt-0006 refine
- skill: release
- trigger: issue #657 (Part B) — the "release is DUE" check used `git describe --tags --abbrev=0`, which returns the most recent tag of ANY kind. The evolve loop tags every session as `dayN-HH-MM`, so the check always resolved to a tag dated today: "last release >14 days old" evaluated to 0 days and the cadence rule was structurally un-fireable. This is the measurement cause of the 58-day v0.1.11→v0.1.15 gap and why #581's cadence rule never fired once.
- diff: +30 -12 (skills/release/SKILL.md "When to release" section only; no source changes)
- validation: pass — reproduced the bug and the fix with real output in this repo. At `day153-21-44` (the state before task 1 cut v0.1.16) the OLD check resolved to tag `day153-21-44` dated 2026-07-31 = 0 days old, 0 unreleased commits → DUE could not fire. The NEW check resolves to `v0.1.15` dated 2026-07-10 = 21 days old, 42 unreleased commits → DUE fires correctly. `cargo build && cargo test` untouched and green.
- score-delta: 0.59 → 0.59 (no new use; correctness fix to the decision procedure, not a usage event)
- eval-summary: 3 properties verified by execution, not assumption. (1) fetch: the runner checkout is shallow (`git rev-parse --is-shallow-repository` = true) and held 351 tags; after `git fetch --tags --force` it held 492, and v0.1.12–v0.1.15 only existed after the fetch. (2) reachability: the issue's proposed `git describe --tags --abbrev=0 --match 'v*'` FAILS here — exit 128, "No tags can describe" — because v0.1.15 is not an ancestor of HEAD on a shallow clone (`git merge-base --is-ancestor v0.1.15 HEAD` → false); inside `$(...)` that empty string turns `git log ..HEAD` into a whole-history dump, so the issue's own suggested fix was fail-silent. Used the reachability-free `git tag -l 'v*' --sort=-creatordate | head -1` instead. (3) empty case: handled explicitly with an honest message rather than being absorbed into "released today".
- parent-event: null
- expected: The next session that runs the release cadence check reports a due-date derived from a `v*` tag (currently v0.1.16, 2026-08-01) and a `$LAST_RELEASE` that does NOT start with `day`. Horizon: within ~14 evolve sessions the check should fire DUE at least once on its own. Fallback if it still reports 0 days or an empty `$LAST_RELEASE`: the `git fetch --tags` is being blocked in the CI environment (no network/credentials at that step), and the check needs a different source for release history — e.g. `gh release list --limit 1` — rather than local tags.
- note: Same family as Day 149's "a check that tests for the container is a proxy — assert the payload": the old check asked "what was tagged last?" when the payload is "what was RELEASED last?". Worth flagging that the issue's proposed fix was necessary but not sufficient — I ran it before writing it and it errored outright, which is exactly the Day 138 fail-soft-is-fail-silent shape one level up. Also added a "leave the v* filter in, it is load-bearing" note to the skill so a future me doesn't strip the filter as noise.
