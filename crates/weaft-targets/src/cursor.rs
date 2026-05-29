//! Cursor backend.
//!
//! - Skills → `rules/<name>.mdc` with YAML frontmatter (`description`, optional
//!   `globs`, `alwaysApply`). Note `alwaysApply` is **camelCase**.
//! - Subagents → `agents/<name>.md` with frontmatter (`name`, `description`, optional
//!   `model`, `readonly`, `is_background`). Cursor subagents use `readonly` rather than
//!   a tool allowlist.
//! - Assets are not supported; the build/lint layer warns when assets are present.

use crate::yaml::{self, frontmatter, s, seq};
use crate::{EmitOutput, EmittedFile, Target};
use weaft_core::capability::{self, HostCapabilities};
use weaft_core::ir::{AgentMeta, ProjectInfo, SkillMeta};

pub struct Cursor;

impl Target for Cursor {
    fn id(&self) -> &'static str {
        "cursor"
    }

    fn capabilities(&self) -> &'static HostCapabilities {
        &capability::CURSOR
    }

    fn emit_skill(
        &self,
        _project: &ProjectInfo,
        skill: &SkillMeta,
        rendered_body: &str,
    ) -> EmitOutput {
        let ov = skill.targets.override_for(self.id());
        let globs = yaml::get_str_seq(ov, "globs");
        let always_apply = yaml::get_bool(ov, "always_apply");

        let fm = frontmatter(vec![
            ("description", Some(s(&skill.description))),
            ("globs", globs.as_deref().map(seq)),
            ("alwaysApply", always_apply.map(serde_yaml::Value::Bool)),
        ]);

        let path = format!("rules/{}.mdc", skill.name);
        EmitOutput::file(EmittedFile::text(path, format!("{fm}\n{rendered_body}")))
    }

    fn emit_agent(
        &self,
        _project: &ProjectInfo,
        agent: &AgentMeta,
        rendered_body: &str,
    ) -> EmitOutput {
        let fm = frontmatter(vec![
            ("name", Some(s(&agent.name))),
            ("description", Some(s(&agent.description))),
            ("model", agent.model.clone().map(s)),
            ("readonly", agent.readonly.map(serde_yaml::Value::Bool)),
            (
                "is_background",
                agent.is_background.map(serde_yaml::Value::Bool),
            ),
        ]);

        let path = format!("agents/{}.md", agent.name);
        EmitOutput::file(EmittedFile::text(path, format!("{fm}\n{rendered_body}")))
    }
}
