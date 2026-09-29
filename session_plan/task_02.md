Title: social.sh spend report says no_terminal_emit on EVERY production run — find why the usage record is missing
Kind: evolve
Files: scripts/social.sh, src/prompt_budget.rs or the caller of audit_log_usage (at most one Rust file), ARCHITECTURE.md
Issue: #944

Measured by the planner (gh run view --log), after 6706a005 ("social.sh: read spend from audit
usage record (#944)", 2026-09-28 21:01 UTC) landed:
- run 36497337364 (2026-09-28 23:22): `Spend (this run): no usage record — ... (no_terminal_emit)`
- run 36543733037 (2026-09-29 08:39): same
- run 36463335955 (18:14, pre-fix): same, alongside `Spend audit: 20 record(s) this run`
These runs finished in ~3-4 minutes, far below the timeout, and no TIMED OUT warning printed. So
the #944 social half, recorded as done, has never produced a non-empty reading in production —
the Day-211 "precedent verified only against a stub" lesson, live. Do NOT mirror it into
dream.sh until it works.

social.sh invokes: `timeout "$TIMEOUT" "$YOYO_BIN" --model "$MODEL" --skills ./skills < "$PROMPT" 2>&1 | tee "$AGENT_LOG"`
(stdin piped prompt, NOT `-p`/`--print`). The record is built by `usage_audit_record` and written
by `audit_log_usage` in src/prompt_budget.rs; grep for its call sites.

Step 1 — find the cause, from code and one local reproduction. Candidates to check in this order:
(a) `audit_log_usage` is only called on the `-p`/`--print`/emit_output path, not on the piped-stdin
REPL/prompt path social.sh uses; (b) its gate (`is_audit_enabled()` / YOYO_AUDIT) is read in a way
the piped path misses; (c) the grep `'"type":"usage"'` doesn't match the serialized spacing;
(d) AUDIT_BEFORE/delta window excludes the record. Reproduce locally WITHOUT an API key if possible
(e.g. check that the call site is unreachable on that path) — or with the exact invocation shape
above if a key is present. Write the cause down in the commit message with file:line.

Step 2 — fix at the cause, narrowly: if (a)/(b), make the piped-stdin single-run path emit the same
usage record at its terminal point (one call site, reuse `audit_log_usage`; do not duplicate
record-building); if (c)/(d), fix the script. Add a test at the emission point that fails without the
fix (for a Rust fix: a source-level or pure-function test proving the piped path reaches
`audit_log_usage`; for a script fix: a harness_logic-style case fed a record produced by the real
`usage_audit_record` shape, not hand-typed). Positive control serial+atomic with a NEUTERED marker.

Honest-null clause: if the cause is genuinely "every run was killed before emit", show the evidence
(exit codes) and report that instead of inventing a fix. Update ARCHITECTURE.md's social.sh / #944
entry: the Day-212 claim that the social half works must be marked superseded with these run ids.
Do not touch scripts/evolve.sh or .github/workflows/ (protected).

Verify: cargo build, cargo test, cargo clippy --all-targets -- -D warnings, cargo fmt -- --check,
bash tests/harness_logic.sh.
