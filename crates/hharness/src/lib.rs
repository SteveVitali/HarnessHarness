//! `hharness` — the umbrella facade crate (ticket L4.5; ADR-0380; R-2.12.4's
//! packaging half).
//!
//! One dependency that re-exports every publishable public-API crate of the E1
//! HarnessHarness workspace under a short module name, so downstream code can
//! write `hharness::ledger`, `hharness::hir`, `hharness::embed`, … instead of
//! depending on each `hharness-*` package separately.
//!
//! The installable binaries ride their own crates and are not re-exported:
//! `cargo install hharness-cli` provides `hh`, `hharness-kernel` provides
//! `hh-kernel`, `hharness-helper` provides `hh-helper`. The packaged plugin
//! variants and fixtures (`plugins/hh-*`, `hharness-plugin-fixture`,
//! `hharness-tracker-fixture`, `hharness-github-issues`, …) are deliberately
//! outside the publish set (ADR-0380 (a)).
//!
//! Tier features forward the workspace's removability contract (CC6):
//! `tier-c1` (default) enables the C1 durability halves of
//! `hharness-env`/`hharness-helper`/`hharness-ledger`; `tier-c4` (default)
//! enables `hharness-embed`'s C4 surface and the facade's own `fleet` /
//! `evolution` / `debt` re-exports. Building with
//! `default-features = false` removes both layers honestly — the gated ops
//! answer typed `unsupported` refusals rather than vanishing silently.

// The Stage-0/1 kernel spine and Stage-1 service slices.
pub use hharness_assembly as assembly;
pub use hharness_budget as budget;
pub use hharness_compiler as compiler;
pub use hharness_containment as containment;
pub use hharness_context as context;
pub use hharness_control as control;
pub use hharness_embed as embed;
pub use hharness_embed_client_generated as embed_client;
pub use hharness_embed_schema as embed_schema;
pub use hharness_env as env;
pub use hharness_gateway as gateway;
pub use hharness_helper as helper;
pub use hharness_hir as hir;
pub use hharness_identity as identity;
pub use hharness_ledger as ledger;
pub use hharness_monitor as monitor;
pub use hharness_ontology as ontology;
pub use hharness_plugin as plugin;
pub use hharness_provenance as provenance;
pub use hharness_registry as registry;
pub use hharness_secrets as secrets;
pub use hharness_telemetry as telemetry;
pub use hharness_tls as tls;
pub use hharness_varhost as varhost;
pub use hharness_verification as verification;
pub use hharness_wire as wire;

// The Lab plane (spec §6) and the C3+ services.
pub use hharness_analysis as analysis;
pub use hharness_bench as bench;
pub use hharness_bundle as bundle;
pub use hharness_eval as eval;
pub use hharness_experiment as experiment;
pub use hharness_hosting as hosting;
pub use hharness_lab as lab;
pub use hharness_orchestrator as orchestrator;
pub use hharness_results as results;
pub use hharness_subagent as subagent;

// The C4 removable tier — present only under `tier-c4` (default on).
#[cfg(feature = "tier-c4")]
pub use hharness_debt as debt;
#[cfg(feature = "tier-c4")]
pub use hharness_evolution as evolution;
#[cfg(feature = "tier-c4")]
pub use hharness_fleet as fleet;
pub use hharness_fleet_adapter as fleet_adapter;

// The surfaces and the cross-implementation checker.
pub use hharness_acp as acp;
pub use hharness_cli as cli;
pub use hharness_mcp as mcp;
pub use hharness_mcp_lab as mcp_lab;
pub use hharness_web as web;
pub use hharness_xcheck as xcheck;
