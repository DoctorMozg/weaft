---
status: "accepted"
date: 2026-05-30
deciders: drmozg
---

# Single-source host registry

<!-- mz-gov:agdr start -->
agdr_id: agdr-adr-0003
timestamp: 2026-05-30T19:02:45+04:00
agent: gov-artifact-writer
model: claude-sonnet-4-6
trigger: user-prompt
status: executed
human_signoff: drmozg@2026-05-30T19:02:45+04:00
<!-- mz-gov:agdr end -->

## Context and Problem Statement

In v1, each host identifier is repeated across at least six sites: capability lookup by id,
the capability list, backend dispatch by id, the backend list, the lint help string, and the
CLI init defaults. Drift between them is a latent defect. How can the set of supported hosts
derive from a single declaration consumed by capability lookup, emission, linting, and the
CLI, without violating the crate dependency direction?

## Decision Drivers

- [RFD-0001 §C-REGISTRY](../rfcs/0001-weaft-v2-compilation-model.md#c-registry) — single-source
  host declaration and a fail-closed consistency check.
- Capability data must live in the core layer; emission backend code must live in the targets
  layer; core must not depend on targets.

## Considered Options

- Two parallel ordered slices (capability data in core, backend code in targets) joined by
  id, with a consistency test
- A shared leaf crate holding both capability data and backend code
- Status quo: duplicated id match arms across both layers and the CLI

## Decision Outcome

Chosen option: **declare hosts as a single ordered slice in the core layer and backends as a
parallel slice in the targets layer, joined by id and enforced by a consistency test**.

1. **Single source per layer** — capability lookup, the capability list, lint help, and CLI
   defaults all derive from the core slice; emission derives the matching backend from the
   targets slice (C-REGISTRY).
2. **Fail closed** — a test asserts the two slices share the same ids in the same order, so a
   host added to one but not the other breaks the build.
3. **Derived help** — known-id help text and init defaults are computed from the slice, not
   hand-written.

### Consequences

- Good, because adding a host is one core entry plus one backend entry; nothing else.
- Good, because drift between the layers is caught automatically by a test.
- Good, because lint help and CLI defaults can never fall out of sync with the host set.
- Bad, because the two-slice invariant must be taught to contributors — mitigated: the
  consistency-test failure message states the rule explicitly.
- Neutral: CLI init default hosts become a data flag on each host entry rather than a
  hardcoded list.

### Confirmation

A consistency test asserts the core slice and the targets slice carry the same host ids in
the same order; the build fails if a declared host lacks a backend or a backend lacks a
declared host.

## Pros and Cons of the Options

### Two parallel ordered slices joined by id, with a consistency test

- Good, because it respects the core → targets dependency direction.
- Good, because there is one declaration per layer, with host ids derived for lint and CLI
  help.
- Good, because a consistency test fails the build on any drift.
- Bad, because there are two slices rather than one literal source.

### A shared leaf crate holding both

- Bad, because backend trait objects cannot live in a data-only leaf, forcing a larger
  restructure.
- Rejected: heavier than needed now; revisit only if a fourth consumer appears.

### Status quo: duplicated id match arms

- Bad, because six-site drift is exactly the defect being removed.
- Rejected: it is the problem this decision exists to fix.

## More Information

- Governed by
  [RFD-0001 — weaft v2 compilation model](../rfcs/0001-weaft-v2-compilation-model.md).
- Migrated to mz-gov format on 2026-05-31 by `gov-artifact-writer` (`claude-sonnet-4-6`)
  from the govctl record `ADR-0003`; the provenance `timestamp` and `human_signoff`
  preserve the original 2026-05-30 decision and approval.
