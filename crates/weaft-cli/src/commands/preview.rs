//! `weaft preview` — render artifacts for one target and print to stdout (no files).

use super::build::{compile_target, resolve_params, select_targets};
use std::path::Path;
use std::process::ExitCode;
use weaft_core::diag::WeftError;

#[derive(clap::Args)]
pub struct Args {
    /// Target to preview (default: the project's first supported target).
    #[arg(long)]
    pub target: Option<String>,
    /// Override a declared parameter, repeatable: `--param key=value`.
    #[arg(long = "param", value_name = "KEY=VALUE")]
    pub params: Vec<String>,
}

pub fn run(manifest: &Path, args: Args) -> miette::Result<ExitCode> {
    let project = super::load(manifest)?;
    let resolved = resolve_params(&project, &args.params)?;

    // Default to the first selected target rather than every one.
    let target = *select_targets(&project, args.target.as_deref())?
        .first()
        .ok_or_else(|| WeftError::UnknownTarget("<none supported>".to_string()))?;

    let (files, diags) = compile_target(&project, target, &resolved)?;

    for d in &diags {
        eprintln!("{}[{}]: {}", d.severity.label(), d.code, d.message);
    }

    println!(
        "# preview: {} ({})\n",
        target.capabilities().display_name,
        target.id()
    );
    for f in files {
        println!("# === {} ===", f.relative_path.display());
        println!("{}\n", String::from_utf8_lossy(&f.contents));
    }

    Ok(ExitCode::SUCCESS)
}
