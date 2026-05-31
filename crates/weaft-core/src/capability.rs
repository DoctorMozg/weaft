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

pub const CLAUDE_CODE: HostCapabilities = HostCapabilities {
    id: "claude-code",
    display_name: "Claude Code",
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
};

pub const CURSOR: HostCapabilities = HostCapabilities {
    id: "cursor",
    display_name: "Cursor",
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
};

pub const AGENTS_MD: HostCapabilities = HostCapabilities {
    id: "agents-md",
    display_name: "AGENTS.md (generic)",
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
};

pub const OPENCODE: HostCapabilities = HostCapabilities {
    id: "opencode",
    display_name: "opencode",
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
};

pub const CODEX: HostCapabilities = HostCapabilities {
    id: "codex",
    display_name: "OpenAI Codex",
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
};

/// Look up a host by its stable id.
pub fn by_id(id: &str) -> Option<&'static HostCapabilities> {
    match id {
        "claude-code" => Some(&CLAUDE_CODE),
        "cursor" => Some(&CURSOR),
        "agents-md" => Some(&AGENTS_MD),
        "opencode" => Some(&OPENCODE),
        "codex" => Some(&CODEX),
        _ => None,
    }
}

/// All known hosts, in display order.
pub fn all() -> &'static [&'static HostCapabilities] {
    &[&CLAUDE_CODE, &CURSOR, &AGENTS_MD, &OPENCODE, &CODEX]
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
