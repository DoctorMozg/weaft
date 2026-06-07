//! Heuristic lint over the raw template source: parameters that are declared but never
//! referenced. A warning.

use crate::diag::Diagnostic;
use crate::ir::Project;

const UNUSED: &str = "weaft::lint::unused_parameter";

pub fn check(project: &Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // Concatenate every artifact body once; parameter references are project-wide. Singleton
    // config artifacts carry an empty body, so they contribute nothing (WU-19 generic iteration).
    let mut all_bodies = String::new();
    for artifact in &project.artifacts {
        all_bodies.push_str(&artifact.body);
        all_bodies.push('\n');
    }

    for name in project.info.parameters.keys() {
        if !references_param(&all_bodies, name) {
            out.push(
                Diagnostic::warning(
                    UNUSED,
                    format!("parameter `{name}` is declared but never referenced"),
                )
                .with_help(format!(
                    "reference it as {{{{ params.{name} }}}} or remove it"
                )),
            );
        }
    }

    out
}

/// Does the source reference `params.<name>` (allowing whitespace after `params.`)?
fn references_param(src: &str, name: &str) -> bool {
    let needle = format!("params.{name}");
    src.match_indices(&needle).any(|(idx, _)| {
        // Ensure the match isn't a prefix of a longer identifier (params.foo vs foobar).
        let after = idx + needle.len();
        src[after..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_param_reference() {
        assert!(references_param("run {{ params.cmd }}", "cmd"));
        assert!(!references_param("run {{ params.cmdline }}", "cmd"));
        assert!(!references_param("no refs here", "cmd"));
    }
}
