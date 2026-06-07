//! WU-11: the `resolve` stage — disposition (native / fold / drop) + effective-kind remap.
//!
//! `resolve` is the first new stage of the seven-stage pipeline (parse → **resolve** → render →
//! map → serialize → emit → merge). For one `(artifact, host)` pair it reads the host's
//! kind-scoped capability cell and decides what the host does with that kind:
//!
//! - [`Disposition::Native`] / [`Disposition::Drop`] — the effective kind is the artifact's own
//!   kind and the resolved cell is that kind's cell.
//! - [`Disposition::Fold`] — the kind folds into another kind's output, so the effective kind
//!   becomes the fold target and the resolved cell is the *target* kind's cell.
//!
//! It is a pure function of `(artifact.kind, host)` plus the capability matrix — no templates,
//! no I/O (C-PIPELINE). The downstream [`crate::pipeline::map`] stage reads the host id and the
//! resolved cell off the returned [`Resolved`], so both are carried through.
//!
//! Two entry points: [`resolve`] is infallible and assumes a well-formed matrix; [`resolve_checked`]
//! additionally enforces the ≤1-hop fold termination invariant (C-SUPPORT-DISPOSITION) and
//! returns a [`WeftError`] if a fold target itself folds.

use crate::capability::{Disposition, HostCapabilities, KindCapabilities};
use crate::diag::WeftError;
use crate::ir::Artifact;
use crate::kind::ArtifactKind;

/// The outcome of resolving one `(artifact, host)` pair, consumed by the render/map/emit
/// stages.
///
/// The `'a` lifetime ties the borrowed [`KindCapabilities`] `cell` to the host it was resolved
/// against. Shipped host consts are `'static`, but the resolver also accepts a temporary host
/// (so fold logic can be exercised against a synthetic matrix), so the cell borrow is generic
/// rather than `'static`.
#[derive(Debug, Clone, Copy)]
pub struct Resolved<'a> {
    /// The artifact's own kind, carried through unchanged.
    pub artifact_kind: ArtifactKind,
    /// The kind whose output this artifact contributes to. Equals `artifact_kind` for
    /// `Native`/`Drop`; for a fold it is the fold target kind.
    pub effective_kind: ArtifactKind,
    /// What the host does with this artifact's kind.
    pub disposition: Disposition,
    /// The capability cell that governs emission. For a fold this is the *target* kind's cell
    /// (the one that actually emits), not the folded source cell.
    pub cell: &'a KindCapabilities,
    /// The id of the host this was resolved against (e.g. `"claude-code"`). The `map` stage
    /// needs it to read `FieldSource::TargetOverride` blocks via `Targets::override_for`.
    pub host_id: &'static str,
}

/// Resolve one `(artifact, host)` pair into a [`Resolved`], assuming a well-formed matrix.
///
/// For `Native`/`Drop` the effective kind is the artifact's own kind and the resolved cell is
/// that kind's cell. For `Fold { into }` the effective kind becomes `into` and the resolved
/// cell is the fold *target* cell. This is the infallible happy path; the termination invariant
/// is checked only by [`resolve_checked`].
#[must_use]
pub fn resolve<'a>(artifact: &Artifact, host: &'a HostCapabilities) -> Resolved<'a> {
    let cell = host.kinds.get(artifact.kind);
    match cell.disposition {
        Disposition::Native | Disposition::Drop => Resolved {
            artifact_kind: artifact.kind,
            effective_kind: artifact.kind,
            disposition: cell.disposition,
            cell,
            host_id: host.id,
        },
        Disposition::Fold { into } => Resolved {
            artifact_kind: artifact.kind,
            effective_kind: into,
            disposition: cell.disposition,
            cell: host.kinds.get(into),
            host_id: host.id,
        },
    }
}

/// Resolve one `(artifact, host)` pair, additionally enforcing the ≤1-hop fold termination
/// invariant (C-SUPPORT-DISPOSITION): a fold target must itself be `Native`, never another
/// fold (or a drop). The WU-6 matrix tests guard this statically; this is the defensive runtime
/// guard for a matrix-authoring slip, returning a hard [`WeftError`] rather than chasing the
/// chain.
pub fn resolve_checked<'a>(
    artifact: &Artifact,
    host: &'a HostCapabilities,
) -> Result<Resolved<'a>, WeftError> {
    let source_cell = host.kinds.get(artifact.kind);
    if let Disposition::Fold { into } = source_cell.disposition {
        let target = host.kinds.get(into);
        if !matches!(target.disposition, Disposition::Native) {
            return Err(WeftError::FoldChainTooDeep {
                host: host.id,
                from: artifact.kind.serde_name(),
                into: into.serde_name(),
            });
        }
    }
    Ok(resolve(artifact, host))
}

#[cfg(test)]
mod tests {
    use crate::capability::{
        self, Disposition, FieldMap, HostCapabilities, KindCapabilities, KindCapabilitiesTable,
        Layout,
    };
    use crate::ir::{Artifact, ArtifactMeta, Targets};
    use crate::kind::ArtifactKind;
    use crate::serfmt::SerFormat;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    // The two functions under test (authored in GREEN): the pure happy path returns `Resolved`
    // directly; the checked path enforces the ≤1-hop fold termination invariant and returns a
    // `WeftError` when a fold target itself folds (defensive guard, plan WU-11 step 2).
    use crate::pipeline::resolve::{Resolved, resolve, resolve_checked};

    /// A bare file-backed artifact of `kind` named `name`, no targets/fields. The `map` stage is
    /// what reads fields; `resolve` is a pure function of `(artifact.kind, host)`, so the
    /// frontmatter contents are irrelevant to it.
    fn artifact(kind: ArtifactKind, name: &str) -> Artifact {
        Artifact {
            kind,
            frontmatter: ArtifactMeta {
                name: name.to_string(),
                description: format!("{name} description"),
                targets: Targets::default(),
                kind_override: None,
                fields: BTreeMap::new(),
            },
            body: String::new(),
            source_path: PathBuf::from(format!("{}/{name}.md", kind.as_str())),
        }
    }

    /// A `Native` placeholder cell for `kind` — a synthetic fold *target* the resolver must
    /// land on. Layout/format are inert; only the disposition and kind are read by `resolve`.
    fn native_cell(kind: ArtifactKind) -> KindCapabilities {
        KindCapabilities {
            kind,
            disposition: Disposition::Native,
            layout: Layout {
                path_template: "skills/{name}/SKILL.md",
                merge: false,
            },
            format: SerFormat::YamlFrontmatterMarkdown,
            frontmatter_dialect: None,
            field_map: FieldMap(&[]),
            budget: None,
        }
    }

    /// A cell for `kind` that folds into `into` (synthetic — the real matrix has no fold cells).
    fn fold_cell(kind: ArtifactKind, into: ArtifactKind) -> KindCapabilities {
        KindCapabilities {
            kind,
            disposition: Disposition::Fold { into },
            layout: Layout {
                path_template: "",
                merge: true,
            },
            format: SerFormat::PlainMarkdown,
            frontmatter_dialect: None,
            field_map: FieldMap(&[]),
            budget: None,
        }
    }

    /// Clone a real host const and overwrite individual kind cells, so a synthetic fold matrix
    /// can be fed to the *real* `resolve` function (the only honest way to exercise fold logic,
    /// since no shipped host currently folds). `HostCapabilities` is `Clone`.
    fn host_with_cells(
        base: &HostCapabilities,
        overrides: &[KindCapabilities],
    ) -> HostCapabilities {
        let mut host = base.clone();
        let mut cells = host.kinds.0;
        for cell in overrides {
            cells[cell.kind.index()] = *cell;
        }
        host.kinds = KindCapabilitiesTable(cells);
        host
    }

    #[test]
    fn claude_code_subagent_resolves_native_with_subagent_effective_kind() {
        // A real-matrix native cell: claude-code represents subagents natively, so a Subagent
        // artifact resolves Native and keeps Subagent as its effective (emitted) kind.
        let claude = capability::by_id("claude-code").expect("claude-code host must exist");
        let art = artifact(ArtifactKind::Subagent, "code-reviewer");

        let resolved: Resolved = resolve(&art, claude);

        assert_eq!(
            resolved.disposition,
            Disposition::Native,
            "claude-code Subagent must resolve Native",
        );
        assert_eq!(
            resolved.effective_kind,
            ArtifactKind::Subagent,
            "a native Subagent keeps Subagent as its effective kind",
        );
        assert_eq!(
            resolved.artifact_kind,
            ArtifactKind::Subagent,
            "the original artifact kind is carried through unchanged",
        );
        // The resolved cell must be the Subagent cell on this host (Native, not a fold target).
        assert_eq!(resolved.cell.kind, ArtifactKind::Subagent);
        assert_eq!(resolved.cell.disposition, Disposition::Native);
    }

    #[test]
    fn agents_md_subagent_resolves_drop() {
        // agents-md has no subagent primitive, so a Subagent artifact resolves Drop (the driver
        // and lint turn this into a warning rather than a file).
        let agents_md = capability::by_id("agents-md").expect("agents-md host must exist");
        let art = artifact(ArtifactKind::Subagent, "code-reviewer");

        let resolved = resolve(&art, agents_md);

        assert_eq!(
            resolved.disposition,
            Disposition::Drop,
            "agents-md cannot represent subagents — resolve must yield Drop",
        );
    }

    #[test]
    fn claude_code_settings_resolves_native_no_drop() {
        // Fix 6: a declared, natively-supported singleton (claude-code Settings) resolves Native
        // — it must NOT drop. This is the regression guard against the drop-warning flood.
        let claude = capability::by_id("claude-code").expect("claude-code host must exist");
        let art = artifact(ArtifactKind::Settings, "settings");

        let resolved = resolve(&art, claude);

        assert_eq!(
            resolved.disposition,
            Disposition::Native,
            "claude-code Settings is Native (a real emitted file, not a drop) — Fix 6",
        );
        assert_eq!(resolved.effective_kind, ArtifactKind::Settings);
    }

    #[test]
    fn fold_remaps_effective_kind_to_the_fold_target_cell() {
        // A `Fold { into: Skill }` cell must resolve with effective_kind == Skill and `cell`
        // pointing at the *target* kind's (Skill) cell, not the folded source cell. The real
        // matrix ships no folds, so synthesize one over a cloned host (Instruction folds into
        // the host's native Skill cell).
        let claude = capability::by_id("claude-code").expect("claude-code host must exist");
        let folded = host_with_cells(
            claude,
            &[
                fold_cell(ArtifactKind::Instruction, ArtifactKind::Skill),
                native_cell(ArtifactKind::Skill),
            ],
        );
        let art = artifact(ArtifactKind::Instruction, "house-rules");

        let resolved = resolve(&art, &folded);

        assert_eq!(
            resolved.artifact_kind,
            ArtifactKind::Instruction,
            "the source artifact kind is still Instruction",
        );
        assert_eq!(
            resolved.effective_kind,
            ArtifactKind::Skill,
            "a fold remaps the effective kind to the fold target",
        );
        assert_eq!(
            resolved.cell.kind,
            ArtifactKind::Skill,
            "the resolved cell must be the fold *target* (Skill) cell, not the Instruction cell",
        );
        assert_eq!(
            resolved.cell.disposition,
            Disposition::Native,
            "the fold target cell is Native (a real emit), per the ≤1-hop invariant",
        );
    }

    #[test]
    fn two_hop_fold_is_a_hard_error_via_the_checked_path() {
        // C-SUPPORT-DISPOSITION termination invariant (defensive): if a fold target itself
        // folds (Instruction → Skill → Subagent), `resolve_checked` must return a hard
        // `WeftError` rather than chase the chain. Construct the 2-hop matrix in-test.
        let claude = capability::by_id("claude-code").expect("claude-code host must exist");
        let two_hop = host_with_cells(
            claude,
            &[
                fold_cell(ArtifactKind::Instruction, ArtifactKind::Skill),
                fold_cell(ArtifactKind::Skill, ArtifactKind::Subagent),
            ],
        );
        let art = artifact(ArtifactKind::Instruction, "house-rules");

        let result = resolve_checked(&art, &two_hop);

        assert!(
            result.is_err(),
            "a two-hop fold (fold target that itself folds) must be a hard error, not silently \
             chased (≤1-hop termination guard)",
        );
    }

    #[test]
    fn checked_resolve_agrees_with_resolve_on_well_formed_single_hop_folds() {
        // The checked path must accept a legal ≤1-hop fold and return the same resolution as the
        // infallible `resolve` — the guard only fires on the pathological 2-hop case above.
        let claude = capability::by_id("claude-code").expect("claude-code host must exist");
        let folded = host_with_cells(
            claude,
            &[
                fold_cell(ArtifactKind::Instruction, ArtifactKind::Skill),
                native_cell(ArtifactKind::Skill),
            ],
        );
        let art = artifact(ArtifactKind::Instruction, "house-rules");

        let checked = resolve_checked(&art, &folded)
            .expect("a well-formed single-hop fold must pass the termination guard");
        assert_eq!(checked.effective_kind, ArtifactKind::Skill);
        assert_eq!(checked.cell.kind, ArtifactKind::Skill);
    }
}
