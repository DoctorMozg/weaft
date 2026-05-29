# Subagents

Subagents live in `agents/*.md` and are compiled alongside skills. They are the clearest
demonstration of capability-aware compilation, because the two hosts that support them
use **different frontmatter dialects**.

## One source

```markdown
---
name: code-reviewer
description: Expert code reviewer. Use proactively after code changes.
tools:
  - Read
  - Grep
  - Bash
model: inherit
readonly: true
is_background: false
---
You are an expert code reviewer for {{ project.name }}.

{% if host.agent_supports_tools -%}
Use only these tools: {{ agent.tools | join(", ") }}.
{%- elif host.agent_supports_readonly -%}
You run in read-only mode; report findings without editing files.
{%- endif %}
```

## Two outputs

**Claude Code** (`agents/code-reviewer.md`) — uses a tool allowlist:

```markdown
---
name: code-reviewer
description: Expert code reviewer. Use proactively after code changes.
tools:
- Read
- Grep
- Bash
model: inherit
---
...
Use only these tools: Read, Grep, Bash.
```

**Cursor** (`agents/code-reviewer.md`) — uses `readonly` instead:

```markdown
---
name: code-reviewer
description: Expert code reviewer. Use proactively after code changes.
model: inherit
readonly: true
is_background: false
---
...
You run in read-only mode; report findings without editing files.
```

## Unsupported hosts

`agents-md` has no subagent concept. weaft **skips** the subagent and emits a warning
rather than producing a file the host can't interpret. The author writes the frontmatter
once, with both `tools` and `readonly`; each target takes only the fields it understands.
