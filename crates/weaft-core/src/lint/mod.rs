//! Lint passes. Each pass is a free function returning `Vec<Diagnostic>`; there is no
//! trait or plugin system in v0.1 (YAGNI). [`run`] aggregates them all.

pub mod budget;
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
