//! WU-14: the pure `emit` stage — compute an artifact's output spec from the matrix layout.
//!
//! `emit` is the sixth stage of the seven-stage pipeline (parse → resolve → render → map →
//! serialize → **emit** → merge). It is a *pure* function of the resolved capability cell and
//! the already-framed body: it substitutes `{name}` into the cell's
//! [`crate::capability::Layout::path_template`] to compute the relative output path, copies the
//! `merge` flag off the layout, and carries the framed contents through unchanged.
//!
//! It performs **no byte check** — at this stage no file has been merged yet, so the final size
//! is unknown. The hard-byte enforcement lives downstream in `weaft-cli`'s `merge_and_check`
//! (WU-14, the merge stage), the only place that sees the merged bytes *and* core's budget data
//! (ADR-0005, Fix 2/3). `emit` must never hardcode a `format!`-built path; every path comes from
//! matrix data (C-CAPABILITY-MATRIX "MUST NOT hardcode file paths").
//!
//! [`EmittedSpec`]: the `{ relative_path, contents, merge }` output spec computed below.

use crate::pipeline::resolve::Resolved;
use std::path::PathBuf;

/// The output spec for one artifact: where it lands, what it contains, and whether it merges.
///
/// Produced by [`emit`] purely from the resolved capability cell's
/// [`crate::capability::Layout`] and the already-framed body. The downstream merge stage
/// (`weaft-cli`'s `merge_and_check`) concatenates specs whose `merge` is `true` and that share a
/// `relative_path`, and applies the hard-byte budget to the final merged bytes.
#[derive(Debug, Clone)]
pub struct EmittedSpec {
    /// Path relative to the target's output root, templated from the cell's layout.
    pub relative_path: PathBuf,
    /// The framed body, carried through from the serialize stage unchanged.
    pub contents: String,
    /// Whether artifacts sharing this path merge into one file (e.g. `AGENTS.md` sections).
    pub merge: bool,
}

/// The pure `emit` stage: compute an artifact's [`EmittedSpec`] from its resolved cell.
///
/// The relative path comes *only* from the cell's
/// [`crate::capability::Layout::path_template`] with `{name}` substituted by `name` — never a
/// hardcoded `format!` (C-CAPABILITY-MATRIX). The `merge` flag is copied off the layout, and the
/// `framed` body is carried through unchanged. This function is pure: no I/O and no byte check
/// (no file has been merged yet, so the final size is unknown — that enforcement lives downstream
/// in `weaft-cli`'s `merge_and_check`).
#[must_use]
pub fn emit(resolved: &Resolved, name: &str, framed: String) -> EmittedSpec {
    let layout = resolved.cell.layout;
    EmittedSpec {
        relative_path: PathBuf::from(layout.path_template.replace("{name}", name)),
        contents: framed,
        merge: layout.merge,
    }
}

/// WU-14: the pure `emit` stage (RED).
///
/// These tests pin the `emit` contract the GREEN coder must author:
///
/// - `pub fn emit(resolved: &Resolved, name: &str, framed: String) -> EmittedSpec`
/// - `pub struct EmittedSpec { pub relative_path: PathBuf, pub contents: String, pub merge: bool }`
///
/// The path is resolved purely from the cell's `path_template` (`{name}` substituted — no
/// hardcoded `format!`), `merge` is read off the layout, and `contents` is the framed body
/// carried through unchanged. The artifact *name* is passed explicitly (the GREEN signature may
/// instead take `&Artifact` and read `frontmatter.name` — if so, adjust the two `emit(...)`
/// call sites accordingly; the assertions on `EmittedSpec` stay identical).
///
/// Until WU-14 lands `emit` / `EmittedSpec`, this module fails to compile with missing-symbol
/// errors — the expected RED. It must not be softened by stubbing the production function.
#[cfg(test)]
mod tests {
    // RED: `emit` and `EmittedSpec` do not exist yet — these imports are the missing-symbol RED.
    use super::{EmittedSpec, emit};
    use crate::capability;
    use crate::ir::{Artifact, ArtifactMeta, Targets};
    use crate::kind::ArtifactKind;
    use crate::pipeline::resolve::{Resolved, resolve};
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    /// A bare file-backed artifact of `kind` named `name`. `emit` reads only the name (for path
    /// templating); the rest of the frontmatter is irrelevant to a pure path/spec computation.
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

    #[test]
    fn claude_skill_path_comes_from_the_matrix_template() {
        // A claude-code Skill named "demo" must emit to `skills/demo/SKILL.md` with merge=false
        // and the framed body carried through unchanged. The path is produced purely by
        // substituting `{name}` into the cell's `path_template` — no hardcoded `format!`.
        let host = capability::by_id("claude-code").expect("claude-code host must exist");
        let art = artifact(ArtifactKind::Skill, "demo");
        let resolved: Resolved = resolve(&art, host);

        let spec: EmittedSpec = emit(&resolved, &art.frontmatter.name, "framed".to_string());

        assert_eq!(
            spec.relative_path,
            Path::new("skills/demo/SKILL.md"),
            "claude Skill path must come from the `skills/{{name}}/SKILL.md` template",
        );
        assert!(
            !spec.merge,
            "a claude Skill is its own file (merge=false), not part of a merged document",
        );
        assert_eq!(
            spec.contents, "framed",
            "emit carries the framed body through unchanged (no reframing)",
        );
    }

    #[test]
    fn agents_md_skill_is_a_merged_agents_md_section() {
        // An agents-md Skill folds into the single merged `AGENTS.md`, so its path is `AGENTS.md`
        // and merge=true (multiple artifacts concatenate into that one file downstream).
        let host = capability::by_id("agents-md").expect("agents-md host must exist");
        let art = artifact(ArtifactKind::Skill, "demo");
        let resolved = resolve(&art, host);

        let spec = emit(&resolved, &art.frontmatter.name, "section body".to_string());

        assert_eq!(
            spec.relative_path,
            Path::new("AGENTS.md"),
            "agents-md Skill must emit to the merged AGENTS.md",
        );
        assert!(
            spec.merge,
            "agents-md Skill sections must merge into one AGENTS.md (merge=true)",
        );
    }

    #[test]
    fn path_template_substitutes_the_artifact_name_not_a_hardcoded_format() {
        // The `{name}` placeholder must be replaced by the *artifact's* name, proving the path is
        // templated from matrix data rather than a backend `format!("skills/{}/SKILL.md", ...)`.
        // Two different names through the same cell must yield two correspondingly different paths.
        let host = capability::by_id("claude-code").expect("claude-code host must exist");

        let alpha = artifact(ArtifactKind::Skill, "alpha-skill");
        let beta = artifact(ArtifactKind::Skill, "beta-skill");
        let r_alpha = resolve(&alpha, host);
        let r_beta = resolve(&beta, host);

        let s_alpha = emit(&r_alpha, &alpha.frontmatter.name, "a".to_string());
        let s_beta = emit(&r_beta, &beta.frontmatter.name, "b".to_string());

        assert_eq!(
            s_alpha.relative_path,
            Path::new("skills/alpha-skill/SKILL.md")
        );
        assert_eq!(
            s_beta.relative_path,
            Path::new("skills/beta-skill/SKILL.md")
        );
        assert_ne!(
            s_alpha.relative_path, s_beta.relative_path,
            "distinct artifact names must template to distinct paths (no hardcoded constant path)",
        );
        // And no stray `{name}` placeholder may survive substitution.
        assert!(
            !s_alpha.relative_path.to_string_lossy().contains("{name}"),
            "the `{{name}}` placeholder must be fully substituted",
        );
    }

    #[test]
    fn subagent_path_comes_from_the_matrix_template() {
        // A second native, non-merged kind to prove the templating is general: a claude-code
        // Subagent named "code-reviewer" emits to `agents/code-reviewer.md`, merge=false.
        let host = capability::by_id("claude-code").expect("claude-code host must exist");
        let art = artifact(ArtifactKind::Subagent, "code-reviewer");
        let resolved = resolve(&art, host);

        let spec = emit(&resolved, &art.frontmatter.name, "body".to_string());

        assert_eq!(
            spec.relative_path,
            Path::new("agents/code-reviewer.md"),
            "claude Subagent path must come from the `agents/{{name}}.md` template",
        );
        assert!(
            !spec.merge,
            "a claude Subagent is its own file (merge=false)"
        );
    }
}
