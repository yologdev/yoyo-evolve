---
name: explore-codebase
description: "RLM-style large-codebase comprehension — build a mental map of any codebase by dispatching sub-agents to explore regions without bloating main context"
tools: [bash, read_file, list_files, search, sub_agent, shared_state]
core: false
origin: yoyo
status: active
score: 0.66
uses: 2
wins: 2
last_used: "2026-05-22T02:49:39Z"
last_evolved: "2026-08-24"
parent_pattern_key: null
keywords: ["skills/explore-codebase", "explore.", "key_invariants"]
---

# Explore Codebase

You are building a **mental map** of an unfamiliar or large codebase region. The goal is structural comprehension — understanding what the code does, how it's organized, and what the key entry points and invariants are — without loading everything into your main context window.

This skill exists because codebases are too large to read in one prompt. The pattern (Recursive Language Model — see the RLM substrate section in CLAUDE.md) is: keep your root context small, dispatch sub-agents to read individual files or modules, and have each sub-agent return a structured summary. Synthesize the summaries into a coherent map.

This skill covers **any** codebase the agent encounters — forks, dependencies, unfamiliar regions, user projects. For analyzing yoyo's own source to find bugs and gaps, use `self-assess` instead.

## When to use

Trigger this skill when ANY of these hold:

- A planned task touches **>5 files** you haven't recently worked on
- A community issue references a feature or module you don't have a mental map for
- You're investigating a bug whose surface spans **multiple modules** you haven't read recently
- A new dependency is being introduced and you want to know its public API + key invariants before integrating
- A user explicitly asks you to explore, understand, or map a codebase
- `/add` brought in a large project and you need structural context before acting
- You're working in a fork and need to understand what the fork changed relative to upstream

## When NOT to use

- **Small known regions.** A single file ≤300 lines, or a function you wrote yesterday — just read it directly. Sub-agent overhead exceeds the savings.
- **Precise edits across files.** Refactoring needs mutual context (seeing all pieces at once), not summaries. Summaries lose the fidelity you need for surgical edits.
- **Sequential workflows with strong mutual context.** When each step depends on the full output of the previous step, fan-out doesn't help — you need serial reading.
- **The region is already known.** If you or the user can name the exact module and its API from memory, direct read is faster. Don't explore what you already understand.
- **You're inside a sub-agent at depth 3.** Stop. Return what you have. Do not dispatch further.

## Procedure

### 1. Identify the region

Define the exploration scope — be specific:

- A directory: `src/format/`
- A file glob: `src/agent_builder.rs src/main.rs src/tools.rs`
- A dependency: `~/.cargo/registry/src/*/yoagent-*/src/`
- A commit range: `git diff main..feature-branch --name-only`

If the scope is vague ("understand this project"), start with orientation (Step 2). If the scope is precise (a named set of files), skip to Step 3.

### 2. Orient — build a rough map

Before dispatching sub-agents, gather cheap structural signals directly:

```bash
# Project structure
find <root> -type f -name '*.rs' | head -50
# or use /map if available in the REPL

# README / docs
cat <root>/README.md 2>/dev/null | head -100

# File sizes (to plan dispatch)
wc -l <root>/src/*.rs | sort -rn | head -20

# Recent activity
git log --oneline -20 -- <root>/src/
```

From this, build a **file inventory** with rough sizes. Files >300 lines are candidates for sub-agent exploration. Files ≤300 lines can be read directly if needed later.

### 3. Decide: direct read or sub-agent?

For each file in the region:

- **≤5KB (roughly ≤150 lines)**: Read directly with `read_file` or `bash`. No sub-agent needed.
- **>5KB**: Dispatch a sub-agent (Step 4). Don't load large files into your main context.

If the total region is small enough to read directly (≤5 files, all ≤5KB), skip sub-agents entirely — just read and synthesize in your main context.

### 4. Dispatch per-file sub-agents

Store each file's content in shared state, then dispatch a sub-agent to summarize it. **One file per sub-agent** — files are the natural unit of structure in code.

#### 4a. Store the artifact

```
shared_state set key="explore.<region>.<filename>" value="<file contents>"
```

Namespace convention: `explore.<region>.<filename>` (e.g., `explore.format.markdown`, `explore.yoagent.agent`).

If a single file exceeds 30KB (~120,000 bytes), chunk it before storing:
- Split into chunks of ~80KB with 8KB overlap between consecutive chunks
- Store as `explore.<region>.<filename>.chunk-1`, `.chunk-2`, etc.
- Dispatch one sub-agent per chunk (same as analyze-trajectory's Section 3.5)

#### 4b. Dispatch the sub-agent

```
sub_agent: You are exploring a source file to build a structural summary.

The file is stored in shared state under key "explore.<region>.<filename>".
Read it with: shared_state get key="explore.<region>.<filename>"

Describe this file's structure in a JSON response. Reply with ONLY a JSON object (no markdown fences, no prose):
{
  "file": "<filename>",
  "purpose": "1 sentence: what this file does",
  "public_api": ["list of exported functions/structs/traits with 1-line descriptions"],
  "key_invariants": ["non-obvious behaviors, constraints, or assumptions"],
  "dependencies": ["other modules/crates this file depends on"],
  "dependents": ["who calls into this file, if visible from imports/use statements"],
  "complexity": "low|medium|high",
  "deeper_question": "a follow-up question if something is unclear, or null"
}
```

**Skills do not chain.** Sub-agents don't load this skill or any other; include the full question and shared-state key reference directly in the sub-agent's prompt.

#### 4c. Handle sub-agent responses

Parse each sub-agent's response as JSON:

1. **Valid JSON with all fields**: Store the summary in shared state under `explore.<region>.<filename>.summary` for the synthesis step.
2. **Malformed JSON but readable text**: Extract what you can. Construct a partial summary: `{"file": "<filename>", "purpose": "<first 200 chars of response>", "public_api": [], "key_invariants": [], "dependencies": [], "dependents": [], "complexity": "unknown", "deeper_question": null}`.
3. **Empty or errored**: Fall back to direct read of the file's first and last 50 lines. Produce a low-confidence summary manually.

### 5. Recurse on deeper questions

If a sub-agent returns a non-null `deeper_question` and `complexity` is `"high"`:

1. Dispatch another sub-agent with the narrower question, referencing the same shared-state key.
2. Merge the answer into the existing summary.

**Hard cap: recursion depth = 3.** That's: initial dispatch → 1st recursion → 2nd recursion. After depth 3, accept whatever you have. If you find yourself wanting depth 4, your initial scope was probably too broad — narrow the region and retry.

### 6. Synthesize into a mental map

After all per-file summaries are collected, dispatch a **synthesis sub-agent** (or do this in your main context if the total summary data is small enough, ≤5KB):

```
sub_agent: You are synthesizing per-file summaries into a structural map of a codebase region.

The following shared-state keys contain per-file summaries:
- explore.<region>.<file1>.summary
- explore.<region>.<file2>.summary
...

Read each summary, then produce a structural map as a JSON object:
{
  "region": "<region name>",
  "overview": "2-3 sentences: what this region does as a whole",
  "module_graph": ["<file-A> -> <file-B>: <relationship>", ...],
  "entry_points": ["the key functions/structs a caller would use"],
  "invariants": ["cross-file constraints or assumptions"],
  "risk_areas": ["files or interactions that look fragile or complex"],
  "open_questions": ["things the summaries couldn't resolve"]
}
```

### 7. Use the map

The mental map is your working context for the rest of the session. Reference it when:

- Planning which files to modify for a task
- Estimating the blast radius of a change
- Deciding whether a refactor is safe
- Explaining code structure to a user or in a journal entry

Store the final map in shared state under `explore.<region>.map` so sub-agents in later steps can reference it without re-exploring.

## Pitfalls

- **Don't explore what you already know.** If you wrote the code recently or have it in active memory, skip this skill. It's for building *new* understanding, not confirming existing knowledge.
- **Don't ask sub-agents to make decisions.** They summarize structure; you decide what to do with it. Sub-agents that plan or recommend tend to drift.
- **Don't dump multiple files to one sub-agent.** One file per dispatch keeps the JSON output reliable and the summary focused. The synthesis step is where cross-file reasoning happens.
- **Don't forget the recursion cap.** 3 is the hard limit. If your region needs depth 4, the region is too broad — split it.
- **Don't explore before acting on small tasks.** If the task is "fix this one function," reading that function directly is faster than exploring the whole module. Match the tool to the task size.
- **Don't re-explore within the same session.** If you've already explored a region, the summaries are in shared state. Read them with `shared_state get` instead of re-dispatching sub-agents.
- **Per-file, not per-byte.** Unlike analyze-trajectory (which chunks CI logs by byte offset), this skill fans out by file. Files are the natural structural unit in codebases. Only chunk within a file if it exceeds 30KB.

## Verification

An exploration is "good enough" when ALL of:

- The map names **concrete files and functions** (not "some module that handles X")
- Each file in the region has a summary (even if low-confidence for some)
- The module graph shows **how files relate** (who calls whom, who depends on whom)
- Entry points are identified — a caller knows where to start
- The total exploration used **≤ N+2 sub-agent dispatches** where N is the number of files explored (N per-file + 1 synthesis + 1 possible recursion)
- The work stayed within the depth-3 recursion cap

If the map fails any of these, narrow the region and re-explore the gap, or accept the partial result and document open questions.

## What this skill deliberately does NOT do

- **Does not modify code.** Exploration produces understanding, not changes. The actual edits are a separate task.
- **Does not find bugs.** That's `self-assess`. This skill builds the map; self-assess uses the map to find problems.
- **Does not auto-create documentation.** If the map is worth preserving as docs, that's a separate decision outside this skill's scope.
- **Does not write to the audit-log branch.** The exploration results live in shared state for the current session only.
