//! `weaft lint` — run all lint passes and report findings.

use super::report;
use std::path::Path;
use std::process::ExitCode;
use weaft_core::lint;

#[derive(clap::Args)]
pub struct Args {
    /// Treat token-budget overflows as errors (otherwise warnings).
    #[arg(long)]
    pub strict: bool,
}

pub fn run(manifest: &Path, args: &Args) -> miette::Result<ExitCode> {
    let project = super::load(manifest)?;
    let diags = lint::run(&project, args.strict);
    let tally = report(&diags);

    if diags.is_empty() {
        eprintln!("no issues found");
    } else {
        eprintln!("{} error(s), {} warning(s)", tally.errors, tally.warnings);
    }

    Ok(if tally.has_errors() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}
