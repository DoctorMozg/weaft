//! Lint passes. Each pass is a free function returning `Vec<Diagnostic>`; there is no
//! trait or plugin system in v0.1 (YAGNI). [`run`] aggregates them all.

pub mod ask_user;
pub mod budget;
pub mod layout;
pub mod required;
pub mod targets;
pub mod unused;

use crate::capability::{self, HostCapabilities};
use crate::diag::Diagnostic;
use crate::ir::{Project, Targets};

/// Run every lint pass over a project. `strict` upgrades budget overflows to errors.
pub fn run(project: &Project, strict: bool) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    out.extend(required::check(project));
    out.extend(targets::check(project));
    out.extend(layout::check(project, strict));
    out.extend(ask_user::check(project));
    out.extend(unused::check(project));
    out.extend(budget::check(project, strict));
    out
}

/// The hosts a project targets: its declared `supported` list, or all known hosts if
/// the list is empty. Unknown ids are skipped here (reported by [`targets::check`]).
pub fn project_hosts(project: &Project) -> Vec<&'static HostCapabilities> {
    if project.info.targets.supported.is_empty() {
        capability::all().to_vec()
    } else {
        project
            .info
            .targets
            .supported
            .iter()
            .filter_map(|id| capability::by_id(id))
            .collect()
    }
}

/// Effective hosts for one artifact: the project hosts intersected with the artifact's
/// own `targets.supported`.
pub fn artifact_hosts<'a>(
    project: &Project,
    artifact_targets: &Targets,
) -> Vec<&'a HostCapabilities> {
    project_hosts(project)
        .into_iter()
        .filter(|h| artifact_targets.supports(h.id))
        .collect()
}

/// ADR-0007 WU-4: the `layout` passes are wired into `run` (RED).
///
/// `run` must aggregate `layout::check` so the new `duplicate_skill` / `missing_skill_md`
/// diagnostics surface in `weaft lint` and in the build's lint step. This test builds a
/// coexistence project (`skills/dup.md` + `skills/dup/SKILL.md`) on disk and asserts that
/// `lint::run(&project, false)` output includes a `duplicate_skill` diagnostic — proving the
/// registration, not just that `layout::check` works in isolation.
///
/// Today `run` does not call `layout::check` (and `layout::check` is a `todo!()` stub once it is),
/// so this is RED: either `duplicate_skill` is absent from the aggregate, or `run` panics through
/// the stub. It reaches GREEN only when WU-3 implements the passes AND WU-4 registers them.
#[cfg(test)]
mod layout_registration_tests {
    use super::run;
    use crate::ir::{Meta, Project, ProjectInfo, Targets};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn fresh_root(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("weaft-layoutrun-{}-{}", tag, std::process::id()));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp project root");
        root
    }

    fn write_under(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create folder");
        }
        fs::write(&path, contents).expect("write file");
    }

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

    #[test]
    fn layout_check_is_registered_in_run() {
        // A coexistence project must surface a `duplicate_skill` diagnostic through the aggregate
        // `lint::run` — proving `layout::check` is wired in, not merely correct standalone.
        let root = fresh_root("registered");
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

        let diags = run(&project, false);
        assert!(
            diags
                .iter()
                .any(|d| d.code == "weaft::lint::duplicate_skill"),
            "lint::run must surface a duplicate_skill diagnostic for a coexistence project \
             (layout pass registered); got: {diags:?}",
        );

        drop(fs::remove_dir_all(&root));
    }
}
