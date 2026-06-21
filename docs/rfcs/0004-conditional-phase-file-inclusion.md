---
rfd: "0004"
title: "Conditional phase-file inclusion via {% part %} tag"
state: prediscussion
authors: drmozg
discussion: <pending>
---

# RFD 0004 — Conditional phase-file inclusion via {% part %} tag

<!-- mz-gov:agdr start -->
agdr_id: agdr-task-2026_06_21_govern_part_tag_cond_inc
timestamp: 2026-06-21T00:00:00Z
agent: gov-artifact-writer
model: claude-sonnet-4-6
trigger: user-prompt
status: proposed
human_signoff: drmozg
<!-- mz-gov:agdr end -->

## Summary

weaft skill authors can split large skills into phase files (`skills/name/phases/research.md`)
and include them via Jinja's `{% include %}`. Today, `{% include %}` is **always inlined** at
render time — the phase content is expanded into the main body before any host backend sees it.
This wastes token budget even for hosts that could, in principle, reference the phase file
separately and load it on demand. This RFD proposes a `{% part %}` tag (and a variant option
of extending `{% include %}`) that conditionally inlines or emits a host-appropriate reference
based on a new flag on the Skill `KindCapabilities` cell in the host matrix.

## Problem statement

A directory-layout skill with several phase files (`research`, `plan`, `execute`, `review`)
inlines all four phases into one SKILL.md body, easily exceeding 8 000-token soft budgets even
when most invocations only use one or two phases. The current design optimises for simplicity
(one file per skill per host) at the cost of context-window economy.

The question is: should weaft support a mode where phase content stays in separate files that
the host loads on demand, and if so, what mechanism unlocks that behaviour?

Three sub-problems are entangled:

1. **Capability gap**: no current host (claude-code, cursor, agents-md, opencode, codex,
   gemini-cli) has a **validated**, documented, machine-readable mechanism to load a second
   instruction file and inject it into an agent's context on demand. This claim is asserted, not
   per-host verified (see Q1).

2. **Pipeline shape**: emitting a phase file as a separate output requires either synthesising a
   second `Artifact` upstream at parse time (the `singleton_artifacts` pattern) or widening
   `pipeline/emit.rs`'s `pub fn emit(...) -> EmittedSpec` to return `Vec<EmittedSpec>`. Either
   path is a cross-crate structural change, not a tag addition.

3. **Budget/token counting contract**: ADR-0007 §8 *deliberately fixed* that "phase content
   IS counted because render expands includes before counting." Any option that replaces
   `{% include %}` expansion with a reference silently removes phase tokens from both
   `lint/budget.rs` (soft budget lint) and `tokens.rs` (the `weaft tokens` command). The RFD
   must decide: when a phase is referenced rather than inlined, do its tokens still count toward
   `max_skill_tokens`?

4. **Authoring surface**: the current `{% include %}` directive has a fixed semantic (inline
   now). Adding conditional behaviour requires either a new tag, a modified tag with parameters,
   or an author-side workaround — each with different ergonomics and migration burdens.

## Impact — affected components

Implementing any of options B, B′, C, D, or E touches:

| Component | Location | Change |
|---|---|---|
| `HostCapabilities` / `KindCapabilities` (Skill cell) | `crates/weaft-core/src/capability.rs` | New `supports_phase_files: bool` on the Skill `KindCapabilities` cell (see §Flag Placement below). **Note**: `KindCapabilities` has no `Default`. Adding a non-`Default` bool forces every cell literal for every kind on every host to be updated — **~20 construction sites** across the 6 host `*_kinds()` constructors, the `native_cell`/`drop_cell` helpers, and test fixtures. Alternative: `pub supports_phase_files: Option<bool>` with `#[serde(default)]` defaults to `None`/`false` and avoids touching all 20 literals. |
| `compile::render` and its 4 call sites | `crates/weaft-core/src/compile.rs`, `build.rs:158`, `tokens.rs:55`, `lint/budget.rs:53`, compile tests | Render gains a conditional inline/reference fork; affects all four callers |
| **`lint/budget.rs`** | `crates/weaft-core/src/lint/budget.rs:53` | Counts `compile::render` output — phase tokens silently drop when a reference is emitted instead of inlined. Budget-counting contract must be explicitly decided. |
| **`tokens.rs` command** | `crates/weaft-cli/src/commands/tokens.rs:55` | Same seam as budget lint — reported token counts change when phase content is referenced. |
| **`pipeline/emit.rs`** | `crates/weaft-core/src/pipeline/emit.rs:27` | `emit(resolved, name, framed) -> EmittedSpec` is 1-artifact-to-1-spec. A second phase-file spec has no capability cell and no host attribution; acquiring those requires a design decision (see §Emit Attribution below). |
| **`build.rs` + `lint.rs` `path_meta` + `merge_and_check`** | `crates/weaft-cli/src/commands/build.rs:146, :227` and `crates/weaft-cli/src/commands/lint.rs:76-78` | `path_meta` derives budget+host from a resolved cell. A cell-less phase spec silently passes `merge_and_check` with no diagnostic (`path_meta` fallback → `(None, "")`, byte cap skipped when `budget = None`). On Codex, this is a **silent 32 KiB cap breach** — the exact failure mode ADR-0005 exists to prevent. **The same escape exists on the `lint` dry-run path** (`hard_byte_dry_run` in `lint.rs:76-78` calls the same `compile_target` + `merge_and_check`): `weaft lint --strict` would green-light a Codex file that `weaft build` silently truncates, breaking the documented build/lint parity guarantee. Both entry points must be hardened; the required test must cover both. |
| **`preview.rs` command** | `crates/weaft-cli/src/commands/preview.rs:27,38` | `preview` iterates all `compile_target` output specs and prints them. A separately-emitted phase spec appears as an unlabeled/orphaned block in preview output, or is silently dropped — a user-visible behavior change in a third command. Must decide: labeled, suppressed, or inlined when displayed via `weaft preview`. |
| ADR-0007 §8 (token counting), §14 (flat render contract), and snapshot suite | `docs/decisions/0007-skill-directory-layout.md`; `crates/weaft-cli/tests/snapshots/*.snap`; CLAUDE.md demo-diff check | Options B, C, D contradict §14 "backends always receive a flat rendered string — no backend special-casing." All value-adding options contradict §8. This RFD must amend both clauses if any non-A option is chosen. The `compile_tests.rs` snapshots pin the flat-rendered output; any non-A option shifts them (regenerated via `INSTA_UPDATE=always`). CLAUDE.md's demo-diff check (`diff dist/claude-code/…`) must also be re-reviewed. |

### Flag placement

**Resolved in this RFD**: `supports_phase_files` belongs on the **Skill `KindCapabilities` cell**,
not on top-level `HostCapabilities`. Rationale: phases are a Skill-only concept; Instruction,
Subagent, Command, McpServer, Settings, Plugin, Hook, Ignore cannot have phases. The matrix
already models per-`(host, kind)` facts via `KindCapabilities` (`disposition`, `layout`,
`format`, `budget`). A host-wide bool is dead/meaningless on eight of nine kind-cells and
invites bugs where non-Skill artifacts are checked against it. This also means Option C's
`{{ host.kind.supports_phase_files }}` template variable would require explicit projection through
`HostView` (`compile.rs:142`); the flag cannot ride `HostCapabilities`'s `#[serde(flatten)]`
the way a top-level scalar would.

### Emit attribution

When `supports_phase_files = true` and a phase file is emitted separately, the phase
`EmittedSpec` needs budget and host attribution to pass through `merge_and_check` correctly.
**A phase spec without a capability cell silently passes `merge_and_check` with no diagnostic**
(the `path_meta` fallback at `build.rs:176` resolves to `(None, "")` and `merge_and_check`
skips the byte cap when `budget = None`). On Codex (32 KiB hard budget, ADR-0005), this is a
silent cap breach. **Hard determination required**: a cell-less phase spec must be a build error,
not a silent `None`-budget pass. A required test: a phase file pushing a Codex merged file past
32 KiB must produce the hard-byte error.

Two approaches to giving the phase spec a cell:
- **(i) Inherit parent skill's cell**: the phase spec borrows the parent Skill `KindCapabilities`
  cell for budget/host attribution (consistent with "phase tokens still count toward skill
  budget"). Requires threading parent-cell context into the phase-file emit path.
- **(ii) Synthesise a second Artifact at parse time**: synthesise a phase-kind `Artifact`
  at parse time so it has its own cell. **Important caveat**: this cannot mirror the
  `singleton_artifacts` pattern — `singleton_artifacts(info)` reads only declarative manifest
  fields (`info.mcp_servers`, `info.settings`, etc.) producing a frozen list before render.
  A `{% part %}` reference lives inside the Jinja body, which is not scanned until
  `compile::render`. So Option (ii) at parse time requires either (a) an eager regex scan of
  every skill body for `{% part %}` directives before render — reintroducing exactly the
  fragile two-pass scan that is Option D's main weakness — or (b) deferring phase-artifact
  synthesis to render time, which is no longer a parse-time pattern. This makes (ii)
  materially more expensive than (i) and may collapse toward Option D or E.

**Phase file path and co-location (unstated premise)**: §Emit Attribution covers budget and
host attribution, but a separately-emitted phase file must also land at a location the host can
resolve relative to the main skill file. This premise is currently unstated. The Fix-6 asset-
path bug — where assets were emitted to a hardcoded `skills/<name>/` rather than the per-host
matrix path, later fixed with `skill_asset_dir` — is direct evidence that "where co-located
files land per host" is a real, already-bitten footgun class. The phase file path must be
derived from the host matrix layout data (as `skill_asset_dir` does), not a hardcoded
`phases/` constant. Q3 (emit pipeline path) must also settle the output path and whether the
reference string the compiler writes into the skill body correctly resolves at host load time.

**The reference format must be validated as consumed, not just chosen**: a reference string
written into `dist/` is useful only if the host actually loads the referenced file and injects
it into agent context. Q2 must require demonstrated consumption (emit → host loads → content
appears), with a documented host behavior and a round-trip test, before any host's
`supports_phase_files` flag is set `true`. Absent this validation, a separately-emitted phase
file silently disappears from agent context with no build error, lint warning, or snapshot diff
— a strictly worse outcome than current inlining.

### Budget/token counting policy

**Open for determination** — and **conditional on Q1 resolution**: when `{% part %}` emits a
reference rather than inlining content, do the phase file's tokens still count toward
`max_skill_tokens`?
- **Count them** (consistent with ADR-0007 §8): preserves the CLAUDE.md "Honesty about
  budgets" invariant. However, the "honest" justification ("the host still loads the phase on
  demand; the token cost is real") assumes the same unverified host capability as Q1 — that the
  host *does* load the referenced file and pay the token cost. If the host ignores the
  reference, "count them" over-reports tokens never paid.
- **Don't count them** (optimistic): the motivation for the whole feature is token-budget
  relief; excluding the phase tokens is the whole point. But this silently reports a lower
  budget usage than the host actually experiences (if the host loads the content) or reports
  fictional savings (if the host does not load at all).

**Determination must be deferred until Q1 resolves**: the correct counting rule is a function
of how the host loads phase content (load-on-demand vs. load-all vs. not-at-all). Recording a
rule before host behavior is validated means the "honesty" invariant is satisfied nominally.
Preferred default: defer to "count them" (ADR-0007 §8 status quo) and revisit when a host with
a confirmed loading model is in scope. If discussion selects "don't count," the Determinations
section must record that reverting the policy is a metric-contract migration.

## Decision criteria

Options are evaluated against:
1. **Compiler surface added** — new parser, tag extension, emit path changes
2. **Authoring ergonomics** — discoverability, verbosity, migration from `{% include %}`
3. **Static analysability** — can lint detect misuse without rendering?
4. **Migration cost if a host later gains support** — does existing source automatically benefit?
5. **Benefit to current hosts** — zero benefit until a host gains `supports_phase_files = true`

## Proposal / options on the table

### A — Status quo: `{% include %}` always inlines (no change)

Keep the current behaviour. Phase files are always inlined at render time. A skill with five
phases compiles to one large file on every host.

**Pros**: zero new compiler surface; no authoring migration; well-understood; zero reversal
cost — no infrastructure ships before a consumer exists; fully reversible.  
**Cons**: no token-budget relief for hosts that could support separate loading in the future;
authors who want lazy loading have no mechanism to express it.

---

### B — New `{% part "file" %}` custom tag (conditional inline vs. reference)

Register a custom minijinja tag `part`. The tag's behaviour is driven by a new
`supports_phase_files: bool` flag on the Skill `KindCapabilities` cell (see §Flag Placement):

```rust
// In KindCapabilities (Skill cell only)
pub struct KindCapabilities {
    // existing fields …
    pub supports_phase_files: bool,  // NEW — only meaningful on Skill cells
}
```

Render-time semantics:
- `supports_phase_files = true` → emit the phase file as a separate `EmittedSpec` alongside
  the main skill; replace the `{% part %}` call site with a host-specific reference string.
- `supports_phase_files = false` → inline the file content, identical to `{% include %}` today.

All six current hosts would set `supports_phase_files = false` — making `{% part %}` degrade
to inline on all current targets.

**Important: the "emit a separate file" part requires a cross-crate pipeline change**, not just
a tag addition. `pipeline/emit.rs` is `1 artifact → 1 EmittedSpec`; fanning out a second phase
file requires either approach (i) or (ii) from §Emit Attribution above. This is the primary
cost driver, not the tag registration.

**Pros**: clean authoring surface; one tag expresses intent; graceful degradation; tag is the
public surface, so future changes to the reference format don't require author rewrites.  
**Cons**: significant compiler work (custom minijinja tag + pipeline-shape change); reference
format per host is unspecified and becomes a wire contract once set; zero current hosts benefit;
the `{% part %}` syntax is a one-way door once adopted by skill authors.

---

### B′ — `lazy` flag on the existing `{% include %}` tag

Instead of a new tag, extend `{% include %}` with a `lazy` parameter:

```jinja
{% include "phases/research.md" lazy=true %}
```

Behaviour:
- `lazy=true` AND `supports_phase_files = true` → emit reference, emit phase file separately.
- `lazy=true` AND `supports_phase_files = false` (all current hosts) → inline, same as today.
- `lazy=false` (default) → inline always, exact current behaviour.

**Pros**: reuses a documented directive; zero new authoring vocabulary; the naming debate is
avoided; no `{% include %}` callers need to change today.  
**Cons**: overloads one directive with two semantics, which may confuse authors; the emit
pipeline change is identical to B (same cost); per-host reference-format problem is inherited
from B; still a one-way door once `lazy=true` is authored in source.

---

### C — Author-side conditional using `{{ host.kind.supports_phase_files }}` (or similar projection)

Add the flag to the Skill `KindCapabilities` cell and expose it through the `HostView` render-
context projection explicitly. Authors write:

```jinja
{% if host.kind.supports_phase_files %}
<!-- phase: phases/research.md -->
{% else %}
{% include "phases/research.md" %}
{% endif %}
```

Note: this still requires a mechanism to emit the phase file as a separate artifact when the
branch is taken — the template cannot trigger emit side-effects. Requires the same pipeline
change as B.

**Pros**: zero new Jinja extension machinery; reuses existing template language.  
**Cons**: verbose and error-prone; the emit-phase-file side effect needs compiler support
(pipeline change same as B); harder to analyse statically (lint cannot inspect template
branches); the flag *must* be explicitly projected through `HostView` — it cannot be inherited
from `HostCapabilities` because the Skill `KindCapabilities` cell is `#[serde(skip)] kinds` in
`HostView`; exposing this as a template variable makes it stable `{{ host.* }}` API with the
same de-facto stability promise as `{{ ask() }}` (RFC-0002), raising retraction cost.

---

### D — Pre-render extraction pass (no minijinja extension)

Before passing the template body to minijinja, scan for `{% part "…" %}` syntax with a regex
or simple parser. For each `part` directive:
- If `supports_phase_files = true`: register the file for separate emission; replace in the
  template string with a reference placeholder before minijinja runs.
- If `supports_phase_files = false`: replace with an equivalent `{% include "…" %}` before
  minijinja runs.

**Pros**: avoids minijinja Extension API complexity.  
**Cons**: the pre-pass is fragile (edge cases: template strings, multi-line tags, nested
blocks); splitting template processing across two passes complicates the render stage contract;
the pipeline-shape emit change is still required.

---

### E — Manifest-level `weaft_phase` directive in `weaft.yaml`

Do not extend the Jinja template language. Instead, add a manifest-level declaration:

```yaml
skills:
  safe-deleter:
    phases:
      - phases/research.md
      - phases/execute.md
```

The compiler reads this and either inlines or emits separately based on the capability flag,
without any author-side template change. Skills that don't declare phases compile exactly as
today.

**Pros**: no Jinja extension; manifest-level declaration is statically analysable; orthogonal
to the template body.  
**Cons**: authoring split between `weaft.yaml` (structure) and `SKILL.md` (order of
appearance); phases can't be conditionally included based on template logic (e.g. only include
the "refusal phase" for hosts that support refusals); significant scope increase to the parser;
same emit pipeline change required.

## Open questions

1. **Does any current host actually support on-demand phase-file loading?** This claim must be
   validated per-host with a document reference and date (the same discipline `capability.rs`
   applies — gemini-cli is marked "UNRESEARCHED" rather than assumed). Until validated, the
   correct call is to ship A and revisit when a specific host capability is confirmed. (Gate
   condition: if Q1 resolves to "no host, none planned", the choice collapses to A vs. defer,
   and B/C/D/E become forward-looking infrastructure with zero present benefit.)

2. **What is the reference format per host?** For claude-code, would a separate phase file be
   auto-loaded by the host, or does the skill body need a tool-call hint, a markdown link, or a
   comment with a schema marker? The answer must be host-documented before any reference format
   is chosen. Note: once chosen, the reference format is a **wire contract** (a migration to
   change, not a free edit). Prefer a format forgiving to versioning (a comment with an explicit
   schema marker) over one parsed positionally.

3. **Emit pipeline path**: when emitting a phase file separately, does the implementation use
   (i) inheriting the parent skill's `KindCapabilities` cell for budget/host attribution, or
   (ii) synthesising a second `Artifact` at parse time (the `singleton_artifacts` pattern)?
   This determines the blast radius on `pipeline/emit.rs` and `build.rs`.

4. **Budget/token counting policy** (see §Budget/token counting policy above): when `{% part %}`
   emits a reference, do phase file tokens still count toward `max_skill_tokens`?

5. **Anonymous parts**: should `{% part %}` accept an inline body
   (`{% part %}…content…{% endpart %}`) in addition to file references?

6. **Lint implications**: should a `{% part %}` in a skill targeting only hosts with
   `supports_phase_files = false` trigger a lint warning ("{% part %} degrades to {% include %}
   for all your targets")? Note: any new lint must be designed alongside the existing
   `unused::check` false-positive for phase content (ADR-0007 §8 known limitation).

7. **Deprecation path**: if `{% part %}` is later removed or renamed, what is the deprecation
   mechanism — a lint that rewrites to `{% include %}`, a major-version gate, a compat shim?

8. **Migration path**: if authors adopt `{% part %}` today and a host adds support later, do
   existing skills automatically benefit (yes, by design of options B/B′) — or does the author
   need to opt in per skill?

## Reversibility notes

- **`supports_phase_files` flag on Skill `KindCapabilities`** — two-way door, **conditional**.
  Project-owned field in `capability.rs`, removable in an afternoon while all six hosts set it
  `false` and the flag is never projected into the template context. However, if any option
  exposes the flag via `{{ host.* }}` (notably Option C, which requires projecting it through
  `HostView`), the serialized template context changes shape, every committed skill snapshot
  shifts, and "zero rendered output" is false. In that case the flag acquires the stable-API
  retraction cost of the `{{ host.* }}` surface (see RFC-0002 precedent in §Relationship to
  prior decisions) and is no longer a cheap two-way door. **The reversal cost depends on the
  chosen option**: flag-only or Options B/B′/D/E keep it cheap; Option C makes it expensive.
- **`{% part %}` / `{% include lazy %}` tag in author source** — one-way door. Lives in user-
  owned `.md` files the compiler does not control. Reversing requires a deprecation window
  plus every downstream author rewriting their skills.
- **Per-host reference wire format** — intermediate one-way door. Once a host is documented to
  consume a specific reference shape from `dist/`, that shape is a wire contract.
- **Budget/token-counting policy** — soft one-way door once published. Once `weaft tokens`
  and `lint/budget.rs` report a number under whichever policy is chosen, skill authors
  calibrate their phase splitting against it. Reversing the policy (e.g. switching from
  "don't count" back to "count") silently tightens headroom and turns previously-green skills
  red — a metric contract migration. Prefer the "count them" rule: it is both the reversible
  choice (consistent with "count" = ADR-0007 §8 status quo) and the honest one (the host
  still loads the tokens).

## Relationship to prior decisions

- **ADR-0007** (`docs/decisions/0007-skill-directory-layout.md`) — the governing artifact for
  the `phases/` directory layout and `{% include %}` resolution. §8 explicitly requires phase
  content to be counted in skill token budgets. §14 asserts "all backends always receive a flat
  rendered string — they never see the source directory structure or phase files." If any
  non-A option is chosen, this RFD partially supersedes ADR-0007 §8 (budget counting must be
  re-decided) and §14 (backends gain per-host phase-file emit logic). The Determinations must
  state which clauses are amended.
- **ADR-0005** (`docs/decisions/0005-budget-honesty-soft-tokens-versus-hard-bytes.md`) — the
  32 KiB hard byte budget for Codex AGENTS.md. Phase files emitted separately must pass through
  the byte-budget check; a cell-less phase spec in `merge_and_check` would escape it.
- **RFC-0002** (`docs/rfcs/0002-generalized-ask-gates.md`) — precedent that `{{ host.* }}`
  template variables are stable authoring API with non-trivial retraction cost. Option C
  inherits this.

## Determinations

<to be resolved in discussion>

## Provenance / Links

- **Task directory**: `.mz/task/2026_06_21_govern_part_tag_cond_inc/`
- **Branch**: `claude/weaft-v0-1-spec-pKbqm`
- **Base commit at sign-off**: `108c652` (Ask gates)
- **Human sign-off**: drmozg, 2026-06-21
- **Governed by**: [weaft Development Governance Policy](../../CLAUDE.md#development-governance-policy)
- **Amends**: [ADR-0007](../decisions/0007-skill-directory-layout.md) §8 and §14 (if any non-A option chosen)
- **Referenced by**: [ADR-0005](../decisions/0005-budget-honesty-soft-tokens-versus-hard-bytes.md), [RFC-0002](./0002-generalized-ask-gates.md)
