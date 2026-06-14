Title: Structured plan steps with checklist tracking in /plan
Files: src/commands_plan.rs
Issue: none

## What

Enhance `/plan` to parse generated plans into structured, numbered steps with completion tracking. Add `/plan status` to show progress and `/plan step N done` to manually check off steps. When `/plan apply` runs, auto-detect step completion from the agent's output.

## Why

Cursor's plan mode generates structured task lists from natural language descriptions with progress tracking. yoyo's `/plan` exists and can generate plans and apply them, but treats the plan as opaque text — there's no structure, no progress tracking, no way to see which steps are done. Developers working through complex changes need to see their progress.

## Implementation

1. **Add a `PlanStep` struct**:
   ```rust
   pub struct PlanStep {
       pub number: usize,
       pub title: String,
       pub description: String,
       pub completed: bool,
   }
   ```

2. **Add a `StructuredPlan` struct**:
   ```rust
   pub struct StructuredPlan {
       pub raw_text: String,
       pub steps: Vec<PlanStep>,
   }
   ```

3. **Add `parse_plan_steps(plan_text: &str) -> Vec<PlanStep>`**:
   - Parse numbered items from the plan text. Look for patterns like:
     - `1. **Title** — description`
     - `1. Title\n   description`
     - `- [ ] Title` (markdown checklist)
     - `Step 1: Title`
   - Extract the step number, title, and description body
   - Return a vec of `PlanStep` with all `completed: false`

4. **Update `set_last_plan` / `get_last_plan`** to store `StructuredPlan` instead of raw `String`:
   - When a plan is generated, parse it into steps and store the structured version
   - `get_last_plan()` returns `Option<StructuredPlan>` (update the `LAST_PLAN` static)

5. **Add `/plan status` subcommand**:
   - Display each step with a checkbox: `[x] Step 1: ...` or `[ ] Step 2: ...`
   - Show completion percentage: `3/7 steps complete (43%)`
   - Highlight the next incomplete step

6. **Add `/plan step N done` subcommand** (and `/plan step N undo`):
   - Mark step N as completed (or uncompleted)
   - Print updated status

7. **Update the `PLAN_SUBCOMMANDS` const** to include `"status"` and `"step"`.

8. **Tests**:
   - `parse_plan_steps` correctly parses numbered lists
   - `parse_plan_steps` handles markdown checklist format
   - `parse_plan_steps` handles mixed formatting
   - Step marking works (done/undo)
   - Status display format
   - Empty plan produces empty steps
   - Plan with no parseable steps stores raw text with empty steps vec

## Scope guard
Only modify `src/commands_plan.rs`. The plan generation prompt (`build_plan_prompt`) can optionally be updated to encourage numbered output format, but this is not required — the parser should handle whatever format the model produces.

## Verification
`cargo build && cargo test && cargo clippy --all-targets -- -D warnings`
