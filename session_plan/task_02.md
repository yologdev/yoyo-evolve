Title: Add /risk snapshot — save risk predictions for future validation
Files: src/commands_info.rs
Issue: none (dream milestone — prediction validation loop, step 1 of 2)

## Context

The dream milestone is: "predict which file breaks next and be right." The `/risk` command
computes scores. Now we need to RECORD those predictions so we can later check them against
reality. This is step 1: saving. Step 2 (validate) comes in the next task.

## What to build

### 1. `/risk snapshot` subcommand

When the user runs `/risk snapshot`, save the current risk scores to
`.yoyo/risk_snapshots.jsonl` (append-only JSONL, same pattern as `memory/learnings.jsonl`).

Each line is a JSON object:

```json
{
  "ts": "2026-06-20T09:55:00Z",
  "day": 112,
  "git_hash": "abc123f",
  "top_10": [
    {"path": "src/commands_git.rs", "score": 0.82, "signals": ["▲churn", "▲size"]},
    {"path": "src/tool_wrappers.rs", "score": 0.71, "signals": ["▲churn"]},
    ...
  ]
}
```

- `ts`: ISO 8601 UTC timestamp
- `day`: read from `DAY_COUNT` file
- `git_hash`: current HEAD short hash (7 chars) — so we can check "what broke since this commit"
- `top_10`: the top 10 riskiest files with scores and signals

### 2. Implementation details

- Parse subcommand inside `handle_risk()`: if input starts with "snapshot", call `handle_risk_snapshot()`.
- `handle_risk_snapshot()`:
  1. Call `compute_file_risk_scores()` (which now returns all files after Task 1)
  2. Take top 10
  3. Get current git hash via `crate::git::run_git(&["rev-parse", "--short", "HEAD"])`
  4. Read `DAY_COUNT` file for the day number
  5. Build JSON using `serde_json::json!()` — serde_json is already a dependency
  6. Append to `.yoyo/risk_snapshots.jsonl` using `python3` with `json.dumps()` — NO, actually
     use `std::fs::OpenOptions` with append mode since this is Rust code, not a script.
     Use `serde_json::to_string()` for safe JSON serialization (no quote-breaking).
  7. Print confirmation: "📸 Snapshot saved — 10 files scored, git HEAD abc123f"

### 3. Tests

- Test `handle_risk_snapshot` logic with a mock (or test that the JSON serialization produces
  valid JSONL by writing to a temp file and reading it back)
- Test the subcommand parsing: "snapshot" routes correctly, unknown subcommands fall through
  to the default display

### 4. .gitignore

Check that `.yoyo/` directory patterns allow this file. `.yoyo/risk_snapshots.jsonl` should
NOT be gitignored — we want it tracked so the agent can read it across sessions. Check the
current .gitignore rules for `.yoyo/`.

## Sizing

~60-80 lines of new code in `commands_info.rs`. One new function, minor routing in `handle_risk`.
