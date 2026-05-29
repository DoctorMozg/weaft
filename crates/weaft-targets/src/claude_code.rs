//! Claude Code backend.
//!
//! - Skills → `skills/<name>/SKILL.md` with YAML frontmatter (`name`, `description`,
//!   optional `allowed-tools`, optional `model`). Note the emitted field is
//!   **`allowed-tools`** (hyphen), even though the override key is `allowed_tools`.
//! - Subagents → `agents/<name>.md` with frontmatter (`name`, `description`,
//!   optional `tools`, optional `model`).
//! - Assets are copied by the build layer (this host supports them).

use crate::yaml::{self, frontmatter, s, seq};
use crate::{EmitOutput, EmittedFile, Target};
use weaft_core::capability::{self, HostCapabilities};
use weaft_core::ir::{AgentMeta, ProjectInfo, SkillMeta};

pub struct ClaudeCode;

impl Target for ClaudeCode {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn capabilities(&self) -> &'static HostCapabilities {
        &capability::CLAUDE_CODE
    }

    fn emit_skill(
        &self,
        _project: &ProjectInfo,
        skill: &SkillMeta,
        rendered_body: &str,
    ) -> EmitOutput {
        let ov = skill.targets.override_for(self.id());
        let tools = yaml::get_str_seq(ov, "allowed_tools");
        let model = yaml::get_str(ov, "model").or_else(|| yaml::get_str(ov, "model_preference"));

        let fm = frontmatter(vec![
            ("name", Some(s(&skill.name))),
            ("description", Some(s(&skill.description))),
            ("allowed-tools", tools.as_deref().map(seq)),
            ("model", model.map(s)),
        ]);

        let path = format!("skills/{}/SKILL.md", skill.name);
        EmitOutput::file(EmittedFile::text(path, format!("{fm}\n{rendered_body}")))
    }

    fn emit_agent(
        &self,
        _project: &ProjectInfo,
        agent: &AgentMeta,
        rendered_body: &str,
    ) -> EmitOutput {
        let tools = (!agent.tools.is_empty()).then(|| seq(&agent.tools));
        let fm = frontmatter(vec![
            ("name", Some(s(&agent.name))),
            ("description", Some(s(&agent.description))),
            ("tools", tools),
            ("model", agent.model.clone().map(s)),
        ]);

        let path = format!("agents/{}.md", agent.name);
        EmitOutput::file(EmittedFile::text(path, format!("{fm}\n{rendered_body}")))
    }
}
