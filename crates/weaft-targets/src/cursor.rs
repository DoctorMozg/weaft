//! Cursor backend.
//!
//! A thin shell since the v2 migration: the host's layout, frontmatter keys, and
//! disposition are all matrix data (`weaft_core::capability::CURSOR`), so the generic
//! [`crate::Target::emit_artifact`] default produces the files. For reference, the matrix
//! declares: skills → `rules/<name>.mdc` with `description`/`globs`/`alwaysApply`
//! (camelCase, no `name`); subagents → `agents/<name>.md` with
//! `name`/`description`/`model`/`readonly`/`is_background`. Assets are not supported; the
//! build/lint layer warns when assets are present.

use crate::Target;
use weaft_core::capability::{self, HostCapabilities};

pub struct Cursor;

impl Target for Cursor {
    fn id(&self) -> &'static str {
        "cursor"
    }

    fn capabilities(&self) -> &'static HostCapabilities {
        &capability::CURSOR
    }
}
