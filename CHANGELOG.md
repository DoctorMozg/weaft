# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-05-29

### Added

- Project model: a `weaft.yaml` info file plus `skills/`, `agents/`, `fragments/`, and
  optional `assets/` directories. Frontmatter is YAML; bodies are Jinja2.
- **Capability matrix** (`weaft-core::capability`) for `claude-code`, `cursor`, and
  `agents-md`, exposed to templates as `{{ host.* }}`. Includes subagent capabilities
  (`supports_subagents`, `subagent_tool`, `agent_layout`, `agent_supports_tools`,
  `agent_supports_readonly`).
- Capability-aware **skill** compilation: Claude `SKILL.md` (with `allowed-tools`),
  Cursor `.mdc` (with `globs` / `alwaysApply`), and a single concatenated `AGENTS.md`.
- Capability-aware **subagent** compilation: Claude `agents/<name>.md` (tool allowlist)
  and Cursor `agents/<name>.md` (`readonly` / `is_background`). `agents-md` is skipped
  with a warning.
- Shared `fragments/` via Jinja `{% include %}` / `{% import %}`; strict-undefined
  rendering so `{{ host.* }}` typos fail the build.
- Parameters declared in `weaft.yaml`, overridable with `--param key=value`.
- CLI: `init`, `build`, `preview`, `lint`, `tokens`, `targets`.
- Lints: required fields, target validity (unknown id, asset/subagent unsupported),
  unused parameter, and a soft token budget (`--strict`).
- `examples/quickstart` plus snapshot and CLI integration tests.

### Notes

- Token budgets are weaft heuristics, not host-documented limits; token counts use an
  approximate (`cl100k_base`) tokenizer.

[0.1.0]: https://github.com/doctormozg/weaft/releases/tag/v0.1.0
