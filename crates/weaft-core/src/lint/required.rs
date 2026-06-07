//! Required-field lint: the project and every *prose* artifact must carry non-empty identifying
//! metadata. Hard errors.
//!
//! WU-19 iterates the generic `project.artifacts` and is **kind-aware**: prose kinds
//! (instructions / skills / subagents / commands / hooks) require a non-empty `name` and
//! `description`, while manifest-only config singletons (settings / mcp_server / plugin / ignore)
//! are exempt — their payload is the document, so an empty description is legitimate (permissive
//! per-kind validity, C-OPEN-QUESTIONS).

use crate::diag::Diagnostic;
use crate::ir::{Artifact, Project};
use crate::kind::ArtifactKind;

const CODE: &str = "weaft::lint::required";

pub fn check(project: &Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    if project.info.name.trim().is_empty() {
        out.push(Diagnostic::error(
            CODE,
            "project `name` is empty in weaft.yaml",
        ));
    }
    if project.info.version.trim().is_empty() {
        out.push(Diagnostic::error(
            CODE,
            "project `version` is empty in weaft.yaml",
        ));
    }

    for artifact in &project.artifacts {
        if is_prose(artifact.kind) {
            check_prose(artifact, &mut out);
        }
    }

    out
}

/// Does this kind carry author-facing prose metadata that hosts route on? The five folder-backed
/// kinds do; the four manifest-only singleton kinds (their payload *is* the document) do not.
fn is_prose(kind: ArtifactKind) -> bool {
    matches!(
        kind,
        ArtifactKind::Instruction
            | ArtifactKind::Skill
            | ArtifactKind::Subagent
            | ArtifactKind::Command
            | ArtifactKind::Hook
    )
}

/// Require a non-empty `name`/`description` on one prose artifact. The diagnostic names the kind
/// (`skill`/`subagent`/…) so the existing skill/subagent messages are preserved verbatim.
fn check_prose(artifact: &Artifact, out: &mut Vec<Diagnostic>) {
    let kind = artifact.kind.serde_name();
    if artifact.frontmatter.name.trim().is_empty() {
        out.push(
            Diagnostic::error(CODE, format!("{kind} `name` is empty"))
                .with_artifact(artifact.source_path.display().to_string()),
        );
    }
    if artifact.frontmatter.description.trim().is_empty() {
        out.push(
            Diagnostic::error(CODE, format!("{kind} `description` is empty"))
                .with_artifact(artifact.frontmatter.name.clone())
                .with_help("a description is required so hosts can route to the artifact"),
        );
    }
}

/// WU-19: kind-aware required-field lint (RED).
///
/// WU-19 makes `check` iterate `project.artifacts` and require non-empty `name`/`description` on
/// **prose** kinds (skills/subagents/…) while **skipping config singletons** — per-kind validity
/// stays permissive (C-OPEN-QUESTIONS): a `Settings` singleton legitimately has an empty
/// description (its payload is the document), so it must NOT be flagged.
///
/// RED categories:
/// - **Behavioral (kind-aware skip)**: a `Settings` singleton with an empty description, when
///   present in `project.artifacts`, must produce no required-field error. Today `check` only
///   iterates `project.skills()`/`agents()`, so the singleton is invisible to it — this half is a
///   regression guard (it must stay clean after WU-19 starts iterating all artifacts; a naive
///   "iterate everything and require description" would flag the singleton and fail here).
/// - **Regression**: an empty-description skill is still a hard error (the existing contract,
///   preserved across the iteration switch).
///
/// Both halves are asserted on the SAME project so the test pins that WU-19's generic iteration
/// flags the prose artifact while exempting the config singleton — exactly the kind-aware behavior.
#[cfg(test)]
mod wu19_kind_aware_required_tests {
    use super::{CODE, check};
    use crate::ir::{Artifact, ArtifactMeta, Meta, Project, ProjectInfo, Targets};
    use crate::kind::ArtifactKind;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// A file-backed prose artifact of `kind` with the supplied name/description (either may be
    /// empty to exercise the required-field rule).
    fn prose(kind: ArtifactKind, name: &str, description: &str) -> Artifact {
        Artifact {
            kind,
            frontmatter: ArtifactMeta {
                name: name.into(),
                description: description.into(),
                targets: Targets::default(),
                kind_override: None,
                fields: BTreeMap::new(),
            },
            body: "body".into(),
            source_path: PathBuf::from(format!("{}/{name}.md", kind.as_str())),
        }
    }

    fn project(artifacts: Vec<Artifact>) -> Project {
        Project {
            info: ProjectInfo {
                name: "p".into(),
                version: "0.1.0".into(),
                description: String::new(),
                meta: Meta::default(),
                targets: Targets::default(),
                parameters: BTreeMap::new(),
                settings: None,
                mcp_servers: BTreeMap::new(),
                ignore: Vec::new(),
                plugin: None,
            },
            artifacts,
            root: PathBuf::from("/weaft-nonexistent-root"),
        }
    }

    /// Required-field errors that concern the named artifact.
    fn required_errors_for<'a>(diags: &'a [crate::diag::Diagnostic], name: &str) -> Vec<&'a str> {
        diags
            .iter()
            .filter(|d| d.code == CODE && d.is_error())
            .filter(|d| {
                d.artifact.as_deref().is_some_and(|a| a.contains(name)) || d.message.contains(name)
            })
            .map(|d| d.message.as_str())
            .collect()
    }

    #[test]
    fn empty_skill_description_is_flagged_but_a_settings_singleton_is_exempt() {
        // Kind-aware required-field lint on one mixed project:
        //   - the Skill with an empty description IS a hard error (regression-preserving);
        //   - the Settings singleton with an empty description is NOT flagged (permissive config).
        let settings = Artifact::from_singleton(
            ArtifactKind::Settings,
            "settings",
            serde_yaml_ng::Value::Null,
        );
        let p = project(vec![prose(ArtifactKind::Skill, "greet", ""), settings]);
        let diags = check(&p);

        assert!(
            !required_errors_for(&diags, "greet").is_empty(),
            "an empty skill description must be a hard required-field error; got: {diags:?}",
        );
        assert!(
            required_errors_for(&diags, "settings").is_empty(),
            "a config singleton with an empty description must be exempt from the required-field \
             lint (permissive per-kind validity); got: {diags:?}",
        );
    }

    #[test]
    fn a_settings_singleton_alone_produces_no_required_field_error() {
        // Isolation: a project whose only non-skill artifact is a Settings singleton (empty name
        // is the from_singleton-supplied name; empty description) must yield zero required-field
        // errors — the singleton is config, not prose. A naive WU-19 that requires fields on every
        // artifact would flag the singleton's empty description and fail this.
        let settings = Artifact::from_singleton(
            ArtifactKind::Settings,
            "settings",
            serde_yaml_ng::Value::Null,
        );
        let p = project(vec![settings]);
        let diags = check(&p);

        assert!(
            diags.iter().all(|d| d.code != CODE),
            "a lone config singleton must not trip the required-field lint; got: {diags:?}",
        );
    }
}
