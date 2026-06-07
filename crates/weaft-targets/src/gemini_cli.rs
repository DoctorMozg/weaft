//! gemini-cli backend.
//!
//! The most conservative backend in the registry. gemini-cli is UNRESEARCHED, so its
//! capability matrix (`weaft_core::capability::GEMINI_CLI`) marks every one of the nine kind
//! cells `Drop` — the backend deliberately *under-claims*, emitting no file for any artifact
//! until the host's real layout and per-kind support are validated (RFD-0001
//! §C-OPEN-QUESTIONS). A build targeting only gemini-cli therefore produces zero files and one
//! drop-warning per declared artifact, never a broken emit.
//!
//! It still needs a registered [`Target`] purely so the core capability slice
//! (`weaft_core::capability::all`) and the targets slice ([`crate::all_targets`]) stay equal
//! id-for-id and in order (ADR-0003). Because nothing is ever emitted, the backend needs no
//! value transforms and no layout overrides: the generic [`crate::Target`] defaults suffice.

use crate::Target;
use weaft_core::capability::{self, HostCapabilities};

pub struct GeminiCli;

impl Target for GeminiCli {
    fn id(&self) -> &'static str {
        "gemini-cli"
    }

    fn capabilities(&self) -> &'static HostCapabilities {
        &capability::GEMINI_CLI
    }
}
