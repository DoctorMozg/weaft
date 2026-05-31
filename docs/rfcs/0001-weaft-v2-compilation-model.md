---
rfd: 0001
state: published
authors: drmozg
discussion: "migrated from govctl RFC-0001 (status: normative)"
---

# RFD 0001 — weaft v2 compilation model

<!-- mz-gov:agdr start -->
agdr_id: agdr-rfd-0001
timestamp: 2026-05-30T19:02:45+04:00
agent: gov-artifact-writer
model: claude-sonnet-4-6
trigger: user-prompt
status: executed
human_signoff: drmozg@2026-05-30T19:02:45+04:00
<!-- mz-gov:agdr end -->

## Summary

weaft v2 specifies a capability-aware transpiler that compiles one source project into
host-specific configuration for multiple AI coding agents.

This document governs the v2 compilation model — the set of artifact kinds weaft recognizes,
the (host × kind × field) capability matrix that drives emission, the compilation pipeline,
template exposure, serialization and frontmatter fidelity, budget honesty, the v2
source-project layout, and the host registry. It governs externally observable behavior and
the user-facing project contract; it does not prescribe internal code structure.

- **Hosts in scope:** `claude-code`, `cursor`, `agents-md`, `opencode`, `codex`, `gemini-cli`.
- **Kinds in scope:** instruction, skill, subagent, command, mcp_server, settings, plugin,
  hook, ignore.

## Problem statement

v1 compiled only skills and subagents and hardcoded per-host layout and frontmatter in
backend code, leaving the capability matrix decorative and making each new host a multi-site
edit. v2 makes host and kind differences *data*, so a single source yields meaningfully
different, host-valid output per target. The decision needs a written, ratified contract
because authors and host backends both depend on it, and because it spans several
independent design choices that must agree with one another.

## Specification (normative)

This RFD is **published**: the clauses below are normative and ratified. Each clause states
its requirement using RFC-2119 keywords (MUST / SHOULD / MAY) and the rationale behind it.
The governing design decisions are recorded in ADR-0001 through ADR-0006, which link back to
these clause anchors.

### C-ARTIFACT-KINDS

**Artifact kinds** — normative.

weaft MUST recognize a closed, enumerated set of artifact kinds: instruction, skill,
subagent, command, mcp_server, settings, plugin, hook, and ignore. Every source artifact
MUST be classified into exactly one kind. Unbounded, host-specific variation MUST be carried
as opaque per-artifact fields rather than by introducing new kinds; the kind set MUST NOT be
open to arbitrary user-defined kinds.

**Rationale.** The kind set is bounded by what real hosts actually expose, so an enumerated
set lets every pipeline stage handle kinds exhaustively and keeps weaft honest about what
each kind means. The genuinely unbounded variation lives at the field level, where it is
absorbed without changing the kind set. Because the set is closed, extending it is a
governance act — a change to this clause by amendment — rather than a runtime extension
point.

### C-CAPABILITY-MATRIX

**Capability matrix drives compilation** — normative.

Compilation MUST be driven by a capability matrix indexed by host, artifact kind, and field.
For each supported (host, kind) pair the matrix MUST declare: a support disposition (see
[C-SUPPORT-DISPOSITION](#c-support-disposition)), the output layout (file path, and whether
artifacts are separate files or merged into one), the serialization format (see
[C-SERIALIZATION](#c-serialization)), the field mapping from canonical fields to
host-specific keys (see [C-FRONTMATTER-FIDELITY](#c-frontmatter-fidelity)), and any
applicable budget (see [C-BUDGET-HONESTY](#c-budget-honesty)). Where the chosen serialization
format carries frontmatter, the matrix MUST also declare the frontmatter dialect.

Host backends MUST NOT hardcode layout, file paths, frontmatter key names, or key casing
outside the matrix; those facts MUST be expressed as matrix data. Where a field mapping
cannot be expressed declaratively, weaft MAY apply a code-level transform. Such a transform
MUST be a pure function of the resolved matrix cell and the artifact, and MUST NOT introduce
host behavior beyond what the matrix declares.

**Rationale.** A single declarative source of host and kind facts makes adding a host or kind
primarily a data change, and prevents the v1 failure mode in which backend code bypassed the
matrix and the matrix became decorative. Restricting the code-level escape hatch to a pure
cell-plus-artifact function keeps the matrix the authoritative, inspectable description of
host behavior. Frontmatter dialect is required only where it is meaningful, since config
formats such as JSON or TOML carry no frontmatter.

### C-SUPPORT-DISPOSITION

**Support disposition: native, fold, or drop** — normative.

For every (host, kind) pair the matrix MUST resolve to exactly one disposition: native,
fold-into-another-kind, or drop. A native disposition MUST emit the artifact in the host's
own representation. A fold-into disposition MUST place the artifact's rendered content into
the file of the named target kind for that host. The fold-into target kind MUST be declared
for that host and MUST itself resolve to a native disposition, and a fold chain MUST NOT
exceed one hop, so folding always terminates at an emitted file. A drop disposition MUST NOT
emit a file and MUST produce a warning diagnostic naming the artifact and the host. weaft
MUST NOT silently omit an artifact that a host cannot represent.

**Rationale.** Hosts differ in which kinds they support and how — a generic `AGENTS.md` folds
skills into one file and cannot represent standalone subagents at all. Making disposition
explicit data lets weaft adapt output per host while staying loud about anything it cannot
carry. Requiring a fold target to be declared and native, with at most one hop, guarantees
termination and keeps the resolved output predictable.

### C-PIPELINE

**Compilation pipeline stages** — normative.

A compilation MUST proceed through these ordered stages: parse, resolve, render, map,
serialize, emit, and merge. Template rendering MUST be the only stage that branches on host
semantics through user-authored templates. The map, serialize, emit, and merge stages MUST
NOT execute user templates; they MUST act only on declared matrix data and already-rendered
content.

**Rationale.** Confining host-semantic, template-driven logic to a single render stage — with
mapping, serialization, and layout downstream as pure, data-driven transforms — keeps
backends thin and behavior predictable, and preserves weaft's separation between host-aware
rendering and format/layout emission.

### C-TEMPLATE-CONTEXT

**Template capability exposure is strict** — normative.

During rendering, weaft MUST expose the active host's capabilities to the template. Access to
an undefined capability field MUST be a hard error that fails the build. Host-wide facts MUST
be addressable independently of the current kind. Facts specific to the artifact's kind MUST
be addressable as kind-scoped capabilities. A template MUST be able to query whether the host
supports a given artifact kind.

**Rationale.** Strict undefined access turns a capability typo into an actionable build
failure instead of silently empty output, preserving weaft's fail-loud invariant. Scoping
kind-specific facts to the current kind prevents a template from accidentally reading the
wrong kind's capabilities.

### C-SERIALIZATION

**Multi-format serialization, per (host, kind)** — normative.

weaft MUST be able to emit artifacts as YAML frontmatter with a Markdown body, as TOML, as
JSON, as plain Markdown, and as Markdown-with-frontmatter variants such as `.mdc`. The
serialization format and the frontmatter dialect MUST be selectable per (host, kind) pair,
not fixed per host. The serialization stage MUST own the framing that combines serialized
fields with rendered body content for its format.

**Rationale.** A single host can require different formats for different kinds — for example,
TOML subagent definitions alongside YAML-frontmatter skills — so format cannot be a per-host
constant. Framing differs by format (a fenced YAML block versus a TOML field that carries the
body), so it must live with serialization rather than with layout.

### C-FRONTMATTER-FIDELITY

**Frontmatter key and casing fidelity** — normative.

Emitted field keys and their casing MUST match each host's documented representation exactly,
as declared in the matrix field mapping. A canonical source field MUST be renamed, recased,
dropped, or folded according to the (host, kind) field mapping; weaft MUST NOT emit a
canonical field name that the host does not use.

**Rationale.** Output must be valid for the target host. Hosts deliberately differ in key
spelling and casing — for example a hyphenated tool-allowlist key versus a camelCase
always-apply key — and emitting the wrong key produces files the host silently ignores.

### C-BUDGET-HONESTY

**Soft budgets versus hard limits** — normative.

weaft MUST distinguish soft budgets from hard limits. A soft budget is a weaft heuristic;
weaft MUST treat it as advisory and MUST escalate it to a build failure only in strict mode,
an opt-in build flag that turns soft-budget violations into errors. A hard limit is a
documented host limit; weaft MUST report a violation of a hard limit as an error regardless
of strict mode. Token-based counts MUST be labeled as approximate wherever they are surfaced.
A hard limit expressed in bytes MUST be checked against the final emitted file.

**Rationale.** Conflating weaft's own heuristics with real host limits would misrepresent
both. Some hosts impose real, silently-truncating size caps that must fail loudly, whereas
weaft's token budgets are approximations and must say so.

### C-SOURCE-LAYOUT

**v2 source project layout** — normative.

A v2 source project MUST organize artifacts by kind, with an artifact's location determining
its kind. An artifact MAY override its kind through a field in its own frontmatter.
Project-wide singleton configuration — including permissions/settings, MCP servers, ignore
patterns, and plugin identity — MUST be declarable in the project manifest. The manifest MUST
declare the project's supported hosts.

**Rationale.** Organizing by kind mirrors how each host lays out its own configuration and
keeps authored source readable, while configuration that is not naturally one-file-per-artifact
belongs in the manifest. Locating the kind override in the artifact's own frontmatter keeps an
artifact self-describing. Because authors depend on this structure, it is a normative
contract.

### C-REGISTRY

**Single-source host registry** — normative.

The set of supported hosts MUST derive from a single declared source consumed by capability
lookup, emission, linting, and the CLI. Adding or removing a host MUST NOT require editing
host identifiers in more than one declaration per build component. The build MUST fail if a
declared host lacks an emission backend, or an emission backend lacks a declared host.

**Rationale.** In v1 a host identifier was repeated across at least six sites and drift
between them is a latent defect. A single declared source plus a consistency check makes host
registration mechanical and self-verifying.

### C-DETERMINISM

**Deterministic output** — normative.

For identical input, weaft MUST produce byte-identical output across runs. Field order within
serialized output, and emitted file paths, MUST be stable and derived from declared data
rather than from non-deterministic iteration. Emitted output MUST NOT embed wall-clock
timestamps or absolute filesystem paths; path separators MUST be normalized; and line endings
MUST be normalized, so output depends only on the project input and the capability matrix.

**Rationale.** Deterministic output is required for snapshot testing, reproducible builds, and
reviewable diffs — the mechanism by which weaft demonstrates that one source yields stable,
meaningfully different output per host. Excluding timestamps and absolute paths, and
normalizing separators and line endings, keeps that determinism portable across machines and
checkouts.

## Open questions

### C-OPEN-QUESTIONS

The following are unresolved at ratification and SHOULD be closed before the affected
behavior leaves the spec phase.

- **Research gap — `gemini-cli`:** the `gemini-cli` host has not been researched to the depth
  of the other hosts. Until it is verified, every `gemini-cli` capability MUST default to the
  most conservative disposition (unsupported, unbounded, or drop) so weaft never over-claims
  support.
- **Per-kind field validity:** field-level validity for the mcp_server, plugin, and hook
  kinds is specified at shape level only. Validation begins permissive — warning on unknown
  fields and erroring only on representability — and tightens as each host is verified.
- **Migration:** whether v1-to-v2 source migration is provided as tooling or as documented
  manual steps is deferred; v2 is a pre-1.0 breaking change.
- **Compatibility aliases:** whether legacy flat capability names are aliased for one release
  is deferred.
- **Currency:** each host capability value MUST carry a validation date, and a host MUST be
  re-validated before any of its capabilities is treated as stable.

## Determinations

Published as the normative v2 compilation model. The eleven specification clauses above are
ratified. The design decisions that implement them are recorded as ADRs, each linking back to
the clauses it answers:

- [ADR-0001 — weaft v2 compilation pipeline shape](../decisions/0001-weaft-v2-compilation-pipeline-shape.md)
  (C-PIPELINE, C-SUPPORT-DISPOSITION, C-DETERMINISM)
- [ADR-0002 — Field-mapping escape-hatch tiers](../decisions/0002-field-mapping-escape-hatch-tiers.md)
  (C-CAPABILITY-MATRIX, C-FRONTMATTER-FIDELITY)
- [ADR-0003 — Single-source host registry](../decisions/0003-single-source-host-registry.md)
  (C-REGISTRY)
- [ADR-0004 — v2 source project layout](../decisions/0004-v2-source-project-layout.md)
  (C-SOURCE-LAYOUT)
- [ADR-0005 — Budget honesty: soft tokens versus hard bytes](../decisions/0005-budget-honesty-soft-tokens-versus-hard-bytes.md)
  (C-BUDGET-HONESTY)
- [ADR-0006 — Serialization stack and format framing](../decisions/0006-serialization-stack-and-format-framing.md)
  (C-SERIALIZATION, C-DETERMINISM)

Migrated to mz-gov RFD format on 2026-05-31 by `gov-artifact-writer` (`claude-sonnet-4-6`)
from the govctl record `RFC-0001` (status `normative`, phase `spec`); the provenance
`timestamp` and `human_signoff` preserve the original 2026-05-30 ratification.

## Changelog

### v0.1.0 (2026-05-30)

Initial draft.
