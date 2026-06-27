Title: Add text.includeHtmlTags to Exa requests for code block preservation
Files: src/commands_web.rs
Issue: #529

## Context

Issue #529 reports that Exa's default text extraction strips HTML tags, which means code blocks (`<pre>`, `<code>`), tables (`<table>`), and structured formatting are lost. When researching technical documentation or blog posts with code examples, the results come back as flat text with no code structure.

The fix is adding `"includeHtmlTags":true` to the `text` object in the Exa API request body. Then in `strip_html_tags` (which already exists and is called on web results), preserve `<pre>` and `<code>` block contents by converting them to markdown fenced code blocks before stripping the remaining HTML.

## What to do

### 1. Update the Exa request body

In `exa_search()` (~line 1035), the current request body is:
```
r#"{{"query":"{}","type":"auto","numResults":{},"contents":{{"text":{{"maxCharacters":2000}},"highlights":true}}}}"#
```

Change the `text` object to include `includeHtmlTags`:
```
r#"{{"query":"{}","type":"auto","numResults":{},"contents":{{"text":{{"maxCharacters":2000,"includeHtmlTags":true}},"highlights":true}}}}"#
```

### 2. Enhance strip_html_tags to preserve code blocks

In `strip_html_tags()` (~line 54), before stripping all HTML tags, add a pre-processing step:
- Find `<pre>` ... `</pre>` and `<code>` ... `</code>` blocks
- Replace them with markdown fenced code blocks (` ```\n...\n``` `)
- Strip HTML tags from the content *inside* the code blocks (removing nested `<span>` etc. from syntax highlighting)
- Then proceed with normal HTML stripping for the rest

This should be done carefully:
- Handle nested `<code>` inside `<pre>` (common pattern: `<pre><code class="language-rust">...</code></pre>`)
- Don't double-fence — if `<code>` is inside `<pre>`, treat it as one block
- Extract the language hint from `class="language-X"` or `class="X"` if present, use it in the fence

### 3. Add tests

Add tests for:
- `strip_html_tags` with `<pre><code>` blocks → should produce fenced code blocks
- `strip_html_tags` with `<code>` inline → should preserve with backticks
- `strip_html_tags` with syntax-highlighted code (nested `<span>`) → should strip spans but keep code text
- The Exa request body contains `includeHtmlTags`

### Verification:
- `cargo build && cargo test`
- `cargo clippy --all-targets -- -D warnings`
