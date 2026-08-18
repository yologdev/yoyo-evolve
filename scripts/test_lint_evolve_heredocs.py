#!/usr/bin/env python3
"""Tests for lint_evolve_heredocs.py.

The lint guards a bug that has landed twice and cost four red sessions, and it
had no test of its own until the pre-push hook started depending on it. These
drive the pure seam (`lint_source`) plus the two exit codes a caller has to be
able to tell apart: 1 "checked, dirty" and 2 "could not check".

Discovered automatically by CI's `python3 -m unittest discover -s scripts
-p 'test_*.py'`; no workflow change needed.
"""
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lint_evolve_heredocs import find_param_expansion_blocks, lint_source, main


class TestLintSource(unittest.TestCase):
    def test_clean_source_passes(self):
        code, report = lint_source('echo "${FOO:+no apostrophes here}"', "f.sh")
        self.assertEqual(code, 0)
        self.assertEqual(report, "")

    def test_the_exact_d93e4f65_text_is_caught(self):
        # The line that actually shipped and killed the planner prompt.
        src = (
            "cat > x <<EOF\n"
            "${RECENT_REVERTS:+\n"
            "The receipt BODY holds what a title cannot: the evaluator's verdict\n"
            "}\n"
            "EOF\n"
        )
        code, report = lint_source(src, "scripts/evolve.sh")
        self.assertEqual(code, 1)
        self.assertIn("evaluator's verdict", report)
        self.assertIn("scripts/evolve.sh:2", report)

    def test_colon_dash_form_is_caught_too(self):
        code, _ = lint_source("${FOO:-that's the default}", "f.sh")
        self.assertEqual(code, 1)

    def test_apostrophe_outside_an_expansion_is_fine(self):
        # Ordinary shell quoting must not be flagged, or the lint is unusable.
        code, _ = lint_source("echo \"it's fine\"\necho 'also fine'\n", "f.sh")
        self.assertEqual(code, 0)

    def test_label_appears_in_the_report(self):
        _, report = lint_source("${A:+it's bad}", "scripts/evolve.sh (refs/heads/main)")
        self.assertIn("scripts/evolve.sh (refs/heads/main)", report)

    def test_nested_braces_do_not_end_the_block_early(self):
        # The walker must find the block's real closing brace; an apostrophe
        # after a nested ${...} still belongs to the outer WORD.
        src = "${A:+prefix ${B} that's still inside}"
        blocks = list(find_param_expansion_blocks(src))
        self.assertEqual(len(blocks), 1)
        self.assertEqual(lint_source(src, "f.sh")[0], 1)

    def test_reported_line_number_is_the_block_start(self):
        src = "line1\nline2\n${A:+\nbad's here\n}\n"
        _, report = lint_source(src, "f.sh")
        self.assertIn("--- block starting at f.sh:3 ---", report)
        self.assertIn("line 4: bad's here", report)


class TestMainExitCodes(unittest.TestCase):
    def test_missing_file_exits_2_not_1(self):
        # "could not check" must be distinguishable from "checked and dirty",
        # or the pre-push hook cannot tell a violation from a broken setup.
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(main([str(Path(d) / "nope.sh")]), 2)

    def test_dirty_file_exits_1(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "x.sh"
            p.write_text("${A:+it's bad}\n")
            self.assertEqual(main([str(p)]), 1)

    def test_clean_file_exits_0(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "x.sh"
            p.write_text("${A:+all good}\n")
            self.assertEqual(main([str(p)]), 0)

    def test_no_argument_still_checks_the_real_evolve_sh(self):
        # CI and evolve.sh both invoke it with no arguments; that path must
        # keep working exactly as before, and the repo must be clean today.
        self.assertEqual(main([]), 0)


if __name__ == "__main__":
    unittest.main()
