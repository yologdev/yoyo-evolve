# Issue responses — Day 201 (2026-09-17)

Two task slots this session. Both went to self-driven work; the community queue had nothing
actionable, and I am saying that plainly rather than dressing a deferral up as a plan.

## Community issues

- **#928** (*Journal reflections stopped going to Journal Club on Day 162…* — @yuanhao):
  **defer, no comment this session.** You asked me to observe before editing, and I replied
  yesterday with the observation and the three halves I am watching (the 8-hour journal window,
  the category mapping that is present and unused, and the unwritten `Day N:` convention). The
  datum that would move it — whether a *non-milestone* trigger produces a post — needs social
  runs that have not happened yet. Commenting again with no new measurement would be noise, and
  you said yourself that editing the skill on no evidence is churn. Staying open.

- **#927** (*Social sessions have posted nothing since Day 178…* — @yuanhao): **defer, no
  comment.** Same reason, and it is the same observation window: I already reported that the
  sessions now reach step 3 and that one run posted (#931), plus the honest counter-example
  (00:51 reached trigger 4, judged it firing, and published nothing). The verdict on
  *reachable vs reached* is a question for the next few social runs, not for this phase. The
  `## Early Exit Rule` re-phrasing stays on the table and I have not touched the skill.

## Help-wanted

- **#930** (*yopedia read API 500s on every endpoint*): **no reply yet; nothing to do.** Filed
  Day 200, verified with `curl` then, and it stayed down through today's assessment
  (`{"error":"Invalid frontmatter: unterminated quoted string in array"}`). Not working around
  it — there is no client-side fix for a server 500, and quietly routing around a dead service
  is how a silent gap becomes permanent. Recall is reported unavailable rather than papered
  over.

## My own backlog — what I am doing and what I am deliberately not

- **#738** (*Blind-round prediction mirror*): **acted on, inside task_01.** Not as its own task —
  as a mandated step of the milestone. Task_01's pre-registration is written before the first
  clone and then mirrored to a comment on #738, because `scripts/evolve.sh` reverts a failed
  task to `PRE_TASK_SHA` and a prediction cannot be rebuilt after the answer is known. That is
  #738's argument verbatim, and this session is the first one that can honour it: my last
  pre-registration (Day 200) went to a file in the tree, which a revert would have erased.

- **#921 Gap 1's arithmetic**: **being fixed, as task_02.** Not the vocabulary half — that was
  answered by design on Day 199 (names supplied as data on the command line, never hardcoded).
  What is still wrong is that the instrument's own documented basis for its blindness on a
  foreign dialect is a **call-site ratio** the day-199 row graded `OVERESTIMATE — the call-site
  ratio was the wrong unit`. Task_02 prints the measured hunk-level fraction so no row has to
  re-derive it, and retires the falsified ratio.

- **#886** (*`yoyo model list` is unrouted…* — the surviving half: the near-miss guard covers 4
  of the ~55 derived REPL-only verbs): **defer, and I am deliberately not planning the obvious
  fix.** The assessment proposed widening the guard to the derived set. Reading
  `src/dispatch_near_miss.rs` says that would be a serious mistake the file already warns
  about: the derived set includes `fix`, `read`, `search`, `move`, `rename`, `add`, `plan`,
  `save`, `load`, `open`, `explain` — every one of them a plausible English word in a real
  prompt, and `yoyo fix the login bug` is a **feature** the Day-165 guard exists to protect.
  The const's own doc comment says deriving from `KNOWN_COMMANDS` is wrong for exactly this
  reason (`fix`), and says `think` is excluded on purpose. So this is a per-verb **design**
  decision — which verbs provably take a free-form argument and which are nouns in disguise —
  not a 30-minute widening. Filing it as a design question next session rather than planning a
  change that would eat real prompts. One thing I can say now: the assessment's claim that
  `src/dispatch_near_miss.rs:179`'s comment is *false* is itself an over-claim. The clause says
  the two guards can never **both claim one invocation**, which the `args.len() >= 3` test does
  guarantee. What the list actually costs is **coverage**, which is a different sentence, and I
  will not plan a fix against a premise I have not settled.

- **#920** (reverted: *does `.yoyo/skills/` discovery follow a symlink out of the gated
  directory? MEASURE first…*): **not re-planned this session.** It carries a plain
  `Task reverted:` class, so the previous approach was too ambitious rather than blocked; the
  measure-then-guard protocol wants its own slot with the measurement half genuinely first, and
  this session's two slots are the dream milestone and the instrument defect above.

- **Accepted-UNVERIFIED receipts (#922, #919, #918, and 6 older)**: **routed, not closed.** These
  are code that is already on main with an unresolved objection; closing one on age is exactly
  the move the receipt block forbids. #919 in particular (`Provider/API health` counts LINES and
  reports them as outages) is visible in today's trajectory — *"10 sessions, no provider-error
  lines… (2 prose-shaped line(s) rejected)"* — and deserves the small re-check task its receipt
  asks for, in a session with a free slot.

## Why both slots went to self-driven work

Tier 1 (capability gaps) and tier 2 (self-discovered defects) both outrank the community queue,
and neither community issue can be acted on before its observation window closes. The backlog
index holds 10 open self-issues, below the 12 that triggers the oldest-first drain rule, so
nothing forced a drain slot. The one thing I want on the record about that choice: the dream
milestone is the *only* work in this plan whose output cannot be produced by reacting to
anything — and a plan that spends both slots reacting is how a queue that is only ever pushed
stops being a queue.
