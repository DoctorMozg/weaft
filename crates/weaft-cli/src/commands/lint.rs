//! `weaft lint` — run all lint passes and report findings.
//!
//! Besides the core lint passes, `lint` runs a **dry-run** of the build pipeline through the same
//! [`merge_and_check`] path `weaft build` uses (Fix 2): the hard-byte cap (ADR-0005, e.g. codex's
//! 32 KiB `AGENTS.md`) needs the *merged* emitted bytes, which core lint cannot see, so it is
//! enforced here by driving the real merge stage without writing any files. This guarantees `lint`
//! and `build` agree on the byte cap because they exercise one code path, not two.

use super::build::{compile_target, merge_and_check, project_targets};
use super::report;
use std::path::Path;
use std::process::ExitCode;
use weaft_core::diag::Diagnostic;
use weaft_core::lint;

#[derive(clap::Args)]
pub struct Args {
    /// Treat token-budget overflows and skill-layout warnings as errors (otherwise warnings).
    #[arg(long)]
    pub strict: bool,
}

pub fn run(manifest: &Path, args: &Args) -> miette::Result<ExitCode> {
    let project = super::load(manifest)?;

    let mut diags = lint::run(&project, args.strict);
    let dry = hard_byte_dry_run(&project, &diags);
    diags.extend(dry);

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

/// The diagnostic code the core budget lint emits for render failures. The dry-run reuses it so a
/// render error it surfaces deduplicates against (and reads the same as) the core one.
const RENDER: &str = "weaft::lint::render";

/// Drive the build pipeline's merge stage for every target the project compiles for, returning the
/// hard-byte diagnostics without writing any files (Fix 2).
///
/// This reuses [`compile_target`] + [`merge_and_check`] verbatim, so the hard-byte cap activates
/// for `lint` exactly when it activates for `build` — including for any host (e.g. codex) whose
/// backend is registered later. Only [`merge_and_check`]'s diagnostics are folded in; the
/// `compile_target` drop warnings are already produced by the core `targets` lint, so folding them
/// here too would double-report them.
///
/// The dry-run never aborts the lint report: a render failure (a strict-undefined `host.*`, a
/// missing `{% include %}`) is collected as a diagnostic and the remaining targets still run, so
/// every already-collected finding is still reported. A render error is surfaced here only when the
/// core passes reported none — the core budget lint already render-checks budgeted skill cells, so
/// this avoids double-reporting while still catching render failures in the kinds it does not
/// budget (subagents, instructions). A param-resolution failure is likewise left to that lint.
fn hard_byte_dry_run(
    project: &weaft_core::ir::Project,
    existing: &[Diagnostic],
) -> Vec<Diagnostic> {
    // Lint has no `--param`, so render with declared defaults (matches the core budget lint). A
    // failure here is already reported by that lint; swallow it rather than abort the whole report.
    let Ok(resolved) = super::build::resolve_params(project, &[]) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut render_reported = existing.iter().any(|d| d.code == RENDER);
    for target in project_targets(project) {
        match compile_target(project, target, &resolved) {
            Ok((tagged, _drop_warnings)) => {
                let (_files, byte_diags) = merge_and_check(tagged);
                out.extend(byte_diags);
            },
            // Collect at most one render error (the core budget lint owns precise per-cell
            // reporting); the point here is only to not let a render failure swallow the report.
            Err(e) => {
                if !render_reported {
                    render_reported = true;
                    out.push(
                        Diagnostic::error(RENDER, e.to_string())
                            .with_help("check `host.*` references and `{% include %}` paths"),
                    );
                }
            },
        }
    }
    out
}
