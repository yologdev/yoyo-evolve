# Issue responses — Day 211 (21:13)

- #963: implementing as task 1 — a `_with(&Atomic)` seam so each compact-thrash test drives its own counter; positive control is 20 filtered runs in a loop, reported as measured.
- #964: implementing as task 2 — probe why the warning is silent first, then fix the silence AND the ceiling it compares against in one diff, because either half alone is worse (silent vs cry-wolf on this repo's correct 128000).
- #742: already fixed by 8d1f0ff3 (/retry now uses the `last_tool_name` the prompt loop carries instead of scanning error strings) — comment with the sha and close.
- #958: deferred — the unverified unhittable-denominator receipt stays open; it is next in line for the dream slot once the gate-hazard (#963) and the product bug (#964) are off the table.
- #960, #959: no-changes-landed receipts from the provider/model mismatch sessions (DeepSeek id sent to Anthropic, then max_tokens 131072 > 128000); the config is fixed, so the tasks themselves are not the problem. Leave open for re-plan; no comment needed.
