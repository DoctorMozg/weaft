---
status: "accepted"
date: 2026-06-09
deciders: drmozg
human_signoff: "drmozg (2026-06-09)"
---

<!-- mz-gov:agdr start -->
agdr_id: agdr-task-2026_06_09_govern_skill_dir_layout
timestamp: 2026-06-09T00:00:00Z
agent: gov-artifact-writer
model: claude-sonnet-4-6
trigger: user-prompt
status: approved
human_signoff: drmozg (2026-06-09)
<!-- mz-gov:agdr end -->

# Directory-based skill layout with single-root-per-skill-type loader

## Context and Problem Statement

weaft's generic `Artifact`/`ArtifactMeta` model and per-skill-directory *output* layout
(`skills/{name}/SKILL.md`) are already implemented; the output side is already migrated.
This ADR changes the *input* side: the parser and include-resolution loader.

weaft currently parses skill source files as flat Markdown files (`skills/name.md`).
The v1 goal is to compile the mozg-pipelines ecosystem (12 plugins, 53 skills) directly
using `weaft build`. Those plugins use a directory-based layout: each skill lives in its
own folder (`skills/name/SKILL.md`) with detailed per-phase instructions in subdirectory
files (`skills/name/phases/*.md`). The progressive-disclosure pattern — a compact root
SKILL.md delegating to detailed phase files — is an intentional authoring convention.

The mozg-pipelines skills currently use no Jinja include syntax (grep confirmed: 0
occurrences). Migration to weaft source will require adding `{% include "phases/..." %}`
calls to each SKILL.md that delegates to phase files. Under the directory layout, these
includes resolve relative to each skill's own directory — keeping references short and
the skill directory self-contained. Under a flat-file layout, authors would need full
paths from the project root.

This decision fixes the parse interface and loader contract that all future weaft skill
authors will write against.

## Decision Drivers

- v1 migration goal: compile mozg-pipelines plugins without hand-rewriting their layout.
- Authoring ergonomics: short relative include paths (`phases/research.md`) for directory
  skills; existing `{% include "fragments/..." %}` for flat skills and shared content.
- Backward compatibility: existing flat-file skill projects must continue to build without
  modification.
- Resolution safety: the include mechanism must not silently compile wrong content.
  A missing phase file must be a loud, build-stopping error.
- Interface stability: once plugin authors adopt the directory layout, removing it is a
  breaking change.

## Considered Options

**A. Directory layout + ordered two-root loader `[skill_dir, project_root]`**

Skills may live at `skills/name/SKILL.md`. The Jinja loader resolves an include by
probing `skill_dir` first, then falling back to `project_root`. Silent wrong-content
risk: if skill A lacks `phases/research.md` but a same-named file exists at project
root (belonging to skill B), skill A compiles with skill B's content and no error fires.

**B. Directory layout + single-root-per-skill-type loader** (selected)

Skills may live at `skills/name/SKILL.md`. The Jinja environment is built with a single
loader root per skill, determined by the skill's source type:
- Flat skill `skills/name.md` → `loader_root = project_root` (unchanged from today)
- Directory skill `skills/name/SKILL.md` → `loader_root = skill_dir`

No fallback probe exists. A missing include is always a hard render error. Shared
cross-skill content in directory-layout skills is accessed via the fragment pre-registration
mechanism (see Spec §6).

**C. Directory layout + namespaced `@skill/` prefix includes**

Accepts `skills/name/SKILL.md`. Authors write `{% include "@skill/phases/research.md" %}`.
Unambiguous and single-root, but requires every include line to carry the `@skill/` prefix.

**D. Flat files only + `fragments/` convention for phase content**

Skills stay as flat `skills/name.md`. Phase files go at `fragments/phases/name/research.md`.
Zero new loader/parse/lint surface; established `fragments/` convention reused. Skill not
self-contained; include paths verbose.

**E. Status quo — flat-only compiler + one-time author-side flatten**

Migrate 53 mozg-pipelines skills by flattening all phase content into a single flat file.
Zero new compiler surface. One-time migration work; but ongoing recurring flatten friction
as mozg-pipelines keeps authoring directory-style upstream.

## Decision Outcome

Chosen option: **B — directory layout with single-root-per-skill-type loader**.

The single-root-per-type design is the only option that simultaneously satisfies resolution
safety (hard error on any missing include, no silent fallback), authoring ergonomics (short
relative paths), and flat-skill backward compat (their loader root IS `project_root`, so
existing `{% include "fragments/..." %}` includes work without change). Options A and its
reservation-prefix variant both reintroduce a two-root fallback that creates silent
wrong-content risk. Options D and E require verbose full-path includes. Option C adds
per-include noise.

### Specification

1. **Parser** (`parse.rs`): `markdown_paths` / `load_kind_dir` gains two-mode collection:
   - flat: all `*.md` direct children of `skills/` (existing behavior)
   - directory: all `SKILL.md` files one level down inside `skills/*/` (new descent)
   
   Collection is sorted by `(kind.index(), source_path)` — the existing sort key covers
   the new paths because `source_path` is unique per `SKILL.md` (confirmed: sort is
   sufficient, no new sort logic required). The descent is one level only; files deeper
   than `SKILL.md` (phases, shared) are not registered as skill artifacts.

2. **Skill-type detection and `loader_root` derivation** (`compile.rs`): determined
   per-skill from `artifact.source_path.file_name()`:
   - `"SKILL.md"` → directory skill → `loader_root = source_path.parent()` (= `skills/name/`)
   - anything else → flat skill → `loader_root = project_root`
   
   Detection is per-skill, not per-project. A project may freely mix flat and directory
   skills in the same `skills/` directory; each skill is classified independently.
   Per-project enforcement (e.g. a `weaft.yaml` `skill_layout: flat|directory|mixed` field
   that hard-errors on unexpected layout) is deferred.

   `compile::render` already receives `artifact` (with `source_path`) and `project_root`,
   so `loader_root` derivation requires no signature change to `compile::render` or its
   four consumers (`build.rs:138`, `tokens.rs:55`, `lint/budget.rs:53`,
   `compile_tests.rs:55`). The derivation is internal to `compile.rs`.

3. **Custom single-root loader**: a hand-written `minijinja::Loader` closure rooted at
   `loader_root`. The stock `path_loader` (single-root) suffices for both cases — for flat
   skills it receives `project_root` (unchanged from today); for directory skills it
   receives `skill_dir`. No ordered multi-root probe, no fallback.

4. **Fragment pre-registration for directory-skill shared content**: the `fragments/`
   directory at project root is scanned at environment setup, and its files are pre-registered
   as named templates under the `fragments/` prefix, using `env.add_template_owned(name, body)`
   (minijinja `environment.rs:198`, which accepts `Into<Cow<'source, str>>` for runtime
   strings). Note: `add_template` at `compile.rs:38` requires a `'static` `&str` and is used
   only for the `const ASK_J2`; runtime-scanned file contents must use `add_template_owned`.
   This makes `{% include "fragments/shared.md" %}` work from both flat and directory-layout
   skills without any loader fallback — the template is looked up by name in the named-template
   table (minijinja `loader.rs:125-141`: named templates resolve before the loader is queried).
   This also means `fragments/` as a subdirectory name inside a skill dir has no special
   significance and is not reserved.
   
   Note on flat-skill backward compat: for any `fragments/`-prefixed include, resolution
   goes through the named-template table (not the loader), for both flat and directory skills.
   Flat skills remain byte-for-byte compatible because the table is built from the same
   project-root `fragments/` files — the output is identical, though the resolution path
   differs from the prior all-loader implementation.

5. **Missing-include behavior**: a missing include is a hard render error under
   `UndefinedBehavior::Strict` (already set at `compile.rs:35`). No silent fallback.
   No special lint pass needed for this case — the render itself aborts.

6. **Coexistence rule**: if both `skills/name.md` and `skills/name/SKILL.md` exist,
   the directory form wins; the flat file is shadowed. A lint `duplicate_skill` warning
   fires (hard error under `--strict`).

7. **Mis-named directory diagnostic**: if `skills/name/` contains `.md` files but no
   `SKILL.md`, emit a lint warning ("directory `skills/name/` has no SKILL.md; ignored").
   Evidence: all 53 mozg-pipelines skills use `SKILL.md` uppercase (grep confirmed).

8. **Phase files**: files inside `skills/name/` other than `SKILL.md` are not registered
   as skill artifacts. They are inert unless `{% include %}`d from the skill body. Their
   content IS counted in the skill's token budget (because `budget.rs:53` calls
   `compile::render`, which expands includes before counting). Directory skills are budgeted
   on root SKILL.md + all included phase content combined; a directory skill with a small
   SKILL.md and large phase files will breach `max_skill_tokens` where the equivalent flat
   files would each have their own budget.

   **Known v1 limitation**: `unused::check` (`lint/unused.rs:14`) builds its haystack from
   `artifact.body` only. A `{{ params.x }}` used exclusively inside a `phases/*.md` file
   will be flagged as an unused parameter — a false-positive for directory-layout skills.
   This is accepted as a known limitation for v1. Expanding the haystack to include resolved
   phase content (by running a pre-render pass) is deferred.

9. **Determinism**: the directory descent is new collection logic alongside `markdown_paths`
   (the existing collector is flat-only and cannot do descent). The new paths feed into the
   existing `(kind.index(), source_path)` sort in `load_artifacts`, which is sufficient
   because `source_path` is unique per `SKILL.md` — confirmed. The sort handles ordering;
   the descent is new code.

10. **Derived `loader_root` path isolation**: `loader_root` is an absolute path used only
    for include resolution; it must never reach emitted bytes. The `determinism.rs:142`
    no-absolute-path guard is the regression check.

11. **Regression anchors**: `examples/quickstart/skills/safe-deleter.md:31` and
    `examples/quickstart/skills/hello.md:12` both use `{% include "fragments/footer.md.j2" %}`.
    After this change, both resolve via the named-template table (Spec §4), not via loader.
    Output is byte-identical because the table is built from the same project-root files.
    Pre-merge gate: snapshot test proves both render byte-identically to the committed `.snap`
    after the loader change, for all three targets. These are the showcase diff demos (`README.md`).

12. **New lint passes** (`lint/`): `duplicate_skill` and `missing_skill_md` registered in
    `run()`. `duplicate_skill` is warn-by-default, hard error under `--strict`. The `--strict`
    flag, currently consumed only by `budget::check`, now also gates layout warnings. The
    `compile.rs` module doc (`:1–6`) must be updated to describe the per-skill-type loader.

13. **Scope**: Skills kind only. `Instruction` and `Subagent` kinds keep flat-file-only
    parsing. The kind-uniform `load_artifacts` loop gains a Skill-only branch.

14. **Target backend compatibility**: `{% include %}` directives are expanded by Jinja at
    render time inside `compile::render`, before the compiled body reaches any target
    backend. All backends (`claude_code`, `cursor`, `agents_md`, and future targets) always
    receive a flat rendered string — they never see the source directory structure or phase
    files. No backend-level compaction, directory handling, or special-casing is needed.
    A directory skill and an equivalent flat skill produce identical rendered output for
    every target.

### Escape hatch / evolution

This is a pre-1.0 project; the stated policy (`CLAUDE.md`, `weaft.yaml` versioning) is that
breaking changes are permitted before 1.0. The escape hatch is therefore: **flat-file
`skills/name.md` is the stated long-term migration target if directory layout is ever
retracted**. This is not a pre-1.0 "permanent" guarantee, but a commitment that
retraction is never silent — if directory layout is removed, a flat-file migration guide
(and ideally a script) must accompany the removal. The `layout_version` field is deferred;
if it becomes necessary before 1.0, it will be added as a governance act.

### Evidence

- mozg-pipelines `{% include %}` usage: **0 occurrences** (grep confirmed). Migration
  requires adding `{% include "phases/..." %}` calls; no option avoids this. The
  "zero body changes" claim from prior drafts is removed.
- `SKILL.md` exact-basename convention: **all 53 plugin skills** use `SKILL.md` uppercase
  (find confirmed). The `missing_skill_md` diagnostic is a safety net.

### Consequences

- Good, because directory-layout skills are self-contained containers.
- Good, because phase include paths are short (`phases/research.md`) vs. full paths.
- Good, because flat-file skills are byte-for-byte backward compatible: their `loader_root`
  IS `project_root`, so all existing `fragments/` includes resolve unchanged.
- Good, because a missing include is always a hard render error — no silent wrong content.
- Good, because the `fragments/` pre-registration mechanism allows directory skills to
  access project-level shared content without any loader fallback or reserved magic words.
- Bad, because `parse.rs` gains a two-mode collector (flat + directory descent) and a
  new coexistence merge rule.
- Bad, because `compile.rs` replaces the stock `path_loader(project_root)` with a
  per-skill-type dispatch and a custom `minijinja::Loader` closure.
- Bad, because the `fragments/` pre-registration requires a new scan-and-register step at
  environment setup in `compile.rs`.
- Bad, because `lint/` gains two new passes and `--strict` semantics broaden from
  "budget only" to "budget + layout errors."
- Bad, because the kind-uniform `load_artifacts` loop gains a Skill-only branch.
- Neutral: `compile::render`'s signature is unchanged; `skill_dir` derivation is internal
  to `compile.rs`; no call-site changes to `build.rs`, `tokens.rs`, `budget.rs`,
  `compile_tests.rs`.

## Pros and Cons of the Options

| Criterion | A (two-root fallback) | B (single-root-per-type) | C (namespaced) | D (flat + fragments/) | E (status quo) |
|---|---|---|---|---|---|
| Short phase include paths | ✓ | ✓ | ✗ (prefix) | ✗ (full path) | n/a |
| Silent wrong-content risk | ✗ | ✓ (hard error) | ✓ | ✓ | ✓ |
| Flat-skill backward compat | ✓ | ✓ | ✓ | ✓ | ✓ |
| No reserved magic prefixes | ✗ | ✓ | ✗ | ✓ | ✓ |
| Shared cross-skill content | seamless (fallback) | pre-registered templates | explicit | established `fragments/` | established `fragments/` |
| Added compiler surface | medium | medium | medium | low | none |
| Ongoing migration friction | none | none | none | none | yes (recurring flatten) |

**B selected**: only option combining short include paths, hard-error-on-missing, and
flat-skill backward compat without a two-root fallback or reserved-prefix magic.

## More Information

- ADR-0004 established folder-per-kind source layout; this ADR extends `skills/` to allow
  per-skill subdirectories.
- v1 migration target: the mozg-pipelines reference ecosystem (12 plugins, 53 skills,
  ~40 phase files, ~9 shared fragment files). Migration adds `{% include %}` calls; the
  directory layout makes those references short and local.
- `Instructions` and `Subagent` directory layout is explicitly deferred.
- minijinja 2.20 (Cargo.lock confirmed): the stock `path_loader` (single-root) is reused
  for per-skill include resolution. Fragment pre-registration uses `add_template_owned`
  (`environment.rs:198`, owns the body as `Cow`) — NOT `add_template` (`environment.rs:180`,
  which requires `&'source str` / `'static` only). The `weaft/ask.j2` registration at
  `compile.rs:38` uses `add_template` because `ASK_J2` is a `const &'static str`; that
  mechanism does not generalize to runtime-scanned file contents. Named-template precedence
  over the loader is verified in `loader.rs:125-141`.
  
  include path constraints: `safe_join` (minijinja `loader.rs:180-189`) rejects any segment
  starting with `.` or containing `\` (hard not-found error). Relative-escape (`../`) and
  hidden-dir includes are not supported.

## Provenance / Links

- **Branch**: `claude/weaft-v0-1-spec-pKbqm`
- **Head at sign-off**: `108c652` (Ask gates)
- **Task dir**: `.mz/task/2026_06_09_govern_skill_dir_layout/`
- **Discuss iterations**: 3 (heavy panel — 4 critics + synthesizer per iteration)
- **Criticals resolved**: 4 (minijinja two-root loader, `add_template` vs `add_template_owned`, `unused::check` false-positive, Option B / Option A collapse)
- **Human sign-off**: drmozg, 2026-06-09
- **Implemented**: 2026-06-13 — all 14 spec points shipped on branch `claude/weaft-v0-1-spec-pKbqm`; 23 unit tests + 5 CLI integration tests green; 8 committed snapshots unchanged.
