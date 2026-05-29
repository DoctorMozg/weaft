//! Resolution of user-settable parameters.
//!
//! Parameters are declared in `weaft.yaml` (`[parameters]`), each with a type and
//! optional default. At build time the declared defaults are overridden by any
//! `--param key=value` flags, coerced to the declared type, and exposed to templates
//! as `{{ params.<name> }}`.

use crate::diag::WeftError;
use crate::ir::Parameter;
use serde::Serialize;
use std::collections::BTreeMap;

/// A resolved parameter value, serialized to templates as its native type.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum ParamValue {
    String(String),
    Bool(bool),
    Int(i64),
}

/// Map of fully-resolved parameter values (name -> value).
pub type ParamValues = BTreeMap<String, ParamValue>;

/// Parse a single `key=value` CLI override into its parts.
pub fn parse_override(raw: &str) -> Result<(String, String), WeftError> {
    match raw.split_once('=') {
        Some((k, v)) if !k.is_empty() => Ok((k.to_string(), v.to_string())),
        _ => Err(WeftError::BadParam {
            raw: raw.to_string(),
        }),
    }
}

/// Resolve declared parameters against CLI overrides.
///
/// Every declared parameter gets a value (its default, or the coerced override).
/// Overrides naming an undeclared parameter are still accepted as strings, so authors
/// can pass ad-hoc values templates reference directly.
pub fn resolve(
    declared: &BTreeMap<String, Parameter>,
    overrides: &[(String, String)],
) -> Result<ParamValues, WeftError> {
    let mut out = ParamValues::new();

    for (name, param) in declared {
        if let Some(default) = default_value(param) {
            out.insert(name.clone(), default);
        }
    }

    for (name, raw) in overrides {
        let value = match declared.get(name) {
            Some(param) => coerce(name, param, raw)?,
            None => ParamValue::String(raw.clone()),
        };
        out.insert(name.clone(), value);
    }

    Ok(out)
}

fn default_value(param: &Parameter) -> Option<ParamValue> {
    match param {
        Parameter::String { default, .. } => default.clone().map(ParamValue::String),
        Parameter::Bool { default, .. } => default.map(ParamValue::Bool),
        Parameter::Int { default, .. } => default.map(ParamValue::Int),
    }
}

fn coerce(name: &str, param: &Parameter, raw: &str) -> Result<ParamValue, WeftError> {
    match param {
        Parameter::String { .. } => Ok(ParamValue::String(raw.to_string())),
        Parameter::Bool { .. } => match raw {
            "true" | "1" | "yes" => Ok(ParamValue::Bool(true)),
            "false" | "0" | "no" => Ok(ParamValue::Bool(false)),
            _ => Err(WeftError::ParamType {
                name: name.to_string(),
                expected: "bool",
                got: raw.to_string(),
            }),
        },
        Parameter::Int { .. } => {
            raw.parse::<i64>()
                .map(ParamValue::Int)
                .map_err(|_| WeftError::ParamType {
                    name: name.to_string(),
                    expected: "int",
                    got: raw.to_string(),
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl() -> BTreeMap<String, Parameter> {
        let mut m = BTreeMap::new();
        m.insert(
            "project_name".to_string(),
            Parameter::String {
                default: Some("unknown".to_string()),
                description: None,
            },
        );
        m.insert(
            "strict".to_string(),
            Parameter::Bool {
                default: Some(false),
                description: None,
            },
        );
        m
    }

    #[test]
    fn defaults_applied() {
        let r = resolve(&decl(), &[]).unwrap();
        assert_eq!(r["project_name"], ParamValue::String("unknown".into()));
        assert_eq!(r["strict"], ParamValue::Bool(false));
    }

    #[test]
    fn override_coerces_bool() {
        let ov = vec![("strict".to_string(), "true".to_string())];
        let r = resolve(&decl(), &ov).unwrap();
        assert_eq!(r["strict"], ParamValue::Bool(true));
    }

    #[test]
    fn bad_bool_errors() {
        let ov = vec![("strict".to_string(), "maybe".to_string())];
        assert!(resolve(&decl(), &ov).is_err());
    }

    #[test]
    fn parse_override_splits() {
        assert_eq!(
            parse_override("a=b=c").unwrap(),
            ("a".to_string(), "b=c".to_string())
        );
        assert!(parse_override("nokey").is_err());
    }
}
