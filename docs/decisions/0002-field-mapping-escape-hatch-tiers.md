---
status: "accepted"
date: 2026-05-30
deciders: drmozg
---

# Field-mapping escape-hatch tiers

<!-- mz-gov:agdr start -->
agdr_id: agdr-adr-0002
timestamp: 2026-05-30T19:02:45+04:00
agent: gov-artifact-writer
model: claude-sonnet-4-6
trigger: user-prompt
status: executed
human_signoff: drmozg@2026-05-30T19:02:45+04:00
<!-- mz-gov:agdr end -->

## Context and Problem Statement

Most host differences are a field rename, recase, drop, or fold, but a minority are
structural value surgery — for example MCP tool namespacing, or encoding environment
variables as nested tables. How much of the per-(host, kind) field transformation should be
declarative data, and where should code be allowed when data cannot express a transform?

## Decision Drivers

- [RFD-0001 §C-CAPABILITY-MATRIX](../rfcs/0001-weaft-v2-compilation-model.md#c-capability-matrix)
  — the field mapping must be declared as matrix data.
- [RFD-0001 §C-FRONTMATTER-FIDELITY](../rfcs/0001-weaft-v2-compilation-model.md#c-frontmatter-fidelity)
  — emitted keys and casing must match each host exactly.
- Transforms must run inside the `map` stage of
  [ADR-0001](0001-weaft-v2-compilation-pipeline-shape.md) and stay pure.

## Considered Options

- Three tiers: declarative field-map data, a per-(host, kind) field-transform hook, and a
  per-host post-emit hook
- Pure declarative: every transform expressed as data, with no per-host code
- Pure per-host code implementing transformation against a shared trait
- A single code hook covering both per-field value surgery and cross-artifact, whole-file
  concerns

## Decision Outcome

Chosen option: **a data-first field-map with two narrowly-scoped code escape hatches** — a
per-(host, kind) field-transform hook for structural value surgery, and a per-host post-emit
hook for cross-artifact concerns.

1. **Data default** — rename, recase, drop, and fold are declared as matrix data and require
   no code, satisfying C-CAPABILITY-MATRIX and C-FRONTMATTER-FIDELITY.
2. **Justification gate** — a code hook is permitted only when a field-map provably cannot
   express the transform.
3. **Decorator, not rewrite** — a field-transform hook receives the resolved cell so it can
   reuse the generic mapping and override only the field that needs surgery.
4. **Two seams, two scopes** — a field-transform runs per (host, kind) inside the `map`
   stage, while a post-emit hook runs per host across already-emitted files; the two scopes
   are distinct, so each is served by its own seam.

### Consequences

- Good, because the matrix stays introspectable; most hosts need zero transformation code.
- Good, because rare structural encodings remain expressible without bloating the data
  model.
- Bad, because of the risk of escape-hatch sprawl — mitigated: the justification gate, plus
  review that counts transforms per host and challenges each one.
- Neutral: capability facts still live only in the matrix; hooks shape values, not
  capabilities.

### Confirmation

Transform hooks run inside the `map` stage of ADR-0001 and must remain pure (no I/O), so
they stay unit-testable; review counts the transform hooks per host and challenges each.

## Pros and Cons of the Options

### Three tiers (data field-map, per-(host,kind) transform hook, per-host post-emit hook)

- Good, because roughly 90% of cases are pure data.
- Good, because code is used only where data provably cannot express a transform.
- Good, because transform hooks are pure, testable, and can reuse the generic mapping path.
- Bad, because two code seams must be governed against sprawl.

### Pure declarative

- Bad, because it requires an ever-growing mini-DSL to express nested or structural
  encodings.
- Rejected: an over-complex configuration DSL for a handful of rare cases.

### Pure per-host code against a shared trait

- Bad, because it reproduces v1 hardcoding, loses matrix introspection, and scales poorly
  across many hosts.
- Rejected: it defeats the data-driven goal of the redesign.

### A single combined code hook

- Bad, because the two concerns operate at different scopes and stages: per-field within
  mapping versus per-file after emission.
- Rejected: a single hook would force cross-artifact state into the per-field mapping stage,
  which structurally cannot see other artifacts.

## More Information

- Governed by
  [RFD-0001 — weaft v2 compilation model](../rfcs/0001-weaft-v2-compilation-model.md);
  builds on [ADR-0001](0001-weaft-v2-compilation-pipeline-shape.md) (the `map` stage hosts
  the field-transform hook).
- Migrated to mz-gov format on 2026-05-31 by `gov-artifact-writer` (`claude-sonnet-4-6`)
  from the govctl record `ADR-0002`; the provenance `timestamp` and `human_signoff`
  preserve the original 2026-05-30 decision and approval.
