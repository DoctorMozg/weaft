//! Intermediate representation: the parsed in-memory form of a weaft project.
//!
//! A project is a directory containing a `weaft.yaml` info file, a `skills/` folder of
//! skill `.md` files, an `agents/` folder of subagent `.md` files, and an optional
//! `fragments/` folder of shared Jinja partials. Frontmatter is YAML throughout.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A fully-loaded weaft project: the `weaft.yaml` info plus every skill and subagent.
#[derive(Debug, Clone)]
pub struct Project {
    pub info: ProjectInfo,
    pub skills: Vec<Skill>,
    pub agents: Vec<Agent>,
    /// Absolute path to the project root (directory containing `weaft.yaml`).
    pub root: PathBuf,
}

/// Contents of `weaft.yaml`: project-wide metadata, target config, shared parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub meta: Meta,
    #[serde(default)]
    pub targets: Targets,
    #[serde(default)]
    pub parameters: BTreeMap<String, Parameter>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Which targets a skill/agent/project supports, plus per-target override blocks.
///
/// `supported` lists target ids. Any other key (e.g. `claude-code:`, `cursor:`) is
/// captured verbatim into `overrides` for the target backend to interpret.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Targets {
    #[serde(default)]
    pub supported: Vec<String>,
    #[serde(flatten)]
    pub overrides: BTreeMap<String, serde_yaml_ng::Value>,
}

impl Targets {
    /// Fetch the raw override block for a given target id, if present.
    pub fn override_for(&self, target_id: &str) -> Option<&serde_yaml_ng::Value> {
        self.overrides.get(target_id)
    }

    /// Is this artifact compiled for `target_id`? If `supported` is empty, the artifact
    /// is considered to support every target the project supports.
    pub fn supports(&self, target_id: &str) -> bool {
        self.supported.is_empty() || self.supported.iter().any(|t| t == target_id)
    }
}

/// A user-settable parameter declared in `weaft.yaml`, surfaced to templates as
/// `{{ params.<name> }}` and overridable via `--param name=value`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Parameter {
    String {
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        description: Option<String>,
    },
    Bool {
        #[serde(default)]
        default: Option<bool>,
        #[serde(default)]
        description: Option<String>,
    },
    Int {
        #[serde(default)]
        default: Option<i64>,
        #[serde(default)]
        description: Option<String>,
    },
}

impl Parameter {
    pub fn description(&self) -> Option<&str> {
        match self {
            Parameter::String { description, .. }
            | Parameter::Bool { description, .. }
            | Parameter::Int { description, .. } => description.as_deref(),
        }
    }
}

/// A single skill: YAML frontmatter metadata + raw (pre-render) Jinja body.
#[derive(Debug, Clone)]
pub struct Skill {
    pub frontmatter: SkillMeta,
    /// Raw Jinja source for the body, exactly as authored (post-frontmatter).
    pub body: String,
    /// Absolute path to the source `.md` file.
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub targets: Targets,
}

/// A single subagent: YAML frontmatter metadata + raw (pre-render) Jinja body.
#[derive(Debug, Clone)]
pub struct Agent {
    pub frontmatter: AgentMeta,
    pub body: String,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMeta {
    pub name: String,
    pub description: String,
    /// Tool allowlist (Claude Code). Hosts without `agent_supports_tools` ignore it.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Preferred model, e.g. `"inherit"` or a model id.
    #[serde(default)]
    pub model: Option<String>,
    /// Cursor-style read-only restriction. Hosts without `agent_supports_readonly`
    /// ignore it.
    #[serde(default)]
    pub readonly: Option<bool>,
    #[serde(default)]
    pub is_background: Option<bool>,
    #[serde(default)]
    pub targets: Targets,
}
