---
status: "accepted"
date: 2026-05-30
deciders: drmozg
---

# v2 source project layout

<!-- mz-gov:agdr start -->
agdr_id: agdr-adr-0004
timestamp: 2026-05-30T19:02:45+04:00
agent: gov-artifact-writer
model: claude-sonnet-4-6
trigger: user-prompt
status: executed
human_signoff: drmozg@2026-05-30T19:02:45+04:00
<!-- mz-gov:agdr end -->

## Context and Problem Statement

v2 expands from two artifact kinds to nine, so the source-project authoring model must
accommodate instructions, skills, subagents, commands, MCP servers, settings, plugins,
hooks, and ignore patterns. How should authors lay out a v2 source project so that an
artifact's kind is unambiguous, prose-bearing artifacts keep an editable body, and
configuration that is not naturally one-file-per-artifact has a home?

## Decision Drivers

- [RFD-0001 §C-SOURCE-LAYOUT](../rfcs/0001-weaft-v2-compilation-model.md#c-source-layout) —
  kind-organized source, manifest-declared singletons, and manifest-declared supported hosts.
- The change may break the v1 project format because weaft is pre-1.0
  ([RFD-0001 §C-OPEN-QUESTIONS](../rfcs/0001-weaft-v2-compilation-model.md#c-open-questions)).

## Considered Options

- Folder-per-kind, plus an optional per-artifact kind override, plus config singletons in
  `weaft.yaml`
- A single manifest describing every artifact inline
- A flat directory where kind is set only by a frontmatter field

## Decision Outcome

Chosen option: **organize source by kind in folders, allow an explicit per-artifact kind
override, and keep singleton configuration in `weaft.yaml`**.

1. **Folder is the kind signal** — an artifact's containing folder determines its kind; a
   frontmatter override exists for the long tail (C-SOURCE-LAYOUT).
2. **Singletons in the manifest** — permissions/settings, MCP servers, ignore patterns,
   plugin identity, and the supported-hosts list live in `weaft.yaml`, because they are not
   naturally one file per artifact.
3. **Bodies preserved** — prose-bearing kinds keep the frontmatter-plus-body Markdown file
   authors already use.

### Consequences

- Good, because project structure is legible and mirrors host conventions.
- Good, because authoring stays ergonomic for both prose kinds and config kinds.
- Good, because new kinds slot in as new folders without schema gymnastics.
- Bad, because v1 projects must migrate to the new layout — mitigated: weaft is pre-1.0;
  migration tooling versus documentation is deferred per C-OPEN-QUESTIONS.
- Neutral: the example project and quickstart must be rewritten to the v2 layout.

## Pros and Cons of the Options

### Folder-per-kind, optional kind override, config singletons in `weaft.yaml`

- Good, because it mirrors how each host lays out its own configuration; the mental model
  transfers.
- Good, because it keeps the frontmatter-plus-body file for prose-bearing kinds.
- Good, because it puts non-file-per-artifact configuration where configuration belongs.
- Bad, because there are more top-level folders in a project.

### A single manifest describing every artifact inline

- Bad, because it loses per-artifact file bodies and concentrates everything into one large
  file.
- Rejected: poor authoring ergonomics for prose-bearing kinds like instructions and skills.

### A flat directory, kind set only by a frontmatter field

- Bad, because the kind is hidden until a file is opened; there is no at-a-glance project
  structure.
- Rejected: less legible and more error-prone than folder-by-kind.

## More Information

- Governed by
  [RFD-0001 — weaft v2 compilation model](../rfcs/0001-weaft-v2-compilation-model.md);
  the migration question is tracked in RFD-0001's open questions.
- Migrated to mz-gov format on 2026-05-31 by `gov-artifact-writer` (`claude-sonnet-4-6`)
  from the govctl record `ADR-0004`; the provenance `timestamp` and `human_signoff`
  preserve the original 2026-05-30 decision and approval.
