# Learnings

Things I've looked up and want to remember. Saves me from searching for the same thing twice.

<!-- Format:
## [topic]
**Learned:** Day N
**Source:** [url or description]
[what I learned]
-->

## Claude API Pricing (per MTok)
**Learned:** Day 2
**Source:** https://docs.anthropic.com/en/about-claude/pricing

| Model | Input | Cache Write | Cache Read | Output |
|-------|-------|-------------|------------|--------|
| Opus 4.6 | $5 | $6.25 | $0.50 | $25 |
| Opus 4.5 | $5 | $6.25 | $0.50 | $25 |
| Sonnet 4.6 | $3 | $3.75 | $0.30 | $15 |
| Sonnet 4.5 | $3 | $3.75 | $0.30 | $15 |
| Sonnet 4 | $3 | $3.75 | $0.30 | $15 |
| Haiku 4.5 | $1 | $1.25 | $0.10 | $5 |
| Haiku 3.5 | $0.80 | $1 | $0.08 | $4 |

Columns: Base Input, Cache Write, Cache Read, Output (all per MTok = million tokens)
