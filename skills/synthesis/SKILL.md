---
name: synthesis
description: "Multi-source research synthesis — aggregate and compare 3+ sources or any source >5KB using sub-agent dispatch and SharedState"
version: "1.0"
origin: yoyo
core: false
status: active
score: 0.66
uses: 2
wins: 2
last_used: "2026-05-01T06:18:55Z"
last_evolved: "2026-08-22"
parent_pattern_key: null
tools: [bash, read_file, write_file, search, sub_agent, shared_state]
keywords: ["skills/synthesis", "synthesis.source", "multi-source", "aggregate sources", "compare sources", "multiple sources"]
---

# Synthesis

You are performing **multi-source research synthesis** — aggregating, comparing, and composing insights from multiple sources (papers, blog posts, docs, discussions, code) into a coherent answer. The goal is a single composed response that draws on all sources, not a list of summaries.

This skill exists because loading 3+ full web pages or documents into your main context window is wasteful and noisy. The pattern (Recursive Language Model — see the RLM substrate section in CLAUDE.md) is: fetch each source, store it in shared state, dispatch a sub-agent per source to extract key claims, then dispatch a final synthesis sub-agent to compose the answer.

This skill complements the existing `research` skill. Research handles single small sources (fetch → read → answer). Synthesis handles the cases where multiple sources or large sources make direct reading impractical.

## When to use

Trigger this skill when ANY of these hold:

- **3 or more sources** on a single topic need to be compared or aggregated
- **Any single source exceeds 5KB** (roughly 150 lines / 1,250 tokens) — too large to read efficiently in main context
- A question requires **cross-referencing** claims from different authors or documents
- A community issue links to multiple external references that need to be digested together

## When NOT to use

- **1-2 sources, all under 5KB.** Use the `research` skill's single-source procedure — direct `curl` and read. Sub-agent overhead exceeds the savings.
- **The sources are code files, not prose.** Use `explore-codebase` instead — it's optimized for structural code comprehension, not prose synthesis.
- **You already know the answer.** Don't synthesize to confirm what you already understand. That's burning sub-agent budget for validation theater.
- **You're inside a sub-agent at depth 3.** Stop. Return what you have. Do not dispatch further.

## Procedure

### 1. Frame the research question (single sentence)

Examples of well-framed questions:
- *"What are the tradeoffs between streaming JSON parsing libraries in Rust (serde_json, simd-json, sonic-rs)?"*
- *"How do different coding agents (Aider, Continue, Cursor) handle context window management?"*
- *"What does the academic literature say about recursive LLM agent architectures?"*

A good question names a specific topic and what you want to learn. Don't ask vague questions like *"tell me about Rust"*.

### 2. Gather sources

Fetch each source via `bash`. Typical patterns:

```bash
# Web page
curl -sL "https://example.com/article" | sed 's/<[^>]*>//g' | head -500

# GitHub README
curl -sL "https://raw.githubusercontent.com/org/repo/main/README.md"

# Documentation page
curl -sL "https://docs.rs/crate/latest/crate/" | sed 's/<[^>]*>//g' | head -500

# Search results (to find sources)
curl -s "https://lite.duckduckgo.com/lite?q=your+query" | sed 's/<[^>]*>//g' | head -60
```

Aim for 3-7 sources. More than 7 rarely adds insight — diminishing returns set in fast.

### 3. Decide: direct synthesis or sub-agent dispatch?

Estimate total content size:

```bash
echo "$SOURCE_1" | wc -c
# repeat for each source
```

- **Total < 5KB across all sources**: Synthesize directly in your main context. Skip sub-agents — the overhead isn't worth it.
- **Total ≥ 5KB**: Proceed with sub-agent dispatch (Step 4).

### 4. Store sources in shared state

Store each source under a namespaced key:

```
shared_state set key="synthesis.<topic>.source-1" value="<source 1 content>"
shared_state set key="synthesis.<topic>.source-2" value="<source 2 content>"
shared_state set key="synthesis.<topic>.source-3" value="<source 3 content>"
```

Namespace convention: `synthesis.<topic>.source-<N>` where `<topic>` is a short kebab-case slug (e.g., `synthesis.rust-json-parsers.source-1`).

### 4a. Chunking for large sources (>30KB)

If any single source exceeds 30KB (~120,000 bytes):

1. **Split into chunks** of ~25KB at paragraph boundaries (double newline `\n\n`). Don't split mid-sentence.
2. **Store each chunk separately**:
   ```
   shared_state set key="synthesis.<topic>.source-<N>.chunk-1" value="<first ~25KB>"
   shared_state set key="synthesis.<topic>.source-<N>.chunk-2" value="<next ~25KB>"
   ```
3. **Dispatch one sub-agent per chunk** (same prompt as Step 5, but referencing the chunk key instead of the source key).
4. **Merge chunk summaries** before the final synthesis: combine `key_claims` and `key_quotes` from all chunks of the same source, deduplicate, and store the merged result as the source's summary.

### 5. Dispatch per-source sub-agents

For each source, dispatch a sub-agent with a focused extraction question. **One source per sub-agent** — sources are the natural unit of synthesis.

```
sub_agent: You are extracting key claims from a research source.

The source is stored in shared state under key "synthesis.<topic>.source-<N>".
Read it with: shared_state get key="synthesis.<topic>.source-<N>"

Research question: <your single-sentence question from step 1>

Extract the source's relevant claims and evidence. Reply with ONLY a JSON object (no markdown fences, no prose):
{
  "key_claims": ["claim 1", "claim 2", ...],
  "key_quotes": ["exact quote or close paraphrase with attribution", ...],
  "relevance": "high|medium|low",
  "confidence": 0.0-1.0,
  "source_type": "paper|blog|docs|discussion|code|other",
  "deeper_question": "a follow-up question if something is unclear, or null"
}
```

**Skills do not chain.** Sub-agents don't load this skill or any other; include the full question and shared-state key reference directly in the sub-agent's prompt.

### 5a. Handle sub-agent responses

Parse each sub-agent's response as JSON:

1. **Valid JSON with all fields**: Store the summary in shared state under `synthesis.<topic>.summary-<N>`.
2. **Malformed JSON but readable text**: Extract what you can. Construct a partial summary: `{"key_claims": ["<first 300 chars of response>"], "key_quotes": [], "relevance": "low", "confidence": 0.2, "source_type": "other", "deeper_question": null}`.
3. **Empty or errored**: Fall back to direct read of the source via `curl | head -100`. Produce a low-confidence summary manually from what you can see.

### 5b. Recurse on deeper questions

If a sub-agent returns a non-null `deeper_question` AND `confidence` < 0.5:

1. Dispatch another sub-agent with the narrower question, referencing the same shared-state key.
2. Merge the answer into the existing summary (append new claims, update confidence).

**Hard cap: recursion depth = 3.** That's: initial dispatch → 1st recursion → 2nd recursion. After depth 3, accept whatever you have. If you find yourself wanting depth 4, your original question was probably too vague — go back to Step 1 and narrow it.

### 6. Store summaries and dispatch synthesis sub-agent

After all per-source summaries are collected, store them together:

```
shared_state set key="synthesis.<topic>.summaries" value="<JSON array of all summaries>"
```

Then dispatch a **synthesis sub-agent** to compose the final answer:

```
sub_agent: You are synthesizing research from multiple sources into a composed answer.

The per-source summaries are stored in shared state under key "synthesis.<topic>.summaries".
Read them with: shared_state get key="synthesis.<topic>.summaries"

Research question: <your single-sentence question from step 1>

Compose a synthesis that:
1. Identifies areas of AGREEMENT across sources
2. Identifies areas of DISAGREEMENT or tension
3. Notes any gaps — important aspects of the question that no source addressed
4. Weighs claims by source confidence and relevance

Reply with ONLY a JSON object (no markdown fences, no prose):
{
  "answer": "3-5 paragraph composed answer to the research question",
  "consensus": ["claims that multiple sources agree on"],
  "disagreements": ["claims where sources conflict, with attribution"],
  "gaps": ["aspects of the question not covered by any source"],
  "confidence": 0.0-1.0,
  "source_count": <number of sources that contributed>
}
```

### 7. Use the synthesis

The synthesis sub-agent's `answer` field is your composed response. Use it to:

- Answer the user's original question
- Inform a technical decision in an evolve session
- Write a journal entry or issue comment with cited sources
- Add to `memory/learnings.jsonl` if the finding is novel and would change future behavior

Store the final synthesis in shared state under `synthesis.<topic>.result` so it can be referenced later in the session without re-running.

## Relationship to the research skill

| Scenario | Use |
|----------|-----|
| 1 source, < 5KB | `research` — direct curl + read |
| 1 source, ≥ 5KB | `synthesis` — store in shared state, sub-agent extract |
| 2 sources, both < 5KB | `research` — direct curl + read both |
| 2 sources, any ≥ 5KB | `synthesis` — sub-agent dispatch |
| 3+ sources, any size | `synthesis` — always |

The research skill finds and fetches sources. This skill processes and composes them. They're complementary: research is the scout, synthesis is the analyst.

## Pitfalls

- **Don't ask sub-agents to make decisions.** They extract claims and evidence; you (or the synthesis sub-agent) compose the answer. Per-source sub-agents that try to answer the whole question tend to hallucinate beyond their single source.
- **Don't dump multiple sources to one sub-agent.** One source per dispatch keeps the extraction focused and the JSON output reliable. Cross-source reasoning belongs in the synthesis step (Step 6).
- **Don't forget the recursion cap.** 3 is the hard limit. If you find yourself wanting depth 4, your research question was too broad — narrow it.
- **Don't synthesize without a question.** "Research topic X" is not a question. "What are the tradeoffs of X vs Y for use case Z?" is. The question shapes what each sub-agent extracts.
- **Don't over-fetch.** 3-7 sources is the sweet spot. More than 7 sources means you're probably not filtering enough — use search to find the 5 best sources, not all sources.
- **Don't re-synthesize within the same session.** If you've already synthesized a topic, the result is in shared state under `synthesis.<topic>.result`. Read it with `shared_state get` instead of re-dispatching sub-agents.
- **Skills do not chain.** Sub-agents can't load skills. Every sub-agent prompt must be self-contained — include the question and the shared-state key reference directly.

## Verification

A synthesis is "good enough" when ALL of:

- The answer addresses the **specific research question** (not a generic overview of the topic)
- **Multiple sources** contributed claims to the answer (not just one source restated)
- Areas of **agreement and disagreement** are explicitly identified
- The answer cites specific claims to specific sources (even if informally — "the docs.rs page says X while the blog post argues Y")
- The total work used **≤ N+2 sub-agent dispatches** where N is the number of sources (N per-source + 1 synthesis + 1 possible recursion)
- The work stayed within the **depth-3 recursion cap**

If the synthesis fails any of these, either add another source to fill the gap, or accept the partial result and note the open question.

## What this skill deliberately does NOT do

- **Does not find sources.** Source discovery is the `research` skill's job (search → evaluate → pick). This skill takes sources as input and produces synthesis as output.
- **Does not modify code.** Synthesis produces understanding, not changes. If the synthesis informs a code change, that's a separate task.
- **Does not write to the audit-log branch.** Synthesis results live in shared state for the current session only.
- **Does not replace human judgment.** The synthesis is a starting point for decisions, not a verdict. Cross-reference with your own experience and the project's context before acting on synthesis results.
