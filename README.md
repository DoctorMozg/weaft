# weaft

**weaft** is a capability-aware compiler for agent skills and subagents. You maintain
**one** source project; weaft compiles it into host-specific files for multiple coding
agents — Claude Code, Cursor, and the generic `AGENTS.md` convention — adapting each
artifact to what the target host can actually do.

The load-bearing idea is a **capability matrix**. Each host's traits (subagent support
and dialect, permission model, token budget, file layout, frontmatter dialect) are
exposed to your templates as `{{ host.* }}`. A single source therefore produces
*meaningfully different* output per target — not just a renamed file, but different
instructions and different frontmatter.

> **Status:** v0.1. Targets: `claude-code`, `cursor`, `agents-md`.

## Project layout

```
my-project/
├── weaft.yaml          # project info: name, version, targets.supported, parameters
├── skills/   *.md      # skills: YAML frontmatter + Jinja2 body
├── agents/   *.md      # subagents: YAML frontmatter + Jinja2 body
├── fragments/ *.md.j2  # shared partials, pulled in with {% include %} / {% import %}
└── assets/             # optional; copied into Claude skill dirs (hosts that support it)
```

```
weaft build --manifest-path my-project        # → dist/<target>/...
```

## The skill demo

`examples/quickstart/skills/safe-deleter.md` has one permission step:

```jinja
{% if host.permission_model == "explicit" %}
{{ host.ask_user_syntax }} List exactly which files you will delete and wait for "yes".
{% elif host.permission_model == "implicit" %}
{{ host.ask_user_syntax }} The host's approval UI will gate the deletion — surface the list.
{% endif %}
```

Compiled for **Claude Code** (`dist/claude-code/skills/safe-deleter/SKILL.md`):

```markdown
## Permission step
Pause and ask the user explicitly in chat before proceeding. List exactly which files
you will delete and wait for an explicit "yes".
```

Compiled for **Cursor** (`dist/cursor/rules/safe-deleter.mdc`):

```markdown
## Permission step
Output a clear question in the chat panel and wait for the user's reply. The host's own
approval UI will gate the deletion — surface the full file list so the user can approve.
```

Same source, different *behavior* — plus Claude gets `allowed-tools:` frontmatter while
Cursor gets `globs:` / `alwaysApply:`.

## The subagent demo

`examples/quickstart/agents/code-reviewer.md` is one subagent. weaft knows Claude Code
subagents use a **tool allowlist** while Cursor subagents use a **`readonly`** flag, so:

| | Claude Code (`agents/code-reviewer.md`) | Cursor (`agents/code-reviewer.md`) |
|---|---|---|
| frontmatter | `tools: [Read, Grep, Bash]`, `model` | `model`, `readonly: true`, `is_background` |
| body line | "Use only these tools: Read, Grep, Bash." | "You run in read-only mode; report findings without editing files." |

`agents-md` has no subagent format, so weaft **skips it with a warning** rather than
emitting something the host can't use.

## Commands

| command | what it does |
|---|---|
| `weaft init <name>` | scaffold a new project (`--target`, `--minimal`) |
| `weaft build` | compile every skill + subagent for every supported target into `dist/` |
| `weaft preview --target <id>` | render to stdout without writing files |
| `weaft lint` | required-field, target-validity, unused-parameter, missing-example, and token-budget checks (`--strict`) |
| `weaft tokens` | per-target token usage |
| `weaft targets` | print the capability matrix |

Most commands take `--manifest-path` (a `weaft.yaml` or its directory) and repeatable
`--param key=value`.

## Honesty notes

- **Token budgets** (8000 Claude / 6000 Cursor) are weaft's own soft **heuristics**, not
  host-documented hard limits. They drive the budget lint and the `tokens` report only.
- **Token counts** use `cl100k_base` as an **approximation** — Claude's real tokenizer is
  not public. Expect ~5–10% drift on typical English+code. Treat counts as guidance.
- Undefined `{{ host.* }}` access is a **hard error** (strict mode), so a capability typo
  fails the build instead of silently emitting nothing.

## Building from source

```
cargo build
cargo test                              # unit + snapshot + CLI tests
cargo run -p weaft-cli -- build --manifest-path examples/quickstart
```

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your
option.
