//! Token-budget lint. Renders each artifact for each supported host and compares the
//! token count against the host's soft budget. Also surfaces render failures (including
//! undefined `host.*` references caught by strict-undefined mode) as hard errors.
//!
//! Budgets are weaft heuristics, not host-documented limits (see `capability.rs`).

use crate::diag::Diagnostic;
use crate::ir::Project;
use crate::{compile, params, tokens};

const BUDGET: &str = "weaft::lint::token_budget";
const RENDER: &str = "weaft::lint::render";

/// Fraction of the budget at which a warning fires.
const WARN_AT: f64 = 0.80;

pub fn check(project: &Project, strict: bool) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // Lint renders with declared defaults; CLI build may override with --param.
    let resolved = match params::resolve(&project.info.parameters, &[]) {
        Ok(p) => p,
        Err(e) => {
            out.push(Diagnostic::error(RENDER, e.to_string()));
            return out;
        }
    };

    for skill in &project.skills {
        for host in super::artifact_hosts(project, &skill.frontmatter.targets) {
            match compile::render_skill(skill, &project.info, host, &resolved, &project.root) {
                Ok(body) => check_one(&mut out, &skill.frontmatter.name, host, &body, strict),
                Err(e) => out.push(render_error(&skill.frontmatter.name, host.id, &e)),
            }
        }
    }

    for agent in &project.agents {
        for host in super::artifact_hosts(project, &agent.frontmatter.targets) {
            if !host.supports_subagents {
                continue; // skipped at emit time; not a budget concern
            }
            match compile::render_agent(agent, &project.info, host, &resolved, &project.root) {
                Ok(body) => check_one(&mut out, &agent.frontmatter.name, host, &body, strict),
                Err(e) => out.push(render_error(&agent.frontmatter.name, host.id, &e)),
            }
        }
    }

    out
}

fn check_one(
    out: &mut Vec<Diagnostic>,
    name: &str,
    host: &crate::capability::HostCapabilities,
    body: &str,
    strict: bool,
) {
    let Some(budget) = host.max_skill_tokens else {
        return; // unbounded host
    };
    let count = tokens::count(body, host.tokenizer);
    let pct = count as f64 / budget as f64;
    let artifact = format!("{name} → {}", host.id);

    if count > budget {
        let msg = format!(
            "{count}/{budget} tokens ({:.0}%) — over the {} budget",
            pct * 100.0,
            host.id
        );
        let diag = if strict {
            Diagnostic::error(BUDGET, msg)
        } else {
            Diagnostic::warning(BUDGET, msg)
        };
        out.push(
            diag.with_artifact(artifact)
                .with_help("trim the body or move detail into assets/ (Claude) or fragments"),
        );
    } else if pct >= WARN_AT {
        out.push(
            Diagnostic::warning(
                BUDGET,
                format!(
                    "{count}/{budget} tokens ({:.0}%) — approaching budget",
                    pct * 100.0
                ),
            )
            .with_artifact(artifact),
        );
    }
}

fn render_error(name: &str, host_id: &str, e: &crate::diag::WeftError) -> Diagnostic {
    Diagnostic::error(RENDER, format!("{e}"))
        .with_artifact(format!("{name} → {host_id}"))
        .with_help("check `host.*` references and `{% include %}` paths")
}
