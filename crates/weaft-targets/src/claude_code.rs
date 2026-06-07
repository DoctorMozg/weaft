//! Claude Code backend.
//!
//! A thin shell since the v2 migration: the host's layout, frontmatter keys, and
//! disposition are all matrix data (`weaft_core::capability::CLAUDE_CODE`), so the generic
//! [`crate::Target::emit_artifact`] default produces the files. For reference, the matrix
//! declares: skills → `skills/<name>/SKILL.md` with `name`/`description`/`allowed-tools`
//! (hyphen)/`model` frontmatter; subagents → `agents/<name>.md` with
//! `name`/`description`/`tools`/`model`. Assets are copied by the build layer.

use crate::Target;
use weaft_core::capability::{self, HostCapabilities};

pub struct ClaudeCode;

impl Target for ClaudeCode {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn capabilities(&self) -> &'static HostCapabilities {
        &capability::CLAUDE_CODE
    }
}
