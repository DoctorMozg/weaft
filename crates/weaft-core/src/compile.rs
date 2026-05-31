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

/// The built-in `ask` macro, importable from any project body as
/// `{% from "weaft/ask.j2" import ask %}`. Registered as a borrowed template, which
/// minijinja resolves *before* consulting the project-root loader — so it coexists with
/// `{% include "fragments/..." %}` without shadowing project files.
const ASK_J2: &str = include_str!("templates/ask.j2");

fn environment(project_root: &Path) -> Environment<'static> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_loader(path_loader(project_root));
    // A parse error here is a weaft bug, not user input — fail loudly at first render.
    env.add_template("weaft/ask.j2", ASK_J2)
        .expect("built-in weaft/ask.j2 macro template must compile");
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
    use crate::capability::{AGENTS_MD, CLAUDE_CODE, CURSOR};
    use crate::ir::{Agent, AgentMeta, SkillMeta, Targets};
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

    fn agent(body: &str) -> Agent {
        Agent {
            frontmatter: AgentMeta {
                name: "demo-agent".into(),
                description: "d".into(),
                tools: Vec::new(),
                model: None,
                readonly: None,
                is_background: None,
                targets: Targets::default(),
            },
            body: body.into(),
            source_path: "demo-agent.md".into(),
        }
    }

    const ASK_BODY: &str = "{% from \"weaft/ask.j2\" import ask %}{{ ask(host, \"Pick one\", options=[\"a\", \"b\"]) }}";

    #[test]
    fn ask_macro_emits_structured_tool_for_claude_skill() {
        let out = render_skill(
            &skill(ASK_BODY),
            &project(),
            &CLAUDE_CODE,
            &BTreeMap::new(),
            Path::new("."),
        )
        .unwrap();
        assert!(out.contains("AskUserQuestion"), "got: {out}");
        assert!(out.contains("Pick one"));
        assert!(out.contains("options: a, b"));
    }

    #[test]
    fn ask_macro_uses_non_blocking_phrasing_for_cursor() {
        let out = render_skill(
            &skill(ASK_BODY),
            &project(),
            &CURSOR,
            &BTreeMap::new(),
            Path::new("."),
        )
        .unwrap();
        assert!(!out.contains("AskUserQuestion"));
        assert!(out.contains("ask question tool"));
        assert!(out.contains("keep working"));
    }

    #[test]
    fn ask_macro_falls_back_to_prose_for_agents_md() {
        let out = render_skill(
            &skill(ASK_BODY),
            &project(),
            &AGENTS_MD,
            &BTreeMap::new(),
            Path::new("."),
        )
        .unwrap();
        assert!(!out.contains("AskUserQuestion"));
        // agents-md has no question primitive → uses the prose fallback verbatim.
        assert!(
            out.contains("Ask the user explicitly before any destructive action."),
            "got: {out}"
        );
    }

    #[test]
    fn ask_macro_degrades_in_claude_subagent_when_flagged() {
        // Claude's AskUserQuestion is not available to subagents, so the macro must NOT
        // name the tool when rendered in a subagent body flagged `subagent=true`.
        let body =
            "{% from \"weaft/ask.j2\" import ask %}{{ ask(host, \"Pick one\", subagent=true) }}";
        let out = render_agent(
            &agent(body),
            &project(),
            &CLAUDE_CODE,
            &BTreeMap::new(),
            Path::new("."),
        )
        .unwrap();
        assert!(
            !out.contains("AskUserQuestion"),
            "subagent must degrade, got: {out}"
        );
        assert!(out.contains("Pick one"));
    }

    #[test]
    fn ask_macro_autodetects_subagent_context() {
        // minijinja shares the render context with imported macros, so `agent is defined`
        // inside the macro sees the render `agent`. An agent body therefore auto-degrades
        // (no explicit `subagent=true` needed) where the host's question tool is unavailable
        // to subagents — Claude here. The skill test above confirms the inverse: a skill
        // render (no `agent` in context) names the tool.
        let body = "{% from \"weaft/ask.j2\" import ask %}{{ ask(host, \"Pick one\") }}";
        let out = render_agent(
            &agent(body),
            &project(),
            &CLAUDE_CODE,
            &BTreeMap::new(),
            Path::new("."),
        )
        .unwrap();
        assert!(
            !out.contains("AskUserQuestion"),
            "agent body should auto-degrade, got: {out}"
        );
        assert!(out.contains("Pick one"));
    }
}
