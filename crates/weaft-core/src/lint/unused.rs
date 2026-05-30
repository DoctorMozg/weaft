//! Heuristic lints over the raw template source: unused parameters and skills that
//! ship no example. Both are warnings.

use crate::diag::Diagnostic;
use crate::ir::Project;

const UNUSED: &str = "weaft::lint::unused_parameter";
const MISSING_EXAMPLE: &str = "weaft::lint::missing_example";

pub fn check(project: &Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // Concatenate every body once; parameter references are project-wide.
    let mut all_bodies = String::new();
    for skill in &project.skills {
        all_bodies.push_str(&skill.body);
        all_bodies.push('\n');
    }
    for agent in &project.agents {
        all_bodies.push_str(&agent.body);
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

    for skill in &project.skills {
        if !has_example_heading(&skill.body) {
            out.push(
                Diagnostic::warning(
                    MISSING_EXAMPLE,
                    "skill has no `## Example` / `### Example` heading",
                )
                .with_artifact(skill.frontmatter.name.clone())
                .with_help("concrete examples markedly improve skill reliability"),
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

/// Heuristic: any Markdown heading line whose text contains "example".
fn has_example_heading(body: &str) -> bool {
    body.lines().any(|line| {
        let l = line.trim_start();
        l.starts_with('#') && l.to_ascii_lowercase().contains("example")
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

    #[test]
    fn detects_example_heading() {
        assert!(has_example_heading("## Examples\nfoo"));
        assert!(has_example_heading("### Example: x"));
        assert!(!has_example_heading("## Usage"));
    }
}
