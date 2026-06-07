---
rfd: 0002
title: Generalized Ask Gates — host-adaptive interactive prompt fragments
state: discussion
authors: Stanislav Gonorovskii
discussion: resolved via govern pipeline panel (sign-off 2026-06-08)
---

# RFD 0002 — Generalized Ask Gates — host-adaptive interactive prompt fragments

<!-- mz-gov:agdr start -->
agdr_id: agdr-0002
timestamp: 2026-06-08T00:00:00Z
agent: gov-artifact-writer
model: claude-opus-4-8
trigger: user-prompt
status: discussion
human_signoff: approved by Stanislav Gonorovskii (drmozg@gmail.com) 2026-06-08
<!-- mz-gov:agdr end -->

## Summary

weaft ships a built-in capability-matrix-driven `ask` macro (`weaft/ask.j2`) that already renders
host-appropriate interactive-prompt guidance for five host profiles. The macro covers the main use
cases, but the ergonomics of consuming it (an explicit `{% from "weaft/ask.j2" import ask %}` import
and an explicit `host` argument) add ceremony to every skill that asks a question. This RFD asks
whether the macro should be elevated to a compiler-registered global, extended with additional
features (named gates, declarations across skills), or left as-is; and if elevated, what API
stability commitment that implies.

## What already exists

`crates/weaft-core/src/templates/ask.j2` is a built-in Jinja macro already registered in the
compiler's minijinja environment. It is importable from any skill or subagent body:

```jinja
{% from "weaft/ask.j2" import ask %}
{{ ask(host, "Which files should I delete?", options=["build/", "dist/"]) }}
```

The macro reads three `HostCapabilities` fields to route per-host:

| Field | Variants / type | Purpose |
|---|---|---|
| `ask_user_support` | `AskUserSupport` enum: `Structured`/`NonBlocking`/`None` | Which ask mode the host supports |
| `ask_user_primitive` | `Option<&'static str>` | The tool name (e.g. `"AskUserQuestion"`) for `Structured` hosts |
| `ask_user_syntax` | `&'static str` | Plain-text fallback phrasing for `None` hosts |
| `ask_user_in_subagents` | `bool` | Whether the structured tool is available inside subagent context |

Per-host matrix as of v0.1:

| Host | `ask_user_support` | Notes |
|---|---|---|
| claude-code | `Structured` | Uses `AskUserQuestion` tool |
| cursor | `NonBlocking` | Asks while continuing work |
| opencode | `Structured` | Uses structured tool |
| agents-md | `None` | Plain prose fallback |
| codex | `None` | Plain prose fallback |
| gemini-cli | `None` | Plain prose fallback |

**API status of `weaft/ask.j2`**: the import path is currently treated as an internal
implementation detail — it is not explicitly committed as public API in any governance artifact.
Options A and E inherit this ambiguity; if any published skill file references `weaft/ask.j2`, that
path is already a de-facto commitment that was never formally classified. Clarifying its status
is a prerequisite for accurately rating Options A and E's reversibility.

A dedicated lint pass (`crates/weaft-core/src/lint/ask_user.rs`) already detects `weaft/ask.j2`
usage and warns when the target host has `AskUserSupport::None` (no structured tool), or when a
subagent uses a host's question tool that is filtered in subagent context
(`ask_user_in_subagents: false`). The `examples/quickstart/skills/safe-deleter.md` uses
`{{ host.ask_user_syntax }}` directly as a simpler per-host branch.

## Problem statement

The existing `ask` macro solves the core rendering problem. Three narrower ergonomic and
representational gaps remain open:

1. **Import ceremony.** Every skill that asks a question must open with
   `{% from "weaft/ask.j2" import ask %}` and pass `host` explicitly. In skills with one or two
   ask calls this is mechanical boilerplate.

2. **No cross-skill gate reuse.** Named gates (e.g. "the standard delete-confirmation ask") cannot
   be declared once in a project and referenced by multiple skills. Each skill redeclares its
   questions inline. This may or may not matter at the current project scale.

3. **Lint coverage gap for manual branches.** `lint/ask_user.rs` only sees macro usage. A skill
   that bypasses the macro with `{% if host.id == "claude-code" %}` branches is invisible to lint
   and can produce host-invalid output silently.

Whether any of these gaps justifies a compiler-level change — versus documenting the current macro
as the intended pattern — is the open question this RFD structures.

## Proposal / options on the table

Comparison matrix (higher = better for that criterion):

| Criterion | A (extend macro) | B (compiler global) | C (new kind) | D (lint-only) | E (status quo) |
|---|---|---|---|---|---|
| Author ceremony | Medium | Low | Medium | High | High |
| Compiler scope change | None | Low | Large | None | None |
| Lint coverage | Current | Current + importless | Full (via kind) | Partial (convention) | Current |
| Blast radius | Low | Medium | Very large | Low | None |
| Reversibility | Two-way door | One-way door | One-way door (irreversible) | Two-way door | N/A |

---

### Option A — Extend the existing `weaft/ask.j2` macro (no compiler change)

The current macro is extended in-place: additional parameters (e.g. a `name` for gate reuse,
`blocking_hint` override), richer documentation, and a project-level `fragments/` convention for
named gates that re-export the built-in. Authors use the macro exactly as today.

**Reversibility**: **Two-way door.** Rollback cost = edit or revert a Jinja file. An afternoon.

**Pros:** Zero compiler change, immediately usable, extends what already exists.
**Cons:** Import ceremony (`{% from "weaft/ask.j2" import ask %}` + explicit `host`) persists; lint
coverage gap for manual branches unchanged; still no cross-skill gate-reuse primitive.
Note: the current lint already detects macro usage per-host — the gap is manual `{% if host.id %}`
branches, not total lint absence.

---

### Option B — Auto-prepend the `ask` import in the compiler's render pass (elevate the macro)

The compiler automatically prepends `{% from "weaft/ask.j2" import ask %}` to every template body
before rendering, so authors write `{{ ask(host, "question", options=[...]) }}` in their skill
files without a manual import line. The `ask` macro itself (`templates/ask.j2`) is unchanged; only
the render pass in `compile.rs` changes. The four existing `ask_user_*` fields in `HostCapabilities`
remain unchanged; no new capability fields are added.

**Implementation mechanism**: The auto-prepend approach (not a native Rust `add_global` function)
is critical for two reasons: (1) minijinja macros share the caller's Jinja render scope, so `agent
is defined` auto-detection in `ask.j2:14` continues to work correctly; a Rust native function
registered via `add_global` receives only its explicit arguments and cannot inspect the render
context, which would silently break subagent auto-degradation. (2) Keeping `ask` as a Jinja macro
means the implementation remains in a `.j2` file that can be versioned and swapped independently.
If minijinja's `add_global` is used, `subagent=true` would need to be a mandatory parameter
instead of being auto-detected — a regression in ergonomics.

**Reversibility**: **One-way door.** Once skill files shipped in a project omit the explicit
`{% from "weaft/ask.j2" import ask %}` import, those files stop working when the auto-prepend is
removed. Migration requires adding back the import line in every affected file. Adoption of Option B
requires an explicit API stability decision as a precondition (see Open Questions). There is no
existing escape hatch; removal would be a coordinated semver-major break.

**Blast radius of adopting Option B:**
- All six host consts in `capability.rs` are unaffected (no new field added — existing fields used).
- `compile.rs` render pass gains one auto-prepend step before each body template render.
- **`lint/ask_user::uses_ask()` at `lint/ask_user.rs:71` is a required co-change.** The probe
  currently checks for `"weaft/ask.j2"` and `"ask_user_primitive"` in the raw body. An importless
  skill body containing only `{{ ask(host, "q") }}` matches neither string, silently suppressing
  both lint codes (`ask_user_unsupported`, `ask_user_in_subagent`) for the promoted usage pattern.
  `uses_ask()` must be extended (e.g. also match `{{ ask(` or `ask(host,`) as a co-change shipped
  with Option B, not as a follow-up.
- Existing snapshot tests (8 files): unaffected. No current skill uses the macro in a way that
  changes rendered output — the `safe-deleter` skill uses `{{ host.ask_user_syntax }}` directly.
  New snapshots should be added for the importless form on Structured, NonBlocking, and None hosts.
- `weaft targets` CLI display: unaffected (no new `HostCapabilities` field).
- `docs/src/capability-matrix.md`: no update needed (capability fields unchanged).
- Both the `{% from "weaft/ask.j2" import ask %}` import form and the importless form coexist;
  the auto-prepend is additive to the authoring surface, not a breaking change for existing skills.

**Pros:** Eliminates import ceremony; call sites read more naturally; capability-matrix-driven
without any schema change; `agent is defined` auto-detection preserved (Jinja macro, not Rust fn).
**Cons:** One-way-door API commitment once adopted; requires `uses_ask()` extension as co-change;
the compiler gains a small semantic opinion (every body gets the import injected).

---

### Option C — New `ArtifactKind::AskGate` (or `Dialog`)

Add `ArtifactKind::AskGate` to the closed nine-variant enum. Each gate is a standalone file
(`ask-gates/confirm-delete.md`) that compiles to host-specific output. Skills reference gates by
name.

**Reversibility**: **Irreversible (one-way door).** An `ArtifactKind` variant that enters the IR,
lint, backends, and capability matrix cannot be removed without breaking every project that
references it. This is the strongest commitment in the option space.

**Concrete compile-time barriers to adding this kind:**
- `KindCapabilitiesTable([KindCapabilities; 9])` in `capability.rs` becomes `[...; 10]` — the
  array size literal, `index()` bijection, and all six host const `*_kinds()` builders must update.
- `ArtifactKind::all()` const and every exhaustive `match` arm across the pipeline breaks at
  compile time (this is the intentional safety net, per `kind.rs`).
- The pinned test at `kind.rs:191` (`all_contains_every_named_variant`) fails immediately.
- Adding this kind is itself a governance act under C-ARTIFACT-KINDS (RFD-0001), requiring its own
  capability matrix column, lint pass, and all-hosts disposition declaration before it can ship.

**Pros:** Maximum composability; consistent with the nine-kind model; cross-skill gate sharing is
first-class.
**Cons:** Very large blast radius; governed separately under C-ARTIFACT-KINDS; irreversible after
any adoption; disproportionate to the ceremony gap this RFD is actually trying to close.

---

### Option D — Lint-only enforcement (comment convention, no new syntax)

A lint pass recognizes `<!-- ask-gate: ... -->` comment markers in skill bodies and warns when
rendered output is likely host-inappropriate. Authors still write the body manually; weaft catches
common mistakes.

**Reversibility**: **Two-way door.** Rollback cost = disable a lint pass. An afternoon.

**Pros:** Minimal surface; backward-compatible.
**Cons:** Doesn't close the import-ceremony gap; convention is outside the compiler's semantic
model; provides only advisory coverage (lint, not enforcement); `<!-- ask-gate -->` is a
second convention parallel to `weaft/ask.j2` rather than a replacement.

---

### Option E — Status quo (document and leave as-is)

Accept that `{% from "weaft/ask.j2" import ask %}` is the correct authoring pattern; improve the
documentation and examples; no compiler change.

**Reversibility**: N/A.

**Pros:** Zero new surface; zero migration; the existing macro already covers structured,
non-blocking, and prose hosts; the ceremony is a one-time include per skill.
**Cons:** Import ceremony persists; manual `{% if host.id %}` branches remain invisible to lint;
no cross-skill gate reuse.

---

## Open questions

- **Is the import ceremony actually a material problem?** The `{% from %}` import is one line per
  skill. If the project's skill count is small and the macro is well-documented, Option E may be
  the correct answer.
- **Is cross-skill gate reuse needed?** If skills rarely share the same exact question, named gates
  add ceremony without payoff. If they do share, what is the right declaration site (frontmatter?
  project-level config? a separate `ask-gates/` folder)?
- **What is the right API stability commitment for Option B? (precondition for adoption)** This
  must be resolved before Option B can be adopted — not deferred to discussion. Is the importless
  `{{ ask(host, ...) }}` form stable API (like `{{ host.* }}`), or a convenience tier that can
  change with a deprecation notice? There is no existing escape hatch; removal is a coordinated
  semver-major break. If the team cannot commit to stability, Option B is not ready to adopt.
- **Blocking vs. non-blocking semantics:** `AskUserSupport::NonBlocking` (Cursor since 2.4) means
  the agent continues working while waiting — a behavioral difference, not just formatting. Any
  new abstraction layer must preserve this distinction or it degrades into "single generic phrasing
  suboptimal for every host." Open question: should `ask()` expose a `blocking=true/false` hint,
  or delegate to the matrix entirely?
- **Is this in scope for v0.1?** Options A and D are safe at any milestone (two-way doors). Options
  B and C should not be adopted until the irreversibility is explicitly justified. Option E defers
  all compiler changes and is always safe.

## Determinations

**Selected option: B — Auto-prepend the `ask` import in the render pass, AND drop the explicit `host` parameter from the macro.**

The committed call form is `{{ ask("question", options=[...]) }}` — no import line, no `host` argument. Both ergonomic gaps (import ceremony and the redundant `host` pass-through) are closed together, because they share one root cause: the macro can already read the render context.

### Sub-determination: drop the `host` parameter (read the render-context global instead)

The macro signature changes from `ask(host, question, options=[], multi=false, subagent=false)` to `ask(question, options=[], multi=false, subagent=false)`. Inside the macro, `host` resolves from the render-context global rather than a parameter.

**Feasibility is proven, not assumed.** The macro already reads `agent is defined` (`ask.j2:14`), and `agent` is never a macro parameter — it is only ever a render-context global set in `render_agent`. The passing test `ask_macro_autodetects_subagent_context` (`compile.rs:366`) confirms minijinja macros resolve names from the render context. `host` is set as a sibling of `agent` in the same `context!{}` block in all three render paths (`render_skill:100`, `render_agent:119`, `render_instruction:139`), so the macro can read it the same way. Passing `host` today is pure redundancy — the global is bound to a parameter of the same name.

This is a breaking change to the macro signature and is correct to make **now**, before the API-stability commitment below locks the call form. Making it after would be a semver-major break.

### Sub-determination: API stability tier (required precondition — resolved)

The importless, host-less `{{ ask("question", ...) }}` call form is **stable API**, versioned identically to `{{ host.* }}` template variables. Removal requires a semver-major bump and a coordinated migration. There is no escape hatch; this is a one-way door accepted knowingly. The signature is settled (`host` dropped) precisely so the form committed as stable is the final one.

### Sub-determination: auto-prepend injection site

Auto-prepend of `{% from "weaft/ask.j2" import ask %}` MUST be placed in `render_skill` and `render_agent` independently — **not** in the shared `render_body` helper. `render_instruction` does not receive the auto-prepend; an instruction body that needs `ask` retains the manual-import escape (and renders correctly, since `host` is in its context and `agent is defined` is false → non-subagent host output). This confines the injection to the two paths where it is wanted and answers the third-render-path concern explicitly.

### Sub-determination: required co-changes (must ship atomically)

1. **Macro signature + all call sites.** Drop `host` from `templates/ask.j2`; update the `ASK_BODY` / subagent test fixtures in `compile.rs` from `ask(host, ...)` to `ask(...)`. (No production skill currently uses the macro — `safe-deleter` uses `{{ host.ask_user_syntax }}` directly — so the call-site blast radius is the test fixtures only.)

2. **`lint/ask_user::uses_ask()` at `lint/ask_user.rs:71`** must be extended to match the importless form (e.g. `body.contains("{{ ask(")`), plus a unit-test assertion for that form, so `ask_user_unsupported` / `ask_user_in_subagent` still fire for the promoted pattern.

3. **Test: double-import coexistence** — assert a body that *still* carries the explicit `{% from "weaft/ask.j2" import ask %}` import renders correctly when the compiler also auto-prepends it (minijinja must tolerate the duplicate import). Must pass before ship.

4. **Test: importless form per host profile** — new tests for `{{ ask("q", ...) }}` against Structured, NonBlocking, and None hosts.

### Deferred (not decided in this RFD)

- Cross-skill named gate reuse (gap 2) — deferred; this determination closes only the ergonomic gaps (ceremony + host pass-through).
- `blocking=` hint parameter — deferred; the matrix handles NonBlocking semantics for now.
- Option C (`ArtifactKind::AskGate`) — not adopted; requires its own governance artifact under C-ARTIFACT-KINDS if ever pursued.

## Provenance / Links

- **Branch**: `claude/weaft-v0-1-spec-pKbqm`
- **Commit at sign-off**: `2fb00ca`
- **PR**: none open at record time
- **Governance task**: `.mz/task/2026_06_07_govern_ask_gate_generalize/` (routing, 3-iteration heavy critic panel, discussion syntheses)
- **Sign-off**: approved by Stanislav Gonorovskii (drmozg@gmail.com), 2026-06-08
- **Known-open at sign-off** (panel did not converge within MAX_DISCUSS_ITERATIONS; accepted with the resolutions recorded in Determinations): one-way-door API precondition, `render_instruction` scope, minijinja double-import coexistence — each addressed by a sub-determination or a required pre-ship test above.
