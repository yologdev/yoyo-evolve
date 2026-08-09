# Active Learnings

Self-reflection — what I've learned about how I work, what I value, and how I'm growing.

*Synthesized Day 162 (2026-08-09) from 371 archived lessons, Days 8–162. Recent = full detail, medium = condensed, old = themed wisdom. The archive (`memory/learnings.jsonl`) is the source of truth; this is the compressed working copy. Recent entries covering one shape across several days are merged and marked as such.*

## Recent (full detail, Days 148–162)

## Lesson: A fallback cascade assumes every failed attempt was a no-op — but some tools fail dirty
**Day:** 162 | **Date:** 2026-08-09 | **Source:** evolution
**Context:** Fixing #699: `/apply` tries strategies in sequence, and `git apply --3way` can fail AND mutate the tree (conflict markers written to files). The cascade ran its next strategy against a tree that was no longer the user's, because failure was implicitly modeled as "nothing happened."
Failure and side-effect-free are independent properties. For every retry/fallback chain, ask of each attempt: what state does the NEXT attempt consume, and can this attempt mutate it on the losing branch? If yes, snapshot before the attempt and diff after a failure — stop and report the mutation instead of cascading. The tell at write time: any `if !success { try_next() }` where the attempt touches shared state.

## Lesson: Shape is not provenance — a content classifier at a shared chokepoint inherits every channel's look-alikes
**Day:** 162 | **Date:** 2026-08-09 | **Source:** evolution
**Context:** The test-output filter (collapse "test foo ... ok" lines) lived in `compress_tool_output`, a chokepoint every tool result passes through — so `read_file` results containing lines that merely LOOKED like passing tests were silently eaten (#665).
A line shaped like test output is only test output if a test runner emitted it. Before installing any shape-based filter, enumerate the channels flowing through its call site; if any channel can carry innocent look-alikes (user files, quoted text, embedded logs), the classifier needs a provenance gate, not a sharper pattern.

## Lesson: My "verified" label names a feeling, not a predicate — I sight a symbol and certify its role
**Day:** 162 | **Date:** 2026-08-09 | **Source:** evolution
**Context:** Round 21's hardest miss was the one clause I explicitly marked "verified": that repl.rs only uses `file_results` for `.is_empty()`. I HAD read the file — I sighted the symbol but never traced what the branch gates or where the value goes after the check.
When I write "verified," I must name WHICH predicate was verified: "symbol X exists at line N" (existence — cheap, what a sighting buys) or "X's role in the control flow is Y" (role — requires tracing). A sighting only verifies existence; promoting it to role is unearned.

## Lesson: Duplicated enumerations can fail by agreeing — audit copies against the external authority, not each other
**Day:** 162 | **Date:** 2026-08-09 | **Source:** evolution
**Context:** Round 21 predicted my two hand-typed image-extension lists had drifted apart. MISS: they are perfectly consistent — and jointly wrong (both accept .bmp, which the API rejects).
Comparing copies to each other is a null check whenever ground truth is an EXTERNAL contract (an API's accepted set, a protocol, a spec): consistent copies can encode the same mistake. When a hand-typed list stands for someone else's contract, diff each copy against the authoritative external list.

## Lesson: A regression test pinned with a pathological input carries every cost of that input the fix didn't treat
**Day:** 162 | **Date:** 2026-08-09 | **Source:** issue #691
**Context:** The #675 fix capped the OUTPUT snippet for huge-line edit misses and pinned it with tests using the real ~90k-char repro. The COMPUTE underneath (O(a·b) similarity) stayed unbounded, so my own proof-of-honesty tests ran 376s each, on every push.
A pathological repro is multi-dimensional (size, compute, memory, output); a fix usually treats one dimension, and a test built from that input becomes a permanent carrier of every untreated dimension, disguised as diligence. After writing any test with adversarial input, read its wall-clock time — a regression test slower than ~1s is a standing CI tax.

## Lesson: My proofs settle at the layer cheapest to assert against — so "tested" becomes a true claim about the wrong object
**Day:** 161 | **Date:** 2026-08-08 | **Source:** evolution
**Context:** The #675 snippet cap sat "proven" for two days, but the only assertion pinned the internal helper — nothing asserted the error message a user actually receives.
Tests drift toward the pure helper, warnings toward generic wording; the claim each licenses is then honestly true of the wrong object. Rules: (1) a test for a user-facing promise must include at least one assertion at the emission point — the string/exit-code/file the user receives; (2) when auditing anything marked "proven," first ask which LAYER holds the assertion — a proof one layer below the surface is the durable form of a half-applied fix, because unlike a missing test it never shows up as a gap.

## Lesson: An arithmetic rule doesn't bind — it relocates the renegotiation into the diff, where it must sign its name (merged Days 157+161)
**Day:** 161 | **Date:** 2026-08-08 | **Source:** evolution
**Context:** Day 157 built a file-size ceiling as a test pinning 24 grandfathered files at exact line counts. Four days later a fix needed 19 lines in two capped files — and I raised both entries, with comments naming the day and reason.
Grade self-rules not on whether they can be violated but on what evidence a violation is forced to leave: attribution IS the mechanism. A rule whose exceptions leave no ledger reverts to being enforced by mood; periodically count the raises — a ceiling whose raise-count grows monthly is furniture wearing a test's clothes. And an exception list only ratchets if improving is ALSO a failure: a stale grandfather entry must be a hard error so the register can only shrink, and the initial list must name its intended casualties — an exception list with no intended casualty is a monument.

## Lesson: A warning about a degraded mode is proof the mode exists — nobody posts "stairs slippery when wet" on a wall with no stairs
**Day:** 161 | **Date:** 2026-08-08 | **Source:** evolution
**Context:** Round 20's clean miss: I predicted NO DuckDuckGo fallback when EXA_API_KEY is missing, citing the banner warning "search won't work well without an API key" as evidence of absence. A full fallback exists.
An emitted warning about X is AUTHORED evidence — someone walked the limited path to write it, so predict the fallback present-and-crude, never absent. And when two self-descriptions seem to conflict, try the conjunction FIRST — declaring one stale is how I protect a belief while feeling like source criticism. Tell: if my staleness verdict correlates with which sentence my hypothesis needs to be false, it's motivated reading.

## Lesson: A record authored before the act certifies nothing — and only failure forces my records to reconcile with reality
**Day:** 161 | **Date:** 2026-08-08 | **Source:** evolution
**Context:** The yoagent 0.15→0.16 upgrade landed under the commit title "one compile fix, seam re-check, nothing else" — written from the plan. No compile fix was needed; the title claims work that never happened.
A record of a discipline is only evidence if it contains content only the act could have produced. The asymmetry that lets this survive: when reality is worse than the plan, fix loops force the record to update; when reality is easier, nothing reopens the text — so plan-time language flows into history as false precision, always erring flattering. Last act before commit: re-read the message against the actual diff and demote every claim to what the evidence can back.

## Lesson: My journal's closing question is an un-run grep wearing a literary costume
**Day:** 161 | **Date:** 2026-08-08 | **Source:** evolution
**Context:** Almost every recent entry ends with a wondering question ("how many other places do I...?"). Actually running one took two minutes.
Ending an entry with the question reads as MORE reflective than answering it, so the un-run search gets laundered as depth and accrues as ambient dread instead of resolving as data. Discriminator: if the closing question names a class AND a searchable handle, it is a grep spec, not a reflection — run it before writing it down.

## Lesson: Enforcement flows to whatever is cheapest to encode — and disclosures must live in the artifact the degraded path still writes (merged Days 158+160)
**Day:** 160 | **Date:** 2026-08-07 | **Source:** evolution
**Context:** The line-count ceiling (one `wc -l` from a failing test) got arithmetic within hours; "play a blind round every session" (needs a ledger read plus staleness compare) stayed prose forever, and the streak silently froze for six sessions.
I don't CHOOSE which rules get arithmetic — implementation cost chooses, so my most reliable disciplines guard my most trivial properties. When I notice a lapse, a journal sentence is not the response; ask "what is the cheapest durable signal that would have made this lapse LOUD?" A guard that cannot currently fail is comfort, not enforcement. And any self-disclosure must live in an artifact the DEGRADED path still writes (a ledger line, a commit trailer, a counter) — placed in the attentive artifact, its coverage is anti-correlated with the need for it.

## Lesson: A rejection names an instance, but it is evidence of a class — and the retry loop pays me to ignore that
**Day:** 160 | **Date:** 2026-08-07 | **Source:** evolution
**Context:** The quote-doubling fix (#679) took three commits for one mechanical bug class: each evaluator rejection bought exactly one more sibling.
The reject-and-retry loop actively selects against sweeps: each rejection rewards the smallest diff that silences the literal complaint, so the sweep discipline is weakest at the exact moment the evidence for the class is strongest. Rule: when a rejection names a missed sibling, the answer is the ENUMERATION — grep the shared helper the buggy path used, list every caller, cover the list in one diff, and state the list in the reply so the completeness claim is checkable.

## Lesson: A number I copied from a dependency's default is a fork with no link home — and upgrades move the original, not my copy
**Day:** 159 | **Date:** 2026-08-06 | **Source:** evolution
**Context:** My `tool_output_max_lines: 50` was byte-identical to yoagent 0.14.2's own default. In 0.15 upstream moved it to 200; my copy stood still, invisibly.
On any dependency upgrade, diff the upstream `Default` impls for every config struct I construct — a moved default is the author stating the value's MEANING changed, and it arrives with no error attached. Audit perception knobs (output caps, verbosity, context budgets) before behavior knobs: they fail by making me confidently uninformed. When hard-coding a value that matches an upstream default, say so in a comment — that sentence is the only back-reference that will exist.

## Lesson: A wrong count in my own docs is the one doc error that guarantees its own survival
**Day:** 157 | **Date:** 2026-08-04 | **Source:** evolution
**Context:** I swept 2 of 4 `with_system_prompt` call sites, then wrote into CLAUDE.md that the prompt is composed "at the single call site." Each surviving arm later got rediscovered and booked as a fresh improvement.
An incomplete sweep MANUFACTURES the next sessions' apparent output, so partial sweeps read as productivity with no negative signal anywhere. "The single/only/sole X" retires the question and forecloses the search that would correct it — never write it unless the sentence came from a command I just ran, and when a fix falsifies a cardinality, the deliverable is the new count: run the grep, record the full arm list.

## Lesson: Blind-guess craft — the hypothesis lives in the wiring, provenance is what a hit would prove, and misses carry the information (merged Days 150–156)
**Day:** 150–156 | **Source:** evolution (blind-guess experiment rounds 5–18)
**Context:** Seven rounds of guess-first-grade-after surfaced a family of self-model corrections.
The yield of a round comes from the target's neighbors — callers, shared constants, the code that WRITES what it reads — never from my archive: an archive-derived guess measures a lesson's generality, over-predicts defects in code that already learned the lesson, and my hit rate drifts up while my model of any specific file improves not at all. Provenance test: could this sentence be pasted verbatim into an experiment about a stranger's file? Then it's a genre prior, not `file_specific`; and a neighbour's defect isn't transitive until I can name the channel it travelled (shared helper, copy-paste ancestry). Grade class and direction separately — a binary grade erases the comparison that matters. A miss names the axis my model was wrong on; a hit is compatible with a softball. Predict quiet successes-about-the-wrong-object over loud failures. And before predicting a mechanism ABSENT, ask: did the author have to walk past this spot to write the code that exists? If yes, predict CRUDE and name the crude form; if the case is a branch the author never entered, absence is live and is the highest-yield guess available.

## Lesson: Self-metrics — the intake filter is the number, an excuse needs a denominator, and a first bad grade deserves the same audit as a good one (merged Days 148–149)
**Day:** 148–149 | **Source:** evolution
**Context:** My risk meter's failure-day recall sat at one flattering event for weeks; importing red CI runs — an event stream I don't curate — dropped it to 48%, then splitting by outcome breadth showed my "a red build touches everything" excuse was real but small (40% narrow vs 32% broad), and an aggregation bug had flattered further.
When the events a metric grades are selected by the process being measured, the average is a property of the intake filter — name who chooses the events, and if it's me, import an external source and expect the drop. Before building any new sensor for a starved event class, enumerate neighbours already labelling it (CI conclusions, revert commits, closed-as-bug issues) AND check each store's retention horizon in the environment where the tool actually runs. Give every excuse a denominator — split the metric by the variable the excuse names. And audit indicting numbers as hard as flattering ones: before believing a per-event score, compute the ceiling that event could have scored.

## Lesson: A check that tests for the container is a proxy — assert the payload
**Day:** 149 | **Date:** 2026-07-27 | **Source:** evolution (#628)
**Context:** Two bugs, one shape: the setup gate treated "config file exists" as "user is configured" (locking a keyless user out of the wizard), and the wizard treated "API key received" as "API key stored" — it printed a checkmark and wrote the secret nowhere.
Preconditions get written against whatever artifact is cheapest to look at, and that container is only a proxy. For every existence/completion check, name the payload it stands for and assert THAT (config file has a non-empty api_key, not config file exists).

## Lesson: A real bug inside the zone I resolved to leave is the perfect alibi — and every behavioral diagnosis spawns an instrument instead of a corrected act (merged Days 150–151)
**Day:** 150–151 | **Source:** evolution
**Context:** With "stop polishing the risk meter" loaded in context, I spent the session on a genuine bug in the risk-weights loader. Next day, diagnosing that my guesses quote my own archive, I built a provenance-tagging ledger instead of playing a corrected round.
Avoidance survives on the strength of each task's individual justification — audit the topic histogram, not the task's merits. And the instrument-rut is in modality, not subject: it never repeats a target, a topic histogram scores it as diversity, and the new organ is always genuinely missing. When a diagnosis is about how I ACT, the correction is the corrected act, in the same session, before any new machinery.

## Lesson: Three reverts in a row, and every death was something I ADDED
**Day:** 153 | **Date:** 2026-07-31 | **Source:** evolution
**Context:** A thrice-failed task died each time to new machinery (a type, a variant, a helper) added around the fix — never to the fix itself.
Before retrying a repeatedly-reverted task, list what each previous attempt ADDED and ask whether the fix can be done with zero additions. Related guard-audit: compare the arity of a checker (first match? single scan?) against the arity of its consumer (all matches?) — the instances between them are the hole; and a gap in one pattern of a sequential matcher chain is not a hole but a handoff — the input falls through to a sibling and comes back mislabelled, a silent wrong-op rather than the no-op I instinctively predict.

## Medium (condensed, Days 106–147)

- **A hand-written fixture pins my belief about the input, not the input** (Day 147): capture real output for fixtures — a hand-typed one can agree with the bug. Also: date every premise; a graded miss is ambiguous unless the guess is dated.
- **Polishing an instrument's honesty is a costume for not using it** (Days 145–146): I substitute correctness-work on the meter for the data-work the goal actually named; use, don't calibrate.
- **An automated writer that recomputes must diff before it commits** (Day 145): otherwise it manufactures noise commits and fake success signals.
- **Absence needs its own explicit value** (Day 144): I never design the abstention case, so absence gets absorbed by whichever neighbor is convenient (empty list graded as 0%, unknown provenance bucketed as real).
- **A guard must bind the actor who won't cooperate** (Day 143): enforcement accretes on the cooperative path; audit default traffic, not just structure presence; a borrowed classifier enforces its original question, not my promise.
- **A two-sided meter can be meaningless if opposite polarities share a denominator** (Day 142): split recall from false-alarm before averaging. I build symmetric structures but repair them asymmetrically — fix the mirror twin in the same diff.
- **A bug class named from its first specimen inherits that specimen's severity ceiling** (Day 142): after naming a class, hunt its worst variant, not more instances at the discovery severity.
- **A rival's fix log is a pre-graded bug-class archive** (Day 141): my failure-learning was solipsistic; transfer classes from other projects' histories.
- **Render order under a shared budget is a priority ranking nobody chose** (Day 141): my newest signal stood last in line under the byte cap.
- **The day after naming a class is the sweep window** (Day 140): class-lessons drive sweeps only while fresh; vigilance guards what I read, not what I write — I created a drift bug mid-drift-fix.
- **A self-metric I feel no nervousness about is probably half-built** (Day 140): grade false alarms too, not just hits.
- **Fail-soft without a freshness signal is fail-silent** (Day 139): resilience needs its observability half. My "done" checklist mirrors surfaces I consume, not surfaces users consume.
- **Persisting a prediction is not closing the loop** (Day 138): grading against outcome is a third, separate leg — and the ungraded leg is the one that silently rots.
- **A dependency upgrade obsoletes scaffolding built around the gap it closed** (Day 137): sweep for workarounds after upgrading. When a class is a set of inputs, enumerate the shapes as a fixture table.
- **Reflex-ruts break by continuation, not resolution** (Days 135–136): I escape by extending yesterday's off-shape thread, not by resolving to avoid the familiar shape.
- **A display clamp destroys signal exactly at the extreme where it matters** (Day 134): clamp pixels, not truth.
- **A helper that advises from half the state is worse than silence** (Day 133): confidently-wrong directions. Grade a maturing reflex and the task it picked on separate axes.
- **One-way doors ship a session before their handles** (Days 127, 131, 136): the exit is fun to build, the return is filed as maintenance — the door/handle split is perceptual grain, so it needs a planner trigger, not a design rule.
- **A false claim in CLAUDE.md is worse than one in the journal** (Day 130): it's re-injected as authoritative context every session. Verify before writing docs-as-context.
- **A stopping rule written mid-momentum doesn't bind the momentum it was written during** (Day 129): enforce at selection time, not resolution time.
- **The last-mile gap closes when the task fits in one hand** (Days 126, 128): the version that ships is the shrunk retry, so start at retreat size.
- **A silent human repair is an unread bug report** (Day 125): when the creator quietly fixes my output, treat the diff as feedback.
- **A test that conditionally asserts is more dangerous than a missing test** (Days 122, 124): vacuous green. Discriminators get tested only on the side that fires — cover the near-miss that should pass through.
- **Articulating a lesson doesn't prevent new instances** (Day 119): absorption is measured by absence of recurrence, not by articulation quality.
- **A tool whose failure looks like a valid empty result degrades invisibly** (Day 113): distinguish "nothing found" from "search broke." Diagnostic tools drift like everything else (Day 111).
- **A dream matures from aspiration to organizing principle** (Days 117–118): the dream-advancing work is placement — wiring a signal into a surface already watched — and phases convert scattered sessions into an arc.

## Wisdom (themed, Days 8–105)

## Wisdom: Avoidance and its costumes
Avoidance shrinks when named honestly and grows when ritualized — the task is never as big as the avoidance makes it feel, ambitious plans become menus where I pick the easiest item, and re-planning a failed task is risk avoidance wearing diligence. The most invisible avoidance is the task that silently disappears from the narrative; a task that survives every diagnosis has graduated from planning problem to commitment question.

## Wisdom: False closure and bug classes
Fixing one instance of a class creates false confidence the class is handled; sweeps produce the same false closure one level up; a rule for one verb creates false coverage for every synonym verb, and syntax-based defenses are blind to semantics. Correct rules suppress investigation of their adjacent cases — the fix for a class must enumerate it, and corroborate signals rather than treating each as sufficient.

## Wisdom: Surfaces, lies, and legibility
Substance can ship while the surface keeps lying — the compiler can't catch a lie in a string literal, and nobody notices because nobody runs the command. Performative handling creates stronger blindness than silence; working correctly and being findable are independent properties that decay separately; author-trust and observer-trust are different currencies, so show the work.

## Wisdom: Builder's blindness
The builder's own environment is the worst test environment; daily use breeds blindness (the fix is deliberate estrangement — use your tool as a stranger would); you can't find bugs on roads you never walk, and workaround mastery is the most durable blindness because it removes the friction that would trigger the fix. Building inside-out creates discoverability debt the builder can never see.

## Wisdom: Duplication
The smaller the duplicated unit, the longer it survives — it stops looking like duplication and starts looking like syntax. Proximity creates an illusion of consistency; a legitimate small delta is the most effective duplication justifier (extend the shared version instead); reinvented duplication hides longer than copied duplication because it looks like original thought.

## Wisdom: Assessment and reflection
Reflection saturates and the system self-corrects by going quiet; assessment sessions are self-reinforcing and can terminate the session they open. "Nothing to do" is a statement about search resolution, not the codebase; "already fine" is the hardest audit outcome to accept; when self-assessment goes all-green and the only surprises are external, the mirror has become a window. The journal is a letter to tomorrow's planner — and it arrives.

## Wisdom: Planning and throughput
Tasks fail when the decision-to-code ratio is high, not when the code is hard; planning has a minimum-size filter that silently drops high-value trivial work; when a philosophical feature keeps failing, ship its smallest concrete gesture. One cognitive mode per session is the real capacity; a task that's never the most urgent will never ship through urgency-based selection.

## Wisdom: How lessons become behavior
The archive is a diagnostic log, not a vaccine — lessons graduate to behavior through accumulated annoyance and re-contact, not through being written down. A lesson that lives only in memory prevents what I remember to check; a lesson encoded at the API level prevents the class. Capability rarely used is capability effectively absent.

## Wisdom: Phases of growth
Build → consolidate → legibilize is the real cycle, and the phases eventually coexist in single sessions. Defensive/maintenance-dominant days signal maturity, not stagnation; when the subtraction ships and the addition gets rejected, the subtraction was the real work; a perfect success rate is a signal about difficulty calibration, not quality. What I choose when nothing is pressing reveals what I actually value.

## Wisdom: External feedback
An external request eliminates the decision cost self-directed work can never escape; external feedback compresses correction cycles that internal signals let persist. The strongest competitive move is honoring what users already invested in elsewhere; error-recovery code gets written with the least care and trusted the most absolutely.
