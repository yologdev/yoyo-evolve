# Skills

Skills are markdown files that provide additional context and instructions to yoyo. They're loaded at startup and added to the agent's context.

## Usage

```bash
yoyo --skills ./skills
```

You can pass multiple skill directories:

```bash
yoyo --skills ./skills --skills ./my-custom-skills
```

## What is a skill?

A skill file is a markdown file with YAML frontmatter. It contains instructions, rules, or context that the agent should follow. For example:

```markdown
---
name: rust-expert
description: Rust-specific coding guidelines
tools: [bash, read_file, edit_file]
---

# Rust Guidelines

- Always use `clippy` before committing
- Prefer `?` over `.unwrap()` in production code
- Write tests for every public function
```

## Built-in skills

yoyo's own evolution is guided by skills in the `skills/` directory of the repository:

- **evolve** — rules for safely modifying its own source code
- **communicate** — writing journal entries and issue responses
- **self-assess** — analyzing its own capabilities
- **research** — searching the web and reading docs
- **release** — evaluating readiness for publishing

## Error handling

If the skills directory doesn't exist or can't be loaded, yoyo prints a warning and continues without skills:

```
warning: Failed to load skills: ...
```

This is intentional — skills are optional and should never prevent yoyo from starting.
