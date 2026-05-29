//! End-to-end CLI tests driving the `weaft` binary.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn weaft() -> Command {
    Command::cargo_bin("weaft").unwrap()
}

fn quickstart() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/quickstart")
}

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("weaft-it-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn targets_lists_hosts() {
    weaft()
        .arg("targets")
        .assert()
        .success()
        .stdout(predicate::str::contains("claude-code"))
        .stdout(predicate::str::contains("cursor"));
}

#[test]
fn build_writes_files() {
    let out = tmp_dir("build");
    weaft()
        .args(["build", "--manifest-path"])
        .arg(quickstart())
        .arg("--out")
        .arg(&out)
        .assert()
        .success();

    assert!(
        out.join("claude-code/skills/safe-deleter/SKILL.md")
            .is_file()
    );
    assert!(out.join("cursor/rules/safe-deleter.mdc").is_file());
    assert!(out.join("claude-code/agents/code-reviewer.md").is_file());
    assert!(out.join("agents-md/AGENTS.md").is_file());
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn lint_clean_project_succeeds() {
    weaft()
        .args(["lint", "--manifest-path"])
        .arg(quickstart())
        .assert()
        .success();
}

#[test]
fn preview_prints_without_writing() {
    weaft()
        .args(["preview", "--target", "cursor", "--manifest-path"])
        .arg(quickstart())
        .assert()
        .success()
        .stdout(predicate::str::contains("Cursor"))
        .stdout(predicate::str::contains("# === rules/safe-deleter.mdc ==="));
}

#[test]
fn tokens_reports_budget() {
    weaft()
        .args(["tokens", "--manifest-path"])
        .arg(quickstart())
        .assert()
        .success()
        .stdout(predicate::str::contains("/8000"))
        .stdout(predicate::str::contains("approximations"));
}

#[test]
fn unknown_target_errors() {
    weaft()
        .args(["build", "--target", "gemini", "--manifest-path"])
        .arg(quickstart())
        .assert()
        .failure();
}

#[test]
fn init_then_build_roundtrips() {
    let parent = tmp_dir("init");
    std::fs::create_dir_all(&parent).unwrap();

    weaft()
        .args(["init", "demo", "--path"])
        .arg(&parent)
        .assert()
        .success();

    let project = parent.join("demo");
    assert!(project.join("weaft.yaml").is_file());
    assert!(project.join("skills/hello.md").is_file());
    assert!(project.join("agents/reviewer.md").is_file());

    let out = parent.join("dist");
    weaft()
        .args(["build", "--manifest-path"])
        .arg(&project)
        .arg("--out")
        .arg(&out)
        .assert()
        .success();

    let _ = std::fs::remove_dir_all(&parent);
}
