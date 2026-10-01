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
//! - **stage 3 `lower_profile`** — the bound profile chain's rules produce the
//!   `ModelSurface{layout, tools, interaction_mode, params, transcript_renderer}`
//!   (§3.2.8): naming, `tool_shape` variants, schema dialect, description templates,
//!   error/result render specs, prompt layout; every element carries `rule_ids[]`.
//! - **stage 4 `lower_target`** — per bound target: `mcp` → `hh-mcp-target/1` (the
//!   canonical catalogue + the `_meta` minimum carried set + the ADR-0096 D5 hint
//!   projection); `provider_tool_api` → `hh-provider-request/1` (strict-subset
//!   schemas, layout, params, the ADR-0118 D6 normalisation record). Every dropped
//!   or narrowed field lands in a typed `LoweringLossReport` (§3.2.5's amended
//!   `{no_slot, hint_only, untyped_slot, narrowed, truncated}` classes).
//! - **stage 5 `seal_outputs`** — `derivation_key` (idp/1 over canonical inputs),
//!   `bundle_id` (idp/1 over canonical outputs), the total `trace_map`, the static
//!   `lcd_report` (composed from `validate_assembly`'s derived results — CF-050), and the
//!   per-surface `EquivalenceEvidence` — E1–E3/E7 static, **E4 executable** over the
//!   profile's declared `tests.e4_suites[]`, E5/E6 over the compiled specs.
//!
//! `lift` (`mcp`) recovers `PartialHIR{recovered, unknown, declared_unverified}`;
//! `relower` re-runs stages 3–5 under a new profile producing `TranscriptMigration`
//! (the `runtime_plan` is unchanged — checked). `probe_profile` (conformance is a
//! target-side property — ADR-0021 D3) remains staged later.
//!
//! The out-of-process seam is `hh-compile` (canonical `CompileInputs` bytes on stdin →
//! canonical bundle or typed diagnostics on stdout — AC-CP-11/T-LCD-12).

pub mod compiler;
pub mod e4;
pub mod equiv;
pub mod errors;
pub mod exposure;
pub mod lcd;
pub mod link;
pub mod lower;
pub mod plan;
pub mod profile;
pub mod profile_test;
pub mod regex;
pub mod relower;
pub mod retrieval;
pub mod schema;
pub mod seal;
pub mod surface;
pub mod target;
pub mod trace;

pub use compiler::{accept, accept_bytes, compile, CompileInputs};
pub use errors::{CompileError, LinkErrorKind, TraceError};
pub use relower::{relower, relowered_event, TranscriptMigration};
pub use target::{lift, lower_target, PartialHIR, HH_META_KEY};
