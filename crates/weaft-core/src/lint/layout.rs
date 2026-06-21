//! Skill-layout lint passes (ADR-0007): on-disk checks over the `skills/` directory.
//!
//! Two passes re-derive the raw on-disk facts from `project.root.join("skills")` (not from
//! `project.artifacts`, which the parser has already de-duplicated):
//! - `duplicate_skill` (ADR §6): both `skills/<name>.md` and `skills/<name>/SKILL.md` exist —
//!   the directory form wins, the flat file is shadowed. Warn-by-default, hard error under
//!   `--strict`.
//! - `missing_skill_md` (ADR §7): a `skills/<name>/` directory holds `.md` files but no
//!   `SKILL.md` — its content is ignored. Always a warning (never escalated under `--strict`).

use crate::diag::Diagnostic;
use crate::ir::Project;
use std::collections::BTreeMap;
use std::path::Path;

const DUPLICATE_SKILL: &str = "weaft::lint::duplicate_skill";
const MISSING_SKILL_MD: &str = "weaft::lint::missing_skill_md";

pub fn check(project: &Project, strict: bool) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let skills_dir = project.root.join("skills");
    if !skills_dir.is_dir() {
        return out;
    }

    // Re-derive the raw on-disk facts: the parser (WU-1) has already dropped shadowed flat files
    // from `project.artifacts`, so coexistence is only visible by reading `skills/` directly.
    let (flat_stems, subdirs) = read_skills_dir(&skills_dir);

    // duplicate_skill (ADR §6): both `skills/<name>.md` and `skills/<name>/SKILL.md` exist. The
    // directory form wins at parse time; surface the ambiguity here. Warn-by-default, hard error
    // under `--strict`.
    for (stem, flat_path) in &flat_stems {
        if subdirs.contains_key(stem.as_str()) {
            let message = format!(
                "skill `{stem}` exists as both `{flat_path}` and `{stem}/SKILL.md`; \
                 the directory form wins"
            );
            let diag = if strict {
                Diagnostic::error(DUPLICATE_SKILL, message)
            } else {
                Diagnostic::warning(DUPLICATE_SKILL, message)
            };
            out.push(
                diag.with_artifact(stem.clone())
                    .with_help("remove one of the two forms to avoid ambiguity"),
            );
        }
    }

    // missing_skill_md (ADR §7): a `skills/<name>/` subdir holding `.md` files but no `SKILL.md`
    // is silently ignored by the parser. Always a warning (never escalated under `--strict`); a
    // pure-asset subdir with no `.md` files at all stays silent.
    for (dir_name, has_skill_md) in &subdirs {
        if !has_skill_md && has_md_files(&skills_dir.join(dir_name)) {
            out.push(
                Diagnostic::warning(
                    MISSING_SKILL_MD,
                    format!("directory `skills/{dir_name}/` has no SKILL.md; ignored"),
                )
                .with_artifact(dir_name.clone())
                .with_help(
                    "rename the file to `SKILL.md` (uppercase) for it to be collected as a skill",
                ),
            );
        }
    }

    out
}

/// Read `skills_dir` once and split it into the two views the passes need:
/// - `flat_stems`: every `*.md` directly inside `skills_dir` (excluding a stray top-level
///   `SKILL.md`), mapping its file stem to a display path for the diagnostic message;
/// - `subdirs`: every immediate subdirectory name, mapping to whether it contains a `SKILL.md`.
///
/// `BTreeMap` keeps both views in sorted order so the emitted diagnostics are deterministic
/// (C-DETERMINISM). Unreadable entries are silently skipped — a lint must not abort on I/O noise.
fn read_skills_dir(skills_dir: &Path) -> (BTreeMap<String, String>, BTreeMap<String, bool>) {
    let mut flat_stems = BTreeMap::new();
    let mut subdirs = BTreeMap::new();

    let Ok(entries) = std::fs::read_dir(skills_dir) else {
        return (flat_stems, subdirs);
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if let Some(dir_name) = path.file_name().and_then(|s| s.to_str()) {
                let has_skill_md = path.join("SKILL.md").is_file();
                subdirs.insert(dir_name.to_string(), has_skill_md);
            }
        } else if is_markdown(&path) && path.file_name() != Some(std::ffi::OsStr::new("SKILL.md")) {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                flat_stems.insert(stem.to_string(), path.display().to_string());
            }
        }
    }

    (flat_stems, subdirs)
}

/// Does `dir` contain at least one `*.md` file directly inside it? Returns `false` on an
/// unreadable directory — the pass treats I/O failure as "no markdown to warn about".
fn has_md_files(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .any(|e| is_markdown(&e.path()))
}

/// A `*.md` path (case-sensitive extension, matching the parser's `markdown_paths`).
fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("md")
}

/// ADR-0007 WU-3: `duplicate_skill` + `missing_skill_md` on-disk layout passes (RED).
///
/// These tests build a `skills/` tree in a temp dir, wrap it in a minimal `Project` whose `root`
/// points at it, and call `check(&project, strict)` directly. The contract (ADR §6, §7):
/// - both `skills/dup.md` and `skills/dup/SKILL.md` present → one `duplicate_skill` diagnostic,
///   a warning when `!strict` and an error when `strict`;
/// - a `skills/x/` with `.md` files but no `SKILL.md` → one `missing_skill_md` warning that stays
///   a warning even under `--strict`;
/// - a well-formed `skills/x/SKILL.md` → zero layout diagnostics;
/// - an empty `skills/` or no `skills/` at all → zero diagnostics, no panic.
///
/// `check` is a `todo!()` stub today, so every test panics when it calls `check` — the expected
/// RED. It must not be softened by implementing the passes here.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Severity;
    use crate::ir::{Meta, ProjectInfo, Targets};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A fresh, empty temp project root unique to `(tag, pid)`, removed first so a crashed re-run
    /// starts clean. The caller writes a `skills/` tree beneath it; some tests deliberately do NOT
    /// create `skills/` (the missing-dir case).
    fn fresh_root(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("weaft-layoutlint-{}-{}", tag, std::process::id()));
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

    /// A minimal `Project` rooted at `root` with no in-memory artifacts — the layout passes read
    /// the filesystem under `root.join("skills")`, not `project.artifacts`.
    fn project_at(root: PathBuf) -> Project {
        Project {
            info: ProjectInfo {
                name: "p".into(),
                version: "0.1.0".into(),
                description: String::new(),
                meta: Meta::default(),
                targets: Targets::default(),
                parameters: BTreeMap::new(),
                settings: None,
                mcp_servers: BTreeMap::new(),
                ignore: Vec::new(),
                plugin: None,
            },
            artifacts: Vec::new(),
            root,
        }
    }

    /// The diagnostics carrying `code`.
    fn with_code<'a>(diags: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
        diags.iter().filter(|d| d.code == code).collect()
    }

    #[test]
    fn duplicate_skill_warns_without_strict() {
        // ADR §6: both forms present, `strict=false` → exactly one duplicate_skill WARNING.
        let root = fresh_root("dup-warn");
        write_under(
            &root,
            "skills/dup.md",
            "---\nname: dup\ndescription: flat.\n---\nflat.\n",
        );
        write_under(
            &root,
            "skills/dup/SKILL.md",
            "---\nname: dup\ndescription: dir.\n---\ndir.\n",
        );
        let project = project_at(root.clone());

        let diags = check(&project, false);
        let dup = with_code(&diags, DUPLICATE_SKILL);
        assert_eq!(
            dup.len(),
            1,
            "exactly one duplicate_skill diagnostic for a coexistence; got: {diags:?}",
        );
        assert_eq!(
            dup[0].severity,
            Severity::Warning,
            "duplicate_skill without --strict must be a warning",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn duplicate_skill_errors_with_strict() {
        // ADR §6: the same coexistence under `strict=true` → an error-severity duplicate_skill.
        let root = fresh_root("dup-error");
        write_under(
            &root,
            "skills/dup.md",
            "---\nname: dup\ndescription: flat.\n---\nflat.\n",
        );
        write_under(
            &root,
            "skills/dup/SKILL.md",
            "---\nname: dup\ndescription: dir.\n---\ndir.\n",
        );
        let project = project_at(root.clone());

        let diags = check(&project, true);
        let dup = with_code(&diags, DUPLICATE_SKILL);
        assert_eq!(
            dup.len(),
            1,
            "exactly one duplicate_skill diagnostic under --strict; got: {diags:?}",
        );
        assert!(
            dup[0].is_error(),
            "duplicate_skill under --strict must be a hard error",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn missing_skill_md_warns() {
        // ADR §7: a `skills/x/` with `.md` files but no SKILL.md → one missing_skill_md warning.
        let root = fresh_root("missing-warn");
        write_under(&root, "skills/x/notes.md", "loose notes, not a SKILL.md\n");
        let project = project_at(root.clone());

        let diags = check(&project, false);
        let missing = with_code(&diags, MISSING_SKILL_MD);
        assert_eq!(
            missing.len(),
            1,
            "exactly one missing_skill_md diagnostic for a dir with .md but no SKILL.md; got: {diags:?}",
        );
        assert_eq!(
            missing[0].severity,
            Severity::Warning,
            "missing_skill_md must be a warning",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn missing_skill_md_stays_warning_under_strict() {
        // ADR §7: missing_skill_md is an authoring foot-gun, NOT a coexistence conflict, so it is
        // never escalated — it stays a warning even under `--strict`.
        let root = fresh_root("missing-strict");
        write_under(&root, "skills/x/notes.md", "loose notes, not a SKILL.md\n");
        let project = project_at(root.clone());

        let diags = check(&project, true);
        let missing = with_code(&diags, MISSING_SKILL_MD);
        assert_eq!(
            missing.len(),
            1,
            "exactly one missing_skill_md diagnostic under --strict; got: {diags:?}",
        );
        assert_eq!(
            missing[0].severity,
            Severity::Warning,
            "missing_skill_md must remain a warning under --strict (never escalated)",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn directory_with_skill_md_produces_no_layout_diagnostic() {
        // ADR §7: a well-formed `skills/x/SKILL.md` raises neither layout diagnostic.
        let root = fresh_root("well-formed");
        write_under(
            &root,
            "skills/x/SKILL.md",
            "---\nname: x\ndescription: well-formed.\n---\nbody.\n",
        );
        let project = project_at(root.clone());

        let diags = check(&project, false);
        assert!(
            diags.is_empty(),
            "a well-formed directory skill must produce no layout diagnostics; got: {diags:?}",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn empty_skills_dir_is_silent() {
        // An empty `skills/` directory → zero layout diagnostics, no panic.
        let root = fresh_root("empty-skills");
        fs::create_dir_all(root.join("skills")).expect("create empty skills dir");
        let project = project_at(root.clone());

        let diags = check(&project, false);
        assert!(
            diags.is_empty(),
            "an empty skills/ dir must produce no layout diagnostics; got: {diags:?}",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn missing_skills_dir_is_silent() {
        // No `skills/` directory at all → zero diagnostics, no panic (absence of skills is legal).
        let root = fresh_root("no-skills");
        let project = project_at(root.clone());

        let diags = check(&project, false);
        assert!(
            diags.is_empty(),
            "a project with no skills/ dir must produce no layout diagnostics; got: {diags:?}",
        );

        drop(fs::remove_dir_all(&root));
    }
}
