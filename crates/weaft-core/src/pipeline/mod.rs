//! The v2 compilation pipeline stages that live in `weaft-core` (the pure, emission-free
//! half of the seven-stage ADR-0001 pipeline: parse → **resolve** → render → **map** →
//! serialize → emit → merge).
//!
//! Core owns the *pure* stages that read only the capability matrix and the IR:
//! - [`resolve`] — decides native / fold / drop and the effective target kind for an
//!   `(artifact, host)` pair, before rendering.
//! - [`map`] — turns canonical artifact fields into host-keyed, ordered output fields, reading
//!   each field from the source its [`crate::capability::FieldSource`] names (Fix 1).
//! - [`emit`] — computes the output spec (relative path from the layout template + merge flag)
//!   for a resolved artifact and its framed body. Pure: no byte check (the merged size does not
//!   exist yet — that enforcement lives in `weaft-cli`'s `merge_and_check`).
//!
//! The `serialize` / `merge` stages and the transform *impls* live in `weaft-targets` /
//! `weaft-cli` (they pull in emission deps and see both crates' slices). `emit` is pure path/spec
//! computation, so it stays here; the byte-budget enforcement that needs the merged size lives in
//! `weaft-cli`'s `merge_and_check` (WU-14, Fix 2/3).

pub mod emit;
pub mod map;
pub mod resolve;
