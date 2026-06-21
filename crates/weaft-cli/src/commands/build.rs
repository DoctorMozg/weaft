//! `weaft build` — compile skills and subagents to host-specific files under `dist/`.

use super::{report, write_file};
use miette::IntoDiagnostic;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use weaft_core::budget::{Budget, BudgetSeverity, BudgetUnit};
use weaft_core::capability::{self, Disposition};
use weaft_core::compile;
use weaft_core::diag::{Diagnostic, WeftError};
use weaft_core::ir::Project;
use weaft_core::params::{self, ParamValues};
use weaft_core::pipeline::emit::EmittedSpec;
use weaft_core::pipeline::map::map_fields;
use weaft_core::pipeline::resolve::{Resolved, resolve};
use weaft_core::{fs, lint};
use weaft_targets::{EmittedFile, Target, serialize, target_by_id};

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

pub fn run(manifest: &Path, args: &Args) -> miette::Result<ExitCode> {
    let project = super::load(manifest)?;
    let resolved = resolve_params(&project, &args.params)?;
    let targets = select_targets(&project, args.target.as_deref())?;

    let mut all_diags: Vec<Diagnostic> = Vec::new();
    let mut total_files = 0usize;

    for target in &targets {
        let (tagged, diags) = compile_target(&project, *target, &resolved)?;
        all_diags.extend(diags);

        // Drive the single merge stage so the hard-byte check (WU-14) fires on real builds.
        let (merged, merge_diags) = merge_and_check(tagged);
        all_diags.extend(merge_diags);

        let target_dir = args.out.join(target.id());
        for (relative, contents) in merged {
            write_file(&target_dir, &relative, &contents)?;
            total_files += 1;
        }

        if target.capabilities().supports_assets {
            total_files += copy_assets(&project, *target, &target_dir)?;
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
pub(crate) fn resolve_params(project: &Project, raw: &[String]) -> miette::Result<ParamValues> {
    let overrides = raw
        .iter()
        .map(|r| params::parse_override(r))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(params::resolve(&project.info.parameters, &overrides)?)
}

/// Pick the target backends to build: one explicit `--target`, or all project hosts.
///
/// With no explicit `--target`, an unknown id in the project's declared `supported` list is a hard
/// error — otherwise a typo (`claud-code`) would silently select zero targets and let a build /
/// preview / tokens run "succeed" having produced nothing. `lint` deliberately tolerates the same
/// typo (it reports it through the core `targets` lint) and so uses [`project_targets`] directly.
pub(crate) fn select_targets(
    project: &Project,
    explicit: Option<&str>,
) -> miette::Result<Vec<&'static dyn Target>> {
    if let Some(id) = explicit {
        let t = target_by_id(id).ok_or_else(|| WeftError::unknown_target(id))?;
        return Ok(vec![t]);
    }
    if let Some(unknown) = project
        .info
        .targets
        .supported
        .iter()
        .find(|id| capability::by_id(id.as_str()).is_none())
    {
        return Err(WeftError::unknown_target(unknown.as_str()).into());
    }
    Ok(project_targets(project))
}

/// Every registered backend the project compiles for, silently skipping any unknown declared id.
/// Tolerant by design: build / preview / tokens reach it through [`select_targets`], which
/// validates the declared ids first; `lint` calls it directly because it reports unknown ids
/// through the core `targets` lint and must not abort its dry-run on one.
pub(crate) fn project_targets(project: &Project) -> Vec<&'static dyn Target> {
    lint::project_hosts(project)
        .into_iter()
        .filter_map(|h| target_by_id(h.id))
        .collect()
}

/// A merge-ready emitted item: the framed spec plus the per-path budget and host id the
/// hard-byte check needs (so [`merge_and_check`] never re-enters the capability matrix).
pub(crate) type TaggedSpec = (EmittedSpec, Option<Budget>, &'static str);

/// The generic v2 pipeline driver: drive resolve → render → map → serialize → emit over every
/// artifact for one target (ADR-0001).
///
/// For each artifact the project compiles for this host:
/// - `resolve` decides the disposition. A `Drop` raises one warning naming the artifact and the
///   host (the matrix-driven drop that replaces v1's hardcoded backend skips) and emits no file.
/// - `Native`/`Fold` flow through `render` (core) → `map_fields` (core, with the host's transform
///   set) → `serialize::frame` (targets) → `Target::emit_artifact` (targets, path from the matrix
///   layout). Each emitted file is tagged with its cell's budget + host id for the byte check.
///
/// After the per-artifact loop the host's `post_emit` seam runs over the file list; the returned
/// [`TaggedSpec`]s are then merged and byte-checked by [`merge_and_check`] in `run`.
pub fn compile_target(
    project: &Project,
    target: &dyn Target,
    params: &ParamValues,
) -> miette::Result<(Vec<TaggedSpec>, Vec<Diagnostic>)> {
    let host = target.capabilities();
    let mut files: Vec<EmittedFile> = Vec::new();
    let mut diags: Vec<Diagnostic> = Vec::new();
    // Budget + host id keyed by normalized emit path: path-scoped metadata that survives any
    // reordering `post_emit` might do, mirroring how `merge_and_check` keys budgets by path.
    let mut path_meta: BTreeMap<String, (Option<Budget>, &'static str)> = BTreeMap::new();

    for artifact in &project.artifacts {
        if !artifact.frontmatter.targets.supports(target.id()) {
            continue;
        }
        let resolved = resolve(artifact, host);
        if resolved.disposition == Disposition::Drop {
            diags.push(drop_warning(artifact, host.id));
            continue;
        }

        let body = compile::render(artifact, &project.info, host, params, &project.root)?;
        let mapped = map_fields(&resolved, artifact, target.field_transforms());
        let framed = serialize::frame(resolved.cell.format, &mapped, &body);
        let file = target.emit_artifact(&resolved, &artifact.frontmatter.name, framed);

        let key = normalize_separators(&file.relative_path);
        path_meta
            .entry(key)
            .or_insert((resolved.cell.budget, host.id));
        files.push(file);
    }

    target.post_emit(&mut files);

    let tagged = files
        .into_iter()
        .map(|file| {
            let key = normalize_separators(&file.relative_path);
            let (budget, host_id) = path_meta.get(&key).copied().unwrap_or((None, ""));
            // EmittedFile contents are built from a String (text/section/emit_artifact), so the
            // bytes are provably valid UTF-8 — the conversion cannot fail on real input.
            let contents = String::from_utf8(file.contents).expect(
                "emitted files are rendered UTF-8 text (built from String, never raw bytes)",
            );
            (
                EmittedSpec {
                    relative_path: file.relative_path,
                    contents,
                    merge: file.concatenate,
                },
                budget,
                host_id,
            )
        })
        .collect();

    Ok((tagged, diags))
}

/// The matrix-driven drop warning for a present `(artifact, host)` pair whose cell is `Drop`.
/// Replaces the v1 hardcoded backend skip (e.g. agents-md subagents) with one generic, data-driven
/// diagnostic naming both the artifact and the host (C-SUPPORT-DISPOSITION).
fn drop_warning(artifact: &weaft_core::ir::Artifact, host_id: &str) -> Diagnostic {
    Diagnostic::warning(
        "weaft::emit::dropped",
        format!(
            "{} `{}` skipped: host `{host_id}` cannot represent its kind",
            artifact.kind.serde_name(),
            artifact.frontmatter.name,
        ),
    )
    .with_artifact(artifact.frontmatter.name.clone())
}

/// WU-14: the merge stage — deterministic concatenation + hard-byte enforcement.
///
/// This is the seventh pipeline stage and the only place that sees the *merged* bytes together
/// with core's budget data, so ADR-0005's hard-byte check lives here, not in core (Fix 2/3).
/// Each item carries its own `Option<Budget>` (copied from the resolved cell) and host id, so the
/// stage never re-enters the capability matrix.
///
/// Behavior:
/// 1. Specs with `merge == true` sharing a path concatenate with a single `\n` (byte-identical to
///    the v1 `merge()` join); `merge == false` specs are last-wins on their path.
/// 2. Output is normalized for determinism (C-DETERMINISM): `\r\n` → `\n` in contents, and `\`
///    → `/` in path keys, so the merged map is byte-identical regardless of authoring platform.
/// 3. *After* merging, each final path's hard-byte budget (`Bytes` + `Hard`) is checked against
///    the final size; an overflow yields a blocking error naming the host and warning of silent
///    truncation. The over-budget file is still produced — the check reports, it does not drop.
pub fn merge_and_check(
    items: Vec<(EmittedSpec, Option<Budget>, &'static str)>,
) -> (BTreeMap<PathBuf, Vec<u8>>, Vec<Diagnostic>) {
    // Group on the normalized forward-slash path string so two specs that differ only in
    // separator style still merge into one entry. Insertion order is preserved within a path so
    // sections concatenate in emit order; the final BTreeMap re-sorts paths deterministically.
    let mut merged: BTreeMap<String, String> = BTreeMap::new();
    let mut path_meta: BTreeMap<String, (Option<Budget>, &'static str)> = BTreeMap::new();

    for (spec, budget, host_id) in items {
        let key = normalize_separators(&spec.relative_path);

        // All specs sharing a merged path come from the same (host, kind) cell, so they carry the
        // same budget; record the first non-None budget seen for this path for the byte check.
        path_meta
            .entry(key.clone())
            .and_modify(|meta| {
                if meta.0.is_none() {
                    *meta = (budget, host_id);
                }
            })
            .or_insert((budget, host_id));

        match merged.get_mut(&key) {
            Some(existing) if spec.merge => {
                existing.push('\n');
                existing.push_str(&spec.contents);
            },
            _ => {
                merged.insert(key, spec.contents);
            },
        }
    }

    let mut diags = Vec::new();
    let out = merged
        .into_iter()
        .map(|(key, contents)| {
            let contents = normalize_newlines(&contents);
            if let Some((Some(budget), host_id)) = path_meta.get(&key) {
                check_hard_bytes(budget, contents.len(), &key, host_id, &mut diags);
            }
            (PathBuf::from(key), contents.into_bytes())
        })
        .collect();

    (out, diags)
}

/// Emit a blocking hard-byte diagnostic if a final merged file exceeds a `Bytes` + `Hard` budget.
/// `check_bytes` returns `None` for non-byte budgets, so soft/token budgets are silently skipped
/// here (they are enforced by core's lint on the rendered body instead, Fix 2).
fn check_hard_bytes(
    budget: &Budget,
    final_len: usize,
    path: &str,
    host_id: &str,
    diags: &mut Vec<Diagnostic>,
) {
    if budget.unit != BudgetUnit::Bytes || budget.severity != BudgetSeverity::Hard {
        return;
    }
    let Some(violation) = budget.check_bytes(final_len) else {
        return;
    };
    diags.push(
        Diagnostic::error(
            "weaft::budget::hard_bytes",
            format!(
                "{path} is {} bytes — {} bytes over the {}-byte limit for host `{host_id}`, \
                 which truncates silently past {} bytes",
                violation.actual, violation.over_by, violation.limit, violation.limit
            ),
        )
        .with_help(format!(
            "trim the merged output: `{host_id}` silently truncates this file past {} bytes ({})",
            violation.limit, violation.source
        ))
        .with_artifact(path.to_string()),
    );
}

/// Normalize a path to a forward-slash string (C-DETERMINISM): emitted keys are `/`-separated
/// regardless of host OS, so the merged map is byte-identical across platforms.
fn normalize_separators(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Normalize line endings to `\n` (C-DETERMINISM): a CRLF-bearing body emits with LF only, so
/// output does not depend on the authoring platform.
fn normalize_newlines(contents: &str) -> String {
    contents.replace("\r\n", "\n")
}

/// Copy `assets/` alongside each emitted skill, into the directory the host's matrix layout puts
/// `SKILL.md` in. Only hosts whose skill cell bundles assets reach here (claude-code, codex); each
/// host gets the assets in the right place — `skills/<name>/` for claude, `.agents/skills/<name>/`
/// for codex — instead of a hardcoded `skills/<name>/` that misplaced codex's.
fn copy_assets(project: &Project, target: &dyn Target, target_dir: &Path) -> miette::Result<usize> {
    let assets = fs::list_assets(&project.root)?;
    if assets.is_empty() {
        return Ok(0);
    }
    let host = target.capabilities();
    let mut count = 0;
    for skill in project.skills() {
        if !skill.frontmatter.targets.supports(target.id()) {
            continue;
        }
        let resolved = resolve(skill, host);
        // A host that drops the Skill kind emits no SKILL.md, so there is nowhere to put assets.
        if resolved.disposition == Disposition::Drop {
            continue;
        }
        let skill_dir = target_dir.join(skill_asset_dir(&resolved, &skill.frontmatter.name));
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

/// The directory a skill's bundled assets land in, relative to the target output root: the parent
/// of its rendered `SKILL.md` path, taken from the matrix layout (C-CAPABILITY-MATRIX — never a
/// hardcoded path). This is what lets codex assets follow `.agents/skills/<name>/` and claude
/// assets `skills/<name>/` from one code path.
fn skill_asset_dir(resolved: &Resolved, name: &str) -> PathBuf {
    let skill_rel = PathBuf::from(resolved.cell.layout.path_template.replace("{name}", name));
    skill_rel
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// WU-14: `merge_and_check` — deterministic merge + hard-byte enforcement (RED).
///
/// These tests are inline (not an integration test under `tests/`) on purpose: `weaft-cli` is a
/// **binary-only crate** (`main.rs`, no `lib.rs`), so a `tests/` integration target cannot import
/// `weaft-cli`'s internal `merge_and_check`. An inline `#[cfg(test)] mod` is the only way to
/// unit-test a function living inside the binary, and it can reach `merge_and_check` whether the
/// GREEN coder makes it `pub` or leaves it private. (The dispatch's `tests/merge_check_tests.rs`
/// option is not viable for this crate for that reason.)
///
/// ## Chosen `merge_and_check` input shape (the GREEN coder must match this)
///
/// `merge_and_check` is self-contained: each emitted item carries the budget and host-id it needs
/// so the function never has to re-enter the capability matrix. The chosen signature is:
///
/// ```ignore
/// pub fn merge_and_check(
///     items: Vec<(EmittedSpec, Option<Budget>, &'static str /* host_id */)>,
/// ) -> (BTreeMap<PathBuf, Vec<u8>>, Vec<Diagnostic>)
/// ```
///
/// - `EmittedSpec` is `weaft_core::pipeline::emit::EmittedSpec { relative_path, contents, merge }`
///   (WU-14, authored alongside this).
/// - The optional `Budget` is copied verbatim from the resolved cell (`resolved.cell.budget`),
///   so the merge stage can apply a `Bytes`+`Hard` budget to the *final* merged file without a
///   matrix lookup (ADR-0005 / Fix 2 — the byte check belongs in the merge stage, in cli).
/// - The `&'static str` is the host id (e.g. `"codex"`), threaded so the byte-overflow diagnostic
///   can *name the host* (the plan requires the message to name the host and mention silent
///   truncation). Items sharing a merged path carry the same budget+host.
///
/// The GREEN coder MAY instead fold the tuple into a small struct (e.g.
/// `MergeItem { spec, budget, host_id }`) or attach the budget+host id onto `EmittedSpec` itself;
/// if so, update the constructor helper `item(...)` below — the assertions are agnostic to that.
///
/// Until WU-14 lands `merge_and_check` and `EmittedSpec`, this module fails to compile with
/// missing-symbol errors — the expected RED. It must not be softened by stubbing them.
#[cfg(test)]
mod tests {
    // RED: neither `merge_and_check` (this module) nor `EmittedSpec` (weaft-core WU-14) exists yet.
    use super::merge_and_check;
    use std::path::{Path, PathBuf};
    use weaft_core::budget::{Budget, BudgetSeverity, BudgetUnit};
    use weaft_core::diag::Diagnostic;
    use weaft_core::pipeline::emit::EmittedSpec;

    /// The one concrete hard-byte cell ADR-0005 requires: Codex's merged `AGENTS.md` silently
    /// truncates past ~32 KiB. Attached to a merged item, it drives the `merge_and_check` byte
    /// check (this is the exact `Budget` the codex Instruction cell carries — WU-6).
    fn codex_hard_byte_budget() -> Budget {
        Budget {
            limit: 32_768,
            unit: BudgetUnit::Bytes,
            severity: BudgetSeverity::Hard,
            source: "Codex AGENTS.md ~32 KiB cap (ADR-0005)",
        }
    }

    /// Build one merge input: a spec at `path` with `contents` and `merge=true`, tagged with an
    /// optional budget and the host id used in any overflow diagnostic. This is the single place
    /// to retune if the GREEN coder picks a struct over the `(spec, budget, host_id)` tuple.
    fn item(
        path: &str,
        contents: &str,
        budget: Option<Budget>,
        host_id: &'static str,
    ) -> (EmittedSpec, Option<Budget>, &'static str) {
        (
            EmittedSpec {
                relative_path: PathBuf::from(path),
                contents: contents.to_string(),
                merge: true,
            },
            budget,
            host_id,
        )
    }

    /// A non-merging (standalone) spec — for paths that never concatenate.
    fn standalone(
        path: &str,
        contents: &str,
        budget: Option<Budget>,
        host_id: &'static str,
    ) -> (EmittedSpec, Option<Budget>, &'static str) {
        let (mut spec, b, h) = item(path, contents, budget, host_id);
        spec.merge = false;
        (spec, b, h)
    }

    #[test]
    fn two_merge_sections_join_with_a_single_newline() {
        // Two `merge=true` specs sharing `AGENTS.md` must concatenate into one entry, joined by a
        // single `\n` — byte-identical to the v1 `merge()` behavior (push(b'\n') between sections).
        let items = vec![
            item("AGENTS.md", "first section", None, "agents-md"),
            item("AGENTS.md", "second section", None, "agents-md"),
        ];

        let (merged, diags) = merge_and_check(items);

        let body = merged
            .get(Path::new("AGENTS.md"))
            .expect("the two sections must merge into one AGENTS.md entry");
        assert_eq!(
            String::from_utf8(body.clone()).expect("merged AGENTS.md is utf-8"),
            "first section\nsecond section",
            "merged sections must join with exactly one newline (v1 merge parity)",
        );
        assert_eq!(
            merged.len(),
            1,
            "both sections share one path, so exactly one merged file results",
        );
        assert!(
            diags.iter().all(|d| !d.is_error()),
            "a small merged file must produce no error diagnostics",
        );
    }

    #[test]
    fn codex_instruction_over_32kib_is_a_blocking_error_naming_the_host() {
        // Fix-5 keystone: a codex Instruction merged `AGENTS.md` whose final contents exceed the
        // 32_768-byte hard cap must yield a BLOCKING error diagnostic even without `--strict`
        // (the cell's budget is Hard). The message must name `codex` and mention silent
        // truncation. Author the fixture with a 40_000-byte body (> 32_768) through the codex
        // Instruction cell's byte budget.
        let big_body = "x".repeat(40_000);
        let items = vec![item(
            "AGENTS.md",
            &big_body,
            Some(codex_hard_byte_budget()),
            "codex",
        )];

        let (merged, diags) = merge_and_check(items);

        // The merged file is still produced (the byte check reports, it does not drop the file).
        let body = merged
            .get(Path::new("AGENTS.md"))
            .expect("the oversized instruction file is still merged (the check only reports)");
        assert_eq!(
            body.len(),
            40_000,
            "the final merged size is the body length the byte budget is checked against",
        );

        let errors: Vec<&Diagnostic> = diags.iter().filter(|d| d.is_error()).collect();
        assert_eq!(
            errors.len(),
            1,
            "an over-budget hard-byte file must yield exactly one blocking error, got: {diags:?}",
        );
        let err = errors[0];
        // Blocking without --strict: it is an Error-severity diagnostic (Hard budget always bites).
        assert!(
            err.is_error(),
            "a Hard byte overflow must be an error even without --strict",
        );

        // The message must name the host and mention silent truncation (ADR-0005 wording).
        let haystack = format!(
            "{} {}",
            err.message,
            err.help.as_deref().unwrap_or_default()
        );
        assert!(
            haystack.contains("codex"),
            "the byte-overflow diagnostic must name the host `codex`; got: {haystack:?}",
        );
        assert!(
            haystack.to_lowercase().contains("truncat"),
            "the diagnostic must mention silent truncation; got: {haystack:?}",
        );
    }

    #[test]
    fn merged_file_under_the_hard_budget_yields_no_error() {
        // A codex Instruction merged file comfortably under 32_768 bytes must produce no error —
        // the hard-byte check fires only on overflow (boundary symmetry with the overflow test).
        let small_body = "x".repeat(1_024);
        let items = vec![item(
            "AGENTS.md",
            &small_body,
            Some(codex_hard_byte_budget()),
            "codex",
        )];

        let (merged, diags) = merge_and_check(items);

        assert!(
            merged.contains_key(Path::new("AGENTS.md")),
            "the under-budget file must still be emitted",
        );
        assert!(
            diags.iter().all(|d| !d.is_error()),
            "a file under the hard byte budget must produce no error, got: {diags:?}",
        );
    }

    #[test]
    fn two_codex_sections_overflow_only_on_their_combined_size() {
        // The byte check applies to the FINAL merged size, not per-section: two codex sections that
        // are each under the cap but together exceed it must still trip the hard-byte error. This
        // pins that `check_bytes` runs AFTER merging (ADR-0001 ordering), not before.
        let half = "x".repeat(20_000); // 20_000 each, 40_000 + 1 newline joined > 32_768
        let items = vec![
            item("AGENTS.md", &half, Some(codex_hard_byte_budget()), "codex"),
            item("AGENTS.md", &half, Some(codex_hard_byte_budget()), "codex"),
        ];

        let (_merged, diags) = merge_and_check(items);

        assert_eq!(
            diags.iter().filter(|d| d.is_error()).count(),
            1,
            "the combined merged size (~40 KiB) must trip exactly one hard-byte error",
        );
    }

    #[test]
    fn crlf_contents_normalize_to_lf_in_the_merged_output() {
        // C-DETERMINISM: `merge_and_check` normalizes line endings to `\n` in the final output, so
        // a CRLF-bearing body emits with LF only (determinism regardless of authoring platform).
        let items = vec![standalone(
            "skills/demo/SKILL.md",
            "line one\r\nline two\r\n",
            None,
            "claude-code",
        )];

        let (merged, _diags) = merge_and_check(items);

        let body = merged
            .get(Path::new("skills/demo/SKILL.md"))
            .expect("the standalone file must be present in the merged map");
        let text = String::from_utf8(body.clone()).expect("merged file is utf-8");
        assert!(
            !text.contains('\r'),
            "merge_and_check must normalize CRLF to LF; got carriage returns in: {text:?}",
        );
        assert_eq!(
            text, "line one\nline two\n",
            "CRLF must become LF byte-for-byte in the emitted output",
        );
    }

    #[test]
    fn path_separators_are_normalized_to_forward_slashes() {
        // C-DETERMINISM: emitted path keys use `/` separators regardless of host OS, so the merged
        // map is byte-identical across platforms. A spec authored with a nested path must key on
        // forward slashes in the returned map.
        let items = vec![standalone(
            "skills/demo/SKILL.md",
            "body",
            None,
            "claude-code",
        )];

        let (merged, _diags) = merge_and_check(items);

        let key_displays: Vec<String> = merged
            .keys()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(
            key_displays.iter().any(|k| k == "skills/demo/SKILL.md"),
            "merged path keys must use forward slashes; got: {key_displays:?}",
        );
    }
}

/// Asset placement follows the matrix skill layout, not a hardcoded `skills/<name>/` (Fix 6).
///
/// `copy_assets` itself does filesystem I/O, but the bug it had was purely in *where* it computed
/// the destination — so these tests pin the extracted [`super::skill_asset_dir`] path computation
/// directly. Before the fix every host's assets went to `skills/<name>/`; codex's `SKILL.md` lives
/// at `.agents/skills/<name>/SKILL.md`, so its assets were landing in the wrong directory.
#[cfg(test)]
mod asset_path_tests {
    use super::skill_asset_dir;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use weaft_core::capability;
    use weaft_core::ir::{Artifact, ArtifactMeta, Targets};
    use weaft_core::kind::ArtifactKind;
    use weaft_core::pipeline::resolve::resolve;

    fn skill(name: &str) -> Artifact {
        Artifact {
            kind: ArtifactKind::Skill,
            frontmatter: ArtifactMeta {
                name: name.to_string(),
                description: "d".to_string(),
                targets: Targets::default(),
                kind_override: None,
                fields: BTreeMap::new(),
            },
            body: String::new(),
            source_path: PathBuf::from(format!("skills/{name}.md")),
        }
    }

    #[test]
    fn claude_assets_land_in_the_skill_md_parent_dir() {
        let host = capability::by_id("claude-code").expect("claude-code host");
        let resolved = resolve(&skill("safe-deleter"), host);
        assert_eq!(
            skill_asset_dir(&resolved, "safe-deleter"),
            Path::new("skills/safe-deleter"),
            "claude assets must sit beside SKILL.md in `skills/<name>/`",
        );
    }

    #[test]
    fn codex_assets_follow_the_agents_skills_layout_not_a_hardcoded_skills_dir() {
        // The Fix-6 keystone: codex's SKILL.md is at `.agents/skills/<name>/SKILL.md`, so its
        // assets must follow into `.agents/skills/<name>/` — not the pre-fix hardcoded
        // `skills/<name>/` that applied claude's layout to every host.
        let host = capability::by_id("codex").expect("codex host");
        let resolved = resolve(&skill("safe-deleter"), host);
        assert_eq!(
            skill_asset_dir(&resolved, "safe-deleter"),
            Path::new(".agents/skills/safe-deleter"),
            "codex assets must follow the `.agents/skills/<name>/` matrix layout",
        );
    }
}
