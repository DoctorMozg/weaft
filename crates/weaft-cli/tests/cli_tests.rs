//! End-to-end CLI tests driving the `weaft` binary.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn weaft() -> Command {
    Command::cargo_bin("weaft").unwrap()
}

fn quickstart() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/quickstart")
}

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("weaft-it-{}-{}", tag, std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    dir
}

/// Assert an emitted config-singleton JSON file is a well-formed, non-empty object that carries
/// every `must_contain` substring. The structural checks (object braces, balanced + non-empty)
/// guard well-formedness without pulling a JSON parser into dev-deps; the substring checks pin
/// that the declared singleton content actually reached the file rather than being dropped to `{}`.
fn assert_json_singleton(path: &std::path::Path, must_contain: &[&str]) {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    let trimmed = text.trim();
    assert!(
        trimmed.starts_with('{') && trimmed.ends_with('}'),
        "{} must be a JSON object document; got:\n{text}",
        path.display(),
    );
    assert_eq!(
        trimmed.matches('{').count(),
        trimmed.matches('}').count(),
        "{} must have balanced JSON braces; got:\n{text}",
        path.display(),
    );
    assert_ne!(
        trimmed,
        "{}",
        "{} must carry the declared singleton content, not an empty object",
        path.display(),
    );
    for needle in must_contain {
        assert!(
            text.contains(needle),
            "{} must contain {needle:?}; got:\n{text}",
            path.display(),
        );
    }
}

#[test]
fn targets_lists_hosts() {
    weaft()
        .arg("targets")
        .assert()
        .success()
        .stdout(predicate::str::contains("claude-code"))
        .stdout(predicate::str::contains("cursor"));
}

#[test]
fn build_writes_files() {
    let out = tmp_dir("build");
    weaft()
        .args(["build", "--manifest-path"])
        .arg(quickstart())
        .arg("--out")
        .arg(&out)
        .assert()
        .success();

    assert!(
        out.join("claude-code/skills/safe-deleter/SKILL.md")
            .is_file()
    );
    assert!(out.join("cursor/rules/safe-deleter.mdc").is_file());
    assert!(out.join("claude-code/agents/code-reviewer.md").is_file());
    assert!(out.join("agents-md/AGENTS.md").is_file());

    // v2 quickstart additions: claude-code natively emits the instruction (CLAUDE.md) and the
    // two declared config singletons (settings + the single mcp_server) as real files (Fix 6).
    assert!(
        out.join("claude-code/CLAUDE.md").is_file(),
        "claude-code must emit the instruction artifact as CLAUDE.md",
    );
    assert!(
        out.join("claude-code/.claude/settings.json").is_file(),
        "claude-code must emit the declared settings singleton as .claude/settings.json",
    );
    // Claude Code's `.mcp.json` lives at the repo ROOT, not under `.claude/`. The legacy
    // `.claude/.mcp.json` path was host-invalid and must no longer be emitted.
    assert!(
        out.join("claude-code/.mcp.json").is_file(),
        "claude-code must emit the mcp_servers singleton as repo-root .mcp.json",
    );
    assert!(
        !out.join("claude-code/.claude/.mcp.json").exists(),
        "the host-invalid .claude/.mcp.json path must not be emitted",
    );
    // Both config singletons must be well-formed JSON objects AND carry their declared content
    // (not a silently-dropped `{}`). The quickstart declares `permissions.defaultMode: acceptEdits`
    // for settings; `.mcp.json` must use the host `mcpServers` envelope nesting the `fs` server
    // (with its `command: npx`) under its name key.
    assert_json_singleton(
        &out.join("claude-code/.claude/settings.json"),
        &["permissions", "acceptEdits"],
    );
    assert_json_singleton(
        &out.join("claude-code/.mcp.json"),
        &["mcpServers", "fs", "command", "npx"],
    );

    // The cursor/agents-md hosts do NOT get the config singletons (their cells Drop) — guard that
    // the drop is honored and nothing leaks a settings file onto a host that cannot represent it.
    assert!(
        !out.join("cursor/.claude/settings.json").exists(),
        "cursor cannot represent settings — no settings file may be emitted for it",
    );
    drop(std::fs::remove_dir_all(&out));
}

#[test]
fn lint_clean_project_succeeds() {
    weaft()
        .args(["lint", "--manifest-path"])
        .arg(quickstart())
        .assert()
        .success();
}

#[test]
fn preview_prints_without_writing() {
    weaft()
        .args(["preview", "--target", "cursor", "--manifest-path"])
        .arg(quickstart())
        .assert()
        .success()
        .stdout(predicate::str::contains("Cursor"))
        .stdout(predicate::str::contains("# === rules/safe-deleter.mdc ==="));
}

#[test]
fn tokens_reports_budget() {
    weaft()
        .args(["tokens", "--manifest-path"])
        .arg(quickstart())
        .assert()
        .success()
        .stdout(predicate::str::contains("/8000"))
        .stdout(predicate::str::contains("approximations"));
}

#[test]
fn unknown_target_errors() {
    weaft()
        .args(["build", "--target", "gemini", "--manifest-path"])
        .arg(quickstart())
        .assert()
        .failure();
}

#[test]
fn init_then_build_roundtrips() {
    let parent = tmp_dir("init");
    std::fs::create_dir_all(&parent).unwrap();

    weaft()
        .args(["init", "demo", "--path"])
        .arg(&parent)
        .assert()
        .success();

    let project = parent.join("demo");
    assert!(project.join("weaft.yaml").is_file());
    assert!(project.join("skills/hello.md").is_file());
    assert!(project.join("agents/reviewer.md").is_file());

    let out = parent.join("dist");
    weaft()
        .args(["build", "--manifest-path"])
        .arg(&project)
        .arg("--out")
        .arg(&out)
        .assert()
        .success();

    drop(std::fs::remove_dir_all(&parent));
}

// --- WU-19: the cli `lint` hard-byte dry-run (Fix 2/5) ---
//
// WU-19 adds a hard-byte dry-run to `weaft lint`: after `lint::run`, the lint command drives the
// SAME `merge_and_check` build path `weaft build` uses (no files written) and folds its
// diagnostics into the tally. The single concrete hard-byte cell is codex's merged `AGENTS.md`
// (32_768-byte Hard budget, ADR-0005), so a codex instruction file whose merged output exceeds
// ~32 KiB must fail `weaft lint` exactly as it fails `weaft build` — proving the shared path.
//
// RED: today `lint.rs` runs only `lint::run` (no byte check), so `weaft lint` on an over-cap codex
// project exits 0. The test below asserts a NON-ZERO exit — behavioral RED until WU-19 wires the
// dry-run. (See the concern in the dispatch report: the *codex backend* is registered in WU-16, a
// later wave, so this test reaches GREEN only once both the WU-19 lint dry-run AND the WU-16 codex
// backend exist; until then it correctly stays RED.)

/// Build a temp project dir targeting one host with a single oversized instruction file, returning
/// the project root. `instruction_bytes` is the size of the instruction *body* (the merged
/// `AGENTS.md` ends up at least this large), so callers can straddle the 32_768-byte codex cap.
fn project_with_big_instruction(tag: &str, host: &str, instruction_bytes: usize) -> PathBuf {
    let root = tmp_dir(tag);
    std::fs::create_dir_all(root.join("instructions")).unwrap();
    std::fs::write(
        root.join("weaft.yaml"),
        format!("name: {tag}\nversion: 0.1.0\ntargets:\n  supported:\n    - {host}\n"),
    )
    .unwrap();
    let body = "x".repeat(instruction_bytes);
    std::fs::write(
        root.join("instructions/big.md"),
        format!("---\nname: big\ndescription: oversized instruction.\n---\n{body}\n"),
    )
    .unwrap();
    root
}

#[test]
fn lint_fails_on_codex_instruction_over_32kib() {
    // Fix-2/5 keystone: a codex project whose merged AGENTS.md instruction exceeds the 32_768-byte
    // HARD cap must make `weaft lint` exit NON-ZERO — the byte cap is enforced through the same
    // merge_and_check path `build` uses. 40_000 bytes is comfortably over the cap.
    let root = project_with_big_instruction("lint-codex-over", "codex", 40_000);

    weaft()
        .args(["lint", "--manifest-path"])
        .arg(&root)
        .assert()
        .failure();

    drop(std::fs::remove_dir_all(&root));
}

#[test]
fn lint_and_build_agree_on_an_over_cap_codex_project() {
    // Parity (the plan's "build and lint both fail via the same merge_and_check path"): on the same
    // over-cap codex project, `weaft build` and `weaft lint` must BOTH fail. Asserting both ends in
    // one test pins that lint reuses the build byte path rather than reimplementing it. Today build
    // emits 0 files (codex backend lands in WU-16) and exits 0, so this `failure()` pair is RED now
    // and goes GREEN once the WU-16 codex backend + WU-19 lint dry-run are both in place.
    let root = project_with_big_instruction("lint-build-parity", "codex", 40_000);

    weaft()
        .args(["build", "--manifest-path"])
        .arg(&root)
        .arg("--out")
        .arg(root.join("dist"))
        .assert()
        .failure();

    weaft()
        .args(["lint", "--manifest-path"])
        .arg(&root)
        .assert()
        .failure();

    drop(std::fs::remove_dir_all(&root));
}

#[test]
fn lint_passes_on_a_codex_instruction_under_the_cap() {
    // Boundary symmetry: a codex instruction comfortably UNDER 32_768 bytes must NOT fail lint on
    // the byte axis. This guards against a future over-eager byte check that fails every codex
    // instruction. (It is exit 0 today too — a regression guard that must stay green through GREEN.)
    let root = project_with_big_instruction("lint-codex-under", "codex", 1_024);

    weaft()
        .args(["lint", "--manifest-path"])
        .arg(&root)
        .assert()
        .success();

    drop(std::fs::remove_dir_all(&root));
}

#[test]
fn lint_quickstart_exits_zero() {
    // Fix-6 guard: `weaft lint` on the quickstart must exit 0 — warnings (e.g. the agents-md
    // subagent drop) are allowed, but there must be no errors and no drop-warning flood. This
    // complements `lint_clean_project_succeeds` and stays the clean-project oracle through WU-19's
    // disposition-driven drop rewrite. (GREEN now by design.)
    weaft()
        .args(["lint", "--manifest-path"])
        .arg(quickstart())
        .assert()
        .success();
}

#[test]
fn lint_quickstart_warns_only() {
    // WU-22 authoring guard: `weaft lint` on the migrated quickstart must exit 0 with a *bounded,
    // enumerated* set of drop-warnings and ZERO errors (Fix 6 — declared singletons + native
    // claude-code cells mean no flood). The migrated manifest declares settings + one mcp_servers
    // entry and an `overview` instruction, with targets [claude-code, cursor, agents-md]. The
    // mcp_servers entries aggregate into ONE McpServer artifact named `mcp`, so the disposition
    // matrix yields exactly six drops:
    //   - cursor:    instruction `overview`, mcp_servers `mcp`, settings `settings` (cursor has no
    //                instruction/config cells)
    //   - agents-md: subagent `code-reviewer` (pre-existing v1 drop), mcp_servers `mcp`,
    //                settings `settings`
    // claude-code drops nothing (skills/subagent/instruction/settings/mcp all Native). Pinning the
    // exact count + each (artifact, host) pair catches a regression that turns a drop into an error
    // or floods warnings for undeclared kinds (plugin/hook/ignore/command are not declared).
    let output = weaft()
        .args(["lint", "--manifest-path"])
        .arg(quickstart())
        .assert()
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).expect("lint stderr is utf-8");

    // Every drop is a warning under the one generic disposition code — and there are no errors.
    assert_eq!(
        stderr.matches("weaft::lint::dropped").count(),
        6,
        "exactly six drop-warnings expected (cursor: overview/mcp/settings; agents-md: \
         code-reviewer/mcp/settings); got stderr:\n{stderr}",
    );
    assert!(
        !stderr.contains("error["),
        "lint on the quickstart must produce no error-level diagnostics; got stderr:\n{stderr}",
    );

    // Each enumerated (artifact, host) drop pair must be named. A drop line names both the artifact
    // and the host it targets, so asserting both substrings appear on some line pins the pair.
    for (artifact, host) in [
        ("overview", "cursor"),
        ("mcp", "cursor"),
        ("settings", "cursor"),
        ("code-reviewer", "agents-md"),
        ("mcp", "agents-md"),
        ("settings", "agents-md"),
    ] {
        let named = stderr.lines().any(|line| {
            line.contains("weaft::lint::dropped") && line.contains(artifact) && line.contains(host)
        });
        assert!(
            named,
            "expected a drop-warning naming `{artifact}` -> `{host}`; got stderr:\n{stderr}",
        );
    }

    // Undeclared config kinds must NOT appear (Fix 6: no synthetic artifact, no warning).
    for kind in ["plugin", "hook", "ignore", "command"] {
        assert!(
            !stderr.contains(kind),
            "no warning may mention the undeclared `{kind}` kind (Fix 6); got stderr:\n{stderr}",
        );
    }
}

// --- WU-17: a build targeting only gemini-cli emits nothing but drop-warnings ---
//
// gemini-cli is the conservative sixth host: every kind cell is `Drop` (RFD-0001
// §C-OPEN-QUESTIONS), so a build targeting only gemini-cli writes ZERO files and raises exactly
// one drop-warning per *declared* artifact (Fix-6 scoping — warnings are per present artifact, not
// per kind), with no errors (a Drop is honest under-claiming, not a failure → exit 0).
//
// RED: today `target_by_id("gemini-cli")` returns `None`, so `weaft build --target gemini-cli`
// resolves no backend and exits with an `UnknownTarget` error (failure) — the `success()` +
// zero-files + per-artifact-warning assertions below all fail. They reach GREEN only once WU-17
// registers the gemini-cli backend and adds its all-`Drop` capability matrix.

/// Build a temp project declaring exactly one skill and one subagent, both supported by `host`.
/// Returns the project root. The two declared artifacts are what the gemini-cli build must turn
/// into exactly two drop-warnings (one per artifact) — a self-contained fixture independent of the
/// quickstart's evolving artifact set.
fn project_with_skill_and_subagent(tag: &str, host: &str) -> PathBuf {
    let root = tmp_dir(tag);
    std::fs::create_dir_all(root.join("skills")).unwrap();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::write(
        root.join("weaft.yaml"),
        format!("name: {tag}\nversion: 0.1.0\ntargets:\n  supported:\n    - {host}\n"),
    )
    .unwrap();
    std::fs::write(
        root.join("skills/cleaner.md"),
        "---\nname: cleaner\ndescription: a demo skill.\n---\nClean things up.\n",
    )
    .unwrap();
    std::fs::write(
        root.join("agents/reviewer.md"),
        "---\nname: reviewer\ndescription: a demo subagent.\n---\nReview the diff.\n",
    )
    .unwrap();
    root
}

/// Count the files written under an output directory tree (recursively). A gemini-cli build must
/// leave this at zero — every artifact drops, so no file is emitted.
fn count_files(dir: &std::path::Path) -> usize {
    fn walk(dir: &std::path::Path, count: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, count);
            } else {
                *count += 1;
            }
        }
    }
    let mut count = 0;
    walk(dir, &mut count);
    count
}

#[test]
fn build_gemini_cli_only_emits_zero_files() {
    // A build targeting only gemini-cli (all kind cells Drop) must succeed (warnings, no errors)
    // and write NO files — honest under-claiming, not a broken emit.
    let root = project_with_skill_and_subagent("gemini-zero-files", "gemini-cli");
    let out = root.join("dist");

    weaft()
        .args(["build", "--target", "gemini-cli", "--manifest-path"])
        .arg(&root)
        .arg("--out")
        .arg(&out)
        .assert()
        .success();

    assert_eq!(
        count_files(&out),
        0,
        "a gemini-cli build must emit zero files (every kind cell is Drop)",
    );

    drop(std::fs::remove_dir_all(&root));
}

#[test]
fn build_gemini_cli_only_warns_once_per_declared_artifact() {
    // The drop-warnings are scoped per *declared* artifact (Fix 6), not per kind: this project
    // declares exactly two artifacts (the `cleaner` skill + the `reviewer` subagent), so the build
    // must emit exactly one drop-warning naming each — two `weaft::emit::dropped` warnings total,
    // each naming the artifact, and zero error-level diagnostics.
    let root = project_with_skill_and_subagent("gemini-warns-per-artifact", "gemini-cli");
    let out = root.join("dist");

    weaft()
        .args(["build", "--target", "gemini-cli", "--manifest-path"])
        .arg(&root)
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        // One drop-warning per declared artifact, each naming the artifact by name.
        .stderr(predicate::str::contains("weaft::emit::dropped"))
        .stderr(predicate::str::contains("cleaner"))
        .stderr(predicate::str::contains("reviewer"))
        // Drops are warnings, never errors — gemini-cli under-claims honestly, it does not fail.
        .stderr(predicate::str::contains("error[").not());

    drop(std::fs::remove_dir_all(&root));
}

// --- WU-20: CLI registry-derived help + honesty surfaces + kind-aware iteration ---
//
// WU-20 removes the last hardcoded host lists from the CLI surface and makes the budget/kind
// reporting derive from the registry + disposition resolver rather than legacy bools:
//   - `init` defaults come from `capability::init_default_ids()` (not a hardcoded
//     `["claude-code","cursor"]` literal).
//   - the `UnknownTarget` help lists `capability::known_ids()` (not the hardcoded five-host
//     literal in `diag.rs`, which predates gemini-cli).
//   - `targets` surfaces per-kind disposition / budget unit alongside the (preserved) honesty
//     footer (C-BUDGET-HONESTY).
//   - `tokens` gates per-artifact reporting on `resolve(...).disposition != Drop`, not the legacy
//     `host.supports_subagents` bool, and keeps the "approximations" honesty note.
//
// The honesty/six-id/init-value guards stay GREEN by design (they pin a stated repo invariant and
// the registry source). The disposition-surface, registry-derived-help, and resolver-gate guards
// are RED now: today `targets` shows no disposition, the unknown-target help omits `gemini-cli`,
// and `tokens` reports a skill even on a host whose Skill cell drops.

/// Write a minimal project declaring exactly one skill, supported only by `host`, returning the
/// project root. One artifact keeps the `tokens` reporting assertions unambiguous: whether the
/// skill line appears is governed solely by `host`'s Skill-cell disposition.
fn project_with_one_skill(tag: &str, host: &str) -> PathBuf {
    let root = tmp_dir(tag);
    std::fs::create_dir_all(root.join("skills")).unwrap();
    std::fs::write(
        root.join("weaft.yaml"),
        format!("name: {tag}\nversion: 0.1.0\ntargets:\n  supported:\n    - {host}\n"),
    )
    .unwrap();
    std::fs::write(
        root.join("skills/cleaner.md"),
        "---\nname: cleaner\ndescription: a demo skill.\n---\nClean things up.\n",
    )
    .unwrap();
    root
}

#[test]
fn init_default_targets_are_registry_derived() {
    // `weaft init` with NO `--target` must prefill `targets.supported` from the registry's
    // init-default set (`capability::init_default_ids()` == [claude-code, cursor]), not a hardcoded
    // literal. Asserting against the registry helper (not a `["claude-code","cursor"]` constant in
    // the test) keeps this a single-source guard: if the init-default flags change, the expectation
    // tracks them. `init_then_build_roundtrips` independently pins that hello.md/reviewer.md still
    // scaffold and build, so this case focuses on the manifest's supported set.
    let parent = tmp_dir("init-registry-default");
    std::fs::create_dir_all(&parent).unwrap();

    weaft()
        .args(["init", "demo", "--path"])
        .arg(&parent)
        .assert()
        .success();

    let manifest = std::fs::read_to_string(parent.join("demo/weaft.yaml"))
        .expect("init must write demo/weaft.yaml");

    // Every init-default host id (from the registry) must appear as a supported target...
    let defaults = weaft_core::capability::init_default_ids();
    assert!(
        !defaults.is_empty(),
        "the registry must declare at least one init-default host",
    );
    for id in &defaults {
        assert!(
            manifest.contains(&format!("- {id}")),
            "init weaft.yaml must list the registry init-default `{id}` as a supported target; \
             got:\n{manifest}",
        );
    }

    // ...and no NON-default known host id may leak into the scaffolded supported set (proves the
    // default set is exactly the registry init-default set, not an over-broad literal).
    for host in weaft_core::capability::all() {
        if !host.init_default {
            assert!(
                !manifest.contains(&format!("- {}", host.id)),
                "init must not scaffold the non-default host `{}` into targets.supported; got:\n{manifest}",
                host.id,
            );
        }
    }

    drop(std::fs::remove_dir_all(&parent));
}

#[test]
fn targets_lists_all_six_hosts_with_honesty_footer() {
    // The `targets` table must list every registered host id and keep the honesty footer verbatim
    // (C-BUDGET-HONESTY: budgets are weaft heuristics; token counts are approximate). This guard
    // stays GREEN — it pins the stated repo invariant and that no host silently drops off the table.
    let mut cmd = weaft();
    let assert = cmd.arg("targets").assert().success();
    let assert = [
        "claude-code",
        "cursor",
        "agents-md",
        "opencode",
        "codex",
        "gemini-cli",
    ]
    .into_iter()
    .fold(assert, |a, id| a.stdout(predicate::str::contains(id)));
    // The honesty footer must survive WU-20's table changes: match the load-bearing word stems
    // ("heuristic"/"approximat") rather than the exact sentence, so a reworded-but-honest footer
    // still passes while a removed disclaimer fails.
    assert
        .stdout(predicate::str::contains("heuristic"))
        .stdout(predicate::str::contains("approximat"));
}

#[test]
fn targets_surfaces_per_kind_disposition() {
    // WU-20 extends the `targets` table to show what each host does per kind (disposition) and/or
    // the budget unit. With six hosts, several kind cells are `Drop` (agents-md subagents, every
    // gemini-cli cell, cursor's config kinds), so a disposition-aware table MUST surface "drop"
    // somewhere in its output.
    //
    // RED: today `targets` prints only the v1 host-wide bool columns (subagents/assets/tools/ask/
    // budget) and never the word "drop", so this assertion fails until WU-20 adds the per-kind
    // disposition surface. (Case-insensitive to allow "Drop"/"drop" headers either way.)
    weaft()
        .arg("targets")
        .assert()
        .success()
        .stdout(predicate::str::contains("drop").or(predicate::str::contains("Drop")));
}

#[test]
fn tokens_preserves_honesty_and_reports_a_supported_skill_budget() {
    // Companion to `tokens_reports_budget`, pinned at WU-20: the quickstart's claude-code Skill
    // cell is non-Drop, so its 8000-token budget must be reported, and the "approximations" honesty
    // note must remain (C-BUDGET-HONESTY). Stays GREEN — guards that the kind-aware rewrite does not
    // drop the budget line for an artifact the host actually emits, nor the honesty disclaimer.
    weaft()
        .args(["tokens", "--manifest-path"])
        .arg(quickstart())
        .assert()
        .success()
        .stdout(predicate::str::contains("/8000"))
        .stdout(predicate::str::contains("approximat"));
}

#[test]
fn tokens_omits_a_skill_whose_host_skill_cell_drops() {
    // WU-20 secondary fix: `tokens` must gate per-artifact reporting on the disposition resolver
    // (`resolve(...).disposition != Drop`), not whether the artifact merely targets the host. The
    // gemini-cli Skill cell is `Drop`, so a skill targeting only gemini-cli is emitted by NO host
    // and must NOT appear in `tokens` output — reporting a budget for a file that is never produced
    // would be dishonest.
    //
    // RED: today the `tokens` skill loop prints every skill whose `targets.supported` includes the
    // host, with no disposition gate, so `cleaner` IS printed under gemini-cli. The `.not()`
    // assertion below fails until WU-20 wires the resolver gate. The honesty note must still print.
    let root = project_with_one_skill("tokens-gemini-drop", "gemini-cli");

    weaft()
        .args(["tokens", "--manifest-path"])
        .arg(&root)
        .assert()
        .success()
        // The skill is never emitted (Skill cell drops on gemini-cli) → no budget line for it.
        .stdout(predicate::str::contains("cleaner").not())
        // Honesty disclaimer is preserved regardless of which artifacts report.
        .stdout(predicate::str::contains("approximat"));

    drop(std::fs::remove_dir_all(&root));
}

#[test]
fn unknown_target_help_is_registry_derived_listing_gemini_cli() {
    // The `UnknownTarget` diagnostic help must list `capability::known_ids()` (registry-derived),
    // not the hardcoded five-host literal in `diag.rs` that predates gemini-cli. `gemini-cli` is a
    // registered host, so an unknown-target error must name it in the "known targets" help — that
    // it appears proves the list is derived from the registry, not the stale literal.
    //
    // RED: today `diag.rs` carries `help("known targets: claude-code, cursor, agents-md, opencode,
    // codex")` — a hardcoded literal MISSING `gemini-cli` — so the help omits it. The assertion
    // fails until WU-20 makes the help registry-derived. The miette diagnostic (help included)
    // renders to stderr.
    let root = project_with_one_skill("unknown-target-help", "claude-code");

    weaft()
        .args(["build", "--target", "bogus", "--manifest-path"])
        .arg(&root)
        .assert()
        .failure()
        // The diagnostic still names the offending id...
        .stderr(predicate::str::contains("bogus"))
        // ...and the registry-derived help must list gemini-cli (absent from the old literal).
        .stderr(predicate::str::contains("gemini-cli"));

    drop(std::fs::remove_dir_all(&root));
}
