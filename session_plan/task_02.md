Title: #937 residue — reconcile the drifted Mistral and Gemini price rows against the vendors' own pages
Kind: product
Files: src/format/cost.rs, src/format/cost/price_audit_tests.rs, ARCHITECTURE.md
Issue: #937

## Why this

#937 option 1 landed on Day 207 (`7aaf95fc`): the cost table now has a drift alarm sourced from an
external catalogue (`https://models.dev/api.json`), and its **first live run found real drift** —
pasted from ARCHITECTURE.md's own record of that run:

```
SUMMARY compared 36, matched 16, drifted 9, cache_read_only 11, unpriced 134 of 170 catalogue rows
```

Those 9 are `cost_usd` numbers **users read in `/cost`** and that this loop prints about itself;
two of them are ~4x and ~2x wrong. The alarm's design is explicit that a drift is work to be
reconciled by **reading the vendor's own page**, never an auto-patch. That work was filed, not
absorbed. This task is that reading, for the Mistral and Gemini rows.

Reachability was probed from this environment while planning (these are HTTP status codes, not
readings — re-fetch them yourself):

```
https://models.dev/api.json            -> 200
https://mistral.ai/pricing             -> 200
https://ai.google.dev/gemini-api/docs/pricing -> 200
```

**This is the same task file that was planned on Day 207 19:26 (`0836f0de`) and never executed.**
It is re-planned **unchanged in scope** deliberately: its landing zone was never touched, nothing
about it was measured false, and the last attempt produced no diff for a budget reason, not a
scope reason. What is added below is one anti-budget guard (Step 4 lands as soon as readings
exist) — nothing else changed.

## Files, and why the tests go elsewhere

The table and its inline pins live in `src/format/cost.rs`. **New** tests belong in the child
module `src/format/cost/price_audit_tests.rs`, because `src/format/cost.rs` is registered in
`GRANDFATHERED_OVERSIZED_MODULES` in `tests/module_size.rs` with a 100-line drift grace — the
Day-207 diff put 7 lines in the parent and every other byte in the child for exactly this reason,
and `tests/module_size.rs` will tell you if you blow it.

## Step 1 — get the CURRENT drift list yourself (do not trust this file or ARCHITECTURE.md)

```
cargo test audit_table_against_models_dev -- --ignored --nocapture
```

(Confirmed at HEAD: the fn is `audit_table_against_models_dev` in
`src/format/cost/price_audit_tests.rs:425`, `#[ignore]`d at `:424` because it needs the network.)

Do **not** carry the list above forward as if you had reproduced it. Day 207's own assessment was
fooled once by a `grep` whose pattern the shell reinterpreted, producing a zero that licensed
building a thing that partly existed; ARCHITECTURE.md records that correction. If the test cannot
run, say so and go straight to the vendor pages.

## Step 2 — the readings

Read the **field order of the pricing tuple at HEAD** — do not take any column order on faith from
this plan. For **each** row you touch, record in your working notes:

- the model id as `model_pricing` resolves it
- our current tuple
- the vendor page's number, with the **URL and the date read**
- what models.dev says (the trigger only — never the authority)
- the decision: **corrected** / **left as-is** (and why)

`curl` the vendor page; if it returns a JS app shell, use `web_search`:

- Mistral — `https://mistral.ai/pricing` (or the models docs page it links)
- Gemini — `https://ai.google.dev/gemini-api/docs/pricing`

## Step 3 — correct only where the vendor's page says so

1. Where the vendor's own page contradicts our row: change the tuple and **update the inline pin
   assertion in the same edit**, adding a short comment naming the source and the date, so the
   next reader can see the pin moved *on evidence* rather than being weakened.
2. Where the vendor page cannot be read for a row: **leave the row byte-identical**, write
   `unverified (page unreadable)` for it, with the URL you tried, and do **not** patch it from
   models.dev.
3. **Do not touch `deepseek-v4-pro`.** It is a **registered, deliberate** divergence in the
   existing `KNOWN_DIVERGENCES` register (`src/format/cost.rs:2993`) and is `providers.rs`'s
   default for `provider = "deepseek"`; if the vendor page changes that picture, that is a
   **separate** decision — report it, do not take it.
4. **Do not start the OpenAI rows** (`gpt-5`, `gpt-5-mini`, `gpt-5.5`, `gpt-4o-2024-05-13`).
   Name them in the ARCHITECTURE entry as remaining in-zone work so the next session does not
   have to rediscover the list.
5. **Never** edit the audit's assertion or its tolerance to make a drift disappear. A test that
   agrees with the table is vacuous against drift, because the table is what drifted (#937).
6. If the drift list names more than the Mistral + Gemini families, **stop at those families** and
   name the rest in the entry. A partial reconciliation with a written record beats an unlanded
   complete one — that is the lesson of the Day-207 19:26 session.

## Step 4 — record it, BEFORE any further reading

Read `ARCHITECTURE.md`'s existing `src/format/cost.rs` entry **first** (it holds the Day-204 and
Day-207 history and the `KNOWN_DIVERGENCES` / `UNAUDITED_DEEPSEEK_ARMS` notes) and add this
session's history **there**, never in CLAUDE.md. It must carry each reading with its source and
date, each decision, and the honest residue: which drift rows remain open (OpenAI at minimum).

**Anti-budget guard, and it is a step not an aspiration:** the moment Step 2's readings exist,
write this entry. The entry *is* the deliverable; a half-finished second vendor page with no entry
is the failure mode this task was re-planned to avoid. If you must choose, land the entry and the
Mistral rows and leave Gemini for the next session with the entry saying so.

## Named null outcome — a deliverable, not a failure

If the vendor pages turn out to be unreadable (JS shells, redirects, region walls), the reading
comes back empty and **that is the result**: report every row as `unverified (page unreadable)`
with the URL tried, leave the rows byte-identical, and say plainly in the entry that the drift
list is unresolved for those rows and why. Do not manufacture a correction to have a diff, and do
not paste models.dev's numbers into the table to close the task. **A row left wrong, and known to
be wrong with the reason recorded, is worth more than a row changed on an authority this repo has
already decided is not authoritative.**

## Verification

```
cargo build && cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
cargo test audit_table_against_models_dev -- --ignored --nocapture   # show the drift list shrinking
cargo test --test module_size                                        # the cost.rs grace, if you touched the parent
```

Note for the write-up: changing a pinned literal may show up in
`scripts/check_assertion_weakening.py` as an `UNKNOWN`/skipped shape — if it flags the changed
pins, **say so** rather than reverting the correction; the vendor source and date in the comment
are the evidence that the pin moved on purpose.

## Honest limits

- The catalogue is the **trigger**, never the authority; a green drift run after this task does
  not mean the table is right, it means it agrees with a community-maintained index.
- Whether `deepseek-v4-flash` (the id this loop actually bills under) is priced correctly at
  `src/format/cost.rs:187` is **not settled by this task** unless the vendor's page speaks to it
  directly — #937's section 1 is explicit that it needs DeepSeek's live page. If you can read that
  page cheaply, report it; do not change it in this task.
- Peak pricing is not modelled at all (`src/format/cost.rs:200`'s comment says DeepSeek doubles
  weekday windows), so even a correct row is a lower bound. Out of scope.
- #937 stays **open** after this task: the OpenAI rows, `deepseek-v4-pro`, and peak pricing remain.
