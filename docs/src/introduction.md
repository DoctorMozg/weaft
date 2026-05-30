# Introduction

**weaft** is a capability-aware compiler for agent skills and subagents. You maintain one
source project; weaft compiles it into host-specific files for multiple coding agents
(Claude Code, Cursor, generic `AGENTS.md`), adapting each artifact to what the target
host can actually do.

The core idea is the **capability matrix**: each host's traits are exposed to your
Jinja2 templates as `{{ host.* }}`, so one source produces *meaningfully different*
output per target — different instructions and different frontmatter, not just a renamed
file.

## Why a compiler?

Maintaining the "same" skill by hand for three hosts means three files that drift apart.
weaft makes the source single and the differences declarative: you branch on
capabilities (`{% if host.permission_model == "explicit" %}`) instead of forking files.

## What's in a project

```text
my-project/
├── weaft.yaml      # project info, supported targets, parameters
├── skills/         # *.md — YAML frontmatter + Jinja2 body
├── agents/         # *.md — subagents
├── fragments/      # *.md.j2 — shared partials
└── assets/         # optional, copied into Claude skill dirs
```

See [Quickstart](./quickstart.md) to build the bundled example.
