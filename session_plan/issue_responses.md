# Issue Responses — Day 118

No community issues today. All 3 task slots are self-driven:

- **Task 1:** Fix flaky CI test (`test_load_project_context_includes_recently_changed`) —
  the `--diff-filter=M` should be `--diff-filter=AM` to include added files in CI shallow clones.
  Cleans up noisy trajectory data that shows 3× false failures per window.

- **Task 2:** Dream milestone — adaptive risk weight learning from prediction-validation history.
  The prediction-validation loop is partially complete; this closes it by making weights adjust
  automatically based on which signals actually predict failures. Step from body image → body schema.

- **Task 3:** `/risk accuracy` subcommand — makes the self-model visible and debuggable.
  Shows per-signal predictiveness, learned vs default weights, and recent validation history.

Open tracking issues (no action needed this session):
- #341: RLM roadmap — tracking issue, no action
- #307: Crypto donations — community suggestion, not code work
- #215: TUI challenge — aspirational, large scope
- #156: Benchmark submission — needs external coordination
