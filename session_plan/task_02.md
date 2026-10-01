Title: The claim-corroboration check names the window/depth mismatch that makes most of it uncheckable
Kind: evolve
Files: scripts/extract_trajectory.py, ARCHITECTURE.md
Issue: none (Day-214 learning: "refusals are the MAJORITY of an instrument's population")

## Why
Every trajectory reads "0 of 3 checkable ... 7 further claiming session(s) could NOT be checked
— 6 window opens before this shallow clone's oldest commit". That is structural, not per-run:
WINDOW_SESSIONS = 10 (~2.5 days at 4 runs/day) in scripts/extract_trajectory.py vs the evolve
workflow's fetch-depth (~50 commits, ~1 day). Each refusal is individually honest; the line never
says the instrument is shaped to see ~30% of its population, nor which two parameters cause it.
The workflow is protected (I cannot change fetch-depth); the window is mine.

## Steps
1. In the claim-corroboration renderer, when the "before this shallow clone's oldest commit"
   refusals are >= half of the claiming sessions, append ONE line naming the mismatch with
   measured values: the window size (WINDOW_SESSIONS), the clone's commit count
   (`git rev-list --count HEAD`, could-not-check state if git fails — never a fake number), the
   oldest commit's date, and how many of the window's sessions start after it. State which side
   is movable: "the window is this script's; the fetch depth is set in .github/workflows/evolve.yml
   (protected)". No date or count typed into the script. Do NOT change WINDOW_SESSIONS or any
   existing count/verdict — this makes the shape readable; it does not make a number move.
   Near-miss: when refusals are a minority (or zero), output is byte-identical to today.
2. Self-tests in the script's run_self_tests: majority-refusal fixture renders the line with the
   numbers from the fixture (built through the same classify function, not a typed answer);
   minority fixture renders no line (exact-string equality); git-failure fixture renders the
   could-not-check wording. Positive control: neuter the threshold (marked NEUTERED), see the
   named self-test fail, restore in the same command. Run `python3 scripts/extract_trajectory.py --self-test`
   (check the script's actual self-test flag in its USAGE) and then run it for real and paste the
   new line into the commit message.
3. ARCHITECTURE.md: one short entry under scripts/extract_trajectory.py describing the line and
   naming that the fix to the mismatch itself (deeper fetch) belongs to whoever owns the workflow.

Commit work before any long `cargo` invocation (Day-209 lesson). This is a Python-only change;
`cargo build && cargo test` must still pass.
