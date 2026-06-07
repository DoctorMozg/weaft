//! Render a skill or subagent body through minijinja with the host capability context.
//!
//! The loader root is the **project root**, so `{% include "fragments/header.md.j2" %}`
//! and `{% import "fragments/macros.md.j2" as m %}` resolve relative to it. Undefined
//! variable access (e.g. a typo'd `host.subagnets`) is a hard error, not silent empty
//! output — that is what turns capability typos into actionable diagnostics.

use crate::capability::{Disposition, HostCapabilities, KindCapabilities};
use crate::diag::WeftError;
use crate::ir::{Agent, Artifact, ProjectInfo, Skill};
use crate::kind::ArtifactKind;
use crate::params::ParamValues;
use minijinja::{Environment, UndefinedBehavior, context, path_loader};
use serde::Serialize;
use std::collections::BTreeMap;
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

/// The `{{ host.* }}` render context (WU-10): host-wide facts (flattened, so
/// `host.display_name` stays addressable independently of kind), the capability cell for the
/// artifact being rendered (`host.kind.disposition`, `host.kind.format`), and a
/// `host.supports.<kind>` map answering whether the host can represent each kind.
///
/// Authored here rather than in `capability.rs`: it is a render-context projection, and the v1
/// `{{ host.* }}` contract (the flattened host fields) is preserved exactly so existing
/// templates and snapshots keep rendering byte-identically.
#[derive(Serialize)]
struct HostView<'a> {
    #[serde(flatten)]
    host: &'a HostCapabilities,
    /// The capability cell for the kind currently being rendered.
    kind: &'a KindCapabilities,
    /// `supports[k]` is true when host kind `k`'s disposition is not `Drop` — i.e. the host can
    /// represent that kind (C-TEMPLATE-CONTEXT). Keyed by the kind's canonical snake_case name.
    supports: BTreeMap<&'static str, bool>,
}

/// Build the kind-scoped `host` projection for `artifact_kind` on `host`.
fn host_view(host: &HostCapabilities, artifact_kind: ArtifactKind) -> HostView<'_> {
    let supports = ArtifactKind::all()
        .iter()
        .map(|k| {
            let representable = host.kinds.get(*k).disposition != Disposition::Drop;
            (k.serde_name(), representable)
        })
        .collect();
    HostView {
        host,
        kind: host.kinds.get(artifact_kind),
        supports,
    }
}

/// Render a skill body. Templates see `skill`, `project`, `host`, and `params`. The `host`
/// context is the [`HostView`] projection scoped to the `Skill` kind cell.
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
        host => host_view(host, ArtifactKind::Skill),
        params => params,
    };
    render_body(&env, &skill.frontmatter.name, &skill.body, ctx)
}

/// Render a subagent body. Templates see `agent`, `project`, `host`, and `params`. The `host`
/// context is the [`HostView`] projection scoped to the `Subagent` kind cell.
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
        host => host_view(host, ArtifactKind::Subagent),
        params => params,
    };
    render_body(&env, &agent.frontmatter.name, &agent.body, ctx)
}

/// Render an instruction body. Unlike skills/subagents an instruction carries no kind-specific
/// frontmatter object, so templates see only `project`, `host`, and `params`. The body is real
/// prose (parsed from `instructions/*.md`) that the matrix merges into a shared file
/// (`AGENTS.md` / `CLAUDE.md`), so it must be rendered, not discarded.
fn render_instruction(
    artifact: &Artifact,
    project: &ProjectInfo,
    host: &HostCapabilities,
    params: &ParamValues,
    project_root: &Path,
) -> Result<String, WeftError> {
    let env = environment(project_root);
    let ctx = context! {
        project => project,
        host => host_view(host, ArtifactKind::Instruction),
        params => params,
    };
    render_body(&env, &artifact.frontmatter.name, &artifact.body, ctx)
}

/// Render any artifact's body, dispatching on its kind to the kind-scoped context.
///
/// The generic pipeline driver renders every artifact regardless of kind, so this is the
/// single entry the driver calls. `Skill` and `Subagent` delegate to [`render_skill`] /
/// [`render_agent`] (reconstructing the v1 [`Skill`] / [`Agent`] view via the lossless
/// `to_skill_meta` / `to_agent_meta` round-trip) so their rendered bodies stay byte-identical
/// to v1 — the contract that keeps the snapshots valid. `Instruction` is also a file-backed
/// prose kind: its body is rendered via [`render_instruction`] and merged into a shared file
/// (`AGENTS.md` / `CLAUDE.md`). The remaining kinds are singleton config whose payload lives
/// entirely in frontmatter (an empty Jinja body), so rendering yields the empty string without
/// invoking the template engine.
pub fn render(
    artifact: &Artifact,
    project: &ProjectInfo,
    host: &HostCapabilities,
    params: &ParamValues,
    project_root: &Path,
) -> Result<String, WeftError> {
    match artifact.kind {
        ArtifactKind::Skill => {
            let skill = Skill {
                frontmatter: artifact.frontmatter.to_skill_meta(),
                body: artifact.body.clone(),
                source_path: artifact.source_path.clone(),
            };
            render_skill(&skill, project, host, params, project_root)
        },
        ArtifactKind::Subagent => {
            let agent = Agent {
                frontmatter: artifact.frontmatter.to_agent_meta(),
                body: artifact.body.clone(),
                source_path: artifact.source_path.clone(),
            };
            render_agent(&agent, project, host, params, project_root)
        },
        ArtifactKind::Instruction => {
            render_instruction(artifact, project, host, params, project_root)
        },
        ArtifactKind::Command
        | ArtifactKind::McpServer
        | ArtifactKind::Settings
        | ArtifactKind::Plugin
        | ArtifactKind::Hook
        | ArtifactKind::Ignore => Ok(String::new()),
    }
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
            settings: None,
            mcp_servers: BTreeMap::new(),
            ignore: Vec::new(),
            plugin: None,
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

/// WU-10: kind-scoped template context — host-wide facts PLUS `host.kind.*` cell facts PLUS
/// `host.supports.<kind>` booleans (RED).
///
/// These tests assert the render-context surface authored in a later GREEN step (a `HostView`
/// projection built in `compile.rs`): the host-wide `{{ host.* }}` fields stay addressable
/// (regression), the artifact's kind cell is exposed as `host.kind.disposition` /
/// `host.kind.format`, and `host.supports.<kind>` is true exactly when that kind's disposition
/// is not `Drop`. Strict-undefined remains, so `host.kind.<typo>` is a hard render error.
///
/// They drive the existing public `render_skill`/`render_agent` entry points (the plan keeps
/// these as thin wrappers over the generalized `render`), asserting observable rendered output.
/// Today the context exposes only host-wide fields, so every `host.kind.*` / `host.supports.*`
/// access is undefined under `UndefinedBehavior::Strict` and the renders error — the expected
/// RED. The host-wide and ask-macro tests in the sibling module above must keep passing.
#[cfg(test)]
mod v2_kind_scoped_context_tests {
    use super::*;
    use crate::capability::{AGENTS_MD, CLAUDE_CODE, HostCapabilities};
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
            settings: None,
            mcp_servers: BTreeMap::new(),
            ignore: Vec::new(),
            plugin: None,
        }
    }

    /// Render a skill body against a host, returning the rendered string (panics on error).
    /// Used where a successful render is expected so the assertion is on the output text.
    fn render(body: &str, host: &HostCapabilities) -> String {
        render_skill(
            &skill(body),
            &project(),
            host,
            &BTreeMap::new(),
            Path::new("."),
        )
        .expect("render must succeed")
    }

    #[test]
    fn host_wide_fact_still_resolves() {
        // Regression: host-wide facts remain addressable as `host.<field>` independently of the
        // kind sub-object (C-TEMPLATE-CONTEXT). The HostView must flatten the host fields.
        let out = render("{{ host.display_name }}", &CLAUDE_CODE);
        assert_eq!(out, "Claude Code");
    }

    #[test]
    fn kind_scoped_disposition_resolves_to_the_artifacts_cell() {
        // `host.kind.disposition` must resolve to the cell for the artifact being rendered (a
        // Skill here). Claude's Skill cell is Native, which serializes snake_case to "native"
        // (every capability enum uses `rename_all = "snake_case"`).
        let out = render("{{ host.kind.disposition }}", &CLAUDE_CODE);
        assert_eq!(
            out, "native",
            "host.kind.disposition must be the rendered Skill cell's disposition for claude",
        );
    }

    #[test]
    fn kind_scoped_format_resolves_to_the_artifacts_cell() {
        // The other kind-scoped fact: `host.kind.format` is the cell's SerFormat. Claude's Skill
        // cell is YamlFrontmatterMarkdown → snake_case "yaml_frontmatter_markdown".
        let out = render("{{ host.kind.format }}", &CLAUDE_CODE);
        assert_eq!(
            out, "yaml_frontmatter_markdown",
            "host.kind.format must be the rendered cell's serialization format",
        );
    }

    #[test]
    fn supports_query_is_true_for_a_natively_supported_kind() {
        // C-TEMPLATE-CONTEXT: a template MUST be able to query whether a host supports a kind.
        // claude-code represents subagents natively → host.supports.subagent is true.
        let out = render(
            "{% if host.supports.subagent %}YES{% else %}NO{% endif %}",
            &CLAUDE_CODE,
        );
        assert_eq!(
            out, "YES",
            "claude-code supports subagents → host.supports.subagent true"
        );
    }

    #[test]
    fn supports_query_is_false_for_a_dropped_kind() {
        // agents-md has no subagent primitive (its Subagent cell is Drop) → the support query is
        // false. This is the disposition != Drop derivation made template-visible.
        let out = render(
            "{% if host.supports.subagent %}YES{% else %}NO{% endif %}",
            &AGENTS_MD,
        );
        assert_eq!(
            out, "NO",
            "agents-md drops subagents → host.supports.subagent false"
        );
    }

    #[test]
    fn unknown_kind_scoped_field_is_a_hard_render_error_while_real_ones_resolve() {
        // Strict-undefined must apply to the kind sub-object specifically: a *real* kind-scoped
        // field (`host.kind.disposition`) renders, while a typo'd sibling (`host.kind.nonexistent`)
        // is a hard error — not silent empty output. Pinning both in one test makes it RED today
        // (the real field does not resolve before WU-10) and meaningful after (the typo still
        // errors), rather than passing trivially because `host.kind` is wholly undefined now.
        let ok = render_skill(
            &skill("{{ host.kind.disposition }}"),
            &project(),
            &CLAUDE_CODE,
            &BTreeMap::new(),
            Path::new("."),
        );
        assert!(
            ok.is_ok(),
            "a real kind-scoped field (host.kind.disposition) must resolve after WU-10",
        );

        let typo = render_skill(
            &skill("{{ host.kind.nonexistent }}"),
            &project(),
            &CLAUDE_CODE,
            &BTreeMap::new(),
            Path::new("."),
        );
        assert!(
            typo.is_err(),
            "host.kind.nonexistent must be a hard render error under UndefinedBehavior::Strict",
        );
    }
}
