//! WU-12: the `map` stage — field-map application by [`crate::capability::FieldSource`] plus
//! the transform-hook seam (Fix 1).
//!
//! `map` is the fourth pipeline stage (parse → resolve → render → **map** → serialize → emit →
//! merge). It turns the canonical artifact fields into the host-keyed, ordered output fields the
//! serialize stage frames, driven entirely by the resolved cell's [`crate::capability::FieldMap`].
//! For each [`crate::capability::FieldRule`] it:
//!
//! - skips the rule when `emitted` is `None` (the drop tier of ADR-0002);
//! - reads the canonical value from the source the rule's [`crate::capability::FieldSource`]
//!   names — `TargetOverride` from `targets.<host>` (how skill fields are authored), `TopLevel`
//!   from the structural `name`/`description` or the flattened `fields` map (how subagent fields
//!   are authored). This source distinction is the keystone of Fix 1;
//! - when `transform` is set, defers the value to the injected [`FieldTransformSet`] hook (a
//!   per-field decorator over the generic read, ADR-0002), otherwise uses the generic read;
//! - emits the value under the recased `emitted` key, omitting absent (`None`) values — matching
//!   the v1 `frontmatter()` behaviour of dropping `None`.
//!
//! Output order equals the field-map declaration order (C-DETERMINISM). The stage is pure: it
//! reads only matrix data + the IR + the injected transform set, with no serializer or emission
//! dependency, which is why it lives in core (Fix 3).

use crate::capability::FieldSource;
use crate::ir::{Artifact, SINGLETON_KEY};
use crate::kind::ArtifactKind;
use crate::pipeline::resolve::Resolved;
use serde_yaml_ng::Value;

/// The ordered output of the `map` stage: `(emitted_key, value)` pairs in field-map order, ready
/// for the serialize stage to frame.
pub type MappedFields = Vec<(String, Value)>;

/// The transform-hook seam (ADR-0002): a per-`(host, kind, field)` decorator over the generic
/// field-map read. The trait lives in core so [`map_fields`] stays pure; the concrete impls live
/// in `weaft-targets`, injected here as `&dyn FieldTransformSet`.
pub trait FieldTransformSet {
    /// Compute the value for a field flagged `transform: true`. `current` is whatever the generic
    /// read produced for that field; a decorator impl may use it, replace it, or drop it
    /// (returning `None`). `field` is the canonical field name; `kind` is the effective kind.
    #[expect(
        clippy::too_many_arguments,
        reason = "the hook needs the full (host, kind, field, current, artifact) decision context \
                  to decorate one field's value; collapsing into a params struct would obscure the \
                  per-field decorator contract"
    )]
    fn transform(
        &self,
        host_id: &str,
        kind: ArtifactKind,
        field: &str,
        current: Option<Value>,
        artifact: &Artifact,
    ) -> Option<Value>;
}

/// The no-op transform set: every field flows through the generic mapping unchanged. Most hosts
/// need no transforms, so this is the default seam (ADR-0002).
pub struct NoTransforms;

impl FieldTransformSet for NoTransforms {
    fn transform(
        &self,
        _host_id: &str,
        _kind: ArtifactKind,
        _field: &str,
        current: Option<Value>,
        _artifact: &Artifact,
    ) -> Option<Value> {
        current
    }
}

/// Apply the resolved cell's field-map to an artifact, producing ordered output fields.
///
/// Iterates the field-map in declaration order; for each non-dropped rule it reads the canonical
/// value (per its [`FieldSource`]), optionally routes it through the `transforms` hook, and emits
/// the result under the recased key. Absent values are omitted. See the module docs for the full
/// contract.
#[must_use]
pub fn map_fields(
    resolved: &Resolved<'_>,
    artifact: &Artifact,
    transforms: &dyn FieldTransformSet,
) -> MappedFields {
    // Config-singleton cells (claude `settings.json` / `.mcp.json`) carry an empty field-map but
    // the whole payload packed under `SINGLETON_KEY` (`Artifact::from_singleton`). With no rules
    // to iterate, the generic loop below would emit nothing and the serializer would frame `{}`,
    // silently dropping the declared content. Expand the singleton mapping's top-level pairs here
    // so the document carries real fields. Guarded on the empty field-map so non-singleton
    // artifacts (which always have a populated field-map) are unaffected and the `SINGLETON_KEY`
    // sentinel never leaks as a literal field.
    if resolved.cell.field_map.0.is_empty() {
        return expand_singleton(artifact);
    }

    let mut out: MappedFields = Vec::new();
    for rule in resolved.cell.field_map.0 {
        let Some(emitted) = rule.emitted else {
            continue;
        };
        let canonical_key = rule.canonical.key();
        let generic = read_canonical(artifact, resolved.host_id, rule.canonical);
        let value = if rule.transform {
            transforms.transform(
                resolved.host_id,
                resolved.effective_kind,
                canonical_key,
                generic,
                artifact,
            )
        } else {
            generic
        };
        if let Some(value) = value {
            out.push((emitted.to_string(), value));
        }
    }
    out
}

/// Expand a config singleton's packed value into ordered output fields.
///
/// A singleton artifact ([`Artifact::from_singleton`]) stores its opaque document under
/// [`SINGLETON_KEY`] in `frontmatter.fields`. When that value is a mapping, its top-level
/// `(key, value)` pairs become the mapped fields verbatim (nested values pass through untouched —
/// the serializer handles nesting). `serde_yaml_ng::Mapping` preserves source/insertion order, so
/// the output is deterministic across runs of the same input (C-DETERMINISM). A missing or
/// non-mapping `SINGLETON_KEY` yields no fields rather than panicking.
fn expand_singleton(artifact: &Artifact) -> MappedFields {
    let Some(Value::Mapping(mapping)) = artifact.frontmatter.fields.get(SINGLETON_KEY) else {
        return MappedFields::new();
    };
    mapping
        .iter()
        .filter_map(|(key, value)| Some((key.as_str()?.to_string(), value.clone())))
        .collect()
}

/// Read a canonical field's value from the source its [`FieldSource`] names.
///
/// `TargetOverride` reads `targets.<host>` then the key; `TopLevel("name")`/`("description")`
/// read the structural fields; any other `TopLevel(key)` reads the flattened `fields` map. The
/// value is cloned out so the result is owned and the stage stays pure.
fn read_canonical(artifact: &Artifact, host_id: &str, source: FieldSource) -> Option<Value> {
    match source {
        FieldSource::TargetOverride(key) => {
            get_in_mapping(artifact.frontmatter.targets.override_for(host_id)?, key)
        },
        FieldSource::TopLevel("name") => Some(Value::String(artifact.frontmatter.name.clone())),
        FieldSource::TopLevel("description") => {
            Some(Value::String(artifact.frontmatter.description.clone()))
        },
        FieldSource::TopLevel(key) => artifact.frontmatter.fields.get(key).cloned(),
    }
}

/// Look up `key` inside a YAML mapping value, cloning the matched value out. Mirrors the override
/// read pattern in `weaft-targets/src/yaml.rs`, kept pure and in core so the `map` stage takes no
/// dependency on the targets crate (this `serde_yaml_ng` version keys mappings by `Value`).
fn get_in_mapping(value: &Value, key: &str) -> Option<Value> {
    value
        .as_mapping()?
        .get(Value::String(key.to_string()))
        .cloned()
}

#[cfg(test)]
mod tests {
    use crate::capability::{self, HostCapabilities};
    use crate::ir::{Artifact, ArtifactMeta};
    use crate::kind::ArtifactKind;
    use crate::pipeline::resolve::resolve;
    use serde_yaml_ng::Value;
    use std::path::PathBuf;

    // The map-stage surface under test (authored in GREEN). `MappedFields` is the ordered
    // `Vec<(String, serde_yaml_ng::Value)>` the serialize stage consumes; `FieldTransformSet`
    // is the injected decorator seam; `NoTransforms` is its no-op impl.
    use crate::pipeline::map::{FieldTransformSet, MappedFields, NoTransforms, map_fields};

    /// Real Claude/Cursor skill frontmatter, copied verbatim from
    /// `examples/quickstart/skills/safe-deleter.md`: `allowed_tools`/`model` live under the
    /// `targets.claude-code` override block, `globs`/`always_apply` under `targets.cursor`.
    /// The whole point of Fix 1 is that the `map` stage reads these from `targets.<host>`, not
    /// from top-level frontmatter.
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
    /// `tools`/`model`/`readonly`/`is_background` are TOP-LEVEL fields, so the map stage reads
    /// them via `FieldSource::TopLevel` from `ArtifactMeta::fields`.
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

    /// Deserialize an [`ArtifactMeta`] from a frontmatter YAML block (mirrors the proven
    /// `ir.rs` `v2_artifact_ir_tests` pattern).
    fn parse_meta(fm: &str) -> ArtifactMeta {
        serde_yaml_ng::from_str(fm).expect("frontmatter must deserialize into ArtifactMeta")
    }

    /// Build an artifact of `kind` from a frontmatter block plus a body.
    fn artifact_from(kind: ArtifactKind, fm: &str, body: &str) -> Artifact {
        Artifact {
            kind,
            frontmatter: parse_meta(fm),
            body: body.to_string(),
            source_path: PathBuf::from(format!("{}/x.md", kind.as_str())),
        }
    }

    fn host(id: &str) -> &'static HostCapabilities {
        capability::by_id(id).unwrap_or_else(|| panic!("host {id} must exist"))
    }

    /// The value emitted under `key`, if the mapped output contains it. Ordered lookup over the
    /// `Vec<(String, Value)>` the serialize stage will frame.
    fn value_for<'a>(mapped: &'a MappedFields, key: &str) -> Option<&'a Value> {
        mapped.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// The emitted keys in order — the determinism contract is that this equals the declared
    /// field-map order.
    fn keys_in_order(mapped: &MappedFields) -> Vec<&str> {
        mapped.iter().map(|(k, _)| k.as_str()).collect()
    }

    /// Read a `Value` sequence as a `Vec<&str>` for terse assertions.
    fn as_str_seq(value: &Value) -> Vec<&str> {
        value
            .as_sequence()
            .expect("expected a YAML sequence value")
            .iter()
            .filter_map(Value::as_str)
            .collect()
    }

    #[test]
    fn fix1_claude_skill_allowed_tools_value_is_sourced_from_the_override_block() {
        // THE critical Fix-1 pin. Resolve the safe-deleter Skill cell for claude-code and map
        // its fields with NoTransforms. The output must contain `allowed-tools` whose value is
        // the sequence [Read, Bash] authored under `targets.claude-code.allowed_tools` — i.e.
        // sourced from the per-target override block, NOT from top-level frontmatter and NOT
        // absent. `model` must likewise come from the same override block.
        let claude = host("claude-code");
        let art = artifact_from(ArtifactKind::Skill, SAFE_DELETER_FM, "# body\n");
        let resolved = resolve(&art, claude);

        let mapped = map_fields(&resolved, &art, &NoTransforms);

        // The emitted key is the hyphenated `allowed-tools` (v1 casing), present and non-empty.
        let allowed = value_for(&mapped, "allowed-tools").expect(
            "claude skill output must contain an `allowed-tools` key sourced from \
             targets.claude-code.allowed_tools (Fix 1) — it is missing, so the value was not \
             read from the override block",
        );
        assert_eq!(
            as_str_seq(allowed),
            ["Read", "Bash"],
            "allowed-tools value must be the [Read, Bash] sequence from the \
             targets.claude-code override block",
        );

        // `model` is also override-sourced (targets.claude-code.model).
        let model = value_for(&mapped, "model")
            .and_then(Value::as_str)
            .expect("claude skill output must carry the override-sourced `model`");
        assert_eq!(
            model, "claude-sonnet-4-5",
            "model must be read from the targets.claude-code override block",
        );

        // The canonical (un-hyphenated) source key must NOT leak through unrecased.
        assert!(
            value_for(&mapped, "allowed_tools").is_none(),
            "the emitted key must be the recased `allowed-tools`, never the canonical \
             `allowed_tools`",
        );
    }

    #[test]
    fn cursor_skill_omits_name_and_emits_override_sourced_keys() {
        // Cursor's Skill field-map has NO `name` rule (the v1 .mdc snapshot omits it), emits the
        // camelCase `alwaysApply` from targets.cursor.always_apply, and `globs` from
        // targets.cursor.globs.
        let cursor = host("cursor");
        let art = artifact_from(ArtifactKind::Skill, SAFE_DELETER_FM, "# body\n");
        let resolved = resolve(&art, cursor);

        let mapped = map_fields(&resolved, &art, &NoTransforms);

        assert!(
            value_for(&mapped, "name").is_none(),
            "cursor skill frontmatter must NOT emit a `name` key (v1 .mdc fidelity)",
        );

        let always_apply = value_for(&mapped, "alwaysApply")
            .and_then(Value::as_bool)
            .expect("cursor skill output must carry the camelCase `alwaysApply` key");
        assert!(
            !always_apply,
            "always_apply is authored false under targets.cursor — its mapped value must be false",
        );

        let globs = value_for(&mapped, "globs")
            .map(as_str_seq)
            .expect("cursor skill output must carry `globs` from the targets.cursor block");
        assert_eq!(
            globs,
            ["**/*"],
            "globs value must come from the targets.cursor override block",
        );

        // The override source key for always_apply is snake_case; only the recased key emits.
        assert!(
            value_for(&mapped, "always_apply").is_none(),
            "cursor must emit only the camelCase `alwaysApply`, not the canonical `always_apply`",
        );
    }

    #[test]
    fn claude_subagent_maps_top_level_tools_field() {
        // Subagent fields are top-level on the artifact, so the Claude Subagent `tools` rule
        // (`FieldSource::TopLevel("tools")`) reads from `ArtifactMeta::fields` and emits `tools`.
        let claude = host("claude-code");
        let art = artifact_from(
            ArtifactKind::Subagent,
            CODE_REVIEWER_FM,
            "You review code.\n",
        );
        let resolved = resolve(&art, claude);

        let mapped = map_fields(&resolved, &art, &NoTransforms);

        let tools = value_for(&mapped, "tools")
            .map(as_str_seq)
            .expect("claude subagent output must carry the top-level `tools` field");
        assert_eq!(
            tools,
            ["Read", "Grep", "Bash"],
            "tools must be read from the top-level frontmatter (FieldSource::TopLevel)",
        );

        // Structural name/description map through too (both TopLevel on the subagent cell).
        assert_eq!(
            value_for(&mapped, "name").and_then(Value::as_str),
            Some("code-reviewer"),
            "subagent `name` is a top-level structural field and must map",
        );
    }

    #[test]
    fn output_field_order_equals_the_declared_field_map_order() {
        // C-DETERMINISM: the mapped output order is exactly the cell's field-map declaration
        // order. The Claude Skill field-map declares name, description, allowed_tools, model;
        // emitted as name, description, allowed-tools, model — in that order.
        let claude = host("claude-code");
        let art = artifact_from(ArtifactKind::Skill, SAFE_DELETER_FM, "# body\n");
        let resolved = resolve(&art, claude);

        let mapped = map_fields(&resolved, &art, &NoTransforms);

        assert_eq!(
            keys_in_order(&mapped),
            ["name", "description", "allowed-tools", "model"],
            "mapped fields must be emitted in the declared field-map order (determinism)",
        );
    }

    #[test]
    fn no_transforms_leaves_the_generic_mapping_unchanged() {
        // The `NoTransforms` no-op must not perturb the generic field-map result: mapping with
        // it produces exactly the same ordered (key, value) pairs as the field-map dictates.
        let claude = host("claude-code");
        let art = artifact_from(ArtifactKind::Skill, SAFE_DELETER_FM, "# body\n");
        let resolved = resolve(&art, claude);

        let mapped = map_fields(&resolved, &art, &NoTransforms);

        // Full, exact shape: keys in order + their generic values, all sourced as Fix 1 dictates.
        assert_eq!(
            keys_in_order(&mapped),
            ["name", "description", "allowed-tools", "model"]
        );
        assert_eq!(
            value_for(&mapped, "name").and_then(Value::as_str),
            Some("safe-deleter"),
        );
        assert_eq!(
            value_for(&mapped, "allowed-tools").map(as_str_seq),
            Some(vec!["Read", "Bash"]),
        );
        assert_eq!(
            value_for(&mapped, "model").and_then(Value::as_str),
            Some("claude-sonnet-4-5"),
        );
    }

    /// A stub transform set that overrides exactly ONE field (`developer_instructions`) and
    /// leaves every other field to the generic mapping. This is the decorator-behavior probe:
    /// the hook fires only for the field flagged `transform: true` in the codex subagent cell,
    /// returning a synthesized value; all other fields must still flow generically.
    struct OnlyDeveloperInstructions;

    impl FieldTransformSet for OnlyDeveloperInstructions {
        fn transform(
            &self,
            _host_id: &str,
            _kind: ArtifactKind,
            field: &str,
            current: Option<Value>,
            _artifact: &Artifact,
        ) -> Option<Value> {
            if field == "developer_instructions" {
                Some(Value::String("SYNTHESIZED BODY".to_string()))
            } else {
                // Decorator: defer to whatever the generic read produced for every other field.
                current
            }
        }
    }

    /// Build the `permissions: {defaultMode: acceptEdits}` mapping the quickstart declares as its
    /// `settings:` singleton — a nested mapping, to prove the expansion preserves inner structure.
    fn settings_singleton_value() -> Value {
        serde_yaml_ng::from_str("permissions:\n  defaultMode: acceptEdits\n")
            .expect("settings singleton fixture must be valid YAML")
    }

    #[test]
    fn singleton_settings_expand_into_mapped_fields_not_empty() {
        // The config-singleton correctness pin (regression for the silently-dropped `{}` bug):
        // the claude-code Settings cell has an EMPTY field-map, but the artifact carries its
        // payload under `__singleton` (Artifact::from_singleton). `map_fields` must expand that
        // mapping's top-level pairs into the output so the JSON serializer frames real content —
        // here the nested `permissions` mapping, NOT an empty output and NOT a literal
        // `__singleton` key.
        let claude = host("claude-code");
        let art = Artifact::from_singleton(
            ArtifactKind::Settings,
            "settings",
            settings_singleton_value(),
        );
        let resolved = resolve(&art, claude);

        let mapped = map_fields(&resolved, &art, &NoTransforms);

        assert!(
            !mapped.is_empty(),
            "a declared Settings singleton must map to non-empty fields, not be dropped to `{{}}`",
        );

        // The inner `permissions` key/value must surface verbatim (nesting preserved).
        let permissions = value_for(&mapped, "permissions").expect(
            "the singleton's top-level `permissions` key must expand into the mapped fields",
        );
        let expected: Value = serde_yaml_ng::from_str("defaultMode: acceptEdits\n")
            .expect("expected nested mapping must parse");
        assert_eq!(
            permissions, &expected,
            "the expanded value must equal the nested mapping verbatim (JSON handles nesting)",
        );

        // The storage key must never leak as a literal emitted field.
        assert!(
            !keys_in_order(&mapped).contains(&"__singleton"),
            "the `__singleton` storage key must not leak into the mapped output",
        );
    }

    #[test]
    fn singleton_mcp_server_expands_inner_object_fields() {
        // The McpServer singleton packs the inner server object (`{command, args}`) under
        // `__singleton`; its cell field-map is empty too. The expansion must surface `command`
        // and `args` so `.mcp.json` carries the real server object rather than `{}`.
        let claude = host("claude-code");
        let value: Value =
            serde_yaml_ng::from_str("command: npx\nargs:\n  - \"-y\"\n  - server-filesystem\n")
                .expect("mcp server fixture must be valid YAML");
        let art = Artifact::from_singleton(ArtifactKind::McpServer, "fs", value);
        let resolved = resolve(&art, claude);

        let mapped = map_fields(&resolved, &art, &NoTransforms);

        assert_eq!(
            value_for(&mapped, "command").and_then(Value::as_str),
            Some("npx"),
            "the mcp server's `command` must expand into the mapped fields",
        );
        assert_eq!(
            value_for(&mapped, "args").map(as_str_seq),
            Some(vec!["-y", "server-filesystem"]),
            "the mcp server's `args` sequence must expand into the mapped fields",
        );
        assert!(
            !keys_in_order(&mapped).contains(&"__singleton"),
            "the `__singleton` storage key must not leak into the mapped output",
        );
    }

    #[test]
    fn a_stub_transform_overrides_one_field_and_leaves_the_rest_generic() {
        // Decorator behavior (ADR-0002): the codex subagent cell flags `developer_instructions`
        // with `transform: true`, so the hook computes it; `name`/`description` are transform:
        // false and must still map generically from the top-level frontmatter. This proves the
        // transform is a narrow per-field decorator over the generic map, not a wholesale rewrite.
        let codex = host("codex");
        let art = artifact_from(
            ArtifactKind::Subagent,
            CODE_REVIEWER_FM,
            "You review code.\n",
        );
        let resolved = resolve(&art, codex);

        let mapped = map_fields(&resolved, &art, &OnlyDeveloperInstructions);

        // The single transform-flagged field carries the synthesized value.
        assert_eq!(
            value_for(&mapped, "developer_instructions").and_then(Value::as_str),
            Some("SYNTHESIZED BODY"),
            "the transform-flagged field must take the hook's value",
        );

        // Every non-flagged field is untouched by the hook — still the generic read.
        assert_eq!(
            value_for(&mapped, "name").and_then(Value::as_str),
            Some("code-reviewer"),
            "name is transform:false — the stub must leave it to the generic mapping",
        );
        assert_eq!(
            value_for(&mapped, "description").and_then(Value::as_str),
            Some(
                "Expert code reviewer. Use proactively after code changes to catch bugs and \
                 security issues."
            ),
            "description is transform:false — the stub must leave it to the generic mapping",
        );
    }
}
