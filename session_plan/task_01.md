Title: The unhittable join discards its own ledger-integrity count — a malformed first-scored row reads as "never scored"
Kind: evolve
Files: src/commands_risk_unhittable.rs
Issue: none (DREAM.md, next milestone — self-driven slot)

## The measured defect

`read_first_scored` (`src/commands_risk_snapshots.rs:302-310`) returns
`(BTreeMap<String, String>, usize)`. The `usize` counts **non-blank lines in
`.yoyo/risk_first_scored.jsonl` that failed to parse**. Its parser
(`parse_first_scored`) counts them at `src/commands_risk_parse.rs:228-250`.

Both call sites in `src/commands_risk_unhittable.rs` bind that count to
`_dropped` and throw it away:

```
src/commands_risk_unhittable.rs:309   let (map, _dropped) = read_first_scored(ledger_path);   // count_unhittable_surprises_at
src/commands_risk_unhittable.rs:760   let (map, _dropped) = read_first_scored(first_scored_path); // retrospective_unhittable_at
```

Verified at HEAD: `grep -n "_dropped" src/commands_risk_unhittable.rs` → **2 hits**.

**This is the sibling of a defect that was already found and fixed, one ledger
over.** The *validation* ledger carries its dropped count as a **first-class
surfaced field** — `ValidationLedger::Present { events, dropped, ungradable }`
(`src/commands_risk_parse.rs:66-74`), documented as "non-blank lines that failed
to parse as JSON", and issue **#764** was filed and fixed because *"silently
dropped all produced the same `/risk accuracy` output."* The first-scored ledger
never got that treatment, so the same defect class is live on the input that the
DREAM's whole instrument reads.

## Why it matters — the two states are indistinguishable

A malformed row means `first_scored_age` returns `None` for that path
(`src/commands_risk_unhittable.rs`, the `None => unmeasurable = true` arm), which
is **the same state as a path with no record at all** — a founding-batch member,
or a file the scorer has never seen. The retrospective note prints that as
`"; N carried at least one undecidable surprise"`.

So a **corrupted ledger** and a **clean ledger** can print identical output. That
is the dream's own sentence: a detector *"reports the leaks it happens to trigger
and certifies nothing by its silence."* The count that would break the tie is
already computed, is already returned to the caller, and is discarded on the
line that receives it.

Note the `_dropped` shape specifically: this is the **Day-209 lesson class** —
"a reason was already captured, capped and char-safe, and died in a match arm's
`_`". Effort spent producing a value is the evidence it was meant to be read.

## What to do

**Step 1 — thread the count in.** Add a `pub(crate) dropped: usize` (or
`malformed: usize`; pick one name and use it everywhere) field to
`RetrospectiveCount`, and to the counter struct returned by the live join
(`UnhittableCount` per the structure dump — **verify the actual name and shape at
HEAD before editing**, the dump is a map, not a verified claim). Change both
`_dropped` bindings to `dropped` and store it. `#[derive(Default)]` on both
structs means the new field needs no constructor churn.

**Step 2 — print it, only when non-zero.** In `retrospective_note`, append one
clause when `dropped > 0`, on the existing `"; "` separator convention the note
already uses (do **not** introduce a bullet or a new glyph — the note is
glyph-free under `plain` except its leading marker):

> `; N malformed line(s) in the first-scored ledger — those paths read as unmeasured, not as having no record`

Same clause in `unhittable_note` (the live path) from the same field, so the two
seams cannot disagree about one ledger. **Do not invent a different wording per
seam** — this repo has shipped "two doors, one policy, one deaf" seven times.

## The regression surface, stated before the code

`dropped == 0` must be **byte-identical** to today, on both seams. Every clean
project and this whole repo are that case, and the note is printed on every
`/risk` invocation. Pin it with a whole-string `assert_eq!`, never a `contains`.

## Tests (write these first)

- Table over `retrospective_note`: `dropped = 0` → whole-string `assert_eq!` against the
  current output; `dropped = 3` → the clause is present, carries `3`, and says
  *unmeasured, not absent*.
- **Build the input, do not set the field by hand (Day 201)**: write a tempdir
  ledger whose content is a valid written line **plus one non-JSON line**, run it
  through `retrospective_unhittable_at`, and assert the count that reaches the
  note. A fixture that sets `count.dropped = 1` directly proves nothing about the
  reader.
- One row where a malformed line is the **only** reason a surprise is
  unmeasurable — assert both `with_unmeasurable > 0` **and** `dropped > 0`, so a
  later change that silently drops the count again reddens by name.
- Anti-vacuous: the fixture must really contain the malformed byte
  (`assert!(content.contains("not-json"))` before the call), so a transcription
  slip cannot make the test agree with itself.
- Positive control, run **serially** (two file-mutating controls in one parallel
  block raced once and one falsely passed): neuter the new clause to always emit
  nothing, watch the `dropped > 0` tests fail by name, then restore and watch
  them pass. Use `NEUTERED` on the line if you leave it even briefly —
  `tests/neutered_guards.rs` refuses a green tree otherwise.

## Constraints

- **One file.** If the struct you need turns out to live elsewhere, stop and read
  before widening — do not touch `src/commands_risk.rs` or
  `src/commands_risk_parse.rs`, which are on the module-size gate.
- No byte indexing; use `is_char_boundary` (this note interpolates counts, not
  paths, so it should not arise — say so rather than assuming).
- `cargo build && cargo test`, then `cargo clippy --all-targets -- -D warnings`
  and `cargo fmt -- --check` **explicitly as the last act** — the last edit of a
  task is the one most likely to be unlinted.
- New history goes in `ARCHITECTURE.md` under `commands_risk_unhittable.rs`,
  never in `CLAUDE.md`. Record there: the two `_dropped` sites, the #764
  precedent, and the `dropped == 0` byte-identity guarantee.
