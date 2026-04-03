Title: Interactive slash-command completion with descriptions (Issue #214)
Files: src/repl.rs
Issue: #214

## What to do

Upgrade the rustyline Completer from returning plain `String` candidates to returning `rustyline::completion::Pair` candidates with descriptions, and configure the editor to show list-style completions. This is the biggest UX gap vs Claude Code for interactive use.

### Changes in `src/repl.rs`:

1. **Change the `Candidate` type** in `impl Completer for YoyoHelper`:
   - Change `type Candidate = String;` to `type Candidate = Pair;` (from `rustyline::completion::Pair`)
   - Add `use rustyline::completion::Pair;` to imports

2. **Update the `complete()` method** to return `Vec<Pair>` instead of `Vec<String>`:
   - For slash commands: create `Pair { display: format!("{cmd}  {desc}"), replacement: cmd.to_string() }` where `desc` comes from `command_short_description()` in `help.rs`
   - For subcommand completions (model names, thinking levels, git subcommands, etc.): create `Pair { display: name.to_string(), replacement: name.to_string() }` (no description needed)
   - For file path completions: create `Pair { display: path.clone(), replacement: path }` (no description)

3. **Configure the rustyline Editor** where `Editor::new()` is called:
   - Instead of `Editor::new()`, use `Editor::with_config(config)` where:
     ```rust
     let config = rustyline::config::Builder::new()
         .completion_type(rustyline::config::CompletionType::List)
         .completion_prompt_limit(50)
         .build();
     ```
   - `CompletionType::List` shows all matches below the prompt line (bash-style)
   - `completion_prompt_limit(50)` asks before showing if there are >50 matches

4. **Get command descriptions**: Import `command_short_description` from `help.rs`. This function already exists and returns one-line descriptions for all 43+ commands. If it's not public, make it `pub`.

### Test updates:

- Existing tests use `helper.complete(...)` and check the returned candidates. Update them to check `candidate.replacement()` instead of direct string comparison, since the type changes from `String` to `Pair`.
- Add a test that verifies `/` completion returns pairs where `display` contains the description text.
- Add a test that verifies subcommand completions (like `/model `) still work correctly with `Pair` type.

### Expected behavior after change:

When user types `/` and presses Tab:
```
> /
/add        Add file contents to context
/apply      Apply a patch or diff
/ast        Structural code search
/changes    Show session changes
/commit     Create git commit
/compact    Compact conversation context
...
```

When user types `/he` and presses Tab:
```
> /help
```
(auto-completes since there's only one match)

This is a focused change to one file with clear before/after behavior.
