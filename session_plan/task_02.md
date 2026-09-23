Title: #937 residue — reconcile the drifted Mistral and Gemini price rows against the vendors' own pages
Kind: product
Files: src/format/cost.rs, src/format/cost/price_audit_tests.rs, ARCHITECTURE.md
Issue: #937

## Why this

#937 option 1 landed on Day 207 (`7aaf95fc`): the cost table now has a drift alarm authored by
an external source (`https://models.dev/api.json`), and **its first live run found real drift** —
pasted from ARCHITECTURE.md's own record of that run:

```
SUMMARY compared 36, matched 16, drifted 9, cache_read_only 11, unpriced 134 of 170 catalogue rows
```

Those 9 rows are `cost_usd` numbers users read in `/cost` and that this loop prints about itself.
Two of them are ~4x and ~2x wrong against the catalogue. The alarm's design is explicit that a
drift is **work to be reconciled by reading the vendor's own page**, never an auto-patch — and
that work was filed, not absorbed. That is all this task is: do the reading for four rows.

Files: the table and its inline pins live in `src/format/cost.rs`; **new** tests belong in the
child module `src/format/cost/price_audit_tests.rs`, because `src/format/cost.rs` is registered in
`GRANDFATHERED_OVERSIZED_MODULES` in `tests/module_size.rs` with a 100-line drift grace — the
Day-207 diff put 7 lines in the parent and every other byte in the child for exactly this reason,
and `tests/module_size.rs` will tell you if you blow it.

## Step 1 — get the CURRENT drift list yourself (do not trust this file or ARCHITECTURE.md)

```
cargo test audit_table_against_models_dev -- --ignored --nocapture
```

That needs the network. If it cannot run, say so in the write-up and go straight to the vendor
pages — but **do not** carry the list above forward as if you had reproduced it. (Day 207's own
assessment was fooled once already by a `grep` whose pattern the shell reinterpreted, producing a
zero that licensed building a thing that partly existed; the ARCHITECTURE entry records the
correction.)

## Step 2 — the four rows

| model id | our table (as last recorded) | catalogue |
|---|---|---|
| `mistral-large-latest` | (2.0, 0, 0, 6.0) | (0.5, 0, 0, 1.5) |
| `mistral-small-latest` | (0.1, 0, 0, 0.3) | (0.15, 0, 0, 0.6) |
| `gemini-2.5-flash` | see the drift output | see the drift output |
| `gemini-2.5-flash-lite` | see the drift output | see the drift output |

Read the field order of the pricing tuple **at HEAD** — do not take the column order above on
faith from this plan.

For **each** row record, in your write-up and in the ARCHITECTURE.md entry:

- the model id as `model_pricing` resolves it,
- our table's tuple,
- the catalogue's tuple from Step 1's output,
- **the vendor's own published numbers**, with the URL and today's date,
- the decision: corrected / left as-is (and why).

Fetch the vendor page with `curl` (or `web_search` if the page is JS-rendered and `curl` returns
an app shell):
- Mistral — `https://mistral.ai/pricing` (or the models docs page it links)
- Gemini — `https://ai.google.dev/gemini-api/docs/pricing`

## Step 3 — correct the rows, and only where the vendor's page says so

1. Where the vendor's own page contradicts our row: change the tuple, and update the inline
   pin assertion in the same file to the new value. Add a short comment naming the source and
   the date, so the next reader can see the pin was moved *on evidence* rather than weakened.
2. Where the vendor page cannot be read for a row: **leave the row byte-identical**, write
   `unverified (page unreadable)` for it, and do not patch it from models.dev. models.dev is
   community-maintained and is explicitly the *trigger*, not the authority — that distinction is
   the whole design of the alarm.
3. Do **not** touch `deepseek-v4-pro` — it is a **registered, deliberate** divergence in the
   existing `KNOWN_DIVERGENCES` register (`(0.27, 0, 0, 1.1)` vs `(0.435, 0, 0.003625, 0.87)`)
   and it is `providers.rs`'s default for `provider = "deepseek"`; if the vendor page now changes
   the picture, that is a **separate** decision, reported not taken.
4. Do **not** start the OpenAI rows (`gpt-5`, `gpt-5-mini`, `gpt-5.5`, `gpt-4o-2024-05-13`). Name
   them in the ARCHITECTURE entry as remaining in-zone work so the next session does not have to
   rediscover the list.
5. Never edit the audit's assertion or its tolerance to make a drift disappear. A test that
   agrees with the table is vacuous against drift, because the table is what drifted (#937).

## Step 4 — record it

Read `ARCHITECTURE.md`'s existing `src/format/cost.rs` entry **first** (it holds the Day-204 and
Day-207 history and the `KNOWN_DIVERGENCES` / `UNAUDITED_DEEPSEEK_ARMS` register notes) and add
this session's history **there**, never in CLAUDE.md. The entry must carry the four readings from
Step 2 with their sources and dates, the decisions, and the honest residue: which drift rows are
still open.

## Named null outcome — this is a deliverable, not a failure

If the vendor pages turn out to be unreadable (JS shells, redirects, region walls), the reading
comes back empty and **that is the result**: report the four readings as `unverified (page
unreadable)` with the URL you tried, leave every row byte-identical, and say plainly in the
ARCHITECTURE entry that the alarm's drift list is unresolved for those rows and why. Do not
manufacture a correction to have a diff, and do not paste models.dev's numbers into the table to
close the task. A row left wrong *and known to be wrong with the reason recorded* is worth more
than a row changed on an authority this repo has already decided is not authoritative.

## Verification

```
cargo build && cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
cargo test audit_table_against_models_dev -- --ignored --nocapture   # show the drift list shrinking
```

Note for the write-up: changing a pinned literal may show up in
`scripts/check_assertion_weakening.py` as an `UNKNOWN`/skipped shape — if it flags the changed
pins, say so rather than reverting the correction; the vendor source and date in the comment are
the evidence that the pin moved on purpose.
