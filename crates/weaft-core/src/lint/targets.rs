//! Target-validity lints: unknown target ids (hard errors) and the disposition-driven drop
//! warning.
//!
//! The drop rule is a single generic pass over [`crate::capability::Disposition`] (WU-19): for
//! every artifact genuinely *present* in `project.artifacts`, each host the artifact targets is
//! resolved; a host whose capability cell resolves [`Disposition::Drop`] yields one warning naming
//! both the artifact and the host (C-SUPPORT-DISPOSITION). This replaces the two bespoke v1
//! warnings that hardcoded `supports_assets` / `supports_subagents` bools.
//!
//! Scope is **present artifacts only** (Fix 6): an undeclared config kind synthesizes no artifact
//! (WU-9 only materializes declared singletons), so it never floods the lint. The hard-byte budget
//! check is *not* here — it needs the merged bytes, so it lives in `weaft-cli/merge_and_check`.

use crate::capability::{self, Disposition};
use crate::diag::Diagnostic;
use crate::ir::{Artifact, Project, Targets};
use crate::pipeline::resolve::resolve;

const UNKNOWN: &str = "weaft::lint::invalid_target_id";
const DROPPED: &str = "weaft::lint::dropped";

// The two bespoke v1 target-validity codes the generic disposition rule retires. They are no
// longer emitted by `check`; the names are kept solely so the read-only `wu19_disposition_drop_tests`
// module can assert (via `RETIRED_CODES`) that they never reappear in the lint output. Gated on
// `cfg(test)` (rather than an `allow`/`expect(dead_code)` attribute) so they exist only in the test
// build that references them — no dead-code lint in the non-test compile, and nothing to suppress.
#[cfg(test)]
const ASSET: &str = "weaft::lint::asset_unsupported_by_target";
#[cfg(test)]
const SUBAGENT: &str = "weaft::lint::subagent_unsupported_by_target";

pub fn check(project: &Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // Unknown target ids — hard errors — across the project and every artifact.
    check_ids(&project.info.targets, "weaft.yaml", &mut out);
    for artifact in &project.artifacts {
        check_ids(
            &artifact.frontmatter.targets,
            &artifact.frontmatter.name,
            &mut out,
        );
    }

    // Disposition-driven drop warning, scoped to present artifacts (Fix 6): one warning per
    // (present artifact, targeted host) pair whose resolved cell is `Drop`.
    for artifact in &project.artifacts {
        for host in super::artifact_hosts(project, &artifact.frontmatter.targets) {
            if resolve(artifact, host).disposition == Disposition::Drop {
                out.push(drop_warning(artifact, host.id));
            }
        }
    }

    out
}

/// One generic, data-driven drop warning naming both the artifact and the host. Replaces the v1
/// bespoke `asset_unsupported_by_target` / `subagent_unsupported_by_target` codes with the single
/// disposition rule (C-SUPPORT-DISPOSITION). Mirrors the build-time `weaft::emit::dropped` warning
/// so `lint` and `build` describe a drop the same way.
fn drop_warning(artifact: &Artifact, host_id: &str) -> Diagnostic {
    Diagnostic::warning(
        DROPPED,
        format!(
            "{} `{}` targets `{host_id}`, which cannot represent its kind; it will be skipped",
            artifact.kind.serde_name(),
            artifact.frontmatter.name,
        ),
    )
    .with_artifact(artifact.frontmatter.name.clone())
}

fn check_ids(targets: &Targets, artifact: &str, out: &mut Vec<Diagnostic>) {
    for id in &targets.supported {
        if capability::by_id(id).is_none() {
            out.push(
                Diagnostic::error(UNKNOWN, format!("unknown target id `{id}`"))
                    .with_artifact(artifact.to_string())
                    .with_help(format!(
                        "known targets: {}",
                        capability::known_ids().join(", ")
                    )),
            );
        }
    }
}

/// WU-19: disposition-driven drop warnings + drop-scope (Fix 6) + registry-derived help (RED).
///
/// These tests pin the *new* target-validity lint behavior authored in a later GREEN step. WU-19
/// replaces the two bespoke v1 warnings (`asset_unsupported_by_target` /
/// `subagent_unsupported_by_target`, both hardcoded on `supports_assets` / `supports_subagents`
/// bools) with **one generic disposition rule** over all nine kinds: for every artifact that is
/// genuinely present in `project.artifacts`, a `(artifact, host)` pair whose capability cell
/// resolves [`crate::capability::Disposition::Drop`] yields a single drop warning naming both the
/// artifact and the host (C-SUPPORT-DISPOSITION). The scope is **present artifacts only** (Fix 6):
/// an undeclared config kind synthesizes no artifact, so it never floods the lint with warnings.
///
/// The "known targets:" help on the [`UNKNOWN`] diagnostic must also become registry-derived from
/// [`crate::capability::known_ids`] rather than the hardcoded literal in [`check_ids`].
///
/// RED categories:
/// - **Behavioral (new warning)**: today `check` only inspects `supports_assets` (gated on a real
///   `assets/` dir) and `supports_subagents` (only over `project.agents()`), so a *declared
///   config singleton* whose cell drops (e.g. a `Settings` artifact targeting cursor/agents-md)
///   produces **zero** diagnostics. After WU-19 it must produce exactly one drop warning. (0 → 1.)
/// - **Behavioral (code change)**: today the agents-md subagent drop carries the bespoke code
///   [`SUBAGENT`]; after WU-19 the generic disposition rule emits a different, kind-agnostic code,
///   so asserting the *bespoke* codes are gone is RED now.
///
/// They build an in-memory [`Project`] from kind-tagged [`Artifact`]s (the same fixture shape the
/// sibling `ask_user` tests use), so no on-disk scaffolding is needed. The `root` is a path with
/// no `assets/` dir, keeping the legacy asset branch silent so each test isolates the drop rule.
#[cfg(test)]
mod wu19_disposition_drop_tests {
    use super::*;
    use crate::capability;
    use crate::ir::{
        Agent, AgentMeta, Artifact, Meta, Project, ProjectInfo, Skill, SkillMeta, Targets,
    };
    use crate::kind::ArtifactKind;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// The two bespoke v1 target-validity codes WU-19 retires in favor of one generic
    /// disposition-driven drop code. Asserting these are ABSENT from the output is the
    /// code-change RED signal: today the agents-md subagent drop is reported under [`SUBAGENT`].
    const RETIRED_CODES: [&str; 2] = [SUBAGENT, ASSET];

    fn targets(supported: &[&str]) -> Targets {
        Targets {
            supported: supported.iter().map(|s| (*s).to_string()).collect(),
            overrides: BTreeMap::new(),
        }
    }

    fn skill(name: &str, supported: &[&str]) -> Skill {
        Skill {
            frontmatter: SkillMeta {
                name: name.into(),
                description: "d".into(),
                targets: targets(supported),
            },
            body: "body".into(),
            source_path: PathBuf::from(format!("skills/{name}.md")),
        }
    }

    fn agent(name: &str, supported: &[&str]) -> Agent {
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
            body: "body".into(),
            source_path: PathBuf::from(format!("agents/{name}.md")),
        }
    }

    /// A project from raw kind-tagged artifacts (so a singleton can be included alongside file
    /// artifacts). `root` is a synthetic path with no `assets/` dir, so the legacy asset branch
    /// stays silent and each test isolates the disposition drop rule.
    fn project(artifacts: Vec<Artifact>, supported: &[&str]) -> Project {
        Project {
            info: ProjectInfo {
                name: "p".into(),
                version: "0".into(),
                description: String::new(),
                meta: Meta::default(),
                targets: targets(supported),
                parameters: BTreeMap::new(),
                settings: None,
                mcp_servers: BTreeMap::new(),
                ignore: Vec::new(),
                plugin: None,
            },
            artifacts,
            root: PathBuf::from("/weaft-nonexistent-root"),
        }
    }

    fn codes(diags: &[Diagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.code).collect()
    }

    /// Drop warnings: warnings that name a host id the project targets, i.e. the
    /// disposition-driven `(present artifact, host) -> Drop` diagnostics (independent of the exact
    /// code the GREEN coder picks). The legacy asset branch is silent here (no `assets/` dir), so
    /// the only warnings a populated project produces are drops.
    fn drop_warnings(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
        diags
            .iter()
            .filter(|d| !d.is_error())
            .filter(|d| {
                capability::known_ids()
                    .iter()
                    .any(|id| d.message.contains(id))
            })
            .collect()
    }

    #[test]
    fn agents_md_subagent_drop_uses_the_generic_disposition_rule_not_the_bespoke_code() {
        // A Subagent targeting agents-md (whose Subagent cell is Drop) must still warn — but via
        // the new generic disposition rule, NOT the bespoke `subagent_unsupported_by_target` code.
        // The message must name both the artifact and `agents-md`. Today `check` emits exactly the
        // retired SUBAGENT code, so the "retired code absent" assertion is RED now.
        let p = project(
            vec![Artifact::from_agent(agent("code-reviewer", &["agents-md"]))],
            &["agents-md"],
        );
        let diags = check(&p);

        let drops = drop_warnings(&diags);
        assert_eq!(
            drops.len(),
            1,
            "exactly one drop warning for the agents-md subagent; got: {diags:?}",
        );
        let drop = drops[0];
        assert!(
            drop.message.contains("code-reviewer"),
            "the drop warning must name the artifact `code-reviewer`; got: {:?}",
            drop.message,
        );
        assert!(
            drop.message.contains("agents-md"),
            "the drop warning must name the host `agents-md`; got: {:?}",
            drop.message,
        );

        for retired in RETIRED_CODES {
            assert!(
                !codes(&diags).contains(&retired),
                "WU-19 retires the bespoke `{retired}` code in favor of one disposition rule; \
                 it must no longer appear, got: {diags:?}",
            );
        }
    }

    #[test]
    fn declared_settings_singleton_targeting_cursor_drops_with_a_warning() {
        // Fix-6 keystone (0 -> 1 behavioral RED): a *declared* config singleton (Settings) whose
        // cell on the target host is Drop (cursor has no Settings cell) must produce exactly one
        // drop warning. Today `check` never inspects singleton dispositions, so it emits ZERO
        // diagnostics here — the cleanest behavioral RED in WU-19's lint surface.
        let settings = Artifact::from_singleton(
            ArtifactKind::Settings,
            "settings",
            serde_yaml_ng::Value::Null,
        );
        let p = project(
            vec![Artifact::from_skill(skill("greet", &["cursor"])), settings],
            &["cursor"],
        );
        let diags = check(&p);

        let drops = drop_warnings(&diags);
        assert_eq!(
            drops.len(),
            1,
            "a declared Settings singleton targeting cursor must drop with exactly one warning; \
             got: {diags:?}",
        );
        assert!(
            drops[0].message.contains("settings") && drops[0].message.contains("cursor"),
            "the singleton drop warning must name both `settings` and `cursor`; got: {:?}",
            drops[0].message,
        );
        // The natively-supported Skill on cursor must NOT drop — only the singleton does.
        assert!(
            !drops[0].message.contains("greet"),
            "a natively-supported skill must not produce a drop warning; got: {:?}",
            drops[0].message,
        );
    }

    #[test]
    fn undeclared_config_kinds_produce_no_drop_warning_flood() {
        // Fix-6 scope guard: a project that declares NO plugin/hook/ignore/command/mcp singletons
        // must produce no drop warnings for those kinds, even on claude-code where every one of
        // those cells is Drop. Only *present* artifacts can drop. The project here is a single
        // natively-supported skill, so there must be zero drop warnings at all.
        let p = project(
            vec![Artifact::from_skill(skill("greet", &["claude-code"]))],
            &["claude-code"],
        );
        let diags = check(&p);

        assert!(
            drop_warnings(&diags).is_empty(),
            "undeclared config kinds must not flood the lint with drop warnings; got: {diags:?}",
        );
        // And specifically none of the always-Drop claude-code config kinds may be named.
        for kind in ["plugin", "hook", "ignore", "command", "mcp_server"] {
            assert!(
                !diags.iter().any(|d| d.message.contains(kind)),
                "no warning may mention the undeclared `{kind}` kind (Fix 6); got: {diags:?}",
            );
        }
    }

    #[test]
    fn one_declared_drop_does_not_drag_in_undeclared_kinds() {
        // Combined scope assertion (RED + flood guard in one): with a present Settings singleton
        // and a present skill on agents-md, exactly the singleton + the (agents-md) subagent-style
        // drops fire — and crucially NOT the undeclared plugin/hook/ignore kinds. Here only the
        // skill (Native on agents-md) and the settings singleton (Drop on agents-md) are present,
        // so there must be exactly one drop warning (settings), and zero for undeclared kinds.
        let settings = Artifact::from_singleton(
            ArtifactKind::Settings,
            "settings",
            serde_yaml_ng::Value::Null,
        );
        let p = project(
            vec![
                Artifact::from_skill(skill("greet", &["agents-md"])),
                settings,
            ],
            &["agents-md"],
        );
        let diags = check(&p);

        let drops = drop_warnings(&diags);
        assert_eq!(
            drops.len(),
            1,
            "only the present Settings singleton drops on agents-md (the skill is Native); \
             undeclared kinds must not appear, got: {diags:?}",
        );
        assert!(
            drops[0].message.contains("settings"),
            "the single drop must be the declared settings singleton; got: {:?}",
            drops[0].message,
        );
        for kind in ["plugin", "hook", "ignore", "command"] {
            assert!(
                !diags.iter().any(|d| d.message.contains(kind)),
                "undeclared `{kind}` must not be dragged into the drop set; got: {diags:?}",
            );
        }
    }

    #[test]
    fn unknown_target_help_is_registry_derived() {
        // C-REGISTRY (Fix): the `UnknownTarget` help on the unknown-id error must be derived from
        // `capability::known_ids()`, not a hardcoded literal. Pinning that every known id appears
        // in the help makes a drift between the matrix and the help string fail. Today the help is
        // a frozen literal string; adding a host to the matrix would not update it — RED-meaningful
        // for the single-source requirement (and it already lists today's five, so the *gap* this
        // pins is the derivation, which a future host addition exercises).
        let p = project(
            vec![Artifact::from_skill(skill("greet", &["not-a-real-host"]))],
            &["not-a-real-host"],
        );
        let diags = check(&p);

        let unknown = diags
            .iter()
            .find(|d| d.code == UNKNOWN)
            .expect("an unknown target id must raise the UnknownTarget error");
        let help = unknown
            .help
            .as_deref()
            .expect("the unknown-target error must carry a help string");
        for id in capability::known_ids() {
            assert!(
                help.contains(id),
                "the unknown-target help must list every registry id (derived from known_ids()); \
                 `{id}` is missing from: {help:?}",
            );
        }
    }
}
