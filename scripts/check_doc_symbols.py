#!/usr/bin/env python3
"""Census my own doc-vs-code drift: backticked Rust symbols in CLAUDE.md vs `src/`.

WHY THIS EXISTS
---------------
Eleven deterministic gates live in `tests/`. Every one of them answers *is the code
consistent with itself*. **Not one answers *is the prose consistent with the code*.**
`cargo test` cannot fail for a false sentence, and CLAUDE.md is re-injected as
authoritative context every single session, so a stale sentence there is read as fact by
the next agent before it reads any code. Three of the last four sessions each spent part
of a diff repairing a sentence that had gone stale.

The class is named and measured OUTSIDE this repo, which is what makes it worth a slot
rather than a private itch. arXiv 2606.09090 ("Context Rot in AI-Assisted Software
Development") defines context rot as divergence between what an AI config file
(CLAUDE.md, AGENTS.md, .cursorrules, .github/copilot-instructions.md) claims and what
holds, and its diagnosis is my own journal almost verbatim: *code changes are
continuously exercised by compilers, tests and CI, so drift is caught quickly;
documentation changes are not.* Running DOCER (an existing README consistency checker)
against AI config files unmodified, they found stale code-element references in **23.0%
of 356 repositories**. DOCER's cheapest class is REFERENTIAL ROT and its rule is
deterministic: extract identifiers with regular expressions, check their presence in the
source. No model, no network, no `cargo`. That is a thing I can actually build.

THIS IS A READER, NOT A GATE
----------------------------
Deliberately. Nothing here fails, reverts or files an issue. The reason is a number
nobody has: how many backticked identifiers in CLAUDE.md no longer exist in `src/`? If
the answer is 5, a twelfth gate is trivial. If it is 200, a gate is impossible without a
register larger than the gate. Building the gate first and discovering the number second
is how a verified narrow change becomes an unverified wide one. This is the #875
discipline (a census corrected a 29% denominator error before anything was built) and the
#870 discipline (a readability census answered "is widening worth it?" before widening).

There is also a known over-firing hazard that would make a naive gate useless: this repo
DELIBERATELY preserves superseded names. CLAUDE.md records `truncate_diff_line` as a dead
symbol on purpose, records `REGISTER_TEST_FILES` as deleted, and carries dozens of
"superseded claim, recorded rather than erased" paragraphs. A gate firing on every one of
those is the churn that trains a reader to paste past a gate without reading it -- the
exact failure `tests/doc_version_claims.rs` refuses when it declines a blanket wording
scan.

SHAPE
-----
Pure decision functions (`extract_backticked`, `classify_token`, `symbol_present`,
`classify_symbol`) with all I/O at one call site, a real argparse surface so `--help`
works (`scripts/measure_abstentions.py` shipped without one and raised
`FileNotFoundError: '--help'` for a whole day -- not repeating that), and `--test`
self-tests that exit non-zero on failure. It shells NOTHING: no `cargo` (#832 -- a nested
cargo rebuilds over the shared `target/debug/yoyo` uplift path and reddened CI for three
sessions), no `gh`, no network, no `git`. Reading files is all it does.

THREE TOKEN CLASSES, NONE FOLDED INTO ANOTHER
---------------------------------------------
`TOKEN_SYMBOL` / `TOKEN_NOT_A_SYMBOL` / `TOKEN_AMBIGUOUS`. "Out of scope" is NOT the same
fact as "absent", and a bare lowercase English word that is also a legal identifier
(`release`, `search`, `todo`) is a third fact again -- `complaint_signals` in skill-evolve
has already been measured firing on the English word "release" 14 times (#858), and that
is the same trap one instrument over.

SELF-CONTAMINATION, ASKED FORWARD RATHER THAN AUDITED BACKWARD
--------------------------------------------------------------
`scripts/measure_abstentions.py` exists because a meter I wrote matched my own prose
about the meter, and it now stands at 16 measured instances of that contamination in the
wild. Asked forward here: THIS FILE's own source, and the CLAUDE.md paragraph describing
it, both contain backticked identifiers. The anchor is the path population -- the scanner
reads CLAUDE.md as the doc and `src/**/*.rs` as the code, so `scripts/*.py` is in NEITHER
population however much identifier vocabulary it carries. That anchor is pinned by a
self-test rather than left to reading.
"""

from __future__ import annotations

import argparse
import datetime
import os
import re
import sys
from collections import Counter

# --------------------------------------------------------------------------------------
# Token classes. Three, and none folds into another.
# --------------------------------------------------------------------------------------

TOKEN_SYMBOL = "TOKEN_SYMBOL"
TOKEN_NOT_A_SYMBOL = "TOKEN_NOT_A_SYMBOL"
TOKEN_AMBIGUOUS = "TOKEN_AMBIGUOUS"

# --------------------------------------------------------------------------------------
# Symbol verdicts. `SYM_ABSENT_MARKED_SUPERSEDED` is a REPORT, never a FILTER: those
# identifiers stay in the absent count AND get their own sub-count, because whether a
# superseded marker licenses an absence is exactly the design question the NEXT session
# has to answer, and it must not be pre-decided here.
# --------------------------------------------------------------------------------------

SYM_PRESENT = "SYM_PRESENT"
SYM_ABSENT = "SYM_ABSENT"
SYM_ABSENT_MARKED_SUPERSEDED = "SYM_ABSENT_MARKED_SUPERSEDED"

# Markers this repo uses when it deliberately preserves a name that no longer exists.
# Matched case-insensitively against a window around the token, never the whole line --
# see MARKER_WINDOW_CHARS for why that distinction is load-bearing here specifically.
SUPERSEDED_MARKERS = (
    "superseded",
    "dead symbol",
    "no longer",
    "renamed",
    "was deleted",
    "retired",
)

# A JUDGMENT THRESHOLD, NOT A MEASUREMENT. CLAUDE.md's bullets are single lines of up to
# ~10,000 characters, so "does the LINE carry a superseded marker" is very nearly "does
# this bullet mention the word superseded anywhere", which would fire on almost
# everything and make the sub-count meaningless. This approximates "the sentence around
# it" by a character window either side of the token's own column. Nothing measured says
# 300 is right; it is wide enough to catch a marker in the same clause and narrow enough
# that a marker 4,000 characters away does not vouch for an unrelated name.
MARKER_WINDOW_CHARS = 300

# Display caps. CLAUDE.md has single lines over 10,000 characters -- that is exactly how
# tests/doc_version_claims.rs once reported `claimed: 0.16.6 --> **Day 180 ...` and burned
# 19 fix attempts. Every cut is marked IN BAND: a silent elision is the bug.
SNIPPET_CHARS = 110
DEFAULT_MAX_ROWS = 30

# A single-backticked span, confined to one line by construction. Scanning line by line
# is deliberate: a stray unbalanced backtick can then swallow at most the rest of ONE
# line rather than the rest of the file.
BACKTICK_RE = re.compile(r"`([^`\n]+)`")

# A bare Rust identifier: the thing left after stripping call parens and any `::` path.
IDENT_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")

# Word-ish tokens in Rust source, for the presence index.
SRC_WORD_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")

# Anything in here means "this backticked span is not an identifier we could look up":
# shell fragments, attributes, versions, paths, macros, operators, prose punctuation.
# `.` covers versions (0.16.6), filenames (mod.rs) and method fragments (.contains().
# `!` covers macros, which live in std far more often than in src/ and would otherwise
# manufacture a wall of false absences.
DISQUALIFYING_CHARS = set(" \t/.!$>|&*\"';\\{}[]=+%?,@#<~^")


def is_indexed_source(path: str) -> bool:
    """Is this file part of the CODE population?

    The anti-self-contamination anchor. `src/**/*.rs` only -- so this script, and every
    other `scripts/*.py`, is structurally outside the population no matter how many
    backticked identifiers it carries. Pinned by a self-test.
    """
    norm = path.replace(os.sep, "/")
    return norm.endswith(".rs") and (norm == "src" or "src/" in norm or norm.startswith("src/"))


def extract_backticked(text: str) -> list[tuple[int, int, str]]:
    """Every single-backtick span in the doc, as (1-based line, 0-based column, token).

    Fenced code blocks are skipped: their contents are example code and shell
    transcripts, not claims about `src/`. Fence state is tracked line by line so an
    unclosed fence cannot silently consume the remainder of the document -- it will
    consume the rest, which is the honest reading of an unclosed fence, but it can never
    do so by accident inside a single line.
    """
    out: list[tuple[int, int, str]] = []
    in_fence = False
    for lineno, line in enumerate(text.splitlines(), start=1):
        stripped = line.lstrip()
        if stripped.startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        for m in BACKTICK_RE.finditer(line):
            out.append((lineno, m.start(1), m.group(1)))
    return out


def normalize_symbol(token: str) -> str:
    """Strip call parens and any `::` path down to the final segment."""
    core = token.strip()
    if core.endswith("()"):
        core = core[:-2]
    elif core.endswith("("):
        core = core[:-1]
    if "::" in core:
        core = core.rsplit("::", 1)[-1]
    return core


def classify_token(token: str) -> str:
    """TOKEN_SYMBOL / TOKEN_NOT_A_SYMBOL / TOKEN_AMBIGUOUS.

    The load-bearing discriminator. Three values because "out of scope" and "absent" are
    different facts, and a bare English word that is also a legal identifier is a third
    fact again.

    The ambiguity rule is a SHAPE rule, not a wordlist, and that is deliberate: a wordlist
    goes stale and needs its own authority-reading guard (the `MECHANICAL_SUBJECTS` /
    `GLOBAL_SETTERS` shape). A bare all-lowercase word with no underscore, no `::` path
    and no call parens carries NO morphological evidence that it is an identifier rather
    than English -- `release`, `search`, `todo`, `verbatim` are indistinguishable from
    prose by construction. Add an underscore, an uppercase transition, a path or a pair of
    parens and the evidence exists.
    """
    if token is None:
        return TOKEN_NOT_A_SYMBOL
    raw = token.strip()
    if not raw:
        return TOKEN_NOT_A_SYMBOL
    if raw.startswith("-"):
        # `--restricted`, `-C`, `--no-tools`: flags, not identifiers.
        return TOKEN_NOT_A_SYMBOL
    # Path separators, dots, whitespace, shell metacharacters, attributes, macros.
    # Checked on the RAW token so `src/cli.rs` and `0.16.6` are rejected before any
    # normalization can make them look identifier-shaped.
    if any(ch in DISQUALIFYING_CHARS for ch in raw):
        return TOKEN_NOT_A_SYMBOL
    if raw[0].isdigit():
        return TOKEN_NOT_A_SYMBOL

    had_path = "::" in raw
    had_parens = raw.endswith("()") or raw.endswith("(")
    core = normalize_symbol(raw)
    if not IDENT_RE.match(core):
        return TOKEN_NOT_A_SYMBOL

    if had_path or had_parens or "_" in core or not core.islower():
        return TOKEN_SYMBOL
    return TOKEN_AMBIGUOUS


def build_src_index(blobs: list[str]) -> set[str]:
    """The presence index: every word-ish token appearing anywhere in the code.

    A SET of word tokens rather than a concatenated searched blob -- stated here because
    it is a real choice with a real consequence. A set gives exact word-boundary matching
    (so `min` does not match inside `minimum`) and O(1) lookup over ~165k lines; a
    substring search over one blob would report a name present because it is a prefix of
    an unrelated one, which fails in the FLATTERING direction.
    """
    index: set[str] = set()
    for blob in blobs:
        index.update(SRC_WORD_RE.findall(blob))
    return index


def symbol_present(sym: str, src_index: set[str]) -> bool:
    """Presence anywhere in the indexed source. See LIMITS item 1 for what that is not."""
    return sym in src_index


def superseded_marker_near(line: str, col: int, window: int = MARKER_WINDOW_CHARS) -> str | None:
    """The deliberate-staleness marker in the window around `col`, or None."""
    lo = max(0, col - window)
    hi = min(len(line), col + window)
    haystack = line[lo:hi].lower()
    for marker in SUPERSEDED_MARKERS:
        if marker in haystack:
            return marker
    return None


def classify_symbol(sym: str, present: bool, line: str, col: int) -> str:
    """SYM_PRESENT / SYM_ABSENT / SYM_ABSENT_MARKED_SUPERSEDED.

    The third value is a REPORT, never a FILTER: a marked symbol is still absent and is
    still counted as absent. Whether a superseded marker LICENSES an absence is the design
    question the next session has to answer with this number in hand.
    """
    if present:
        return SYM_PRESENT
    if superseded_marker_near(line, col) is not None:
        return SYM_ABSENT_MARKED_SUPERSEDED
    return SYM_ABSENT


def snippet(line: str, col: int, width: int = SNIPPET_CHARS) -> str:
    """A trimmed context window, with both cuts marked in band."""
    half = max(8, width // 2)
    lo = max(0, col - half)
    hi = min(len(line), col + half)
    body = line[lo:hi].replace("\n", " ").strip()
    prefix = "…" if lo > 0 else ""
    suffix = "…" if hi < len(line) else ""
    return f"{prefix}{body}{suffix}"


class Census:
    """Every number named, and never summed with another."""

    def __init__(self) -> None:
        self.tokens = 0
        self.by_class: Counter[str] = Counter()
        self.by_verdict: Counter[str] = Counter()
        self.absent_rows: list[tuple[int, str, str]] = []
        self.files_indexed = 0
        self.distinct_symbols = 0


def run_census(doc_text: str, src_blobs: list[str], files_indexed: int) -> Census:
    """The whole decision, pure over text. All filesystem work is at the call site."""
    census = Census()
    census.files_indexed = files_indexed
    index = build_src_index(src_blobs)

    spans = extract_backticked(doc_text)
    census.tokens = len(spans)
    lines = doc_text.splitlines()
    seen: set[str] = set()

    for lineno, col, token in spans:
        kind = classify_token(token)
        census.by_class[kind] += 1
        if kind != TOKEN_SYMBOL:
            continue
        sym = normalize_symbol(token.strip())
        line = lines[lineno - 1] if 0 <= lineno - 1 < len(lines) else ""
        verdict = classify_symbol(sym, symbol_present(sym, index), line, col)
        census.by_verdict[verdict] += 1
        if verdict != SYM_PRESENT and sym not in seen:
            seen.add(sym)
            census.absent_rows.append((lineno, sym, snippet(line, col)))
    census.distinct_symbols = len(seen)
    return census


LIMITS = """LIMITS (printed on every run, including a clean one, so "could not check" can
never read as "checked; clean"):
  1. PRESENCE ANYWHERE, NOT PRESENCE AS A DEFINITION. A name surviving only in a comment,
     a test name or a doc string counts as present. This finds DELETED names, never MOVED
     ones and never wrongly-described ones.
  2. IT READS TODAY'S TREE, NOT HISTORY. DOCER's real rule compares successive snapshots;
     this clone is shallow, and stalebrain's own docs warn drift detection needs history
     and must re-verify rather than trust an empty pass on a shallow clone. So this cannot
     tell "was present and was removed" from "never existed".
  3. A PRESENT SYMBOL PROVES NOTHING ABOUT THE SENTENCE AROUND IT. The prose can name a
     live function and describe it wrongly, and that passes exactly as an honest sentence
     does -- the same limit tests/blind_round_grades.rs states about grades and
     tests/doc_version_claims.rs states about markers.
  This is a READER, not a gate: nothing here fails, reverts or files an issue."""


def render_census(census: Census, doc_path: str, max_rows: int) -> str:
    today = datetime.date.today().isoformat()
    n_sym = census.by_class[TOKEN_SYMBOL]
    n_not = census.by_class[TOKEN_NOT_A_SYMBOL]
    n_amb = census.by_class[TOKEN_AMBIGUOUS]
    present = census.by_verdict[SYM_PRESENT]
    absent = census.by_verdict[SYM_ABSENT]
    marked = census.by_verdict[SYM_ABSENT_MARKED_SUPERSEDED]

    out = [
        f"doc-symbol census — {doc_path} vs src/  ({today})",
        f"  {census.tokens} backticked token(s) extracted",
        f"  {n_sym} symbol / {n_not} not-a-symbol / {n_amb} ambiguous-english-word",
        f"  of the symbols: {present} present / {absent} absent / "
        f"{marked} absent-but-marked-superseded",
        f"  src/: {census.files_indexed} file(s) indexed",
        f"  {census.distinct_symbols} distinct absent symbol(s)",
    ]
    if census.absent_rows:
        out.append("")
        out.append("absent symbols (distinct, first occurrence):")
        shown = census.absent_rows[:max_rows]
        for lineno, sym, snip in shown:
            out.append(f"  {lineno}:{sym}")
            out.append(f"      {snip}")
        dropped = len(census.absent_rows) - len(shown)
        if dropped > 0:
            out.append(f"  … (+{dropped} more)")
    out.append("")
    out.append(LIMITS)
    return "\n".join(out)


def walk_src(root: str) -> list[tuple[str, str]]:
    """(path, text) for every indexed source file. The only filesystem walk."""
    found: list[tuple[str, str]] = []
    src_root = os.path.join(root, "src")
    for dirpath, _dirnames, filenames in os.walk(src_root):
        for name in sorted(filenames):
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, root)
            if not is_indexed_source(rel):
                continue
            try:
                with open(path, "r", encoding="utf-8", errors="replace") as fh:
                    found.append((rel, fh.read()))
            except OSError as exc:  # pragma: no cover - reported, never silently dropped
                print(f"warn: could not read {rel}: {exc}", file=sys.stderr)
    return found


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="check_doc_symbols.py",
        description=(
            "Census backticked Rust symbols in CLAUDE.md against src/. A READER, not a "
            "gate: it reports how many identifiers the doc names that no longer exist in "
            "the code, so the question of whether to build a gate can be answered with a "
            "number in hand."
        ),
        epilog=(
            "Tokens are classified SYMBOL / NOT-A-SYMBOL / AMBIGUOUS-ENGLISH-WORD and "
            "never pooled. An absent symbol carrying a deliberate-staleness marker is "
            "still counted absent and also sub-counted -- the marker is a report, not a "
            "filter. It shells nothing: no cargo, no gh, no git, no network."
        ),
    )
    parser.add_argument(
        "--doc", default="CLAUDE.md", metavar="PATH", help="the doc to scan (default: CLAUDE.md)"
    )
    parser.add_argument(
        "--root", default=".", metavar="DIR", help="repo root holding src/ (default: .)"
    )
    parser.add_argument(
        "--max-rows",
        type=int,
        default=DEFAULT_MAX_ROWS,
        help=f"cap on printed absent rows (default: {DEFAULT_MAX_ROWS}); the cut is marked in band",
    )
    parser.add_argument("--test", action="store_true", help="run the self-tests and exit")
    return parser


def main(argv: list[str]) -> int:
    args = build_parser().parse_args(argv)
    if args.test:
        return run_self_tests()

    doc_path = args.doc if os.path.isabs(args.doc) else os.path.join(args.root, args.doc)
    try:
        with open(doc_path, "r", encoding="utf-8", errors="replace") as fh:
            doc_text = fh.read()
    except OSError as exc:
        print(f"REFUSAL: could not read {doc_path}: {exc}", file=sys.stderr)
        print("This is a REFUSAL, not a clean census.", file=sys.stderr)
        return 2

    files = walk_src(args.root)

    # ------------------------------------------------------------------------------
    # ANTI-VACUOUS, ASSERTED FIRST. A scanner that finds nothing and reports a clean
    # bill is this very defect wearing the opposite sign, and it is quieter than the bug.
    # ------------------------------------------------------------------------------
    if not files:
        print(
            f"REFUSAL: walked {args.root}/src and indexed ZERO .rs files. "
            "This is a REFUSAL, not 'no drift'.",
            file=sys.stderr,
        )
        return 2

    census = run_census(doc_text, [text for _p, text in files], len(files))

    if census.tokens == 0:
        print(
            f"REFUSAL: extracted ZERO backticked tokens from {doc_path}. "
            "This is a REFUSAL, not 'no drift'.",
            file=sys.stderr,
        )
        return 2
    if census.by_class[TOKEN_SYMBOL] == 0:
        print(
            "REFUSAL: classified ZERO tokens as symbols -- the discriminator put the "
            "entire population out of scope, which would render a beautiful clean census "
            "over nothing. This is a REFUSAL, not 'no drift'.",
            file=sys.stderr,
        )
        return 2

    print(render_census(census, os.path.relpath(doc_path, args.root), args.max_rows))
    return 0


def run_self_tests() -> int:
    failures: list[str] = []

    def check(name: str, cond: bool, extra: object = "") -> None:
        if not cond:
            failures.append(f"{name}: {extra}")

    # -- the path population, which is the anti-self-contamination anchor --------------
    # THIS FILE contains backticked identifiers in its own docstring, and so will the
    # CLAUDE.md paragraph describing it. The anchor is that only src/**/*.rs is CODE.
    check("anchor: src/cli.rs is indexed", is_indexed_source("src/cli.rs"))
    check("anchor: src/format/mod.rs is indexed", is_indexed_source("src/format/mod.rs"))
    check(
        "anchor: THIS script is NOT indexed (self-contamination)",
        not is_indexed_source("scripts/check_doc_symbols.py"),
    )
    check("anchor: a sibling meter is NOT indexed", not is_indexed_source("scripts/evolve.sh"))
    check("anchor: a test gate is NOT indexed", not is_indexed_source("tests/module_size.rs"))
    check("anchor: the doc itself is NOT indexed", not is_indexed_source("CLAUDE.md"))

    # -- extraction --------------------------------------------------------------------
    spans = extract_backticked("a `foo_bar` and `Baz` here")
    check("extract: two spans", [t for _l, _c, t in spans] == ["foo_bar", "Baz"], spans)
    check("extract: 1-based line numbers", all(l == 1 for l, _c, _t in spans), spans)

    multi = extract_backticked("line one `alpha`\nline two `beta`")
    check(
        "extract: line numbers track the line",
        [(l, t) for l, _c, t in multi] == [(1, "alpha"), (2, "beta")],
        multi,
    )

    unbalanced = extract_backticked("stray ` backtick here\nnext `real_one` line")
    check(
        "extract: a stray backtick cannot swallow the next line",
        [t for _l, _c, t in unbalanced] == ["real_one"],
        unbalanced,
    )

    fenced = extract_backticked("before `outside_fn`\n```\n`inside_fence`\n```\nafter `after_fn`")
    check(
        "extract: fenced code blocks are skipped",
        [t for _l, _c, t in fenced] == ["outside_fn", "after_fn"],
        fenced,
    )

    doubled = extract_backticked("a ``primary`` token")
    check("extract: double backticks yield the inner token", "primary" in
          [t for _l, _c, t in doubled], doubled)

    # -- THE NEAR-MISS TABLE. Both directions -- a discriminator tested only on the side
    # -- that fires is vacuous green. These rows never call symbol_present, so they are
    # -- the population that must STAY GREEN under the symbol_present positive control.
    not_symbols = [
        "cargo test",
        "git diff --name-only",
        "--restricted",
        "-C",
        "tests/module_size.rs",
        "src/cli.rs",
        "0.16.6",
        "5062",
        "#[cfg(test)]",
        "git diff",
        "$(...)",
        ">",
        ".contains(",
        "assert_eq!",
        "",
        "   ",
    ]
    for tok in not_symbols:
        check(
            f"NEAR-MISS: {tok!r} is NOT a symbol",
            classify_token(tok) == TOKEN_NOT_A_SYMBOL,
            classify_token(tok),
        )

    symbols = [
        "truncate_long_line",
        "sanitize_for_display",
        "RESTRICTED_REMOVED_TOOLS",
        "CamelCase",
        "commands_refactor::significant_braces",
        "build_agent()",
        "run_git_output",
        "GaspRecorder",
    ]
    for tok in symbols:
        check(
            f"IS-A-SYMBOL: {tok!r} classifies as a symbol",
            classify_token(tok) == TOKEN_SYMBOL,
            classify_token(tok),
        )

    ambiguous = ["release", "search", "todo", "verbatim", "bash", "main"]
    for tok in ambiguous:
        check(
            f"AMBIGUOUS: bare lowercase word {tok!r} is its own class",
            classify_token(tok) == TOKEN_AMBIGUOUS,
            classify_token(tok),
        )
    check(
        "AMBIGUOUS: ... but with call parens the evidence exists and it is a symbol",
        classify_token("release()") == TOKEN_SYMBOL,
        classify_token("release()"),
    )

    # -- normalization -----------------------------------------------------------------
    check("normalize: strips ()", normalize_symbol("foo()") == "foo")
    check("normalize: strips a trailing (", normalize_symbol("foo(") == "foo")
    check("normalize: takes the final :: segment", normalize_symbol("a::b::c") == "c")
    check("normalize: leaves a bare name alone", normalize_symbol("plain_name") == "plain_name")

    # -- presence ----------------------------------------------------------------------
    index = build_src_index(["fn truncate_long_line(x: u32) {}\nlet minimum = 3;"])
    check("present: an indexed name is present", symbol_present("truncate_long_line", index))
    check("present: an unindexed name is absent", not symbol_present("truncate_diff_line", index))
    check(
        "present: word-boundary, not substring -- `min` must NOT match inside `minimum`",
        not symbol_present("min", index),
    )

    # -- classify_symbol: three values, and the marked one is still ABSENT ---------------
    check(
        "symbol: present wins",
        classify_symbol("f", True, "superseded blah `f` blah", 20) == SYM_PRESENT,
    )
    check(
        "symbol: plain absence",
        classify_symbol("f", False, "an ordinary sentence about `f` here", 28) == SYM_ABSENT,
    )
    marked_line = "this is a superseded claim about `truncate_diff_line`, recorded"
    check(
        "symbol: a nearby marker sub-classifies but does NOT filter",
        classify_symbol("truncate_diff_line", False, marked_line, marked_line.index("truncate"))
        == SYM_ABSENT_MARKED_SUPERSEDED,
    )
    far_line = "superseded" + (" " * 2000) + "`lonely_name` sits far away"
    check(
        "symbol: a marker 2000 chars away does NOT vouch for the name",
        classify_symbol("lonely_name", False, far_line, far_line.index("lonely")) == SYM_ABSENT,
    )
    for marker in SUPERSEDED_MARKERS:
        line = f"the thing was {marker} and `gone_name` went with it"
        check(
            f"symbol: marker {marker!r} is recognised",
            classify_symbol("gone_name", False, line, line.index("gone_name"))
            == SYM_ABSENT_MARKED_SUPERSEDED,
        )

    # -- the census fold ----------------------------------------------------------------
    # ANTI-VACUOUS on the fixture itself: it must genuinely contain a symbol, or every
    # "expected N" below is satisfied by having nothing to count.
    doc = (
        "`live_fn` is real and `gone_fn` is not.\n"
        "This is superseded: `also_gone` was renamed.\n"
        "Run `cargo test` and pass `--restricted` at version `0.16.6`.\n"
        "The word `release` is ambiguous.\n"
    )
    src = ["fn live_fn() -> u32 { 1 }"]
    c = run_census(doc, src, files_indexed=1)
    check("census ANTI-VACUOUS: tokens were extracted", c.tokens > 0, c.tokens)
    check("census ANTI-VACUOUS: symbols were found", c.by_class[TOKEN_SYMBOL] >= 3, c.by_class)
    check("census: present count", c.by_verdict[SYM_PRESENT] == 1, c.by_verdict)
    check("census: absent count", c.by_verdict[SYM_ABSENT] == 1, c.by_verdict)
    check(
        "census: marked-superseded is its own sub-count",
        c.by_verdict[SYM_ABSENT_MARKED_SUPERSEDED] == 1,
        c.by_verdict,
    )
    check("census: ambiguous is not pooled with symbols", c.by_class[TOKEN_AMBIGUOUS] == 1,
          c.by_class)
    check("census: not-a-symbol is not pooled either", c.by_class[TOKEN_NOT_A_SYMBOL] == 3,
          c.by_class)
    check(
        "census: distinct absent symbols counts BOTH absent kinds",
        c.distinct_symbols == 2,
        c.distinct_symbols,
    )
    check(
        "census: the absent rows name the symbols",
        sorted(s for _l, s, _n in c.absent_rows) == ["also_gone", "gone_fn"],
        c.absent_rows,
    )

    # -- rendering: the in-band cut marker ----------------------------------------------
    many = Census()
    many.tokens = 10
    many.by_class[TOKEN_SYMBOL] = 10
    many.by_verdict[SYM_ABSENT] = 10
    many.absent_rows = [(i, f"sym_{i}", "ctx") for i in range(10)]
    many.files_indexed = 1
    many.distinct_symbols = 10
    rendered = render_census(many, "CLAUDE.md", max_rows=3)
    check("render: the cut is marked IN BAND", "… (+7 more)" in rendered, rendered[-200:])
    check("render: the limits print on every run", "LIMITS" in rendered)
    check("render: it says outright it is a reader, not a gate", "not a gate" in rendered)
    no_cut = render_census(many, "CLAUDE.md", max_rows=50)
    check("render NEAR-MISS: no cut marker when nothing was dropped", "more)" not in no_cut)

    # -- snippet capping -----------------------------------------------------------------
    long_line = "x" * 5000 + "`sym`" + "y" * 5000
    snip = snippet(long_line, 5000)
    check("snippet: capped well under the 10,000-char line", len(snip) <= SNIPPET_CHARS + 4,
          len(snip))
    check("snippet: both cuts marked in band", snip.startswith("…") and snip.endswith("…"), snip)

    if failures:
        print(f"SELF-TESTS FAILED ({len(failures)}):", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print(f"self-tests passed ({len(SUPERSEDED_MARKERS)} superseded markers pinned)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
