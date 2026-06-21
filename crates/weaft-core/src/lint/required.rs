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
use std::collections::HashMap;

const CODE: &str = "weaft::lint::required";
const DUPLICATE: &str = "weaft::lint::duplicate_name";
const NAME_FORMAT: &str = "weaft::lint::invalid_name";

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

    let mut seen: HashMap<(ArtifactKind, &str), &str> = HashMap::new();
    for artifact in &project.artifacts {
        if is_prose(artifact.kind) {
            check_prose(artifact, &mut out);
            check_name_format(artifact, &mut out);
        }
        // Duplicate (kind, name) check applies to all artifact kinds: the pipeline uses name as
        // the primary key for a kind (e.g. two skills named "greet" would overwrite each other).
        let key = (artifact.kind, artifact.frontmatter.name.as_str());
        if let Some(first_path) = seen.get(&key) {
            out.push(
                Diagnostic::error(
                    DUPLICATE,
                    format!(
                        "{} `{}` is declared more than once",
                        artifact.kind.serde_name(),
                        artifact.frontmatter.name,
                    ),
                )
                .with_artifact(artifact.frontmatter.name.clone())
                .with_help(format!("first declaration at {first_path}")),
            );
        } else {
            seen.insert(key, artifact.source_path.to_str().unwrap_or("<unknown>"));
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

/// Artifact names must match `[a-z0-9][a-z0-9._-]*`: lowercase alphanumeric start, then any
/// combination of lowercase alphanumeric, `.`, `_`, `-`. No path separators or whitespace.
/// The pattern mirrors file-system–safe slug conventions used by every host.
fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

/// Flag a prose artifact whose `name` violates the slug convention.
fn check_name_format(artifact: &Artifact, out: &mut Vec<Diagnostic>) {
    let name = &artifact.frontmatter.name;
    if !name.trim().is_empty() && !is_valid_name(name) {
        out.push(
            Diagnostic::error(
                NAME_FORMAT,
                format!(
                    "{} name `{name}` is not a valid slug",
                    artifact.kind.serde_name(),
                ),
            )
            .with_artifact(artifact.source_path.display().to_string())
            .with_help(
                "names must match [a-z0-9][a-z0-9._-]* (lowercase, no spaces or path separators)",
            ),
        );
    }
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

#[cfg(test)]
mod duplicate_name_tests {
    use super::{DUPLICATE, check};
    use crate::ir::{Artifact, ArtifactMeta, Meta, Project, ProjectInfo, Targets};
    use crate::kind::ArtifactKind;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn named(kind: ArtifactKind, name: &str) -> Artifact {
        Artifact {
            kind,
            frontmatter: ArtifactMeta {
                name: name.into(),
                description: "d".into(),
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

    #[test]
    fn two_skills_with_same_name_is_an_error() {
        let p = project(vec![
            named(ArtifactKind::Skill, "greet"),
            named(ArtifactKind::Skill, "greet"),
        ]);
        let diags = check(&p);

        assert!(
            diags
                .iter()
                .any(|d| d.code == DUPLICATE && d.message.contains("greet")),
            "two skills with the same name must produce a DUPLICATE error; got: {diags:?}",
        );
    }

    #[test]
    fn same_name_different_kinds_is_not_a_duplicate() {
        // A skill and a subagent may share a name — they occupy different kind namespaces.
        let p = project(vec![
            named(ArtifactKind::Skill, "greet"),
            named(ArtifactKind::Subagent, "greet"),
        ]);
        let diags = check(&p);

        assert!(
            diags.iter().all(|d| d.code != DUPLICATE),
            "a skill and a subagent sharing a name must not be flagged as duplicate; got: {diags:?}",
        );
    }

    #[test]
    fn unique_names_produce_no_duplicate_error() {
        let p = project(vec![
            named(ArtifactKind::Skill, "greet"),
            named(ArtifactKind::Skill, "farewell"),
        ]);
        let diags = check(&p);

        assert!(
            diags.iter().all(|d| d.code != DUPLICATE),
            "skills with distinct names must not produce a DUPLICATE error; got: {diags:?}",
        );
    }
}

#[cfg(test)]
mod name_format_tests {
    use super::{NAME_FORMAT, check, is_valid_name};
    use crate::ir::{Artifact, ArtifactMeta, Meta, Project, ProjectInfo, Targets};
    use crate::kind::ArtifactKind;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn skill_named(name: &str) -> Artifact {
        Artifact {
            kind: ArtifactKind::Skill,
            frontmatter: ArtifactMeta {
                name: name.into(),
                description: "d".into(),
                targets: Targets::default(),
                kind_override: None,
                fields: BTreeMap::new(),
            },
            body: "body".into(),
            source_path: PathBuf::from(format!("skills/{name}.md")),
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

    #[test]
    fn valid_slug_names_pass() {
        for name in [
            "a",
            "z9",
            "my-skill",
            "safe_deleter",
            "foo.bar",
            "code-review-v2",
        ] {
            assert!(is_valid_name(name), "`{name}` should be a valid slug");
        }
    }

    #[test]
    fn invalid_slug_names_fail() {
        for name in [
            "My-Skill", // uppercase
            "my skill", // space
            "../evil",  // path traversal
            "-start",   // starts with dash
            ".start",   // starts with dot
            "",         // empty
            "UPPER",    // uppercase
            "a/b",      // slash
        ] {
            assert!(!is_valid_name(name), "`{name}` should fail slug validation");
        }
    }

    #[test]
    fn skill_with_uppercase_name_produces_invalid_name_error() {
        let p = project(vec![skill_named("MySkill")]);
        let diags = check(&p);

        assert!(
            diags
                .iter()
                .any(|d| d.code == NAME_FORMAT && d.message.contains("MySkill")),
            "a skill with an uppercase name must produce NAME_FORMAT error; got: {diags:?}",
        );
    }

    #[test]
    fn skill_with_valid_name_produces_no_name_format_error() {
        let p = project(vec![skill_named("my-skill")]);
        let diags = check(&p);

        assert!(
            diags.iter().all(|d| d.code != NAME_FORMAT),
            "a skill with a valid slug must not produce NAME_FORMAT error; got: {diags:?}",
        );
    }
}
