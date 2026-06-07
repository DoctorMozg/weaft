//! WU-21: determinism guarantees (C-DETERMINISM).
//!
//! These end-to-end tests drive the `weaft` binary against `examples/quickstart` and assert the
//! reproducibility properties the spec requires of emitted output:
//!
//! - **Byte-identical across runs** — two independent builds of the same source produce the same
//!   set of relative paths *and* byte-identical contents for every file. This is the load-bearing
//!   guarantee; it transitively proves run-invariance (no wall-clock, no `HashMap` iteration order,
//!   no `read_dir` order leaking into output).
//! - **No host-path leakage** — emitted *contents* embed no absolute filesystem path, no Windows
//!   drive letter, and no backslash path separator, so an artifact built on one machine is
//!   byte-identical to one built on another (C-DETERMINISM "no absolute filesystem paths").
//! - **No wall-clock timestamps** — emitted contents embed no `YYYY-MM-DD` date that would vary
//!   run-to-run. A light guard; the two-build-identical test is the stronger proof.
//!
//! Determinism was designed into the pipeline throughout (array-indexed capability table, ordered
//! field-maps, parser sort, `BTreeMap` merge, EOL/separator normalization in `merge_and_check`), so
//! these are expected to pass now — they are **regression guards** that fail the build if a future
//! change reintroduces non-determinism. The contents guards may pass vacuously on today's clean
//! quickstart; that is intended.

use assert_cmd::Command;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

fn weaft() -> Command {
    Command::cargo_bin("weaft").unwrap()
}

/// The quickstart project that every determinism check builds (the `cli_tests.rs` /
/// `compile_tests.rs` convention: `CARGO_MANIFEST_DIR` + `../../examples/quickstart`).
fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/quickstart")
}

/// A unique, collision-free temp directory for one build output. The tag plus the process id plus a
/// monotonic counter keeps two builds in the *same* test (and across parallel tests) from sharing a
/// directory — the byte-identity test depends on the two output trees being entirely independent.
fn unique_out(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "weaft-determinism-{}-{}-{}",
        tag,
        std::process::id(),
        n
    ));
    drop(std::fs::remove_dir_all(&dir));
    dir
}

/// Build the quickstart into `out` via the `weaft` binary, asserting success. Exercising the real
/// binary (not the in-process pipeline) is deliberate: it covers `run()`'s file-writing path, which
/// is where any separator/EOL normalization must finally hold.
fn build_quickstart_into(out: &Path) {
    weaft()
        .args(["build", "--manifest-path"])
        .arg(example_root())
        .arg("--out")
        .arg(out)
        .assert()
        .success();
}

/// Recursively collect every regular file under `root` as `relative_path -> bytes`, keyed on a
/// normalized forward-slash relative path so two trees are comparable regardless of host separator.
/// A `BTreeMap` gives a deterministic, order-independent comparison (the test asserts on the whole
/// map, so insertion/`read_dir` order is irrelevant).
fn collect_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, acc: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, acc);
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .expect("entry is under root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = std::fs::read(&path).expect("read emitted file");
                acc.insert(rel, bytes);
            }
        }
    }
    let mut acc = BTreeMap::new();
    walk(root, root, &mut acc);
    acc
}

#[test]
fn two_builds_produce_byte_identical_trees() {
    // The keystone C-DETERMINISM guarantee: building the same source twice, into two independent
    // output directories, yields the same set of relative paths AND byte-identical contents for
    // every file. Comparing the two whole `(relpath -> bytes)` maps proves both the file SET and
    // each file's BYTES match — and transitively rules out any run-varying input (timestamps,
    // hash-map iteration order, directory read order) reaching the emitted output.
    let out_a = unique_out("identity-a");
    let out_b = unique_out("identity-b");

    build_quickstart_into(&out_a);
    build_quickstart_into(&out_b);

    let tree_a = collect_tree(&out_a);
    let tree_b = collect_tree(&out_b);

    // A non-empty build is a precondition: an empty tree would let this test pass vacuously.
    assert!(
        !tree_a.is_empty(),
        "the quickstart build must emit at least one file (else the identity check is vacuous)",
    );

    // Compare the path SETS first so a divergence surfaces as a readable path diff rather than an
    // opaque "byte maps differ".
    let paths_a: Vec<&String> = tree_a.keys().collect();
    let paths_b: Vec<&String> = tree_b.keys().collect();
    assert_eq!(
        paths_a, paths_b,
        "two builds must emit the identical set of relative file paths",
    );

    // Then assert byte-for-byte equality of the full maps (paths + contents together).
    assert!(
        tree_a == tree_b,
        "two builds of the same source must be byte-identical; the following paths differ: {:?}",
        tree_a
            .iter()
            .filter(|(k, v)| tree_b.get(*k) != Some(*v))
            .map(|(k, _)| k)
            .collect::<Vec<_>>(),
    );

    drop(std::fs::remove_dir_all(&out_a));
    drop(std::fs::remove_dir_all(&out_b));
}

#[test]
fn emitted_contents_contain_no_absolute_paths_or_windows_separators() {
    // C-DETERMINISM "no absolute filesystem paths": emitted CONTENTS must not embed a build-machine
    // path, a Windows drive letter, or a backslash path separator — any of those would make output
    // vary by host or OS. This is a regression guard: today's quickstart is clean, so it may pass
    // vacuously; it bites if a future change interpolates an absolute path into emitted bytes.
    let out = unique_out("no-abs-paths");
    build_quickstart_into(&out);

    let tree = collect_tree(&out);
    assert!(
        !tree.is_empty(),
        "the build must emit files for the no-absolute-path guard to be meaningful",
    );

    for (rel, bytes) in &tree {
        let text = String::from_utf8(bytes.clone())
            .unwrap_or_else(|_| panic!("emitted file `{rel}` must be valid utf-8 text"));

        assert!(
            !text.contains("/home/"),
            "emitted `{rel}` must not embed an absolute `/home/...` path (host-path leak)",
        );
        // A literal temp-dir leak (e.g. the `--out` path interpolated into a file) would also be a
        // host-specific path; assert the output directory's own path never appears in contents.
        assert!(
            !text.contains(&out.to_string_lossy().to_string()),
            "emitted `{rel}` must not embed the build output directory path (host-path leak)",
        );
        assert!(
            !contains_windows_drive_letter(&text),
            "emitted `{rel}` must not embed a `C:\\`-style Windows drive letter (host-path leak)",
        );
        // The merge stage normalizes path separators to `/`; emitted contents likewise carry no
        // backslash path separators (the quickstart authors none). Guards against a Windows path
        // (or an unnormalized separator) leaking into a file body.
        assert!(
            !text.contains('\\'),
            "emitted `{rel}` must not embed a backslash path separator (separators normalize to /)",
        );
    }

    drop(std::fs::remove_dir_all(&out));
}

#[test]
fn emitted_contents_contain_no_wall_clock_dates() {
    // Light run-invariance guard: emitted contents must not embed an obvious `YYYY-MM-DD` calendar
    // date, which would vary run-to-run and break reproducibility. The two-build-identical test is
    // the stronger proof of run-invariance; this catches a date string directly so a regression is
    // diagnosed at its source rather than as a mysterious byte diff.
    let out = unique_out("no-timestamps");
    build_quickstart_into(&out);

    let tree = collect_tree(&out);
    assert!(
        !tree.is_empty(),
        "the build must emit files for the no-timestamp guard to be meaningful",
    );

    for (rel, bytes) in &tree {
        let text = String::from_utf8(bytes.clone())
            .unwrap_or_else(|_| panic!("emitted file `{rel}` must be valid utf-8 text"));
        if let Some(found) = first_iso_date(&text) {
            panic!(
                "emitted `{rel}` embeds a wall-clock date `{found}` that would vary run-to-run; \
                 builds must be timestamp-free",
            );
        }
    }

    drop(std::fs::remove_dir_all(&out));
}

/// True if `text` contains a `C:\`-style Windows drive-letter prefix: a single ASCII letter
/// followed by `:\`. Scoped to the drive-letter shape so a normal `key: value` colon never trips it.
fn contains_windows_drive_letter(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes
        .windows(3)
        .any(|w| w[0].is_ascii_alphabetic() && w[1] == b':' && (w[2] == b'\\' || w[2] == b'/'))
}

/// Return the first `YYYY-MM-DD` substring in `text`, if any. A deliberately narrow shape (four
/// digits, dash, two digits, dash, two digits) so version-like tokens such as `claude-sonnet-4-5`
/// never match — only a full ISO calendar date does.
fn first_iso_date(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    bytes.windows(10).find_map(|w| {
        let is_iso = w[0].is_ascii_digit()
            && w[1].is_ascii_digit()
            && w[2].is_ascii_digit()
            && w[3].is_ascii_digit()
            && w[4] == b'-'
            && w[5].is_ascii_digit()
            && w[6].is_ascii_digit()
            && w[7] == b'-'
            && w[8].is_ascii_digit()
            && w[9].is_ascii_digit();
        is_iso.then(|| String::from_utf8_lossy(w).into_owned())
    })
}
