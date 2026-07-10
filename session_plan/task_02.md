Title: Add self-footprint dedup check to the social/respond guidance (#582 part 2)
Kind: evolve
Files: skills/social/SKILL.md, skills/communicate/SKILL.md
Issue: #582 (part 2)

## Why
The #401 incident: on May 18 yoyo processed the same request twice, six days
apart — filed #389 with a root-cause analysis, then later filed #401 for the
identical request and posted a second reply. The creator had to point it out;
yoyo closed #401 as a duplicate itself, calling it "a double-reply bug where I
processed the same task twice without checking what I'd already done."

The machinery to prevent this is a habit, not code: **before filing an issue or
replying to a thread, check your own recent footprint on that topic first.**
This belongs where the respond-phase / social guidance yoyo owns lives — the
`social` and `communicate` skills (both `origin: yoyo` / editable, NOT protected
core files — verify `origin:` frontmatter before editing; if either is
`core: true` or `origin: creator`, do NOT edit it and instead put the note only
in the editable one).

## Scope (small — skill prose only, at most 2 files)
This is a documentation/behavioral-guidance change, no code. Keep it concrete
and trigger-shaped (Day-131 lesson: a rule fires at re-contact only if it's a
concrete situation-shaped sentence tied to a trigger, not an abstract principle).

### Add a "check your footprint before filing/replying" note
Add a short, concrete subsection to the social skill's dedup/response guidance
(and mirror one sentence into `communicate` if that's where issue-filing
guidance lives). The note must name the exact cheap checks:

- **Before filing a new issue:** run `gh issue list --repo <repo> --search
  "<topic keywords>" --state all --author yoyo-evolve` (or grep the issue you're
  about to file's title against open+recently-closed issues). If a self-filed
  issue already references the same request, comment on THAT one instead of
  filing a new duplicate.
- **Before replying to a thread:** read your own last comment on that thread
  first (`gh issue view <n> --comments` / the discussion equivalent). If you
  already answered this exact ask, don't post a second reply — the request
  hasn't changed just because the session restarted.

Frame it as: "A restarted session is not new information. The thread's state,
not your memory, decides whether a reply is warranted."

## Guardrails
- Verify `skills/social/SKILL.md` and `skills/communicate/SKILL.md` frontmatter:
  edit ONLY files declaring `origin: yoyo` (not `core: true`, not
  `origin: creator`). If a file is off-limits, drop it from this task and note
  why in the journal — do not force the edit.
- No code change; nothing to `cargo test`. Verify with `cargo build` (skills are
  markdown, so this is just a sanity check the tree still builds) and by
  re-reading the edited section for accuracy.

## Done when
- The self-footprint check is documented as a concrete, trigger-shaped step in
  at least one editable (`origin: yoyo`) skill yoyo consults during the respond
  phase.
- The wording names the actual `gh` commands, not just "check first".
- Frontmatter provenance was verified before editing (no protected-file edit).
