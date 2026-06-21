//! Intermediate representation: the parsed in-memory form of a weaft project.
//!
//! A project is a directory containing a `weaft.yaml` info file, a `skills/` folder of
//! skill `.md` files, an `agents/` folder of subagent `.md` files, and an optional
//! `fragments/` folder of shared Jinja partials. Frontmatter is YAML throughout.
//!
//! ## v2 generic artifact model (WU-5)
//! [`Project`] now carries a single `Vec<Artifact>` rather than separate `skills`/`agents`
//! vectors: every compiled thing is a kind-tagged [`Artifact`]. The pre-v2 [`Skill`] /
//! [`Agent`] types are retained as a migration shim — the [`Project::skills`] /
//! [`Project::agents`] accessors filter by kind.

use crate::kind::ArtifactKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A fully-loaded weaft project: the `weaft.yaml` info plus every compiled artifact.
#[derive(Debug, Clone)]
pub struct Project {
    pub info: ProjectInfo,
    /// Every artifact, kind-tagged. Skills, subagents, and (later) singleton config all live
    /// here so the pipeline is uniform across the nine [`ArtifactKind`] variants.
    pub artifacts: Vec<Artifact>,
    /// Absolute path to the project root (directory containing `weaft.yaml`).
    pub root: PathBuf,
}

impl Project {
    /// Every `Skill`-kind artifact, in declaration order. Migration accessor: callers that
    /// only need the structural `name`/`description`/`targets`/`body` read these directly.
    pub fn skills(&self) -> impl Iterator<Item = &Artifact> {
        self.artifacts
            .iter()
            .filter(|a| a.kind == ArtifactKind::Skill)
    }

    /// Every `Subagent`-kind artifact, in declaration order. Migration accessor (see
    /// [`Project::skills`]).
    pub fn agents(&self) -> impl Iterator<Item = &Artifact> {
        self.artifacts
            .iter()
            .filter(|a| a.kind == ArtifactKind::Subagent)
    }
}

/// Contents of `weaft.yaml`: project-wide metadata, target config, shared parameters, and
/// the manifest-only singleton config (`settings` / `mcp_servers` / `ignore` / `plugin`).
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
    /// Opaque host settings document (C-SOURCE-LAYOUT singleton). Carried verbatim and
    /// emitted by hosts whose `Settings` cell is native (e.g. `.claude/settings.json`).
    #[serde(default)]
    pub settings: Option<serde_yaml_ng::Value>,
    /// MCP server declarations, keyed by server name (C-SOURCE-LAYOUT singleton). Each entry is
    /// opaque; at parse time they are aggregated into one `mcpServers`-enveloped document emitted
    /// by hosts whose `McpServer` cell is native (e.g. claude-code's repo-root `.mcp.json`).
    #[serde(default)]
    pub mcp_servers: BTreeMap<String, serde_yaml_ng::Value>,
    /// Ignore patterns (C-SOURCE-LAYOUT singleton). Emitted to hosts with a native ignore
    /// file; empty means no synthetic ignore artifact is produced.
    #[serde(default)]
    pub ignore: Vec<String>,
    /// Optional plugin identity (C-SOURCE-LAYOUT singleton).
    #[serde(default)]
    pub plugin: Option<PluginIdentity>,
}

/// Plugin identity declared in `weaft.yaml`. Validation begins permissive
/// (C-OPEN-QUESTIONS): only `name` is required; any other key is captured into `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginIdentity {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    /// Any plugin key beyond `name`/`version`, captured verbatim so the schema can grow
    /// without breaking older manifests.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml_ng::Value>,
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

/// The `frontmatter.fields` key under which [`Artifact::from_singleton`] packs a manifest
/// singleton's opaque value. Keeping it nested under one key (rather than flattening the blob
/// into structural frontmatter) is what lets the `serialize` stage read the whole singleton
/// document back verbatim. Every stage that reads a synthesized singleton must use this same
/// constant so map/serialize stay in lockstep with parse.
pub const SINGLETON_KEY: &str = "__singleton";

/// A kind-tagged artifact: the v2 unit of compilation. One `Artifact` type covers all nine
/// [`ArtifactKind`] variants; per-kind richness lives in the opaque frontmatter `fields`.
#[derive(Debug, Clone)]
pub struct Artifact {
    pub kind: ArtifactKind,
    pub frontmatter: ArtifactMeta,
    /// Raw Jinja source for the body, exactly as authored (post-frontmatter). Empty for
    /// singleton config artifacts whose payload lives entirely in the frontmatter.
    pub body: String,
    /// Absolute path to the source `.md` file (synthetic singletons use a sentinel path).
    pub source_path: PathBuf,
}

impl Artifact {
    /// Test-only: lift a v1 [`Skill`] fixture into the v2 generic IR. Production parse paths
    /// build `Artifact` directly; this exists only to keep test helpers readable.
    #[cfg(test)]
    pub(crate) fn from_skill(skill: Skill) -> Self {
        Artifact {
            kind: ArtifactKind::Skill,
            frontmatter: ArtifactMeta::from_skill_meta(skill.frontmatter),
            body: skill.body,
            source_path: skill.source_path,
        }
    }

    /// Test-only: lift a v1 [`Agent`] fixture into the v2 generic IR.
    #[cfg(test)]
    pub(crate) fn from_agent(agent: Agent) -> Self {
        Artifact {
            kind: ArtifactKind::Subagent,
            frontmatter: ArtifactMeta::from_agent_meta(agent.frontmatter),
            body: agent.body,
            source_path: agent.source_path,
        }
    }

    /// Synthesize a singleton config artifact (settings / mcp / plugin / ignore) from a manifest
    /// declaration. The opaque `value` is packed verbatim under [`SINGLETON_KEY`] in
    /// `frontmatter.fields` — deliberately *not* flattened into structural frontmatter, so the
    /// whole blob round-trips to the `serialize` stage as one document. The body is empty (a
    /// singleton carries its payload entirely in frontmatter) and `source_path` is a synthetic
    /// `<manifest:{name}>` sentinel so diagnostics can tell it apart from a file-backed artifact.
    #[must_use]
    pub fn from_singleton(kind: ArtifactKind, name: &str, value: serde_yaml_ng::Value) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert(SINGLETON_KEY.to_string(), value);
        Artifact {
            kind,
            frontmatter: ArtifactMeta {
                name: name.to_string(),
                description: String::new(),
                targets: Targets::default(),
                kind_override: None,
                fields,
            },
            body: String::new(),
            source_path: PathBuf::from(format!("<manifest:{name}>")),
        }
    }
}

/// Generic frontmatter common to every artifact kind.
///
/// The structural `name`/`description` and the per-target `targets` override blocks are named
/// fields; everything else an author writes top-level lands in the flattened `fields` map.
/// Keeping `targets` *named* (not folded into `fields`) is the IR half of Fix 1: skill fields
/// authored under `targets.<host>` stay reachable via [`Targets::override_for`], exactly as
/// in v1, while subagent fields authored top-level land in `fields`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMeta {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub targets: Targets,
    /// Per-artifact kind override (C-SOURCE-LAYOUT): a `kind:` frontmatter key lets a file in
    /// one folder declare a different kind. Read from the `kind` key, kept out of `fields`.
    #[serde(default, rename = "kind")]
    pub kind_override: Option<ArtifactKind>,
    /// Every top-level frontmatter key that is not structural (`name`/`description`/`targets`/
    /// `kind`). This is the `FieldSource::TopLevel` read site for the `map` stage.
    #[serde(flatten)]
    pub fields: BTreeMap<String, serde_yaml_ng::Value>,
}

impl ArtifactMeta {
    /// Lift a v1 [`SkillMeta`] into generic frontmatter. Skill fields live under `targets`,
    /// so `fields` stays empty.
    #[must_use]
    pub fn from_skill_meta(meta: SkillMeta) -> Self {
        ArtifactMeta {
            name: meta.name,
            description: meta.description,
            targets: meta.targets,
            kind_override: None,
            fields: BTreeMap::new(),
        }
    }

    /// Lift a v1 [`AgentMeta`] into generic frontmatter, packing the subagent top-level
    /// fields into `fields` so they round-trip back through [`ArtifactMeta::to_agent_meta`].
    #[must_use]
    pub fn from_agent_meta(meta: AgentMeta) -> Self {
        let mut fields = BTreeMap::new();
        insert_field(&mut fields, "tools", &meta.tools);
        insert_field(&mut fields, "model", &meta.model);
        insert_field(&mut fields, "readonly", &meta.readonly);
        insert_field(&mut fields, "is_background", &meta.is_background);
        ArtifactMeta {
            name: meta.name,
            description: meta.description,
            targets: meta.targets,
            kind_override: None,
            fields,
        }
    }

    /// Reconstruct the v1 [`SkillMeta`] view (structural fields only).
    #[must_use]
    pub fn to_skill_meta(&self) -> SkillMeta {
        SkillMeta {
            name: self.name.clone(),
            description: self.description.clone(),
            targets: self.targets.clone(),
        }
    }

    /// Reconstruct the v1 [`AgentMeta`] view by deserializing the structural fields plus the
    /// flattened top-level `fields` through serde — reusing serde's exact `#[serde(default)]`
    /// behavior so the round-trip is lossless and the v1 emit output is byte-identical.
    #[must_use]
    pub fn to_agent_meta(&self) -> AgentMeta {
        let value =
            serde_yaml_ng::to_value(self).expect("ArtifactMeta serializes to a YAML mapping");
        serde_yaml_ng::from_value(value)
            .expect("ArtifactMeta mapping deserializes back into AgentMeta")
    }
}

/// Insert a serializable subagent field into the opaque `fields` map under `key`. The value
/// is stored even when defaulted, so the `to_agent_meta` round-trip is faithful.
fn insert_field<T: Serialize>(
    fields: &mut BTreeMap<String, serde_yaml_ng::Value>,
    key: &str,
    value: &T,
) {
    let yaml = serde_yaml_ng::to_value(value).expect("subagent field serializes to YAML");
    fields.insert(key.to_string(), yaml);
}

/// WU-5: generic `Artifact` IR + manifest singletons (RED).
///
/// These tests target the v2 IR types authored in a later GREEN step: the kind-tagged
/// [`Artifact`]/`ArtifactMeta` that *retains* the per-target `Targets` override blocks
/// (Fix 1), the extended [`ProjectInfo`] singletons (`settings`/`mcp_servers`/`ignore`/`plugin`),
/// `PluginIdentity`, and the `Project::skills()`/`agents()` kind accessors. Until those
/// land, this module fails to compile with missing-symbol / missing-field errors — the
/// expected RED state. It must not be softened by stubbing the production types.
#[cfg(test)]
mod v2_artifact_ir_tests {
    use super::*;
    use crate::kind::ArtifactKind;
    use std::path::PathBuf;

    /// Real Claude/Cursor skill frontmatter, copied verbatim from
    /// `examples/quickstart/skills/safe-deleter.md`: `allowed_tools`/`model` live under the
    /// `targets.claude-code` override block, `globs`/`always_apply` under `targets.cursor`.
    /// The whole point of Fix 1 is that these stay in `targets`, NOT in top-level `fields`.
    const SAFE_DELETER_FM: &str = "\
name: safe-deleter
description: Deletes files on request, but never destructively without confirmation.
targets:
  claude-code:
    allowed_tools:
      - Read
      - Bash
    model: claude-sonnet-4-5
  cursor:
    globs:
      - \"**/*\"
    always_apply: false
";

    /// Real subagent frontmatter, copied from `examples/quickstart/agents/code-reviewer.md`:
    /// `tools`/`model`/`readonly`/`is_background` are TOP-LEVEL fields (the subagent authoring
    /// shape), so they must land in `ArtifactMeta::fields` (the `FieldSource::TopLevel` site).
    const CODE_REVIEWER_FM: &str = "\
name: code-reviewer
description: Expert code reviewer. Use proactively after code changes to catch bugs and security issues.
tools:
  - Read
  - Grep
  - Bash
model: inherit
readonly: true
is_background: false
";

    /// Look up a key inside a YAML mapping value, mirroring the proven access pattern in
    /// `weaft-targets/src/yaml.rs` (this serde_yaml_ng version keys mappings by `Value`).
    fn map_get<'a>(value: &'a serde_yaml_ng::Value, key: &str) -> Option<&'a serde_yaml_ng::Value> {
        value
            .as_mapping()?
            .get(serde_yaml_ng::Value::String(key.to_string()))
    }

    /// Deserialize an `ArtifactMeta` from a frontmatter YAML block.
    fn parse_meta(fm: &str) -> ArtifactMeta {
        serde_yaml_ng::from_str(fm).expect("frontmatter must deserialize into ArtifactMeta")
    }

    #[test]
    fn fix1_skill_override_fields_land_in_targets_not_top_level_fields() {
        // The Fix-1 regression guard: `targets.claude-code.allowed_tools` MUST be reachable
        // via `meta.targets.override_for("claude-code")` (the override block), and MUST NOT be
        // flattened into the top-level `fields` map. If a future refactor folds override blocks
        // into `fields`, `override_for` returns None here and the test fails — by design.
        let meta = parse_meta(SAFE_DELETER_FM);

        let claude = meta
            .targets
            .override_for("claude-code")
            .expect("claude-code override block must be retained under `targets`");
        let allowed = map_get(claude, "allowed_tools")
            .and_then(serde_yaml_ng::Value::as_sequence)
            .expect("allowed_tools must live inside the claude-code override block");
        let tools: Vec<&str> = allowed
            .iter()
            .filter_map(serde_yaml_ng::Value::as_str)
            .collect();
        assert_eq!(
            tools,
            ["Read", "Bash"],
            "allowed_tools value must come from the targets.claude-code override block",
        );

        // And it must NOT have leaked into top-level `fields`.
        assert!(
            !meta.fields.contains_key("allowed_tools"),
            "override-block fields must NOT be flattened into top-level `fields` (Fix 1)",
        );
        // The whole `targets` block itself must not appear as an opaque top-level field.
        assert!(
            !meta.fields.contains_key("targets"),
            "`targets` is structural and must not be captured into the opaque `fields` map",
        );
    }

    #[test]
    fn fix1_cursor_override_fields_also_stay_under_targets() {
        // The cursor override block carries `globs`/`always_apply`; these are read by
        // `FieldSource::TargetOverride` too, so they must stay under `targets`, not `fields`.
        let meta = parse_meta(SAFE_DELETER_FM);

        let cursor = meta
            .targets
            .override_for("cursor")
            .expect("cursor override block must be retained under `targets`");
        let always_apply = map_get(cursor, "always_apply")
            .and_then(serde_yaml_ng::Value::as_bool)
            .expect("always_apply must live inside the cursor override block");
        assert!(
            !always_apply,
            "always_apply is authored as false in the fixture"
        );
        assert!(
            !meta.fields.contains_key("globs") && !meta.fields.contains_key("always_apply"),
            "cursor override fields must not leak into top-level `fields`",
        );
    }

    #[test]
    fn subagent_top_level_fields_land_in_fields_map() {
        // Subagent fields are authored top-level (not under `targets`), so they are exactly the
        // `FieldSource::TopLevel` site: `ArtifactMeta::fields`. `name`/`description` are
        // structural and must NOT appear in `fields` (they are their own fields).
        let meta = parse_meta(CODE_REVIEWER_FM);

        assert_eq!(meta.name, "code-reviewer");
        assert!(
            !meta.fields.contains_key("name") && !meta.fields.contains_key("description"),
            "structural name/description must not be captured into the opaque `fields` map",
        );

        let tools = meta
            .fields
            .get("tools")
            .and_then(serde_yaml_ng::Value::as_sequence)
            .expect("top-level `tools:` list must be accessible as a top-level field");
        let tool_names: Vec<&str> = tools
            .iter()
            .filter_map(serde_yaml_ng::Value::as_str)
            .collect();
        assert_eq!(tool_names, ["Read", "Grep", "Bash"]);

        // model/readonly/is_background are also top-level subagent fields.
        assert_eq!(
            meta.fields
                .get("model")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("inherit"),
        );
        assert_eq!(
            meta.fields
                .get("readonly")
                .and_then(serde_yaml_ng::Value::as_bool),
            Some(true),
        );
        assert_eq!(
            meta.fields
                .get("is_background")
                .and_then(serde_yaml_ng::Value::as_bool),
            Some(false),
        );
    }

    #[test]
    fn kind_override_parses_from_frontmatter() {
        // C-SOURCE-LAYOUT: an artifact MAY override its kind via a `kind:` frontmatter field.
        // `kind: subagent` must deserialize into `Some(ArtifactKind::Subagent)` and must NOT be
        // captured into the opaque `fields` map (it is the structural `kind_override`).
        let fm = "name: x\ndescription: y\nkind: subagent\n";
        let meta = parse_meta(fm);
        assert_eq!(meta.kind_override, Some(ArtifactKind::Subagent));
        assert!(
            !meta.fields.contains_key("kind"),
            "`kind` is the structural override field, not opaque top-level data",
        );
    }

    #[test]
    fn kind_override_absent_is_none() {
        // No `kind:` field → no override; the folder decides the kind (WU-8).
        let meta = parse_meta("name: x\ndescription: y\n");
        assert_eq!(meta.kind_override, None);
    }

    #[test]
    fn project_skills_returns_only_skill_kind_artifacts() {
        // `Project::skills()` filters `artifacts` by kind. Build a mixed-kind project and
        // assert only the Skill-kind artifacts come back (and `agents()` only Subagent-kind).
        let project = Project {
            info: minimal_info(),
            artifacts: vec![
                artifact(ArtifactKind::Skill, "alpha"),
                artifact(ArtifactKind::Subagent, "reviewer"),
                artifact(ArtifactKind::Skill, "beta"),
                artifact(ArtifactKind::Instruction, "house-rules"),
            ],
            root: PathBuf::from("/tmp/proj"),
        };

        let skill_names: Vec<&str> = project
            .skills()
            .map(|s| s.frontmatter.name.as_str())
            .collect();
        assert_eq!(
            skill_names,
            ["alpha", "beta"],
            "skills() must return only Skill-kind artifacts, in order",
        );

        let agent_names: Vec<&str> = project
            .agents()
            .map(|a| a.frontmatter.name.as_str())
            .collect();
        assert_eq!(
            agent_names,
            ["reviewer"],
            "agents() must return only Subagent-kind artifacts",
        );
    }

    #[test]
    fn project_info_singletons_deserialize_when_present() {
        // C-SOURCE-LAYOUT manifest singletons: settings/mcp_servers/ignore deserialize from
        // weaft.yaml when present. `plugin` carries a permissive PluginIdentity (name + extra).
        let yaml = "\
name: demo
version: 0.1.0
settings:
  theme: dark
mcp_servers:
  fs:
    command: fs-server
  search:
    command: search-server
ignore:
  - \"target/\"
  - \".git/\"
plugin:
  name: demo-plugin
  version: 1.2.3
  homepage: https://example.test
";
        let info: ProjectInfo =
            serde_yaml_ng::from_str(yaml).expect("manifest with singletons must deserialize");

        assert!(
            info.settings.is_some(),
            "settings singleton must be parsed when present"
        );
        assert_eq!(
            info.mcp_servers
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["fs", "search"],
            "mcp_servers must deserialize keyed by server name",
        );
        assert_eq!(info.ignore, ["target/", ".git/"]);

        let plugin = info
            .plugin
            .expect("plugin identity must parse when present");
        assert_eq!(plugin.name, "demo-plugin");
        assert_eq!(plugin.version.as_deref(), Some("1.2.3"));
        assert!(
            plugin.extra.contains_key("homepage"),
            "unknown plugin keys must be captured permissively into `extra` (C-OPEN-QUESTIONS)",
        );
    }

    #[test]
    fn project_info_singletons_default_when_absent() {
        // Absence must produce defaults, not errors: empty mcp_servers/ignore, None settings,
        // None plugin. A bare manifest (the v1 shape) must still deserialize unchanged.
        let info: ProjectInfo = serde_yaml_ng::from_str("name: demo\nversion: 0.1.0\n")
            .expect("bare manifest must parse");

        assert!(info.settings.is_none(), "absent settings → None");
        assert!(
            info.mcp_servers.is_empty(),
            "absent mcp_servers → empty map"
        );
        assert!(info.ignore.is_empty(), "absent ignore → empty vec");
        assert!(info.plugin.is_none(), "absent plugin → None");
    }

    // --- helpers ---

    fn minimal_info() -> ProjectInfo {
        serde_yaml_ng::from_str("name: demo\nversion: 0.1.0\n").expect("minimal manifest")
    }

    /// A file-backed artifact of a given kind with a `name` and an empty body.
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
}

/// WU-9: `Artifact::from_singleton` constructor (RED).
///
/// The manifest singleton parser (WU-9) synthesizes config artifacts (settings / mcp / plugin /
/// ignore) by packing an opaque `serde_yaml_ng::Value` into an [`Artifact`] via
/// `Artifact::from_singleton(kind, name, value)`. The packed value is carried in
/// `frontmatter.fields` under the documented key `"__singleton"` (see WU-9 in the plan), where
/// the JSON serializer (WU-13) later reads it back as the document body.
///
/// These tests pin that constructor's contract: kind, name, an empty body (the payload lives in
/// frontmatter, not the body), and a faithfully-retrievable value under the documented storage
/// key. `Artifact::from_singleton` does not exist yet, so this module fails to compile with a
/// missing-method error — the expected RED. It must not be softened by stubbing the constructor.
#[cfg(test)]
mod v2_singleton_ir_tests {
    use super::*;
    use crate::kind::ArtifactKind;

    /// The documented `frontmatter.fields` key under which `from_singleton` packs the opaque
    /// singleton value (plan WU-9: "carried in `frontmatter.fields` under a documented key, e.g.
    /// `"__singleton"`"). If the GREEN implementation chooses a different key, this constant is
    /// the single place to retune the IR-level assertion.
    const SINGLETON_KEY: &str = "__singleton";

    /// Build a small YAML mapping value (`theme: dark`) to stand in for an opaque settings blob.
    fn settings_value() -> serde_yaml_ng::Value {
        serde_yaml_ng::from_str("theme: dark\nfont_size: 14\n")
            .expect("settings fixture must be valid YAML")
    }

    #[test]
    fn from_singleton_sets_kind_name_and_empty_body() {
        // The structural contract: the synthetic artifact carries the requested kind and name,
        // and an EMPTY body — a singleton's payload lives entirely in the frontmatter, never in
        // the Jinja body (Artifact::body doc-comment: "Empty for singleton config artifacts").
        let artifact =
            Artifact::from_singleton(ArtifactKind::Settings, "settings", settings_value());

        assert_eq!(
            artifact.kind,
            ArtifactKind::Settings,
            "from_singleton must tag the artifact with the requested kind",
        );
        assert_eq!(
            artifact.frontmatter.name, "settings",
            "from_singleton must set the artifact name to the supplied name",
        );
        assert!(
            artifact.body.is_empty(),
            "a singleton artifact carries its payload in frontmatter, so the body must be empty",
        );
    }

    #[test]
    fn from_singleton_packs_value_under_documented_key() {
        // The value-retrieval contract: the opaque value is stored verbatim in
        // `frontmatter.fields[SINGLETON_KEY]` and is byte-for-byte the value handed in — this is
        // exactly what the WU-13 JSON serializer reads back as the emitted document.
        let value = settings_value();
        let artifact = Artifact::from_singleton(ArtifactKind::Settings, "settings", value.clone());

        let stored =
            artifact.frontmatter.fields.get(SINGLETON_KEY).expect(
                "from_singleton must pack the value under the documented `__singleton` key",
            );
        assert_eq!(
            stored, &value,
            "the packed singleton value must equal the value passed to from_singleton",
        );
    }

    #[test]
    fn from_singleton_does_not_leak_value_as_structural_fields() {
        // The packed value must NOT be spread across structural frontmatter: a settings blob with
        // its own `theme`/`font_size` keys must not populate name/description from those keys, and
        // must not appear as top-level `fields` entries (only the single `__singleton` key holds
        // the payload). This guards against a tempting-but-wrong `serde_yaml_ng::from_value`
        // flatten of the blob into the frontmatter.
        let artifact =
            Artifact::from_singleton(ArtifactKind::Settings, "settings", settings_value());

        assert!(
            !artifact.frontmatter.fields.contains_key("theme"),
            "singleton inner keys must stay nested under `__singleton`, not flattened into fields",
        );
        let field_keys: Vec<&str> = artifact
            .frontmatter
            .fields
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            field_keys,
            [SINGLETON_KEY],
            "the only top-level field on a singleton artifact must be the packed `__singleton`",
        );
    }
}
