//! `weaft build` — compile skills and subagents to host-specific files under `dist/`.

use super::{report, write_file};
use miette::IntoDiagnostic;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use weaft_core::compile;
use weaft_core::diag::{Diagnostic, WeftError};
use weaft_core::ir::Project;
use weaft_core::params::{self, ParamValues};
use weaft_core::{fs, lint};
use weaft_targets::{EmittedFile, Target, target_by_id};

#[derive(clap::Args)]
pub struct Args {
    /// Build only this target id (default: every target the project supports).
    #[arg(long)]
    pub target: Option<String>,
    /// Output directory.
    #[arg(long, default_value = "dist")]
    pub out: PathBuf,
    /// Override a declared parameter, repeatable: `--param key=value`.
    #[arg(long = "param", value_name = "KEY=VALUE")]
    pub params: Vec<String>,
}

pub fn run(manifest: &Path, args: Args) -> miette::Result<ExitCode> {
    let project = super::load(manifest)?;
    let resolved = resolve_params(&project, &args.params)?;
    let targets = select_targets(&project, args.target.as_deref())?;

    let mut all_diags: Vec<Diagnostic> = Vec::new();
    let mut total_files = 0usize;

    for target in &targets {
        let (files, diags) = compile_target(&project, *target, &resolved)?;
        all_diags.extend(diags);

        let target_dir = args.out.join(target.id());
        for (relative, contents) in merge(files) {
            write_file(&target_dir, &relative, &contents)?;
            total_files += 1;
        }

        if target.capabilities().supports_assets {
            total_files += copy_assets(&project, *target, &target_dir, &resolved)?;
        }
    }

    let tally = report(&all_diags);
    eprintln!(
        "built {total_files} file(s) for {} target(s) → {}",
        targets.len(),
        args.out.display()
    );

    Ok(if tally.has_errors() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// Resolve declared parameters against `--param` overrides.
pub fn resolve_params(project: &Project, raw: &[String]) -> miette::Result<ParamValues> {
    let overrides = raw
        .iter()
        .map(|r| params::parse_override(r))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(params::resolve(&project.info.parameters, &overrides)?)
}

/// Pick the target backends to build: one explicit `--target`, or all project hosts.
pub fn select_targets(
    project: &Project,
    explicit: Option<&str>,
) -> miette::Result<Vec<&'static dyn Target>> {
    match explicit {
        Some(id) => {
            let t = target_by_id(id).ok_or_else(|| WeftError::UnknownTarget(id.to_string()))?;
            Ok(vec![t])
        }
        None => Ok(lint::project_hosts(project)
            .into_iter()
            .filter_map(|h| target_by_id(h.id))
            .collect()),
    }
}

/// Render and emit every supported skill/agent for one target.
pub fn compile_target(
    project: &Project,
    target: &dyn Target,
    params: &ParamValues,
) -> miette::Result<(Vec<EmittedFile>, Vec<Diagnostic>)> {
    let host = target.capabilities();
    let mut files = Vec::new();
    let mut diags = Vec::new();

    for skill in &project.skills {
        if !skill.frontmatter.targets.supports(target.id()) {
            continue;
        }
        let body = compile::render_skill(skill, &project.info, host, params, &project.root)?;
        let out = target.emit_skill(&project.info, &skill.frontmatter, &body);
        files.extend(out.files);
        diags.extend(out.diagnostics);
    }

    for agent in &project.agents {
        if !agent.frontmatter.targets.supports(target.id()) {
            continue;
        }
        let body = compile::render_agent(agent, &project.info, host, params, &project.root)?;
        let out = target.emit_agent(&project.info, &agent.frontmatter, &body);
        files.extend(out.files);
        diags.extend(out.diagnostics);
    }

    Ok((files, diags))
}

/// Merge emitted files by path. Files flagged `concatenate` (e.g. AGENTS.md sections)
/// are joined; others are unique and pass through.
fn merge(files: Vec<EmittedFile>) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut merged: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
    for f in files {
        match merged.get_mut(&f.relative_path) {
            Some(existing) if f.concatenate => {
                existing.push(b'\n');
                existing.extend_from_slice(&f.contents);
            }
            _ => {
                merged.insert(f.relative_path, f.contents);
            }
        }
    }
    merged
}

/// Copy `assets/` into each emitted skill directory (Claude-style hosts only).
fn copy_assets(
    project: &Project,
    target: &dyn Target,
    target_dir: &Path,
    params: &ParamValues,
) -> miette::Result<usize> {
    let assets = fs::list_assets(&project.root)?;
    if assets.is_empty() {
        return Ok(0);
    }
    let mut count = 0;
    for skill in &project.skills {
        if !skill.frontmatter.targets.supports(target.id()) {
            continue;
        }
        let _ = params; // assets are copied verbatim; params do not affect them
        let skill_dir = target_dir.join("skills").join(&skill.frontmatter.name);
        for asset in &assets {
            let bytes = fs::read_bytes(&asset.absolute)?;
            let dest = skill_dir.join(&asset.relative);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).into_diagnostic()?;
            }
            std::fs::write(&dest, bytes).into_diagnostic()?;
            count += 1;
        }
    }
    Ok(count)
}
