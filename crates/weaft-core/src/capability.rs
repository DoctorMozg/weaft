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
    /// Free-form description of how to ask the user, emitted into the body.
    pub ask_user_syntax: &'static str,

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
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentLayout {
    /// `.claude/agents/<name>.md`.
    ClaudeAgents,
    /// `.cursor/agents/<name>.md`.
    CursorAgents,
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
    max_skill_tokens: None,
    tokenizer: Tokenizer::Cl100kBase,
    skill_layout: SkillLayout::AgentsMd,
    frontmatter_dialect: FrontmatterDialect::None,
};

/// Look up a host by its stable id.
pub fn by_id(id: &str) -> Option<&'static HostCapabilities> {
    match id {
        "claude-code" => Some(&CLAUDE_CODE),
        "cursor" => Some(&CURSOR),
        "agents-md" => Some(&AGENTS_MD),
        _ => None,
    }
}

/// All known hosts, in display order.
pub fn all() -> &'static [&'static HostCapabilities] {
    &[&CLAUDE_CODE, &CURSOR, &AGENTS_MD]
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
}
