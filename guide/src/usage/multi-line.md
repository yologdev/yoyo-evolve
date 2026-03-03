# Multi-Line Input

yoyo supports two ways to enter multi-line input.

## Backslash continuation

End a line with `\` to continue on the next line:

```
main > Please review this code and \
  ...  check for any bugs or \
  ...  performance issues.
```

The backslash and newline are removed, and the lines are joined. The `...` prompt indicates yoyo is waiting for more input.

## Code fences

Start a line with triple backticks (`` ``` ``) to enter a fenced code block. Everything until the closing `` ``` `` is collected as a single input:

```
main > ```
  ...  Here is a function I want you to review:
  ...  
  ...  fn parse(input: &str) -> Result<Config, Error> {
  ...      let data = serde_json::from_str(input)?;
  ...      Ok(Config::from(data))
  ...  }
  ...  
  ...  Is this handling errors correctly?
  ...  ```
```

This is useful for pasting code or structured text that spans multiple lines.
