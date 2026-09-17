//! `hh-compiler` — the **compilation model** (spec §3.2; ticket S1.10; R-2.1.3).
//!
//! A compiler is a pure, staged function: *canonical input → typed documents* (ADR-0019).
//! It emits runtime-interpreted data — never host-language code — so the compiled artifact
//! is reproducible, content-addressed, traceable, and attributable to HIR identities.
//!
//! Stages (§3.2.2):
//! - **stage 0 `accept`** — sealed definitions only; re-asserts the canonical-form hash
//!   against the recorded `version_id`; calls `hh_assembly::validate_assembly` (never
//!   re-implements `validate` — ADR-0025 D2).
//! - **stage 1 `link`** — `hh_assembly::link_precheck` first (a surviving selector is
//!   `C-LINK-1` `LinkError{unbound_slot}` — the compiler never re-resolves; DF-S1.9-3's
//!   consumer half); binds the profile chain and target specs; checks
//!   `capability_requires` (ADR-0087), conditioned-rule debt completeness (T-LCD-05), and
//!   that every slot is bound. `NoProfile` unless an explicit `fallback_profile` with a
//!   dated debt hypothesis is bound (ADR-0124 §5).
//! - **stage 2 `lower_native`** — the closed `RuntimePlan/1` node set `{loop, step,
//!   branch-on-validator, delegate, stop-rule}`; every node carries `hir_node_id`; the
//!   control boundary lowers into the budget-envelope + stop-rule nodes (ADR-0106);
//!   policy tables derive only from `Permission`/`EffectClass`/`Budget`; an unsupported
//!   construct is `PlanError{unsupported_construct}` — never a silent fallback.
//! - **stage 5 `seal_outputs`** — `derivation_key` (idp/1 over canonical inputs),
//!   `bundle_id` (idp/1 over canonical outputs), the total `trace_map`, the static
//!   `lcd_report` (composed from `validate_assembly`'s derived results — CF-050), and the
//!   per-surface `EquivalenceEvidence` (E1–E3 + E7 static; E4 `n/a{open-world}` for
//!   open-world capabilities — T-LCD-15; E5/E6 `n/a{stage_3}` — §5b/§5f own them).
//!
//! Staged later (§3.2.14 — **not** this crate yet): stage 3 `lower_profile` /
//! `ModelSurface` production, stage 4 `lower_target` / `TargetArtefact`s /
//! `LoweringLossReport`s, `lift`, `relower`, `probe_profile` (conformance is a target-side
//! property — ADR-0021 D3).
//!
//! The out-of-process seam is `hh-compile` (canonical `CompileInputs` bytes on stdin →
//! canonical bundle or typed diagnostics on stdout — AC-CP-11/T-LCD-12).

pub mod compiler;
pub mod equiv;
pub mod errors;
pub mod lcd;
pub mod link;
pub mod plan;
pub mod profile;
pub mod schema;
pub mod seal;
pub mod trace;

pub use compiler::{accept, accept_bytes, compile, CompileInputs};
pub use errors::{CompileError, LinkErrorKind, TraceError};
