//! Render a compiled [`Artifact`] to a host-specific string via minijinja.
//!
//! **Loader root** is per-skill-type: flat skills (`skills/name.md`) use the project root;
//! directory skills (`skills/name/SKILL.md`) use the skill's own directory so that
//! `{% include "phases/..." %}` paths resolve relative to the skill.
//!
//! **Fragment pre-registration**: at environment setup, all files under `fragments/` at
//! the project root are registered as named templates (e.g. `"fragments/footer.md.j2"`).
//! Named templates resolve before the loader, so `{% include "fragments/..." %}` works
//! from both flat and directory skills, and flat-skill output is byte-identical before
//! and after this change.
//!
//! `UndefinedBehavior::Strict` is set — a `{{ host.* }}` typo is a hard error by design.

use crate::capability::{Disposition, HostCapabilities, KindCapabilities};
use crate::diag::WeftError;
use crate::ir::{Agent, Artifact, ProjectInfo, Skill};
use crate::kind::ArtifactKind;
use crate::params::ParamValues;
use minijinja::{Environment, UndefinedBehavior, context, path_loader};
use serde::Serialize;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;
use walkdir::WalkDir;

/// The built-in `ask` macro. Skills and subagents get the import auto-prepended by the
/// compiler, so their bodies call `{{ ask("...", ...) }}` directly with no manual import.
/// Instructions still import it explicitly with `{% from "weaft/ask.j2" import ask %}`.
/// Registered as a borrowed template, which minijinja resolves *before* consulting the
/// project-root loader — so it coexists with `{% include "fragments/..." %}` without
/// shadowing project files.
const ASK_J2: &str = include_str!("templates/ask.j2");

/// The auto-prepended import that makes the [`ASK_J2`] `ask` macro callable without a manual
/// `{% from ... %}` in skill and subagent bodies. The `{%- -%}` whitespace control is load-bearing:
/// a bare `{% %}` tag leaves the trailing newline in place, leaking a leading `\n` into every
/// rendered artifact; the trimming variant renders to an empty string so output stays byte-identical
/// to rendering the body alone.
const ASK_IMPORT: &str = "{%- from \"weaft/ask.j2\" import ask -%}";

/// The loader root for a skill source. A directory skill (`skills/name/SKILL.md`) resolves its
/// relative includes against its own directory; any other source (a flat skill, or a kind that
/// is flat-file-only) resolves against `project_root`. This is the single, per-skill point of
/// skill-type detection (ADR-0007 §2).
fn skill_loader_root<'a>(source_path: &'a Path, project_root: &'a Path) -> &'a Path {
    if source_path.file_name() == Some(OsStr::new("SKILL.md")) {
        source_path.parent().unwrap_or(project_root)
    } else {
        project_root
    }
}

/// Build a minijinja environment whose loader is rooted at `loader_root` and whose named-template
/// table is pre-seeded from `project_root/fragments/`.
///
/// `loader_root` serves `{% include "phases/..." %}` (relative to a directory skill, or the
/// project root for flat sources). Every file under `project_root/fragments/` is registered as a
/// named template keyed by its project-root-relative, forward-slash path, so
/// `{% include "fragments/..." %}` resolves via the table for both flat and directory skills —
/// named templates resolve before the loader is consulted. A missing `fragments/` directory is not
/// an error; a read failure on a file that exists is (ADR-0007 §4).
fn environment(loader_root: &Path, project_root: &Path) -> Result<Environment<'static>, WeftError> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_loader(path_loader(loader_root));
    // A parse error here is a weaft bug, not user input — fail loudly at first render.
    env.add_template("weaft/ask.j2", ASK_J2)
        .expect("built-in weaft/ask.j2 macro template must compile");

    let fragments_dir = project_root.join("fragments");
    if fragments_dir.is_dir() {
        for entry in WalkDir::new(&fragments_dir).sort_by_file_name() {
            let entry = entry.map_err(|source| WeftError::Read {
                path: source
                    .path()
                    .map_or_else(|| fragments_dir.clone(), Path::to_path_buf),
                source: source.into(),
            })?;
            if !entry.file_type().is_file() {
                continue;
            }
            let file_path = entry.path();
            // Project-root-relative, forward-slash name (e.g. "fragments/footer.md.j2"). The
            // prefix always strips since file_path is under project_root/fragments.
            let name = file_path
                .strip_prefix(project_root)
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let mut body =
                std::fs::read_to_string(file_path).map_err(|source| WeftError::Read {
                    path: file_path.to_path_buf(),
                    source,
                })?;
            // minijinja strips one trailing newline from every template at lex time. A fragment is
            // prose included into a larger document, so its final newline is content — double it so
            // the strip leaves the file's own trailing newline intact.
            if body.ends_with('\n') {
                body.push('\n');
            }
            env.add_template_owned(name.clone(), body)
                .map_err(|source| WeftError::Render {
                    artifact: name,
                    source: Box::new(source),
                })?;
        }
    }

    Ok(env)
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
pub(crate) fn render_skill(
    skill: &Skill,
    project: &ProjectInfo,
    host: &HostCapabilities,
    params: &ParamValues,
    project_root: &Path,
) -> Result<String, WeftError> {
    let loader_root = skill_loader_root(&skill.source_path, project_root);
    let env = environment(loader_root, project_root)?;
    let ctx = context! {
        skill => &skill.frontmatter,
        project => project,
        host => host_view(host, ArtifactKind::Skill),
        params => params,
    };
    let body = format!("{ASK_IMPORT}\n{}", skill.body);
    render_body(&env, &skill.frontmatter.name, &body, ctx)
}

/// Render a subagent body. Templates see `agent`, `project`, `host`, and `params`. The `host`
/// context is the [`HostView`] projection scoped to the `Subagent` kind cell.
pub(crate) fn render_agent(
    agent: &Agent,
    project: &ProjectInfo,
    host: &HostCapabilities,
    params: &ParamValues,
    project_root: &Path,
) -> Result<String, WeftError> {
    let env = environment(project_root, project_root)?;
    let ctx = context! {
        agent => &agent.frontmatter,
        project => project,
        host => host_view(host, ArtifactKind::Subagent),
        params => params,
    };
    let body = format!("{ASK_IMPORT}\n{}", agent.body);
    render_body(&env, &agent.frontmatter.name, &body, ctx)
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
    let env = environment(project_root, project_root)?;
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

    const ASK_BODY: &str = "{{ ask(\"Pick one\", options=[\"a\", \"b\"]) }}";

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
        let body = "{{ ask(\"Pick one\", subagent=true) }}";
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
        let body = "{{ ask(\"Pick one\") }}";
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

    #[test]
    fn ask_macro_importless_emits_structured_tool() {
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
    fn ask_macro_importless_uses_non_blocking_phrasing() {
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
    fn ask_macro_importless_falls_back_to_prose() {
        let out = render_skill(
            &skill(ASK_BODY),
            &project(),
            &AGENTS_MD,
            &BTreeMap::new(),
            Path::new("."),
        )
        .unwrap();
        assert!(!out.contains("AskUserQuestion"));
        assert!(
            out.contains("Ask the user explicitly before any destructive action."),
            "got: {out}"
        );
    }

    #[test]
    fn ask_macro_double_import_coexists() {
        // A skill body that already carries the explicit import must render correctly even though
        // the compiler also auto-prepends it. minijinja must tolerate the duplicate.
        let body = "{% from \"weaft/ask.j2\" import ask %}\n{{ ask(\"Pick one\") }}";
        let out = render_skill(
            &skill(body),
            &project(),
            &CLAUDE_CODE,
            &BTreeMap::new(),
            Path::new("."),
        )
        .unwrap();
        assert!(out.contains("AskUserQuestion"), "got: {out}");
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

/// ADR-0007 WU-2: per-skill `loader_root` derivation + `fragments/` pre-registration (RED).
///
/// These tests target the loader-root machinery authored in the later GREEN step:
/// - `skill_loader_root(source_path, project_root)` — the single, per-skill detection point:
///   a `SKILL.md` source resolves to its own directory; anything else to `project_root` (ADR §2);
/// - directory-skill includes resolve relative to the skill dir (`{% include "phases/..." %}`),
///   while `{% include "fragments/..." %}` resolves via the project-root named-template table for
///   BOTH forms (ADR §4);
/// - a missing include is a hard render error (ADR §5, no silent fallback);
/// - a project with no `fragments/` dir still builds an environment (ADR §4).
///
/// `skill_loader_root` does not exist yet, so this module fails to compile with a missing-function
/// error — the expected RED. It must not be softened by stubbing the helper. The render tests build
/// an in-memory `Skill` whose `source_path` points into a temp dir holding the `phases/`/`fragments/`
/// files, then call the public `render_skill`.
#[cfg(test)]
mod skill_dir_loader_tests {
    use super::*;
    use crate::capability::CLAUDE_CODE;
    use crate::ir::{Skill, SkillMeta, Targets};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

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

    /// A `Skill` named `name` with the given body and `source_path`. The `source_path` is what
    /// `render_skill` keys on to derive the per-skill loader root (`file_name` == "SKILL.md").
    fn skill_at(name: &str, body: &str, source_path: PathBuf) -> Skill {
        Skill {
            frontmatter: SkillMeta {
                name: name.into(),
                description: "d".into(),
                targets: Targets::default(),
            },
            body: body.into(),
            source_path,
        }
    }

    /// A fresh, empty temp project root unique to `(tag, pid)`, removed first so a crashed re-run
    /// starts clean. The caller writes `skills/`, `fragments/`, etc. beneath it.
    fn fresh_root(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("weaft-loaderroot-{}-{}", tag, std::process::id()));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp project root");
        root
    }

    /// Write `relative` under `root`, creating parent folders as needed.
    fn write_under(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create folder");
        }
        fs::write(&path, contents).expect("write file");
    }

    #[test]
    fn flat_skill_loader_root_is_project_root() {
        // ADR §2: a flat skill source (`skills/x.md`) must resolve its loader root to the project
        // root, leaving flat-skill include behavior unchanged.
        let root = skill_loader_root(Path::new("skills/x.md"), Path::new("/root"));
        assert_eq!(
            root,
            Path::new("/root"),
            "a flat skill's loader root must be the project root",
        );
    }

    #[test]
    fn directory_skill_loader_root_is_skill_dir() {
        // ADR §2: a directory skill source (`skills/x/SKILL.md`) must resolve its loader root to
        // the skill's own directory (`skills/x`), so its relative includes resolve locally.
        let root = skill_loader_root(Path::new("skills/x/SKILL.md"), Path::new("/root"));
        assert_eq!(
            root,
            Path::new("skills/x"),
            "a directory skill's loader root must be its own SKILL.md parent directory",
        );
    }

    #[test]
    fn fragment_include_resolves_for_directory_skill() {
        // ADR §4: a directory skill rooted at `skills/greeter/` must still resolve a
        // `{% include "fragments/note.md" %}` via the project-root named-template table — even
        // though `fragments/` is not under the skill's loader root.
        let root = fresh_root("dir-fragment");
        write_under(&root, "fragments/note.md", "FRAGMENT");
        write_under(
            &root,
            "skills/greeter/SKILL.md",
            "---\nname: greeter\ndescription: d.\n---\nbody",
        );

        let skill = skill_at(
            "greeter",
            "{% include \"fragments/note.md\" %}",
            root.join("skills/greeter/SKILL.md"),
        );
        let out = render_skill(&skill, &project(), &CLAUDE_CODE, &BTreeMap::new(), &root)
            .expect("a directory skill including a project-root fragment must render");
        assert!(
            out.contains("FRAGMENT"),
            "the project-root fragment must resolve for a directory skill; got: {out:?}",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn directory_skill_relative_include_resolves_from_skill_dir() {
        // ADR §2/§4: a directory skill's relative include (`{% include "phases/intro.md" %}`) must
        // resolve from its own directory (loader root = skill dir), rendering the phase content.
        let root = fresh_root("dir-relative");
        write_under(&root, "skills/greeter/phases/intro.md", "PHASE_CONTENT");
        write_under(
            &root,
            "skills/greeter/SKILL.md",
            "---\nname: greeter\ndescription: d.\n---\nbody",
        );

        let skill = skill_at(
            "greeter",
            "{% include \"phases/intro.md\" %}",
            root.join("skills/greeter/SKILL.md"),
        );
        let out = render_skill(&skill, &project(), &CLAUDE_CODE, &BTreeMap::new(), &root)
            .expect("a directory skill's relative phase include must render");
        assert!(
            out.contains("PHASE_CONTENT"),
            "the relative phase include must resolve from the skill dir; got: {out:?}",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn flat_skill_fragment_render_is_byte_identical() {
        // Regression anchor (ADR §4, §11): a flat skill whose body is exactly the fragment include
        // must render to the fragment content verbatim. The auto-prepended `ASK_IMPORT` is a
        // `{%- -%}`-trimmed import that renders to empty, so the output equals the fragment body
        // byte-for-byte. The fragment content is static (no `{{ host.* }}`) so the expected string
        // is unambiguous. This must hold both before and after WU-2 (the named-table is built from
        // the same project-root file the loader served), pinning byte-identity of flat-skill output.
        let root = fresh_root("flat-byte-identical");
        let fragment = "shared footer line\n";
        write_under(&root, "fragments/footer.md.j2", fragment);

        let skill = skill_at(
            "safe",
            "{% include \"fragments/footer.md.j2\" %}",
            root.join("skills/safe.md"),
        );
        let out = render_skill(&skill, &project(), &CLAUDE_CODE, &BTreeMap::new(), &root)
            .expect("a flat skill including a project-root fragment must render");
        assert_eq!(
            out, fragment,
            "a flat skill's fragment render must be byte-identical to the fragment content",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn missing_include_is_hard_render_error() {
        // ADR §5: a directory skill including a non-existent `phases/nonexistent.md` (no phases/
        // dir at all) must be a hard render error under UndefinedBehavior::Strict — never silent
        // fallback or empty content.
        let root = fresh_root("missing-include");
        write_under(
            &root,
            "skills/greeter/SKILL.md",
            "---\nname: greeter\ndescription: d.\n---\nbody",
        );

        let skill = skill_at(
            "greeter",
            "{% include \"phases/nonexistent.md\" %}",
            root.join("skills/greeter/SKILL.md"),
        );
        let result = render_skill(&skill, &project(), &CLAUDE_CODE, &BTreeMap::new(), &root);
        assert!(
            matches!(result, Err(WeftError::Render { .. })),
            "a missing include must be a hard WeftError::Render, not silent fallback; got: {result:?}",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn environment_missing_fragments_dir_is_ok() {
        // ADR §4: a project with NO `fragments/` directory must still build an environment — a
        // simple body (no includes) renders without error.
        let root = fresh_root("no-fragments");
        write_under(
            &root,
            "skills/greeter/SKILL.md",
            "---\nname: greeter\ndescription: d.\n---\nbody",
        );

        let skill = skill_at(
            "greeter",
            "just plain body text",
            root.join("skills/greeter/SKILL.md"),
        );
        let out = render_skill(&skill, &project(), &CLAUDE_CODE, &BTreeMap::new(), &root)
            .expect("a project with no fragments/ dir must still render a no-include body");
        assert_eq!(out, "just plain body text");

        drop(fs::remove_dir_all(&root));
    }
}
