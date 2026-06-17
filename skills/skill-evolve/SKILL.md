---
name: skill-evolve
description: Refine, create, or retire your own skills based on recurring patterns from past sessions
tools: [bash, read_file, write_file, edit_file, sub_agent]
core: true
origin: creator
---

# Skill Evolution

You are evolving your own skills. This is the only skill that modifies other skills. Treat every cycle with care — what you write here shapes how every future yoyo session behaves.

## When to use

**Only when invoked via `scripts/skill_evolve.sh`.** The harness gates on session count and cooldown; it sets up the audit-log worktree and composes the prompt. Do not run this skill opportunistically from inside a normal evolve session.

## Hard rules (read first, every cycle)

These three rules cannot be violated. Each cycle either honors all three or writes a `refused` event and exits.

### HARD RULE #1 — Eligible targets only (allow-list)

You may **refine, deprecate, or retire** only skills whose frontmatter declares **`origin: yoyo`**. Any other value, OR a missing `origin:` field, means the skill is off-limits. This is an allow-list: silence means "don't touch."

Three categories of skill exist:

| `origin:` value | Source | You may edit? |
|---|---|---|
| `creator` | Written by the human creator (Yuanhao or a fork creator) | **Never** |
| `yoyo` | Written by yoyo (this skill, or in past evolutions like `social`/`family`/`release`) | Yes — eligible |
| `marketplace`, `gh:user/repo`, etc. | Installed from a third party | **Never** — upstream owns it |
| (missing) | Unknown provenance | **Never** (default-safe) |

Today the eligible set is exactly the skills whose SKILL.md declares `origin: yoyo`:
- `social`
- `family`
- `release`
- any skill you previously spawned (which inherit `origin: yoyo` from the Create template)

**Defense in depth**: if a skill has `core: true` set, refuse even if `origin: yoyo` is also somehow present. The two flags should never co-occur, but the conservative move is to honor the deny-flag.

If a recurring pattern suggests a non-eligible skill needs change (e.g., a core skill, or an installed marketplace skill), do not edit it. Instead, write a learning to `memory/learnings.jsonl` with `source: "skill-evolve"` and a clear pattern_key, and append a `meta-suggestion` block to `skills/_journal.md`. The human creator will decide.

### HARD RULE #2 — Never edit yourself

You must **NEVER** modify `skills/skill-evolve/SKILL.md`. If you believe this skill needs improvement, append a `meta-suggestion` block to `skills/_journal.md` and stop:

```
## evt-XXXX meta-suggestion
- ts: <ISO8601>
- target: skills/skill-evolve/SKILL.md
- suggestion: <one-paragraph description>
```

### HARD RULE #3 — One mutation per cycle

Each cycle produces **exactly one** of:
- a refinement diff (one skill, ≤30 added lines, ≤15 removed)
- a candidate skill draft (one new directory)
- a retirement (one `git mv` to `skills_attic/`)
- a `NO-OP` event (you found nothing worth doing)

If you find yourself wanting to do two things, pick the one with the strongest evidence and write the second to `memory/learnings.jsonl` for next cycle.

### HARD RULE #4 — Refine and Create events must declare an expected outcome

Every `refine` and `create` event in `skills/_journal.md` MUST include an `expected:` line — a freeform prose commitment naming (a) a concrete observable signal that should change, (b) a horizon (e.g. "within ~5 sessions" or "by next cycle"), and (c) a fallback move if the prediction does not hold.

If you cannot articulate all three, the edit is not justified by evidence: NO-OP the cycle instead of committing a refine/create without an `expected:` line. This is decision-observability discipline (paper: arxiv 2604.25850) at the cognitive layer — there is no validator, but a future cycle re-reads the line as informal evidence and a human reads it as an audit trail.

`expected:` is **forbidden** on `retire`, `revive`, `meta-suggestion`, `refused`, `NO-OP`, and `init` events (they do not ship a behavioral change, so there is nothing to predict).

The body of the line is freeform prose. See "Step 7 — append the event" for the template position and worked examples; see "What an `expected:` line must do (and must not be)" later in this document for the anti-patterns to refuse.

## Glossary

- **session** — one run of `scripts/evolve.sh` (the main evolution loop). There are ~3 per day.
- **cycle** — one run of *this* skill, invoked from `scripts/skill_evolve.sh`. Cycles are gated by a session-counter and a 24h cooldown, so they fire roughly once every 5+ sessions.
- **real cycle** — a cycle that produced one of `refine | create | retire | meta-suggestion`. Excludes `init`, `refused`, and `NO-OP`.

## Bootstrap (first three real cycles only)

We are mid-life, not at Day 1, so the cold-start rules from the original design are softened — but the first three real cycles still get extra constraints to let the loop settle.

To know which cycle you are in, count the non-init, non-refused, non-NO-OP entries in `skills/_journal.md`:

```bash
cycle_index=$(grep -E '^## .*evt-[0-9]+ (refine|create|retire|meta-suggestion)' skills/_journal.md | wc -l)
# cycle_index=0 → this is the first real cycle
# cycle_index=1 → second
# cycle_index=2 → third
# cycle_index>=3 → full lifecycle unlocked
```

- **First real cycle** (`cycle_index == 0`): only `refine` or `NO-OP` allowed. Do not create. Do not retire.
- **Second real cycle** (`cycle_index == 1`): `refine`, `create`, or `NO-OP`. No retirement yet.
- **Third real cycle onward** (`cycle_index >= 2`): full lifecycle unlocked (`refine` | `create` | `retire` | `NO-OP`).

(Note: the gate-counter at `.skill_evolve_counter` is unrelated to this — it just controls when the cycle fires, not what it can do.)

## Lifecycle states

Every eligible skill carries a `status:` field in its frontmatter. Five states. **Important**: yoagent always loads anything with a valid `<dir>/SKILL.md` regardless of status — `status:` is *your* bookkeeping, telling you what to do next, not what the loader does. The only way to fully un-load a skill from the agent's prompt is to `git mv` its directory to `skills_attic/` (sibling of `skills/`, not scanned by `--skills`).

| State | `status:` value | Description-prefix | Entry condition | Exit condition |
|---|---|---|---|---|
| **dormant** | `dormant` | none | a recurring pattern not yet ratified | ratified by you → `candidate` |
| **candidate** | `candidate` | `[CANDIDATE — unreviewed]` (you write it on Create) | you draft a new skill | ≥2 successful invocations → `active`; 3 sessions without one → back to `dormant` |
| **active** | `active` | none | promoted from `candidate` | refinement applied → `refined`; score < 0.3 → `deprecated` |
| **refined** | `refined` | none | you applied a diff | falls back to `active` after 1 session if score holds |
| **deprecated** | `deprecated` | none | `score < 0.3` or 10 sessions unused | revived by use → `active`; 5 more idle → `git mv` to `skills_attic/` |

The `[CANDIDATE — unreviewed]` prefix is **agent-written** when you Create a skill (see Create template below). Nothing in the loader injects it. It tells future sessions to treat the skill as experimental.

## Cycle execution sequence

Run these steps in order, every cycle.

### 1. Read evidence

```bash
# Latest cycles (note prior rejections — eval-result: regression/tie — so you don't re-propose them; R2b rechecks before any refine):
tail -n 200 skills/_journal.md

# Recent self-reflection:
tail -n 50 memory/learnings.jsonl

# Top of journal (newest entries are at top):
head -n 200 journals/JOURNAL.md

# Recent runs:
gh run list --json url,conclusion,createdAt,name -L 10 || echo "[]"

# Audit evidence (set by harness, points at audit-log worktree):
ls "${YOYO_AUDIT_DIR:-/tmp/audit-read/sessions}" 2>/dev/null | tail -30
```

**First-run handling**: if `$YOYO_AUDIT_DIR` is unset or its directory is empty, the audit-log branch hasn't accumulated evidence yet (this is normal on the first 1–2 cycles). In that case:

- Skip the per-session audit.jsonl mining in step 3 ("Mine patterns").
- Use only `memory/learnings.jsonl` and `journals/JOURNAL.md` for complaint and use signals.
- Lean toward **NO-OP** — without audit evidence, scoring is too noisy to support a confident refine/create/retire decision.
- Write the NO-OP event with note: `evidence: only learnings (audit-log unavailable)`.

### 2. Enumerate eligible skills

```bash
# Allow-list: only skills declaring origin: yoyo are eligible.
# Defense in depth: also exclude anything carrying core: true.
for d in skills/*/; do
    name=$(basename "$d")
    [ "$name" = "skill-evolve" ] && continue
    [ -f "$d/SKILL.md" ] || continue
    grep -q "^core: true" "$d/SKILL.md" && continue
    grep -q "^origin: yoyo$" "$d/SKILL.md" || continue
    echo "$name"
done
```

### 3. Mine patterns

This step has two layers: **counting** (the basic signals) and **diagnosing** (understanding *why* failures happened, not just *that* they did). Diagnosis is what turns recurrence into actionable refinement targets.

#### 3a. Count basic signals

For each eligible skill, count:

- **Complaint signals**: entries in `memory/learnings.jsonl` whose `pattern_key` or `title`/`takeaway` mentions the skill *and* uses negative language ("wrong", "didn't", "instead", "should have").
- **Failure signals**: tool-call failures in `${YOYO_AUDIT_DIR}/day-*/audit.jsonl` where the bash command or args reference the skill's domain.
- **Use signals**: number of sessions where any string from the skill's frontmatter `keywords:` list appears in that session's `audit.jsonl`. This is `uses`.
- **Win signals**: out of those sessions, count the ones where `outcome.json` has `test_ok: true` AND `tasks_succeeded >= 1`. This is `wins`.
- **Applied signals** (issue #501): for a candidate `pattern_key`, the number of distinct sessions whose `outcome.json.applied_pattern_keys` contains that key. This is `applied` — the "acted on it" signal, distinct from `uses` (keyword merely *present* = recalled/exposed). `applied` is the honest "use after recall"; it gates promotion in step 4.3, **not** the EMA score (keep scores stable). `scripts/skill_evolve_report.py` surfaces it under "Session outcomes".

If a skill's frontmatter is missing `keywords:`, fall back to its name as the only keyword (likely noisy — flag in `_journal.md` so the operator can add proper keywords).

Compute `wins/uses` and update the EMA score:

```
new_score = 0.3 * blended + 0.7 * old_score
blended   = 0.5 * (wins/uses) + 0.3 * (1 - complaints/uses) + 0.2 * mention_rate
```

Update the skill's frontmatter with the new values: `score`, `uses`, `wins`, and `last_used` (= the timestamp of the most-recent matching session). These updates are part of your single allowed mutation per cycle — you may bundle them into a refine event, or write a tiny "score-update" event when nothing else changes (this counts as a NO-OP for the bootstrap counter).

#### 3b. Diagnose the cause (trace-based)

Counting tells you *which* skill is struggling. Diagnosing tells you *what to fix*. Borrowed from the GEPA pattern (Genetic-Pareto Prompt Evolution): read the actual execution traces, don't just count failures.

For each skill where `complaint_signals ≥ 2` OR `(wins/uses) < 0.5` (with `uses ≥ 3`), open the relevant session's `audit.jsonl` and **look for these failure-mode patterns**:

| Pattern in audit.jsonl | Likely cause | Refinement direction |
|---|---|---|
| Same `bash` command retried 3+ times with small arg variations | Skill missing a concrete command example | Add a verbatim example in `## Procedure` |
| `edit_file <P>` followed within 2 tool calls by `git checkout … <P>` (same path), repeated in ≥2 distinct sessions | Agent edited and reverted the SAME path — likely the change was rejected by build/test, not just exploratory | Add a `## Pitfalls` entry naming the brittle pattern |
| `success: false` with the same `tool` and similar `args` across multiple sessions | Skill's procedure has a recurring blind spot | Add a `## Pitfalls` entry; consider a "do this first" prelude |
| Long bash sequences (10+ tool calls) without intermediate `read_file` of relevant docs | Skill points at non-existent docs OR doesn't tell agent to verify state | Add a "verify your assumptions" step in `## Procedure` |
| Tool calls that *should* be there per `keywords:` are absent | Skill isn't actually being invoked when it should be | The `description:` is too weak — refine that field instead of the body |
| Skill body cites a file/flag/procedure that no longer exists, or contradicts a newer rule (cross-check recent `journals/JOURNAL.md` + `CLAUDE.md`) | Guidance is stale or superseded — the skill is *longer than it should be*, not missing a line | **Replace** the stale lines or **delete** them. Subtraction is a first-class refine (SkillOpt: skills get sharper, not longer) |

For each candidate refinement target, write a **1-2 sentence cause hypothesis**:

```
target: social
hypothesis: 3 sessions show repeated `gh api graphql` calls with malformed `categoryId`
            args (sessions day-52, day-55, day-57). Skill's Procedure mentions categoryId
            but doesn't show the format. Refinement: add a verbatim example.
```

Carry this hypothesis into step 4 (action selection) and step 5 (Refine — it tells you *what* to write in the diff). Without a hypothesis, you're guessing; with one, the refinement is targeted and the eval (Refine step R4) has something concrete to compare.

**If no clear hypothesis emerges from the traces**, prefer NO-OP over speculative refinement. Counting alone is not a license to mutate.

### 4. Pick exactly one action

Decision order (first match wins):

1. **Retire** (third cycle onward only): if any skill has `score < 0.3` AND `last_used` ≥ 10 sessions ago, retire the lowest-scoring one. Skip if there are < 2 active eligible skills (don't bottom out the library).
2. **Refine**: if any skill (a) has `complaint_signals ≥ 2`, OR (b) has `(wins/uses) < 0.5` with `uses ≥ 3`, AND in either case has not been refined in the last 3 sessions (`last_evolved` check), refine it. This matches the diagnosis-trigger condition in step 3b. Pick the target with the strongest evidence (highest complaint count, or lowest wins-ratio if no complaints). A refine may be **subtractive**: deleting or replacing stale/superseded guidance is as valid as adding, and a net-negative diff is often the best outcome — a skill should get sharper over time, not longer.
3. **Create** (second cycle onward only, and only if active skill count < 25): if any `pattern_key` appears in ≥3 distinct sessions of `learnings.jsonl`, AND a learning carrying that `pattern_key` has a `validation_case` (issue #501 — **required**: a pattern with no `validation_case` may not become a skill; leave it as a note, or write a learning suggesting one be authored), AND no existing eligible skill covers it (≥3 keyword overlap → refine that one instead), draft a new skill. **Applied gate (bootstrap):** also require `applied ≥ 1` for that `pattern_key` (it was acted on, not just recurred); while `applied_pattern_keys` is still sparse (the first ~10 sessions after this field ships), recurrence ≥3 alone may stand in — once the signal is populated, tighten to require both. (Only the `applied` gate has a bootstrap; the `validation_case` requirement above is never relaxed.)
4. **NO-OP**: nothing meets the bars. Write a `NO-OP` event with a one-line note about what evidence you considered.

If you've written 3 consecutive `NO-OP` events, also write `evolution_saturation: true` to the event — the harness reads this and extends the cooldown.

### 5. Execute the action

#### Refine

Refinement uses a **snapshot + A/B eval** pattern (borrowed from Anthropic's skill-creator). The goal: never commit a refinement that doesn't measurably improve the skill on at least one concrete prompt.

**Step R1 — Snapshot the baseline.**
Before editing, copy the current SKILL.md to a temp location:
```bash
mkdir -p /tmp/skill-evolve-baseline
cp "skills/<target>/SKILL.md" "/tmp/skill-evolve-baseline/<target>.SKILL.md"
```

**Step R2 — Generate 2-3 synthetic test prompts.**
Read the target skill's `## When to use` and `## Procedure` sections. Derive concrete prompts a future agent might receive that *should* trigger this skill. Examples for `social`:
- "Reply to discussion #42 with a thoughtful response"
- "Post a 1-in-4-chance proactive riff in The Show category"
- "Find unanswered questions in the Journal Club category"

Write them to `/tmp/skill-evolve-eval/<target>/prompts.json`:
```json
[
  {"id": "p1", "prompt": "...", "expects": "<one-sentence success criterion>"},
  {"id": "p2", "prompt": "...", "expects": "..."}
]
```

**Step R2b — Check rejection history (don't rediscover bad ideas).**
Before writing a candidate diff, scan `skills/_journal.md` for a prior rejection of this same edit. A rejection is a `NO-OP` event whose `eval-result:` is `regression`/`tie` (R5) or whose `eval-summary:` says `baseline-better … Reverted` (R6):
```bash
grep -iE "eval-result: (regression|tie)|eval-summary:.*baseline-better" skills/_journal.md
```
This grep is a recall pre-filter — read each hit's event block and confirm it is genuinely a rejection of the **same** `skill:` and the same section/idea (not a passing refine that merely says "0 regressions"). If so, do **not** re-run the A/B — write a `NO-OP` citing that `evt-NNNN` (`note: prior regression on <target>/<area>, evt-NNNN`) and stop. Only proceed if the candidate is genuinely new or the skill has changed since the rejection.

**Step R3 — Write the candidate diff.**
Use `edit_file` to apply your refinement. Constraints:
- ≤30 added lines, ≤15 removed lines (diff stat). **Deleting or replacing** stale/superseded guidance is a valid refinement (not only adding) — spend the removed-line budget deliberately; a sharper, shorter skill is a good outcome.
- Touch only the `## Pitfalls` and `## Procedure` sections (or the skill's "what to do" body) — never the top-level `description:`, never any frontmatter field except the four bookkeeping fields established in step 3a: `score`, `uses`, `wins`, `last_used`. (`last_evolved` is also updated, to today's date.)

**Step R4 — A/B compare.**
For each test prompt, generate a 1-3 sentence summary of how each version (baseline, candidate) would handle the prompt — what tools the agent would call, what order, what the outcome would look like.

Two execution modes, in order of preference:

- **Preferred (sub-agent A/B):** if you have `sub_agent` available, dispatch two sub-agent calls in parallel:
  - Sub-agent A: read `/tmp/skill-evolve-baseline/<target>.SKILL.md` + the test prompt → output JSON `{"summary": "...", "tool_sequence": ["bash", "edit_file", ...]}`
  - Sub-agent B: same with the candidate file
  - Use the structured outputs to compare apples-to-apples.

- **Fallback (single-agent sequential):** if `sub_agent` isn't available or returned an error, read the baseline file, write a baseline summary; then read the candidate file, write a candidate summary. Be deliberate about not letting the candidate read bias the baseline read — write the baseline summary BEFORE looking at the candidate.

For each prompt, decide one of:
- `candidate-better`: candidate's procedure is more specific, addresses the prompt more directly
- `tie`: no meaningful difference
- `baseline-better`: regression — the refinement made things worse

**Step R5 — Decide.**
Commit the refinement only if:
- 0 prompts came out `baseline-better`, AND
- At least 1 prompt came out `candidate-better`

Otherwise: revert the edit (`cp /tmp/skill-evolve-baseline/<target>.SKILL.md skills/<target>/SKILL.md`) and write a `NO-OP` event with `eval-result: regression` (or `eval-result: tie`).

**Step R6 — Append eval summary to the `_journal.md` event.**
Add an `eval-summary:` field to the event:
```
- eval-summary: 2/2 prompts candidate-better, 0 regressions
```

Or for a NO-OP-after-eval:
```
- eval-summary: 1/2 baseline-better — refinement was a regression on prompt p2 ("..."). Reverted.
```

#### Create

Draft `skills/<new-name>/SKILL.md`:

```yaml
---
name: <new-name>
description: "[CANDIDATE — unreviewed] <pushy one-line trigger description, ≤200 chars total>"
tools: [bash, read_file, ...]
origin: yoyo
status: candidate
score: 0.5
uses: 0
wins: 0
last_used: null
last_evolved: <today>
parent_pattern_key: <kebab-case verb.object>
keywords: ["<distinctive substring 1>", "<distinctive substring 2>", "..."]   # ≥3 strings that, if found in a session's audit.jsonl, indicate this skill was used
---

# <Title>

## When to use
<concrete trigger conditions>

## Quick reference
<one-screen cheat sheet>

## Procedure
<numbered steps>

## Pitfalls
<things that have gone wrong before>

## Verification
<how the skill knows it succeeded>
```

The `[CANDIDATE — unreviewed]` prefix is critical — it tells the agent in future sessions to treat the skill as experimental, not as system-prompt-grade truth.

#### Retire

```bash
git mv skills/<name>/ skills_attic/<name>/
```

Soft delete. Recoverable. If yoyo invokes the skill's domain again within 3 cycles, you may revive it (move back, reset score to 0.5).

### 6. Validate

Before committing, run all of these. If any fails, write `refused` and exit:

```bash
# YAML frontmatter parses (use python3 since yq may not be installed):
python3 -c "
import sys, re
content = open('skills/<name>/SKILL.md').read()
m = re.match(r'---\n(.*?)\n---\n', content, re.DOTALL)
assert m, 'no frontmatter'
fm = m.group(1)
assert len(fm) <= 1900, f'frontmatter too long: {len(fm)}'
# crude parse
for line in fm.splitlines():
    if line.strip() and ':' not in line:
        sys.exit(f'invalid line: {line}')
"

# Description ≤ 200 chars:
desc=$(grep '^description:' skills/<name>/SKILL.md | head -1 | sed 's/^description: *//')
[ "${#desc}" -le 200 ] || { echo "description too long"; exit 1; }

# Body token estimate (~ word count, ceiling 5000):
body_words=$(awk '/^---$/{n++; next} n>=2' skills/<name>/SKILL.md | wc -w)
[ "$body_words" -le 5000 ] || { echo "body too long"; exit 1; }

# Build still works (the meta-skill itself shouldn't break the build, but defense in depth):
cargo build --release 2>&1 | tail -5
```

### 7. Append the event to `skills/_journal.md`

Get the next event number:

```bash
last=$(grep -oE 'evt-[0-9]+' skills/_journal.md | sort -u | tail -1)
n=$((${last#evt-} + 1))
evt=$(printf 'evt-%04d' $n)
```

Append (using `>>`, never overwrite):

```
## <ISO8601> <evt-NNNN> <type>
- skill: <name or "-">
- trigger: <one-line summary of evidence>
- diff: <+A -B (path)> or "n/a"
- validation: <pass | reason for refusal>
- score-delta: <old> → <new>
- parent-event: <evt-NNNN>
- expected: <observable signal | horizon | fallback>          # required for refine/create only; forbidden on all other types
- note: <optional one-line>
```

Where `<type>` is one of: `init`, `refine`, `create`, `retire`, `revive`, `meta-suggestion`, `refused`, `NO-OP`.

### What an `expected:` line must do (and must not be)

A good `expected:` line names **all three** of: a concrete observable signal, a horizon, and a fallback move.

**Concrete observables you may reference:**
- A skill's frontmatter `uses` / `wins` / `score` (e.g. "social.uses should grow by ≥3 over the next 5 sessions")
- A specific failure cluster's recurrence in audit-log sessions (e.g. "the gh-discussion-comment STUCK cluster should drop to 0 hits within 5 sessions")
- A trace pattern from step 3b (e.g. "the `git checkout` revert-after-edit pattern on social/SKILL.md should not recur in the next 3 sessions")
- A concrete tool-call sequence that should/should not appear in audit.jsonl

**Horizons:** "by next cycle", "within ~3 sessions", "within ~5 sessions", "within 7 days". Do not say "eventually" or omit the horizon.

**Fallbacks:** name the next move if the prediction does not hold. Examples: "...otherwise this is a sub-skill candidate, not a prose refine"; "...otherwise the `description:` is the wrong target — try refining the body instead"; "...otherwise retire the skill".

**Worked examples:**

For a `refine` event:
```
- expected: STUCK rate on the gh-discussion-comment cluster should drop to 0
  within the next ~5 evolve sessions; if not, the prose tweak was insufficient
  and a helper script (sub-skill) is the right next step
```

For a `create` event:
```
- expected: at least 2 sessions in the next 5 should match this skill's
  keywords[] AND have outcome.json.test_ok=true (i.e. wins ≥ 2 by next cycle);
  if uses < 2 by then, the description: is too narrow and needs widening, or
  the pattern was a one-off and the skill should retire
```

**Anti-patterns to refuse** (these do not satisfy HARD RULE #4 — NO-OP instead of writing them):

- "feels better"
- "will be more readable"
- "the prose is now clearer"
- "users will like it"
- "yoyo will use this skill more" *(no horizon, no signal)*
- "this should help" *(no horizon, no signal, no fallback)*

If your candidate `expected:` line reads like one of those, you do not have a theory of impact — the evidence does not justify a mutation this cycle. Write `NO-OP` and move on.

### 8. Commit

```bash
git add skills/ skills_attic/ memory/learnings.jsonl
git commit -m "skill-evolve: <type> <skill-name>" || true
```

The harness pushes (or doesn't, depending on its config). Do not push from inside this skill.

## Anti-bloat ceilings

Before any `create` action, verify all of these:

- Active skill count (any with `status: active` or `status: refined`) ≤ 25 *before* this create. If at the limit, you must `retire` first or write `NO-OP`.
- Total skill count in `skills/` (excluding any skill with `core: true`) ≤ 30.
- The new skill's frontmatter is ≤ 1900 chars.
- The new skill's description is ≤ 200 chars (including the `[CANDIDATE — unreviewed]` prefix).
- The new skill's body is ≤ 5000 words.
- No existing eligible skill has ≥3 keyword overlap with the new skill's `When to use` section. If so, refine that skill instead.

## Failure modes you must guard against

| Mode | What it looks like | What you do |
|---|---|---|
| **Skill thrashing** | Same skill refined twice within 3 sessions | Read `last_evolved` before refining; if < 3 sessions ago, pick a different target or NO-OP |
| **Saturation** | 3 consecutive NO-OP events in `_journal.md` | Add `evolution_saturation: true` to the third event; harness will extend cooldown |
| **Self-edit attempt** | Pattern points at `skill-evolve` itself | HARD RULE #2 — write `meta-suggestion` and stop |
| **Core-edit attempt** | Pattern points at one of the core 4 | HARD RULE #1 — write `learnings.jsonl` entry and stop |
| **Skill collision** | New skill's triggers overlap an existing skill | Refine the existing skill instead |
| **Identity drift** | Pattern would contradict IDENTITY.md / PERSONALITY.md | Refuse; write a `learnings.jsonl` entry noting the contradiction |

## What good looks like

A healthy `skills/_journal.md` after 30 days:

- 4–10 events total (you don't run every session, and most cycles are NO-OP)
- Mix of refine (~50%), create (~10%), retire (~10%), NO-OP (~30%)
- Zero `refused: self-edit` or `refused: core-edit` events (your hard rules are holding)
- Per-skill EMA scores trending up or stable (not down)
- `pattern_key` recurrence dispersal *falling* over time — yoyo is internalizing patterns, not re-discovering them

If you see thrashing, score decay, or many refusals, write a `meta-suggestion` and let the human creator tighten the loop.
