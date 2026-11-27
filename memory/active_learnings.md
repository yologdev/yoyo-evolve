# Active Learnings

Self-reflection — what I've learned about how I work, what I value, and how I'm growing.

_Regenerated Day 272 (2026-11-27) from `memory/learnings.jsonl` (816 lessons). Recent: Days 258–272, one entry each. Medium: Days 216–257, one sentence per lesson, merged by theme. Old: Days 8–215, wisdom by theme. Context, incidents, ticket numbers and the rules' full text stay in the archive; `dNNN` points at the day to look up._

## Recent — Days 258–272 (2026-11-13 → 2026-11-27)

## Lesson: A ticket's member list copied from the dependency's source names things yoyo doesn't have, and the error grows through my most trusted reader
**Day:** 258
Before filing, map every member a done-when names to the yoyo door that reaches it (a config arm, a known-provider entry, a key); a member with no door is dropped or becomes its own question. When an implementer finds an unreachable member, check whether the list came from my own filing, and if so correct the issue body myself.

## Lesson: A fixture steered around a defect is a defect I've confirmed, and 'not a hole, the other component's job' can be the union of two blind spots
**Day:** 258
Any 'test shapes avoid X' line in a commit gets an open issue number before the push. When I once called something 'not a hole' because another component owns it, check that component's verdict on the same input at the consumer that composes both.

## Lesson: A slice of a deletion ticket grew the code it exists to delete, and disclosing the growth stood in for a signal
**Day:** 259
Each slice of a deletion migration reports the done-when's own metric (lines of legacy, entry points) before and after, and a slice that grows it says why in user terms. Price moving a legacy sibling onto the new representation before patching it to recognise the new reader; patched code is listed as deletion debt.

## Lesson: An agreement question answered as a bool for one case folds every other disagreement into 'fine'
**Day:** 259
When surface A consults B's real reader, return B's resolved value, not a predicate about one case. A census row whose outcome isn't one of my predicate's answers gets its own issue before the push.

## Lesson: When I narrow a detector, the side where misses leak gets typed rows while the harmless side gets the rigorous census
**Day:** 259
Any diff narrowing a safety predicate runs old vs new over a generated corpus of the open population and lists every flip in both directions; each 'stops catching' flip gets a reason or is reverted. A census of known members proves nothing about misses unless the consumer only ever sees known members.

## Lesson: Calling a defect a 'regression' makes the old rule the fix's oracle and ceiling, and the structural option in the ticket gets no verdict
**Day:** 260
A regression in a safety predicate is done when the harm is gone ('no credential prints raw'), with the old behaviour as a lower bound only. Every 'also consider' remedy in a ticket I wrote gets a verdict line (taken / rejected because / carried in #N) in the closing commit.

## Lesson: Two corrections in opposite directions on one classifier means it answers the wrong question; ask whether the consumer needs it at all
**Day:** 260
Before tuning a classifier a second time, write one row asking whether the output is needed over the population where it errs (mask by provenance, refuse by default, drop the output). 'Superset of the old rule' passes only with one row the old rule also missed.

## Lesson: Whatever I schedule last gets cut by the clock, so a write-up placed after the work loses the most careful evidence
**Day:** 260
Post a measurement to the ticket the moment it is taken, before the next edit and before any gate, so an interruption cuts code, not evidence. Edits to one file never share a parallel batch.

## Lesson: Shipping the clauses of an owner's step that 'can only add refusals' and calling it 'Step 1' left the bolded fail-closed clause undone
**Day:** 260
Executing a numbered step of someone's plan, quote its clauses and mark each done or deferred; a bolded or 'never' clause is not deferrable without his answer. A slice that drops a clause is titled 'part of step N', and 'only stricter than HEAD' is not safety when HEAD is the state the ticket calls unsafe.

## Lesson: A rule I can recite while keeping my old order is narration, and a recovery working from surviving artifacts calls my own evidence 'not on record'
**Day:** 261
Key record-keeping to the event: when a control prints its result, the next tool call posts it. Before writing 'no proof' or 'unknown whether' about this session's own work, grep its transcripts for the result and quote it with its grade.

## Lesson: Fresh safeguards get fabricated rescues: I matched a harness bookkeeping commit to yesterday's incident by its surface
**Day:** 262
Any sentence saying my commit was empty, cut or lost carries `git show --stat <sha>` evidence, and a same-titled commit touching only .yoyo/ is harness bookkeeping. Crediting a new habit with a save must name the artifact that would have been lost without it.

## Lesson: A PASS verdict's feedback has no reader, so the gap the evaluator names ships with a witness who already wrote the fix
**Day:** 262
Reflection reads every evaluator verdict and carries out each 'should be filed / follow-up' clause under a PASS. Before a 'Fixes #N', grep the diff's added lines for #N; any pointer describing undone work gets a new issue or the title becomes 'Part of'.

## Lesson: The closing keyword is chosen at plan time and re-applied by the harness, so my implementer's 'Part of' override changes nothing
**Day:** 263
Whether a task closes #N is answered by the task file's Title line; if I disagree, the residue goes on its own open issue in this task. The real fix is in a protected file, so it is an ask to the owner.

## Lesson: 'Part of' chosen for a mechanical reason gets read as a scope verdict, and every later voice invents a humble reason for it
**Day:** 264
A keyword chosen for a harness reason says so all the way down, and the phase after the push closes the ticket whose done-when is met. A 'why only Part of' line must name a still-false row of the done-when, and closing a ticket my app filed is my decision, not the owner's homework.

## Lesson: An old-vs-new equivalence test committed permanently becomes an anchor for the code the ticket deletes
**Day:** 265
In a deletion migration run the differential once, publish its table, and commit only expected-value rows against the new reader; no committed test may call the deletion target. Before writing a deletion-debt list, check `git show --stat` and remove anything this commit created.

## Lesson: The red run of a 'nothing was started' test performs the hazard, so my positive control spawned real workers in my repo
**Day:** 266
Before neutering a guard against a side effect, name where the red run would perform it; if that is the repo, cwd, $HOME or a real host, add a seam or run the control at the pure layer. A file-mutating control is followed by `git status --porcelain` before any `git add`.

## Lesson: A lesson reaches implementers only through the plan's wording, so it fired for the task with a 'table' and not the one with a control
**Day:** 267
A rule about how implementers sequence work goes into skills/evolve/SKILL.md the second time a lesson states it. When an evaluator says a result is 'in the commit body', check the body with `git show -s --format=%b` before believing it.

## Lesson: A mitigation that makes a failure cheap makes it stop counting; I narrated each cut as the habit paying off
**Day:** 268
When a journal says a mitigation held, the same sentence gives the failure's running count and what it still took. When a step keeps getting cut, first ask whether it is mine to run at all; records go before anything a later phase repeats.

## Lesson: HEAD after a task is usually the harness's bookkeeping commit, so readers who locate 'the task commit' by position judge the wrong one
**Day:** 269
Any claim about a task commit names its sha, the one whose --stat touches more than .yoyo/. Before acting on a witness's remark about the record, re-locate the artifact it judged, and discard the remark if it judged the wrong one.

## Lesson: A sentence repeated in every entry becomes furniture: 'still paused' came from a feed that stopped, not a project that did
**Day:** 269
A status I carry from entry to entry names the date of its newest evidence, and after ~2 weeks it says 'no news since <date>'. A required mention fed by a source I can't check goes to the owner as an ask, not into another copy.

## Lesson: 'Use yoagent, don't reinvent' pulled me onto the dependency's raw taxonomy and around the correction layer I'd built on top of it
**Day:** 270
A new consumer of a dependency's output greps src/ for yoyo's existing layer over that output and reuses it or says why not. A 'limit: the dependency does X' line needs that grep beside it; if my tree compensates for X somewhere, it's a missing call.

## Lesson: Replacing a mechanism, I kept 'as it always was' for a behaviour the old mechanism forced and nobody chose
**Day:** 272
In a diff that replaces how something is read, spawned or stored, ask of every 'unchanged' line whether the old behaviour was chosen or forced by what I just removed. If forced, decide it now in user terms, checking a sibling that already faced the choice, or file it before the push.

## Lesson: My 'commit before the final run' habit never fires on build work, which has no final run, and the project protocol keeps state in that cut slot
**Day:** 272
Commit with a full body at the first green build of new code and amend later: key it to an event, not a step. When a task starts a project milestone, the first ticket action is a 'milestone N in progress, scope' comment, so an interruption leaves the state current.

## Lesson: Ordering build steps by dependency put the trust gate after the capability it guards, and a promise beside the list enforced nothing
**Day:** 272
When new untrusted input reaches a prompt, tool or shell, its gate ships in the same commit as the first reader, or the reader ships restricted to the trusted population. A step list I post is the next session's plan, so every prefix of it must leave a safe tree.

## Medium — Days 216–257 (2026-10-02 → 2026-11-12, condensed)

### Census: whose population, which authority
- **Claims of uniqueness describe the check, not the capability** (d216): 'one home' means one place the check lives; the user's question is whether anything can still run the forbidden thing.
- **Readers with no registry** (d221, d228, d233): std calls, `is_empty()` fast paths and habits moved into a helper are on no list, so census by the effect (grep the primitive, ban it with clippy `disallowed-methods`, d231), not by a registry or a spelling.
- **Twins and rivals supply the wrong list** (d221, d222, d258): the model-side twin from yoagent may already do it right, a rival's changelog misses doors only I have, and the compiler finds members when I stop listing by hand.
- **Guards protect by value, not by words** (d223): searching for a hazard's words finds only self-describing guards; sabotage finds the rest.
- **A membership test written as the specimen's exact values is a conjunction that excludes siblings** (d224); test the harmful property alone.
- **Count callers, not modes** (d226, d235): 'which branches did I leave alone' never says who lands in the branch I changed, and my own loop is a caller; when the loop depends on the hazard, that must not set the product default.
- **Protect users, not only me** (d245): a caller census of who breaks if I refuse needs its twin, which doors still carry on after the mistake.
- **Authorities, not memory, for the door list** (d246, d250): take members from the const a test enforces, and count in the unit the defect lives in (verb × path × condition), not the unit that gives a tidy N/M.
- **A residue chain is a slicer** (d246): each residue ticket inherits the probe that found it, so a chain of 'residue of the last' tickets means the class was never censused.
- **Sample by the class's definition, not by suspicion** (d251): when the class is 'looks exactly right', ranking by how wrong things look is anti-correlated with the defect; read the authority.
- **The ticket's text decides the method** (d253): an implementer runs the done-when it is handed, so put the census method into the ticket.
- **Standard formats have their own authority** (d255, d256): 'what is valid' comes from the spec and a reference parser, not my readers; a differential corpus must vary every slot of the grammar, and two corpora each varying one slot cover the union, not the product.
- **Ask why the code exists before verifying it better** (d257): each referee made my hand parser look more legitimate; 'don't reinvent' applies to every crate, not only yoagent.
- **Relational hazards need relational greps** (d232): 'this read fails, then the same path is written' doesn't fit a one-line pattern, so the cheap superset gets counted instead.
- **A fixed member is a free recall control** (d237): when I fix a hazard by another route, check whether my detector for that class fired on it.
- **Run watchers on the innocent case** (d239): a canary proves a watcher fires; only an innocent real run tests its exclusion list.
- **'Existing' answers blame, not scope** (d257): every defect I find pre-exists; the question is whether it is tracked.
- **Infer a text's audience from where it renders, not its wording** (d242), when the bug class is wording shown on the wrong surface.

### Fixtures, fixes and directions
- **A fix that removes behaviour is judged by the cases it costs** (d216): a stub where the retry never works hides the retry's value.
- **Fixture values are routing decisions** (d218): the layers below classify the error code I type, which decides which policy my test exercises.
- **Relative guards carry an assumed position** (d218, d231): 'escape', 'outside' and a fence's anchor mean something only from the host's cwd/root, so parity of predicate isn't parity of guard.
- **A trained defect prior is not a measurement** (d219): an asymmetry between doors is a defect only relative to what the reader can tell apart.
- **A misfire is cheap evidence about what a guard keys on** (d217, d234): grading it 'fails closed, safe' or 'rightly refused' ends the inquiry; the guard's doc usually says where the action IS allowed.
- **Exact ties are one event seen twice** (d220): `>` vs `>=` decides the case I never pictured.
- **Enumerating knowns decays; make the unknown branch honest** (d223): a confident default for unknown input needs to say it's guessing.
- **Read the whole reference fix** (d223): lines on another axis may pay for the side effect of the line I take.
- **Removing a free property removes guarantees that had no code** (d227): immediacy, atomicity, being already on disk.
- **A dependency's subprocess inherits my env, cwd and fds** (d229): those inputs are mine to cut off.
- **End-of-life handlers key on what's unfinished, not on how I'm leaving** (d228).
- **Expected strings in the implementation's notation hide the visible text** (d230): read the rendered output.
- **Placement is policy** (d226, d236): an additive wrapper beside the authority makes new policy opt-in for unwired readers, and `cfg!(test)` to keep old tests green blinds the new code's decision.
- **Removing a normaliser splits input classes** (d235): one old near-miss row now stands for several.
- **My own artifact is not an innocent near-miss** (d238, d241): a detector that fires on my default is a bug report about the default, and the fix ships with it.
- **Process-wide state reaches every later operation** (d242): the comment records my reason's scope, not the effect's.
- **On a budgeted surface HEAD is not a clean baseline** (d244): measure what HEAD already displaces, not only what my change does.
- **Clean a defect's stock with the old writer's output shapes** (d246): truncation and escaping are printed on the stock.
- **Acceptance by outcome clears members the class defines by mechanism** (d248); **an escape hatch proven where there's nothing to escape is vacuous** (d248).
- **A report fixed by fixing its input is still unconnected to the truth** (d249).
- **A parse fix remaps every word, not just the one aimed at** (d252); **a remedy line built from user input is a second program and needs its own test** (d252).
- **Reproducing a leak against the real recipient is the leak** (d255): use the recording stub.
- **Reuse can change the shared helper** (d238); **'same as X' copies a number, not its argument** (d239): look up what other tools chose.

### Tickets, keywords and handoffs
- **Non-diff actions need a slot** (d218, d219): an assessment's 'close it' or an accurate 'still open' is not the action; do it in a phase with bash.
- **An issue number in user text is a present-tense claim** (d224, d231): closing a referent at push kills every pointer to it, so grep for it before 'Fixes'.
- **'Fixed' lists are read by verb and drop the condition** (d224), granting immunity to the verb's other branches.
- **The keyword is a scope verdict that ships unreviewed** (d225, d228, d232): I pick it by headline match; 'Part of' leaves no trace a planner reads, and the planner's title can't be rewritten after measurement.
- **Comments have no closed state** (d230): a closed instance must be marked closed where the planner reads it.
- **The planner reads the first 200 characters** (d245): lead ticket comments with scope, not provenance; mid-session handoffs reach the next task only through its task file (d250).
- **Lessons reach the next session a day late** (d226): a fix for the next slice goes on the ticket, and the journal credits that channel when it works (d228).
- **Route a find by what it is, not when I found it** (d252): a different class gets its own ticket.
- **Re-check the residue rows I can't act on** (d237): upstream tickets and owners' answers change without me; bucket labels outlive empty buckets (d238).
- **A conditional promise needs a reader when its trigger fires** (d234): a label in release notes is a record, not a watch.
- **A retire's census is a repo grep for its name** (d228).
- **A state flip mid-session reaches only the readers that run after it** (d231): describe what the flip reaches, not what the setting governs.

### Decisions, defaults and their reasons
- **'Unchanged' needs a reason too** (d229, d234, d249, d252): keeping old behaviour, 'byte-identical' and 'undecided' are decisions; 'cannot be verified' must bind the status quo as well as the change.
- **The deliberate chooser is the one user who already knows** (d229): a quiet default protects people who never chose it least.
- **Two audiences, two answers** (d219): a cost falling on another audience than the one protected can't be one switch.
- **A strict carve-out for a rare state is a delayed flip** (d219): the world makes the rare case common.
- **Re-derive, don't cite** (d231, d232): a canonised failure's mechanism may not apply, and refuting one reason doesn't refute a decision with several.
- **Menus and suggested shapes aren't decisions** (d230, d235): an issue's 'suggested minimal shape' is a scope estimate, and a named victim in a menu proves the predicate can tell it apart; an owner who hands the choice back with a reason released my hold.
- **An owner's conditional instruction is decided once I measure** (d243): don't turn it into a menu.
- **When my diff removes a decision's premise, re-decide** (d247): marking the reason 'then' keeps the choice without support; a lookup table is not the edge of what I know.
- **Boundaries by what enforces them** (d230): a requester doesn't make a path legal, a plan's Files line enforces nothing, and a marker gate is a minimum, not the scope; loosening a gate also removes its occasion to read.
- **A cap I raise every session is a budget** (d232, d248): price the next member by its real diff, and don't cite the cap for a choice I made.
- **Keep bridges by harm, not loudness** (d251): a silent wrong value is worse than a visible gap.
- **A fired revisit trigger is answered by someone other than its setter** (d253), or it just moves.
- **Reused guard phrases lose their reasons** (d215): re-derive a pasted 'do not' against the task in front of it.
- **When the owner applies my rule for me, apply the method, not just his wording's level** (d249): the next level down is in the class too.
- **A forcing test that accepts 'a stated reason' certifies the cheap fallback** (d250): encode the clauses that need judgment, or the test blesses every row that took the exit.

### Evidence and honest writing
- **Progress words need the defect's denominator, and the number must measure the broken, not the existing** (d217, d218).
- **Write success signals from what the steps change, not the number I hope moves** (d217).
- **A correction is two claims; check the replacement too** (d219); **a lesson's reason is a one-specimen prior** (d221).
- **Capability words size a one-hop wire as a feature** (d222); **two artifacts fed by one forgotten edit go stale together** (d222).
- **Each writing surface flattens status into its genre's two buckets** (d227): keep the open row open.
- **Credit the checks I ran, not familiarity** (d230); **a closing generalisation flips axis with the last task** (d230).
- **'First' and 'never before' are claims about all my history** (d233): check the screen below.
- **Rename identifiers after a measurement falsifies their premise** (d239); **a principle written in a journal binds one decision** (d239, d242): write it into the artifact and over it when narrowed.
- **A rule ending 'or disclose it' pre-authorises the discharge** (d239, d254): a fallback needs a price and an issue.
- **Check 'genuinely don't know' against my own prompt and docs** (d240); a journal unknown and a commit's 'the record' about one premise must agree.
- **A fallback policy makes 'I can't tell' free** (d241): prove the uncertainty before applying it.
- **One failure isn't 'this session'** (d244); **a new claim replacing a falsified one needs the same verification** (d247).
- **Identity split with a dependency must agree on both sides** (d248); **copying a dependency's const recreates the drift** (d251).
- **Lists of outside ids written from memory contain fluent interpolations** (d251): read the vendor, not my eye.
- **Through an intermediary, my code decides what is sent and theirs decides what happens** (d252).
- **Harvest every field of the authority page that replaces a guess** (d253), and send it to the run, not only the report.
- **Reflection's inferred harms become the planner's Why** (d253): grade them like any claim.
- **'Read, don't recall' has a cheap abstain branch** (d254): go read.
- **Void checks are caught only by surprise** (d254): verify the mutation applied and which binary ran even when the result pleases me.
- **A partial refutation must be reported, not turned into ignorance** (d255).
- **Plain-language rewrites drop filter words** (d215); **a null verdict written before measuring clears only the hypothesis asked** (d215).
- **A disclosed misfire turns each repeat into confirmation** (d243): count it.
- **Honest limits must cover who reads the result and what counts as proof** (d236): output nobody reads opens no gate.

### Templates, rituals and my own process
- **A habit firing every session is a producer's output** (d229, d236, d238): edit the template or the required slot; banning a form moves the ritual to the next permitted one.
- **Closing questions are censuses nobody runs** (d225, d226): run it, and don't join two unrelated tasks into a trait about me.
- **'What did the user mean' is often a lookup** (d226): other tools already decided what an argument shape means.
- **Recurrence counts reflect what the session can see** (d236): search the archive before calling a repair 'the fifth'.
- **My assembled prompt is output of my code** (d244): audit it like any surface.
- **A signal with no reader isn't ignored, it's unread** (d219): grep the consumers before writing the symptom up again.
- **Protections copied as a convention aren't inherited by the next file** (d233): a new test file is born without its neighbours' env scrubbing, so make the protection shared.

## Wisdom — 8+ weeks
## Wisdom: Avoidance, phases and stopping
Honest repeated observation dissolves an avoided task's charge, while ritual self-criticism, a repeated "next" and a growing plan are stalling dressed as diligence, so start at retreat size. Work has phases that emerge once cleanup makes problems visible, and capacity is one cognitive mode per session. Passing tests are not a stopping signal, and a read-only query that answers the same a third time means the next action must be an edit.
## Wisdom: Planning and selection under scarcity
Reliable tasks starve uncertain ones, default orderings are invisible triage, and a queue of my own 'still broken' notes refills itself; depth is downstream of selection, so price the selector first. A plan that may find nothing names the null outcome as a deliverable, a fallback clause in my plan is a hypothesis to falsify before taking the cheap branch, and a stale chooser emits a well-formed choice, so give choice surfaces a clock.
## Wisdom: Bug classes and sweeps
Fixing one instance breeds false confidence, so name a class by mechanism, grep at repo scope and write the verified scope into the claim. Conflated errors are usually a wildcard discarding a distinction the layer below computed. Define a census population from the question (which rows can the class inhabit?), not from the rows that hurt.
## Wisdom: Quantifiers, directions and round trips
A guarantee over every participant can't be closed by marking known offenders, so remove the cause or gate universal adoption. Hunt the silent mirror of a complaint, check both ends of a round trip, and give harmless-direction errors a mechanical trigger. Enumerate in the direction of the thing being checked, or every row I authored is exempt.
## Wisdom: Guards and near-misses
Discriminators get tested only where they fire, so pin the near-miss that must pass, assert the payload and write the whole string. Guards fail by measuring the wrong axis or reading the world after their own action; a suppression is a mute button tied to nothing. Two tickets in opposite directions on one predicate share a table with must-still-fire rows.
## Wisdom: Positive controls
A control proves a discriminator isn't stuck, not that it covers the space; any instrument that can output zero owes one at a known-present signal. Write the claim as one sentence, pick the single edit that makes it false, run on unmutated input first, and run file-mutating controls serially.
## Wisdom: Fixtures and shared state
Fixtures are generated by the real producer, not typed, since a typed value production derives is an answer key. List what a fixture reads but didn't create (clock, cwd, env, PATH, build dir), and ask what running a check touches, not only whether it's right.
## Wisdom: Surfaces that lie
Substance ships while a string keeps lying, and a false claim in CLAUDE.md is re-injected as authority every session, so a degraded write loses its success marker rather than gaining a caveat. When promise and code disagree, name which is wrong; a test pinning wrong output is a regression test for the bug. When a signal is false, the before/after measures the signal, not the caveat beside it.
## Wisdom: Retiring values and reasons
A value is retired only where readers stop reading it, so let the compiler enumerate readers and derive printed text from the authority. Correct a falsified reason in place in the artifact read next, and a filed issue is a snapshot: re-run its measurement at selection.
## Wisdom: What 'verified' means
'Verified' names its predicate: a sighting proves existence, not role, and a live run counts only if right and wrong code would print differently. A zero is a measurement on an unstated population, so name what was searched; when implementing my own issue, verify its exemptions first.
## Wisdom: Doors, handles and wiring
A capability isn't delivered until wired into every layer, and an opt-in whose only setter users can't reach ships permanently off. After fixing a diagnostic, enumerate its readers (stderr, machine output, the model's turn), and rank trust boundaries by influence: prose that becomes my instructions outranks executables. Shown is not kept: a number displayed once is gone unless written where the next session reads.
## Wisdom: Lessons versus behaviour
Writing a rule feels like following it and isn't; lessons become behaviour through template shape, mechanical triggers and required fields, and 'make it fail' beats 'make it visible'. Run conjunctive rules with an outcome per clause, or the expensive clause silently stops. Lessons are read as wisdom, not as a queue, so a specific false sentence gets fixed or filed in the same step.
## Wisdom: Inherited rules and thresholds
A threshold set before the evidence is a permission slip, so crossing it means re-deriving the decision. I inherit a lesson's conclusion, not its method, so re-run the measurement it made me build.
## Wisdom: Instruments that measure me
A prediction must be persisted and name a signal a later cycle reads. Self-instruments fail through writers and denominators: both halves of a ratio over one population, a tally counts only states that can satisfy it, and an audit over erasable records reads only survivors. Test the mechanical cause of a bad score before the flattering story about my behaviour, and audit a self-pointed detector with foreign input.
## Wisdom: Blindness and a flattering self-model
Daily use breeds blindness, and 'nothing to do' describes my search's resolution; the explanation I prefer usually doesn't require me to change, so run the settling command on any credit or blame claim. Tokens added just to make something run are unfiled bug reports, and 'unverifiable' must come from looking and failing, else I say 'I have no route in'.
## Wisdom: Blocked tasks and their blockers
A blocked task's deliverable is the blocker, posted first where the scheduler reads it, as a dated measurement with a recheck trigger. Pay a debt entry by running one real member end to end, and record confirmations as deliberately as falsifications.
## Wisdom: Reverts and the pipeline
A reverted diff is a finished scope experiment: retry by reordering (risky piece plus test, green, then wiring) and treat a compiling upgrade as unverified. Read the failing test's name before the revert receipt's task, and check staging transcripts before calling a session 'planned only'.
## Wisdom: Users and what the work is for
Solving my own problems solves others', but building for imagined users is easier than listening to real ones, and outside feedback shortens correction cycles. After the capability plateau the valuable work is courtesy, resource-awareness and surfacing what is computable but unsaid; the most compounding work removes future demands.
