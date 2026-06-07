//! Token-budget lint. Renders each present artifact for each supported host and compares the
//! token count against that `(host, kind)` cell's soft token budget. Also surfaces render
//! failures (including undefined `host.*` references caught by strict-undefined mode) as hard
//! errors.
//!
//! Core keeps **only** the soft-token branch (WU-19 / Fix 2): the hard-byte cap needs the merged
//! emitted bytes, which core cannot see, so it is enforced in `weaft-cli/merge_and_check`. The
//! per-artifact gate is the kind-cell disposition — an artifact a host drops (e.g. an agents-md
//! subagent) is skipped here, exactly as it is at emit time — not the legacy `supports_subagents`
//! bool.
//!
//! Budgets are weaft heuristics, not host-documented limits (see `capability.rs`).

use crate::budget::{Budget, BudgetUnit};
use crate::capability::Disposition;
use crate::diag::Diagnostic;
use crate::ir::Project;
use crate::pipeline::resolve::resolve;
use crate::{compile, params, tokens};

const BUDGET: &str = "weaft::lint::token_budget";
const RENDER: &str = "weaft::lint::render";

/// Fraction of the budget at which a warning fires.
const WARN_AT: f64 = 0.80;

pub fn check(project: &Project, strict: bool) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // Lint renders with declared defaults; CLI build may override with --param.
    let resolved = match params::resolve(&project.info.parameters, &[]) {
        Ok(p) => p,
        Err(e) => {
            out.push(Diagnostic::error(RENDER, e.to_string()));
            return out;
        },
    };

    for artifact in &project.artifacts {
        for host in super::artifact_hosts(project, &artifact.frontmatter.targets) {
            let cell = resolve(artifact, host);
            // A dropped kind emits no file, so it is not a budget concern (disposition gate
            // replaces the v1 `supports_subagents` skip).
            if cell.disposition == Disposition::Drop {
                continue;
            }
            // Only the soft-token branch lives in core; a `Bytes`+`Hard` cell (codex AGENTS.md)
            // is checked on the merged file in `weaft-cli`, not here (Fix 2).
            let Some(budget) = soft_token_budget(cell.cell.budget) else {
                continue;
            };
            let name = &artifact.frontmatter.name;
            match compile::render(artifact, &project.info, host, &resolved, &project.root) {
                Ok(body) => out.extend(check_one(name, host.id, &budget, &body, strict)),
                Err(e) => out.push(render_error(name, host.id, &e)),
            }
        }
    }

    out
}

/// The cell's budget if it is a soft *token* budget, else `None`. Byte budgets (checked in the
/// cli merge stage) and hard budgets are not the core lint's concern.
fn soft_token_budget(budget: Option<Budget>) -> Option<Budget> {
    let budget = budget?;
    matches!(budget.unit, BudgetUnit::Tokens(_)).then_some(budget)
}

/// Build the soft-token diagnostic for one rendered body, or `None` if it is comfortably within
/// budget. Over budget → an error under `--strict`, a warning otherwise; in the 80–100% band →
/// an advisory "approaching budget" warning.
fn check_one(
    name: &str,
    host_id: &str,
    budget: &Budget,
    body: &str,
    strict: bool,
) -> Option<Diagnostic> {
    let BudgetUnit::Tokens(tokenizer) = budget.unit else {
        return None; // not a token budget; nothing for the soft-token lint to do
    };
    let limit = budget.limit;
    let count = tokens::count(body, tokenizer);
    let pct = count as f64 / limit as f64;
    let artifact = format!("{name} → {host_id}");

    if count > limit {
        let msg = format!(
            "{count}/{limit} tokens ({:.0}%) — over the {host_id} budget",
            pct * 100.0
        );
        let diag = if strict {
            Diagnostic::error(BUDGET, msg)
        } else {
            Diagnostic::warning(BUDGET, msg)
        };
        Some(
            diag.with_artifact(artifact)
                .with_help("trim the body or move detail into assets/ (Claude) or fragments"),
        )
    } else if pct >= WARN_AT {
        Some(
            Diagnostic::warning(
                BUDGET,
                format!(
                    "{count}/{limit} tokens ({:.0}%) — approaching budget",
                    pct * 100.0
                ),
            )
            .with_artifact(artifact),
        )
    } else {
        None
    }
}

fn render_error(name: &str, host_id: &str, e: &crate::diag::WeftError) -> Diagnostic {
    Diagnostic::error(RENDER, format!("{e}"))
        .with_artifact(format!("{name} → {host_id}"))
        .with_help("check `host.*` references and `{% include %}` paths")
}

/// WU-19: soft-token-only budget lint in core (warn without `--strict`, error with).
///
/// WU-19 keeps **only** the soft-token branch in core (the hard-byte branch moves to
/// `weaft-cli/merge_and_check`, Fix 2) and switches the per-artifact gate from the legacy
/// `supports_subagents` bool to the kind-cell disposition. For the *shipped* hosts the soft-token
/// warn/error contract is unchanged by that refactor (claude-code's Skill cell carries the same
/// 8000-token soft budget the v1 `max_skill_tokens` did), so these tests are **GREEN-now-by-design
/// regression guards**: they must keep passing through the WU-19 GREEN refactor, proving it did
/// not drop, weaken, or invert the soft-token severity contract while removing the byte logic.
///
/// Constructed RED-resistant on purpose (the dispatch: "Where current code already does the right
/// thing, make the test stricter"): they pin the *exact* severity flip across `--strict` plus the
/// over-budget artifact name, so a refactor that, say, always errors, never warns, or stops
/// checking skills would fail.
#[cfg(test)]
mod wu19_soft_token_tests {
    use super::check;
    use crate::diag::{Diagnostic, Severity};
    use crate::ir::{Artifact, Meta, Project, ProjectInfo, Skill, SkillMeta, Targets};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    const BUDGET_CODE: &str = "weaft::lint::token_budget";

    /// A body comfortably over the claude-code 8000-token soft budget. cl100k counts most words
    /// as ~1 token, so ~12k whitespace-separated words is safely over 8000 tokens.
    fn over_budget_body() -> String {
        let mut s = String::with_capacity(120_000);
        for i in 0..12_000 {
            s.push_str("word");
            s.push_str(&i.to_string());
            s.push(' ');
        }
        s
    }

    fn skill(name: &str, body: &str, supported: &[&str]) -> Skill {
        Skill {
            frontmatter: SkillMeta {
                name: name.into(),
                description: "d".into(),
                targets: Targets {
                    supported: supported.iter().map(|s| (*s).to_string()).collect(),
                    overrides: BTreeMap::new(),
                },
            },
            body: body.into(),
            source_path: PathBuf::from(format!("skills/{name}.md")),
        }
    }

    fn project(skills: Vec<Skill>) -> Project {
        Project {
            info: ProjectInfo {
                name: "p".into(),
                version: "0".into(),
                description: String::new(),
                meta: Meta::default(),
                targets: Targets::default(),
                parameters: BTreeMap::new(),
                settings: None,
                mcp_servers: BTreeMap::new(),
                ignore: Vec::new(),
                plugin: None,
            },
            artifacts: skills.into_iter().map(Artifact::from_skill).collect(),
            root: PathBuf::from("/weaft-nonexistent-root"),
        }
    }

    /// The budget diagnostics for the over-budget skill (filters out any approaching-budget noise
    /// from other artifacts by matching the artifact tag the budget lint attaches: `name → host`).
    fn over_budget_diags<'a>(diags: &'a [Diagnostic], name: &str) -> Vec<&'a Diagnostic> {
        diags
            .iter()
            .filter(|d| d.code == BUDGET_CODE)
            .filter(|d| d.artifact.as_deref().is_some_and(|a| a.contains(name)))
            .collect()
    }

    #[test]
    fn soft_token_overflow_warns_without_strict() {
        // An over-budget skill on claude-code (8000-token soft cap) must produce a budget WARNING
        // when strict is false — advisory, non-blocking.
        let p = project(vec![skill("huge", &over_budget_body(), &["claude-code"])]);
        let diags = check(&p, false);

        let budget = over_budget_diags(&diags, "huge");
        assert_eq!(
            budget.len(),
            1,
            "exactly one budget diagnostic for the over-budget skill; got: {diags:?}",
        );
        assert_eq!(
            budget[0].severity,
            Severity::Warning,
            "a soft-token overflow without --strict must be a warning, not an error",
        );
    }

    #[test]
    fn soft_token_overflow_errors_with_strict() {
        // The same over-budget skill must be promoted to a budget ERROR under --strict (the soft
        // budget's only blocking mode in core). Pinning both severities across the strict flag is
        // the stricter contract that survives WU-19's "soft-token only" refactor meaningfully.
        let p = project(vec![skill("huge", &over_budget_body(), &["claude-code"])]);
        let diags = check(&p, true);

        let budget = over_budget_diags(&diags, "huge");
        assert_eq!(
            budget.len(),
            1,
            "exactly one budget diagnostic for the over-budget skill under --strict; got: {diags:?}",
        );
        assert!(
            budget[0].is_error(),
            "a soft-token overflow under --strict must be a blocking error",
        );
    }

    #[test]
    fn within_budget_skill_produces_no_budget_diagnostic() {
        // A tiny skill body cannot exceed the soft cap, so the budget lint must stay silent for it
        // (boundary symmetry: the warn/error tests above only fire on genuine overflow).
        let p = project(vec![skill("tiny", "just a few words", &["claude-code"])]);
        let diags = check(&p, false);

        assert!(
            over_budget_diags(&diags, "tiny").is_empty(),
            "a within-budget skill must produce no budget diagnostic; got: {diags:?}",
        );
    }
}
