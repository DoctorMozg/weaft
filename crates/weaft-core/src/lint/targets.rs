//! Target-validity lints: unknown target ids, assets on hosts that lack asset support,
//! and subagents declared for hosts without a subagent concept.

use crate::capability;
use crate::diag::Diagnostic;
use crate::fs;
use crate::ir::{Project, Targets};

const UNKNOWN: &str = "weaft::lint::invalid_target_id";
const ASSET: &str = "weaft::lint::asset_unsupported_by_target";
const SUBAGENT: &str = "weaft::lint::subagent_unsupported_by_target";

pub fn check(project: &Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // Unknown target ids — hard errors — across project and every artifact.
    check_ids(&project.info.targets, "weaft.yaml", &mut out);
    for skill in &project.skills {
        check_ids(
            &skill.frontmatter.targets,
            &skill.frontmatter.name,
            &mut out,
        );
    }
    for agent in &project.agents {
        check_ids(
            &agent.frontmatter.targets,
            &agent.frontmatter.name,
            &mut out,
        );
    }

    let hosts = super::project_hosts(project);

    // Assets present but a supported host can't carry them — warning.
    let has_assets = fs::assets_dir(&project.root).is_some();
    if has_assets {
        for host in &hosts {
            if !host.supports_assets {
                out.push(
                    Diagnostic::warning(
                        ASSET,
                        format!(
                            "assets/ present but target `{}` does not support assets; they will be skipped",
                            host.id
                        ),
                    )
                    .with_artifact(host.id.to_string()),
                );
            }
        }
    }

    // Subagents declared for a host with no subagent concept — warning.
    for agent in &project.agents {
        for host in super::artifact_hosts(project, &agent.frontmatter.targets) {
            if !host.supports_subagents {
                out.push(
                    Diagnostic::warning(
                        SUBAGENT,
                        format!(
                            "subagent `{}` targets `{}`, which has no subagent format; it will be skipped",
                            agent.frontmatter.name, host.id
                        ),
                    )
                    .with_artifact(agent.frontmatter.name.clone()),
                );
            }
        }
    }

    out
}

fn check_ids(targets: &Targets, artifact: &str, out: &mut Vec<Diagnostic>) {
    for id in &targets.supported {
        if capability::by_id(id).is_none() {
            out.push(
                Diagnostic::error(UNKNOWN, format!("unknown target id `{id}`"))
                    .with_artifact(artifact.to_string())
                    .with_help("known targets: claude-code, cursor, agents-md"),
            );
        }
    }
}
