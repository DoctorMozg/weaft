//! Target backends: turn a rendered skill/subagent body into host-specific files.
//!
//! Each backend describes only the format-specific bits (frontmatter dialect, file
//! layout); the capability matrix lives in [`weaft_core::capability`]. Emitted file
//! paths are relative to `dist/<target-id>/`. When two emitted files share a relative
//! path (e.g. every skill maps to a single `AGENTS.md`), the build layer concatenates
//! them — see [`EmittedFile`].

pub mod agents_md;
pub mod claude_code;
pub mod cursor;
mod yaml;

use weaft_core::capability::HostCapabilities;
use weaft_core::diag::Diagnostic;
use weaft_core::ir::{AgentMeta, ProjectInfo, SkillMeta};

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

/// The result of emitting one artifact for one target.
#[derive(Debug, Clone, Default)]
pub struct EmitOutput {
    pub files: Vec<EmittedFile>,
    pub diagnostics: Vec<Diagnostic>,
}

impl EmitOutput {
    pub fn file(f: EmittedFile) -> Self {
        Self {
            files: vec![f],
            diagnostics: Vec::new(),
        }
    }

    pub fn skipped(diag: Diagnostic) -> Self {
        Self {
            files: Vec::new(),
            diagnostics: vec![diag],
        }
    }
}

/// A compilation target / host backend.
pub trait Target {
    fn id(&self) -> &'static str;
    fn capabilities(&self) -> &'static HostCapabilities;

    /// Emit a skill from its already-rendered body.
    fn emit_skill(
        &self,
        project: &ProjectInfo,
        skill: &SkillMeta,
        rendered_body: &str,
    ) -> EmitOutput;

    /// Emit a subagent from its already-rendered body. Backends whose host lacks a
    /// subagent concept return an empty [`EmitOutput`] with a warning diagnostic.
    fn emit_agent(
        &self,
        project: &ProjectInfo,
        agent: &AgentMeta,
        rendered_body: &str,
    ) -> EmitOutput;
}

static CLAUDE_CODE: claude_code::ClaudeCode = claude_code::ClaudeCode;
static CURSOR: cursor::Cursor = cursor::Cursor;
static AGENTS_MD: agents_md::AgentsMd = agents_md::AgentsMd;

/// Resolve a target backend by its id.
pub fn target_by_id(id: &str) -> Option<&'static dyn Target> {
    match id {
        "claude-code" => Some(&CLAUDE_CODE),
        "cursor" => Some(&CURSOR),
        "agents-md" => Some(&AGENTS_MD),
        _ => None,
    }
}

/// All target backends, in display order.
pub fn all_targets() -> Vec<&'static dyn Target> {
    vec![&CLAUDE_CODE, &CURSOR, &AGENTS_MD]
}
