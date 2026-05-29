//! Snapshot tests proving capability-aware compilation: one source artifact compiled to
//! two targets must differ *meaningfully* (the launch demo and the subagent demo).

use std::path::PathBuf;
use weaft_core::params::{self, ParamValues};
use weaft_core::{compile, parse};
use weaft_targets::target_by_id;

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

/// Render + emit a skill for a target and return the primary file's text.
fn skill_output(target_id: &str, skill_name: &str) -> String {
    let project = project();
    let target = target_by_id(target_id).unwrap();
    let host = target.capabilities();
    let params = defaults(&project);
    let skill = project
        .skills
        .iter()
        .find(|s| s.frontmatter.name == skill_name)
        .expect("skill exists");
    let body = compile::render_skill(skill, &project.info, host, &params, &project.root).unwrap();
    let out = target.emit_skill(&project.info, &skill.frontmatter, &body);
    String::from_utf8(out.files[0].contents.clone()).unwrap()
}

/// Render + emit a subagent for a target and return the primary file's text.
fn agent_output(target_id: &str, agent_name: &str) -> String {
    let project = project();
    let target = target_by_id(target_id).unwrap();
    let host = target.capabilities();
    let params = defaults(&project);
    let agent = project
        .agents
        .iter()
        .find(|a| a.frontmatter.name == agent_name)
        .expect("agent exists");
    let body = compile::render_agent(agent, &project.info, host, &params, &project.root).unwrap();
    let out = target.emit_agent(&project.info, &agent.frontmatter, &body);
    String::from_utf8(out.files[0].contents.clone()).unwrap()
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
