//! `weaft lint` — run all lint passes and report findings.
//!
//! Besides the core lint passes, `lint` runs a **dry-run** of the build pipeline through the same
//! [`merge_and_check`] path `weaft build` uses (Fix 2): the hard-byte cap (ADR-0005, e.g. codex's
//! 32 KiB `AGENTS.md`) needs the *merged* emitted bytes, which core lint cannot see, so it is
//! enforced here by driving the real merge stage without writing any files. This guarantees `lint`
//! and `build` agree on the byte cap because they exercise one code path, not two.

use super::build::{compile_target, merge_and_check, select_targets};
use super::report;
use std::path::Path;
use std::process::ExitCode;
use weaft_core::diag::Diagnostic;
use weaft_core::lint;

#[derive(clap::Args)]
pub struct Args {
    /// Treat token-budget overflows as errors (otherwise warnings).
    #[arg(long)]
    pub strict: bool,
}

pub fn run(manifest: &Path, args: &Args) -> miette::Result<ExitCode> {
    let project = super::load(manifest)?;

    let mut diags = lint::run(&project, args.strict);
    diags.extend(hard_byte_dry_run(&project)?);

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

/// Drive the build pipeline's merge stage for every target the project compiles for, returning the
/// hard-byte diagnostics without writing any files (Fix 2).
///
/// This reuses [`compile_target`] + [`merge_and_check`] verbatim, so the hard-byte cap activates
/// for `lint` exactly when it activates for `build` — including for any host (e.g. codex) whose
/// backend is registered later. Only [`merge_and_check`]'s diagnostics are folded in; the
/// `compile_target` drop warnings are already produced by the core `targets` lint, so folding them
/// here too would double-report them.
fn hard_byte_dry_run(project: &weaft_core::ir::Project) -> miette::Result<Vec<Diagnostic>> {
    // Lint has no `--param`, so render with declared defaults (matches the core budget lint).
    let resolved = super::build::resolve_params(project, &[])?;
    let targets = select_targets(project, None)?;

    let mut out = Vec::new();
    for target in targets {
        let (tagged, _drop_warnings) = compile_target(project, target, &resolved)?;
        let (_files, byte_diags) = merge_and_check(tagged);
        out.extend(byte_diags);
    }
    Ok(out)
}
