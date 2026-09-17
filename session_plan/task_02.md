Title: #927 — trigger 3 has no memo of "remedy already delivered": give it an already-asked precondition and a stated outcome
Kind: evolve
Files: skills/social/SKILL.md, tests/integration.rs
Issue: #927

## What the observation window (which the creator asked for) actually found

@yuanhao filed #927 with the reading *"the session ends at idempotency verification and never
evaluates the five proactive triggers"*, and asked me to **observe before editing**. I did: three
social runs (09-16 21:21 `35151925665`, 09-17 00:35 `35167144541`, 09-17 07:43 `35195997013`), all
`Rate limit: clear`, all five triggers walked every time. So the filed mechanism is **false**, and I
said so in the issue rather than quietly editing anyway.

What the traces did show is a **different, real defect**, and it is the one this task fixes:

> 07:43 found #930 open with no human replies → trigger 3 matched → then caught itself: *"#931 already
> asks exactly that question. So the action is done. … Trigger 3 matches, and its action is already
> complete. So no post."*

**Trigger 3's test is `open agent-help-wanted issue without human replies`. It has no memo of "have I
already asked this, and did anyone answer."** So the same unanswered issue keeps satisfying it
forever, and the only thing between that and a repost is the session's own memory of last week's
posts. My comment on #927 committed to this as *"a small, test-shaped change and wants its own diff"*.

The second half is the one the creator named as latent regardless of cause: **an evaluated silence and
a walked-off silence currently read identically in the trace.** The three runs above are only
distinguishable because those sessions happened to narrate their reasoning. That is luck, not a
mechanism.

## What to change

### 1. `skills/social/SKILL.md` — trigger 3's definition (`## Proactive Posting`, the line currently reading `3. **Help wanted without replies** — open `agent-help-wanted` issue without human replies → start a discussion asking the community for input`)

Add an **already-asked precondition** in the same line or directly beneath it, in the file's existing
terse style. It must require, before trigger 3 may fire:

- that the issue is open **and** has no human reply (unchanged), **and**
- that no discussion of mine already carries the remedy for it — checkable with the prompt's own
  recent-discussion list and/or the issue's own comments for a `yoyo-evolve` comment linking a
  discussion. A remedy already posted means the trigger is **already delivered**, which is a
  *successful* state, not a firing one.

State the three outcomes in words, because this is the half that makes the trace legal: **fired** /
**already-delivered (naming the discussion that delivered it)** / **declined (with the reason)**.
Keep it to a few lines — this is a skill a session reads under a token budget, and the file already
argues for the cheapest-first ordering.

Also add **one clause to `## Early Exit Rule`**: a trigger whose action is already complete is
recorded as `already-delivered` naming the prior post — it is not silently walked off. This is the
same shape the Day-200 rework established for the trigger ledger, applied to the completed case.

Do **not** restructure the section, do **not** touch the frontmatter (`origin: yoyo`, `score`,
`uses`, `last_evolved`, `parent_pattern_key` stay byte-identical), and do **not** reorder the five
triggers — the ordering is the Day-200 fix and is guarded.

### 2. `tests/integration.rs` — the guard

There is already the right precedent in the file:
`social_skill_early_exit_rule_requires_trigger_ledger_before_exit` (~:2247) reads
`skills/social/SKILL.md`, finds a `## ` heading, **slices to the next `## ` heading** so a `contains`
cannot be satisfied by text elsewhere, and asserts a minimum section length first so a renamed or
emptied section reddens as *missing* rather than passing vacuously.

Add a **sibling test in the same idiom** for the new clause, scoped to the `## Proactive Posting`
section (slice to the next `## `, anti-vacuous length assertion first). Assert:

- the trigger-3 line declares the already-asked precondition (its distinctive wording), and
- the section still contains **all five** numbered triggers — so a rewrite that drops trigger 5
  cannot pass by being merely shorter, and
- the near-miss that matters: triggers 1, 2, 4 and 5 keep their current text (a `contains` on the
  present strings), so the guard fires on the change it is for and not on an unrelated rewrite.

Do **not** edit or weaken `social_skill_early_exit_rule_requires_trigger_ledger_before_exit` or the
skills-loading tests at ~:2217 — they are the regression evidence that this is an addition.

## Constraints

- Two files, one subject. No `src/` changes.
- Markdown-only edits to the skill: minimal diff, no reflowing of untouched paragraphs.
- This skill is **`origin: yoyo`** (verified this session) — editable. Do not confuse it with
  `skills/skill-evolve/SKILL.md`, which is `origin: creator` + `core: true` and is off limits.

## Verify

```
cargo build && cargo test                       # integration tests read every skills/*/SKILL.md
cargo test social_skill                         # the new guard and its neighbour
cargo clippy --all-targets -- -D warnings && cargo fmt -- --check
```

Positive control, run **serially as one atomic mutate→run→restore**: revert the new trigger-3
wording in the skill, watch the **new** test redden by name, restore, watch it pass. If the control
reddens a pre-existing test too, the claim is wider than this task — say so rather than claiming it.

## Definition of done

- Trigger 3 cannot fire on an issue whose remedy is already posted, and that state has a name
  (`already-delivered`) a later trace can read.
- A scoped, anti-vacuous Rust guard pins the precondition and the five-trigger list, green.
- `cargo build && cargo test` green; the pre-existing early-exit guard untouched and green.
- Note: no comment on #927 from this task — Phase C posts `issue_responses.md`. The observation
  numbers above are already in the thread; do not restate them.
