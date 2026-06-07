//! Host capability matrix — the load-bearing artifact of weaft.
//!
//! Each supported host (Claude Code, Cursor, generic AGENTS.md) is described by a
//! static [`HostCapabilities`] value. The struct is `Serialize`, so it is exposed to
//! templates verbatim as the `{{ host.* }}` context object. A single source skill or
//! subagent therefore renders differently per target without the author writing any
//! target-specific conditionals beyond `{% if host.* %}`.
//!
//! ## Sources of truth (validated 2026-05)
//! - Claude Code skills & subagents: <https://code.claude.com/docs/en/skills>
//! - Cursor rules & subagents: <https://cursor.com/docs/subagents>
//! - AGENTS.md: <https://agents.md>
//!
//! ## Honesty notes
//! - `max_skill_tokens` (8000 Claude / 6000 Cursor) are **weaft's own soft heuristics**,
//!   not host-documented hard limits. They drive the budget lint only.
//! - `tokenizer = Cl100kBase` is an **approximation**; Claude's real tokenizer is not
//!   public. Counts are within ~5–10% for typical English+code.

use serde::Serialize;

use crate::budget::{Budget, BudgetSeverity, BudgetUnit};
use crate::kind::ArtifactKind;
use crate::serfmt::SerFormat;

/// Static description of a single compilation target / host runtime.
#[derive(Debug, Clone, Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "capability matrix: each bool is an independent host feature flag, not a state enum"
)]
pub struct HostCapabilities {
    /// Stable identifier, e.g. `"claude-code"`.
    pub id: &'static str,
    /// Human-readable name, e.g. `"Claude Code"`.
    pub display_name: &'static str,
    /// Whether `weaft init` scaffolds this host by default. Data flag (WU-7) so the CLI
    /// default set is derived from the registry, not a hardcoded id list (C-REGISTRY).
    pub init_default: bool,

    // --- Skill features ---
    /// Does the host support a tool allowlist in skill frontmatter?
    pub supports_tool_allowlist: bool,
    /// Does the host support a per-skill model preference?
    pub supports_per_skill_model: bool,
    /// Can the skill bundle binary/markdown assets alongside it?
    pub supports_assets: bool,
    /// Can the host invoke skills as slash commands?
    pub supports_slash_commands: bool,
    /// Does the host speak MCP?
    pub supports_mcp: bool,

    // --- Subagent features ---
    /// Does the host support standalone subagent definitions?
    pub supports_subagents: bool,
    /// Free-form name of the primitive used to dispatch to a subagent
    /// (for the LLM to reference in prose), e.g. `"Task"` / `"Agent"`.
    pub subagent_tool: Option<&'static str>,
    /// Where compiled subagent files land for this host.
    pub agent_layout: AgentLayout,
    /// Does the subagent frontmatter carry a tool allowlist (`tools:`)?
    pub agent_supports_tools: bool,
    /// Does the subagent frontmatter carry a `readonly:` flag (Cursor-style)?
    pub agent_supports_readonly: bool,

    // --- Permission / safety model ---
    pub permission_model: PermissionModel,
    /// Free-form description of how to ask the user, emitted into the body. Used as the
    /// prose fallback by the built-in `ask` macro for [`AskUserSupport::None`] hosts.
    pub ask_user_syntax: &'static str,
    /// How the host can ask the user a *structured* clarifying question.
    pub ask_user_support: AskUserSupport,
    /// Runtime tool name the rendered guidance should reference (e.g. `"AskUserQuestion"`).
    /// `None` when the host has no named, author-referenceable question tool.
    pub ask_user_primitive: Option<&'static str>,
    /// Is that question primitive usable *inside a subagent*? Claude's `AskUserQuestion`
    /// is filtered out of subagents even when allow-listed, so this is `false` there.
    pub ask_user_in_subagents: bool,

    // --- Budgeting ---
    /// Soft token budget (weaft heuristic, not a host hard limit). `None` = unbounded.
    pub max_skill_tokens: Option<usize>,
    pub tokenizer: Tokenizer,

    // --- Layout / dialect ---
    pub skill_layout: SkillLayout,
    pub frontmatter_dialect: FrontmatterDialect,

    // --- Kind-scoped capability cells (WU-6) ---
    /// Per-`(host, kind)` facts (disposition / layout / format / field-map / budget) that v1
    /// hardcoded in the backends. Every host populates all nine cells (fail-closed).
    ///
    /// Skipped from `Serialize`: the `{{ host.* }}` context exposes only host-wide facts in
    /// v1, and kind-scoped template access is added later (WU-10) through a dedicated
    /// projection, not by serializing the raw table. Skipping here keeps the v1 template
    /// context byte-identical so existing snapshots stay valid.
    #[serde(skip)]
    pub kinds: KindCapabilitiesTable,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionModel {
    /// Host requires the LLM to ask the user before destructive ops.
    Explicit,
    /// Host gates ops itself via an approval UI.
    Implicit,
    /// Host runs everything sandboxed; no asking needed.
    Sandboxed,
}

/// How a host can ask the user a *structured* clarifying question (distinct from the
/// destructive-op gating captured by [`PermissionModel`]).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AskUserSupport {
    /// Dedicated, blocking multiple-choice question tool (Claude `AskUserQuestion`,
    /// opencode `question`).
    Structured,
    /// Can ask clarifying questions, but non-blocking and with no documented schema
    /// (Cursor since 2.4 — the agent keeps working while it waits for the answer).
    NonBlocking,
    /// No author-declarable question primitive; rendered guidance falls back to prose.
    None,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Tokenizer {
    /// Approximates Claude / GPT-4 family.
    Cl100kBase,
    /// GPT-4o family.
    O200kBase,
    /// Rough approximation for Gemini (uses cl100k tables).
    GeminiApprox,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillLayout {
    /// `.claude/skills/<name>/SKILL.md` + assets.
    ClaudeStyle,
    /// `.cursor/rules/<name>.mdc`.
    CursorMdc,
    /// `AGENTS.md` at project root.
    AgentsMd,
    /// `.opencode/skills/<name>/SKILL.md`.
    OpencodeSkill,
    /// `.agents/skills/<name>/SKILL.md` (Agent Skills standard; read by Codex).
    CodexSkill,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentLayout {
    /// `.claude/agents/<name>.md`.
    ClaudeAgents,
    /// `.cursor/agents/<name>.md`.
    CursorAgents,
    /// `.opencode/agent/<name>.md` (markdown + YAML frontmatter).
    OpencodeAgents,
    /// `.codex/agents/<name>.toml` (TOML document, not markdown).
    CodexAgents,
    /// Host has no subagent concept.
    None,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FrontmatterDialect {
    /// `---\n...\n---`
    YamlDashes,
    /// No frontmatter block.
    None,
    /// TOML document (Codex subagent files).
    Toml,
}

// --- WU-6: kind-scoped matrix cells ---
//
// The field-maps below reproduce the *exact* v1 backend field names, emitted key casing, and
// read sources (Fix 1), so the rewired v2 backends emit byte-identical output and the existing
// snapshots stay valid until the quickstart migration. The slice order *is* the deterministic
// emission order (C-DETERMINISM); each table is keyed by `ArtifactKind::index()`.

/// Soft token budget shared by skill cells (a weaft heuristic, not a host hard limit).
const fn skill_token_budget(limit: usize) -> Budget {
    Budget {
        limit,
        unit: BudgetUnit::Tokens(Tokenizer::Cl100kBase),
        severity: BudgetSeverity::Soft,
        source: "weaft heuristic",
    }
}

/// A `Drop` cell: the host cannot represent this kind, so the pipeline warns instead of
/// emitting a file. Layout/format are inert placeholders never read for a dropped kind.
const fn drop_cell(kind: ArtifactKind) -> KindCapabilities {
    KindCapabilities {
        kind,
        disposition: Disposition::Drop,
        layout: Layout {
            path_template: "",
            merge: false,
        },
        format: SerFormat::PlainMarkdown,
        frontmatter_dialect: None,
        field_map: FieldMap(&[]),
        budget: None,
    }
}

/// A `Native` plain-Markdown instruction/skill cell merged into one shared file (no
/// frontmatter), e.g. `AGENTS.md` / `CLAUDE.md`.
const fn merged_markdown_cell(
    kind: ArtifactKind,
    path_template: &'static str,
    budget: Option<Budget>,
) -> KindCapabilities {
    KindCapabilities {
        kind,
        disposition: Disposition::Native,
        layout: Layout {
            path_template,
            merge: true,
        },
        format: SerFormat::PlainMarkdown,
        frontmatter_dialect: None,
        field_map: FieldMap(&[]),
        budget,
    }
}

// Claude Code field-maps.
const CLAUDE_SKILL_FIELDS: &[FieldRule] = &[
    FieldRule {
        canonical: FieldSource::TopLevel("name"),
        emitted: Some("name"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TopLevel("description"),
        emitted: Some("description"),
        transform: false,
    },
    // Authored under `targets.claude-code`, emitted hyphenated (v1 snapshot casing).
    FieldRule {
        canonical: FieldSource::TargetOverride("allowed_tools"),
        emitted: Some("allowed-tools"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TargetOverride("model"),
        emitted: Some("model"),
        transform: false,
    },
];

const CLAUDE_SUBAGENT_FIELDS: &[FieldRule] = &[
    FieldRule {
        canonical: FieldSource::TopLevel("name"),
        emitted: Some("name"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TopLevel("description"),
        emitted: Some("description"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TopLevel("tools"),
        emitted: Some("tools"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TopLevel("model"),
        emitted: Some("model"),
        transform: false,
    },
];

const fn json_singleton_cell(
    kind: ArtifactKind,
    path_template: &'static str,
    merge: bool,
) -> KindCapabilities {
    KindCapabilities {
        kind,
        disposition: Disposition::Native,
        layout: Layout {
            path_template,
            merge,
        },
        format: SerFormat::Json,
        frontmatter_dialect: None,
        field_map: FieldMap(&[]),
        budget: None,
    }
}

const fn claude_code_kinds() -> KindCapabilitiesTable {
    KindCapabilitiesTable([
        // 0 Instruction
        merged_markdown_cell(ArtifactKind::Instruction, "CLAUDE.md", None),
        // 1 Skill
        KindCapabilities {
            kind: ArtifactKind::Skill,
            disposition: Disposition::Native,
            layout: Layout {
                path_template: "skills/{name}/SKILL.md",
                merge: false,
            },
            format: SerFormat::YamlFrontmatterMarkdown,
            frontmatter_dialect: Some(FrontmatterDialect::YamlDashes),
            field_map: FieldMap(CLAUDE_SKILL_FIELDS),
            budget: Some(skill_token_budget(8000)),
        },
        // 2 Subagent
        KindCapabilities {
            kind: ArtifactKind::Subagent,
            disposition: Disposition::Native,
            layout: Layout {
                path_template: "agents/{name}.md",
                merge: false,
            },
            format: SerFormat::YamlFrontmatterMarkdown,
            frontmatter_dialect: Some(FrontmatterDialect::YamlDashes),
            field_map: FieldMap(CLAUDE_SUBAGENT_FIELDS),
            budget: None,
        },
        // 3 Command
        drop_cell(ArtifactKind::Command),
        // 4 McpServer — all declared servers aggregate (in parse) into one `mcpServers`-enveloped
        // document, emitted as a single (merge:false) JSON file at the repo ROOT `.mcp.json` (the
        // host-valid path; NOT `.claude/`).
        json_singleton_cell(ArtifactKind::McpServer, ".mcp.json", false),
        // 5 Settings — single JSON document (Fix 6).
        json_singleton_cell(ArtifactKind::Settings, ".claude/settings.json", false),
        // 6 Plugin
        drop_cell(ArtifactKind::Plugin),
        // 7 Hook
        drop_cell(ArtifactKind::Hook),
        // 8 Ignore
        drop_cell(ArtifactKind::Ignore),
    ])
}

// Cursor field-maps. Note: NO `name` rule on skills (the v1 .mdc snapshot omits it).
const CURSOR_SKILL_FIELDS: &[FieldRule] = &[
    FieldRule {
        canonical: FieldSource::TopLevel("description"),
        emitted: Some("description"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TargetOverride("globs"),
        emitted: Some("globs"),
        transform: false,
    },
    // camelCase emitted key (v1 snapshot casing).
    FieldRule {
        canonical: FieldSource::TargetOverride("always_apply"),
        emitted: Some("alwaysApply"),
        transform: false,
    },
];

const CURSOR_SUBAGENT_FIELDS: &[FieldRule] = &[
    FieldRule {
        canonical: FieldSource::TopLevel("name"),
        emitted: Some("name"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TopLevel("description"),
        emitted: Some("description"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TopLevel("model"),
        emitted: Some("model"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TopLevel("readonly"),
        emitted: Some("readonly"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TopLevel("is_background"),
        emitted: Some("is_background"),
        transform: false,
    },
];

const fn cursor_kinds() -> KindCapabilitiesTable {
    KindCapabilitiesTable([
        drop_cell(ArtifactKind::Instruction),
        // 1 Skill
        KindCapabilities {
            kind: ArtifactKind::Skill,
            disposition: Disposition::Native,
            layout: Layout {
                path_template: "rules/{name}.mdc",
                merge: false,
            },
            format: SerFormat::Mdc,
            frontmatter_dialect: Some(FrontmatterDialect::YamlDashes),
            field_map: FieldMap(CURSOR_SKILL_FIELDS),
            budget: Some(skill_token_budget(6000)),
        },
        // 2 Subagent
        KindCapabilities {
            kind: ArtifactKind::Subagent,
            disposition: Disposition::Native,
            layout: Layout {
                path_template: "agents/{name}.md",
                merge: false,
            },
            format: SerFormat::YamlFrontmatterMarkdown,
            frontmatter_dialect: Some(FrontmatterDialect::YamlDashes),
            field_map: FieldMap(CURSOR_SUBAGENT_FIELDS),
            budget: None,
        },
        drop_cell(ArtifactKind::Command),
        drop_cell(ArtifactKind::McpServer),
        drop_cell(ArtifactKind::Settings),
        drop_cell(ArtifactKind::Plugin),
        drop_cell(ArtifactKind::Hook),
        drop_cell(ArtifactKind::Ignore),
    ])
}

const fn agents_md_kinds() -> KindCapabilitiesTable {
    KindCapabilitiesTable([
        // 0 Instruction — shares the merged AGENTS.md with skills.
        merged_markdown_cell(ArtifactKind::Instruction, "AGENTS.md", None),
        // 1 Skill — a section of the merged AGENTS.md, no frontmatter (v1 snapshot).
        merged_markdown_cell(ArtifactKind::Skill, "AGENTS.md", None),
        // 2 Subagent — agents-md has no subagent format, so it drops (warning).
        drop_cell(ArtifactKind::Subagent),
        drop_cell(ArtifactKind::Command),
        drop_cell(ArtifactKind::McpServer),
        drop_cell(ArtifactKind::Settings),
        drop_cell(ArtifactKind::Plugin),
        drop_cell(ArtifactKind::Hook),
        drop_cell(ArtifactKind::Ignore),
    ])
}

// opencode skill/subagent share the Claude YAML-frontmatter field shape.
const fn opencode_kinds() -> KindCapabilitiesTable {
    KindCapabilitiesTable([
        drop_cell(ArtifactKind::Instruction),
        // 1 Skill
        KindCapabilities {
            kind: ArtifactKind::Skill,
            disposition: Disposition::Native,
            layout: Layout {
                path_template: ".opencode/skills/{name}/SKILL.md",
                merge: false,
            },
            format: SerFormat::YamlFrontmatterMarkdown,
            frontmatter_dialect: Some(FrontmatterDialect::YamlDashes),
            field_map: FieldMap(CLAUDE_SKILL_FIELDS),
            budget: None,
        },
        // 2 Subagent
        KindCapabilities {
            kind: ArtifactKind::Subagent,
            disposition: Disposition::Native,
            layout: Layout {
                path_template: ".opencode/agent/{name}.md",
                merge: false,
            },
            format: SerFormat::YamlFrontmatterMarkdown,
            frontmatter_dialect: Some(FrontmatterDialect::YamlDashes),
            field_map: FieldMap(CLAUDE_SUBAGENT_FIELDS),
            budget: None,
        },
        drop_cell(ArtifactKind::Command),
        drop_cell(ArtifactKind::McpServer),
        drop_cell(ArtifactKind::Settings),
        drop_cell(ArtifactKind::Plugin),
        drop_cell(ArtifactKind::Hook),
        drop_cell(ArtifactKind::Ignore),
    ])
}

// Codex subagent is a TOML document; its rendered body is carried in `developer_instructions`
// (flagged transform, encoded in WU-13/WU-16), not appended as prose.
const CODEX_SUBAGENT_FIELDS: &[FieldRule] = &[
    FieldRule {
        canonical: FieldSource::TopLevel("name"),
        emitted: Some("name"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TopLevel("description"),
        emitted: Some("description"),
        transform: false,
    },
    FieldRule {
        canonical: FieldSource::TopLevel("developer_instructions"),
        emitted: Some("developer_instructions"),
        transform: true,
    },
];

const fn codex_kinds() -> KindCapabilitiesTable {
    KindCapabilitiesTable([
        // 0 Instruction — merged AGENTS.md with the one concrete HARD byte budget (Fix 5):
        // Codex silently truncates this file past ~32 KiB.
        merged_markdown_cell(
            ArtifactKind::Instruction,
            "AGENTS.md",
            Some(Budget {
                limit: 32_768,
                unit: BudgetUnit::Bytes,
                severity: BudgetSeverity::Hard,
                source: "Codex AGENTS.md ~32 KiB cap (ADR-0005)",
            }),
        ),
        // 1 Skill — Agent Skills YAML-frontmatter standard.
        KindCapabilities {
            kind: ArtifactKind::Skill,
            disposition: Disposition::Native,
            layout: Layout {
                path_template: ".agents/skills/{name}/SKILL.md",
                merge: false,
            },
            format: SerFormat::YamlFrontmatterMarkdown,
            frontmatter_dialect: Some(FrontmatterDialect::YamlDashes),
            field_map: FieldMap(CLAUDE_SKILL_FIELDS),
            budget: None,
        },
        // 2 Subagent — TOML document.
        KindCapabilities {
            kind: ArtifactKind::Subagent,
            disposition: Disposition::Native,
            layout: Layout {
                path_template: ".codex/agents/{name}.toml",
                merge: false,
            },
            format: SerFormat::Toml,
            frontmatter_dialect: Some(FrontmatterDialect::Toml),
            field_map: FieldMap(CODEX_SUBAGENT_FIELDS),
            budget: None,
        },
        drop_cell(ArtifactKind::Command),
        drop_cell(ArtifactKind::McpServer),
        drop_cell(ArtifactKind::Settings),
        drop_cell(ArtifactKind::Plugin),
        drop_cell(ArtifactKind::Hook),
        drop_cell(ArtifactKind::Ignore),
    ])
}

// gemini-cli is the conservative sixth host. It is UNRESEARCHED: until its real layout,
// frontmatter dialect, and per-kind support are validated, RFD-0001 §C-OPEN-QUESTIONS mandates
// the safest possible matrix so weaft never over-claims — every one of the nine kind cells is
// `Drop` (no file is ever emitted; a build targeting only gemini-cli raises a drop-warning per
// declared artifact and writes nothing). Promoting any cell to `Native`/`Fold` is a deliberate,
// reviewable edit gated on research, not a default. Validation date: pending (no source yet).
const fn gemini_cli_kinds() -> KindCapabilitiesTable {
    KindCapabilitiesTable([
        drop_cell(ArtifactKind::Instruction),
        drop_cell(ArtifactKind::Skill),
        drop_cell(ArtifactKind::Subagent),
        drop_cell(ArtifactKind::Command),
        drop_cell(ArtifactKind::McpServer),
        drop_cell(ArtifactKind::Settings),
        drop_cell(ArtifactKind::Plugin),
        drop_cell(ArtifactKind::Hook),
        drop_cell(ArtifactKind::Ignore),
    ])
}

pub const CLAUDE_CODE: HostCapabilities = HostCapabilities {
    id: "claude-code",
    display_name: "Claude Code",
    init_default: true,
    supports_tool_allowlist: true,
    supports_per_skill_model: true,
    supports_assets: true,
    supports_slash_commands: true,
    supports_mcp: true,
    supports_subagents: true,
    subagent_tool: Some("Task"),
    agent_layout: AgentLayout::ClaudeAgents,
    agent_supports_tools: true,
    agent_supports_readonly: false,
    permission_model: PermissionModel::Explicit,
    ask_user_syntax: "Pause and ask the user explicitly in chat before proceeding.",
    ask_user_support: AskUserSupport::Structured,
    ask_user_primitive: Some("AskUserQuestion"),
    ask_user_in_subagents: false,
    max_skill_tokens: Some(8000),
    tokenizer: Tokenizer::Cl100kBase,
    skill_layout: SkillLayout::ClaudeStyle,
    frontmatter_dialect: FrontmatterDialect::YamlDashes,
    kinds: claude_code_kinds(),
};

pub const CURSOR: HostCapabilities = HostCapabilities {
    id: "cursor",
    display_name: "Cursor",
    init_default: true,
    supports_tool_allowlist: false,
    supports_per_skill_model: false,
    supports_assets: false,
    supports_slash_commands: false,
    supports_mcp: true,
    // Cursor gained first-class subagents (.cursor/agents/<name>.md) — validated 2026-05.
    supports_subagents: true,
    subagent_tool: Some("Agent"),
    agent_layout: AgentLayout::CursorAgents,
    agent_supports_tools: false,
    agent_supports_readonly: true,
    permission_model: PermissionModel::Implicit,
    ask_user_syntax: "Output a clear question in the chat panel and wait for the user's reply.",
    // Non-blocking clarifying-question tool (Cursor 2.4); schema undocumented, so no named
    // primitive — guidance references it as "the ask question tool". Usable in subagents.
    ask_user_support: AskUserSupport::NonBlocking,
    ask_user_primitive: None,
    ask_user_in_subagents: true,
    max_skill_tokens: Some(6000),
    tokenizer: Tokenizer::Cl100kBase,
    skill_layout: SkillLayout::CursorMdc,
    frontmatter_dialect: FrontmatterDialect::YamlDashes,
    kinds: cursor_kinds(),
};

pub const AGENTS_MD: HostCapabilities = HostCapabilities {
    id: "agents-md",
    display_name: "AGENTS.md (generic)",
    init_default: false,
    supports_tool_allowlist: false,
    supports_per_skill_model: false,
    supports_assets: false,
    supports_slash_commands: false,
    supports_mcp: false,
    supports_subagents: false,
    subagent_tool: None,
    agent_layout: AgentLayout::None,
    agent_supports_tools: false,
    agent_supports_readonly: false,
    permission_model: PermissionModel::Explicit,
    ask_user_syntax: "Ask the user explicitly before any destructive action.",
    ask_user_support: AskUserSupport::None,
    ask_user_primitive: None,
    ask_user_in_subagents: false,
    max_skill_tokens: None,
    tokenizer: Tokenizer::Cl100kBase,
    skill_layout: SkillLayout::AgentsMd,
    frontmatter_dialect: FrontmatterDialect::None,
    kinds: agents_md_kinds(),
};

pub const OPENCODE: HostCapabilities = HostCapabilities {
    id: "opencode",
    display_name: "opencode",
    init_default: false,
    supports_tool_allowlist: false,
    supports_per_skill_model: false,
    // No documented mechanism for bundling/loading skill-sibling files on opencode's side.
    supports_assets: false,
    supports_slash_commands: false,
    supports_mcp: true,
    supports_subagents: true,
    subagent_tool: Some("Task"),
    agent_layout: AgentLayout::OpencodeAgents,
    // opencode agents use a per-tool `permission` map, not a tools allowlist or readonly flag.
    agent_supports_tools: false,
    agent_supports_readonly: false,
    permission_model: PermissionModel::Implicit,
    ask_user_syntax: "Use the question tool to ask the user and wait for their selection.",
    ask_user_support: AskUserSupport::Structured,
    ask_user_primitive: Some("question"),
    // Available to subagents subject to the `question` permission (docs flag only todowrite
    // as subagent-disabled); asking subagents should emit `permission.question: allow`.
    ask_user_in_subagents: true,
    max_skill_tokens: None,
    tokenizer: Tokenizer::Cl100kBase,
    skill_layout: SkillLayout::OpencodeSkill,
    frontmatter_dialect: FrontmatterDialect::YamlDashes,
    kinds: opencode_kinds(),
};

pub const CODEX: HostCapabilities = HostCapabilities {
    id: "codex",
    display_name: "OpenAI Codex",
    init_default: false,
    supports_tool_allowlist: false,
    supports_per_skill_model: false,
    // Agent Skills standard: a skill dir may bundle scripts/, references/, assets/.
    supports_assets: true,
    supports_slash_commands: false,
    supports_mcp: true,
    supports_subagents: true,
    // Codex invokes a subagent by referring to it by name, not via a named delegation tool.
    subagent_tool: None,
    agent_layout: AgentLayout::CodexAgents,
    agent_supports_tools: false,
    agent_supports_readonly: false,
    // OS-sandbox enforced (Seatbelt / bubblewrap+seccomp) × an approval policy.
    permission_model: PermissionModel::Sandboxed,
    ask_user_syntax: "Ask the user before crossing the sandbox boundary; Codex gates approvals itself.",
    // Codex has experimental runtime elicitation (`tool/requestUserInput`, MCP elicitations)
    // but nothing an emitted SKILL.md / agent TOML can declare — so guidance stays prose.
    ask_user_support: AskUserSupport::None,
    ask_user_primitive: None,
    ask_user_in_subagents: false,
    max_skill_tokens: None,
    tokenizer: Tokenizer::O200kBase,
    skill_layout: SkillLayout::CodexSkill,
    // Subagents are TOML documents (name/description/developer_instructions). Skills follow
    // the Agent Skills YAML-frontmatter standard; this names the headline subagent dialect.
    frontmatter_dialect: FrontmatterDialect::Toml,
    kinds: codex_kinds(),
};

/// The conservative sixth host. UNRESEARCHED — every host-wide flag is set to its safest value
/// (all features off, no ask primitive, no budget) to match the all-`Drop` kind table in
/// [`gemini_cli_kinds`]. RFD-0001 §C-OPEN-QUESTIONS: weaft must under-claim a host it has not
/// validated rather than emit something that may be wrong. Each field flips from this default
/// only when a real Gemini CLI source is consulted (validation date: pending).
pub const GEMINI_CLI: HostCapabilities = HostCapabilities {
    id: "gemini-cli",
    display_name: "Gemini CLI",
    init_default: false,
    supports_tool_allowlist: false,
    supports_per_skill_model: false,
    supports_assets: false,
    supports_slash_commands: false,
    supports_mcp: false,
    supports_subagents: false,
    subagent_tool: None,
    agent_layout: AgentLayout::None,
    agent_supports_tools: false,
    agent_supports_readonly: false,
    permission_model: PermissionModel::Explicit,
    ask_user_syntax: "Ask the user explicitly before any destructive action.",
    ask_user_support: AskUserSupport::None,
    ask_user_primitive: None,
    ask_user_in_subagents: false,
    max_skill_tokens: None,
    // Gemini CLI runs Gemini models, so token budgets (none today) would count with this approx.
    tokenizer: Tokenizer::GeminiApprox,
    skill_layout: SkillLayout::AgentsMd,
    frontmatter_dialect: FrontmatterDialect::None,
    kinds: gemini_cli_kinds(),
};

/// Look up a host by its stable id.
pub fn by_id(id: &str) -> Option<&'static HostCapabilities> {
    match id {
        "claude-code" => Some(&CLAUDE_CODE),
        "cursor" => Some(&CURSOR),
        "agents-md" => Some(&AGENTS_MD),
        "opencode" => Some(&OPENCODE),
        "codex" => Some(&CODEX),
        "gemini-cli" => Some(&GEMINI_CLI),
        _ => None,
    }
}

/// All known hosts, in display order.
pub fn all() -> &'static [&'static HostCapabilities] {
    &[
        &CLAUDE_CODE,
        &CURSOR,
        &AGENTS_MD,
        &OPENCODE,
        &CODEX,
        &GEMINI_CLI,
    ]
}

/// Every known host id, in registry declaration order. Derived from [`all`] so the id list
/// that backs the `UnknownTarget` help and the lint help strings has a single source
/// (C-REGISTRY) rather than a hardcoded literal that can drift from the matrix.
#[must_use]
pub fn known_ids() -> Vec<&'static str> {
    all().iter().map(|h| h.id).collect()
}

/// The host ids `weaft init` scaffolds by default, in registry order. Derived from the
/// [`HostCapabilities::init_default`] flag so the CLI default set follows the matrix.
#[must_use]
pub fn init_default_ids() -> Vec<&'static str> {
    all()
        .iter()
        .filter(|h| h.init_default)
        .map(|h| h.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_id_roundtrips_every_host() {
        for host in all() {
            assert_eq!(by_id(host.id).map(|h| h.id), Some(host.id));
        }
    }

    #[test]
    fn unknown_id_is_none() {
        assert!(by_id("gemini").is_none());
    }

    #[test]
    fn cursor_supports_subagents() {
        // Looked up at runtime (not a const assertion) — this is the researched fact
        // that distinguishes Cursor from AGENTS.md.
        let cursor = by_id("cursor").unwrap();
        assert!(cursor.supports_subagents);
        assert_eq!(cursor.agent_layout, AgentLayout::CursorAgents);
        assert!(!by_id("agents-md").unwrap().supports_subagents);
    }

    #[test]
    fn ask_user_support_matches_researched_matrix() {
        let claude = by_id("claude-code").unwrap();
        assert_eq!(claude.ask_user_support, AskUserSupport::Structured);
        assert_eq!(claude.ask_user_primitive, Some("AskUserQuestion"));
        // Claude's AskUserQuestion is filtered out of subagents (two primary-doc sources).
        assert!(!claude.ask_user_in_subagents);

        let cursor = by_id("cursor").unwrap();
        assert_eq!(cursor.ask_user_support, AskUserSupport::NonBlocking);
        assert!(cursor.ask_user_in_subagents);

        assert_eq!(
            by_id("agents-md").unwrap().ask_user_support,
            AskUserSupport::None
        );
    }

    #[test]
    fn new_targets_have_expected_capabilities() {
        let oc = by_id("opencode").unwrap();
        assert_eq!(oc.ask_user_support, AskUserSupport::Structured);
        assert_eq!(oc.ask_user_primitive, Some("question"));
        assert!(oc.ask_user_in_subagents);
        assert_eq!(oc.agent_layout, AgentLayout::OpencodeAgents);

        let cx = by_id("codex").unwrap();
        assert_eq!(cx.ask_user_support, AskUserSupport::None);
        assert_eq!(cx.permission_model, PermissionModel::Sandboxed);
        // Codex runs OpenAI models — count with the o200k approximation, not cl100k.
        assert_eq!(cx.tokenizer, Tokenizer::O200kBase);
        assert_eq!(cx.agent_layout, AgentLayout::CodexAgents);
        assert!(cx.supports_assets);
    }
}

// --- WU-2: kind-scoped capability sub-structs + declarative field-map ---
//
// These types make the per-`(host, kind)` facts that v1 hardcoded in backends — layout,
// format, frontmatter key casing, drop/fold disposition, budgets — into matrix *data*. They
// are `&'static`/const-friendly so each host const can construct its full table inline.

/// Where the `map` stage reads a canonical field's *value* from on the source artifact.
///
/// This distinction is the keystone of the v1→v2 migration: skill frontmatter fields are
/// authored under per-target override blocks, while subagent fields are top-level. Making it
/// data lets one generic `map` stage read each field from the correct place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldSource {
    /// Read from the artifact's per-target override block:
    /// `artifact.frontmatter.targets.override_for(host_id)` then this key. This is how SKILL
    /// fields (`allowed_tools`, `model`, `globs`, `always_apply`) are authored.
    TargetOverride(&'static str),
    /// Read from a top-level frontmatter field. This is how SUBAGENT fields (`tools`,
    /// `model`, `readonly`, `is_background`) are authored, and how the structural `name` /
    /// `description` are always read.
    TopLevel(&'static str),
}

impl FieldSource {
    /// The inner field name, independent of which source the value comes from.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        match self {
            FieldSource::TargetOverride(k) | FieldSource::TopLevel(k) => k,
        }
    }
}

/// One declarative field-mapping rule: where to read the canonical value, the key to emit it
/// under (or `None` to drop it), and whether the `map`-stage transform hook overrides it.
///
/// The three tiers of ADR-0002 live in this one type: `emitted: None` drops, an `emitted`
/// key differing from the canonical key recases/renames (e.g. `allowed_tools` →
/// `allowed-tools`), and `transform: true` flags a value the transform hook computes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FieldRule {
    pub canonical: FieldSource,
    pub emitted: Option<&'static str>,
    pub transform: bool,
}

/// An ordered field-map. The slice order *is* the deterministic emission order
/// (C-DETERMINISM), replacing the hand-ordered `frontmatter(vec![...])` of the v1 backends.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FieldMap(pub &'static [FieldRule]);

/// What a host does with an artifact of a given kind (C-SUPPORT-DISPOSITION).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    /// Emit a real file for this kind.
    Native,
    /// Fold this kind into another kind's output (e.g. an instruction into a merged file).
    Fold { into: ArtifactKind },
    /// The host cannot represent this kind; the pipeline emits a warning instead of a file.
    Drop,
}

/// Where an emitted file lands and whether multiple artifacts merge into it.
///
/// `path_template` is a raw template (e.g. `"skills/{name}/SKILL.md"`) substituted at emit
/// time — this data replaces the hardcoded `format!` paths in the v1 backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Layout {
    pub path_template: &'static str,
    pub merge: bool,
}

/// The full capability cell for one `(host, kind)` pair.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct KindCapabilities {
    pub kind: ArtifactKind,
    pub disposition: Disposition,
    pub layout: Layout,
    pub format: SerFormat,
    pub frontmatter_dialect: Option<FrontmatterDialect>,
    pub field_map: FieldMap,
    pub budget: Option<Budget>,
}

/// A host's cells for all nine kinds, keyed by [`ArtifactKind::index`].
///
/// A fixed-size array (not a `HashMap`) so the table is const-constructible and iteration is
/// deterministic. Each host const fills every slot — fail-closed: the build won't compile
/// until all nine cells exist.
#[derive(Debug, Clone, Copy)]
pub struct KindCapabilitiesTable(pub [KindCapabilities; 9]);

impl KindCapabilitiesTable {
    /// The cell for `kind`. Total over [`ArtifactKind::all`] — `index()` is a bijection onto
    /// `0..9`, so the array access never panics.
    #[must_use]
    pub fn get(&self, kind: ArtifactKind) -> &KindCapabilities {
        &self.0[kind.index()]
    }
}

/// WU-2: kind-scoped capability sub-structs, the declarative field-map, and the
/// `FieldSource` distinction (Fix 1). The types under test are authored in a later GREEN
/// step; until then these fail to compile with missing-symbol errors — the expected RED
/// state.
#[cfg(test)]
mod v2_kind_capability_tests {
    use super::*;
    use crate::budget::{Budget, BudgetSeverity, BudgetUnit};
    use crate::kind::ArtifactKind;
    use crate::serfmt::SerFormat;

    /// Build a representative cell for one kind. Every cell carries the same layout/format
    /// shape so the table-totality test only varies `kind`.
    fn cell_for(kind: ArtifactKind) -> KindCapabilities {
        KindCapabilities {
            kind,
            disposition: Disposition::Native,
            layout: Layout {
                path_template: "skills/{name}/SKILL.md",
                merge: false,
            },
            format: SerFormat::YamlFrontmatterMarkdown,
            frontmatter_dialect: Some(FrontmatterDialect::YamlDashes),
            field_map: FieldMap(&[]),
            budget: None,
        }
    }

    /// Assemble a full table, placing each kind's cell at its own `index()` slot so the
    /// array key (index) matches the cell's kind — exactly the invariant `get` relies on.
    fn full_table() -> KindCapabilitiesTable {
        // `KindCapabilities` is not `Default`, so seed every slot then overwrite by index.
        let mut cells: [KindCapabilities; 9] = [
            cell_for(ArtifactKind::Instruction),
            cell_for(ArtifactKind::Skill),
            cell_for(ArtifactKind::Subagent),
            cell_for(ArtifactKind::Command),
            cell_for(ArtifactKind::McpServer),
            cell_for(ArtifactKind::Settings),
            cell_for(ArtifactKind::Plugin),
            cell_for(ArtifactKind::Hook),
            cell_for(ArtifactKind::Ignore),
        ];
        for kind in ArtifactKind::all() {
            cells[kind.index()] = cell_for(*kind);
        }
        KindCapabilitiesTable(cells)
    }

    #[test]
    fn field_rule_with_no_emitted_key_is_a_drop() {
        // `emitted: None` is the drop tier of ADR-0002 (rename / recase / drop in one type).
        let rule = FieldRule {
            canonical: FieldSource::TopLevel("internal_only"),
            emitted: None,
            transform: false,
        };
        assert!(
            rule.emitted.is_none(),
            "a drop rule must carry no emitted key",
        );
    }

    #[test]
    fn field_rule_can_rename_and_recase() {
        // The non-drop tiers: an emitted key that differs from the canonical key encodes a
        // recase (e.g. the Claude `allowed-tools` hyphen casing).
        let rule = FieldRule {
            canonical: FieldSource::TargetOverride("allowed_tools"),
            emitted: Some("allowed-tools"),
            transform: false,
        };
        assert_eq!(rule.emitted, Some("allowed-tools"));
        assert_eq!(rule.canonical.key(), "allowed_tools");
    }

    #[test]
    fn field_source_roundtrips_its_inner_key() {
        // The `key()` accessor exposes the inner field name regardless of source variant.
        let over = FieldSource::TargetOverride("always_apply");
        let top = FieldSource::TopLevel("tools");
        assert_eq!(over.key(), "always_apply");
        assert_eq!(top.key(), "tools");
    }

    #[test]
    fn field_map_preserves_declared_order() {
        // FieldMap is an ordered slice; order is the deterministic emission order
        // (C-DETERMINISM), so the wrapper must expose the rules in declaration order.
        const RULES: &[FieldRule] = &[
            FieldRule {
                canonical: FieldSource::TopLevel("name"),
                emitted: Some("name"),
                transform: false,
            },
            FieldRule {
                canonical: FieldSource::TopLevel("description"),
                emitted: Some("description"),
                transform: false,
            },
        ];
        let map = FieldMap(RULES);
        let keys: Vec<&str> = map.0.iter().map(|r| r.canonical.key()).collect();
        assert_eq!(keys, ["name", "description"]);
    }

    #[test]
    fn table_get_is_total_over_every_kind() {
        // `get` must never panic and must return the cell whose `kind` matches the query,
        // for all nine kinds — proving the array is keyed by `index()` correctly.
        let table = full_table();
        for kind in ArtifactKind::all() {
            let cell = table.get(*kind);
            assert_eq!(
                cell.kind, *kind,
                "table.get({kind:?}) returned a cell for {:?}",
                cell.kind,
            );
        }
    }

    #[test]
    fn disposition_fold_names_a_target_kind() {
        // Fold carries the kind it folds into (C-SUPPORT-DISPOSITION).
        let fold = Disposition::Fold {
            into: ArtifactKind::Skill,
        };
        match fold {
            Disposition::Fold { into } => assert_eq!(into, ArtifactKind::Skill),
            Disposition::Native | Disposition::Drop => {
                panic!("constructed a Fold disposition")
            },
        }
    }

    #[test]
    fn kind_capabilities_carry_layout_and_optional_budget() {
        // A cell with a hard byte budget exposes it; the layout template is raw matrix data.
        let cell = KindCapabilities {
            kind: ArtifactKind::Instruction,
            disposition: Disposition::Native,
            layout: Layout {
                path_template: "AGENTS.md",
                merge: true,
            },
            format: SerFormat::PlainMarkdown,
            frontmatter_dialect: None,
            field_map: FieldMap(&[]),
            budget: Some(Budget {
                limit: 32_768,
                unit: BudgetUnit::Bytes,
                severity: BudgetSeverity::Hard,
                source: "Codex AGENTS.md ~32 KiB cap (ADR-0005)",
            }),
        };
        assert_eq!(cell.layout.path_template, "AGENTS.md");
        assert!(cell.layout.merge);
        assert!(cell.frontmatter_dialect.is_none());
        let budget = cell.budget.expect("instruction cell carries a budget");
        assert_eq!(budget.limit, 32_768);
        assert_eq!(budget.severity, BudgetSeverity::Hard);
    }
}

/// WU-6: kind-scoped capability matrix population across the five hosts (RED).
///
/// These tests assert the *populated* `kinds: KindCapabilitiesTable` on each host const —
/// the per-`(host, kind)` cells (disposition, layout, format, field-map with `FieldSource`,
/// budget) that v1 hardcoded in the backends. They fail to compile until WU-6 adds the
/// `kinds` field to [`HostCapabilities`] and every host const populates all nine cells —
/// the expected RED (missing-field on `host.kinds`). They must not be softened by stubbing
/// the field.
#[cfg(test)]
mod v2_matrix_tests {
    use super::*;
    use crate::budget::{BudgetSeverity, BudgetUnit};
    use crate::kind::ArtifactKind;

    /// Resolve a single `(host, kind)` cell off the populated matrix table.
    fn cell(host: &HostCapabilities, kind: ArtifactKind) -> &KindCapabilities {
        host.kinds.get(kind)
    }

    /// Find the field rule whose canonical source carries `key`, if the cell declares one.
    fn rule_for_key(c: &KindCapabilities, key: &str) -> Option<FieldRule> {
        c.field_map
            .0
            .iter()
            .copied()
            .find(|r| r.canonical.key() == key)
    }

    /// Does the cell emit any rule under the given emitted key?
    fn emits_key(c: &KindCapabilities, emitted: &str) -> bool {
        c.field_map.0.iter().any(|r| r.emitted == Some(emitted))
    }

    #[test]
    fn skill_and_subagent_dispositions_per_host() {
        // The headline support matrix: every host but agents-md represents skills AND subagents
        // natively; agents-md has no subagent concept, so its Subagent cell drops.
        let expected: [(&str, Disposition, Disposition); 5] = [
            ("claude-code", Disposition::Native, Disposition::Native),
            ("cursor", Disposition::Native, Disposition::Native),
            ("agents-md", Disposition::Native, Disposition::Drop),
            ("opencode", Disposition::Native, Disposition::Native),
            ("codex", Disposition::Native, Disposition::Native),
        ];
        for (id, skill_disp, sub_disp) in expected {
            let host = by_id(id).unwrap_or_else(|| panic!("host {id} must exist"));
            assert_eq!(
                cell(host, ArtifactKind::Skill).disposition,
                skill_disp,
                "{id} Skill disposition",
            );
            assert_eq!(
                cell(host, ArtifactKind::Subagent).disposition,
                sub_disp,
                "{id} Subagent disposition",
            );
        }
    }

    #[test]
    fn claude_skill_allowed_tools_rule_is_override_sourced_and_hyphen_cased() {
        // Fix-1 pinned at the matrix level: the Claude Skill `allowed_tools` rule reads from the
        // per-target override block (`TargetOverride("allowed_tools")`) and emits the hyphenated
        // `allowed-tools` key (the v1 snapshot casing). Both halves of Fix 1 are matrix data.
        let claude = by_id("claude-code").unwrap();
        let skill = cell(claude, ArtifactKind::Skill);

        let rule = rule_for_key(skill, "allowed_tools")
            .expect("claude Skill field-map must carry an allowed_tools rule");
        assert_eq!(
            rule.canonical,
            FieldSource::TargetOverride("allowed_tools"),
            "allowed_tools must be sourced from the targets.claude-code override block (Fix 1)",
        );
        assert_eq!(
            rule.emitted,
            Some("allowed-tools"),
            "claude must emit the hyphenated `allowed-tools` key (v1 snapshot casing)",
        );
    }

    #[test]
    fn cursor_skill_emits_always_apply_camelcase_and_has_no_name_rule() {
        // Cursor's Skill field-map: `always_apply` is override-sourced and emitted as the
        // camelCase `alwaysApply`; crucially there is NO `name` key (matches the v1 .mdc
        // snapshot, which omits `name`).
        let cursor = by_id("cursor").unwrap();
        let skill = cell(cursor, ArtifactKind::Skill);

        let rule = rule_for_key(skill, "always_apply")
            .expect("cursor Skill field-map must carry an always_apply rule");
        assert_eq!(
            rule.canonical,
            FieldSource::TargetOverride("always_apply"),
            "always_apply must be sourced from the targets.cursor override block",
        );
        assert_eq!(
            rule.emitted,
            Some("alwaysApply"),
            "cursor must emit the camelCase `alwaysApply` key",
        );
        assert!(
            !emits_key(skill, "name"),
            "cursor Skill frontmatter must NOT emit a `name` key (v1 snapshot fidelity)",
        );
    }

    #[test]
    fn claude_subagent_tools_rule_is_top_level_sourced() {
        // Subagent fields are top-level on the source artifact, so the Claude Subagent `tools`
        // rule reads `FieldSource::TopLevel("tools")` (not an override block) — the other half
        // of the FieldSource distinction.
        let claude = by_id("claude-code").unwrap();
        let subagent = cell(claude, ArtifactKind::Subagent);

        let rule = rule_for_key(subagent, "tools")
            .expect("claude Subagent field-map must carry a tools rule");
        assert_eq!(
            rule.canonical,
            FieldSource::TopLevel("tools"),
            "subagent tools is a top-level field, not a per-target override",
        );
    }

    #[test]
    fn claude_settings_cell_is_native_json_at_settings_json() {
        // Fix 6: settings is Native on claude-code (a real emitted file, not a drop-warning),
        // a JSON document at `.claude/settings.json` (no merge — one settings file).
        let claude = by_id("claude-code").unwrap();
        let settings = cell(claude, ArtifactKind::Settings);

        assert_eq!(
            settings.disposition,
            Disposition::Native,
            "claude Settings must be Native"
        );
        assert_eq!(settings.layout.path_template, ".claude/settings.json");
        assert_eq!(settings.format, SerFormat::Json);
        assert!(
            !settings.layout.merge,
            "settings is a single file, not merged"
        );
    }

    #[test]
    fn claude_mcp_server_cell_is_native_json_at_repo_root_mcp_json() {
        // The mcp_servers singleton aggregates every declared server into ONE document under the
        // `mcpServers` envelope (built in parse), so the cell emits a single non-merged JSON file
        // at the repo ROOT `.mcp.json` — the host-valid path (NOT `.claude/.mcp.json`, NOT
        // merge:true, which previously concatenated into invalid JSON).
        let claude = by_id("claude-code").unwrap();
        let mcp = cell(claude, ArtifactKind::McpServer);

        assert_eq!(
            mcp.disposition,
            Disposition::Native,
            "claude McpServer must be Native"
        );
        assert_eq!(
            mcp.layout.path_template, ".mcp.json",
            "claude .mcp.json lives at the repo root, not under .claude/",
        );
        assert!(
            !mcp.layout.merge,
            "the aggregated mcp_servers document is a single file, not merged",
        );
        assert_eq!(mcp.format, SerFormat::Json);
    }

    #[test]
    fn codex_instruction_cell_carries_the_hard_byte_budget() {
        // Fix 5: the one concrete hard-byte cell ADR-0005 requires. Codex's merged AGENTS.md
        // instruction file carries a 32_768-byte HARD budget (it truncates silently past ~32KiB).
        let codex = by_id("codex").unwrap();
        let instruction = cell(codex, ArtifactKind::Instruction);

        let budget = instruction
            .budget
            .expect("codex Instruction cell must carry a hard byte budget (Fix 5)");
        assert_eq!(
            budget.unit,
            BudgetUnit::Bytes,
            "codex instruction budget is measured in bytes"
        );
        assert_eq!(
            budget.severity,
            BudgetSeverity::Hard,
            "codex instruction budget is hard"
        );
        assert_eq!(
            budget.limit, 32_768,
            "codex AGENTS.md cap is 32 KiB (ADR-0005)"
        );
    }

    #[test]
    fn agents_md_subagent_disposition_is_drop() {
        // agents-md has no subagent primitive, so the Subagent cell drops (warning, not a file).
        let agents_md = by_id("agents-md").unwrap();
        assert_eq!(
            cell(agents_md, ArtifactKind::Subagent).disposition,
            Disposition::Drop,
            "agents-md cannot represent subagents — it must Drop",
        );
    }

    #[test]
    fn every_fold_terminates_native_within_one_hop() {
        // C-SUPPORT-DISPOSITION termination invariant: if a cell folds into another kind, the
        // target kind's cell on the SAME host must resolve to Native (no fold chains, ≤1 hop).
        // This guards against an authoring slip that makes a fold target itself fold or drop.
        for host in all() {
            for kind in ArtifactKind::all() {
                if let Disposition::Fold { into } = cell(host, *kind).disposition {
                    let target = cell(host, into);
                    assert_eq!(
                        target.disposition,
                        Disposition::Native,
                        "{}: {:?} folds into {:?}, whose cell must be Native (≤1 hop)",
                        host.id,
                        kind,
                        into,
                    );
                }
            }
        }
    }
}

/// WU-7: single-source host registry + the `init_default` data flag (RED).
///
/// These tests pin the registry surface authored in a later GREEN step: the
/// `init_default: bool` field on every [`HostCapabilities`] const (true for the two hosts the
/// CLI `init` scaffolds by default — claude-code + cursor — false otherwise), and the two
/// registry-derived helpers `known_ids()` / `init_default_ids()` that replace the hardcoded id
/// literals in `diag.rs` and `lint/targets.rs`. Until WU-7 lands they fail with missing-field
/// (`host.init_default`) / missing-fn errors — the expected RED. They must not be softened by
/// stubbing the field or the helpers.
///
/// Note: `gemini-cli` was added to the registry in WU-17, so `known_ids` now lists all six
/// hosts (the plan's WU-7 detail pins this update: `known_ids()` contains all six host ids once
/// gemini-cli lands). The `init_default` set is unaffected — gemini-cli is not an init default.
#[cfg(test)]
mod v2_registry_tests {
    use super::*;

    #[test]
    fn init_default_ids_are_claude_and_cursor_in_declared_order() {
        // The CLI `init` (WU-20) reads this instead of a hardcoded `vec!["claude-code","cursor"]`.
        // Order is the registry declaration order (`all()`), so the assertion is exact, not a set.
        assert_eq!(
            known_ids_helper_init_default(),
            ["claude-code", "cursor"],
            "init_default_ids() must be exactly [claude-code, cursor] in declared order",
        );
    }

    #[test]
    fn known_ids_contains_the_six_current_hosts() {
        // The registry-derived id list that backs the `UnknownTarget` help and lint help strings.
        // gemini-cli joined the registry in WU-17, so it must now appear here too (the plan's WU-7
        // detail pins this: "known_ids() contains all six host ids once gemini-cli lands").
        let ids = known_ids_helper_known();
        for expected in [
            "claude-code",
            "cursor",
            "agents-md",
            "opencode",
            "codex",
            "gemini-cli",
        ] {
            assert!(
                ids.contains(&expected),
                "known_ids() must contain {expected:?}; got {ids:?}",
            );
        }
        // Pin the count too: exactly the six current hosts, no accidental extras.
        assert_eq!(
            ids.len(),
            6,
            "known_ids() must list exactly the six current hosts; got {ids:?}",
        );
    }

    #[test]
    fn known_ids_matches_the_all_slice_order() {
        // known_ids() is derived from `all()`, so it must equal the ids of `all()` in order —
        // this is the C-REGISTRY single-source guarantee (one slice drives every id list).
        let from_all: Vec<&'static str> = all().iter().map(|h| h.id).collect();
        assert_eq!(
            known_ids_helper_known(),
            from_all,
            "known_ids() must mirror all()'s ids in declared order (single source)",
        );
    }

    #[test]
    fn init_default_flag_matches_expectation_per_host() {
        // The data flag on each const: claude-code + cursor are the CLI init defaults (true);
        // agents-md / opencode / codex are not scaffolded by default (false).
        let expected: [(&str, bool); 5] = [
            ("claude-code", true),
            ("cursor", true),
            ("agents-md", false),
            ("opencode", false),
            ("codex", false),
        ];
        for (id, want) in expected {
            let host = by_id(id).unwrap_or_else(|| panic!("host {id} must exist"));
            assert_eq!(
                host.init_default, want,
                "{id}.init_default must be {want} (CLI init scaffolds claude-code + cursor only)",
            );
        }
    }

    #[test]
    fn init_default_ids_is_exactly_the_hosts_flagged_init_default() {
        // The helper must be derived from the `init_default` flag, not a separate hardcoded list:
        // its membership equals { host in all() | host.init_default }.
        let derived: Vec<&'static str> = all()
            .iter()
            .filter(|h| h.init_default)
            .map(|h| h.id)
            .collect();
        assert_eq!(
            known_ids_helper_init_default(),
            derived,
            "init_default_ids() must be derived from the init_default flag over all()",
        );
    }

    // --- thin shims onto the (not-yet-existing) free functions under test ---
    //
    // Calling the production helpers through one-line wrappers keeps the RED failure localized to
    // these two lines (missing `known_ids` / `init_default_ids`) and documents the exact public
    // signature WU-7 must provide: `fn known_ids() -> Vec<&'static str>` and
    // `fn init_default_ids() -> Vec<&'static str>`.
    fn known_ids_helper_known() -> Vec<&'static str> {
        known_ids()
    }

    fn known_ids_helper_init_default() -> Vec<&'static str> {
        init_default_ids()
    }
}

/// WU-17: the sixth host `gemini-cli`, added with the most conservative disposition (RED).
///
/// RFD-0001 §C-OPEN-QUESTIONS mandates that, until gemini-cli is researched, **every**
/// capability defaults to the safest value so weaft never over-claims support: all nine kind
/// cells `Drop`, no budgets, `init_default: false`, and the `GeminiApprox` tokenizer. These
/// tests pin that conservative const authored in a later GREEN step (WU-17).
///
/// RED is **behavioral**, driven through the runtime registry: today [`by_id`] returns `None`
/// for `"gemini-cli"` and [`all`] omits it, so every `by_id("gemini-cli")` lookup below is
/// `None` and each `unwrap`/`expect` panics, while the membership/order assertions over `all()`
/// fail. They reach GREEN only once WU-17 adds the `GEMINI_CLI` const and its `by_id`/`all`
/// arms. They must not be softened by stubbing the const.
///
/// Note: the pre-existing `unknown_id_is_none` test asserts `by_id("gemini")` (no `-cli`
/// suffix) stays `None` — that bare id is never a host; only `"gemini-cli"` is added in WU-17.
#[cfg(test)]
mod wu17_gemini_cli_tests {
    use super::{Disposition, Tokenizer, all, by_id};
    use crate::kind::ArtifactKind;

    #[test]
    fn by_id_resolves_gemini_cli() {
        // WU-17 adds the gemini-cli host; `by_id("gemini-cli")` must resolve to it and the const
        // must report its own id (the lookup key and the matrix id must agree).
        let host = by_id("gemini-cli")
            .expect("WU-17: by_id(\"gemini-cli\") must resolve to the gemini-cli host const");
        assert_eq!(
            host.id, "gemini-cli",
            "the gemini-cli const must carry id == \"gemini-cli\"",
        );
    }

    #[test]
    fn gemini_cli_is_not_an_init_default() {
        // gemini-cli is not scaffolded by `weaft init` (only claude-code + cursor are). Its
        // `init_default` flag must be false (the conservative default for a new, unresearched host).
        let host = by_id("gemini-cli").expect("WU-17: gemini-cli host must exist");
        assert!(
            !host.init_default,
            "gemini-cli.init_default must be false (not scaffolded by `weaft init`)",
        );
    }

    #[test]
    fn gemini_cli_uses_the_gemini_approx_tokenizer() {
        // gemini-cli runs Gemini models, so its budget tokenizer is the GeminiApprox approximation
        // (the `Tokenizer::GeminiApprox` arm already exists for exactly this host).
        let host = by_id("gemini-cli").expect("WU-17: gemini-cli host must exist");
        assert_eq!(
            host.tokenizer,
            Tokenizer::GeminiApprox,
            "gemini-cli must count with the GeminiApprox tokenizer",
        );
    }

    #[test]
    fn every_gemini_cli_kind_cell_is_drop() {
        // The C-OPEN-QUESTIONS conservative default in full: gemini-cli cannot be claimed to
        // represent ANY kind until researched, so every one of the nine kind cells must be `Drop`
        // (no Native, no Fold). Looping over `ArtifactKind::all()` proves all nine, not just a
        // sampled few, so a future "promote one kind to Native" change is a deliberate, visible edit.
        let host = by_id("gemini-cli").expect("WU-17: gemini-cli host must exist");
        for kind in ArtifactKind::all() {
            let cell = host.kinds.get(*kind);
            assert_eq!(
                cell.disposition,
                Disposition::Drop,
                "gemini-cli {kind:?} cell must be Drop (conservative default until researched)",
            );
            assert!(
                cell.budget.is_none(),
                "gemini-cli {kind:?} cell must carry no budget (nothing is emitted, so nothing is budgeted)",
            );
        }
    }

    #[test]
    fn all_slice_includes_gemini_cli_last() {
        // The registry slice must include gemini-cli, appended AFTER the five existing hosts
        // (WU-17 adds it to the end of `all()` to keep the declared order stable and match the
        // targets slice order — ADR-0003). Pin both membership and the trailing position.
        let ids: Vec<&'static str> = all().iter().map(|h| h.id).collect();
        assert!(
            ids.contains(&"gemini-cli"),
            "all() must include the gemini-cli host once WU-17 lands; got {ids:?}",
        );
        assert_eq!(
            ids.last().copied(),
            Some("gemini-cli"),
            "gemini-cli must be the LAST host in all() (appended after the five existing hosts); got {ids:?}",
        );
    }
}
