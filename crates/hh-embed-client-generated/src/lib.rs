//! `hh-embed-client-generated` — the generated `hh-embed/1` client.
//!
//! Everything of substance lives in `generated.rs`, which is **emitted by `hh-codegen`** from
//! the single schema source and checked in. Do not edit `generated.rs` by hand; the CI drift
//! check regenerates it and fails the build on any diff (CC7). This `lib.rs` is the stable
//! wrapper and is safe to hand-edit.
//!
//! In the polyglot split (ADR-0050) the real third-party clients are generated in the lab
//! (E2) and surface (E3) ecosystems; this in-ecosystem client is the Stage-0 stand-in that
//! exercises the schema-export → codegen → round-trip pipeline before those ecosystems bind
//! (Stage 3 / Stage ≥ 5).

mod generated;
pub use generated::*;
