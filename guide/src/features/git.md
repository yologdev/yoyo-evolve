# Git Integration

yoyo is git-aware. It shows your current branch and provides commands for common git operations.

## Branch display

When you're in a git repository, the REPL prompt shows the current branch:

```
main > _
feature/new-parser > _
```

On startup, the branch is also shown in the status information:

```
  git:   main
```

## Git commands

### /diff

Show a summary of uncommitted changes (equivalent to `git diff --stat`):

```
/diff
```

Output:
```
 src/main.rs | 15 +++++++++------
 README.md   |  3 +++
 2 files changed, 12 insertions(+), 6 deletions(-)
```

If there are no uncommitted changes:
```
  (no uncommitted changes)
```

### /undo

Revert all uncommitted changes. This is equivalent to `git checkout -- .`:

```
/undo
```

Before reverting, `/undo` shows you what will be undone:

```
 src/main.rs | 15 +++++++++------
 1 file changed, 9 insertions(+), 6 deletions(-)
  ✓ reverted all uncommitted changes
```

If there's nothing to undo:
```
  (nothing to undo — no uncommitted changes)
```

## Using git through the agent

yoyo's bash tool can run any git command. You can ask the agent directly:

```
> commit these changes with message "fix: handle empty input"
> show me the last 5 commits
> create a new branch called feature/parser
```

The agent has full access to git through its shell tool.
