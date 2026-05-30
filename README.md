# weaft

<!-- markdownlint-disable MD033 MD013 -->
<p align="center"><em>Write an agent skill once. Compile it for every coding agent — adapted to what each one can actually do.</em></p>

<p align="center">
  <img src="https://img.shields.io/badge/rust-1.85%2B-orange?logo=rust" alt="Rust 1.85+" />
  <a href="docs/src/introduction.md"><img src="https://img.shields.io/badge/docs-mdBook-blue" alt="Documentation" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0--only%20OR%20Commercial-blue" alt="License: AGPL-3.0-only OR Commercial" /></a>
</p>
<!-- markdownlint-enable MD033 MD013 -->

You support more than one coding agent, so the same skill ends up living three times — a
Claude Code `SKILL.md`, a Cursor `.mdc` rule, an `AGENTS.md` block — and the copies drift
the moment you edit one. Worse, a hand-port is usually just a renamed file: it ignores that
Claude Code has subagents and a tool allowlist while Cursor gates permissions through its
own UI and runs agents read-only. **weaft** keeps **one** source project and compiles it per
host, branching on a capability matrix so every target gets output adapted to what it can
actually do — different instructions and different frontmatter, not a renamed file.

> **Status:** v0.1. Targets: `claude-code`, `cursor`, `agents-md`.

## Why weaft

- **One source of truth.** Maintain a single project; compile to Claude Code, Cursor, and
  the generic `AGENTS.md` convention. Edit once, recompile, every host stays in sync.
- **Capability-aware, not cosmetic.** Output branches on a host capability matrix exposed to
  your templates as `{{ host.* }}`. The same source yields *meaningfully different* output
  per target — different instructions and different frontmatter, not just a renamed file.
- **Fails loud, never silent.** Templates render in strict mode: an undefined `{{ host.* }}`
  is a hard error, so a capability typo breaks the build instead of quietly emitting nothing.
- **Skips what a host can't represent.** A subagent has no place in `AGENTS.md`, so weaft
  skips it *with a warning* rather than emitting something the host can't use.
- **Honest about limits.** Token budgets are weaft's own heuristics, not host-documented
  caps, and counts come from an approximate tokenizer — both say so out loud.
- **Lint before you ship.** `weaft lint` checks required fields, target validity, unused
  parameters, and token budgets; `--strict` turns budget overflow into a CI failure.

## The capability matrix

Every compile branches on this matrix. Each host's traits are exposed to your templates as
`{{ host.* }}`, so referencing a capability in a template — or adding one in `weaft-core` —
makes it available everywhere at once.

| Capability | `claude-code` | `cursor` | `agents-md` |
| --- | :---: | :---: | :---: |
| Subagents | ✅ via `Task` | ✅ via `Agent` | — |
| Bundled assets | ✅ | — | — |
| Subagent tool allowlist | ✅ | — *(read-only flag)* | — |
| Token budget *(heuristic)* | `8000` | `6000` | — |

Run `weaft targets` to print this live. Because `agents-md` has no subagent concept, weaft
skips subagents for that target with a warning instead of emitting a broken file.

## Quickstart

### Build from source

weaft is not yet published to crates.io; build it from the workspace (MSRV 1.85):

```bash
git clone https://github.com/DoctorMozg/weaft.git
cd weaft
cargo install --path crates/weaft-cli   # installs the `weaft` binary
weaft --help
```

Prefer not to install? `cargo build --release` leaves the binary at `target/release/weaft`.

### Compile the bundled example

The repo ships a runnable demo at [`examples/quickstart`](examples/quickstart):

```bash
weaft build --manifest-path examples/quickstart    # → dist/<target>/...
```

Now watch the capability matrix do real work — the **same** skill source, compiled two ways:

```bash
diff dist/claude-code/skills/safe-deleter/SKILL.md \
     dist/cursor/rules/safe-deleter.mdc
```

The two files differ in their *instructions* and their *frontmatter*, not just the filename.

### Start your own project

```bash
weaft init my-project     # scaffolds weaft.yaml + skills/ + agents/  (--minimal for the bare version)
weaft build --manifest-path my-project
```

## How a source project is shaped

```text
my-project/
├── weaft.yaml          # project info: name, version, targets.supported, parameters
├── skills/    *.md     # skills:    YAML frontmatter + Jinja2 body
├── agents/    *.md     # subagents: YAML frontmatter + Jinja2 body
├── fragments/ *.md.j2  # shared partials, pulled in with {% include %} / {% import %}
└── assets/             # optional; copied into Claude skill dirs (hosts that support it)
```

`weaft.yaml` declares the project name, version, the `targets.supported` list, and any
`parameters` your templates read (overridable at build time with `--param key=value`). Every
skill and subagent is a Markdown file: `---`-fenced YAML frontmatter on top, a
[MiniJinja](https://docs.rs/minijinja) body below. The body sees the whole capability matrix
as `{{ host.* }}`, so one body branches per target.

## Same source, different output

This is weaft's whole reason to exist — each demo below is **one** source file, compiled into
genuinely different artifacts.

### Skill — `safe-deleter`

[`examples/quickstart/skills/safe-deleter.md`](examples/quickstart/skills/safe-deleter.md)
has one permission step that branches on the host's permission model:

```jinja
## Permission step
{% if host.permission_model == "explicit" -%}
{{ host.ask_user_syntax }} List exactly which files you will delete and wait for an explicit "yes".
{%- elif host.permission_model == "implicit" -%}
{{ host.ask_user_syntax }} The host's own approval UI will gate the deletion — surface the full file list so the user can approve.
{%- endif %}
```

Compiled for **Claude Code** (`dist/claude-code/skills/safe-deleter/SKILL.md`):

```markdown
## Permission step
Pause and ask the user explicitly in chat before proceeding. List exactly which files you will delete and wait for an explicit "yes".
```

Compiled for **Cursor** (`dist/cursor/rules/safe-deleter.mdc`):

```markdown
## Permission step
Output a clear question in the chat panel and wait for the user's reply. The host's own approval UI will gate the deletion — surface the full file list so the user can approve.
```

Same source, different *behavior* — and Claude Code gets `allowed-tools:` frontmatter while
Cursor gets `globs:` / `alwaysApply:`.

### Subagent — `code-reviewer`

[`examples/quickstart/agents/code-reviewer.md`](examples/quickstart/agents/code-reviewer.md)
is one subagent. weaft knows Claude Code subagents use a **tool allowlist** while Cursor
subagents use a **`readonly`** flag, so the frontmatter *and* the body diverge:

| | Claude Code | Cursor |
| --- | --- | --- |
| frontmatter | `tools: [Read, Grep, Bash]`, `model: inherit` | `model: inherit`, `readonly: true`, `is_background: false` |
| body line | `Use only these tools: Read, Grep, Bash.` | `You run in read-only mode; report findings without editing files.` |

`agents-md` has no subagent format, so weaft **skips it with a warning** rather than emitting
something the host can't use.

## Commands

| command | what it does |
| --- | --- |
| `weaft init <name>` | scaffold a new project (`--target` repeatable, `--minimal`, `--path`) |
| `weaft build` | compile every skill + subagent for every supported target into `dist/` (`--target`, `--out`) |
| `weaft preview --target <id>` | render one target to stdout without writing files |
| `weaft lint` | required-field, target-validity, and unused-parameter checks, plus a token-budget pass (`--strict` fails on overflow) |
| `weaft tokens` | per-target token usage |
| `weaft targets` | print the capability matrix |

Every command accepts `--manifest-path` (a `weaft.yaml` or its directory). The rendering
commands — `build`, `preview`, `tokens` — also accept repeatable `--param key=value` to
override declared parameters.

## Honesty notes

- **Token budgets** (8000 Claude / 6000 Cursor) are weaft's own soft **heuristics**, not
  host-documented hard limits. They drive the budget lint and the `tokens` report only.
- **Token counts** use `cl100k_base` as an **approximation** — Claude's real tokenizer is not
  public. Expect ~5–10% drift on typical English + code. Treat counts as guidance.
- **Undefined `{{ host.* }}`** access is a **hard error** (strict mode), so a capability typo
  fails the build instead of silently emitting nothing.

## Documentation

The full guide is an [mdBook](https://rust-lang.github.io/mdBook/) under
[`docs/`](docs/src/introduction.md):

- [Introduction](docs/src/introduction.md) — what weaft is and the problem it solves.
- [Quickstart](docs/src/quickstart.md) — from clone to compiled `dist/`.
- [The capability matrix](docs/src/capability-matrix.md) — every `{{ host.* }}` field and how
  targets differ.
- [Subagents](docs/src/subagents.md) — the tool-allowlist vs. read-only dialects.

Render it locally with `mdbook serve docs`.

## Changelog

See [`CHANGELOG.md`](CHANGELOG.md) for notable changes between releases.

## License

weaft is **dual-licensed**:

- **[AGPL-3.0-only](LICENSE)** — free and open-source use; see `Cargo.toml` for the canonical
  SPDX identifier.
- **[Commercial license](LICENSE-COMMERCIAL.md)** — for embedding, redistribution, or
  hosted-service use that does not fit the AGPL-3.0's copyleft terms.
