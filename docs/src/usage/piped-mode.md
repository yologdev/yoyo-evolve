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

## Structured output (`--output-format`)

Piped and single-prompt modes accept `--output-format <fmt>`:

- `text` (default) — just the response text.
- `json` — a single JSON object with the final result.
- `stream-json` — newline-delimited JSON (NDJSON): one yoagent `AgentEvent`
  per line, emitted in real time as the agent works.

The `stream-json` stream is the raw yoagent `AgentEvent` serialization —
internally tagged on a `"type"` field, `camelCase` keys, full fidelity (no
lossy translation). The first line is always `{"type":"agentStart"}` and the
last is `{"type":"agentEnd", ...}`. Tool activity appears as
`toolExecutionStart` / `toolExecutionEnd`, and assistant text arrives as
`messageStart` / `messageUpdate` / `messageEnd`. Turn boundaries are
`turnStart` / `turnEnd`.

```bash
echo "list the files" | yoyo --output-format stream-json
```

```jsonl
{"type":"agentStart"}
{"type":"turnStart", ...}
{"type":"messageStart", ...}
{"type":"toolExecutionStart","toolCallId":"...","toolName":"bash","args":{...}}
{"type":"toolExecutionEnd","toolCallId":"...", ...}
{"type":"messageEnd", ...}
{"type":"agentEnd", ...}
```

The exact set of fields on each event is defined by yoagent — see its
[messages & events reference](https://github.com/yologdev/yoagent/blob/main/docs/concepts/messages-events.md)
for the authoritative contract.

## Quiet mode

When both stdin and stdout are piped (fully scripted usage), yoyo automatically enables quiet mode, suppressing informational `config:` and `context:` loading messages on stderr. You can also enable this explicitly with `--quiet` or `-q`:

```bash
echo "fix the test" | yoyo -q > result.md  # explicit quiet
echo "fix the test" | yoyo > result.md     # auto-quiet (both pipes detected)
```

The `YOYO_QUIET=1` environment variable also enables quiet mode.

## Slash commands aren't dispatched in piped mode

Slash commands (`/doctor`, `/status`, `/help`, etc.) belong to the interactive REPL — they depend on REPL state that piped mode doesn't have. If you pipe a slash command into yoyo, it won't run it; it would only get sent to the model as a literal string and waste a turn of tokens.

Instead, yoyo detects this case, prints a one-line warning to stderr, and exits with status code `2`. Use one of these alternatives:

```bash
yoyo doctor                       # run the subcommand directly
yoyo --prompt "/doctor"           # send the literal text to the agent
yoyo                              # interactive REPL
```

