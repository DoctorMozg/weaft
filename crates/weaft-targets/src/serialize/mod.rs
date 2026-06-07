//! The `serialize` stage (WU-13): combine mapped fields + rendered body into the final
//! emitted string per [`SerFormat`].
//!
//! Framing lives in `weaft-targets` because it pulls in serializer dependencies (`toml`,
//! `serde_json`) that ADR-0006 confines to the emission layer; core owns only the
//! [`SerFormat`] selector. The public entry point is `frame(format, fields, body)`, a
//! dispatcher over the per-format framing helpers in [`yaml`] and [`toml_json`].
//!
//! The framing must be byte-deterministic: the ordered `fields` slice (the `map` stage's
//! `MappedFields` output) drives emission order, so identical input yields identical output.

pub mod toml_json;
pub mod yaml;

use serde_yaml_ng::Value;
use weaft_core::serfmt::SerFormat;

/// Frame ordered mapped fields plus a rendered body into the final emitted string for one
/// [`SerFormat`].
///
/// `fields` is the `map` stage's ordered output (`MappedFields` =
/// `Vec<(String, serde_yaml_ng::Value)>`); its order is the emission order, so identical
/// input yields byte-identical output (C-DETERMINISM). The body's role depends on the format:
/// the YAML/`Mdc` framings append it after the frontmatter fence, `PlainMarkdown` emits it
/// alone, `Toml` carries it in a body-carrier field, and `Json` ignores it (the document is
/// the fields).
#[must_use]
pub fn frame(format: SerFormat, fields: &[(String, Value)], body: &str) -> String {
    let serializer: &dyn Serializer = match format {
        SerFormat::YamlFrontmatterMarkdown | SerFormat::Mdc => &yaml::YamlFrontmatter,
        SerFormat::PlainMarkdown => &yaml::PlainMarkdown,
        SerFormat::Toml => &toml_json::TomlDocument,
        SerFormat::Json => &toml_json::JsonDocument,
    };
    serializer.frame(fields, body)
}

/// A per-format framing strategy. Implementors own the rule for combining the ordered mapped
/// fields with the rendered body into the emitted bytes.
///
/// The trait lives in `weaft-targets` (not core) because framing is emission and the `Toml` /
/// `Json` impls pull in the `toml` / `serde_json` deps that ADR-0006 confines to this layer.
/// The selector — [`SerFormat`] — stays in core; [`frame`] maps each variant to its impl.
pub trait Serializer {
    /// Combine the ordered `fields` and the rendered `body` into the final emitted string.
    fn frame(&self, fields: &[(String, Value)], body: &str) -> String;
}

#[cfg(test)]
mod tests {
    use crate::serialize::frame;
    use serde_yaml_ng::Value;
    use weaft_core::serfmt::SerFormat;

    fn s(value: &str) -> Value {
        Value::String(value.to_string())
    }

    fn seq(items: &[&str]) -> Value {
        Value::Sequence(items.iter().map(|i| s(i)).collect())
    }

    /// The dispatcher routes `YamlFrontmatterMarkdown` to the YAML framing — the smoke test
    /// that `frame` exists and selects the YAML path (the byte-exactness is pinned in the
    /// `yaml` submodule).
    #[test]
    fn frame_dispatches_yaml_frontmatter() {
        let fields = vec![("name".to_string(), s("x"))];
        let out = frame(SerFormat::YamlFrontmatterMarkdown, &fields, "BODY");

        assert!(
            out.starts_with("---\n"),
            "YAML route must open a frontmatter fence: {out:?}"
        );
        assert!(
            out.ends_with("BODY"),
            "YAML route must keep the body: {out:?}"
        );
    }

    /// The dispatcher routes `PlainMarkdown` to the body-only framing (no fence).
    #[test]
    fn frame_dispatches_plain_markdown() {
        let fields = vec![("name".to_string(), s("x"))];
        let out = frame(SerFormat::PlainMarkdown, &fields, "BODY");

        assert_eq!(out, "BODY", "PlainMarkdown route must emit only the body");
    }

    /// The dispatcher routes `Json` to a JSON object document.
    #[test]
    fn frame_dispatches_json() {
        let fields = vec![("model".to_string(), s("claude-sonnet-4-5"))];
        let out = frame(SerFormat::Json, &fields, "");

        assert!(
            out.trim().starts_with('{'),
            "JSON route must produce an object: {out:?}"
        );
    }

    /// **Cross-format determinism.** For every `SerFormat`, two `frame` calls with identical
    /// input must produce byte-identical output (C-DETERMINISM). This is the single guard
    /// that no format introduces nondeterministic ordering (e.g. a `HashMap` walk).
    #[test]
    fn frame_is_deterministic_for_every_format() {
        let fields = vec![
            ("name".to_string(), s("x")),
            ("allowed-tools".to_string(), seq(&["Read", "Bash"])),
        ];
        let formats = [
            SerFormat::YamlFrontmatterMarkdown,
            SerFormat::Mdc,
            SerFormat::PlainMarkdown,
            SerFormat::Toml,
            SerFormat::Json,
        ];

        for format in formats {
            let first = frame(format, &fields, "BODY");
            let second = frame(format, &fields, "BODY");
            assert_eq!(
                first, second,
                "{format:?} framing must be byte-deterministic for identical input",
            );
        }
    }

    /// The dispatcher accepts a `&MappedFields` (a `Vec<(String, Value)>` slice) — the exact
    /// shape the `map` stage produces — confirming the public signature wires the pipeline
    /// together. (Compile-level contract: this calls `frame` with the map-stage output type.)
    #[test]
    fn frame_accepts_mapped_fields_slice() {
        let mapped: weaft_core::pipeline::map::MappedFields = vec![("name".to_string(), s("x"))];
        let out = frame(SerFormat::YamlFrontmatterMarkdown, &mapped, "BODY");

        assert!(
            out.contains("name: x"),
            "frame must consume the map-stage output: {out:?}"
        );
    }
}
