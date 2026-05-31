# Capability matrix

The capability matrix is weaft's load-bearing artifact. Every target is described by a
static `HostCapabilities` value (`weaft-core::capability`), serialized into your templates
as `{{ host.* }}`.

| capability | claude-code | cursor | agents-md | opencode | codex |
| --- | --- | --- | --- | --- | --- |
| `supports_tool_allowlist` | yes | no | no | no | no |
| `supports_assets` | yes | no | no | no | yes |
| `supports_subagents` | yes (`Task`) | yes (`Agent`) | no | yes (`Task`) | yes |
| `agent_supports_tools` | yes | no | — | no | no |
| `agent_supports_readonly` | no | yes | — | no | no |
| `permission_model` | explicit | implicit | explicit | implicit | sandboxed |
| `ask_user_support` | structured | non_blocking | none | structured | none |
| `ask_user_in_subagents` | no | yes | — | yes | — |
| `max_skill_tokens` | 8000 | 6000 | — | — | — |
| `skill_layout` | `SKILL.md` | `.mdc` | `AGENTS.md` | `SKILL.md` | `SKILL.md` |

Run `weaft targets` to print the live matrix.

## Using it in templates

```jinja
{% if host.permission_model == "explicit" %}
{{ host.ask_user_syntax }} Confirm before deleting.
{% elif host.permission_model == "implicit" %}
Surface the file list; the host's approval UI gates the action.
{% endif %}
```

Rendering uses **strict undefined**: referencing a field that doesn't exist
(`{{ host.subagnets }}`) is a hard error, so capability typos fail the build instead of
silently producing empty output.

## Honesty

- `max_skill_tokens` values are weaft **heuristics**, not host-documented limits.
- Token counts use `cl100k_base` as an **approximation** of Claude's non-public
  tokenizer (~5–10% drift on typical content).
