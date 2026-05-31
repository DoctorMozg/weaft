---
status: "accepted"
date: 2026-05-30
deciders: drmozg
---

# Serialization stack and format framing

<!-- mz-gov:agdr start -->
agdr_id: agdr-adr-0006
timestamp: 2026-05-30T19:02:45+04:00
agent: gov-artifact-writer
model: claude-sonnet-4-6
trigger: user-prompt
status: executed
human_signoff: drmozg@2026-05-30T19:02:45+04:00
<!-- mz-gov:agdr end -->

## Context and Problem Statement

v2 must emit YAML frontmatter, TOML, JSON, plain Markdown, and Markdown-with-frontmatter
variants. v1 serializes only YAML, via `serde_yaml`, which is currently lightly maintained
(as of 2026-05). Which serialization libraries should v2 depend on, where should those
dependencies live, and where should per-format framing — how serialized fields combine with
a rendered body — live?

## Decision Drivers

- [RFD-0001 §C-SERIALIZATION](../rfcs/0001-weaft-v2-compilation-model.md#c-serialization) —
  multiple formats selectable per (host, kind), and the serialization stage owns framing.
- [RFD-0001 §C-DETERMINISM](../rfcs/0001-weaft-v2-compilation-model.md#c-determinism) —
  stable key ordering.
- An early redesign phase must preserve v1 output byte-for-byte, which constrains changing
  the YAML serializer immediately.
- Serialization is part of emission, which resides in the targets layer; the core layer
  carries no emission code, so any new serializer dependency belongs with emission.

## Considered Options

- Keep the current YAML library for now, add TOML and JSON, and isolate all serialization
  behind one boundary
- Replace the YAML library immediately with a more actively maintained one
- Use one generic value model serialized to all formats with no per-format framing

## Decision Outcome

Chosen option: **keep the current YAML library for now, add TOML and JSON serializers, give
the serialization stage ownership of per-format framing, and isolate serialization behind a
single boundary**.

1. **Minimal delta** — add TOML and JSON support in the targets layer; do not change the
   YAML serializer during the output-preserving phase (C-SERIALIZATION).
2. **Framing with format** — each format owns how serialized fields and the rendered body
   combine (a fenced YAML block, a TOML field carrying the body, a JSON document, a Markdown
   section).
3. **Swap-friendly** — all serialization sits behind one boundary so the YAML library can be
   replaced later without touching callers.

### Consequences

- Good, because it is the smallest possible dependency and behavior change to reach
  multi-format output.
- Good, because format framing is correct per format, satisfying the fidelity contract.
- Good, because a future YAML-library replacement is a contained, single-boundary change.
- Bad, because a lightly-maintained YAML dependency remains in the tree for now — mitigated:
  the isolation boundary plus a tracked follow-up to revisit.
- Neutral: new dependencies (TOML and JSON serializers) are added only to the targets layer,
  keeping the core layer lean.

### Confirmation

Key ordering is explicit and data-derived so output stays deterministic (C-DETERMINISM); a
snapshot test asserts byte-identical output across the supported formats.

## Pros and Cons of the Options

### Keep YAML for now, add TOML and JSON, isolate behind one boundary

- Good, because it is the smallest dependency delta, with no risk to the output-preserving
  early phase.
- Good, because per-format framing is co-located with each format.
- Good, because a later YAML-library swap is contained behind the isolation boundary.
- Bad, because a lightly-maintained YAML dependency remains for now.

### Replace the YAML library immediately

- Bad, because it adds churn and output-equivalence risk during the v1-preserving phase.
- Rejected: premature; it can be done later behind the isolation boundary without touching
  callers.

### One generic value model with no per-format framing

- Bad, because it cannot express TOML body-as-field versus YAML fenced body.
- Rejected: it breaks the framing fidelity required by the serialization contract.

## More Information

- Governed by
  [RFD-0001 — weaft v2 compilation model](../rfcs/0001-weaft-v2-compilation-model.md).
- Migrated to mz-gov format on 2026-05-31 by `gov-artifact-writer` (`claude-sonnet-4-6`)
  from the govctl record `ADR-0006`; the provenance `timestamp` and `human_signoff`
  preserve the original 2026-05-30 decision and approval.
