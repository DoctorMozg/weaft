//! CLI subcommands and the small helpers they share.

pub mod build;
pub mod init;
pub mod lint;
pub mod preview;
pub mod targets;
pub mod tokens;

use miette::IntoDiagnostic;
use std::path::Path;
use weaft_core::diag::{Diagnostic, Severity};
use weaft_core::ir::Project;
use weaft_core::parse;

/// Load a project from a `--manifest-path` (file or containing directory).
pub fn load(manifest: &Path) -> miette::Result<Project> {
    Ok(parse::load_project(manifest)?)
}

/// Tally of reported diagnostics.
#[derive(Debug, Default, Clone, Copy)]
pub struct DiagTally {
    pub errors: usize,
    pub warnings: usize,
}

impl DiagTally {
    pub fn has_errors(self) -> bool {
        self.errors > 0
    }
}

/// Print collected lint/emit diagnostics to stderr and return their tally.
pub fn report(diags: &[Diagnostic]) -> DiagTally {
    let mut tally = DiagTally::default();
    for d in diags {
        match d.severity {
            Severity::Error => tally.errors += 1,
            Severity::Warning => tally.warnings += 1,
        }
        let artifact = d
            .artifact
            .as_deref()
            .map(|a| format!(" ({a})"))
            .unwrap_or_default();
        eprintln!(
            "{}[{}]: {}{artifact}",
            d.severity.label(),
            d.code,
            d.message
        );
        if let Some(help) = &d.help {
            eprintln!("  help: {help}");
        }
    }
    tally
}

/// Write `contents` to `base/relative`, creating parent directories as needed.
pub fn write_file(base: &Path, relative: &Path, contents: &[u8]) -> miette::Result<()> {
    let path = base.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).into_diagnostic()?;
    }
    std::fs::write(&path, contents).into_diagnostic()?;
    Ok(())
}
