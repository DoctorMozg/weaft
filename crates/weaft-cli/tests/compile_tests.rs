//! Snapshot tests proving capability-aware compilation: one source artifact compiled to
//! two targets must differ *meaningfully* (the launch demo and the subagent demo).
//!
//! Since WU-15 these helpers drive the generic v2 pipeline directly — find the kind-tagged
//! artifact, then `resolve → render → map_fields → serialize::frame → emit_artifact` — exactly
//! as the build driver does. The `.snap` files are the byte oracle: the v2 path must reproduce
//! the v1 bytes they pin.

use std::path::PathBuf;
use weaft_core::ir::Artifact;
use weaft_core::kind::ArtifactKind;
use weaft_core::params::{self, ParamValues};
use weaft_core::pipeline::map::map_fields;
use weaft_core::pipeline::resolve::resolve;
use weaft_core::{compile, parse};
use weaft_targets::{serialize, target_by_id};

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/quickstart")
}

fn project() -> weaft_core::ir::Project {
    parse::load_project(&example_root()).expect("load quickstart project")
}

/// Resolve declared parameter defaults, as `weaft build` does.
fn defaults(project: &weaft_core::ir::Project) -> ParamValues {
    params::resolve(&project.info.parameters, &[]).unwrap()
}

/// Find the artifact of `kind` named `name` in the parsed project.
fn find_artifact<'a>(
    project: &'a weaft_core::ir::Project,
    kind: ArtifactKind,
    name: &str,
) -> &'a Artifact {
    project
        .artifacts
        .iter()
        .find(|a| a.kind == kind && a.frontmatter.name == name)
        .unwrap_or_else(|| panic!("{kind:?} artifact `{name}` must exist"))
}

/// Drive the generic pipeline for one artifact on one target and return the emitted file's text.
/// Mirrors the build driver: resolve → render → `map_fields` → `serialize::frame` →
/// `emit_artifact`.
fn compile_artifact(target_id: &str, kind: ArtifactKind, name: &str) -> String {
    let project = project();
    let target = target_by_id(target_id).expect("target exists");
    let host = target.capabilities();
    let params = defaults(&project);

    let artifact = find_artifact(&project, kind, name);
    let resolved = resolve(artifact, host);
    let body = compile::render(artifact, &project.info, host, &params, &project.root).unwrap();
    let mapped = map_fields(&resolved, artifact, target.field_transforms());
    let framed = serialize::frame(resolved.cell.format, &mapped, &body);
    let file = target.emit_artifact(&resolved, &artifact.frontmatter.name, framed);
    String::from_utf8(file.contents).unwrap()
}

/// Render + emit a skill for a target and return the primary file's text.
fn skill_output(target_id: &str, skill_name: &str) -> String {
    compile_artifact(target_id, ArtifactKind::Skill, skill_name)
}

/// Render + emit a subagent for a target and return the primary file's text.
fn agent_output(target_id: &str, agent_name: &str) -> String {
    compile_artifact(target_id, ArtifactKind::Subagent, agent_name)
}

/// Render + emit an instruction for a target and return the merged file's text.
fn instruction_output(target_id: &str, instruction_name: &str) -> String {
    compile_artifact(target_id, ArtifactKind::Instruction, instruction_name)
}

#[test]
fn safe_deleter_claude_code() {
    insta::assert_snapshot!(skill_output("claude-code", "safe-deleter"));
}

#[test]
fn safe_deleter_cursor() {
    insta::assert_snapshot!(skill_output("cursor", "safe-deleter"));
}

#[test]
fn hello_agents_md() {
    insta::assert_snapshot!(skill_output("agents-md", "hello"));
}

#[test]
fn code_reviewer_claude_code() {
    insta::assert_snapshot!(agent_output("claude-code", "code-reviewer"));
}

#[test]
fn code_reviewer_cursor() {
    insta::assert_snapshot!(agent_output("cursor", "code-reviewer"));
}

/// The two demos must actually differ — guard against a regression that collapses them.
#[test]
fn targets_produce_different_output() {
    assert_ne!(
        skill_output("claude-code", "safe-deleter"),
        skill_output("cursor", "safe-deleter"),
        "skill output should differ across targets"
    );
    assert_ne!(
        agent_output("claude-code", "code-reviewer"),
        agent_output("cursor", "code-reviewer"),
        "subagent output should differ across targets"
    );
}

// --- WU-22: new-kind and new-host snapshots ---
//
// The five snapshots above pin the v1 artifacts on the three v1 hosts (byte-preserved across the
// matrix migration). The four below extend coverage to a NEW kind (the `overview` instruction) and
// two NEW hosts (opencode, codex) so the demo proves *meaningful* per-host, per-format divergence:
// the same source compiles to plain Markdown (claude `CLAUDE.md`), YAML-frontmatter Markdown
// (opencode `SKILL.md`), and a TOML document (codex subagent).

/// New kind: the `overview` instruction renders to claude-code's `CLAUDE.md` as plain Markdown
/// (no frontmatter), with `{{ project.name }}` / `{{ host.display_name }}` interpolated.
#[test]
fn overview_instruction_claude_code() {
    insta::assert_snapshot!(instruction_output("claude-code", "overview"));
}

/// New host: the `safe-deleter` skill on opencode — YAML frontmatter at the
/// `.opencode/skills/<name>/SKILL.md` layout, distinct from the claude/cursor skill shapes.
#[test]
fn safe_deleter_opencode() {
    insta::assert_snapshot!(skill_output("opencode", "safe-deleter"));
}

/// New host + new format: the `code-reviewer` subagent on codex is a TOML document carrying the
/// rendered body under `developer_instructions` — the multi-format proof.
#[test]
fn code_reviewer_codex() {
    insta::assert_snapshot!(agent_output("codex", "code-reviewer"));
}

/// The same instruction compiled for two hosts must differ (claude `CLAUDE.md` names "Claude Code",
/// agents-md names the generic host) — the new kind also branches on the capability matrix.
#[test]
fn instruction_differs_across_hosts() {
    assert_ne!(
        instruction_output("claude-code", "overview"),
        instruction_output("agents-md", "overview"),
        "instruction output should differ across targets (host.display_name branch)"
    );
}
