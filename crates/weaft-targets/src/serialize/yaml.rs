//! YAML-frontmatter, `.mdc`, and plain-Markdown framing (WU-13).
//!
//! The framing here must reproduce the v1 `yaml::frontmatter()` bytes exactly so the
//! claude-code / cursor / agents-md snapshots stay byte-identical through the migration.
//! The byte format is the one v1's `crate::yaml::frontmatter` produced: a zero-indent block
//! sequence (`- Read`), `serde_yaml_ng`'s single-quoted globs, the `---`-fenced block, then a
//! single blank line before the body.

use crate::serialize::Serializer;
use serde_yaml_ng::{Mapping, Value};

/// `YamlFrontmatterMarkdown` / `Mdc` framing: a `---`-fenced YAML block built from the ordered
/// fields, then one blank line, then the rendered body.
///
/// Both formats share this shape (Cursor's `.mdc` is a YAML-frontmatter Markdown variant), so
/// one impl serves both [`weaft_core::serfmt::SerFormat`] variants.
pub struct YamlFrontmatter;

impl Serializer for YamlFrontmatter {
    fn frame(&self, fields: &[(String, Value)], body: &str) -> String {
        let fm = frontmatter_block(fields);
        // v1 joined the `---\n`-terminated block to the body with a single `\n`, yielding exactly
        // one blank line between the closing fence and the body. Reproduced byte-for-byte so the
        // claude-code / cursor snapshots stay identical through the migration.
        format!("{fm}\n{body}")
    }
}

/// `PlainMarkdown` framing: the rendered body verbatim, with no frontmatter fence.
///
/// This is the merged `AGENTS.md` / `CLAUDE.md` instruction shape; the fields are intentionally
/// ignored (the matching capability cells declare an empty field-map).
pub struct PlainMarkdown;

impl Serializer for PlainMarkdown {
    fn frame(&self, _fields: &[(String, Value)], body: &str) -> String {
        body.to_string()
    }
}

/// Build the `---\n<yaml>---\n` frontmatter block from ordered key/value pairs.
///
/// A [`Mapping`] preserves insertion order, so feeding the fields in `map`-stage order makes the
/// emitted key order equal to the field-map declaration order (C-DETERMINISM). This mirrors v1's
/// `crate::yaml::frontmatter` exactly — the bytes the existing snapshots are pinned to.
fn frontmatter_block(fields: &[(String, Value)]) -> String {
    let mut map = Mapping::new();
    for (key, value) in fields {
        map.insert(Value::String(key.clone()), value.clone());
    }
    let body = serde_yaml_ng::to_string(&Value::Mapping(map)).unwrap_or_default();
    format!("---\n{body}---\n")
}

#[cfg(test)]
mod tests {
    use crate::serialize::frame;
    use serde_yaml_ng::Value;
    use weaft_core::serfmt::SerFormat;

    /// A YAML string scalar — mirrors `yaml::s` so test data reads like the matrix output.
    fn s(value: &str) -> Value {
        Value::String(value.to_string())
    }

    /// A YAML sequence of strings.
    fn seq(items: &[&str]) -> Value {
        Value::Sequence(items.iter().map(|i| s(i)).collect())
    }

    /// The claude-code skill field set, in field-map order: `name`, then the recased
    /// `allowed-tools` carrying a `[Read, Bash]` sequence.
    fn claude_skill_fields() -> Vec<(String, Value)> {
        vec![
            ("name".to_string(), s("x")),
            ("allowed-tools".to_string(), seq(&["Read", "Bash"])),
        ]
    }

    /// **The keystone test.** YAML framing must be byte-identical to what the v1
    /// `yaml::frontmatter()` helper produced followed by `\n<body>` — the exact string the
    /// `safe-deleter` claude-code snapshot is built from. Pinned by running the real v1
    /// helper: `frontmatter()` emits `---\n<yaml>---\n` (a zero-indent block sequence), and
    /// the v1 backend joins it to the body with a single `\n`.
    #[test]
    fn yaml_framing_equals_v1_frontmatter_bytes() {
        let out = frame(
            SerFormat::YamlFrontmatterMarkdown,
            &claude_skill_fields(),
            "BODY",
        );

        // This literal is the verbatim output of the v1 `yaml::frontmatter()` helper plus
        // the backend's `format!("{fm}\n{body}")` join — captured from the live v1 code.
        let expected = "---\nname: x\nallowed-tools:\n- Read\n- Bash\n---\n\nBODY";
        assert_eq!(
            out, expected,
            "YAML framing must reproduce v1 bytes exactly"
        );
    }

    /// A single scalar field frames to `---\nname: x\n---\n\nBODY` — guards the newline
    /// layout (one blank line between the closing fence and the body) for the minimal case.
    #[test]
    fn yaml_framing_single_field_layout() {
        let fields = vec![("name".to_string(), s("x"))];
        let out = frame(SerFormat::YamlFrontmatterMarkdown, &fields, "BODY");

        assert_eq!(out, "---\nname: x\n---\n\nBODY");
    }

    /// The closing fence is followed by exactly one blank line, then the body verbatim — no
    /// trailing-newline normalization, no double blank line. (Regression guard against an
    /// off-by-one in the fence/body join.)
    #[test]
    fn yaml_framing_has_single_blank_line_before_body() {
        let fields = vec![("name".to_string(), s("x"))];
        let out = frame(
            SerFormat::YamlFrontmatterMarkdown,
            &fields,
            "# Heading\n\nText",
        );

        assert!(
            out.contains("---\n\n# Heading"),
            "expected exactly one blank line between closing fence and body, got: {out:?}",
        );
    }

    /// An empty field set still frames as a valid (empty) frontmatter block. serde_yaml_ng
    /// renders an empty mapping as `{}\n`, so the v1 helper yields `---\n{}\n---\n`; the
    /// framing must not panic and must keep the body.
    #[test]
    fn yaml_framing_empty_fields_keeps_body() {
        let fields: Vec<(String, Value)> = Vec::new();
        let out = frame(SerFormat::YamlFrontmatterMarkdown, &fields, "BODY");

        assert!(
            out.starts_with("---\n"),
            "must open a frontmatter fence: {out:?}"
        );
        assert!(
            out.ends_with("BODY"),
            "must preserve the body verbatim: {out:?}"
        );
    }

    /// Cursor `.mdc` framing carries the same YAML-frontmatter + body shape as the
    /// `safe-deleter` cursor snapshot: `description`, a single-quoted glob sequence, and the
    /// camelCase `alwaysApply` bool. Pinned from the live v1 helper — note serde_yaml_ng
    /// single-quotes `'**/*'` and renders `alwaysApply: false` inline.
    #[test]
    fn mdc_framing_matches_cursor_snapshot_shape() {
        let fields = vec![
            ("description".to_string(), s("desc")),
            ("globs".to_string(), seq(&["**/*"])),
            ("alwaysApply".to_string(), Value::Bool(false)),
        ];
        let out = frame(SerFormat::Mdc, &fields, "BODY");

        let expected = "---\ndescription: desc\nglobs:\n- '**/*'\nalwaysApply: false\n---\n\nBODY";
        assert_eq!(
            out, expected,
            "Mdc framing must reproduce v1 cursor bytes exactly"
        );
    }

    /// `.mdc` framing emits the `alwaysApply` key (not `always_apply`) and never injects a
    /// `name` key the caller did not provide — the recasing/key-set is the field-map's job,
    /// the serializer must pass the keys through verbatim.
    #[test]
    fn mdc_framing_preserves_provided_keys_only() {
        let fields = vec![
            ("description".to_string(), s("desc")),
            ("alwaysApply".to_string(), Value::Bool(true)),
        ];
        let out = frame(SerFormat::Mdc, &fields, "BODY");

        assert!(
            out.contains("alwaysApply: true"),
            "must emit camelCase key: {out:?}"
        );
        assert!(
            !out.contains("name:"),
            "must not inject a name key: {out:?}"
        );
    }

    /// PlainMarkdown framing emits the body with NO `---` frontmatter fence — the agents-md /
    /// instruction shape. Matches the `hello` agents-md snapshot, whose emitted content is the
    /// rendered body alone (no frontmatter).
    #[test]
    fn plain_markdown_emits_body_without_frontmatter() {
        let out = frame(SerFormat::PlainMarkdown, &claude_skill_fields(), "BODY");

        assert_eq!(out, "BODY", "PlainMarkdown must emit only the body");
        assert!(
            !out.contains("---"),
            "PlainMarkdown must not emit a frontmatter fence"
        );
    }

    /// PlainMarkdown ignores the fields entirely — agents-md has an empty field-map, but even
    /// if fields are passed they must not leak into the output.
    #[test]
    fn plain_markdown_ignores_fields() {
        let out = frame(
            SerFormat::PlainMarkdown,
            &claude_skill_fields(),
            "# Hello\n\nWorld",
        );

        assert_eq!(out, "# Hello\n\nWorld");
        assert!(
            !out.contains("allowed-tools"),
            "fields must not leak into PlainMarkdown"
        );
    }
}
