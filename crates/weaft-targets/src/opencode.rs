//! opencode backend.
//!
//! A thin shell like the other v2 backends: the host's layout, frontmatter keys, and
//! disposition are all matrix data (`weaft_core::capability::OPENCODE`), so the generic
//! [`crate::Target::emit_artifact`] default produces the files. For reference, the matrix
//! declares: skills → `.opencode/skills/<name>/SKILL.md` with the Claude YAML-frontmatter
//! field shape; subagents → `.opencode/agent/<name>.md` (note the singular `agent/`
//! segment, distinct from the `agents/` of claude/cursor) with
//! `name`/`description`/`tools`/`model`. opencode needs no value transforms, so it keeps the
//! default no-op [`crate::Target::field_transforms`].

use crate::Target;
use weaft_core::capability::{self, HostCapabilities};

pub struct Opencode;

impl Target for Opencode {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn capabilities(&self) -> &'static HostCapabilities {
        &capability::OPENCODE
    }
}
