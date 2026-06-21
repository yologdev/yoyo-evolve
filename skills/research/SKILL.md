---
name: research
description: Search the web and read real pages (Exa + Firecrawl) when stuck or learning something new
tools: [bash]
core: true
origin: creator
---

# Research

You have real web access through bash. Use it when you're stuck, implementing
something unfamiliar, checking what others built, or answering any question about
the outside world.

## Hard rule: never answer from memory

If a question is about the world outside this repo — "what can Claude Code do",
"how does crate X work", "what's new in Y", a competitor's features, an error you
don't recognize — you **MUST fetch and quote a real page**. Do NOT answer from
training knowledge: it is stale and you will be confidently wrong. Search or fetch
first, then answer only from what you actually read. A research step that produces
no fetched source is a failed research step — say so rather than guessing.

## Search + read in one call — Exa (your default)

Exa searches the web AND returns page text in a single request. Use it for
discovery ("find pages about X"):

```bash
curl -sS -X POST https://api.exa.ai/search \
  -H "x-api-key: $EXA_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"query":"YOUR QUESTION","type":"auto","numResults":5,
       "contents":{"text":{"maxCharacters":2000},"highlights":true}}' \
  | jq -r '.results[] | "### \(.title)\n<\(.url)>\nHighlights: \((.highlights // []) | join(" | "))\nText: \((.text // "")[0:1500])\n"'
```

`highlights` are LLM-picked relevant snippets (cheap, token-efficient); `text` is
the page body. Read highlights first, pull more `text` only if you need it.

## Read a specific known URL — Firecrawl (robust)

When you already know the URL (a doc page, a competitor's changelog, a GitHub
file) and want clean, complete content — especially JS-heavy or anti-bot pages
that bare `curl` can't read — use Firecrawl. It returns clean markdown:

```bash
curl -sS -X POST https://api.firecrawl.dev/v2/scrape \
  -H "Authorization: Bearer $FIRECRAWL_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"url":"https://THE/PAGE","formats":["markdown"]}' \
  | jq -r '.data.markdown' | head -c 4000
```

Firecrawl's monthly budget is smaller than Exa's — reserve it for pages you
specifically need clean/complete. For light reads, Exa's `text` is enough.

## Read a simple known URL — plain curl (free)

For raw/simple sources, a direct fetch needs no API:

```bash
curl -sSL "https://raw.githubusercontent.com/ORG/REPO/main/src/main.rs" | head -200
curl -sSL "https://docs.rs/CRATE/latest/CRATE/" | sed 's/<[^>]*>//g' | head -120
```

## Rules

- Have a specific question before you search. No aimless browsing.
- Prefer official docs / changelogs / source over random blogs.
- Quote what you actually read; never paraphrase from memory.
- Keys come from `$EXA_API_KEY` / `$FIRECRAWL_API_KEY` (set in the run env). If a
  key is unset or a call errors, fall back to plain `curl` of a known URL and say
  the API was unavailable — do NOT silently answer from memory instead.

## When to research

- Implementing something you've never done before
- An error you don't understand
- Checking what Claude Code / other agents actually do — read their docs, don't guess
- A community issue references a concept you're unfamiliar with
- Comparing approaches or conventions before choosing one
- (Dream cycles) wandering the world to see what's new
