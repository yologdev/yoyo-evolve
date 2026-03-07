---
name: self-assess
description: Analyze your own source code and capabilities to find bugs, gaps, and improvement opportunities
tools: [bash, read_file, write_file]
---

# Self-Assessment

You are assessing yourself. Your source code is your body. Read it critically.

## Process

1. **Read your source code** completely (`src/main.rs` and any other source files)
2. **Read LEARNINGS.md.** Check your accumulated lessons — patterns that worked, mistakes to avoid, insights from past sessions. Build on what you already know.
3. **Try using yourself.** Pick a small real task and attempt it:
   - Edit a file and check the result
   - Run a shell command that might fail
   - Try an edge case (empty input, long input, special characters)
4. **Note what went wrong.** Be specific:
   - Did you crash? Where?
   - Did you give a bad error message? What should it say?
   - Was something slow or clunky?
   - Is there a feature you needed but didn't have?
5. **Check JOURNAL.md.** Have you tried something before that failed? Don't repeat the same mistake.

## What to look for

- `unwrap()` calls — these are potential panics. Every one is a bug waiting to happen.
- Missing error messages — if something fails silently, that's a problem.
- Hard-coded values — magic numbers, hard-coded paths, assumptions about the environment.
- Missing edge cases — what happens with empty input? Unicode? Very long strings?
- User experience gaps — is anything confusing, unclear, or annoying?

## Output

Write your findings as a prioritized list. The most impactful issue goes first. Format:

```
SELF-ASSESSMENT Day [N]:
1. [CRITICAL/HIGH/MEDIUM/LOW] Description of issue
2. ...
```

Then prioritize which ones to tackle this session. Fix as many as you can.
