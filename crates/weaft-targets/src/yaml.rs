//! Small YAML helpers shared by the backends: build a `---`-fenced frontmatter block
//! with deterministic key order, and read typed values out of a per-target override
//! block (`serde_yaml_ng::Value`).

use serde_yaml_ng::Value;

/// Build a `---\n<yaml>---\n` frontmatter block from ordered key/value pairs.
/// Pairs whose value is `None` are omitted, so optional fields stay absent.
pub fn frontmatter(pairs: Vec<(&str, Option<Value>)>) -> String {
    let mut map = serde_yaml_ng::Mapping::new();
    for (k, v) in pairs {
        if let Some(value) = v {
            map.insert(Value::String(k.to_string()), value);
        }
    }
    let body = serde_yaml_ng::to_string(&Value::Mapping(map)).unwrap_or_default();
    format!("---\n{body}---\n")
}

/// Look up a string field in an override mapping.
pub fn get_str(overrides: Option<&Value>, key: &str) -> Option<String> {
    overrides?
        .as_mapping()?
        .get(Value::String(key.to_string()))?
        .as_str()
        .map(str::to_string)
}

/// Look up a bool field in an override mapping.
pub fn get_bool(overrides: Option<&Value>, key: &str) -> Option<bool> {
    overrides?
        .as_mapping()?
        .get(Value::String(key.to_string()))?
        .as_bool()
}

/// Look up a sequence-of-strings field in an override mapping.
pub fn get_str_seq(overrides: Option<&Value>, key: &str) -> Option<Vec<String>> {
    let seq = overrides?
        .as_mapping()?
        .get(Value::String(key.to_string()))?
        .as_sequence()?;
    Some(
        seq.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
    )
}

/// A YAML string scalar value.
pub fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}

/// A YAML sequence of strings.
pub fn seq(items: &[String]) -> Value {
    Value::Sequence(items.iter().map(|i| Value::String(i.clone())).collect())
}
