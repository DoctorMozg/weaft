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
///
/// Refuses any `relative` path containing a `..` component — backends should never produce
/// such paths, but this guards against a capability-matrix or backend bug that would
/// otherwise silently write files outside the dist directory.
pub fn write_file(base: &Path, relative: &Path, contents: &[u8]) -> miette::Result<()> {
    if relative
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return Err(miette::miette!(
            "refusing to write `{}`: path contains a `..` component",
            relative.display()
        ));
    }
    let path = base.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).into_diagnostic()?;
    }
    std::fs::write(&path, contents).into_diagnostic()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_file;
    use std::path::Path;

    #[test]
    fn write_file_rejects_parent_dir_component() {
        // The path never touches the filesystem — the guard fires before `base.join()`.
        let tmp = std::env::temp_dir();
        let result = write_file(tmp.as_path(), Path::new("subdir/../../../etc/passwd"), b"x");
        assert!(result.is_err(), "write_file must refuse a path with `..`");
        let msg = format!("{result:?}");
        assert!(
            msg.contains(".."),
            "the error message must mention the `..` component; got: {msg}",
        );
    }
}
