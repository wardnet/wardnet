---
name: agents-md-creator
description: |
  Use this skill whenever you need to create or update an AGENTS.md file for a repository or module.
  This includes when a user asks to set up AI agent instructions, configure coding agent behavior,
  or create documentation that guides AI assistants working in a codebase.
  Make sure to use this skill whenever the task involves AGENTS.md, copilot-instructions, or similar
  agent configuration files.
---

# AGENTS.md Creator

Create effective AGENTS.md files that give AI coding agents the context they need to work well in a codebase.

## Overview

An AGENTS.md file is the primary way to instruct AI coding agents about a project's conventions, structure,
boundaries, and workflows. A well-written AGENTS.md dramatically improves agent output quality by reducing
guesswork and enforcing project-specific standards.

## Workflow

1. **Analyze the codebase** — Before writing anything, explore the repository to understand its structure,
   tech stack, conventions, and existing documentation (README, CONTRIBUTING, etc.)
2. **Determine scope** — Is this for a repo root or a specific module? Root-level files cover broad guidance;
   module-level files cover module-specific details.
3. **Draft the AGENTS.md** — Follow the structure and guidelines in the references.
4. **Review with the user** — Present the draft and iterate based on feedback.

## Key Principles

- **Be specific, not generic** — Say "Kotlin 1.9 with Gradle Kotlin DSL and Spring Boot 3" not "JVM project"
- **Show, don't tell** — One real code snippet beats three paragraphs of description
- **Commands first** — Place executable commands (build, test, lint) near the top for easy reference
- **Progressive disclosure** — Keep the main AGENTS.md concise; link to detailed reference files for deep dives
- **Set clear boundaries** — Explicitly state what agents should and should not do

## Structure Guide

Use the following sections. Not all are required — include only what's relevant to the project.

Refer to [references/structure.md](references/structure.md) for the detailed structure template and examples.

## File Organization

### AGENTS.md Placement
- **Root AGENTS.md** (or `.github/AGENTS.md`) — Repo-wide guidance: tech stack, build commands, code style, git workflow
- **Module/directory AGENTS.md** — Module-specific patterns, dependencies, and conventions that override or extend root guidance

### Reference Files
When the AGENTS.md needs to reference detailed documentation (architecture deep dives, API patterns, schema definitions, etc.),
place those files in an `.agents/references/` directory at the same level as the AGENTS.md file.

```
# Root-level example
├── AGENTS.md
├── .agents/
│   └── references/
│       ├── architecture.md
│       ├── code-style.md
│       └── api-patterns.md

# Module-level example
├── modules/my-module/
│   ├── AGENTS.md
│   └── .agents/
│       └── references/
│           ├── module-patterns.md
│           └── data-model.md
```

The AGENTS.md should link to these reference files with clear guidance on when to read them.
Keep the main AGENTS.md under ~200 lines and offload details into references.

## Boundary Tiers

Define agent permissions using three tiers:

- **Always do** — Actions the agent should take without asking (e.g., run linter, follow naming conventions)
- **Ask first** — Actions that need user confirmation (e.g., delete files, modify CI config, change public APIs)
- **Never do** — Hard limits (e.g., commit secrets, skip tests, modify vendor code)

## Anti-Patterns

- Walls of text with no structure or headings
- Generic advice that applies to any project ("write clean code")
- Contradicting existing README or CONTRIBUTING docs
- Overly restrictive rules that prevent agents from being useful
- Duplicating information already in other project docs — link to them instead
