//! Artifact kind taxonomy (WU-1).
//!
//! [`ArtifactKind`] is the closed set of things weaft compiles. It is *closed* on purpose:
//! the unbounded variation between hosts is carried as opaque fields on the capability
//! matrix and the artifact frontmatter, never as new kinds. Extending this set is a
//! governance act (an ADR), not a casual code change, because every pipeline stage matches
//! it exhaustively and a new variant ripples through all of them.
//!
//! Five kinds are *prose* — they are discovered by the folder a source file lives in
//! ([`ArtifactKind::from_folder`]). The remaining four are *manifest-only* singleton config
//! declared in `weaft.yaml`; they are never reached by folder lookup.

use serde::{Deserialize, Serialize};

/// The closed set of artifact kinds weaft compiles.
///
/// Variants are exhaustively matched throughout the pipeline, so this enum is deliberately
/// *not* `#[non_exhaustive]`: a wildcard arm would silently swallow a newly added kind and
/// defeat the compile-time check that every stage handles every kind.
///
/// `Deserialize` is derived so a `kind:` frontmatter override (WU-5's `ArtifactMeta`) parses
/// straight into this closed set — an out-of-set value is a parse error, not a new kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Instruction,
    Skill,
    Subagent,
    Command,
    McpServer,
    Settings,
    Plugin,
    Hook,
    Ignore,
}

/// Stable declaration order; also the source of [`ArtifactKind::index`] slots, so the order
/// here is the key for every array-backed capability table. Do not reorder without updating
/// any persisted index assumptions.
const ALL: [ArtifactKind; 9] = [
    ArtifactKind::Instruction,
    ArtifactKind::Skill,
    ArtifactKind::Subagent,
    ArtifactKind::Command,
    ArtifactKind::McpServer,
    ArtifactKind::Settings,
    ArtifactKind::Plugin,
    ArtifactKind::Hook,
    ArtifactKind::Ignore,
];

impl ArtifactKind {
    /// The stable string label. For the five prose kinds this is exactly the source folder
    /// name, so `as_str` round-trips through [`ArtifactKind::from_folder`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ArtifactKind::Instruction => "instructions",
            ArtifactKind::Skill => "skills",
            ArtifactKind::Subagent => "agents",
            ArtifactKind::Command => "commands",
            ArtifactKind::McpServer => "mcp_server",
            ArtifactKind::Settings => "settings",
            ArtifactKind::Plugin => "plugin",
            ArtifactKind::Hook => "hooks",
            ArtifactKind::Ignore => "ignore",
        }
    }

    /// The canonical snake_case identifier for this kind — the same string the derived
    /// `Serialize` emits (`Subagent` → `"subagent"`). Distinct from [`ArtifactKind::as_str`],
    /// which returns the *folder* name (`Subagent` → `"agents"`). This is the template-facing
    /// key, e.g. the `host.supports.<kind>` map keys built in `compile.rs`. The exhaustive
    /// match keeps it in lockstep with the `#[serde(rename_all = "snake_case")]` derive.
    #[must_use]
    pub const fn serde_name(self) -> &'static str {
        match self {
            ArtifactKind::Instruction => "instruction",
            ArtifactKind::Skill => "skill",
            ArtifactKind::Subagent => "subagent",
            ArtifactKind::Command => "command",
            ArtifactKind::McpServer => "mcp_server",
            ArtifactKind::Settings => "settings",
            ArtifactKind::Plugin => "plugin",
            ArtifactKind::Hook => "hook",
            ArtifactKind::Ignore => "ignore",
        }
    }

    /// Stable index in `0..9`, used as the key into array-backed capability tables. The
    /// mapping is a bijection onto `0..9` (see [`ArtifactKind::all`] order).
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            ArtifactKind::Instruction => 0,
            ArtifactKind::Skill => 1,
            ArtifactKind::Subagent => 2,
            ArtifactKind::Command => 3,
            ArtifactKind::McpServer => 4,
            ArtifactKind::Settings => 5,
            ArtifactKind::Plugin => 6,
            ArtifactKind::Hook => 7,
            ArtifactKind::Ignore => 8,
        }
    }

    /// Map a source *folder* name to its kind. Only the five prose kinds are folder-backed;
    /// the four manifest-only kinds (and any unknown name) return `None`.
    #[must_use]
    pub fn from_folder(name: &str) -> Option<Self> {
        match name {
            "skills" => Some(ArtifactKind::Skill),
            "agents" => Some(ArtifactKind::Subagent),
            "instructions" => Some(ArtifactKind::Instruction),
            "commands" => Some(ArtifactKind::Command),
            "hooks" => Some(ArtifactKind::Hook),
            _ => None,
        }
    }

    /// Every kind in stable declaration order.
    #[must_use]
    pub const fn all() -> &'static [ArtifactKind; 9] {
        &ALL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The five prose kinds are the only ones a source *folder* maps to. The four
    /// manifest-only kinds (mcp_server / settings / plugin / ignore) are singleton config
    /// and must NOT be reachable through folder lookup.
    const PROSE_FOLDERS: [(&str, ArtifactKind); 5] = [
        ("skills", ArtifactKind::Skill),
        ("agents", ArtifactKind::Subagent),
        ("instructions", ArtifactKind::Instruction),
        ("commands", ArtifactKind::Command),
        ("hooks", ArtifactKind::Hook),
    ];

    #[test]
    fn from_folder_maps_each_prose_folder() {
        for (folder, expected) in PROSE_FOLDERS {
            assert_eq!(
                ArtifactKind::from_folder(folder),
                Some(expected),
                "folder {folder:?} should map to {expected:?}",
            );
        }
    }

    #[test]
    fn from_folder_rejects_manifest_only_kinds() {
        // These kinds are declared in weaft.yaml, never discovered by folder name.
        for folder in ["mcp_server", "settings", "plugin", "ignore"] {
            assert_eq!(
                ArtifactKind::from_folder(folder),
                None,
                "manifest-only folder {folder:?} must not resolve to a kind",
            );
        }
    }

    #[test]
    fn from_folder_rejects_unknown_folder() {
        assert_eq!(ArtifactKind::from_folder("not_a_folder"), None);
        assert_eq!(ArtifactKind::from_folder(""), None);
    }

    #[test]
    fn all_returns_exactly_nine_kinds() {
        assert_eq!(ArtifactKind::all().len(), 9);
    }

    #[test]
    fn all_contains_no_duplicates() {
        let kinds = ArtifactKind::all();
        for (i, a) in kinds.iter().enumerate() {
            for b in &kinds[i + 1..] {
                assert_ne!(a, b, "ArtifactKind::all() must not contain duplicates");
            }
        }
    }

    #[test]
    fn all_contains_every_named_variant() {
        // Pin the closed nine-variant set so adding/removing a variant breaks this test
        // (extending the set is a governance act per C-ARTIFACT-KINDS).
        let expected = [
            ArtifactKind::Instruction,
            ArtifactKind::Skill,
            ArtifactKind::Subagent,
            ArtifactKind::Command,
            ArtifactKind::McpServer,
            ArtifactKind::Settings,
            ArtifactKind::Plugin,
            ArtifactKind::Hook,
            ArtifactKind::Ignore,
        ];
        let actual = ArtifactKind::all();
        for kind in expected {
            assert!(
                actual.contains(&kind),
                "ArtifactKind::all() is missing {kind:?}",
            );
        }
    }

    #[test]
    fn index_is_a_bijection_onto_zero_to_nine() {
        // Every kind has a distinct index, and the indices cover exactly 0..9 — the
        // determinism-friendly key for the array-backed capability table (WU-2).
        let mut seen = [false; 9];
        for kind in ArtifactKind::all() {
            let i = kind.index();
            assert!(i < 9, "index {i} for {kind:?} is out of the 0..9 range");
            assert!(
                !seen[i],
                "index {i} produced by {kind:?} collides with another kind",
            );
            seen[i] = true;
        }
        assert!(
            seen.iter().all(|&hit| hit),
            "index() must surject onto every slot in 0..9",
        );
    }

    #[test]
    fn as_str_roundtrips_through_from_folder_for_prose_kinds() {
        // For the five folder-backed kinds, the string label must be exactly the folder
        // name, so as_str -> from_folder is the identity.
        for (folder, kind) in PROSE_FOLDERS {
            assert_eq!(
                kind.as_str(),
                folder,
                "{kind:?}.as_str() should equal its source folder name",
            );
            assert_eq!(
                ArtifactKind::from_folder(kind.as_str()),
                Some(kind),
                "as_str -> from_folder must round-trip for {kind:?}",
            );
        }
    }
}
