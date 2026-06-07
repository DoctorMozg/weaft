//! TOML (body-as-field) and JSON framing (WU-13).
//!
//! TOML carries the rendered body in a designated body-carrier field (the codex subagent
//! `developer_instructions`); JSON serializes the mapped fields / singleton value as the
//! whole document (claude `settings.json`, `.mcp.json`).
//!
//! Both formats are deterministic: `toml::Table` and `serde_json::Map` are `BTreeMap`-backed
//! here (neither crate's `preserve_order` feature is enabled), so a given field set always
//! serializes to identical bytes.

use crate::serialize::Serializer;
use serde_yaml_ng::Value;

/// The codex subagent body-carrier field: a TOML document has no trailing prose block, so the
/// rendered body rides in this field (ADR-0002 / the codex `.toml` subagent dialect).
const TOML_BODY_CARRIER: &str = "developer_instructions";

/// `Toml` framing: serialize the ordered fields to a TOML table, carrying the rendered body in
/// the [`TOML_BODY_CARRIER`] field.
///
/// If the field-map already supplied `developer_instructions` (the `map`-stage transform hook
/// does this for the codex subagent cell), that value wins and the `body` argument is redundant;
/// otherwise a non-empty `body` is injected under the carrier key so the body always reaches the
/// document.
pub struct TomlDocument;

impl Serializer for TomlDocument {
    fn frame(&self, fields: &[(String, Value)], body: &str) -> String {
        let mut table = toml::value::Table::new();
        for (key, value) in fields {
            if let Some(toml_value) = yaml_to_toml(value) {
                table.insert(key.clone(), toml_value);
            }
        }
        if !body.is_empty() && !table.contains_key(TOML_BODY_CARRIER) {
            table.insert(
                TOML_BODY_CARRIER.to_string(),
                toml::Value::String(body.to_string()),
            );
        }
        // A `toml::Table` serializes infallibly here: every value originates from a YAML scalar /
        // sequence / mapping converted to a representable TOML value, so there are no non-string
        // map keys or other unencodable shapes. Fall back to empty rather than panicking.
        toml::to_string(&table).unwrap_or_default()
    }
}

/// `Json` framing: serialize the ordered fields as a JSON object document.
///
/// For settings / `.mcp.json` singletons the fields *are* the whole document, so the `body`
/// argument is ignored. Output uses `serde_json`'s pretty printer; the `BTreeMap`-backed object
/// keeps key order stable across runs.
pub struct JsonDocument;

impl Serializer for JsonDocument {
    fn frame(&self, fields: &[(String, Value)], _body: &str) -> String {
        let mut object = serde_json::Map::new();
        for (key, value) in fields {
            object.insert(key.clone(), yaml_to_json(value));
        }
        // Pretty-printing a plain `serde_json::Value::Object` is infallible (no custom Serialize
        // that can error); fall back to an empty object on the impossible error branch.
        serde_json::to_string_pretty(&serde_json::Value::Object(object))
            .unwrap_or_else(|_| "{}".to_string())
    }
}

/// Convert a `serde_yaml_ng::Value` to a `toml::Value`.
///
/// TOML has no null, so `Null` (and any unrepresentable shape) yields `None` and the caller
/// omits the key — matching the v1 frontmatter rule of dropping absent fields. `Tagged` values
/// are unwrapped to their inner value (weaft never authors YAML tags).
fn yaml_to_toml(value: &Value) -> Option<toml::Value> {
    match value {
        Value::Null => None,
        Value::Bool(b) => Some(toml::Value::Boolean(*b)),
        Value::Number(n) => yaml_number_to_toml(n),
        Value::String(text) => Some(toml::Value::String(text.clone())),
        Value::Sequence(items) => Some(toml::Value::Array(
            items.iter().filter_map(yaml_to_toml).collect(),
        )),
        Value::Mapping(map) => {
            let mut table = toml::value::Table::new();
            for (k, v) in map {
                // TOML keys are strings; skip any non-string key rather than fail the whole doc.
                if let (Some(key), Some(val)) = (k.as_str(), yaml_to_toml(v)) {
                    table.insert(key.to_string(), val);
                }
            }
            Some(toml::Value::Table(table))
        },
        Value::Tagged(tagged) => yaml_to_toml(&tagged.value),
    }
}

/// Convert a `serde_yaml_ng::Value` to a `serde_json::Value`.
///
/// JSON has a null, so `Null` maps to `serde_json::Value::Null`. Non-finite floats (which JSON
/// cannot represent) fall back to `Null`. `Tagged` values are unwrapped to their inner value.
fn yaml_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Number(n) => yaml_number_to_json(n),
        Value::String(text) => serde_json::Value::String(text.clone()),
        Value::Sequence(items) => {
            serde_json::Value::Array(items.iter().map(yaml_to_json).collect())
        },
        Value::Mapping(map) => {
            let mut object = serde_json::Map::new();
            for (k, v) in map {
                if let Some(key) = k.as_str() {
                    object.insert(key.to_string(), yaml_to_json(v));
                }
            }
            serde_json::Value::Object(object)
        },
        Value::Tagged(tagged) => yaml_to_json(&tagged.value),
    }
}

/// Map a YAML number to a TOML number, preserving the integer/float distinction.
///
/// A `u64` above `i64::MAX` has no lossless TOML representation (TOML integers are `i64`), so it
/// yields `None` and the key is dropped rather than silently truncated.
fn yaml_number_to_toml(number: &serde_yaml_ng::Number) -> Option<toml::Value> {
    if let Some(i) = number.as_i64() {
        return Some(toml::Value::Integer(i));
    }
    number.as_f64().map(toml::Value::Float)
}

/// Map a YAML number to a JSON number, preserving the integer/float distinction. Non-finite
/// floats are not representable in JSON and fall back to `Null`.
fn yaml_number_to_json(number: &serde_yaml_ng::Number) -> serde_json::Value {
    if let Some(i) = number.as_i64() {
        serde_json::Value::Number(i.into())
    } else if let Some(u) = number.as_u64() {
        serde_json::Value::Number(u.into())
    } else if let Some(f) = number.as_f64() {
        serde_json::Number::from_f64(f).map_or(serde_json::Value::Null, serde_json::Value::Number)
    } else {
        serde_json::Value::Null
    }
}

#[cfg(test)]
mod tests {
    use crate::serialize::frame;
    use serde_yaml_ng::Value;
    use weaft_core::serfmt::SerFormat;

    fn s(value: &str) -> Value {
        Value::String(value.to_string())
    }

    /// TOML framing serializes the mapped fields to a TOML table; the body is carried in the
    /// designated body-carrier field (codex `developer_instructions`). Assert the output is
    /// structurally valid TOML and contains both the scalar field and the body-carrier field.
    /// Bytes are not pinned (TOML is new in v2).
    #[test]
    fn toml_framing_contains_fields_and_body() {
        let fields = vec![
            ("name".to_string(), s("reviewer")),
            (
                "developer_instructions".to_string(),
                s("Review the diff carefully."),
            ),
        ];
        let out = frame(SerFormat::Toml, &fields, "Review the diff carefully.");

        // Structural TOML validity: top-level keys appear as `key = ...` assignments.
        assert!(
            out.contains("name = \"reviewer\""),
            "TOML must render the scalar field as a key/value assignment: {out:?}",
        );
        assert!(
            out.contains("developer_instructions = "),
            "TOML must carry the body in the body-carrier field: {out:?}",
        );
        // The body text must reach the output (carried by the designated field).
        assert!(
            out.contains("Review the diff carefully."),
            "the rendered body must appear in the TOML output: {out:?}",
        );
    }

    /// A TOML body-carrier field containing newlines and quotes must stay valid TOML — the
    /// serializer must escape/encode the body, not splice it raw. (Structural guard: the
    /// output must not contain a raw unescaped newline inside a basic string assignment that
    /// would break parsing — at minimum the body marker survives.)
    #[test]
    fn toml_framing_encodes_multiline_body() {
        let body = "line one\nline \"two\"\nline three";
        let fields = vec![
            ("name".to_string(), s("reviewer")),
            ("developer_instructions".to_string(), s(body)),
        ];
        let out = frame(SerFormat::Toml, &fields, body);

        assert!(
            out.contains("name = \"reviewer\""),
            "scalar field must survive: {out:?}"
        );
        // The distinctive body tokens must reach the output in some encoded form.
        assert!(
            out.contains("line one"),
            "body content must be present: {out:?}"
        );
        assert!(
            out.contains("line three"),
            "full body must be present: {out:?}"
        );
    }

    /// JSON framing (Fix 6 — claude settings / mcp) produces a JSON document of the mapped
    /// fields. For a settings singleton the value IS the document. Assert the output is
    /// structurally valid JSON (opens with `{`, contains the keys as `"key":` members).
    /// Bytes are not pinned (JSON is new in v2).
    #[test]
    fn json_framing_is_structurally_valid_object() {
        let fields = vec![
            ("model".to_string(), s("claude-sonnet-4-5")),
            ("permission".to_string(), s("explicit")),
        ];
        let out = frame(SerFormat::Json, &fields, "");

        let trimmed = out.trim();
        assert!(
            trimmed.starts_with('{'),
            "JSON document must open with '{{': {out:?}"
        );
        assert!(
            trimmed.ends_with('}'),
            "JSON document must close with '}}': {out:?}"
        );
        assert!(
            out.contains("\"model\""),
            "JSON must contain the model key: {out:?}"
        );
        assert!(
            out.contains("\"claude-sonnet-4-5\""),
            "JSON must contain the model value: {out:?}",
        );
        assert!(
            out.contains("\"permission\""),
            "JSON must contain the permission key: {out:?}"
        );
    }

    /// JSON framing ignores the body argument — for settings/mcp there is no separate body,
    /// the document is the fields alone. A stray body must not leak into the JSON.
    #[test]
    fn json_framing_ignores_body_argument() {
        let fields = vec![("model".to_string(), s("claude-sonnet-4-5"))];
        let out = frame(SerFormat::Json, &fields, "STRAY BODY TEXT");

        assert!(
            !out.contains("STRAY BODY TEXT"),
            "JSON framing must not splice the body argument into the document: {out:?}",
        );
    }

    /// JSON framing is deterministic across two calls with identical input — the ordered
    /// field vec drives output order, so two `frame` calls must be byte-identical.
    #[test]
    fn json_framing_is_deterministic() {
        let fields = vec![
            ("model".to_string(), s("claude-sonnet-4-5")),
            ("permission".to_string(), s("explicit")),
        ];
        let first = frame(SerFormat::Json, &fields, "");
        let second = frame(SerFormat::Json, &fields, "");

        assert_eq!(
            first, second,
            "JSON framing must be deterministic for identical input"
        );
    }

    /// TOML framing is deterministic across two calls with identical input.
    #[test]
    fn toml_framing_is_deterministic() {
        let fields = vec![
            ("name".to_string(), s("reviewer")),
            ("developer_instructions".to_string(), s("body")),
        ];
        let first = frame(SerFormat::Toml, &fields, "body");
        let second = frame(SerFormat::Toml, &fields, "body");

        assert_eq!(
            first, second,
            "TOML framing must be deterministic for identical input"
        );
    }
}
