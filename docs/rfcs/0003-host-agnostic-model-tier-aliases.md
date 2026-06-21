---
rfd: 0003
title: Host-agnostic model tier aliases for skill frontmatter and subagent definitions
state: discussion
authors: Stanislav Gonorovskii
discussion: <to be linked once a PR or thread exists>
---

# RFD 0003 — Host-agnostic model tier aliases for skill frontmatter and subagent definitions

<!-- mz-gov:agdr start -->
agdr_id: agdr-govern-2026_06_10_model_alias_tiers
timestamp: 2026-06-10T00:00:00Z
agent: gov-artifact-writer
model: claude-sonnet-4-6
trigger: user-prompt
status: executed
human_signoff: drmozg@2026-06-10T00:00:00Z
<!-- mz-gov:agdr end -->

## Summary

weaft compiles a single source skill or subagent to multiple host targets. The repo currently ships six hosts: `claude-code`, `cursor`, `opencode`, `codex`, `agents-md`, and `gemini-cli`. Each host uses vendor-specific model identifiers where it supports them at all, and handling differs significantly by both host and artifact kind. This RFD asks whether weaft should introduce a set of host-agnostic **model tier aliases** (e.g. `fast`, `medium`, `high`, `xhigh`) that the compiler resolves to each host's equivalent model at compile time, so a single source file can express capability intent without naming a vendor.

## Problem statement

**The actual current behavior by host and kind.** Before framing a problem to solve, the baseline must be correct:

| Host | Skill `model:` | Subagent `model:` |
|---|---|---|
| `claude-code` | Emitted (via `TargetOverride("model")` in `capability.rs:253`) | Emitted (via `TopLevel("model")` in `capability.rs:276`) |
| `cursor` | **Dropped** — `CURSOR_SKILL_FIELDS` has no `model` rule | Emitted (via `TopLevel("model")` in `capability.rs:380`) |
| `opencode` | Emitted (inherits `CLAUDE_SKILL_FIELDS`) | Emitted (inherits `CLAUDE_SUBAGENT_FIELDS`) |
| `codex` | Emitted (inherits `CLAUDE_SKILL_FIELDS`) | **Dropped** (`CODEX_SUBAGENT_FIELDS` at `capability.rs:492` has no `model` rule) |
| `agents-md` | **Dropped** — merged-markdown cell with no frontmatter | **Not supported** — subagents skipped entirely |
| `gemini-cli` | **Dropped** — all cells `Drop` (UNRESEARCHED target) | **Dropped** |

Key implications: a skill's `model` field is dropped silently for four of six hosts. The motivating harm — "produces a meaningless or invalid model field for Cursor or agents-md" — is incorrect for skills (Cursor drops it; agents-md has no frontmatter). The real portability concern is narrower: **subagent** `model` is emitted for claude-code and cursor, so a Claude-specific model id does reach Cursor's output unchanged. The problem is real for subagents; the problem statement overstates it for skills.

**The portability gap (scoped correctly).** A subagent that writes a vendor-specific `model: claude-sonnet-4-6` emits that string verbatim to both claude-code and cursor, even though cursor's subagent model catalog may differ. Skills are largely insulated today because most hosts drop the field.

**Existing portable mechanisms.** The repo's canonical subagent fixture (`examples/quickstart/agents/code-reviewer.md:8`) already uses `model: inherit` — a host-agnostic keyword that emits verbatim to both claude-code and cursor without change. This mechanism exists but is undocumented. Any proposal for host-agnostic model selection must compare itself against the `inherit` convention.

**Empirical premise — required measurement before the discussion concludes.** The scope and urgency of this RFD depends on how often skill/subagent authors actually write vendor-specific model IDs vs. `inherit`, blank, or nothing at all. The repo's two fixtures split on this axis: the subagent uses `model: inherit` (already portable); the skill uses a vendor-ID under a `claude-code` target-override (intentionally host-scoped). The discussion phase should include grepping the source corpus for `model:` values and classifying them (keyword / vendor-ID / blank) before committing to any aliasing option. If vendor-IDs are rare in practice, the honest determination is Option C or lint-only. This is a required data point, not an optional one.

**Model catalog ownership.** Options A, B, D, and E all require a mapping from tier alias to concrete model ID per host. No such catalog exists anywhere in the codebase — the capability matrix records *whether* a host consumes `model:` (`supports_per_skill_model`), not *which* models exist. Building and maintaining 6 hosts × N tiers of vendor model IDs is itself a continuous engineering obligation: vendors rotate model generations, retire model IDs, and release new identifiers. This is the exact "maintenance under host model-generation churn" burden (decision criterion 2) the RFD lists, but that burden is shifted to the weaft maintainer rather than eliminated.

**The maintenance surface.** Five of six hosts have `supports_per_skill_model: false` (`capability.rs:610,639,665,695,733`); only `claude-code` has `true`. Introducing alias resolution for a host that drops or ignores the `model` field recreates the "meaningless field" harm rather than removing it. Any implementation must be tied to this existing capability flag.

**The semantic intent.** "I want the cost-effective, low-latency model for a quick formatting pass" is a capability expression, not a model name. Encoding it as a tier alias (`fast`) could be more honest than a specific model ID — but only where the host actually consumes the field and a real alias-to-model mapping is being maintained.

## The irreversible decision vs the reversible implementation

**One-way door — the alias vocabulary.** The token set (the strings `fast`, `medium`, `high`, `xhigh` or whatever is chosen) is the only part of this decision that escapes into other people's source files. Once skill and subagent authors write `model: fast` and commit it, renaming or removing a tier requires either a coordinated migration all downstream projects must perform or a permanent backward-compat shim in weaft. This is the choice that must receive the highest scrutiny.

**Two-way doors — everything else.** The resolution site (map stage vs capability const vs backend), the mapping format (`HashMap` vs derived method vs YAML sidecar), the error policy (hard error vs warning vs fallthrough), and user extensibility are all internal implementation choices. They can be changed inside weaft after the fact with zero impact on author-facing source. These deserve proportionally less scrutiny than the vocabulary.

## Proposal / options on the table

### Decision criteria
For comparison across options: (1) authoring overhead; (2) maintenance under host model-generation churn; (3) cross-host portability honesty (no false-portability promise); (4) lint-ability for misconfigured aliases; (5) consistency with weaft's existing capability-matrix idiom; (6) implementation blast radius (which crates/seams must change).

### Option A — Fixed compiler-level alias vocabulary

weaft defines a small, fixed set of tier names in the `HostCapabilities` capability matrix. Each host's const maps each alias to its current model ID **only where that host emits a model field**. Resolution happens in the **`map` stage** (`crates/weaft-core/src/pipeline/map.rs`) via the `FieldTransformSet` trait (the `transform: true` flag, governed by ADR-0002) — not in `render_skill`/`render_agent`, which renders only the Jinja body and never touches frontmatter values.

**Pre-existing value collision.** The alias set must reserve or explicitly handle `inherit` — the live value currently emitted by the canonical quickstart subagent (`examples/quickstart/agents/code-reviewer.md:8`) and pinned in two snapshots (`compile_tests__code_reviewer_claude_code.snap:12`, `__code_reviewer_cursor.snap:8`). Under strict-mode reuse (below), `inherit ∉ {fast, medium, high, xhigh}` is an unknown alias → hard build error → the canonical example and both snapshots break on day one. `inherit` must be declared a reserved passthrough, or strict-mode cannot be applied to the `model` field without a grace period.

**Model catalog ownership.** "Each host's const maps each alias to its current model ID" requires a catalog that does not currently exist in the repo. Authoring and maintaining 6 hosts × N tiers of concrete vendor model IDs is itself a continuous obligation — this is maintenance *shifted to the weaft maintainer*, not maintenance eliminated.

**Implementation blast radius:** This requires hooking **two distinct `FieldSource` read paths** — `FieldSource::TargetOverride("model")` for skills and `FieldSource::TopLevel("model")` for subagents — and providing alias maps for all six hosts (even if three of them resolve to "drop/no-op"). It also touches `CodexTransforms` in `crates/weaft-targets/src/codex.rs`, the only existing `FieldTransformSet` implementation; determinism tests (`map.rs:383`, `:402`) pin exact `model` values and key order; snapshot fixtures (`compile_tests__safe_deleter_claude_code.snap`, `__code_reviewer_*.snap`) pin literal model strings and must be regenerated.

**Trade-offs:**
- (+) Zero overhead for skill authors once the catalog is built and aliases are adopted.
- (+) Strict-mode compilation can reject unknown aliases at build time — but `inherit` must be whitelisted first.
- (−) Requires authoring and maintaining a per-host model catalog that doesn't exist today.
- (−) Vocabulary is fixed and not extensible by individual projects.
- (−) Four tiers may not map cleanly onto every host's model lineup.
- (−) Non-trivial implementation blast radius (map stage seam, two FieldSource paths, six hosts, CodexTransforms, snapshot regen, `inherit` reservation).

### Option B — Project-extensible alias table in weaft.yaml

As Option A, but weaft defines the fixed aliases as a default mapping overrideable per-project under a `model_aliases:` key in `weaft.yaml`. The compiler merges project aliases over the defaults at load time.

**Trade-offs:**
- (+) Project teams can use domain names (`reasoning`, `embedding`) rather than capability-tier labels.
- (+) Future-proofs against hosts with unusual tier structures.
- (−) Increases the authoring surface; project-level aliases become tribal knowledge.
- (−) Aliases in weaft.yaml are invisible to skill authors unless they read the manifest.
- (−) Same blast radius as Option A for the compiler internals.

### Option C — No aliases; document the portability gap accurately

Explicitly document that `model:` is a host-specific field, clarify the per-(host, kind) behavior (table above), recommend leaving it blank for portable skills, and improve per-target override ergonomics. Since four of six hosts already drop the skill `model` field, the gap for skills is narrower than believed; the real problem is subagent portability across claude-code and cursor. The existing `model: inherit` convention already gives subagent authors a portable value without any compiler change.

**Trade-offs:**
- (+) No new abstraction; model names stay explicit and auditable.
- (+) No false-portability promise (an alias named `high` may mean different capability levels on different hosts).
- (+) No one-way door; the vocabulary stays host-native forever.
- (+) Zero implementation blast radius.
- (+) No per-host model catalog to author or maintain.
- (−) Subagent authors who need a *specific* model tier (not just any portable value) still must manage per-target model overrides.
- (−) Leaves the silent-drop behavior in place — an author's skill `model:` vanishes on four of six hosts with no lint warning; portability is achieved by silence, not signal.

### Option D — Aliases with literal fallthrough

As Option A, but unrecognized alias values (values not in the alias set) pass through verbatim to the backend rather than erroring. This allows gradual *adoption*: a skill can write a literal model id today and migrate to an alias later without a flag day.

**Note:** This allows gradual *adoption*, not reversal. Backing out a shipped alias requires the same migration as Option A — the fallthrough only helps the forward direction. There is no stated discrimination rule between a typo'd alias and a deliberate vendor id; a misspelled alias silently passes through.

**Trade-offs:**
- (+) Backward-compatible with existing literal model IDs.
- (−) Mixes alias and literal semantics; output does not distinguish resolved from passed-through values.
- (−) Harder to lint for misconfigured aliases (typo'd alias is indistinguishable from a vendor id).

### Option F — Prioritized model list per tier (first known wins)

Instead of mapping a tier to a single model ID per host, each tier maps to a **prioritized list of model IDs**. At compile time, the compiler walks the list and emits the first model that the target host recognizes as valid (i.e., present in the host's declared model roster). If no entry in the list matches, the tier falls back to `inherit` or drop — whatever the host's current default behavior is for a missing `model` field.

**Example source:**
```yaml
model_tiers:
  fast: ["claude-haiku-4-5", "cursor-fast", "gpt-4o-mini"]
  high: ["claude-opus-4-8", "cursor-pro", "gpt-4o"]
```

The compiler emits the first entry it can validate against the target host's roster. If no entry is recognizable, it emits nothing (same as today's blank behavior) or `inherit`, depending on the configured fallback policy.

**Why this addresses the catalog ownership and seam problems:**
- The per-host model roster (a simple set of known model ID strings per host in `capability.rs`) is smaller and more stable than a full alias-to-model bidirectional mapping — it is a validation allowlist, not a translation table.
- Resolution can be implemented as a filter pass over the list using the host's declared roster, rather than requiring a `FieldTransformSet` catalog-input wiring. The list itself lives in `weaft.yaml` or in the tier vocabulary const; the host's roster lives in `HostCapabilities`.
- When a vendor releases a new model generation, the new ID is prepended to the list; the old ID stays as a fallback for hosts that haven't updated their roster. This is an additive change, not a replacement — the "stale mapping" failure mode is reduced.
- The `inherit` keyword is naturally handled as a terminal fallback entry in the list, not a special case.

**`inherit` in this model:** Authors can write `fast` and if no list entry matches the host's roster, the tier resolves to `inherit`, making the behavior identical to today's portable value. This competes directly with the current `inherit` convention rather than conflicting with it.

**Unresolved implementation questions:**
- What is the host's "model roster" format — a `HashSet<&str>` in each `HostCapabilities` const? Only claude-code and cursor have known, enumerable model catalogs; opencode/codex may share Claude's; gemini-cli is UNRESEARCHED.
- What is the "first known" rule for hosts with no roster (gemini-cli, agents-md)? Drop? The last entry? `inherit`?
- Does the list live in `weaft.yaml` (project-configured) or in `capability.rs` (compiler-controlled)?

**Trade-offs:**
- (+) Resilient to model generation churn — prepend the new ID, leave the old as a fallback; no single-point failure.
- (+) Catalog ownership is distributed — project authors curate their own lists; the compiler just validates against a host roster.
- (+) `inherit` falls out naturally as a terminal fallback without being a special case.
- (+) Backward-compatible — if no list entry matches, behavior is identical to today's `inherit`/drop.
- (−) List authoring is more verbose than a single tier name.
- (−) The "first known" rule requires a per-host model roster that does not currently exist in the capability matrix (but it is an allowlist, not a full translation table — lower authoring cost than Option A's bidirectional mapping).
- (−) Cross-host portability is only as good as the overlap in the list — a list of Claude-only IDs resolves to drop/inherit on all other hosts.

### Option E — Capability-flag model selection (weaft's native idiom)

Instead of a named tier ladder, express model intent as structured capability hints (e.g. `model: { reasoning: high, latency: low }`) and let each host's `HostCapabilities` resolve to a concrete model. This mirrors weaft's existing design: the compiler already branches on `agent_supports_tools`, `agent_supports_readonly`, and the entire `HostCapabilities` struct is exposed as `{{ host.* }}` in templates.

**Trade-offs:**
- (+) Consistent with the project's core abstraction; degrades gracefully when a host has fewer tiers.
- (+) Avoids the flat-ladder problem (four tiers that don't map cleanly onto hosts with two or six levels).
- (−) More verbose at the authoring site than a single word.
- (−) Pushes resolution policy into the capability matrix, which is already load-bearing.
- (−) Requires a structured type for model hints in the IR (currently `Option<String>`), increasing parse complexity.

## Open questions

**Vocabulary — the one-way door (highest priority):**
- **Vocabulary shape**: Should tier names express cost/speed intent (`fast`, `medium`, `high`, `xhigh`) or capability intent (`mini`, `standard`, `pro`, `ultra`)? Is Option E (structured flags) the right abstraction instead of a tier ladder?
- **Vocabulary evolution**: How does weaft version, deprecate, or migrate a tier name after skill authors have adopted it? Options include append-only (never rename or remove), a deprecation lint warning for N minor versions, or a breaking-change gate. This must be decided before shipping, not retroactively.
- **Stale mapping validation**: When a vendor retires a model ID a tier points to, the const still compiles and weaft emits a dead model name silently. What is the validation mechanism — a lint pass, a documented review cadence, a CI check?

**Implementation — reversible decisions:**
- **Resolution site and seam shape**: The `map` stage is the correct location (not `render_skill`/`render_agent`, which renders only the Jinja body). However, the existing `FieldTransformSet` seam is a **stateless static** — its one live implementation (`CodexTransforms`) returns `None` to suppress a carrier field; it has no catalog input. Options A/B/D require either const-baked per-host maps inside each transform impl or a signature/ADR-0002 amendment. Option F (prioritized list) allows an alternative: a filter pass over the list using the host's declared model roster, which may fit as a pre-emit step outside the transform seam. The resolution site and its implementation shape must be confirmed before the vocabulary one-way door is committed.
- **Two FieldSource paths**: How does resolution cover both `FieldSource::TargetOverride("model")` (skills) and `FieldSource::TopLevel("model")` (subagents)?
- **Per-host alias behavior**: For hosts with `supports_per_skill_model: false` (five of six), should an alias drop (mirroring today's behavior), warn, or error? For gemini-cli (all-`Drop`, UNRESEARCHED), what does the alias resolve to?
- **`inherit` reservation and value taxonomy**: The `model:` field already carries three categories of value — reserved keywords (`inherit`, empty/absent), vendor-specific IDs (`claude-sonnet-4-6`), and proposed tier aliases (`fast`). Any implementation must define how each category is recognized and handled. Critically, `inherit` must be reserved as a passthrough before strict-mode error policy can be applied without breaking the canonical quickstart example.
- **Error policy**: Should an unresolved alias be a hard compile error (`UndefinedBehavior::Strict`, already in place), a warning, or a fallthrough (Option D)? The answer depends on whether `inherit` and other keywords are first carved out from the alias namespace.
- **User extensibility**: Is Option B (project-level overrides) worth the added complexity over Option A's fixed vocabulary?
- **Option F — prioritized list**: If the list approach is preferred, does the list live in `weaft.yaml` (project-configured) or in the compiler's vocabulary? What is the per-host "model roster" format (a `HashSet<&str>` in each `HostCapabilities` const)? What is the fallback behavior for hosts with no roster (gemini-cli, agents-md)?

## Determinations

<to be resolved in discussion>

## Provenance / Links

- **Commit**: 108c652a
- **Branch**: claude/weaft-v0-1-spec-pKbqm
- **PR**: none open at record time
- **Decision recorded**: 2026-06-10T00:00:00Z
- **Human sign-off**: drmozg@2026-06-10T00:00:00Z
