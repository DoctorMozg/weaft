//! WU-18: registry consistency between the core capability slice and the targets backend slice.
//!
//! ADR-0003 / C-REGISTRY model the host set as **two parallel ordered slices joined by id**: the
//! core slice is [`weaft_core::capability::all`]; the targets slice is [`weaft_targets::all_targets`].
//! Every declared host MUST have a backend and every backend MUST have a declared host, **in the
//! same order**. This test is the fail-closed gate for that invariant.
//!
//! It lives in `weaft-cli` because that is the only crate depending on **both** `weaft-core` and
//! `weaft-targets` (the dependency direction is cli → targets → core, so neither core nor targets
//! can see both slices). The check therefore cannot live anywhere else without violating the
//! crate-layering rule.
//!
//! ## Green-now guard (not a RED test)
//!
//! Both slices currently list the same five ids in the same order (after WU-16 registered
//! opencode + codex), so this test is **GREEN today** and is expected to stay GREEN. It is a
//! regression guard, not a TDD RED: WU-17 adds `gemini-cli` to BOTH slices (at the end of each),
//! so the ordered-equality assertion below keeps holding 6↔6. The test only goes RED if someone
//! adds a host to one slice but not the other (or in a different position) — which is exactly the
//! drift it exists to catch.

use weaft_core::capability;
use weaft_targets::{all_targets, target_by_id};

/// The rule, stated once, reused in every failure message so a drift report reads as the policy.
const RULE: &str = "C-REGISTRY (ADR-0003): every declared host needs a backend and every backend \
     needs a declared host, listed in the SAME order in both slices.";

#[test]
fn core_and_targets_slices_have_identical_id_sequences() {
    // The headline invariant: the core capability slice and the targets backend slice must be equal
    // id-for-id AND in the same order. Comparing the two `Vec<&str>` as ordered sequences pins both
    // membership and ordering in one assertion (a set comparison would miss a reorder).
    let core_ids: Vec<&'static str> = capability::all().iter().map(|h| h.id).collect();
    let target_ids: Vec<&'static str> = all_targets().iter().map(|t| t.id()).collect();

    assert_eq!(
        core_ids, target_ids,
        "core capability slice and targets backend slice disagree.\n  \
         capability::all() ids = {core_ids:?}\n  all_targets()      ids = {target_ids:?}\n{RULE}",
    );
}

#[test]
fn every_declared_host_has_a_backend() {
    // Forward direction: for each host in the core slice, `target_by_id(host.id)` must resolve to a
    // backend that reports the same id. A host declared in the matrix with no registered backend is
    // a build-breaking drift.
    for host in capability::all() {
        let backend = target_by_id(host.id).unwrap_or_else(|| {
            panic!(
                "declared host `{}` has no registered backend (target_by_id returned None).\n{RULE}",
                host.id,
            )
        });
        assert_eq!(
            backend.id(),
            host.id,
            "target_by_id(\"{}\") resolved to a backend reporting id \"{}\" — the registry key and \
             the backend id must agree.\n{RULE}",
            host.id,
            backend.id(),
        );
    }
}

#[test]
fn every_backend_has_a_declared_host() {
    // Reverse direction: for each registered backend, `capability::by_id(backend.id())` must resolve
    // to a host with the same id. A backend with no matching matrix entry is the symmetric drift.
    for backend in all_targets() {
        let host = capability::by_id(backend.id()).unwrap_or_else(|| {
            panic!(
                "registered backend `{}` has no declared host (capability::by_id returned None).\n{RULE}",
                backend.id(),
            )
        });
        assert_eq!(
            host.id,
            backend.id(),
            "capability::by_id(\"{}\") resolved to a host reporting id \"{}\" — the backend id and \
             the matrix id must agree.\n{RULE}",
            backend.id(),
            host.id,
        );
    }
}

#[test]
fn both_slices_are_the_same_length() {
    // A length mismatch is the simplest possible drift (one slice gained a host the other did not).
    // Asserting it directly gives a crisp failure even before the per-id checks above run.
    let core_len = capability::all().len();
    let target_len = all_targets().len();
    assert_eq!(
        core_len, target_len,
        "capability::all() has {core_len} host(s) but all_targets() has {target_len} backend(s).\n{RULE}",
    );
}
