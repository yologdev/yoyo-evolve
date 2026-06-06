Title: Fix #469 — strip --skills flag from input before passing to handle_skill
Files: src/dispatch_sub.rs
Issue: #469

## Problem

`yoyo skill list --skills ./skills` fails because `quote_args_as_command(args)` joins ALL args (including `--skills ./skills`) into the input string, which becomes `"list --skills ./skills"`. Then `handle_skill` does an exact match on `"list"` and fails.

The `--skills` flag is correctly extracted by `collect_repeatable_flag(args, "--skills")` on the line above, but the raw args are still used for `quote_args_as_command`.

## Fix

In `dispatch_sub.rs`, in the `"skill"` match arm (around line 192-204):

1. After calling `collect_repeatable_flag(args, "--skills")`, build a filtered args list that excludes `--skills` and its value before passing to `quote_args_as_command`.

2. The filtered args should remove any `--skills` flag and the following argument. For example:
   - `["list", "--skills", "./skills"]` → `["list"]`
   - `["show", "my-skill", "--skills", "./skills"]` → `["show", "my-skill"]`
   - `["list"]` → `["list"]` (unchanged when no --skills flag)

3. Use the filtered args for `quote_args_as_command` instead of the raw `args`.

Implementation approach:
```rust
"skill" => {
    let skill_dirs = collect_repeatable_flag(args, "--skills");
    // Strip --skills and its values from args before building the command string
    let mut filtered_args: Vec<String> = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--skills" {
            skip_next = true;
            continue;
        }
        filtered_args.push(arg.clone());
    }
    let input = quote_args_as_command(&filtered_args);
    // ... rest unchanged
}
```

4. Add a test in the existing test module in `dispatch_sub.rs`:
   - `test_skill_list_with_skills_flag_strips_flag`: verify that when args are `["list", "--skills", "./skills"]`, the `--skills` flag is properly stripped and handle_skill receives just `"list"`.

Note: `quote_args_as_command` prefixes with `/`, so the actual input to `handle_skill` would be `/list` or similar. Check the exact format and test accordingly.
