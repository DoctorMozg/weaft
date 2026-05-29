//! Required-field lint: project and every skill/agent must carry non-empty identifying
//! metadata. Hard errors.

use crate::diag::Diagnostic;
use crate::ir::Project;

const CODE: &str = "weaft::lint::required";

pub fn check(project: &Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    if project.info.name.trim().is_empty() {
        out.push(Diagnostic::error(
            CODE,
            "project `name` is empty in weaft.yaml",
        ));
    }
    if project.info.version.trim().is_empty() {
        out.push(Diagnostic::error(
            CODE,
            "project `version` is empty in weaft.yaml",
        ));
    }

    for skill in &project.skills {
        if skill.frontmatter.name.trim().is_empty() {
            out.push(
                Diagnostic::error(CODE, "skill `name` is empty")
                    .with_artifact(skill.source_path.display().to_string()),
            );
        }
        if skill.frontmatter.description.trim().is_empty() {
            out.push(
                Diagnostic::error(CODE, "skill `description` is empty")
                    .with_artifact(skill.frontmatter.name.clone())
                    .with_help("a description is required so hosts can route to the skill"),
            );
        }
    }

    for agent in &project.agents {
        if agent.frontmatter.name.trim().is_empty() {
            out.push(
                Diagnostic::error(CODE, "subagent `name` is empty")
                    .with_artifact(agent.source_path.display().to_string()),
            );
        }
        if agent.frontmatter.description.trim().is_empty() {
            out.push(
                Diagnostic::error(CODE, "subagent `description` is empty")
                    .with_artifact(agent.frontmatter.name.clone())
                    .with_help("the description is the routing signal hosts use to delegate"),
            );
        }
    }

    out
}
