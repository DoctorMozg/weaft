//! `weaft tokens` — report token usage per target.
//!
//! NOTE: token counts are tokenizer **approximations** (Claude's real tokenizer is not
//! public; weaft uses `cl100k_base`). Budgets (8000/6000) are weaft **heuristics**, not
//! host-documented limits. Treat the numbers as guidance, not billing accuracy.
//!
//! Per-artifact reporting is gated on the **disposition resolver** (WU-20): a line is printed
//! only when `resolve(artifact, host).disposition != Drop`, i.e. only for artifacts the host
//! actually emits. Reporting a budget for a file that is never produced would be dishonest, so a
//! skill targeting a host whose Skill cell drops (e.g. gemini-cli) is omitted. The budget unit is
//! taken from the resolved cell, so byte budgets are labeled distinctly from token budgets.

use super::build::{resolve_params, select_targets};
use std::path::Path;
use std::process::ExitCode;
use weaft_core::budget::{Budget, BudgetUnit};
use weaft_core::capability::{Disposition, HostCapabilities};
use weaft_core::compile;
use weaft_core::pipeline::resolve::resolve;
use weaft_core::tokens;

/// Report token usage per target.
///
/// Counts are tokenizer approximations and budgets are weaft heuristics, not
/// host-documented limits — see the module docs.
#[derive(clap::Args)]
pub struct Args {
    /// Report only this target id (default: every supported target).
    #[arg(long)]
    pub target: Option<String>,
    /// Override a declared parameter, repeatable: `--param key=value`.
    #[arg(long = "param", value_name = "KEY=VALUE")]
    pub params: Vec<String>,
}

pub fn run(manifest: &Path, args: &Args) -> miette::Result<ExitCode> {
    let project = super::load(manifest)?;
    let resolved_params = resolve_params(&project, &args.params)?;
    let targets = select_targets(&project, args.target.as_deref())?;

    for target in targets {
        let host = target.capabilities();
        println!("{} ({}):", host.display_name, host.id);

        for artifact in &project.artifacts {
            if !artifact.frontmatter.targets.supports(target.id()) {
                continue;
            }
            // Gate on the disposition resolver, not a host-wide bool: only report budgets for
            // artifacts the host actually emits (a dropped kind produces no file to budget).
            let resolved = resolve(artifact, host);
            if resolved.disposition == Disposition::Drop {
                continue;
            }
            let body = compile::render(
                artifact,
                &project.info,
                host,
                &resolved_params,
                &project.root,
            )?;
            print_line(&artifact_label(artifact), &body, resolved.cell.budget, host);
        }
        println!();
    }

    println!("note: counts are tokenizer approximations; budgets are weaft heuristics.");
    Ok(ExitCode::SUCCESS)
}

/// Display label for an artifact line: the bare name for skills, name plus kind tag otherwise, so
/// a mixed-kind report stays readable (e.g. `code-reviewer (subagent)`).
fn artifact_label(artifact: &weaft_core::ir::Artifact) -> String {
    match artifact.kind {
        weaft_core::kind::ArtifactKind::Skill => artifact.frontmatter.name.clone(),
        kind => format!("{} ({})", artifact.frontmatter.name, kind.serde_name()),
    }
}

/// Print one usage line. The budget unit comes from the resolved cell: a token budget reports
/// `tokens/limit (pct%)`; a byte budget reports `bytes/limit bytes (pct%)` so the byte axis is
/// labeled distinctly (C-BUDGET-HONESTY); a budgetless cell reports the token count only.
fn print_line(name: &str, body: &str, budget: Option<Budget>, host: &HostCapabilities) {
    match budget.map(|b| (b.unit, b.limit)) {
        Some((BudgetUnit::Tokens(tk), limit)) => {
            let count = tokens::count(body, tk);
            let pct = (count as f64 / limit as f64) * 100.0;
            println!("  {name}: {count}/{limit} ({pct:.0}%)");
        },
        Some((BudgetUnit::Bytes, limit)) => {
            let bytes = body.len();
            let pct = (bytes as f64 / limit as f64) * 100.0;
            println!("  {name}: {bytes}/{limit} bytes ({pct:.0}%)");
        },
        None => {
            let count = tokens::count(body, host.tokenizer);
            println!("  {name}: {count} (no budget)");
        },
    }
}
