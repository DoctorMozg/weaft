//! Parse a weaft project from disk into the [`Project`] IR.

use crate::diag::WeftError;
use crate::ir::{AgentMeta, Artifact, ArtifactMeta, Project, ProjectInfo};
use crate::kind::ArtifactKind;
use serde::de::DeserializeOwned;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// v1 per-kind frontmatter types, used only by the test-only `parse_skill`/`parse_agent`.
#[cfg(test)]
use crate::ir::{Agent, Skill, SkillMeta};

/// The project info file name.
const MANIFEST_NAME: &str = "weaft.yaml";

/// Load a whole project given a path to `weaft.yaml` (or a directory containing it).
pub fn load_project(manifest_path: &Path) -> Result<Project, WeftError> {
    let manifest = resolve_manifest(manifest_path)?;
    let root = manifest
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

    let src = read(&manifest)?;
    let info: ProjectInfo = parse_yaml(&src).map_err(|e| WeftError::Manifest {
        label: yaml_label(&e),
        span: yaml_span(&e, &src),
        src,
    })?;

    let mut artifacts = load_artifacts(&root)?;
    // Append manifest singletons after the deterministically-sorted file-backed artifacts. They
    // are synthesized only when the manifest declares them (Fix 6) so an undeclared config kind
    // never produces a synthetic artifact — and thus never a downstream drop-warning.
    artifacts.extend(singleton_artifacts(&info));

    Ok(Project {
        info,
        artifacts,
        root,
    })
}

/// Synthesize singleton config artifacts from the declared manifest fields (C-SOURCE-LAYOUT).
/// Each is built via [`Artifact::from_singleton`], which packs the opaque value under
/// [`crate::ir::SINGLETON_KEY`]. Conditional synthesis (only when declared) keeps undeclared
/// config kinds out of the pipeline entirely (Fix 6). `mcp_servers` is a `BTreeMap`, so its
/// iteration order is sorted by key — deterministic without extra sorting (C-DETERMINISM).
fn singleton_artifacts(info: &ProjectInfo) -> Vec<Artifact> {
    let mut singletons = Vec::new();

    // Aggregate every declared server into ONE McpServer artifact under the standard `mcpServers`
    // envelope — the host `.mcp.json` shape: a single document nesting each server under its name
    // key (`{ "mcpServers": { "<name>": <decl>, ... } }`). McpServer is Native only on claude-code
    // today, so the claude shape is the one emitted; a per-host MCP transform (e.g. Codex's TOML
    // `[mcp_servers.<name>]`) would be a future map-stage transform. One document is also why the
    // cell is merge:false — multiple artifacts would have concatenated into invalid JSON. The
    // `BTreeMap` iteration is key-sorted, so the envelope's order is deterministic (C-DETERMINISM).
    if !info.mcp_servers.is_empty() {
        let servers: serde_yaml_ng::Mapping = info
            .mcp_servers
            .iter()
            .map(|(name, value)| (serde_yaml_ng::Value::String(name.clone()), value.clone()))
            .collect();
        let mut envelope = serde_yaml_ng::Mapping::new();
        envelope.insert(
            serde_yaml_ng::Value::String("mcpServers".to_string()),
            serde_yaml_ng::Value::Mapping(servers),
        );
        singletons.push(Artifact::from_singleton(
            ArtifactKind::McpServer,
            "mcp",
            serde_yaml_ng::Value::Mapping(envelope),
        ));
    }

    if let Some(settings) = &info.settings {
        singletons.push(Artifact::from_singleton(
            ArtifactKind::Settings,
            "settings",
            settings.clone(),
        ));
    }

    if let Some(plugin) = &info.plugin {
        // Re-serialize the typed PluginIdentity back to an opaque value so the serialize stage
        // treats every singleton's payload uniformly. A serialize failure here is a logic bug
        // (PluginIdentity is a plain struct of serializable fields), not a user-input error.
        let value =
            serde_yaml_ng::to_value(plugin).expect("PluginIdentity serializes to a YAML value");
        singletons.push(Artifact::from_singleton(
            ArtifactKind::Plugin,
            &plugin.name,
            value,
        ));
    }

    if !info.ignore.is_empty() {
        let value = serde_yaml_ng::to_value(&info.ignore)
            .expect("ignore patterns serialize to a YAML sequence");
        singletons.push(Artifact::from_singleton(
            ArtifactKind::Ignore,
            "ignore",
            value,
        ));
    }

    singletons
}

/// Load every kind-folder under `root` into a single deterministically-ordered artifact list
/// (WU-8). The containing folder sets each artifact's kind via [`ArtifactKind::from_folder`];
/// a `kind:` frontmatter override then wins over the folder. Unknown top-level folders are not
/// folder-backed kinds, so they are simply skipped — never an error (C-SOURCE-LAYOUT).
fn load_artifacts(root: &Path) -> Result<Vec<Artifact>, WeftError> {
    let mut artifacts: Vec<Artifact> = Vec::new();
    for kind in ArtifactKind::all() {
        // Only the prose kinds are folder-backed; `as_str` is exactly the folder name for
        // those and round-trips through `from_folder` (the manifest-only kinds return None).
        let folder = kind.as_str();
        if ArtifactKind::from_folder(folder) != Some(*kind) {
            continue;
        }
        // Skill is the only two-mode kind (flat `skills/<name>.md` PLUS directory
        // `skills/<name>/SKILL.md`, ADR-0007); every other folder-backed kind stays flat-only.
        if *kind == ArtifactKind::Skill {
            artifacts.extend(load_skill_dir(&root.join(folder))?);
        } else {
            artifacts.extend(load_kind_dir(&root.join(folder), *kind)?);
        }
    }

    // Determinism (C-DETERMINISM): sort by (kind index, source path) so iteration order is
    // independent of the platform's read_dir ordering.
    artifacts.sort_by(|a, b| {
        a.kind
            .index()
            .cmp(&b.kind.index())
            .then_with(|| a.source_path.cmp(&b.source_path))
    });
    Ok(artifacts)
}

/// Load every `*.md` file in one kind-folder into [`Artifact`]s tagged with `folder_kind`,
/// honoring a per-file `kind:` override. A missing folder yields an empty list — a project
/// need not populate every kind.
fn load_kind_dir(dir: &Path, folder_kind: ArtifactKind) -> Result<Vec<Artifact>, WeftError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let paths = markdown_paths(dir)?;
    paths
        .into_iter()
        .map(|p| {
            let src = read(&p)?;
            parse_artifact(&p, &src, folder_kind)
        })
        .collect()
}

/// Load Skill-kind artifacts from `skills_dir` in both supported forms (ADR-0007): the flat
/// `skills/<name>.md` file and the directory `skills/<name>/SKILL.md`. Descent is exactly one
/// level — files deeper than a skill's own `SKILL.md` (e.g. `phases/*.md`) are never collected.
/// When both forms resolve to the same stem the directory form wins and the flat file is silently
/// excluded here (the `duplicate_skill` lint surfaces the coexistence). A missing `skills/`
/// directory yields an empty list — a project need not have skills. Both forms go through the same
/// [`parse_artifact`] path, so a `kind:` frontmatter override still reclassifies either form.
fn load_skill_dir(skills_dir: &Path) -> Result<Vec<Artifact>, WeftError> {
    if !skills_dir.is_dir() {
        return Ok(Vec::new());
    }

    // Directory form: each immediate subdirectory's `SKILL.md`, iff it exists as a file. The
    // subdirectory name is the skill's stem; collect those stems so the flat branch can drop any
    // file the directory form shadows.
    let mut artifacts: Vec<Artifact> = Vec::new();
    let mut dir_stems: BTreeSet<String> = BTreeSet::new();
    for subdir in subdirectories(skills_dir)? {
        let skill_md = subdir.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        if let Some(stem) = subdir.file_name().and_then(|s| s.to_str()) {
            dir_stems.insert(stem.to_string());
        }
        let src = read(&skill_md)?;
        artifacts.push(parse_artifact(&skill_md, &src, ArtifactKind::Skill)?);
    }

    // Flat form: every `*.md` directly inside `skills_dir`, minus a stray top-level `SKILL.md`
    // (not a directory skill) and minus any file whose stem the directory form already claims.
    for path in markdown_paths(skills_dir)? {
        if path.file_name() == Some(std::ffi::OsStr::new("SKILL.md")) {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if dir_stems.contains(stem) {
                continue;
            }
        }
        let src = read(&path)?;
        artifacts.push(parse_artifact(&path, &src, ArtifactKind::Skill)?);
    }

    Ok(artifacts)
}

/// Collect every immediate subdirectory of `dir` as an absolute, sorted path list. A missing
/// directory yields an empty vec (mirrors [`load_kind_dir`] / [`markdown_paths`] for an absent
/// folder); an unreadable existing directory is a [`WeftError::Read`].
fn subdirectories(dir: &Path) -> Result<Vec<PathBuf>, WeftError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|source| WeftError::Read {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    subdirs.sort();
    Ok(subdirs)
}

/// Collect every `*.md` path directly inside `dir`. Order is irrelevant here — the caller
/// sorts the merged artifact list deterministically — but errors on an unreadable directory.
fn markdown_paths(dir: &Path) -> Result<Vec<PathBuf>, WeftError> {
    Ok(std::fs::read_dir(dir)
        .map_err(|source| WeftError::Read {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect())
}

/// Parse one source file into a kind-tagged [`Artifact`]. The artifact's kind defaults to the
/// containing folder's `folder_kind`; a `kind:` frontmatter override (a closed [`ArtifactKind`]
/// value, so an out-of-set value is a parse error) reclassifies it (C-SOURCE-LAYOUT).
fn parse_artifact(
    path: &Path,
    src: &str,
    folder_kind: ArtifactKind,
) -> Result<Artifact, WeftError> {
    let (fm_src, body) = split_frontmatter(path, src)?;
    let frontmatter: ArtifactMeta = parse_yaml(fm_src).map_err(|e| WeftError::Frontmatter {
        path: path.to_path_buf(),
        label: yaml_label(&e),
        span: yaml_span(&e, fm_src),
        src: src.to_string(),
    })?;
    let kind = frontmatter.kind_override.unwrap_or(folder_kind);
    // A Subagent's typed fields (tools/model/readonly/is_background) are not deserialized until
    // render time (`ArtifactMeta::to_agent_meta`), where a type mismatch would panic. Validate the
    // frontmatter as an `AgentMeta` here so a mistyped value (`tools: Read`, `readonly: maybe`)
    // surfaces as a span-bearing frontmatter error at parse time instead of a compile-time panic.
    if kind == ArtifactKind::Subagent {
        parse_yaml::<AgentMeta>(fm_src).map_err(|e| WeftError::Frontmatter {
            path: path.to_path_buf(),
            label: yaml_label(&e),
            span: yaml_span(&e, fm_src),
            src: src.to_string(),
        })?;
    }
    Ok(Artifact {
        kind,
        frontmatter,
        body,
        source_path: path.to_path_buf(),
    })
}

/// Resolve a `--manifest-path` that may be the file itself or its directory.
fn resolve_manifest(path: &Path) -> Result<PathBuf, WeftError> {
    let candidate = if path.is_dir() {
        path.join(MANIFEST_NAME)
    } else {
        path.to_path_buf()
    };
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(WeftError::MissingManifest(candidate))
    }
}

// `parse_skill` / `parse_agent` are the v1 per-kind parsers, retained only for the unit tests
// that pin frontmatter parsing of the rich `SkillMeta`/`AgentMeta` shapes. The production path
// (`load_project`) now parses the generic `ArtifactMeta` via `parse_artifact`, so these are
// test-only.
#[cfg(test)]
fn parse_skill(path: &Path, src: &str) -> Result<Skill, WeftError> {
    let (fm_src, body) = split_frontmatter(path, src)?;
    let frontmatter: SkillMeta = parse_yaml(fm_src).map_err(|e| WeftError::Frontmatter {
        path: path.to_path_buf(),
        label: yaml_label(&e),
        span: yaml_span(&e, fm_src),
        src: src.to_string(),
    })?;
    Ok(Skill {
        frontmatter,
        body,
        source_path: path.to_path_buf(),
    })
}

#[cfg(test)]
fn parse_agent(path: &Path, src: &str) -> Result<Agent, WeftError> {
    let (fm_src, body) = split_frontmatter(path, src)?;
    let frontmatter: AgentMeta = parse_yaml(fm_src).map_err(|e| WeftError::Frontmatter {
        path: path.to_path_buf(),
        label: yaml_label(&e),
        span: yaml_span(&e, fm_src),
        src: src.to_string(),
    })?;
    Ok(Agent {
        frontmatter,
        body,
        source_path: path.to_path_buf(),
    })
}

/// Split a Markdown source into its leading `---`-fenced YAML frontmatter and the
/// remaining body. Returns `(frontmatter_yaml, body)`.
fn split_frontmatter<'a>(path: &Path, src: &'a str) -> Result<(&'a str, String), WeftError> {
    // Tolerate a leading BOM / whitespace-free `---\n` opener.
    let rest = src
        .strip_prefix("---\n")
        .or_else(|| src.strip_prefix("---\r\n"))
        .ok_or_else(|| WeftError::NoFrontmatter {
            path: path.to_path_buf(),
        })?;

    // Find the closing fence on its own line.
    let close = find_closing_fence(rest).ok_or_else(|| WeftError::NoFrontmatter {
        path: path.to_path_buf(),
    })?;

    let fm = &rest[..close.start];
    let body = rest[close.end..].trim_start_matches('\n').to_string();
    Ok((fm, body))
}

struct Fence {
    start: usize,
    end: usize,
}

/// Locate a closing `---` line within `s`.
fn find_closing_fence(s: &str) -> Option<Fence> {
    let mut offset = 0;
    for line in s.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            return Some(Fence {
                start: offset,
                end: offset + line.len(),
            });
        }
        offset += line.len();
    }
    None
}

fn parse_yaml<T: DeserializeOwned>(src: &str) -> Result<T, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(src)
}

fn read(path: &Path) -> Result<String, WeftError> {
    std::fs::read_to_string(path).map_err(|source| WeftError::Read {
        path: path.to_path_buf(),
        source,
    })
}

/// Build a `miette` source span from a `serde_yaml_ng` error location.
fn yaml_span(err: &serde_yaml_ng::Error, src: &str) -> miette::SourceSpan {
    match err.location() {
        Some(loc) => {
            let start = loc.index().min(src.len().saturating_sub(1));
            (start, 1).into()
        },
        None => (0usize, src.len().min(1)).into(),
    }
}

fn yaml_label(err: &serde_yaml_ng::Error) -> String {
    err.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn splits_frontmatter_and_body() {
        let src = "---\nname: x\ndescription: y\n---\n# Body\nhello\n";
        let (fm, body) = split_frontmatter(Path::new("x.md"), src).unwrap();
        assert_eq!(fm, "name: x\ndescription: y\n");
        assert_eq!(body, "# Body\nhello\n");
    }

    #[test]
    fn missing_frontmatter_errors() {
        let src = "# just a body\n";
        let err = split_frontmatter(Path::new("x.md"), src).unwrap_err();
        assert!(matches!(err, WeftError::NoFrontmatter { .. }));
    }

    #[test]
    fn parses_skill_meta() {
        let src = "---\nname: demo\ndescription: A demo skill.\n---\nBody {{ skill.name }}\n";
        let skill = parse_skill(Path::new("demo.md"), src).unwrap();
        assert_eq!(skill.frontmatter.name, "demo");
        assert_eq!(skill.body, "Body {{ skill.name }}\n");
    }

    #[test]
    fn parses_agent_meta_with_tools() {
        let src = "---\nname: rev\ndescription: Reviewer.\ntools: [Read, Grep]\nmodel: inherit\n---\nYou review code.\n";
        let agent = parse_agent(Path::new("rev.md"), src).unwrap();
        assert_eq!(agent.frontmatter.name, "rev");
        assert_eq!(agent.frontmatter.tools, vec!["Read", "Grep"]);
        assert_eq!(agent.frontmatter.model.as_deref(), Some("inherit"));
    }

    #[test]
    fn subagent_with_mistyped_tools_is_a_frontmatter_error() {
        // `tools` must be a sequence; a scalar is a type error. Validation moved to parse time, so
        // this surfaces as a span-bearing frontmatter error here rather than panicking later in
        // `ArtifactMeta::to_agent_meta` during compile.
        let src = "---\nname: rev\ndescription: Reviewer.\ntools: Read\n---\nBody.\n";
        let err =
            parse_artifact(Path::new("agents/rev.md"), src, ArtifactKind::Subagent).unwrap_err();
        assert!(
            matches!(err, WeftError::Frontmatter { .. }),
            "a mistyped subagent `tools` must be a frontmatter parse error, not a panic; got: {err:?}",
        );
    }

    #[test]
    fn valid_subagent_parses_through_parse_artifact() {
        // The added Subagent validation must not reject a well-formed agent: the happy path still
        // yields a Subagent artifact whose body is preserved.
        let src = "---\nname: rev\ndescription: Reviewer.\ntools: [Read, Grep]\n---\nYou review.\n";
        let artifact =
            parse_artifact(Path::new("agents/rev.md"), src, ArtifactKind::Subagent).unwrap();
        assert!(matches!(artifact.kind, ArtifactKind::Subagent));
        assert_eq!(artifact.frontmatter.name, "rev");
        assert_eq!(artifact.body, "You review.\n");
    }
}

/// WU-8: folder-by-kind parsing + per-file `kind:` frontmatter override (RED).
///
/// These tests drive the whole [`load_project`] path against a real on-disk project tree built
/// in a temp dir, exercising the v2 parser behavior authored in a later GREEN step:
/// - kind-folders beyond `skills/`/`agents/` (here `instructions/`) load via
///   [`crate::kind::ArtifactKind::from_folder`];
/// - a `kind:` frontmatter override reclassifies a file regardless of its folder;
/// - the final `artifacts` vec is sorted deterministically by `(kind.index(), source_path)`;
/// - an unknown top-level folder is ignored, not an error;
/// - a malformed `kind:` value is a parse error (closed-set serde rejection or the new
///   `InvalidKindOverride` diagnostic).
///
/// They are integration-style (build a project dir, call the public `load_project`) so they
/// assert observable behavior rather than the not-yet-final internal loader shape. The current
/// `load_project` only loads `skills/` + `agents/` and never honors a folder-derived kind or a
/// `kind:` override, so these fail today — the expected RED. The temp-dir scaffolding follows
/// the existing project convention (`std::env::temp_dir()` + a process-id tag; see
/// `crates/weaft-cli/tests/cli_tests.rs` / `fs.rs`), with a per-test sub-tag because several
/// tests in this binary share one PID.
#[cfg(test)]
mod v2_folder_kind_tests {
    use super::*;
    use crate::ir::Artifact;
    use crate::kind::ArtifactKind;
    use std::fs;
    use std::path::PathBuf;

    /// A fresh, empty project directory unique to `(tag, pid)`. Removed first so a re-run after a
    /// crash starts clean; the caller writes `weaft.yaml` + kind-folders into it.
    fn fresh_project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("weaft-parse-{}-{}", tag, std::process::id()));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).expect("create temp project dir");
        fs::write(dir.join(MANIFEST_NAME), "name: demo\nversion: 0.1.0\n")
            .expect("write weaft.yaml");
        dir
    }

    /// Write `relative/path.md` under `root`, creating parent folders as needed.
    fn write_file(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create kind folder");
        }
        fs::write(&path, contents).expect("write artifact file");
    }

    /// The single artifact named `name`, panicking if absent — keeps assertions terse.
    fn artifact_named<'a>(project: &'a Project, name: &str) -> &'a Artifact {
        project
            .artifacts
            .iter()
            .find(|a| a.frontmatter.name == name)
            .unwrap_or_else(|| panic!("no artifact named {name:?} in parsed project"))
    }

    #[test]
    fn instructions_folder_yields_instruction_kind_artifact() {
        // C-SOURCE-LAYOUT: a file under instructions/ must load as an Instruction-kind artifact,
        // not be ignored. The v1 parser only knows skills/ + agents/, so this is RED today.
        let root = fresh_project("instructions");
        write_file(
            &root,
            "instructions/overview.md",
            "---\nname: overview\ndescription: House rules.\n---\nAlways be careful.\n",
        );

        let project = load_project(&root).expect("project with instructions/ must load");
        let overview = artifact_named(&project, "overview");
        assert_eq!(
            overview.kind,
            ArtifactKind::Instruction,
            "a file under instructions/ must be classified Instruction (folder-by-kind)",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn kind_override_reclassifies_a_file_against_its_folder() {
        // C-SOURCE-LAYOUT: a `kind:` frontmatter override wins over the containing folder. A file
        // in skills/ declaring `kind: subagent` must end up Subagent-kind.
        let root = fresh_project("override");
        write_file(
            &root,
            "skills/actually-an-agent.md",
            "---\nname: actually-an-agent\ndescription: Lives in skills/ but is a subagent.\nkind: subagent\n---\nReview code.\n",
        );

        let project = load_project(&root).expect("project must load");
        let artifact = artifact_named(&project, "actually-an-agent");
        assert_eq!(
            artifact.kind,
            ArtifactKind::Subagent,
            "a `kind: subagent` override must reclassify a file living under skills/",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn artifacts_are_sorted_by_kind_index_then_path_deterministically() {
        // C-DETERMINISM: regardless of read_dir order, the artifacts vec is sorted by
        // (kind.index(), source_path). Instruction (index 0) precedes Skill (index 1); within a
        // kind, source paths sort lexically. Two skills authored with names that sort opposite to
        // their would-be insertion order pin the path-level tiebreak.
        let root = fresh_project("ordering");
        // Create in an order that is NOT the expected output order:
        // a later-sorting skill first, an instruction last.
        write_file(
            &root,
            "skills/zeta.md",
            "---\nname: zeta\ndescription: z.\n---\nZ body.\n",
        );
        write_file(
            &root,
            "skills/alpha.md",
            "---\nname: alpha\ndescription: a.\n---\nA body.\n",
        );
        write_file(
            &root,
            "instructions/house-rules.md",
            "---\nname: house-rules\ndescription: rules.\n---\nBe careful.\n",
        );

        let project = load_project(&root).expect("project must load");
        let order: Vec<(ArtifactKind, &str)> = project
            .artifacts
            .iter()
            .map(|a| (a.kind, a.frontmatter.name.as_str()))
            .collect();

        assert_eq!(
            order,
            [
                (ArtifactKind::Instruction, "house-rules"),
                (ArtifactKind::Skill, "alpha"),
                (ArtifactKind::Skill, "zeta"),
            ],
            "artifacts must be sorted by (kind.index(), source_path), not by read_dir order",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn unknown_top_level_folder_is_ignored_while_known_kind_folders_load() {
        // Two-part contract: (a) a folder that maps to no kind (e.g. random/) is silently skipped,
        // and (b) the v2 prose folders beyond skills/agents (here instructions/) DO load. Binding
        // both into one expected-set assertion means the test only passes once WU-8 loads
        // instructions/ AND ignores random/ — it is RED against the v1 loader (which loads
        // neither folder, so `house-rules` is absent).
        let root = fresh_project("unknown-folder");
        write_file(
            &root,
            "skills/real.md",
            "---\nname: real\ndescription: a real skill.\n---\nReal body.\n",
        );
        write_file(
            &root,
            "instructions/house-rules.md",
            "---\nname: house-rules\ndescription: must load.\n---\nRules.\n",
        );
        write_file(
            &root,
            "random/stray.md",
            "---\nname: stray\ndescription: should be ignored.\n---\nIgnored.\n",
        );

        let project = load_project(&root)
            .expect("an unknown top-level folder must be ignored, not fail the load");
        let mut names: Vec<&str> = project
            .artifacts
            .iter()
            .map(|a| a.frontmatter.name.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["house-rules", "real"],
            "instructions/ and skills/ must load; random/ must be ignored entirely",
        );
        assert!(
            !names.contains(&"stray"),
            "a file under an unknown folder (random/) must never be loaded",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn malformed_kind_override_is_a_parse_error() {
        // A `kind:` value outside the closed ArtifactKind set must fail the load (serde closed-set
        // rejection or the dedicated InvalidKindOverride diagnostic) — never silently default.
        let root = fresh_project("bad-kind");
        write_file(
            &root,
            "skills/broken.md",
            "---\nname: broken\ndescription: bad kind value.\nkind: wizard\n---\nBody.\n",
        );

        let result = load_project(&root);
        assert!(
            result.is_err(),
            "a malformed `kind: wizard` override must be a parse error, not a silent default",
        );

        drop(fs::remove_dir_all(&root));
    }
}

/// WU-9: manifest singleton synthesis — settings / mcp_servers / ignore / plugin (RED).
///
/// After parsing `weaft.yaml` info, [`load_project`] must synthesize singleton config artifacts
/// and append them to `project.artifacts` — but **only when the manifest actually declares them**
/// (Fix 6: absence means no synthetic artifact, so no later drop-warning flood):
/// - one `McpServer`-kind artifact aggregating every `mcp_servers` entry under one `mcpServers`
///   envelope (the host `.mcp.json` shape — every server nested under its name key);
/// - one `Settings` artifact iff `info.settings.is_some()`;
/// - one `Plugin` artifact iff `info.plugin.is_some()`;
/// - one `Ignore` artifact iff `!info.ignore.is_empty()`.
///
/// Each synthetic artifact packs its opaque value via `Artifact::from_singleton`, which stores it
/// in `frontmatter.fields["__singleton"]` (the WU-13 JSON serializer's read site).
///
/// These are integration-style: they author a real `weaft.yaml` (with at least one skill so the
/// project is valid) in a temp dir and call the public [`load_project`], asserting observable
/// behavior. Today's `load_project` never synthesizes singletons, so the "present" cases find no
/// artifacts of those kinds and fail — the expected RED. The "absent" case happens to pass at the
/// behavioral level but compiles and runs against the same harness; it is the Fix-6 pin that must
/// keep holding once GREEN lands. Temp-dir scaffolding follows the project convention
/// (`std::env::temp_dir()` + a process-id tag) used by `v2_folder_kind_tests` above, with a
/// per-test sub-tag since this binary's tests share one PID.
#[cfg(test)]
mod v2_singleton_tests {
    use super::*;
    use crate::ir::Artifact;
    use crate::kind::ArtifactKind;
    use std::fs;
    use std::path::PathBuf;

    /// The documented `frontmatter.fields` key under which `Artifact::from_singleton` packs the
    /// opaque singleton value (plan WU-9). Kept in lockstep with the IR-level test in `ir.rs`.
    const SINGLETON_KEY: &str = "__singleton";

    /// A fresh, empty project directory unique to `(tag, pid)`. Unlike `v2_folder_kind_tests`'
    /// `fresh_project`, this does NOT write a `weaft.yaml` — the singleton tests own the full
    /// manifest contents (they vary the `mcp_servers`/`settings`/… declarations). Removed first so
    /// a re-run after a crash starts clean.
    fn fresh_project_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("weaft-singleton-{}-{}", tag, std::process::id()));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).expect("create temp project dir");
        dir
    }

    /// Write `relative/path` under `root`, creating parent folders as needed.
    fn write_file(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent folder");
        }
        fs::write(&path, contents).expect("write file");
    }

    /// Write a full `weaft.yaml` plus one valid skill so the project loads (a project needs at
    /// least one artifact to be meaningful, and several tests assert "no *singleton* artifacts" —
    /// the skill keeps that distinct from "empty project").
    fn project_with_manifest(tag: &str, manifest_yaml: &str) -> PathBuf {
        let root = fresh_project_dir(tag);
        write_file(&root, MANIFEST_NAME, manifest_yaml);
        write_file(
            &root,
            "skills/keep.md",
            "---\nname: keep\ndescription: keeps the project non-empty.\n---\nBody.\n",
        );
        root
    }

    /// Every artifact of a given kind, in `project.artifacts` order.
    fn of_kind(project: &Project, kind: ArtifactKind) -> Vec<&Artifact> {
        project
            .artifacts
            .iter()
            .filter(|a| a.kind == kind)
            .collect()
    }

    #[test]
    fn mcp_servers_aggregate_into_one_artifact_under_the_mcp_servers_envelope() {
        // Correctness pin (was the per-entry bug): Claude Code's `.mcp.json` is ONE document with a
        // top-level `mcpServers` object nesting every server under its name key. So ALL declared
        // `mcp_servers` entries must aggregate into a SINGLE McpServer artifact whose `__singleton`
        // value is `{ mcpServers: { <name>: <decl>, ... } }` — never one artifact per entry (that
        // dropped the server name and concatenated invalid JSON). Two entries → one artifact whose
        // envelope carries both server keys (BTreeMap order: "fs" before "search").
        let manifest = "\
name: demo
version: 0.1.0
mcp_servers:
  fs:
    command: fs-server
  search:
    command: search-server
";
        let root = project_with_manifest("two-mcp", manifest);

        let project = load_project(&root).expect("manifest with mcp_servers must load");
        let mcp = of_kind(&project, ArtifactKind::McpServer);
        assert_eq!(
            mcp.len(),
            1,
            "all mcp_servers entries must aggregate into exactly ONE McpServer artifact",
        );

        let singleton = mcp[0].frontmatter.fields.get(SINGLETON_KEY).expect(
            "the aggregated McpServer artifact must carry its document under `__singleton`",
        );
        let servers = singleton
            .as_mapping()
            .and_then(|m| m.get(serde_yaml_ng::Value::String("mcpServers".to_string())))
            .and_then(serde_yaml_ng::Value::as_mapping)
            .expect("the singleton value must wrap the servers under a top-level `mcpServers` key");

        let names: Vec<&str> = servers
            .keys()
            .filter_map(serde_yaml_ng::Value::as_str)
            .collect();
        assert_eq!(
            names,
            ["fs", "search"],
            "the `mcpServers` envelope must contain every declared server keyed by its name",
        );

        // Each server's declaration must survive verbatim under its name (the value the JSON
        // serializer later frames as `{"mcpServers":{"fs":{...},"search":{...}}}`).
        let fs_decl = servers
            .get(serde_yaml_ng::Value::String("fs".to_string()))
            .and_then(serde_yaml_ng::Value::as_mapping)
            .expect("the `fs` server's declaration must be nested under its name key");
        assert_eq!(
            fs_decl
                .get(serde_yaml_ng::Value::String("command".to_string()))
                .and_then(serde_yaml_ng::Value::as_str),
            Some("fs-server"),
            "each server's declared fields must be preserved verbatim under its name key",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn absent_singletons_synthesize_no_artifacts() {
        // Fix-6 pin: a manifest declaring NONE of settings/mcp_servers/ignore/plugin must produce
        // ZERO synthetic artifacts of those four kinds — absence means no artifact, hence no later
        // drop-warning flood. Only the file-backed skill remains.
        let manifest = "name: demo\nversion: 0.1.0\n";
        let root = project_with_manifest("no-singletons", manifest);

        let project = load_project(&root).expect("bare manifest must load");
        for kind in [
            ArtifactKind::McpServer,
            ArtifactKind::Settings,
            ArtifactKind::Plugin,
            ArtifactKind::Ignore,
        ] {
            assert!(
                of_kind(&project, kind).is_empty(),
                "no {kind:?} artifact may be synthesized when the manifest does not declare it",
            );
        }
        // Sanity: the file-backed skill is still there, so "no singletons" is not "no artifacts".
        assert_eq!(
            of_kind(&project, ArtifactKind::Skill).len(),
            1,
            "the file-backed skill must still load alongside the (absent) singletons",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn settings_singleton_round_trips_into_one_artifact_under_documented_key() {
        // A `settings:` block synthesizes exactly one Settings artifact, and its packed value
        // (via from_singleton's `__singleton` storage key) equals the declared YAML mapping
        // byte-for-byte — the value the WU-13 JSON serializer will later emit as the document.
        let manifest = "\
name: demo
version: 0.1.0
settings:
  theme: dark
  telemetry: false
";
        let root = project_with_manifest("settings", manifest);

        let project = load_project(&root).expect("manifest with settings must load");
        let settings = of_kind(&project, ArtifactKind::Settings);
        assert_eq!(
            settings.len(),
            1,
            "a single `settings:` block must synthesize exactly one Settings artifact",
        );

        let packed = settings[0]
            .frontmatter
            .fields
            .get(SINGLETON_KEY)
            .expect("the Settings artifact must carry its value under the `__singleton` key");
        let expected: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("theme: dark\ntelemetry: false\n")
                .expect("expected settings YAML must parse");
        assert_eq!(
            packed, &expected,
            "the packed settings value must round-trip the declared YAML mapping exactly",
        );

        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn declared_plugin_and_ignore_each_synthesize_exactly_one_artifact() {
        // The remaining two conditional singletons: a `plugin:` block → exactly one Plugin
        // artifact; a non-empty `ignore:` list → exactly one Ignore artifact. Pinning both in one
        // test guards the `is_some()` / `!is_empty()` synthesis conditions together.
        let manifest = "\
name: demo
version: 0.1.0
ignore:
  - target/
  - .git/
plugin:
  name: demo-plugin
  version: 1.0.0
";
        let root = project_with_manifest("plugin-ignore", manifest);

        let project = load_project(&root).expect("manifest with plugin + ignore must load");
        assert_eq!(
            of_kind(&project, ArtifactKind::Plugin).len(),
            1,
            "a declared `plugin:` block must synthesize exactly one Plugin artifact",
        );
        assert_eq!(
            of_kind(&project, ArtifactKind::Ignore).len(),
            1,
            "a non-empty `ignore:` list must synthesize exactly one Ignore artifact",
        );

        drop(fs::remove_dir_all(&root));
    }
}

/// ADR-0007 WU-1: two-mode Skill source collection (flat `skills/<name>.md` PLUS directory
/// `skills/<name>/SKILL.md`) (RED).
///
/// These tests drive `load_skill_dir(skills_dir)` — the Skill-only collector authored in the
/// later GREEN step — directly against an on-disk `skills/` tree built in a temp dir. The
/// contract (ADR §1, §6, §8, §13):
/// - a `skills/<name>/SKILL.md` becomes one Skill-kind artifact whose `source_path` ends in
///   `SKILL.md`;
/// - a flat `skills/<name>.md` still loads (regression);
/// - when both forms resolve to the same stem the directory form wins and the flat file is
///   silently excluded (one artifact; the `duplicate_skill` lint surfaces the coexistence);
/// - descent is exactly one level — `skills/<name>/phases/*.md` are never collected;
/// - flat and directory skills with distinct stems coexist;
/// - a `kind:` frontmatter override still reclassifies a directory skill (shared `parse_artifact`).
///
/// `load_skill_dir` does not exist yet, so this module fails to compile with a missing-function
/// error — the expected RED. It must not be softened by stubbing the collector. The temp-dir
/// scaffolding follows the existing `v2_folder_kind_tests` convention (`std::env::temp_dir()` +
/// `process::id()` + a per-test sub-tag, since this binary's tests share one PID).
#[cfg(test)]
mod skill_dir_layout_tests {
    use super::*;
    use crate::ir::Artifact;
    use crate::kind::ArtifactKind;
    use std::ffi::OsStr;
    use std::fs;
    use std::path::PathBuf;

    /// A fresh, empty `skills/` directory unique to `(tag, pid)`. The whole project dir is removed
    /// first so a re-run after a crash starts clean; the caller writes the skill sources into the
    /// returned `skills/` path. Returns the `skills/` dir itself (the `load_skill_dir` argument).
    fn fresh_skills_dir(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("weaft-skilldir-{}-{}", tag, std::process::id()));
        drop(fs::remove_dir_all(&root));
        let skills = root.join("skills");
        fs::create_dir_all(&skills).expect("create temp skills dir");
        skills
    }

    /// Write `relative` under `skills_dir`, creating parent folders as needed.
    fn write_under(skills_dir: &Path, relative: &str, contents: &str) {
        let path = skills_dir.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create skill subfolder");
        }
        fs::write(&path, contents).expect("write skill source file");
    }

    /// Remove the whole temp project root (the parent of `skills_dir`).
    fn cleanup(skills_dir: &Path) {
        if let Some(root) = skills_dir.parent() {
            drop(fs::remove_dir_all(root));
        }
    }

    /// The single artifact named `name`, panicking if absent — keeps assertions terse.
    fn artifact_named<'a>(artifacts: &'a [Artifact], name: &str) -> &'a Artifact {
        artifacts
            .iter()
            .find(|a| a.frontmatter.name == name)
            .unwrap_or_else(|| panic!("no artifact named {name:?} in collected skills"))
    }

    #[test]
    fn directory_skill_loads_from_skill_md() {
        // ADR §1: a `skills/<name>/SKILL.md` must collect as exactly one Skill-kind artifact whose
        // `source_path` ends in `SKILL.md` and whose name comes from the SKILL.md frontmatter.
        let skills = fresh_skills_dir("dir-loads");
        write_under(
            &skills,
            "greeter/SKILL.md",
            "---\nname: greeter\ndescription: Says hello.\n---\nHello there.\n",
        );

        let artifacts = load_skill_dir(&skills).expect("a directory skill must load");
        assert_eq!(
            artifacts.len(),
            1,
            "exactly one artifact for one directory skill; got: {artifacts:?}",
        );
        let greeter = artifact_named(&artifacts, "greeter");
        assert_eq!(
            greeter.kind,
            ArtifactKind::Skill,
            "a directory skill must be Skill-kind",
        );
        assert_eq!(
            greeter.source_path.file_name(),
            Some(OsStr::new("SKILL.md")),
            "a directory skill's source_path must point at its SKILL.md",
        );

        cleanup(&skills);
    }

    #[test]
    fn flat_skill_still_loads() {
        // Regression (ADR §1): the existing flat form `skills/<name>.md` must still collect as a
        // Skill-kind artifact under the new two-mode collector.
        let skills = fresh_skills_dir("flat-loads");
        write_under(
            &skills,
            "safe.md",
            "---\nname: safe\ndescription: A flat skill.\n---\nBe careful.\n",
        );

        let artifacts = load_skill_dir(&skills).expect("a flat skill must still load");
        assert_eq!(
            artifacts.len(),
            1,
            "exactly one artifact for one flat skill; got: {artifacts:?}",
        );
        let safe = artifact_named(&artifacts, "safe");
        assert_eq!(
            safe.kind,
            ArtifactKind::Skill,
            "a flat skill must be Skill-kind",
        );

        cleanup(&skills);
    }

    #[test]
    fn directory_form_shadows_flat_form() {
        // ADR §6: when both `skills/dup.md` and `skills/dup/SKILL.md` exist, the directory form
        // wins — exactly one `dup` artifact, and its source_path is the SKILL.md (the flat file is
        // silently excluded here; the `duplicate_skill` lint surfaces the coexistence separately).
        let skills = fresh_skills_dir("shadow");
        write_under(
            &skills,
            "dup.md",
            "---\nname: dup\ndescription: the FLAT form.\n---\nFlat body.\n",
        );
        write_under(
            &skills,
            "dup/SKILL.md",
            "---\nname: dup\ndescription: the DIRECTORY form.\n---\nDirectory body.\n",
        );

        let artifacts = load_skill_dir(&skills).expect("coexisting forms must load");
        let dups: Vec<&Artifact> = artifacts
            .iter()
            .filter(|a| a.frontmatter.name == "dup")
            .collect();
        assert_eq!(
            dups.len(),
            1,
            "the directory form must shadow the flat form — exactly one `dup` artifact; got: {artifacts:?}",
        );
        assert_eq!(
            dups[0].source_path.file_name(),
            Some(OsStr::new("SKILL.md")),
            "the surviving `dup` artifact must be the directory form (source_path = SKILL.md)",
        );

        cleanup(&skills);
    }

    #[test]
    fn non_skill_md_files_in_skill_dir_are_not_artifacts() {
        // ADR §8: descent is one level only. A `skills/greeter/phases/research.md` (and any other
        // non-SKILL.md file inside the skill dir) must NOT be collected as an artifact — only the
        // skill's own SKILL.md is.
        let skills = fresh_skills_dir("one-level");
        write_under(
            &skills,
            "greeter/SKILL.md",
            "---\nname: greeter\ndescription: Says hello.\n---\n{% include \"phases/research.md\" %}\n",
        );
        write_under(
            &skills,
            "greeter/phases/research.md",
            "Phase research content.\n",
        );

        let artifacts = load_skill_dir(&skills).expect("a directory skill with phases must load");
        assert_eq!(
            artifacts.len(),
            1,
            "only the SKILL.md is an artifact; the phase file must not be collected; got: {artifacts:?}",
        );

        cleanup(&skills);
    }

    #[test]
    fn mixed_flat_and_directory_skills_coexist() {
        // A project may freely mix forms (ADR §2): a flat `skills/flat.md` and a directory
        // `skills/dir/SKILL.md` with distinct stems must both load — two artifacts.
        let skills = fresh_skills_dir("mixed");
        write_under(
            &skills,
            "flat.md",
            "---\nname: flat\ndescription: a flat skill.\n---\nFlat body.\n",
        );
        write_under(
            &skills,
            "dir/SKILL.md",
            "---\nname: dir\ndescription: a directory skill.\n---\nDir body.\n",
        );

        let artifacts = load_skill_dir(&skills).expect("mixed forms must load");
        assert_eq!(
            artifacts.len(),
            2,
            "a flat and a directory skill with distinct stems must both load; got: {artifacts:?}",
        );
        // Both forms must be present and Skill-kind.
        assert_eq!(artifact_named(&artifacts, "flat").kind, ArtifactKind::Skill);
        assert_eq!(artifact_named(&artifacts, "dir").kind, ArtifactKind::Skill);

        cleanup(&skills);
    }

    #[test]
    fn kind_override_still_applies_to_directory_skill() {
        // ADR §1: the directory form is parsed through the same `parse_artifact` path, so a `kind:`
        // frontmatter override must still reclassify it. A `skills/rev/SKILL.md` declaring
        // `kind: subagent` must end up Subagent-kind.
        let skills = fresh_skills_dir("dir-kind-override");
        write_under(
            &skills,
            "rev/SKILL.md",
            "---\nname: rev\ndescription: lives in skills/ but is a subagent.\nkind: subagent\n---\nReview code.\n",
        );

        let artifacts =
            load_skill_dir(&skills).expect("a directory skill with kind override must load");
        let rev = artifact_named(&artifacts, "rev");
        assert_eq!(
            rev.kind,
            ArtifactKind::Subagent,
            "a `kind: subagent` override on a directory SKILL.md must reclassify it to Subagent",
        );

        cleanup(&skills);
    }

    #[test]
    fn stray_top_level_skill_md_is_excluded_from_flat_set() {
        // A `SKILL.md` placed directly under `skills/` (not inside a `skills/<name>/` subdir) is
        // neither a valid flat skill nor a valid directory skill: there is no parent subdir named
        // like a skill, and the flat collector filters out paths whose file_name == "SKILL.md".
        // So it produces zero artifacts.
        let skills = fresh_skills_dir("stray-skill-md");
        write_under(
            &skills,
            "SKILL.md",
            "---\nname: stray\ndescription: a top-level SKILL.md, not a directory skill.\n---\nBody.\n",
        );

        let artifacts =
            load_skill_dir(&skills).expect("a stray top-level SKILL.md must load cleanly");
        assert_eq!(
            artifacts.len(),
            0,
            "a stray skills/SKILL.md is neither a flat nor a directory skill — zero artifacts; got: {artifacts:?}",
        );

        cleanup(&skills);
    }

    #[test]
    fn dot_named_skill_directory_does_not_crash() {
        // Regression: a skills/ subdirectory whose name ends in `.md` (e.g. `skills/v0.1/`,
        // `skills/foo.md/`) is a valid directory-skill layout. Before the file-type guard was
        // added to `markdown_paths`, the flat collector would try to `read` the directory and
        // crash with "Is a directory" (os error 21). With the fix it is filtered out from the
        // flat branch and collected correctly as a directory skill instead.
        let skills = fresh_skills_dir("dot-named-dir");
        let subdir = skills.join("foo.md");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(
            subdir.join("SKILL.md"),
            "---\nname: foo-dotmd\ndescription: directory skill with a dot in the folder name.\n---\nBody.\n",
        )
        .unwrap();

        let artifacts = load_skill_dir(&skills)
            .expect("a skills/foo.md/SKILL.md directory skill must load without crashing");
        assert_eq!(
            artifacts.len(),
            1,
            "expected exactly one artifact; got: {artifacts:?}"
        );
        assert_eq!(
            artifacts[0]
                .source_path
                .file_name()
                .and_then(|n| n.to_str()),
            Some("SKILL.md"),
            "source_path must be the SKILL.md file, not the directory",
        );

        cleanup(&skills);
    }
}
