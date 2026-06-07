//! Codex backend.
//!
//! A mostly-thin shell: the host's layout, frontmatter keys, and disposition are matrix data
//! (`weaft_core::capability::CODEX`), so the generic [`crate::Target::emit_artifact`] default
//! produces the files. For reference, the matrix declares: skills → the Agent Skills standard
//! `.agents/skills/<name>/SKILL.md` (YAML frontmatter, *not* codex's own `.codex/` tree);
//! subagents → `.codex/agents/<name>.toml`, a TOML document whose rendered body rides in the
//! `developer_instructions` field rather than as trailing prose.
//!
//! Codex is the one host that opens ADR-0002's first seam: it overrides
//! [`crate::Target::field_transforms`] with [`CodexTransforms`]. The codex subagent field-map
//! (`weaft_core::capability::CODEX_SUBAGENT_FIELDS`) flags `developer_instructions` with
//! `transform: true`; the transform forces that field to `None` so the value never comes from
//! a stray top-level frontmatter field, leaving the rendered body as the single source.
//!
//! ## Where the body injection happens (no double-injection)
//!
//! The body reaches the TOML in exactly one place: the `serialize` stage. The `map` stage and
//! its transform hook only ever see the *un-rendered* `artifact.body` (the map driver passes
//! `&Artifact`, not the rendered output), whereas `serialize::frame` receives the *rendered*
//! body — so only `frame` can carry the correct text. `TomlDocument::frame` (WU-13) injects the
//! rendered body under `developer_instructions` precisely when that key is absent from the mapped
//! fields. [`CodexTransforms`] therefore returns `None` for `developer_instructions`, keeping the
//! carrier key out of the mapped fields so `frame` always injects the rendered body. There is no
//! double-injection: if the field were present `frame` would skip injection, and if it is absent
//! (the case the transform guarantees) `frame` injects exactly once.

use crate::Target;
use serde_yaml_ng::Value;
use weaft_core::capability::{self, HostCapabilities};
use weaft_core::ir::Artifact;
use weaft_core::kind::ArtifactKind;
use weaft_core::pipeline::map::FieldTransformSet;

/// The codex subagent body-carrier field. Kept in sync with
/// `weaft_targets::serialize::toml_json`'s `TOML_BODY_CARRIER` and the codex subagent field-map:
/// the transform suppresses this field so the serializer injects the rendered body under it.
const DEVELOPER_INSTRUCTIONS: &str = "developer_instructions";

pub struct Codex;

impl Target for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn capabilities(&self) -> &'static HostCapabilities {
        &capability::CODEX
    }

    fn field_transforms(&self) -> &'static dyn FieldTransformSet {
        &CODEX_TRANSFORMS
    }
}

/// The shared codex transform set. Stateless, so a single static is borrowed by every emit.
static CODEX_TRANSFORMS: CodexTransforms = CodexTransforms;

/// Codex's field-value transform set (ADR-0002's first seam).
///
/// The only field codex flags `transform: true` is `developer_instructions`, the TOML
/// body-carrier. The transform drops it (returns `None`) so the body never comes from a stray
/// top-level frontmatter field; the rendered body is injected downstream by the TOML serializer
/// instead (see this module's docs). Every other field is `transform: false` and never reaches
/// this hook, so they flow through the generic map unchanged.
pub struct CodexTransforms;

impl FieldTransformSet for CodexTransforms {
    fn transform(
        &self,
        _host_id: &str,
        _kind: ArtifactKind,
        field: &str,
        current: Option<Value>,
        _artifact: &Artifact,
    ) -> Option<Value> {
        if field == DEVELOPER_INSTRUCTIONS {
            // Suppress the carrier field so `serialize::frame` injects the *rendered* body under
            // it (the only stage that sees the rendered body). A stray frontmatter value would
            // otherwise win over the rendered body and leak stale text.
            None
        } else {
            current
        }
    }
}
