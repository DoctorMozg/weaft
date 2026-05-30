//! `weaft tokens` — report token usage per target.
//!
//! NOTE: token counts are tokenizer **approximations** (Claude's real tokenizer is not
//! public; weaft uses `cl100k_base`). Budgets (8000/6000) are weaft **heuristics**, not
//! host-documented limits. Treat the numbers as guidance, not billing accuracy.

use super::build::{resolve_params, select_targets};
use std::path::Path;
use std::process::ExitCode;
use weaft_core::compile;
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
    let resolved = resolve_params(&project, &args.params)?;
    let targets = select_targets(&project, args.target.as_deref())?;

    for target in targets {
        let host = target.capabilities();
        println!("{} ({}):", host.display_name, host.id);

        for skill in &project.skills {
            if !skill.frontmatter.targets.supports(target.id()) {
                continue;
            }
            let body = compile::render_skill(skill, &project.info, host, &resolved, &project.root)?;
            print_line(&skill.frontmatter.name, &body, host);
        }

        for agent in &project.agents {
            if !host.supports_subagents || !agent.frontmatter.targets.supports(target.id()) {
                continue;
            }
            let body = compile::render_agent(agent, &project.info, host, &resolved, &project.root)?;
            print_line(&format!("{} (agent)", agent.frontmatter.name), &body, host);
        }
        println!();
    }

    println!("note: counts are tokenizer approximations; budgets are weaft heuristics.");
    Ok(ExitCode::SUCCESS)
}

fn print_line(name: &str, body: &str, host: &weaft_core::capability::HostCapabilities) {
    let count = tokens::count(body, host.tokenizer);
    match host.max_skill_tokens {
        Some(budget) => {
            let pct = (count as f64 / budget as f64) * 100.0;
            println!("  {name}: {count}/{budget} ({pct:.0}%)");
        },
        None => println!("  {name}: {count} (no budget)"),
    }
}
