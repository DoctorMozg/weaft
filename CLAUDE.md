# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What weaft is

weaft is a **capability-aware compiler** for agent skills and subagents. One source
*project* compiles to host-specific files for multiple coding-agent hosts (`claude-code`,
`cursor`, `agents-md`). The compiler branches on a **host capability matrix** so a single
source produces *meaningfully different* output per target — different instructions and
different frontmatter, not just a renamed file. Read `README.md` for the two showcase
demos (the `safe-deleter` skill and the `code-reviewer` subagent).

## Commands

```bash
cargo build                                   # build the workspace
cargo test                                    # unit + snapshot + CLI integration tests
cargo test -p weaft-cli safe_deleter_cursor   # run a single test by name
cargo clippy --all-targets -- -D warnings     # lint (CI-equivalent; must be clean)
cargo fmt --check                             # formatting gate (run `cargo fmt` to fix)

# Run the CLI
cargo run -p weaft-cli -- build   --manifest-path examples/quickstart
cargo run -p weaft-cli -- preview --target cursor --manifest-path examples/quickstart
cargo run -p weaft-cli -- lint    --manifest-path examples/quickstart   # --strict to fail on budget
cargo run -p weaft-cli -- tokens  --manifest-path examples/quickstart
cargo run -p weaft-cli -- targets                                       # print the capability matrix
```

**Snapshot tests** use `insta`. After an intentional output change, regenerate with
`INSTA_UPDATE=always cargo test` and review the `.snap` diffs before committing.
Snapshots live in `crates/weaft-cli/tests/snapshots/`.

**End-to-end demo check** (the project's reason to exist — keep these diffs meaningful):

```bash
cargo run -p weaft-cli -- build --manifest-path examples/quickstart
diff dist/claude-code/skills/safe-deleter/SKILL.md dist/cursor/rules/safe-deleter.mdc
diff dist/claude-code/agents/code-reviewer.md      dist/cursor/agents/code-reviewer.md
```

## Architecture

Three crates; the dependency direction is **cli → targets → core** (core depends on
nothing internal). A compile is a three-stage pipeline: **parse → render → emit**.

- **`weaft-core`** — the library. Key modules:
  - `capability.rs` — **the load-bearing artifact.** `HostCapabilities` (a `Serialize`
    struct) + one `const` per host. It lives here, *not* in `weaft-targets`, specifically
    to avoid a core↔targets dependency cycle (targets needs core's IR; core's
    compile/lint needs the capability matrix). The whole struct is exposed to templates
    as `{{ host.* }}`, so **adding a field to `HostCapabilities` instantly makes it
    available in every template** — no other wiring.
  - `ir.rs` — `Project { info, skills, agents, root }`. `Targets { supported, overrides }`
    carries per-target override blocks as raw `serde_yaml::Value` (flattened), which each
    backend parses itself.
  - `parse.rs` — `load_project`: reads `weaft.yaml`, globs `skills/*.md` + `agents/*.md`,
    splits `---`-fenced YAML frontmatter from the Jinja body.
  - `compile.rs` — `render_skill` / `render_agent` via minijinja with
    **`UndefinedBehavior::Strict`** (a `{{ host.* }}` typo is a hard error, by design) and
    a `path_loader` rooted at the **project root** (so includes use the `fragments/`
    prefix).
  - `lint/` — free-function passes aggregated by `run()`: `required`, `targets`
    (unknown id / asset+subagent unsupported), `unused`, `budget`.
  - also `params.rs`, `tokens.rs`, `diag.rs`, `fs.rs`.
- **`weaft-targets`** — the `Target` trait (`emit_skill` / `emit_agent`) + one backend per
  host (`claude_code`, `cursor`, `agents_md`). Backends receive the *already-rendered*
  body and only handle format/layout/frontmatter. `EmittedFile.concatenate` lets multiple
  artifacts merge into one file (the single `AGENTS.md`).
- **`weaft-cli`** — `commands/` with one module per subcommand. `build.rs` holds the
  shared `compile_target` helper (also used by `preview`/`tokens`), merges emitted files,
  and copies `assets/` into Claude skill dirs.

### Extending

- **New target**: add a `HostCapabilities` const + `by_id`/`all` arms in `capability.rs`,
  implement `Target` in `weaft-targets`, register it in `target_by_id`/`all_targets`.
- **New subagent dialect detail**: it is driven by capability flags
  (`agent_supports_tools` vs `agent_supports_readonly`), branched on in both the backend
  frontmatter emit and the template body — keep those two in sync.

## Repo-specific invariants (don't regress these)

- **Honesty about budgets/tokenizers.** `max_skill_tokens` (8000/6000) are weaft
  *heuristics*, not host limits; token counts use `cl100k_base` as an *approximation* of
  Claude's non-public tokenizer. The disclaimers in `tokens`/`targets` `--help` and the
  README must stay.
- **Frontmatter key casing is deliberate**: Claude emits `allowed-tools` (hyphen) though
  the override key is `allowed_tools`; Cursor emits `alwaysApply` (camelCase).
- Targets that can't represent an artifact **skip with a warning** rather than emit
  something broken (e.g. subagents on `agents-md`).

## Git

Develop on branch `claude/weaft-v0-1-spec-pKbqm`. Licensed AGPL-3.0-only, with a
commercial option (see `LICENSE-COMMERCIAL.md`).

<!-- mz-gov:governance-policy v=unknown start -->
<!-- source: governance-policy.md -->
## Development Governance Policy

This project records the *why* behind substantial design decisions and keeps AI-proposed decisions traceable and human-approved. The `govern` pipeline does the work; this policy says when to reach for it.

## When governance applies

Run `/govern <the decision>` for a change that is **substantial AND ambiguous** — both must hold.

- **Substantial** — it changes externally observable behavior, a public API or wire format, a data schema or migration, a dependency, a module boundary, or the security posture; or it is **hard to reverse** (a one-way door).
- **Ambiguous** — more than one reasonable approach a competent engineer could defend.

**Exempt** (no artifact needed): a behavior-preserving refactor; an objective improvement along a numeric axis (speedup, warning removal, dependency bump with no API change); a dev-invisible / tool-internal change; a throwaway prototype or spike.

When in doubt, run `/govern` — it tells you if no artifact is needed and records a one-line exemption note. Manufacturing ambiguity that isn't there is not the goal; skipping a genuine one-way door is the failure.

## Where artifacts live

Durable governance artifacts are committed to the repository:

- `docs/decisions/` — ADRs (a decision made; record the why)
- `docs/rfcs/` — RFC/RFD (a decision still open; structure the debate)
- `docs/design/` — design docs (a decision made; specify the build)

Transient pipeline state lives under `.mz/task/` and is not committed.

## AI-provenance and human sign-off (non-negotiable)

Every governed decision carries an AI-provenance block — the proposing **agent**, the exact **model id**, a **timestamp**, and the **trigger** — and a recorded **human sign-off**. An agent **proposes**; a human **approves**. A decision artifact without a recorded sign-off is **not done**.

## How to act (be proactive)

Before you encode a substantial, ambiguous design choice into code, invoke `/govern <the decision>` yourself. Do not silently bake such a decision into a diff and leave the reasoning unrecorded — surface it through the pipeline so it gets a real artifact and a human's name on it.
<!-- mz-gov:governance-policy end -->
