#!/usr/bin/env python3
"""Lint scripts/evolve.sh for the recurring apostrophe-in-parameter-expansion bug.

Bash inside ${VAR:+WORD} and ${VAR:-WORD} interprets single quotes. Any
unescaped apostrophe in the WORD opens a quoted string that scrambles parsing
until a literal } produces "bad substitution: no closing }". The script dies
at that point, mid-session: whichever agents ran before the broken heredoc
have already spent their budget, and every phase after it never runs.

History, verified by running this lint against each commit and its parent
(the authoritative record — two other copies of it drifted, so keep this one
correct and let the others point here):

  988975d9  introduced it (journal prompt, ${EXTERNAL_JOURNALS:+...})
  cb9d9b0   red session, chased the symptom — did not introduce or fix it
  25f4e90   red session, chased the symptom — did not introduce or fix it
  9847db2   FIXED the first landing
  d93e4f65  introduced it again (Phase A2 planner prompt, ${RECENT_REVERTS:+...})
  050e300c  FIXED the second landing

So: landed twice, four red sessions, two fixes. Each early fix chased what
printed right before the crash instead of the cause; this lint enforces the
cause directly — no apostrophes inside ${VAR:+...} or ${VAR:-...} blocks.

Usage:
  lint_evolve_heredocs.py [PATH] [--label NAME]

PATH defaults to scripts/evolve.sh next to this script. --label renames the
file in the messages, so a caller linting a blob extracted from a commit into
a temp file can still say "scripts/evolve.sh". Callers that pass no arguments
(CI, evolve.sh) behave exactly as before.

Exit codes:
  0  clean
  1  one or more apostrophes found (prints location and offending lines)
  2  could not check (file missing or unreadable) — never confused with clean
"""
import argparse
import sys
from pathlib import Path

TARGET = Path(__file__).resolve().parent.parent / "scripts" / "evolve.sh"


def find_param_expansion_blocks(src):
    """Yield (start_line, block_text) for each ${VAR:+...} or ${VAR:-...}.

    Walks the source character by character to handle nested {} correctly.
    """
    i, n = 0, len(src)
    while i < n:
        j = src.find("${", i)
        if j < 0:
            return
        # find the colon that opens :+ or :-
        k = j + 2
        while k < n and src[k] not in ":}":
            k += 1
        if k >= n or src[k] != ":" or k + 1 >= n or src[k + 1] not in "+-":
            i = j + 2
            continue
        # find the balanced closing }
        depth = 1
        m = k + 2
        while m < n and depth > 0:
            if src[m] == "{":
                depth += 1
            elif src[m] == "}":
                depth -= 1
            m += 1
        block = src[j:m]
        line = src[:j].count("\n") + 1
        yield line, block
        i = m


def lint_source(src, label):
    """Return (exit_code, report). Pure: no I/O, so the tests drive this."""
    bad = [
        (line, block)
        for line, block in find_param_expansion_blocks(src)
        if "'" in block
    ]
    if not bad:
        return 0, ""

    out = [
        f"ERROR: {label} contains apostrophes inside ${{VAR:+...}} "
        "or ${VAR:-...} blocks.\n"
        "Bash interprets single quotes inside parameter expansion WORDs, so "
        "an apostrophe (e.g. Don't, Here's, you're) opens a quoted string "
        "that scrambles parsing until a literal } produces "
        '"bad substitution: no closing }". evolve.sh then dies at that '
        "heredoc, mid-session: agents before it have already spent their "
        "budget, and every phase after it never runs.\n"
        "Fix: rephrase to avoid the apostrophe (Don't -> Do not, Here's -> "
        "Here is, etc). See this file's docstring for the full history.\n"
    ]
    for line, block in bad:
        out.append(f"--- block starting at {label}:{line} ---")
        for offset, ln in enumerate(block.splitlines()):
            if "'" in ln:
                out.append(f"  line {line + offset}: {ln.rstrip()}")
        out.append("")
    return 1, "\n".join(out)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("path", nargs="?", default=None, help="file to lint")
    parser.add_argument("--label", default=None, help="name to use in messages")
    args = parser.parse_args(argv)

    path = Path(args.path) if args.path else TARGET
    label = args.label or ("scripts/evolve.sh" if args.path is None else str(path))

    try:
        src = path.read_text()
    except OSError as exc:
        # Exit 2, not 1: "could not check" is not "checked and dirty", and
        # neither is it "clean". A caller must be able to tell them apart.
        print(f"ERROR: cannot read {label}: {exc}", file=sys.stderr)
        return 2

    code, report = lint_source(src, label)
    if report:
        print(report)
    return code


if __name__ == "__main__":
    sys.exit(main())
