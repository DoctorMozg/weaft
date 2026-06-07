//! Target backends: thin shells that name a host and carry its capability matrix.
//!
//! Since the v2 migration (WU-15) the per-host facts — file layout, frontmatter key
//! casing, drop/fold disposition, serialization format — live in the capability matrix
//! ([`weaft_core::capability`]) and the pure pipeline stages, not in the backends. A
//! backend now declares only its [`Target::id`] and [`Target::capabilities`]; the generic
//! [`Target::emit_artifact`] default turns a resolved cell + framed body into an
//! [`EmittedFile`] via the matrix layout. Two seams stay open for the rare host that needs
//! them (ADR-0002): [`Target::field_transforms`] (codex's TOML body field) and
//! [`Target::post_emit`].
//!
//! Emitted file paths are relative to `dist/<target-id>/`. When two emitted files share a
//! relative path (e.g. every skill maps to a single `AGENTS.md`), the build layer
//! concatenates them — see [`EmittedFile::concatenate`].

pub mod agents_md;
pub mod claude_code;
pub mod codex;
pub mod cursor;
pub mod gemini_cli;
pub mod opencode;
pub mod serialize;

use weaft_core::capability::HostCapabilities;
use weaft_core::pipeline::emit::emit;
use weaft_core::pipeline::map::{FieldTransformSet, NoTransforms};
use weaft_core::pipeline::resolve::Resolved;

pub use weaft_core::capability::{all, by_id};

/// A file produced by a target backend.
#[derive(Debug, Clone)]
pub struct EmittedFile {
    /// Path relative to `dist/<target-id>/`.
    pub relative_path: std::path::PathBuf,
    pub contents: Vec<u8>,
    /// If true, contents from multiple artifacts sharing this path are concatenated
    /// (used for the single-file `AGENTS.md` target). Binary/unique files set false.
    pub concatenate: bool,
}

impl EmittedFile {
    pub fn text(path: impl Into<std::path::PathBuf>, contents: String) -> Self {
        Self {
            relative_path: path.into(),
            contents: contents.into_bytes(),
            concatenate: false,
        }
    }

    pub fn section(path: impl Into<std::path::PathBuf>, contents: String) -> Self {
        Self {
            relative_path: path.into(),
            contents: contents.into_bytes(),
            concatenate: true,
        }
    }
}

/// The shared no-op transform set returned by [`Target::field_transforms`]'s default. Most
/// hosts need no value surgery, so they all borrow this one static (ADR-0002 point 1).
static NO_TRANSFORMS: NoTransforms = NoTransforms;

/// A compilation target / host backend.
///
/// A backend is intentionally thin: it names the host and exposes the host's capability
/// matrix. Everything format-specific — layout, frontmatter keys, disposition — is driven
/// by the matrix and the pure pipeline stages (resolve → map → serialize → emit). The two
/// default-provided seams below let a host opt into the rare exceptions.
pub trait Target {
    fn id(&self) -> &'static str;
    fn capabilities(&self) -> &'static HostCapabilities;

    /// The host's field-value transform set (ADR-0002's first seam). The default is a shared
    /// no-op: a field flagged `transform: true` in the matrix flows through the generic map
    /// unchanged. Only a host that needs value surgery (codex's TOML body field, WU-16)
    /// overrides this.
    fn field_transforms(&self) -> &'static dyn FieldTransformSet {
        &NO_TRANSFORMS
    }

    /// Emit one resolved artifact into an [`EmittedFile`] from its already-framed body.
    ///
    /// The default delegates to the pure [`emit`] stage — the relative path comes purely
    /// from the resolved cell's layout template (`{name}` substituted), and the `merge` flag
    /// becomes [`EmittedFile::concatenate`]. The framed body is carried through unchanged. A
    /// host whose framing needs a final adjustment (agents-md trims its `PlainMarkdown`
    /// sections) overrides this.
    fn emit_artifact(&self, resolved: &Resolved<'_>, name: &str, framed: String) -> EmittedFile {
        let spec = emit(resolved, name, framed);
        EmittedFile {
            relative_path: spec.relative_path,
            contents: spec.contents.into_bytes(),
            concatenate: spec.merge,
        }
    }

    /// A post-emission hook over all of this host's files (ADR-0002's second seam). The
    /// default is a no-op; a host that needs to rewrite or inject files after the per-artifact
    /// emit (none in scope today) overrides it.
    fn post_emit(&self, _files: &mut Vec<EmittedFile>) {}
}

static CLAUDE_CODE: claude_code::ClaudeCode = claude_code::ClaudeCode;
static CURSOR: cursor::Cursor = cursor::Cursor;
static AGENTS_MD: agents_md::AgentsMd = agents_md::AgentsMd;
static OPENCODE: opencode::Opencode = opencode::Opencode;
static CODEX: codex::Codex = codex::Codex;
static GEMINI_CLI: gemini_cli::GeminiCli = gemini_cli::GeminiCli;

/// Resolve a target backend by its id.
pub fn target_by_id(id: &str) -> Option<&'static dyn Target> {
    match id {
        "claude-code" => Some(&CLAUDE_CODE),
        "cursor" => Some(&CURSOR),
        "agents-md" => Some(&AGENTS_MD),
        "opencode" => Some(&OPENCODE),
        "codex" => Some(&CODEX),
        "gemini-cli" => Some(&GEMINI_CLI),
        _ => None,
    }
}

/// All target backends, in display order — the same order as [`weaft_core::capability::all`]
/// (ADR-0003 parallel slices, joined by id). gemini-cli is appended last (WU-17).
pub fn all_targets() -> Vec<&'static dyn Target> {
    vec![
        &CLAUDE_CODE,
        &CURSOR,
        &AGENTS_MD,
        &OPENCODE,
        &CODEX,
        &GEMINI_CLI,
    ]
}

/// WU-16: the opencode and codex backends + their registration (RED).
///
/// These tests pin the two new backends authored in a later GREEN step (WU-16): the `opencode`
/// and `codex` [`Target`] impls and their entries in [`target_by_id`] / [`all_targets`]. They
/// drive the *real* pipeline through the backend a host id resolves to — `resolve` →
/// [`map_fields`] → [`serialize::frame`] → [`Target::emit_artifact`] — so each one exercises the
/// backend's id, its `capabilities()` matrix cell, its `field_transforms()`, and the matrix-driven
/// emit path, not a stub.
///
/// RED is **behavioral**: today [`target_by_id`] returns `None` for both ids and [`all_targets`]
/// lists only the three v1 backends, so every `target_by_id("opencode"|"codex").expect(...)`
/// panics and the [`all_targets`] id-order assertion fails. They reach GREEN only once WU-16
/// registers both backends in the same order as [`weaft_core::capability::all`]. The tests must
/// not be softened by stubbing the backends or the registry.
///
/// Note: the two codex hard-byte cli tests in `weaft-cli/tests/cli_tests.rs` cover the
/// `merge_and_check` byte path; these are disjoint backend-behavior tests (registration, TOML
/// framing of the codex subagent, and the opencode/codex layout paths).
#[cfg(test)]
mod wu16_new_backend_tests {
    use super::{all_targets, target_by_id};
    use crate::serialize::frame;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use weaft_core::capability;
    use weaft_core::ir::{Artifact, ArtifactMeta, Targets};
    use weaft_core::kind::ArtifactKind;
    use weaft_core::pipeline::map::map_fields;
    use weaft_core::pipeline::resolve::resolve;

    /// A bare file-backed artifact of `kind` named `name`, with no per-target overrides and an
    /// optional set of top-level fields (the `FieldSource::TopLevel` read site used by subagent
    /// field-maps). `resolve` only reads `artifact.kind`; `map_fields` reads `name`/`description`
    /// structurally and any top-level `fields`.
    fn artifact(kind: ArtifactKind, name: &str, body: &str) -> Artifact {
        Artifact {
            kind,
            frontmatter: ArtifactMeta {
                name: name.to_string(),
                description: format!("{name} description"),
                targets: Targets::default(),
                kind_override: None,
                fields: BTreeMap::new(),
            },
            body: body.to_string(),
            source_path: PathBuf::from(format!("{}/{name}.md", kind.as_str())),
        }
    }

    /// Drive one artifact through the real pipeline for the backend `id` resolves to, returning
    /// the produced [`EmittedFile`]. This is the honest end-to-end path: it fails RED at the
    /// `target_by_id(...).expect(...)` boundary until the backend is registered (WU-16).
    fn emit_through_backend(id: &str, art: &Artifact) -> super::EmittedFile {
        let backend = target_by_id(id)
            .unwrap_or_else(|| panic!("WU-16: target_by_id({id:?}) must resolve to a backend"));
        let host = backend.capabilities();
        let resolved = resolve(art, host);
        let mapped = map_fields(&resolved, art, backend.field_transforms());
        let framed = frame(resolved.cell.format, &mapped, &art.body);
        backend.emit_artifact(&resolved, &art.frontmatter.name, framed)
    }

    #[test]
    fn target_by_id_resolves_opencode_backend() {
        // WU-16 registers the opencode backend; `target_by_id("opencode")` must return it and the
        // backend must report its own id (the registry key and the backend id must agree).
        let backend = target_by_id("opencode")
            .expect("WU-16: target_by_id(\"opencode\") must resolve to a backend");
        assert_eq!(
            backend.id(),
            "opencode",
            "the opencode backend must report id == \"opencode\"",
        );
        // Its capability matrix must be the opencode host const (the registry key joins the two
        // parallel slices by id — ADR-0003).
        assert_eq!(
            backend.capabilities().id,
            "opencode",
            "the opencode backend must carry the opencode capability matrix",
        );
    }

    #[test]
    fn target_by_id_resolves_codex_backend() {
        // WU-16 registers the codex backend; `target_by_id("codex")` must return it with id
        // "codex" and the codex capability matrix.
        let backend = target_by_id("codex")
            .expect("WU-16: target_by_id(\"codex\") must resolve to a backend");
        assert_eq!(
            backend.id(),
            "codex",
            "the codex backend must report id == \"codex\"",
        );
        assert_eq!(
            backend.capabilities().id,
            "codex",
            "the codex backend must carry the codex capability matrix",
        );
    }

    #[test]
    fn all_targets_matches_capability_all_order_including_new_backends() {
        // ADR-0003 parallel-slice ordering: the targets slice (`all_targets`) must list backends
        // in the SAME order as the core slice (`capability::all`). Once WU-16 lands opencode +
        // codex, both slices read [claude-code, cursor, agents-md, opencode, codex] (gemini-cli is
        // appended later by WU-17 to BOTH, so an exact id-sequence match stays forward-compatible).
        let target_ids: Vec<&'static str> = all_targets().iter().map(|t| t.id()).collect();
        let capability_ids: Vec<&'static str> = capability::all().iter().map(|h| h.id).collect();

        assert_eq!(
            target_ids, capability_ids,
            "all_targets() must list backend ids in the same order as capability::all() \
             (ADR-0003 parallel slices); opencode and codex are missing or mis-ordered until WU-16",
        );
        // Pin the two new ids explicitly so a regression that drops one is unambiguous.
        assert!(
            target_ids.contains(&"opencode"),
            "all_targets() must include the opencode backend; got {target_ids:?}",
        );
        assert!(
            target_ids.contains(&"codex"),
            "all_targets() must include the codex backend; got {target_ids:?}",
        );
    }

    #[test]
    fn codex_subagent_emits_valid_toml_with_body_under_developer_instructions() {
        // The headline codex behavior: a Subagent resolves to the codex `.toml` cell
        // (SerFormat::Toml), and the rendered body is carried in the `developer_instructions`
        // field of a VALID TOML document — not appended as trailing prose. We parse the framed
        // output with the `toml` crate (already a targets dependency) to assert real TOML validity,
        // then assert the carrier field holds the body and the structural `name` is present.
        let body = "Review the diff carefully and flag any destructive operations.";
        let art = artifact(ArtifactKind::Subagent, "code-reviewer", body);

        let emitted = emit_through_backend("codex", &art);

        // The emitted file lands at the codex subagent path and is its own (non-merged) file.
        assert_eq!(
            emitted.relative_path,
            Path::new(".codex/agents/code-reviewer.toml"),
            "codex Subagent must emit a `.codex/agents/<name>.toml` document",
        );

        let text = String::from_utf8(emitted.contents.clone())
            .expect("codex subagent output must be valid UTF-8");

        // Real structural validity: the output must parse as a TOML document.
        let parsed: toml::Table = toml::from_str(&text).unwrap_or_else(|e| {
            panic!("codex subagent output must be valid TOML: {e}\n---\n{text}")
        });

        // The rendered body must be carried under `developer_instructions`.
        let dev_instructions = parsed
            .get("developer_instructions")
            .and_then(toml::Value::as_str)
            .expect("codex subagent TOML must carry the body under `developer_instructions`");
        assert!(
            dev_instructions.contains("Review the diff carefully"),
            "the rendered body must reach the developer_instructions field; got {dev_instructions:?}",
        );

        // The structural `name` field maps through (the codex subagent field-map declares it).
        assert_eq!(
            parsed.get("name").and_then(toml::Value::as_str),
            Some("code-reviewer"),
            "codex subagent TOML must carry the structural `name` field",
        );
    }

    #[test]
    fn opencode_agent_emits_to_opencode_agent_path() {
        // opencode's Subagent cell layout is `.opencode/agent/<name>.md` (note the singular
        // `agent/` segment — distinct from the `agents/` used by claude/cursor). Drive a Subagent
        // through the opencode backend and assert the emitted relative path.
        let art = artifact(
            ArtifactKind::Subagent,
            "code-reviewer",
            "You review code.\n",
        );

        let emitted = emit_through_backend("opencode", &art);

        assert_eq!(
            emitted.relative_path,
            Path::new(".opencode/agent/code-reviewer.md"),
            "opencode Subagent must emit to `.opencode/agent/<name>.md` (singular `agent/`)",
        );
        assert!(
            !emitted.concatenate,
            "an opencode Subagent is its own file (merge=false), not a merged document section",
        );
    }

    #[test]
    fn codex_skill_emits_to_agent_skills_path() {
        // codex Skills follow the Agent Skills standard layout `.agents/skills/<name>/SKILL.md`
        // (NOT codex's own `.codex/` tree). Drive a Skill through the codex backend and assert the
        // emitted relative path comes from that matrix template.
        let art = artifact(ArtifactKind::Skill, "safe-deleter", "# Safe deleter\n");

        let emitted = emit_through_backend("codex", &art);

        assert_eq!(
            emitted.relative_path,
            Path::new(".agents/skills/safe-deleter/SKILL.md"),
            "codex Skill must emit to the Agent Skills `.agents/skills/<name>/SKILL.md` path",
        );
        assert!(
            !emitted.concatenate,
            "a codex Skill is its own file (merge=false)",
        );
    }

    #[test]
    fn opencode_skill_emits_to_opencode_skills_path() {
        // Symmetry guard for the opencode Skill cell: `.opencode/skills/<name>/SKILL.md`. Together
        // with the agent-path test this proves the opencode backend is driven entirely by its
        // matrix layout, not a hardcoded claude-style path.
        let art = artifact(ArtifactKind::Skill, "safe-deleter", "# Safe deleter\n");

        let emitted = emit_through_backend("opencode", &art);

        assert_eq!(
            emitted.relative_path,
            Path::new(".opencode/skills/safe-deleter/SKILL.md"),
            "opencode Skill must emit to `.opencode/skills/<name>/SKILL.md`",
        );
    }
}

/// WU-17: the gemini-cli backend + its registration in the parallel targets slice (RED).
///
/// gemini-cli is the conservative sixth host (all kind cells `Drop` — see the matrix tests in
/// `weaft-core::capability`). On the targets side it still needs a registered [`Target`] backend
/// so the core slice ([`weaft_core::capability::all`]) and the targets slice ([`all_targets`])
/// stay equal id-for-id and in order (ADR-0003) — that parity is what the WU-18 consistency test
/// in `weaft-cli` guards. These tests pin the backend authored in a later GREEN step (WU-17).
///
/// RED is **behavioral**: today [`target_by_id`] returns `None` for `"gemini-cli"` and
/// [`all_targets`] omits it, so the `expect` below panics and the trailing-position assertion
/// fails. They reach GREEN only once WU-17 registers the gemini-cli backend at the END of both
/// registration functions (after WU-16's opencode/codex). They must not be softened by stubbing
/// the backend or the registry.
#[cfg(test)]
mod wu17_gemini_cli_backend_tests {
    use super::{all_targets, target_by_id};
    use weaft_core::capability;

    #[test]
    fn target_by_id_resolves_gemini_cli_backend() {
        // WU-17 registers the gemini-cli backend; `target_by_id("gemini-cli")` must return it, the
        // backend must report id == "gemini-cli", and it must carry the gemini-cli capability matrix
        // (the registry key joins the two parallel slices by id — ADR-0003).
        let backend = target_by_id("gemini-cli")
            .expect("WU-17: target_by_id(\"gemini-cli\") must resolve to a backend");
        assert_eq!(
            backend.id(),
            "gemini-cli",
            "the gemini-cli backend must report id == \"gemini-cli\"",
        );
        assert_eq!(
            backend.capabilities().id,
            "gemini-cli",
            "the gemini-cli backend must carry the gemini-cli capability matrix",
        );
    }

    #[test]
    fn all_targets_appends_gemini_cli_last_in_parallel_with_capability_all() {
        // ADR-0003 parallel-slice ordering, extended to the sixth host: once WU-17 lands, both the
        // targets slice (`all_targets`) and the core slice (`capability::all`) must read
        // [claude-code, cursor, agents-md, opencode, codex, gemini-cli] — same ids, same order, with
        // gemini-cli last. An exact id-sequence equality pins both the membership and the ordering;
        // the explicit trailing-position assertion makes a "registered but mis-ordered" regression
        // unambiguous. (This is the targets-side half of what WU-18 checks across crates.)
        let target_ids: Vec<&'static str> = all_targets().iter().map(|t| t.id()).collect();
        let capability_ids: Vec<&'static str> = capability::all().iter().map(|h| h.id).collect();

        assert_eq!(
            target_ids, capability_ids,
            "all_targets() must list backend ids in the same order as capability::all() \
             (ADR-0003 parallel slices); gemini-cli is missing or mis-ordered until WU-17",
        );
        assert_eq!(
            target_ids.last().copied(),
            Some("gemini-cli"),
            "gemini-cli must be the LAST backend in all_targets() (appended after opencode/codex); \
             got {target_ids:?}",
        );
    }
}
