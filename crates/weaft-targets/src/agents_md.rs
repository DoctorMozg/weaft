//! Generic AGENTS.md backend.
//!
//! A thin shell since the v2 migration: the host's layout, format, and disposition are
//! matrix data (`weaft_core::capability::AGENTS_MD`). Skills and instructions fold into a
//! single root `AGENTS.md` as plain-Markdown sections (`merge=true`, no frontmatter);
//! subagents resolve `Drop`, so the generic driver emits a warning and no file (the drop is
//! matrix-driven now, not a hardcoded skip here).
//!
//! ## Why this backend overrides `emit_artifact`
//!
//! v1 emitted each agents-md section as `format!("{}\n", rendered_body.trim_end())` — it
//! trimmed trailing whitespace and appended exactly one newline. The WU-13 `PlainMarkdown`
//! serializer is deliberately *verbatim* (`frame(PlainMarkdown, .., body) == body`), so the
//! trim cannot live in the shared framing without breaking its contract. It therefore lives
//! here, applied to this host's `PlainMarkdown` sections, reproducing the v1 bytes the
//! `hello` agents-md snapshot is pinned to.

use crate::{EmittedFile, Target};
use weaft_core::capability::{self, HostCapabilities};
use weaft_core::pipeline::emit::emit;
use weaft_core::serfmt::SerFormat;

pub struct AgentsMd;

impl Target for AgentsMd {
    fn id(&self) -> &'static str {
        "agents-md"
    }

    fn capabilities(&self) -> &'static HostCapabilities {
        &capability::AGENTS_MD
    }

    fn emit_artifact(
        &self,
        resolved: &weaft_core::pipeline::resolve::Resolved<'_>,
        name: &str,
        framed: String,
    ) -> EmittedFile {
        let spec = emit(resolved, name, framed);
        // Reproduce v1's per-section framing for the merged AGENTS.md: trim trailing
        // whitespace and append a single newline. Confined to PlainMarkdown sections so a
        // future frontmatter-bearing agents-md cell would not be silently trimmed.
        let contents = if resolved.cell.format == SerFormat::PlainMarkdown {
            format!("{}\n", spec.contents.trim_end())
        } else {
            spec.contents
        };
        EmittedFile {
            relative_path: spec.relative_path,
            contents: contents.into_bytes(),
            concatenate: spec.merge,
        }
    }
}
