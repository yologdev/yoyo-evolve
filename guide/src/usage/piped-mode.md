# Piped Mode

When stdin is not a terminal (i.e., input is piped), yoyo reads all of stdin as a single prompt, processes it, and exits. This works like single-prompt mode but takes input from a pipe instead of a flag.

## Usage

```bash
echo "explain this code" | yoyo
cat prompt.txt | yoyo
git diff | yoyo
```

## When to use it

Piped mode is useful for:

- **Passing file contents** as part of the prompt
- **Chaining with other commands** in a pipeline
- **Feeding structured input** from scripts

## Examples

**Review a git diff:**
```bash
git diff HEAD~1 | yoyo --system "Review this diff for bugs."
```

**Analyze a file:**
```bash
cat src/main.rs | yoyo --system "Find all potential panics in this Rust code."
```

**Process command output:**
```bash
cargo test 2>&1 | yoyo --system "Explain these test failures and suggest fixes."
```

## Detection

yoyo detects piped mode automatically by checking if stdin is a terminal. If it is not, piped mode activates. If stdin is a terminal, interactive REPL mode starts instead.

If piped input is empty, yoyo exits with an error: `No input on stdin.`
