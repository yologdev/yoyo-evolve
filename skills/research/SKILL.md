---
name: research
description: Search and read the web (via the web_search tool) when stuck or learning something new
tools: [bash]
core: true
origin: creator
---

# Research

You have real web access. Use it when you're stuck, implementing something
unfamiliar, checking what others built, or answering any question about the
outside world.

## Hard rule: never answer from memory

If a question is about the world outside this repo — "what can Claude Code do",
"how does crate X work", "what's new in Y", a competitor's features, an error you
don't recognize — you **MUST fetch and quote a real page**. Do NOT answer from
training knowledge: it is stale and you will be confidently wrong. Search or fetch
first, then answer only from what you actually read. A research step that produces
no fetched source is a failed research step — say so rather than guessing.

## How to search and read

- **Discover + read** → the `web_search` tool. Give it a specific query; it
  returns search results (titles, URLs, and page content). This is your default
  for "find pages about X" and for reading what they say. The tool handles the
  search/scrape APIs and credentials for you — you don't manage endpoints or keys.
- **A specific known URL** → fetch it directly with bash, e.g.
  `curl -sSL "https://raw.githubusercontent.com/ORG/REPO/main/src/lib.rs" | head -200`
  or `curl -sSL "https://docs.rs/CRATE/latest/CRATE/" | sed 's/<[^>]*>//g' | head -120`.

## Rules

- Have a specific question before you search. No aimless browsing.
- Prefer official docs / changelogs / source over random blogs.
- Quote what you actually read; never paraphrase from memory.
- If a search or fetch fails, say the source was unavailable and try another URL —
  never silently fall back to answering from memory.

## When to research

- Implementing something you've never done before
- An error you don't understand
- Checking what Claude Code / other agents actually do — read their docs, don't guess
- A community issue references a concept you're unfamiliar with
- Comparing approaches or conventions before choosing one
- (Dream cycles) wandering the world to see what's new
