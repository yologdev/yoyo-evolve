Title: Wire emerging-risk annotations into auto-context and watch-mode fix prompts
Files: src/commands_project.rs, src/watch.rs
Issue: none (Dream milestone — allostatic signals reaching decision points)

## Context

Task 01 adds `detect_emerging_risks()` to `commands_risk.rs`. This task wires those signals
into the two places where the model makes editing decisions: auto-context (injected before
prompts) and watch-mode fix prompts (injected when tests fail).

The dream says: "An allostatic system would anticipate the *next* region of fragility based
on the pattern of recent changes." The risk reflex already annotates *currently* high-risk
files. This task adds annotations for files that are *trending toward* high risk — the
anticipatory signal.

## What to build

### src/commands_project.rs — auto_context_for_prompt enhancement
In `auto_context_for_prompt()`, after the existing risk annotation logic (which calls
`top_risk_files()`), add an emerging-risk annotation:
1. Call `crate::commands_risk::detect_emerging_risks()` (from Task 01).
2. If any files in the auto-context match emerging-risk files, add a note:
   `"⚡ Emerging risk: {path} — changing {momentum:.1f}× faster than usual. Extra care advised."`
3. Keep it to at most 2 annotations to avoid noise.

### src/watch.rs — fix prompt enhancement
In the watch-mode fix prompt builder (around line 1094 where `risk_context_for_files` is
already called), add emerging-risk context:
1. Call `detect_emerging_risks()` and check if any of the error files appear.
2. If so, append to the risk context: `"Note: {path} is an emerging risk — its change rate
   is accelerating. Test more carefully."`

## Tests
- Add a unit test in `commands_project.rs` that verifies `format_auto_context` includes
  emerging risk annotations when present (mock the data or use test helpers).
- Verify existing tests pass — this is additive, shouldn't break anything.

## Dependency
This task depends on Task 01 completing successfully (it imports `detect_emerging_risks`
from `commands_risk`). If Task 01 was reverted, this task should be skipped.
