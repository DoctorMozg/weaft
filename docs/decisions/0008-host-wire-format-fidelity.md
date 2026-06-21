---
status: "accepted"
date: 2026-06-12
deciders: drmozg
consulted: opencode docs (opencode.ai/docs/agents), Cursor rules docs
informed: weaft maintainers
---

# Host wire-format fidelity: opencode subagents and cursor MDC rules

<!-- mz-gov:agdr start -->
agdr_id: agdr-adr-0008
timestamp: 2026-06-12T00:00:00Z
agent: gov-artifact-writer
model: claude-sonnet-4-6
trigger: automation
status: executed
human_signoff: drmozg@2026-06-12T00:00:00Z
<!-- mz-gov:agdr end -->

## Context and Problem Statement

weaft compiles one source project to host-specific files; the capability matrix
(`capability.rs`) is the single source of truth for each `(host, kind)` field map. Two emitted
formats reuse a near-neighbour host's field shape rather than the target host's own schema. The
opencode subagent (`.opencode/agent/<name>.md`) reuses Claude's subagent field map
(`CLAUDE_SUBAGENT_FIELDS` — `name`, `description`, `tools`, `model`); a Phase-3 review against
opencode's current docs shows that shape is partly wrong. The cursor `.mdc` skill format also
warranted validation. **Which fields should each host actually emit, by what mechanism, and where
weaft's canonical model cannot faithfully represent a host field, what should it do?**

## Decision Drivers

- **Wire-format fidelity** — emitted files must match the *current* documented schema of the
  target host, not a plausible-looking neighbour's.
- **Honesty invariant (repo-specific)** — weaft already prefers to *skip with a warning* rather
  than emit something broken. A field weaft cannot faithfully represent should be omitted, not
  guessed.
- **Single source of truth** — the resolution is matrix data (a forked `OPENCODE_SUBAGENT_FIELDS`)
  plus the smallest core mechanism needed to express it; no backend special-casing.
- **No deprecated output** — do not emit a field a host has deprecated.
- **Reversibility** — field-map data is trivially revertible. The one asymmetric edge is the
  *deferred tool-restriction posture* (see Consequences), not the data change.

## Considered Options

Two coupled sub-decisions: (1) which opencode fields to emit, and (2) the mechanism to emit a
*constant* field (`mode: subagent`) that has no source on the artifact.

**Sub-decision 1 — opencode tool-restriction field** (the load-bearing ambiguity):

- **Option A — Emit only faithfully-representable fields; drop `tools`.** opencode subagent emits
  `mode: subagent`, `description`, `model`; drop the redundant `name` and the `tools` allowlist.
- **Option B — Keep the deprecated `tools` map.** Transform the `Vec<String>` allowlist to
  opencode's legacy `tools: {name: bool}` map with a Claude→opencode lowercase name table.
- **Option C — Map the allowlist to the modern `permission:` denylist.** Invert the allowlist
  into opencode's `permission: {tool: allow|ask|deny}` map.
- **Status quo — keep emitting `CLAUDE_SUBAGENT_FIELDS` verbatim.** Listed for completeness;
  strictly dominated — it has the union of B's deprecation defect *plus* the wrong `mode` default
  *plus* the redundant `name`.

**Sub-decision 2 — mechanism for the constant `mode: subagent`:**

- **Option M1 — add a `FieldSource::Const { field, value }` variant** to the matrix, so a field
  map can emit a fixed literal as data (field name and literal value carried separately, so
  `key()` still returns the field name).
- **Option M2 — an opencode `FieldTransformSet`** that synthesizes `mode` in the backend (the same
  seam `codex.rs` already uses via `CodexTransforms`).

## Decision Outcome

Chosen: **Option A** (drop the un-representable field) with **Option M1** (`FieldSource::Const`).

**Why A:** it is the only tool-restriction option that emits nothing wrong or deprecated. B emits
opencode's deprecated shape; C requires inverting an allowlist into a denylist over opencode's full
tool universe — a list weaft does not have and that drifts as opencode adds tools — so a
partial/guessed denylist would be *more* dangerous than none.

**Why M1:** emitting `mode: subagent` is **not** expressible by the current field map — `FieldSource`
has only `TargetOverride`/`TopLevel`, both of which read a value *from the source artifact*, and
`read_canonical` omits a `None` (a `mode` field the artifact does not have). M1 keeps the constant
as declarative matrix data, *visible in the field map*, and reusable for any future constant host
field. M2 (a backend `FieldTransformSet`) is a working, already-generalized seam — `codex.rs` ships
`CodexTransforms` — so the rejection is **not** "M2 doesn't generalize" (it does); it is that M2
hides a constant with no per-artifact variation behind a runtime hook, putting matrix data inside
backend code. M1 is a *small core change* — one enum variant carrying `{ field, value }` (so
`key()` keeps returning the field name `mode`, not the literal) plus one arm each in the exhaustive
`read_canonical` and `key()` matches — **not** "pure data with no code"; that earlier framing was
wrong.

Concrete resolutions:

**opencode subagent** — introduce a NEW `OPENCODE_SUBAGENT_FIELDS` const (do **not** edit the
shared `CLAUDE_SUBAGENT_FIELDS`, which also feeds the Claude subagent cell):
1. `mode: subagent` via a new `FieldSource::Const { field: "mode", value: "subagent" }` — without
   it opencode defaults an omitted `mode` to `all`, wrongly making a weaft subagent also
   dispatchable as a primary agent.
2. Drop `name` — opencode derives the name from the filename; there is no documented `name:` field,
   and a redundant key risks drift from the filename-derived identity.
3. Drop the Claude-style `tools` list — opencode's deprecated shape; its modern `permission:` map
   is a denylist that cannot faithfully express a weaft allowlist (see Why A).
4. Keep `description` and `model`.

**cursor `.mdc` skill** (`CURSOR_SKILL_FIELDS`): validated **already faithful** — emits
`description`, an author-supplied `globs` (the `targets.cursor.globs` override, passed through as a
YAML value), and `alwaysApply`. Research confirms cursor expects `globs` as a **YAML array**
(`globs: ["**/*.ts"]`), which the passthrough already produces (verified in the live
`safe_deleter_cursor` snapshot). No code change — this ADR pins the array shape so a future change
does not regress it to a comma-string.

### Consequences

- Good, because no host receives a deprecated or wrong field; opencode subagents carry the correct
  `mode` and identity.
- Good, because it upholds the honesty invariant: an un-representable field is omitted, not faked.
- Good, because `FieldSource::Const` is a general, reusable mechanism for constant host fields, not
  an opencode one-off.
- **Cost / blast radius (corrected):** this is matrix data **plus a small core change**, not data
  only. It touches: a new `FieldSource::Const { field, value }` variant and its arm in
  `read_canonical` (`pipeline/map.rs`) and `key()` (`capability.rs`) — the `{ field, value }` shape
  keeps `key()` returning the field name, not the literal; a forked `OPENCODE_SUBAGENT_FIELDS`; and
  stale reference comments in `opencode.rs` (the `name/description/tools/model` doc line) and the
  `opencode_kinds` module comment, which must be updated. The build warning's natural home is the
  existing `lint/ask_user.rs` pass (already a capability-driven subagent-field warning).
- **Fail-open posture (the asymmetric edge):** dropping `tools` means an author's restrictive
  allowlist has **no effect** on the opencode target — the subagent runs with opencode's
  all-tools-enabled default. This is not retroactively closed by a later fix; it is a deferred
  *posture*, not just a revertible data edit. Mitigated by the build warning below and a follow-up
  to model per-host permissions.
- Neutral, because cursor needs no code change; the ADR is its validation record (the array-shape
  *snapshot* is the actual regression lock, not the prose).

### Confirmation

- A new opencode-subagent snapshot (driven by the `code-reviewer` fixture, which has no opencode
  subagent today) asserts the emitted frontmatter contains `mode: subagent`, omits `name`, omits
  `tools`, and keeps `description`/`model`. The snapshot is authored to **fail** before
  `FieldSource::Const` exists, so a no-op `mode` rule cannot pass silently.
- A lint/build **warning** fires when an opencode subagent target carries a non-empty `tools`
  allowlist, so the dropped restriction is self-announcing at build time rather than discoverable
  only in this ADR.
- The Claude subagent snapshot (`code_reviewer_claude_code`) is asserted **unchanged**, confirming
  the fork did not disturb `CLAUDE_SUBAGENT_FIELDS`.
- A cursor `.mdc` snapshot with an author-supplied `globs` override asserts a YAML array.

## Pros and Cons of the Options

### Option A — drop the un-representable field (chosen)

- Good, because it emits zero wrong/deprecated fields — strictly faithful output.
- Good, because it matches weaft's skip-with-honesty posture.
- Bad, because the opencode target silently ignores an author's tool allowlist — a fail-open gap,
  mitigated by a build warning + follow-up.

### Option B — keep the deprecated `tools` map

- Good, because it preserves the allowlist intent in *some* emitted shape today.
- Bad, because the shape is deprecated — output that warns now and breaks later.
- Bad, because the name-translation table silently drops tools with no opencode analogue
  (`AskUserQuestion`), a quiet fidelity loss.
- Rejected: emitting a deprecated shape contradicts the fidelity driver.

### Option C — map to the modern `permission:` denylist

- Good, because `permission:` is opencode's current mechanism.
- Bad, because inverting an allowlist into a denylist needs opencode's entire tool universe, which
  weaft lacks and which drifts as opencode adds tools.
- Bad, because a partial/guessed denylist looks authoritative while being wrong — more dangerous
  than none.
- Rejected: not faithfully doable without a per-host permission model weaft lacks today.

### Mechanism M1 (`FieldSource::Const`) vs M2 (backend transform)

- M1 Good, because the constant stays declarative matrix data, *visible in the field map* and
  reusable across hosts.
- M1 Bad, because it extends two exhaustive matches (`read_canonical`, `key()`) — small but real
  core surface; the variant must carry `{ field, value }` so `key()` keeps returning the field
  name, not the literal.
- M2 Good, because a `FieldTransformSet` is localized to opencode and touches neither exhaustive
  match — the minimal-core-surface mechanism, and the seam already generalizes (`codex.rs` ships
  `CodexTransforms`).
- M2 Bad, because it hides a constant that has no per-artifact variation behind a runtime hook,
  putting declarative matrix data inside backend code. Rejected: M1 keeps the constant visible in
  the matrix; its two small match arms are paid once and reused by any future constant host field.

### Status quo — keep the Claude shape

- Rejected: strictly dominated — wrong `mode` default + deprecated `tools` + redundant `name` at once.

## More Information

- **Affected governance artifact:** RFC-0003 (`docs/rfcs/0003-host-agnostic-model-tier-aliases.md`,
  status executed, signed 2026-06-10) documents opencode as "inherits `CLAUDE_SUBAGENT_FIELDS`."
  This ADR changes that inheritance; RFC-0003's opencode baseline row must be updated or annotated
  as superseded by this ADR so the governance record stays internally consistent.
- Related: ADR-0002 (field-mapping escape-hatch tiers), ADR-0004 (v2 source layout),
  ADR-0006 (serialization stack and format framing).
- Sources: opencode Agents docs (`https://opencode.ai/docs/agents/`) — `mode` default `all`,
  filename-as-name, `tools` deprecated in favour of `permission`; Cursor rules `.mdc` guides —
  `globs` as a YAML array. The opencode subagent schema was read on **2026-06-12**;
  `OPENCODE_SUBAGENT_FIELDS` should carry that validation date (mirroring capability.rs's existing
  `validated 2026-05` convention) and be re-checked on opencode version bumps — the one premise
  with a silent, out-of-band failure mode (weaft pins what it emits, not what opencode accepts).
- Follow-up: a future ADR may add a weaft per-host permission model, enabling faithful opencode
  `permission:` emission (revisits the Option-A `tools` deferral and the fail-open posture).

## Provenance / Links

- **Status**: implementation pending — the decision is accepted; the opencode field-map change is
  the next build step. No implementing commit or PR linked yet.
- **Branch at record time**: `claude/weaft-v0-1-spec-pKbqm` (commit `108c652`); no open PR.
- **Decision recorded**: 2026-06-12
- **Human sign-off**: drmozg@2026-06-12T00:00:00Z
- Proposed by `gov-artifact-writer` (`claude-sonnet-4-6`), trigger `automation` (self-initiated per
  the project governance policy during a Phase-3 host-format pass; confirmed by the user via
  `/govern both`).
- Heavy four-critic panel: iteration 1 AGGREGATE FAIL (blast-radius + assumptions caught a false
  "pure matrix data" claim, the shared `CLAUDE_SUBAGENT_FIELDS`, and silent RFC-0003 invalidation);
  revised; iteration 2 AGGREGATE PASS (0 Critical).
- Governance task: `.mz/task/2026_06_12_govern_host_wire_format/`.
