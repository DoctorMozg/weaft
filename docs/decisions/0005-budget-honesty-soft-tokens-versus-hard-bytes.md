---
status: "accepted"
date: 2026-05-30
deciders: drmozg
---

# Budget honesty: soft tokens versus hard bytes

<!-- mz-gov:agdr start -->
agdr_id: agdr-adr-0005
timestamp: 2026-05-30T19:02:45+04:00
agent: gov-artifact-writer
model: claude-sonnet-4-6
trigger: user-prompt
status: executed
human_signoff: drmozg@2026-05-30T19:02:45+04:00
<!-- mz-gov:agdr end -->

## Context and Problem Statement

weaft's token budgets are its own heuristics, but some hosts impose real size limits — for
example OpenAI Codex's documented `AGENTS.md` cap of roughly 32 KiB, which silently truncates
excess content. How should weaft model budgets so that its advisory heuristics and hosts'
real limits are never conflated, and so that a real silently-truncating cap fails the build
loudly?

## Decision Drivers

- [RFD-0001 §C-BUDGET-HONESTY](../rfcs/0001-weaft-v2-compilation-model.md#c-budget-honesty) —
  distinguish soft budgets from hard limits, label token counts as approximate, and check
  byte limits against the final emitted file.
- The byte check therefore runs in the `merge` stage of
  [ADR-0001](0001-weaft-v2-compilation-pipeline-shape.md).

## Considered Options

- A two-axis budget model: unit (tokens or bytes) and severity (soft or hard)
- Keep all budgets as soft token heuristics
- Treat all budgets as hard limits

## Decision Outcome

Chosen option: **model budgets on two axes — unit (tokens or bytes) and severity (soft or
hard)** — and enforce them accordingly.

1. **Soft is advisory** — a soft budget warns, and fails the build only in strict mode.
2. **Hard always errors** — a hard limit fails the build regardless of strict mode, with a
   message that names the host and states that the host truncates silently (C-BUDGET-HONESTY).
3. **Approximate counts** — token-based counts are labeled approximate wherever surfaced;
   hard byte limits are checked against the final emitted file.

### Consequences

- Good, because the honesty invariant is preserved as data, not prose.
- Good, because silent-truncation footguns become loud, build-failing errors.
- Bad, because each host budget must be classified correctly as soft or hard — mitigated:
  default to soft unless a host documents a real cap, and record the source.
- Neutral: the existing token-approximation disclaimer remains and is now attached to the
  soft-token unit.

### Confirmation

The byte-limit check runs in the `merge` stage of ADR-0001, because folding many artifacts
into one file is what exceeds a single-file cap and only post-merge is the final size known;
the budget lint warns on soft budgets (error only under `--strict`) and hard-errors on a
byte-cap violation, naming the host.

## Pros and Cons of the Options

### Two-axis budgets: unit (tokens or bytes) and severity (soft or hard)

- Good, because soft heuristics and real host limits are explicitly distinguished.
- Good, because hard byte limits are checked on the final merged file, where truncation
  actually occurs.
- Good, because it is honest by construction; token counts stay labeled approximate.
- Bad, because the budget lint gains an extra axis to reason about.

### Keep all budgets as soft token heuristics

- Bad, because a real, silently-truncating host cap would pass without error.
- Rejected: dishonest about a real footgun that corrupts output.

### Treat all budgets as hard limits

- Bad, because weaft's heuristics are not host limits; valid builds would be blocked.
- Rejected: misrepresents weaft heuristics as host-enforced limits.

## More Information

- Governed by
  [RFD-0001 — weaft v2 compilation model](../rfcs/0001-weaft-v2-compilation-model.md);
  builds on [ADR-0001](0001-weaft-v2-compilation-pipeline-shape.md) (byte check located in
  the `merge` stage).
- Migrated to mz-gov format on 2026-05-31 by `gov-artifact-writer` (`claude-sonnet-4-6`)
  from the govctl record `ADR-0005`; the provenance `timestamp` and `human_signoff`
  preserve the original 2026-05-30 decision and approval.
