//! Serialization-format selector (WU-4).
//!
//! [`SerFormat`] is the matrix-side *selector* — a capability cell names the format it
//! emits in. The framing *impls* (which pull in `toml`/`serde_json`) live in
//! `weaft-targets`, since framing is emission (ADR-0006); core owns only this enum so the
//! choice is reachable as matrix data and surfaces in `{{ host.* }}` introspection.

use serde::Serialize;

/// How an artifact's mapped fields and rendered body are framed into the emitted file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SerFormat {
    /// Fenced YAML frontmatter (`---\n...\n---`) followed by the rendered Markdown body —
    /// the Claude/opencode skill and subagent shape.
    YamlFrontmatterMarkdown,
    /// Cursor's `.mdc` rule format: a YAML-frontmatter Markdown variant Cursor reads as a
    /// rule file. Kept distinct so the matrix can name the `.mdc` extension and dialect.
    Mdc,
    /// Plain Markdown with no frontmatter block — the merged `AGENTS.md` / `CLAUDE.md`
    /// instruction shape.
    PlainMarkdown,
    /// A TOML document where the rendered body is carried as a field (Codex subagent
    /// `developer_instructions`), not as a trailing prose block.
    Toml,
    /// A JSON document — the whole emitted file is the serialized singleton value
    /// (`settings.json`, `.mcp.json`); there is no separate body.
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each variant paired with the snake_case label it must serialize to. These labels
    /// surface in `{{ host.* }}` introspection, so they are a stable contract.
    const CASES: [(SerFormat, &str); 5] = [
        (
            SerFormat::YamlFrontmatterMarkdown,
            "yaml_frontmatter_markdown",
        ),
        (SerFormat::Mdc, "mdc"),
        (SerFormat::PlainMarkdown, "plain_markdown"),
        (SerFormat::Toml, "toml"),
        (SerFormat::Json, "json"),
    ];

    #[test]
    fn serializes_to_snake_case_labels() {
        for (variant, label) in CASES {
            // A bare enum variant serializes to a YAML scalar string; the serde rename is
            // what makes the matrix label deterministic in `{{ host.* }}` introspection.
            let yaml = serde_yaml_ng::to_string(&variant)
                .expect("SerFormat must serialize")
                .trim()
                .to_owned();
            assert_eq!(
                yaml, label,
                "{variant:?} must serialize to the snake_case label {label:?}",
            );
        }
    }

    #[test]
    fn every_variant_serializes_to_a_distinct_label() {
        // No two formats may collapse to the same wire string.
        let mut labels: Vec<String> = CASES
            .iter()
            .map(|(variant, _)| {
                serde_yaml_ng::to_string(variant)
                    .expect("serialize")
                    .trim()
                    .to_owned()
            })
            .collect();
        labels.sort();
        let count = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), count, "SerFormat labels must be unique");
    }
}
