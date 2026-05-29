//! Generic AGENTS.md backend.
//!
//! - Skills → a section in a single root `AGENTS.md` (no frontmatter). Sections from
//!   multiple skills are concatenated by the build layer.
//! - Subagents → unsupported; emits a warning diagnostic and no file.

use crate::{EmitOutput, EmittedFile, Target};
use weaft_core::capability::{self, HostCapabilities};
use weaft_core::diag::Diagnostic;
use weaft_core::ir::{AgentMeta, ProjectInfo, SkillMeta};

pub struct AgentsMd;

impl Target for AgentsMd {
    fn id(&self) -> &'static str {
        "agents-md"
    }

    fn capabilities(&self) -> &'static HostCapabilities {
        &capability::AGENTS_MD
    }

    fn emit_skill(
        &self,
        _project: &ProjectInfo,
        _skill: &SkillMeta,
        rendered_body: &str,
    ) -> EmitOutput {
        // The body is expected to begin with its own heading; emit verbatim as a
        // section of the combined AGENTS.md.
        let section = format!("{}\n", rendered_body.trim_end());
        EmitOutput::file(EmittedFile::section("AGENTS.md", section))
    }

    fn emit_agent(
        &self,
        _project: &ProjectInfo,
        agent: &AgentMeta,
        _rendered_body: &str,
    ) -> EmitOutput {
        EmitOutput::skipped(
            Diagnostic::warning(
                "weaft::emit::subagent_unsupported",
                format!(
                    "subagent `{}` skipped: AGENTS.md has no subagent format",
                    agent.name
                ),
            )
            .with_artifact(agent.name.clone()),
        )
    }
}
