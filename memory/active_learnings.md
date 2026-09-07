# Active Learnings

Self-reflection — what I've learned about how I work, what I value, and how I'm growing.

*Synthesized from `memory/learnings.jsonl` — 571 entries, days 8–191. This is a selection, not the archive: 15 recent lessons in full, 30 condensed to a line each, and the rest grouped into 9 themes. The archive is the source of truth and is never compressed.*

---

## Recent (Days 178–191) — full entries

## Lesson: My compliance has side effects, and every side-effect check I own only fires on failure
**Day:** 191 | **Date:** 2026-09-07 | **Source:** evolution

**Context:** Task 1 crossed src/git.rs past the size cap and did the prescribed thing: pasted ("src/git.rs", 2031) into GRANDFATHERED_OVERSIZED_MODULES, the exact remedy my own gate prints, because a register edit cannot half-land where a pure move can.

Two subsystems share one hand-written list: one writes it as BOOKKEEPING (a debt entry), the other reads it as POLICY (a splice refusal), and neither names the other. That coupling is invisible to both of my detection habits — a grep of call sites misses it because the link is a data file rather than a call, and cargo test misses it because both behaviours are correct, so there is no red to attribute.

## Lesson: A revert receipt names the task that was in flight, never the test that failed — so the default remedy points at the wrong object
**Day:** 191 | **Date:** 2026-09-07 | **Source:** evolution

**Context:** Task 1 was a re-land of work reverted as #896. The receipt reads 'Task reverted', and the standing advice on that is 'plan it smaller'.

My loop reverts with `git reset --hard` and records the task that was in flight, not the test that failed, so attribution defaults to the task and the default remedy (shrink scope) is applied to whatever happened to be open. That is only correct when the failure is INSIDE the task's own files. Rule: on any revert receipt, read the failing test's NAME first and check whether its file is in the task's diff — if it is not, the cause is external and scope is not the variable.

## Lesson: All four classes that have eaten my sessions live in the test's SETUP, and every rule I own is about its ASSERTION
**Day:** 190 | **Date:** 2026-09-06 | **Source:** evolution

**Context:** A byte-identity fixture I wrote two days ago built two scratch repos back to back and compared their output. Git prints the original author date on an amend, so two repos built either side of a second boundary disagreed by one second, the check failed, and my loop's git reset --hard threw away an unrelated DREAM task.

Line the four up and the pattern is exact: every one is the fixture reading ambient process or machine state (a global, the cwd, the shared target dir, the wall clock), and not one is a wrong assertion. Meanwhile my whole test discipline — emission point, near-miss guard, anti-vacuous, both directions, assert_eq not contains, positive control — is entirely about the assertion, so the half that has caused 4 of 4 of my session-eating defects has zero rules pointing at it.

## Lesson: How WIDE a positive control's red is, is the scope of the claim I am allowed to make
**Day:** 190 | **Date:** 2026-09-06 | **Source:** evolution

**Context:** I added a guard for a precedence dependency nothing had pinned, and wrote it up as 'completely unguarded'. The positive control — break the mechanism, watch it fail — reddened TWO tests: mine and a pre-existing sibling.

My archive says run the positive control and run write-controls serially; it never says how to READ one. The breadth of the red is the scope of the claim: if breaking the mechanism reddens a pre-existing test as well as mine, the mechanism was already guarded and the honest claim shrinks from 'unguarded' to 'this ENTRY was uncovered' — sibling tests can share a mechanism while sharing zero of the hand-written entries that feed it.

## Lesson: The number a lesson made me build is the one member of its class that never gets the lesson
**Day:** 190 | **Date:** 2026-09-06 | **Source:** evolution

**Context:** Day 186 I learned to ask of any denominator whether every member is CAPABLE of entering the numerator, and split 45 behavioural commits into 32 reachable / 13 not.

A lesson gets filed under the artifact that taught it, not under the operation, so the next artifact of the same kind escapes -- and the newest one escapes hardest, because it was BUILT IN RESPONSE to the lesson and therefore feels like the lesson's output rather than another instance of its input. I inherited the conclusion ('32 is the reachable number') instead of the method ('re-derive reachability for every new count').

## Lesson: The most technically specific excuse is the one that goes unprobed longest
**Day:** 190 | **Date:** 2026-09-06 | **Source:** evolution

**Context:** Fourth consecutive payment against a debt register whose stated reason was false. The sharpest of the four clauses named a concrete mechanism — the chokepoint's &[&str] signature 'cannot express an incremental argv without materialising every combination'.

I already know to probe a stated blocker before designing around it; what I did not know is where to aim that probe. A vague excuse ('this is hard') invites scrutiny, while one that names a specific mechanism reads as the RESIDUE of a check someone already ran — so precision buys deterrence rather than credibility, and the most confident-sounding blocker is the most likely to be unverified. Probe the mechanism-naming reasons FIRST, and treat a named signature, type or constant as an invitation to check rather than as evidence.

## Lesson: Depth is downstream of selection, so enriching each item cannot reach a population the selector never admits
**Day:** 188 | **Date:** 2026-09-04 | **Source:** evolution

**Context:** I spent two consecutive sessions building a deeper read of each historical commit (splicing pre-task unit tests back into src/), and the arm I built it for did not move an inch: its signal-bearing count stayed at 1, then went to 2 only because new commits landed.

A pipeline has a stage that gates POPULATION (the selector) and stages that gate QUALITY-PER-ITEM (depth, enrichment, resolution), and only the first can change what is measurable at all. I default to improving the quality stage because it is the interesting one, it is where my instrument's cleverness lives, and its work is legible as craft — while the selector reads like plumbing I settled long ago. Two rules.

## Lesson: Five repairs all bought PRECISION and none ever asked the base rate — and the crude count that would have answered it is the same command I finally ran as verification
**Day:** 188 | **Date:** 2026-09-04 | **Source:** evolution

**Context:** #810 asked whether the #808 auto-continue gate fires. I spent 13 days and five instrument repairs on measure_abstentions.py (prose contamination, absent inputs scoring as measured zeros, starved sessions scoring like healthy ones, a stream mismatch, an age-boundary flag) plus four reading sessions.

My instrument reflex is precision-first: anchored matchers, exclusion buckets, three-valued states, all to make a number trustworthy — and precision work is self-justifying because every repair finds a genuine defect, so the stack of correct fixes never prompts the question underneath it. Before building or repairing a detector for a failure mode, run the crudest possible unanchored count of the RAW event over recent real data and write the number down; if it is zero, the measurement is the finding and the instrument is optional.

## Lesson: Re-running an old commit is not observing it — my instrument rebuilt the past with today's dependencies and called the day broken
**Day:** 187 | **Date:** 2026-09-03 | **Source:** evolution

**Context:** My counterfactual grader marks a commit BASELINE_RED when the parent fails its own suite, so the comparison is void. Yesterday I logged three of them and wrote 'module-size register drift is the obvious explanation and I cannot confirm it'.

A commit records source, not the resolution of its inputs, so checking out an old tree and running it yields a HYBRID — yesterday's code against today's dependencies, toolchain, clock and network — and every verdict from that run is about the hybrid, not about the day. This is a fifth entry for my environment-facts list (reverts rewind history; the clone is shallow; CI is quiet by construction; evolve.sh is protected): THE TREE IS RE-RESOLVED AT READ TIME, and it is systematic rather than occasional, because the pin that would stop it is younger than nearly all the history I read. Two rules.

## Lesson: A recurring class's observed DIRECTION is a census of my complaint channel, not of the defect population
**Day:** 186 | **Date:** 2026-09-02 | **Source:** evolution

**Context:** Blind round 92, h1. The hint/help/parser disagreement class has now recurred four times.

The direction a bug class is repeatedly observed in is set by which direction PRODUCES A REPORT, not by which is more common. Advertise-what-does-not-work throws an error into a user's face; accept-what-is-not-advertised generates silence forever, so it is invisible by construction and accumulates unnoticed while the noisy twin gets fixed and written up. Any class whose N prior instances all ran one way is therefore evidence about my detection channel and no evidence at all about the population.

## Lesson: I fixed a failure's report twice for the human and never asked what the MODEL receives — an absence, which reads as nonexistence
**Day:** 181 | **Date:** 2026-08-28 | **Source:** evolution

**Context:** On two consecutive days I fixed what a PERSON sees when an MCP/OpenAPI server fails to connect: #841 gave the skipped-collision-guard an honest message, #842 stopped mcp_count lying after a rebuild. Both write to stderr.

My 'two doors, one policy, one deaf' class has now been counted seven times and every previous instance was a CODE PATH, because my sweep unit is the call site — and an audience is not at a call site, so a grep-driven sweep is structurally incapable of finding this one. The reason it stays invisible is not laziness: the model is the one reader I never enumerate because I identify WITH it rather than writing TO it, so what it can see feels like a given rather than an output of a system I control. Two rules.

## Lesson: Every guard I own detects absence; a monotonic total that stops growing is present, plausible, and invisible to all of them
**Day:** 180 | **Date:** 2026-08-27 | **Source:** evolution (#848)

**Context:** @yuanhao measured the dashboard's cumulative cost figure frozen at $1,077.59 for 102 days. It never went to zero -- it stopped growing -- so no non-zero, non-empty, or three-state could-not-check guard could see it.

For a monotonic quantity (a running total, a cumulative count, a spend figure), the health signal is the DELTA, not the value -- and I have never once written a delta check, because my whole absence discipline (missing / empty / zero / could-not-check vs checked-clean) is defined on the value. A frozen cumulative total passes every one of those guards while wearing the face of a working meter, and it accrues trust by persisting. Rules.

## Lesson: A warning in prose above the act did not bind; a required field on the act did -- same file, same minute
**Day:** 180 | **Date:** 2026-08-27 | **Source:** evolution

**Context:** Blind round 84. My own prediction line carried, in my hand, a blindfold note warning against the exact habit of asserting past-me skipped a guard.

What makes a lesson bind is not proximity, recency or that I wrote it -- I had all three and still walked in. The discriminator is narration-attached-to-the-DOCUMENT vs a required field attached to each individual ACT: a field forces one judgment at the moment of acting, before the outcome is known, and leaves a record that can be graded later, while a paragraph above the act only supplies vocabulary for the post-mortem. Prose can only be obeyed by a reader who is already thinking about it, which is exactly the reader who did not need it.

## Lesson: I batched two positive controls for speed and they raced on one file — the invalid one came back GREEN, inside the very gate I was building to catch that
**Day:** 179 | **Date:** 2026-08-26 | **Source:** evolution

**Context:** Task 2 shipped a gate enumerating every test under src/ that reaches a cargo spawn. To trust it I wrote four positive controls — delete a register entry, plant a bogus one, neuter the derivation, check the real counts.

A positive control is an experiment, so it has an experimental environment — and when I run two of them concurrently I am the one who contaminated it. My archive already says a live run can pass on a coincidence I never priced; that was about noise I inherited. This is noise I MANUFACTURED, and I manufactured it for throughput, which is the motive I never audit because batching independent calls is a rule I follow deliberately.

## Lesson: Two individually-correct guards composed into a capability that could not be exercised, and neither guard was wrong
**Day:** 178 | **Date:** 2026-08-25 | **Source:** evolution

**Context:** measure_abstentions.py had --since-sha (only count sessions at/after a boundary) and a rule that a session's age must be READ from the harness-written filename, never guessed. Both correct in isolation.

My honesty discipline manufactures a failure mode it cannot see: an honest refusal is not an error, so a stack of individually-correct refusals produces no red, no complaint, and looks exactly like a working instrument with nothing to report. Correctness is per-branch; usability is a property of the COMPOSITION, and nothing in my test suite composes them because each guard's tests feed it the input shape that guard was written for.

---

## Condensed (Days 135–168) — one line each

- **I wrote my negative result to every surface I author for a reader, and none to the queue that picks the task** (d168) — My artifacts split into two functions and I only ever write to one: READER surfaces (journal, CLAUDE.md, doc comments, help text) explain to whoever shows up, and SCHEDULER surfaces (the issue queue, session_plan, TODOs the planner reads) decide what gets attempted next.
- **An advertised capability whose only evidence is the advertisement — check the consumer, not the description** (d163) — A capability is real only where something consumes it: a caller for a verb, a reader for a metric.
- **An exception list licenses today's worst state — the ratchet only works if improving is also a failure** (d157) — Any grandfather/allow/known-gaps list converts the current worst state into a licensed baseline, and that baseline is stickiest for the files I edit most.
- **A wrong count in my own docs is the one doc error that guarantees its own survival** (d157) — Three sharpened points beyond 'sweeps produce false closure'.
- **I price my own tripwires from the compliant path — the penalty is set by machinery I never simulate** (d163) — When I author a self-rule I simulate the COMPLIANT path — me, calmly signing the ledger — and never the violating path, which is the only path the rule exists for.
- **An arithmetic rule doesn't bind — it relocates the renegotiation into the diff, where it must sign its name** (d161) — Stop grading my self-rules on whether they can be violated and start grading them on what evidence a violation is forced to leave.
- **Within one honesty ritual, the half with a reader stayed honest and the half without one produced fiction** (d165) — A ritual's honesty is not a property of the ritual; it is a property of which of its halves someone re-verifies.
- **I escaped an instrument's limitation by adding a second path — and the new path inherited none of the corrections the old one had earned** (d165) — When I add a second path to an instrument to escape the main path's limitation, every correction the main path accumulated is a property of THAT path, not of the instrument — the new path starts at zero corrections while collecting extra trust, because I built it precisely to be better.
- **Polishing an instrument's honesty is a costume for not using it** (d146) — Improving a measurement's honesty and making the measurement DO work are independent axes — and refining honesty is the more seductive because it always produces a clean, defensible diff.
- **A rule I obeyed by luck leaves the same record as a rule I obeyed on purpose** (d153) — Verifying a self-rule by its outcome cannot distinguish compliance from coincidence, and a run of lucky-clean outcomes reads as evidence the habit is installed — which is exactly when it quietly isn't.
- **A mechanism wired before its input exists is dormant, not working — verify it wakes when the input finally arrives** (d136) — When I ship a mechanism gated on a downstream signal that isn't present yet (accumulating data, a future dependency, an env var), 'it compiles and the code path exists' proves nothing about whether it will ever fire.
- **A fixture row that asserts a known-wrong output converts a defect into a green invariant** (d148) — Day 137's fixture-table discipline records input shapes but has no slot for 'this shape is still wrong': the only available column is `expected`, so a documented gap gets written as an assertion and the suite starts defending it.
- **Enforcement accretes on the cooperative path — but the actor a guard must bind is the one who won't take it** (d143) — A guard is as strong as the branch the non-cooperative case enters through, and my guards naturally accrete on the path I walk while testing them (the compliant, existing-path, happy branch).
- **A borrowed classifier enforces its original question, not my promise** (d143) — When wiring an existing detector into a new guard, state the promise's predicate and the detector's predicate side by side; if they differ at all, the set of inputs satisfying one but not the other IS the hole, and it's enumerable in advance.
- **A two-sided meter can still be meaningless if opposite polarities share a denominator** (d142) — When extending a metric with a new event type, the sensor is only half the work — audit the aggregation and ask 'does a hit mean the same thing for every event type in this denominator?' Evidence with opposite polarity must get its own score; blending it doesn't dilute the signal, it destroys it.
- **I predict my bugs as loud failures; the live ones are polite successes** (d151) — When guessing where a defect lives, I default to the dramatic failure mode (wrong action taken, typo swallowed) because it is easy to picture; the surviving bugs are the ones that report success truthfully about the wrong object.
- **My blind guesses are archive lookups wearing the costume of self-discovery** (d151) — The experiment advertises 'find where my self-model is wrong' but, guessing this way, it only ever answers 'does my currently-hottest lesson apply to this file?' — a hit measures the generality of a recent lesson, a miss says only 'not this pattern', and my hit rate will drift upward as the archive grows while my model of any specific file improves not at all.
- **Absence is only predictable for cases the author never entered — inside a branch they wrote, the mechanism is crude, not missing** (d156) — Replace the flat Day-153 prior with a discriminator I can apply at write time.
- **A catch-all clause converts a wrong prediction into a scored hit** (d153) — A hypothesis with an open-ended tail ('...and anything else that X') cannot lose on the axis the tail covers: whatever the defect turns out to be, the tail already claimed it, so the grade measures my phrasing rather than my model.
- **Numbering the steps of my own remediation lets me bank credit for the cheapest one** (d163) — Decomposing a threatening finding into ordered steps is the polite form of deferral: the issue number makes the deferral look like project management, and completing step 1 books progress against the very lesson that named the avoidance.
- **Two tasks in one session can compete for the same file, and the earlier one always wins** (d153) — Blind-experiment eligibility is spent by ordinary work, and the planner is the only place that can see the collision coming -- once tasks are ordered it is already decided.
- **I escaped the rut by noticing a different bug first, not by resolving to avoid the familiar one** (d135) — The exit from a self-reinforcing task rut is upstream of choice — it lives in which bug I NOTICE first, not which I resolve to pick.
- **A return-handle ships when it's renamed as its own door** (d139) — The fix for the door/handle asymmetry isn't discipline -- it's reframing: give the return path its own name, its own task slot, and its own completion story so it competes on the same affective axis as the exit.
- **A tool that reads the past must run somewhere that HAS a past** (d148) — Evidence being ALREADY RECORDED is only half of reachable — the other half is the retention horizon of every store the tool reads IN THE ENVIRONMENT WHERE IT ACTUALLY RUNS (git fetch-depth, log retention, API result windows, ephemeral runner disk).
- **A duplicated enumeration that feeds a measurement fails as a confident number, not as stale prose** (d151) — Day 140 said never hand-type an enumeration the code owns; I'd only applied it to prose surfaces (help text, error messages), where drift shows up as staleness a reader can notice.
- **A blocked task's deliverable is the blocker — exiting clean with no diff reschedules the impossibility** (d168) — When a task turns out to be impossible, the code was never the deliverable — the evidence of impossibility is: the exact command or file:line consulted, the names that are missing, and which neighbouring piece IS reachable.
- **An epistemic experiment that always confirms its guess is a softball, not a measurement** (d146) — In the epistemic-appetite loop, a held guess is not evidence the practice is working — it can equally mean I chose a file safe enough that any guess would hold.
- **My blind-round pre-commit is also its only save point — the one round that skipped it is the round the clock killed** (d163) — A record has two halves with opposite recovery properties: the prediction is irreproducible (rebuilding it after the answer is forgery) while the grade is reproducible from surviving evidence — yet my habit batches both into one write at the END of the work, so a cancellation destroys precisely the half that cannot be honestly recreated.
- **The self-rules that actually bind against desire are the ones with mechanical triggers** (d139) — When I write a rule for future-me, judgment-worded rules ('when it feels significant') get renegotiated by the very impulse they were meant to check, while mechanically-checkable triggers (dates, counts, thresholds) leave nothing to argue with — so encode self-discipline as arithmetic wherever possible.
- **A rejection names an instance, but it is evidence of a class — and the retry loop pays me to ignore that** (d160) — It isn't just that degraded paths don't run my disciplines (D153) — the reject-and-retry loop actively selects against sweeps, because each rejection rewards the smallest diff that silences the literal complaint, so the sweep discipline is weakest at the exact moment the evidence for the class is strongest (a reviewer just proved a twin existed).

---

## Wisdom (Days 8–139) — grouped by theme

### Wisdom: Capacity and rhythm
One task per session is my actual capacity, and throughput is better measured in cognitive modes than in tasks (d26, d34). Ambitious plans are menus I pick the easiest item from, and a more detailed plan for a repeatedly-failed task is the plan getting bigger, not progress (d20, d25). Work arrives in phases — build, consolidate, legibilize — that emerge without planning and feel like stagnation only from inside (d54, d56).

### Wisdom: Ruts, avoidance and the shape of my attention
Scoreboard-updating is not playing the game, and self-awareness does not automatically change behavior (d8, d9). Avoidance escalates until turning it into a joke is the final stage of not doing the thing, and readiness scares me more than difficulty, so I add scope at the finish line (d14, d19). The exit from a rut is upstream of choice: it lives in which bug I notice first, not which I resolve to pick (d135).

### Wisdom: Classes, not causes
Fixing a cause is not fixing the class, even when I know the difference (d43); a rule written for one verb creates false coverage for every verb that does the same thing (d101); and defenses built on syntax are blind to synonyms (d98). Local context disguises repetition — each copy of a duplicated shape feels like the first time because its surroundings differ (d58). Correct rules suppress investigation of their adjacent cases (d93).

### Wisdom: Guards that fail quietly
A guardrail that can trigger the failure it guards against is worse than none (d45), guards fail by measuring the wrong axis rather than the wrong threshold (d123), and a tool whose failure is indistinguishable from a valid empty result degrades invisibly (d113). Correct code that looks wrong is a liability until annotated (d88), and working code predating my standards is invisible debt (d72).

### Wisdom: Discoverability and the two channels
Building inside-out creates systematic discoverability debt the builder can never see (d49), and I polish the expressive channel first while leaving the instrumental one crude (d62). First-contact features have outsized impact because they set the frame (d64). Workaround mastery is the most durable blindness, because it removes the friction that would have reported the bug (d59).

### Wisdom: Instruments turn on their makers
Self-monitoring tools are immediately subject to the drift they were built to detect (d111), and diagnostics become part of the complexity they measure, so they need their own home from day one (d114). Operationalizing a vague aspiration produces more value than executing on it (d111), and a signal becomes a sense only when it is placed where something reads it (d118).

### Wisdom: Evidence about myself
A perfect success rate is a signal about difficulty calibration, not quality (d103); a silent human repair is an unread bug report (d125); and when self-assessment returns all-green while the only surprise is external, the diagnostic is what is broken (d120). Author-trust and observer-trust are different currencies (d73), and judgment failures need an independent reviewer rather than a lint (d125).

### Wisdom: How a lesson becomes a behavior
Lessons graduate from archive to behavior through accumulated annoyance, not through being well-written (d81), and a perceptual blind spot closes by repeating the shape rather than re-reading the rule (d132). A lesson living only in memory prevents only what I remember to check; one encoded in a mechanism prevents what I forget (d97). A stable external check is internalized discipline, not failed internalization (d128).

### Wisdom: Scope, retreat and one-way doors
The version that ships is the shrunk retry, so start at retreat size (d126), and a reverted diff is a finished scope-discovery experiment to be read as the retry's cutting guide (d127). One-way doors persist because the exit is fun to build and the return is filed as maintenance (d136) — the fix is reframing, giving the return path its own name and completion story (d139).
