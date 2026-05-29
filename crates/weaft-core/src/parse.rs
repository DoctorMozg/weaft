//! Parse a weaft project from disk into the [`Project`] IR.

use crate::diag::WeftError;
use crate::ir::{Agent, AgentMeta, Project, ProjectInfo, Skill, SkillMeta};
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

/// The project info file name.
pub const MANIFEST_NAME: &str = "weaft.yaml";

/// Load a whole project given a path to `weaft.yaml` (or a directory containing it).
pub fn load_project(manifest_path: &Path) -> Result<Project, WeftError> {
    let manifest = resolve_manifest(manifest_path)?;
    let root = manifest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let src = read(&manifest)?;
    let info: ProjectInfo = parse_yaml(&src).map_err(|e| WeftError::Manifest {
        label: yaml_label(&e),
        span: yaml_span(&e, &src),
        src,
    })?;

    let skills = load_dir(&root.join("skills"), parse_skill)?;
    let agents = load_dir(&root.join("agents"), parse_agent)?;

    Ok(Project {
        info,
        skills,
        agents,
        root,
    })
}

/// Resolve a `--manifest-path` that may be the file itself or its directory.
pub fn resolve_manifest(path: &Path) -> Result<PathBuf, WeftError> {
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

/// Load every `*.md` file in a directory (sorted by name), parsing each with `parse_fn`.
/// A missing directory yields an empty list — projects need not have both folders.
fn load_dir<T>(
    dir: &Path,
    parse_fn: fn(&Path, String) -> Result<T, WeftError>,
) -> Result<Vec<T>, WeftError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|source| WeftError::Read {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    entries.sort();

    entries
        .into_iter()
        .map(|p| {
            let src = read(&p)?;
            parse_fn(&p, src)
        })
        .collect()
}

fn parse_skill(path: &Path, src: String) -> Result<Skill, WeftError> {
    let (fm_src, body) = split_frontmatter(path, &src)?;
    let frontmatter: SkillMeta = parse_yaml(fm_src).map_err(|e| WeftError::Frontmatter {
        path: path.to_path_buf(),
        label: yaml_label(&e),
        span: yaml_span(&e, fm_src),
        src: src.clone(),
    })?;
    Ok(Skill {
        frontmatter,
        body,
        source_path: path.to_path_buf(),
    })
}

fn parse_agent(path: &Path, src: String) -> Result<Agent, WeftError> {
    let (fm_src, body) = split_frontmatter(path, &src)?;
    let frontmatter: AgentMeta = parse_yaml(fm_src).map_err(|e| WeftError::Frontmatter {
        path: path.to_path_buf(),
        label: yaml_label(&e),
        span: yaml_span(&e, fm_src),
        src: src.clone(),
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

fn parse_yaml<T: DeserializeOwned>(src: &str) -> Result<T, serde_yaml::Error> {
    serde_yaml::from_str(src)
}

fn read(path: &Path) -> Result<String, WeftError> {
    std::fs::read_to_string(path).map_err(|source| WeftError::Read {
        path: path.to_path_buf(),
        source,
    })
}

/// Build a `miette` source span from a serde_yaml error location.
fn yaml_span(err: &serde_yaml::Error, src: &str) -> miette::SourceSpan {
    match err.location() {
        Some(loc) => {
            let start = loc.index().min(src.len().saturating_sub(1));
            (start, 1).into()
        }
        None => (0usize, src.len().min(1)).into(),
    }
}

fn yaml_label(err: &serde_yaml::Error) -> String {
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
        let skill = parse_skill(Path::new("demo.md"), src.to_string()).unwrap();
        assert_eq!(skill.frontmatter.name, "demo");
        assert_eq!(skill.body, "Body {{ skill.name }}\n");
    }

    #[test]
    fn parses_agent_meta_with_tools() {
        let src = "---\nname: rev\ndescription: Reviewer.\ntools: [Read, Grep]\nmodel: inherit\n---\nYou review code.\n";
        let agent = parse_agent(Path::new("rev.md"), src.to_string()).unwrap();
        assert_eq!(agent.frontmatter.name, "rev");
        assert_eq!(agent.frontmatter.tools, vec!["Read", "Grep"]);
        assert_eq!(agent.frontmatter.model.as_deref(), Some("inherit"));
    }
}
