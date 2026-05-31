---
status: "accepted"
date: 2026-05-30
deciders: drmozg
---

# weaft v2 compilation pipeline shape

<!-- mz-gov:agdr start -->
agdr_id: agdr-adr-0001
timestamp: 2026-05-30T19:02:45+04:00
agent: gov-artifact-writer
model: claude-sonnet-4-6
trigger: user-prompt
status: executed
human_signoff: drmozg@2026-05-30T19:02:45+04:00
<!-- mz-gov:agdr end -->

## Context and Problem Statement

weaft v2 must transform a broad and growing set of artifact kinds across many hosts
while preserving weaft's separation between host-aware rendering and format/layout
emission. What stage decomposition should a v2 compile use so that host-semantic logic
stays in one place, mapping and serialization stay pure and testable, and fold/drop
dispositions are expressible?

## Decision Drivers

- [RFD-0001 §C-PIPELINE](../rfcs/0001-weaft-v2-compilation-model.md#c-pipeline) —
  template rendering must be the only host-semantic, template-driven stage.
- [RFD-0001 §C-DETERMINISM](../rfcs/0001-weaft-v2-compilation-model.md#c-determinism) —
  output must be byte-stable across runs.
- [RFD-0001 §C-SUPPORT-DISPOSITION](../rfcs/0001-weaft-v2-compilation-model.md#c-support-disposition)
  — native / fold / drop must be resolvable per (host, kind).

## Considered Options

- Seven-stage pipeline — parse, resolve, render, map, serialize, emit, merge
- Retain the v1 three-stage pipeline (parse → render → emit) with per-backend emit doing
  mapping, serialization, and layout
- Fully template-driven emission (templates produce final files, including frontmatter)

## Decision Outcome

Chosen option: **the seven-stage pipeline** (parse, resolve, render, map, serialize, emit,
merge), because it confines host-semantic, template-driven logic to a single `render` stage
while keeping mapping, serialization, and layout downstream as pure, data-driven transforms
— satisfying the pipeline and determinism drivers in a way neither alternative does.

1. **Render isolation** — only `render` sees user templates and branches on host semantics;
   everything downstream consumes declared matrix data and already-rendered content
   (C-PIPELINE).
2. **Resolve** — a dedicated stage selects each (host, kind) disposition and performs kind
   remapping (e.g. skill → section) per C-SUPPORT-DISPOSITION.
3. **Merge** — a terminal stage joins folded artifacts and enforces byte-limit checks once
   the final size is known.

### Consequences

- Good, because backends become thin; per-host divergence is data, not code.
- Good, because pure stages are independently testable and keep output deterministic.
- Good, because fold and drop are first-class, expressed in `resolve` and `merge`.
- Bad, because there are more moving parts than v1 — mitigated: each stage is small and
  individually testable.
- Bad, because authors must understand kind remapping in the `resolve` stage — mitigated:
  documented in the source-layout guide.
- Neutral: byte-budget enforcement
  ([ADR-0005](0005-budget-honesty-soft-tokens-versus-hard-bytes.md)) lives in the `merge`
  stage, because only post-merge is the final file size known.

### Confirmation

Each stage is a pure function of its input plus the capability matrix, so stages are
unit-tested in isolation; a snapshot / determinism test asserts byte-identical output for
identical input (C-DETERMINISM).

## Pros and Cons of the Options

### Seven-stage pipeline (parse, resolve, render, map, serialize, emit, merge)

- Good, because `render` is the sole template-driven, host-semantic stage.
- Good, because map / serialize / emit are pure, data-driven, and individually testable.
- Good, because `resolve` expresses fold / drop and `merge` enforces byte budgets and
  concatenation.
- Bad, because it has more stages and indirection than v1.

### Retain the v1 three-stage pipeline with fat backends

- Bad, because backends re-accumulate hardcoded per-host logic.
- Rejected: it reproduces the v1 entanglement that v2 exists to remove.

### Fully template-driven emission

- Bad, because host-format logic leaks into user templates, losing typed mapping and
  determinism guarantees.
- Rejected: it breaks the render / emit separation and the frontmatter-fidelity guarantees.

## More Information

- Governed by
  [RFD-0001 — weaft v2 compilation model](../rfcs/0001-weaft-v2-compilation-model.md);
  related: [ADR-0005](0005-budget-honesty-soft-tokens-versus-hard-bytes.md) (byte-budget
  enforcement in the `merge` stage).
- Migrated to mz-gov format on 2026-05-31 by `gov-artifact-writer` (`claude-sonnet-4-6`)
  from the govctl record `ADR-0001`; the provenance `timestamp` and `human_signoff`
  preserve the original 2026-05-30 decision and approval.
