//! Render a skill or subagent body through minijinja with the host capability context.
//!
//! The loader root is the **project root**, so `{% include "fragments/header.md.j2" %}`
//! and `{% import "fragments/macros.md.j2" as m %}` resolve relative to it. Undefined
//! variable access (e.g. a typo'd `host.subagnets`) is a hard error, not silent empty
//! output — that is what turns capability typos into actionable diagnostics.

use crate::capability::HostCapabilities;
use crate::diag::WeftError;
use crate::ir::{Agent, ProjectInfo, Skill};
use crate::params::ParamValues;
use minijinja::{Environment, UndefinedBehavior, context, path_loader};
use std::path::Path;

fn environment(project_root: &Path) -> Environment<'static> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_loader(path_loader(project_root));
    env
}

fn render_body(
    env: &Environment<'static>,
    label: &str,
    body: &str,
    ctx: minijinja::Value,
) -> Result<String, WeftError> {
    let tmpl = env
        .template_from_str(body)
        .map_err(|source| WeftError::Render {
            artifact: label.to_string(),
            source: Box::new(source),
        })?;
    tmpl.render(ctx).map_err(|source| WeftError::Render {
        artifact: label.to_string(),
        source: Box::new(source),
    })
}

/// Render a skill body. Templates see `skill`, `project`, `host`, and `params`.
pub fn render_skill(
    skill: &Skill,
    project: &ProjectInfo,
    host: &HostCapabilities,
    params: &ParamValues,
    project_root: &Path,
) -> Result<String, WeftError> {
    let env = environment(project_root);
    let ctx = context! {
        skill => &skill.frontmatter,
        project => project,
        host => host,
        params => params,
    };
    render_body(&env, &skill.frontmatter.name, &skill.body, ctx)
}

/// Render a subagent body. Templates see `agent`, `project`, `host`, and `params`.
pub fn render_agent(
    agent: &Agent,
    project: &ProjectInfo,
    host: &HostCapabilities,
    params: &ParamValues,
    project_root: &Path,
) -> Result<String, WeftError> {
    let env = environment(project_root);
    let ctx = context! {
        agent => &agent.frontmatter,
        project => project,
        host => host,
        params => params,
    };
    render_body(&env, &agent.frontmatter.name, &agent.body, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CLAUDE_CODE, CURSOR};
    use crate::ir::{SkillMeta, Targets};
    use std::collections::BTreeMap;

    fn skill(body: &str) -> Skill {
        Skill {
            frontmatter: SkillMeta {
                name: "demo".into(),
                description: "d".into(),
                targets: Targets::default(),
            },
            body: body.into(),
            source_path: "demo.md".into(),
        }
    }

    fn project() -> ProjectInfo {
        ProjectInfo {
            name: "proj".into(),
            version: "0.1.0".into(),
            description: String::new(),
            meta: Default::default(),
            targets: Targets::default(),
            parameters: BTreeMap::new(),
        }
    }

    #[test]
    fn renders_skill_name_and_host() {
        let s = skill("# {{ skill.name }} on {{ host.display_name }}");
        let out = render_skill(
            &s,
            &project(),
            &CLAUDE_CODE,
            &BTreeMap::new(),
            Path::new("."),
        )
        .unwrap();
        assert_eq!(out, "# demo on Claude Code");
    }

    #[test]
    fn host_branch_differs_per_target() {
        let body = "{% if host.permission_model == \"explicit\" %}ASK{% else %}GATE{% endif %}";
        let claude = render_skill(
            &skill(body),
            &project(),
            &CLAUDE_CODE,
            &BTreeMap::new(),
            Path::new("."),
        )
        .unwrap();
        let cursor = render_skill(
            &skill(body),
            &project(),
            &CURSOR,
            &BTreeMap::new(),
            Path::new("."),
        )
        .unwrap();
        assert_eq!(claude, "ASK");
        assert_eq!(cursor, "GATE");
    }

    #[test]
    fn undefined_capability_is_error() {
        let s = skill("{{ host.nonexistent_field }}");
        assert!(
            render_skill(
                &s,
                &project(),
                &CLAUDE_CODE,
                &BTreeMap::new(),
                Path::new(".")
            )
            .is_err()
        );
    }
}
