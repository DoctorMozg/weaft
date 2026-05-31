//! Ask-user lints. Flag artifacts that rely on a structured "ask the user" primitive on a
//! host where it is unavailable: inside a subagent (e.g. Claude's `AskUserQuestion`, which
//! is filtered out of subagents) or on a host with no question primitive at all. Both are
//! warnings — the built-in `ask` macro degrades to prose, but the author should know the
//! structured experience is lost for that target.

use crate::capability::AskUserSupport;
use crate::diag::Diagnostic;
use crate::ir::{Project, Targets};

const IN_SUBAGENT: &str = "weaft::lint::ask_user_in_subagent";
const UNSUPPORTED: &str = "weaft::lint::ask_user_unsupported";

pub fn check(project: &Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // (a) A subagent asks via a primitive its host cannot use inside a subagent.
    for agent in &project.agents {
        if !uses_ask(&agent.body) {
            continue;
        }
        for host in super::artifact_hosts(project, &agent.frontmatter.targets) {
            if host.ask_user_support != AskUserSupport::None && !host.ask_user_in_subagents {
                out.push(
                    Diagnostic::warning(
                        IN_SUBAGENT,
                        format!(
                            "subagent `{}` asks the user, but `{}` cannot use its question tool inside a subagent",
                            agent.frontmatter.name, host.id
                        ),
                    )
                    .with_artifact(agent.frontmatter.name.clone())
                    .with_help(
                        "weaft's `ask` macro auto-degrades to prose here; confirm the fallback reads well",
                    ),
                );
            }
        }
    }

    // (b) Any artifact asks on a host that has no structured question primitive at all.
    for (name, body, targets) in artifacts(project) {
        if !uses_ask(body) {
            continue;
        }
        for host in super::artifact_hosts(project, targets) {
            if host.ask_user_support == AskUserSupport::None {
                out.push(
                    Diagnostic::warning(
                        UNSUPPORTED,
                        format!(
                            "`{name}` asks the user, but `{}` has no structured question primitive",
                            host.id
                        ),
                    )
                    .with_artifact(name.to_string())
                    .with_help(
                        "the `ask` macro degrades to `host.ask_user_syntax` prose for this target",
                    ),
                );
            }
        }
    }

    out
}

/// Does the raw body rely on the ask-user primitive — via the built-in macro import or a
/// direct `host.ask_user_primitive` reference?
fn uses_ask(body: &str) -> bool {
    body.contains("weaft/ask.j2") || body.contains("ask_user_primitive")
}

/// Every artifact as `(name, body, targets)` — skills first, then agents.
fn artifacts(project: &Project) -> Vec<(&str, &str, &Targets)> {
    let mut v = Vec::new();
    for s in &project.skills {
        v.push((
            s.frontmatter.name.as_str(),
            s.body.as_str(),
            &s.frontmatter.targets,
        ));
    }
    for a in &project.agents {
        v.push((
            a.frontmatter.name.as_str(),
            a.body.as_str(),
            &a.frontmatter.targets,
        ));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Agent, AgentMeta, Meta, ProjectInfo, Skill, SkillMeta};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    const ASK: &str = "{% from \"weaft/ask.j2\" import ask %}{{ ask(host, \"q\") }}";

    fn targets(supported: &[&str]) -> Targets {
        Targets {
            supported: supported.iter().map(|s| (*s).to_string()).collect(),
            overrides: BTreeMap::new(),
        }
    }

    fn skill(name: &str, body: &str, supported: &[&str]) -> Skill {
        Skill {
            frontmatter: SkillMeta {
                name: name.into(),
                description: "d".into(),
                targets: targets(supported),
            },
            body: body.into(),
            source_path: PathBuf::from("x.md"),
        }
    }

    fn agent(name: &str, body: &str, supported: &[&str]) -> Agent {
        Agent {
            frontmatter: AgentMeta {
                name: name.into(),
                description: "d".into(),
                tools: Vec::new(),
                model: None,
                readonly: None,
                is_background: None,
                targets: targets(supported),
            },
            body: body.into(),
            source_path: PathBuf::from("x.md"),
        }
    }

    fn project(skills: Vec<Skill>, agents: Vec<Agent>) -> Project {
        Project {
            info: ProjectInfo {
                name: "p".into(),
                version: "0".into(),
                description: String::new(),
                meta: Meta::default(),
                targets: Targets::default(),
                parameters: BTreeMap::new(),
            },
            skills,
            agents,
            root: PathBuf::from("."),
        }
    }

    fn codes(diags: &[Diagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.code).collect()
    }

    #[test]
    fn detects_macro_and_primitive_usage() {
        assert!(uses_ask("{% from \"weaft/ask.j2\" import ask %}"));
        assert!(uses_ask("use {{ host.ask_user_primitive }} now"));
        assert!(!uses_ask("no ask here"));
    }

    #[test]
    fn warns_when_subagent_asks_on_claude() {
        // Claude's AskUserQuestion is unavailable to subagents → in_subagent warning.
        let p = project(vec![], vec![agent("rev", ASK, &["claude-code"])]);
        assert_eq!(codes(&check(&p)), vec![IN_SUBAGENT]);
    }

    #[test]
    fn warns_when_artifact_asks_on_host_without_primitive() {
        // agents-md has no question primitive → unsupported warning (skill, no subagent rule).
        let p = project(vec![skill("greet", ASK, &["agents-md"])], vec![]);
        assert_eq!(codes(&check(&p)), vec![UNSUPPORTED]);
    }

    #[test]
    fn no_warning_for_skill_on_claude_or_subagent_on_cursor() {
        // Skill on Claude: AskUserQuestion works in skills. Subagent on Cursor: the question
        // tool is available to subagents. Neither should warn.
        let p = project(
            vec![skill("greet", ASK, &["claude-code"])],
            vec![agent("rev", ASK, &["cursor"])],
        );
        assert!(check(&p).is_empty());
    }

    #[test]
    fn silent_when_artifact_does_not_ask() {
        let p = project(vec![skill("greet", "just prose", &["agents-md"])], vec![]);
        assert!(check(&p).is_empty());
    }
}
