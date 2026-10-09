<!-- docs/tickets/00_MANIFEST.md (BM-MANIFEST-01..04). Written by decompose-spec. COMPLETE ON ITS OWN:
     a human with a terminal can drive the chain from it without the ledger. -->
# harnessharness manifest — the ticket chain

> **Committed contract record.** These ticket docs are a derived build artifact; they *cite*
> the spec and its requirement ids, they do not copy the design. This chain table is
> authoritative if it and a filename ever disagree.
> **Cite, don't copy — the spec amendment protocol.** To change a requirement: amend
> `spec/CANONICAL_SPEC.md`; bump its version/delta; requirement ids are append-only; add a
> `## Spec amendments applied` line (date · section · before/after or pointer · approver · ADR);
> write an ADR when a design decision changes; then update the affected tickets' Load/AC lines.
> **Deferrals companion.** `DEFERRALS.md` in this directory — read it first every run.

companions: DEFERRALS.md, _TEMPLATE.md
req_id_pattern: R-2\.[0-9]+\.[0-9]+[a-z]?

## How to build
Stacked-PR chain. Rules: (1) go in table order; (2) between tickets, stay on the previous
ticket's branch so PRs stack; (3) STOP at every GATE row (GATE-G1/G2/G3, GATE-ACCEPT, GATE-G4) until the
operator reads it out — never guessed past; (4) HUMAN rows (HUMAN-H1/H2, Round-2's H3/H4) are operator work —
start them when the table reaches them and never let a code ticket silently block on one (open a
`DEFERRALS.md` row and continue). Run line, per row:
`implement-spec spec=docs/tickets/<file> <per-ticket flags>`. `orchestrate-build` drives this from
`docs/build/LEDGER.md`; a human can drive it by hand from this table alone.

The spine is the spec's **§9 staged build ladder** (Stage 0 → 6d), core-first (C0 → C4): every
ticket is one scope-item **slice at one stage** (§4.4 both DAG tables), cut on the weakest-coupling
subsystem seam. Ordering follows the §4.4 topological order (`tier_violations=[]`, `cycles=[]`).

## Human prerequisites
**HUMAN-H1 — Bind the surface (E3) ecosystem for generated clients (OQ-130 / WS-K2)** (Stage 4). Blocks: S4.10 (read-only web instrument), S5.7 (web-surface editing), S5.8 (surface ecosystem + compat). A code ticket reaching these before the binding records a `DEFERRALS.md` row (proxy = the SDK generated client) and reports 'gate pending' — never a failure. Exit: The surface ecosystem is bound (an ADR-0050 amendment-log row exists) and the read-only web instrument's generated clients build under the drift check.

**HUMAN-H2 — Provision a real issue-tracker + signed webhook for the Stage-5 fleet adapter** (Stage 5). Blocks: S5.6 (fleet adapters). If not provisioned, S5.6 runs against the Stage-4 fixture adapter and opens a `DEFERRALS.md` row (proxy = fixture adapter) — never a fabricated pass. Exit: The real tracker + signed webhook are provisioned and the fleet adapter's capability probe succeeds; credentials are broker-held (never in a ledger).

**HUMAN-H3 — Provision the foreign-verification environment** (Round 2). Blocks: R2.21's foreign
cells only (the offline packaging proceeds regardless). If unprovisioned when the chain reaches row
129, R2.21 lands its offline half and the named DF rows carry dated 'environment pending' notes —
the foreign-verification claims stay withheld, never fabricated.

**HUMAN-H4 — R2 cross-camp reviewer signature** (Round 2). Blocks: the DF-S0.3-3 residual (BL-02);
gates no code ticket. Carried through GATE-ACCEPT's accepted-deviations set — a second deferral
needs the operator's explicit words; dispositioned at GATE-G4.

## The chain

| # | file | phase | kind | scope | gate |
|---|---|---|---|---|---|
| 1 | `001_S0.1__toolchain-codegen.md` | Stage 0 | ticket | Bind the E1 kernel/helper ecosystem and stand up the repository layout, CI, and the sche… | — |
| 2 | `002_S0.2__hand-authored-baseline.md` | Stage 0 | ticket | A hand-authored `react/minimal` definition runs one headless coding task end-to-end thro… | — |
| 3 | `003_S0.3__stage0-spikes.md` | Stage 0 | ticket | Run the S1/S2 measurement spikes — fan-out/footprint (C5/C7) and boundary-crossing cost… | — |
| 3a | `003a_S0.3b__cross-candidate-online-spike.md` | Stage 0 | ticket | Cross-candidate / MCP-ACP **online** measurement spike — closes the machine cells of DF-S0.3-1/-3 (inserted 2026-09-15) | — |
| 4 | `004_GATE-G1__stage0-ecosystem-revalidation.md` | Stage 0 | gate | Stage-0 acceptance & ecosystem-decision revalidation | STOP |
| 5 | `005_S1.1__ontology-formal-model.md` | Stage 1 | ticket | Land the ontology and formal model: the seven-plane home-plane rule, the policy-stack fo… | — |
| 6 | `006_S1.2__identity-versioning.md` | Stage 1 | ticket | Land the identity/versioning/artifact-identity schemas in the single schema source: `idp… | — |
| 7 | `007_S1.3__provenance-authority.md` | Stage 1 | ticket | Land the provenance & authority model: one `ProvenanceRecord`, the seven `AuthorityClass… | — |
| 8 | `008_S1.4__harness-ir-dialect.md` | Stage 1 | ticket | Land the Harness IR: the HIR/1 schema with `validate`/`canonicalize`/`identity`/`seal`/`… | — |
| 9 | `009_S1.5__run-ledger.md` | Stage 1 | ticket | Land the event store / run ledger: the envelope and id model, `open_run`/`append`/`read`… | — |
| 10 | `010_S1.6__budgets-accounting.md` | Stage 1 | ticket | Land the resource-economics & accounting model: the kernel dimension list, budget/cost p… | — |
| 11 | `011_S1.7__effect-model-durability-slice.md` | Stage 1 | ticket | Land the effect & transaction model and the Stage-1 durability slice: the effect family… | — |
| 12 | `012_S1.8__registry-store-slice.md` | Stage 1 | ticket | Land the registry store slice `R-2.10.2⁰`: the store, `hh/` + `local/` namespaces, `regi… | — |
| 13 | `013_S1.9__config-composition.md` | Stage 1 | ticket | Land the configuration & composition model: the assembly grammar, `ClassRecord`/`Variant… | — |
| 14 | `014_S1.10__compilation-model.md` | Stage 1 | ticket | Land the compilation model: compiler stages 0–2 and 5, `RuntimePlan/1`, lowering, `deriv… | — |
| 15 | `015_S1.11__reference-monitor.md` | Stage 1 | ticket | Land the reference-monitor security kernel & capability model: the `AuthorityHandle` tab… | — |
| 16 | `016_S1.12__egress-network.md` | Stage 1 | ticket | Land the egress / network policy & sandbox boundaries: `ContainmentPolicy/1`, `effective… | — |
| 17 | `017_S1.13__credential-broker.md` | Stage 1 | ticket | Land the credential mediation & secret isolation: `SecretRef`/`SecretChannel`, the broke… | — |
| 18 | `018_S1.14__telemetry.md` | Stage 1 | ticket | Land telemetry, tracing & cost/latency instrumentation: scope kinds M1–M12, `TokenVector… | — |
| 19 | `019_S1.15__audit-trail.md` | Stage 1 | ticket | Land the deep audit trail & tamper-evident logging: the audit-grade class list with Rule… | — |
| 20 | `020_S1.16__environment-and-sandboxed-exec.md` | Stage 1 | ticket | Land the execution-environment abstraction and sandboxed tool execution: the `local_host… | — |
| 21 | `021_S1.17__tool-registry-and-exposure.md` | Stage 1 | ticket | Land the tool registry & typed capabilities plus the C0 exposure/surface slices: `ToolCa… | — |
| 22 | `022_S1.18__model-gateway-and-profile-slices.md` | Stage 1 | ticket | Land the model adapter / gateway and the C0 model-plane slices: closed stop-reason/error… | — |
| 23 | `023_S1.19__context-builder-and-memory-slices.md` | Stage 1 | ticket | Land the context builder / policy engine and the C0 memory slices: context-plan schemas… | — |
| 24 | `024_S1.20__control-strategy-and-envelope.md` | Stage 1 | ticket | Land the full C0 control strategy and control envelope: the `control_strategy` contract… | — |
| 25 | `025_S1.21__verification-substrate.md` | Stage 1 | ticket | Land the claim-ledger substrate and the C0 verification slices: `Claim`, `Reconciliation… | — |
| 26 | `026_S1.22__eval-framework.md` | Stage 1 | ticket | Land the eval framework: `MetricDeclaration`, `OracleDeclaration`, `Factor`/`Design`/`Ar… | — |
| 27 | `027_S1.23__approvals-and-extension-trust-slices.md` | Stage 1 | ticket | Land the C0 approvals and extension-trust slices: the `R-2.8.7⁰` Π floor rows with `Unat… | — |
| 28 | `028_S1.24__measurement-and-lab-schemas.md` | Stage 1 | ticket | Land the C0 measurement/Lab schema slices: `R-2.9.4⁰ᵃ` (`EnvironmentFamily`, task/suite/… | — |
| 29 | `029_S1.25__sdk-embed-contract.md` | Stage 1 | ticket | Land the `hh-embed/1` contract: Groups H/S/W/R, the frame model, the closed error sum, b… | — |
| 30 | `030_S1.26__attended-cli.md` | Stage 1 | ticket | Land the C0 attended CLI slice `R-2.11.1⁰ᵇ`: attended terminal mode with per-effect appr… | — |
| 31 | `031_S1.27__extensibility-plugin-architecture.md` | Stage 1 | ticket | Land the extensibility / plugin architecture: `PluginManifest/1`, `ContractRef`, `Contra… | — |
| 32 | `032_S2.1__helper-and-container.md` | Stage 2 | ticket | Move execution out of the kernel process: the helper binary (ADR-0050 helper layer), tok… | — |
| 33 | `033_S2.2__variant-host.md` | Stage 2 | ticket | Land the variant host with `subprocess_confined`: host callbacks, V1–V6, `GuardVerdict`,… | — |
| 34 | `034_S2.3__durable-execution.md` | Stage 2 | ticket | Land the durable execution & recovery item (C1): effect/resource leases, the four lease… | — |
| 35 | `035_S2.4__egress-mediation-and-credentials.md` | Stage 2 | ticket | Land egress mediation with attribution and the credential broker's mediation: `mediate`,… | — |
| 36 | `036_S2.5__audit-checkpoints.md` | Stage 2 | ticket | Land signed audit checkpoints: tree heads, signed `security.audit.checkpoint`, `prove_*`… | — |
| 37 | `037_S2.6__approvals-and-hooks.md` | Stage 2 | ticket | Land the HITL approvals/hooks item (C1) and the approval-basis handles: `approval`-basis… | — |
| 38 | `038_S2.7__ifc-taint-labeling.md` | Stage 2 | ticket | Land information-flow control & taint labeling (C2 item at S2): `FlowContract`, the pros… | — |
| 39 | `039_S2.8__context-label-and-memory.md` | Stage 2 | ticket | Land the kernel-stamped `context_label`, memory labels and the memory C0/C2 slices: memo… | — |
| 40 | `040_S2.9__branch-model.md` | Stage 2 | ticket | Land the branch model (`R-2.2.4⁰ᵃ`): `coherent_fork_points`, inter-run `fork`, `fs_tree`… | — |
| 41 | `041_S2.10__tool-scaling.md` | Stage 2 | ticket | Land tool scaling (C1 item): `lexical_regex`/`bm25`/`hierarchical` indexes, `discover_su… | — |
| 42 | `042_S2.11__steerable-control-and-nudges.md` | Stage 2 | ticket | Land `react/steerable` (`R-2.6.1¹`) and the Stage-2 control/verification slices: nudges… | — |
| 43 | `043_S2.12__attended-cli-item-and-registry-enforcement.md` | Stage 2 | ticket | Land the CLI item (C1) attended mode and the registry-enforcement slices: `run resume/fo… | — |
| 44 | `044_S3.1__lab-embed-groups-and-bundle.md` | Stage 3 | ticket | Cross the kernel↔lab boundary: Groups M and L for the Lab with `ContractIdentity` in bun… | — |
| 45 | `045_S3.2__compiler-full.md` | Stage 3 | ticket | Land the full compiler: `opacity`, `ablate`; compiler stage 3 with the two minimal profi… | — |
| 46 | `046_S3.3__metric-catalogue-and-compare.md` | Stage 3 | ticket | Land the metric catalogue as distributions, compliance metrics, the loss-report export;… | — |
| 47 | `047_S3.4a__experiment-engine.md` | Stage 3 | ticket | Land the single-worker experiment engine (`R-2.10.3⁰ᵇ`): `register`/`expand` with the fu… | — |
| 48 | `048_S3.4b__results-store.md` | Stage 3 | ticket | Land the results store (`R-2.10.5⁰`): `ResultsRow/1`, `project_row`, row versions, `buil… | — |
| 49 | `049_S3.4c__estimator-kernel.md` | Stage 3 | ticket | Land the estimator kernel (`R-2.10.4⁰ᵇ`): A1/A2/A3 `contrast`/A8/A12, transfer rows, KA-… | — |
| 50 | `050_S3.4d__hosting-abi-schemas.md` | Stage 3 | ticket | Land the Hosting ABI schemas (`R-2.10.6⁰`): `HostedEvent`, `proj_ABI`, `ConformanceRecor… | — |
| 51 | `051_S3.5__assembly-service.md` | Stage 3 | ticket | Land the assembly service (C1): `AssemblySource` + D1–D5, `assemble/plan/apply/validate_… | — |
| 52 | `052_S3.6__replay-fault-and-counterfactual.md` | Stage 3 | ticket | Land the replay driver and fault battery: both replay modes (`R-2.2.4⁰ᵇ`), `ReplayValidi… | — |
| 53 | `053_S3.7__model-plane-eval.md` | Stage 3 | ticket | Land the Stage-3 model-plane executables: golden streams and drift probes; the static ro… | — |
| 54 | `054_S3.8__context-memory-eval.md` | Stage 3 | ticket | Land the Stage-3 context/memory executables: `clear_tool_results`, the two-arm `lab/comp… | — |
| 55 | `055_S3.9__tool-protocol-edges.md` | Stage 3 | ticket | Land the protocol edges (`R-2.5.4⁰`): `import_listing`/`refresh` for `mcp_listing`, `syn… | — |
| 56 | `056_S3.10__control-verification-eval.md` | Stage 3 | ticket | Land the Stage-3 control/verification executables: the T-LCD-03 round trip; `lab/control… | — |
| 57 | `057_S3.11a__security-eval-monitor-ifc.md` | Stage 3 | ticket | Land the Stage-3 monitor/IFC security executables (§5g.1–§5g.2): AC-R-2.1.5-3/-4/-7/-10/… | — |
| 58 | `058_S3.11b__security-eval-broker-audit-trust.md` | Stage 3 | ticket | Land the Stage-3 credential/egress/audit/trust/approval security executables (§5g.3–§5g.… | — |
| 59 | `059_S3.12__bundle-and-plugin-conformance.md` | Stage 3 | ticket | Land the run-bundle completeness and plugin conformance: `BundleManifest` members, `chec… | — |
| 59a | `059a_S3.12b__gate-g2-gap-closure.md` | Stage 3 | ticket | GATE-G2 gap closure — hermetic benchmarkSet, real-suite legs, exemplar→compare, hosting-absent run, DEFERRALS sweep (inserted 2026-09-25) | — |
| 60 | `060_GATE-G2__stage3-first-claim.md` | Stage 3 | gate | Stage-3 evaluation-first acceptance — the first claim about a harness may exist | STOP |
| 61 | `061_HUMAN-H1__bind-surface-ecosystem.md` | Stage 4 | human | Bind the surface (E3) ecosystem for generated clients (OQ-130 / WS-K2) | — |
| 62 | `062_S4.1__registry-service.md` | Stage 4 | ticket | Land the component-variation registry service (C1): the multi-namespace service with sig… | — |
| 63 | `063_S4.2__experiment-service.md` | Stage 4 | ticket | Land the experiment & sweep engine service (C1): multi-worker `claim`, pools, `pause/res… | — |
| 64 | `064_S4.3__analysis-engine-surfaces.md` | Stage 4 | ticket | Land the comparison & analysis engine C1 surfaces (`R-2.10.4¹`): A4 frontier, A5/A6, A9… | — |
| 65 | `065_S4.4__results-leaderboard-publication.md` | Stage 4 | ticket | Land the results-store publication item (C1): `arm`/`experiment`/`lineage` bundle kinds… | — |
| 66 | `066_S4.5a__hosting-abi.md` | Stage 4 | ticket | Land the Hosting ABI proper (C2): baseline and capability-declared verbs, session-ABI li… | — |
| 67 | `067_S4.5b__protocol-edges-and-acp.md` | Stage 4 | ticket | Land the protocol edges item (C1) and the ACP session transport: `R-2.5.4` C1 (`Protocol… | — |
| 68 | `068_S4.6__subagent-orchestrator.md` | Stage 4 | ticket | Land the sub-agent orchestrator (C3) and its C1 kernel slice: `spawn` steps 1–8, SP-1…SP… | — |
| 69 | `069_S4.7__value-of-compute-scheduler.md` | Stage 4 | ticket | Land the value-of-compute scheduler (C3): `compute_policy` with `static` + `rules`, `com… | — |
| 70 | `070_S4.8__coordination.md` | Stage 4 | ticket | Land multi-agent coordination & consistency (C3): `scoped_subtree`/`share` with `resourc… | — |
| 71 | `071_S4.9__fleet-reconciler.md` | Stage 4 | ticket | Land the human-agent organizational layer (C4) fleet reconciler: `run_kind = fleet`, `Wo… | — |
| 72 | `072_S4.10__read-only-web-instrument.md` | Stage 4 | ticket | Land the C1 read-only web instrument (`R-2.11.2¹`): V1, V2, V3-scrub, V5-monitor, V6 sco… | — |
| 73 | `073_S4.11__mcp-server-lab-groups.md` | Stage 4 | ticket | Land the C1 `hh-lab/1` MCP server slice (`R-2.11.3¹`): the read/launch/experiment/result… | — |
| 74 | `074_S4.12__sdk-full-and-serve.md` | Stage 4 | ticket | Land the SDK / embedding API item (C1) and `serve`/`acp`: `R-2.11.4` full item; the CLI'… | — |
| 75 | `075_S4.13__hosted-lineage-and-durability.md` | Stage 4 | ticket | Land the hosted/subagent durability slices: subagent events and lineage, hosted ingestio… | — |
| 76 | `076_S4.14a__supply-chain-trust-and-third-party-install.md` | Stage 4 | ticket | Land extension supply-chain trust (C1) and third-party install: `TrustRootPolicy`, signa… | — |
| 77 | `077_S4.14b__security-enrichments-subagent-hosted.md` | Stage 4 | ticket | Land the subagent/hosted security enrichments: `user_space_kernel`/`microvm` with `attes… | — |
| 78 | `078_S4.15__measurement-c1-and-hosted.md` | Stage 4 | ticket | Land the C1 measurement depth: stratum B once its `SuiteValidityRecord` exists, F profil… | — |
| 79 | `079_S4.16a__router-c1.md` | Stage 4 | ticket | Land the C1 model router depth (§5b.2): the router policy family with retry-vs-reroute,… | — |
| 80 | `080_S4.16b__retrieval-and-memory-c1c2.md` | Stage 4 | ticket | Land the C1/C2 retrieval and memory depth (§5c): the ranker family, out-of-process store… | — |
| 81 | `081_S4.16c__verification-c1c2.md` | Stage 4 | ticket | Land the C1/C2 verification depth (§5f): judged validators, hosted `end_state` verdicts… | — |
| 82 | `082_HUMAN-H2__provision-real-tracker.md` | Stage 5 | human | Provision a real issue-tracker + signed webhook for the Stage-5 fleet adapter | — |
| 83 | `083_S5.1__profile-compiler-and-router.md` | Stage 5 | ticket | Land the Profile Compiler rule family and the router bandit family: probes, chains, rend… | — |
| 84 | `084_S5.2__compaction-and-tool-surfaces.md` | Stage 5 | ticket | Land the full compaction family and split/composite tool surfaces: the compaction family… | — |
| 85 | `085_S5.3__analysis-c2.md` | Stage 5 | ticket | Land the comparison & analysis engine C2 depth: A7 `fit_surface` with `factorial_glmm`/`… | — |
| 86 | `086_S5.4__designed-ablation-and-live-debt.md` | Stage 5 | ticket | Land M1 designed ablation, live debt, provider-drift compatibility and R2 reproduction:… | — |
| 87 | `087_S5.5__orchestration-c3-depth.md` | Stage 5 | ticket | Land the C3 orchestration depth that needs Stage-4 data: T3 recursive full harness with… | — |
| 88 | `088_S5.6__fleet-adapters.md` | Stage 5 | ticket | Land the C4 fleet adapters: signed-webhook ingress with capability probes; one real trac… | — |
| 89 | `089_S5.7__web-surface-editing.md` | Stage 5 | ticket | Land the C2 web surface and MCP write/analysis groups: V4, V5-launcher, V3-operations, V… | — |
| 90 | `090_S5.8__surface-ecosystem-and-compat.md` | Stage 5 | ticket | Land the surface ecosystem and compatibility machinery: generated clients in the surface… | — |
| 91 | `091_GATE-G3__stage6-preconditions.md` | Stage 6 | gate | Stage-6 evolution preconditions (all mandatory) | STOP |
| 92 | `092_S6.1a__evolution-pipeline-recipe.md` | Stage 6 | ticket | Land sub-stage 6a — the pipeline as a Lab recipe (`instrument-grade`): human-proposed ca… | — |
| 93 | `093_S6.1b__assumption-debt-manager.md` | Stage 6 | ticket | Land sub-stage 6a — the assumption-debt manager service (R-2.9.6): `schedule_removal_tes… | — |
| 94 | `094_S6.2__automated-family-one-class.md` | Stage 6 | ticket | Land sub-stage 6b — one automated family, one component class (`instrument-grade` candid… | — |
| 95 | `095_S6.3a__multi-family-code-search.md` | Stage 6 | ticket | Land sub-stage 6c — multi-family code search and hosted coordinates (`research-grade`):… | — |
| 96 | `096_S6.3b__causal-attribution-and-judge-integrity.md` | Stage 6 | ticket | Land sub-stage 6c — designed causal attribution and judge integrity (`research-grade`):… | — |
| 97 | `097_S6.4__co-evolution-interface.md` | Stage 6 | ticket | Land sub-stage 6d — the co-evolution interface (`research-grade`, conditional): `export_… | — |
| 98 | `098_CAP.1__capstone-gap-analysis.md` | capstone | capstone | capstone gap analysis | — |
| 99 | `099_CAP.2__capstone-composed-verification.md` | capstone | capstone | capstone composed verification | — |
| 100 | `100_CAP.3__capstone-closure.md` | capstone | capstone | capstone closure | — |
| 101 | `101_GATE-ACCEPT.md` | capstone | gate | operator signs accepted deviations | STOP |
| 102 | `102_REC.1__backlog-and-readiness.md` | reconcile | reconcile | backlog + operational readiness | — |
| 103 | `103_REC.2__spec-reconciliation.md` | reconcile | reconcile | spec reconciliation | — |
| 104 | `104_REC.3__integration-plan.md` | reconcile | reconcile | integration plan | — |
| 105 | `105_DOC.1__repo-docs-refresh.md` | docs | docs | repo-docs refresh | — |
| 106 | `106_DOC.2__agent-docs-refresh.md` | docs | docs | agent-docs refresh | — |

### Round 2 — 2026-10-06 · carried-forward backlog discharge (decompose-spec mode=extend)

Rows 107–136 append below Round-1 history unchanged. Sources: `docs/build/planning/2026-10-06_decision-memo.md`,
`docs/build/BACKLOG.csv` (BL-01…BL-43), `docs/tickets/DEFERRALS.md` open/residual set. The ten
GATE-ACCEPT signed deviations are accepted, not owed — out of scope. `DF-S2.8-1`'s steer-delivery leg
is assigned to R2.6 (single owner of the durable steer-cue seam; `*` on row 111).

| # | file | phase | kind | scope | gate |
|---|---|---|---|---|---|
| 107 | `107_R2.1__round2-hygiene.md` | Round 2 | ticket | Round-2 hygiene: fix the `assembly_ms` ledger-parity flake, rule + fix `hh-kernel doctor` (DF-DOC.1-1), verify-and-flip unflipped discharges (DF-S1.3-2/-9-4/-26-2; BL-29) | — |
| 108 | `108_R2.2__ledger-retention-gc.md` | Round 2 | ticket | Ledger producer legs: tiered retention (hot/warm/cold), compression at rest, gc/escalation emitters (DF-S1.5-1, DF-S2.9-1; BL-11) | — |
| 109 | `109_R2.3__durable-execution-legs.md` | Round 2 | ticket | Durable-execution producers: suspend/compensate/heal entry points, healing_policy_ref, remaining wakeup producers (DF-S2.3-1; BL-12) | — |
| 110 | `110_R2.4__env-ops-snapshot-cadence.md` | Round 2 | ticket | Environment lifecycle boundary ops + automatic fs_tree/memory snapshot cadence (DF-S2.9-3, DF-S2.10-1; BL-13) | — |
| 111 | `111_R2.5__context-memory-producers.md` | Round 2 | ticket | Context/memory producer legs + §5c battery: driver call sites, resume_set consumers, detectors, OOP corpus (DF-S2.8-1*, DF-S1.19-1/-2; BL-15) | — |
| 112 | `112_R2.6__control-steer-interpreters.md` | Round 2 | ticket | Control-plane legs: durable steer-cue transport (OQ-316 arm), queue_next_turn, steerable/plan_execute interpreters, OOP suite, env kill (DF-S2.11-1, DF-S1.20-1; BL-14) | — |
| 113 | `113_R2.7__model-plane-legs.md` | Round 2 | ticket | Model-plane producer legs (machine cells): model.* emitters, non-static routing arms, WireDialect corpus, K4/K5 (DF-S1.18-1; BL-31 partial) | — |
| 114 | `114_R2.8__tool-exposure-residual.md` | Round 2 | ticket | Tool-exposure residual: C1 bindings/ids/indexes/renderers, C2 control + runtime emitters, ≥200-surface battery (DF-S1.17-1/-2/-3; BL-16) | — |
| 115 | `115_R2.9__egress-containment-legs.md` | Round 2 | ticket | Egress/containment residual: amend(), T-CON members, tls.terminate enforcement + inspect_hooks, ask-endorsement ingress, fork rebind, leak-scan surfaces (DF-S1.12-*, DF-S2.4-1/-3; BL-18) | — |
| 116 | `116_R2.10__credential-broker-legs.md` | Round 2 | ticket | Credential-broker legs: audience sender-constraint verify, honest dpop, canary channel in every Stage-3 env image (DF-S1.13-1/-3; BL-19) | — |
| 117 | `117_R2.11__approvals-monitor-acceptance.md` | Round 2 | ticket | Approval UX + Π acceptance: ApprovalOption.modify, ApproverGrant + label.endorsed call site, check_write@prepare, depth-3 (DF-S1.23-1, DF-S1.11-2; BL-22+23) | — |
| 118 | `118_R2.12__ifc-residual.md` | Round 2 | ticket | IFC residual: labels_leaves per-leaf walk, remedy-ingress consume path, shape_endorsed produce-time caller (DF-S2.7-1; BL-20) | — |
| 119 | `119_R2.13__audit-residual.md` | Round 2 | ticket | Audit residual: witness cosignatures, receiver receipts, gc/redaction emitters (DF-S1.15-3, DF-S1.15-1 non-custody; BL-21) | — |
| 120 | `120_R2.14__telemetry-exporter-seam.md` | Round 2 | ticket | Telemetry emitters + live exporter seam: M5/M6/M13-15 classes, turn_phase_profile, subscriber runtime + egress-mediated sinks (DF-S1.14-1/-2/-4; BL-24) | — |
| 121 | `121_R2.15__verification-plane-legs.md` | Round 2 | ticket | Verification-plane legs: postconditions, diff_sanity, OOP validator suite, computed metrics, judge binding, belief probes (DF-S1.21-*; BL-17) | — |
| 122 | `122_R2.16__eval-residual-battery.md` | Round 2 | ticket | Eval residual ops + fault battery: declare_* semantics, combined-failure fixture, unknown budget, LeakedSplit, HAL levels (DF-S1.22-1/-2; BL-25) | — |
| 123 | `123_R2.17__bench-measurement-legs.md` | Round 2 | ticket | Bench/measurement legs: resolver-backed validate_removal_test, DebtPolicy consumers, task_id round-trip (DF-S1.24-1/-2 machine cells; BL-26) | — |
| 124 | `124_R2.18__assembly-residual.md` | Round 2 | ticket | Assembly residual: §5b profile_binding constraint grammar, C2 organisation layers, AC-CC-11 corpus gate (DF-S1.9-2; BL-10) | — |
| 125 | `125_R2.19__embed-mcp-surface-legs.md` | Round 2 | ticket | Embed/MCP surface: DefinitionInput::Ref publishing, Streamable-HTTP SSE (GET /mcp, push, Last-Event-ID), WS-H3 credential mediator seam (DF-S1.25-1, DF-S4.11-1/-2; BL-27) | — |
| 126 | `126_R2.20__extension-trust-producers.md` | Round 2 | ticket | Extension-trust producers: DeclaredSource scanners, LegCrossing/L3 runtime checks, TextHygieneReport consumers (DF-S1.23-2; BL-28) | — |
| 127 | `127_HUMAN-H3__foreign-verification-env.md` | Round 2 | human | Provision the foreign-verification environment — E2/E3 toolchains, online corpus + budget (blocks R2.21's foreign cells) | — |
| 128 | `128_HUMAN-H4__r2-cross-camp-signature.md` | Round 2 | human | R2 cross-camp reviewer signature on the Stage-0 measurement sheet (DF-S0.3-3 residual; BL-02) | — |
| 129 | `129_R2.21__cross-impl-verification.md` | Round 2 | ticket | Cross-implementation verification: golden-corpus/transcript/plugin/snapshot replays vs E2/E3, polyglot-CI leg, live foreign-manifest import + LT-03 live arm (BL-01) — Live stage: operator-gated on H3 | — |
| 130 | `130_R2.22__revalidate-adr-0212-0216.md` | Round 2 | reconcile | Register revalidation: ADR-0212 Stage 0–1 + ADR-0216 fixer packages (12 OQs + ADR-0242 legs; OQ-465/467/468; BL-35, BL-39) | — |
| 131 | `131_R2.23__revalidate-adr-0213.md` | Round 2 | reconcile | Register revalidation: ADR-0213 Stage 2–3 package (23 OQs + ADR-0278's OQ-363; BL-36) | — |
| 132 | `132_R2.24__revalidate-adr-0214.md` | Round 2 | reconcile | Register revalidation: ADR-0214 Stage 4–6 package (34 OQs + ADR-0306; BL-37) | — |
| 133 | `133_R2.25__spec-debt-first-sweep.md` | Round 2 | reconcile | Spec-debt register first revalidation — RK-06 sweep over 216 AssumptionDebtRecords (BL-40) | — |
| 134 | `134_R2.26__round2-closure.md` | Round 2 | reconcile | Round-2 closure: backlog refresh, coverage recompute, OPERATIONAL_READINESS, carried-set accounting for the gate (BL-30/-41/-42/-43) | — |
| 135 | `135_GATE-G4__round2-discharge.md` | Round 2 | gate | Round-2 discharge review — operator dispositions every still-OPEN scoped row | STOP |
| 136 | `136_R2.27__round2-docs-refresh.md` | Round 2 | docs | Round-2 docs refresh — repo docs + agent docs honest-status pass (combined DOC template) | — |

## Milestone gates
**GATE-G1 — Stage-0 acceptance & ecosystem-decision revalidation** (Stage 0). Criterion (verbatim): S1/S2 gates pass and the revalidation rule is evaluated: any C5/C7/C12 score outside the assumed ±1 band, or a gate failure on the winning candidate, re-runs ADR-0009 steps 5–7 (§10.6; OQ-131).

Thresholds: G1–G4 pass; M-S1-1…9 and M-S2-1…7 within the pre-registered bands (M-S2-6 pass, M-S2-7 = 100 %); ADR-0009 score bands per `synthesis/l1-spike-spec.md` §5; the ecosystem decision (ADR-0050) holds or the re-run is triggered.

**GATE-G2 — Stage-3 evaluation-first acceptance — the first claim about a harness may exist** (Stage 3). Criterion (verbatim): The Stage-3 suite is green: the automated gates and executable tests of §10.7 pass; `removability(0…3)` holds and the hosting-absent build passes the native suite (AC-R-2.10.6-5; AC-R-2.11.4-12; AC-R-2.12.2-12). After this stage a claim about a harness may exist (§10.0).

Thresholds: §10.7 automated gates + executable tests (T-LCD-03/-04/-11/-13 executable; T-05/-08/-09/-14/-15 complete); `tier_violations=[]`, `cycles=[]`, `hosting_edges=[]`; both exemplars produce `ComparisonReport{budget_match.status}`; `removability(0…3)` unchanged; the hosting-absent build passes the native suite. OQ-338/OQ-342 block headline eligibility, not Stage-3 acceptance (stratum A carries it).

**GATE-G3 — Stage-6 evolution preconditions (all mandatory)** (Stage 6). Criterion (verbatim): All mandatory preconditions hold before the governed evolution pipeline runs (§9.7): Stage 3 complete; AT-H1-05 passing (the evolution service's handle set, ADR-0053 D-5); the ADR-0135 counterfactual hook and factual arm; `exp/` namespaces; the `retirement` kind and the ADR-0126 removal test; `plugin_abi/1` `subprocess_confined` for evolution-service plugins (ADR-0182 D8); the debt schemas and Stage-3 retirement fixtures (`R-2.9.6⁰ᵃ`, `R-2.9.6⁰ᵇ`; ADR-0209 D2).

Thresholds: Every precondition above is demonstrably met (each cites its landed ticket/AC). No evolution claim runs until all hold; every Stage-6 claim is a `ComparisonReport{benefit_kind: artifact_benefit, label: confirmatory}` at `matched_total` with a complete `SearchBudgetRecord`, `transfer` with sign and interval, no veto regression, `retention` within margin (§10.2 gates 1–6).

**GATE-ACCEPT — operator signs the accepted-deviations list** (capstone). Every row in the CAP.3 ACCEPTED-deviations list is signed or returned to closure; no proposed deviation is left unsigned.

**GATE-G4 — Round-2 discharge review** (Round 2). Criterion: every DEFERRALS row and backlog source
the Round-2 tickets were chartered to discharge is DONE with dated evidence, or carried forward with
an explicit operator disposition (carry / accept / waive by ADR). The environment-gated and human
rows are dispositioned as a named set — 'gate pending' is a valid recorded state, a silent drop is
not. Thresholds: zero undispositioned OPEN scoped rows; check-build-memory 0 violations;
check-backlog exit 0; R2.26's carried-set accounting exists; the 10 GATE-ACCEPT deviations
re-confirmed unchanged.

## Phase gates & ownership notes
- **Removability** (CC6): from Stage 2, every stage gate runs `removability(0…3)` — the build with
  tiers > n absent passes the tier-n acceptance suite unchanged; the hosting-absent, C3-absent and
  C4-absent builds are the executable forms of T-LCD-06 and ADR-0182 X4. This is a per-ticket AC at
  Stages 2–6, not a separate marker.
- **Evaluation-first** (GATE-G2): Stage 3 is the first stage at which a claim about a harness may
  exist; no Stage-4+ service/hosting/surface/orchestration/evolution work begins until it passes.
- **Matched-budget & maturity** (CC9): C3/C4 items are specified unconditionally but their *value* is
  matched-budget conditional; a `research-grade` result never enters a C0–C3 acceptance decision or a
  leaderboard without independent reproduction (ADR-0209 D3).
- **Shared decisions owned by the earliest ticket:** schemas land once (S1.2 identity, S1.3
  provenance, S1.6 budgets, S1.24 measurement/Lab schemas, S1.27 plugin arch); later tickets *cite*
  them and never redefine (CC1/CC7/CC10; propose a CF row instead).
- **Out-of-chain / informational:** the Hosting ABI (S4.5) and hosted slices depend informationally on
  external participants; the surface ecosystem (HUMAN-H1) and real tracker (HUMAN-H2) are operator
  prerequisites with fixture-adapter proxies.

## Cross-cutting invariants
Every ticket's gap-analysis re-checks these; the capstone re-checks all.
- **CC1 — one scheme per concern (§8 rule 1)** — One `ProvenanceRecord` + `Label` lattice, one dimension taxonomy, one identity model, one `plugin_abi/1`. A second class/label/dimension/id-kind/extension-protocol anywhere is a defect (ADR-0033/0039/0036/0181; ADR-0008).
- **CC2 — authority is conferred, never read from content, never widened (§8 rule 2 / X6)** — Authority is derived by the runtime, never declared; narrow-or-preserve; endorsement basis closed; `authority_delta ∈ {none,narrowing}`, `budget_delta ∈ {none,tightening}`; authority never flows up the DAG; `SelfModificationRefused` (ADR-0034/0035/0181 D4/0182 D7).
- **CC3 — nothing unaccounted, unpinned, or silently lost (§8 rule 3)** — Every accountable event posts a charge; every sealed/executed/published form contains only pinned ids; every lowering that cannot carry provenance/cost/foreign-digest records the loss (ADR-0039/0036/0034 P7/0038; T-LCD-11).
- **CC4 — operations over canonical records, language-neutral (§8 rule 4 / ADR-0050 §8)** — No contract, data model, criterion or stage names any language, runtime, library, API or build tool; ecosystems appear only by candidate id (E1/E2/E3) and layer. Implementable out of process (T-LCD-12).
- **CC5 — contract-only reach (X1/X3/X5)** — `depends_on` names only `ContractRef`; extensions interact only through ledger records / HIR entities / effects / registry records; a `substitutable` swap changes no other item's `depends_on`; a class-contract change is a new `contract_version`, never an in-place edit.
- **CC6 — tier monotonicity + removability (X2/X4)** — A C(n) item depends only on contracts of defining tier ≤ n; the item graph is acyclic (`tier_violations=[]`, `cycles=[]`); every C0 class ships its trivial Stage-1 variant; from Stage 2 every gate runs `removability(0..3)` unchanged.
- **CC7 — single schema source (V6)** — Every class-document and `plugin_abi/1` schema is exported from the kernel's single schema source and shared with the §7 kernel↔surfaces binding; plugin/client bindings are generated and CI-checked; schema drift is a build failure, never a runtime negotiation.
- **CC8 — additive / back-compat discipline** — Additive changes never bump `contract_version` (tri-state `hello` capabilities SUPPORTED/UNSUPPORTED/UNKNOWN, T-LCD-07); a breaking change is a new `contract_version` with a dated sunset window; adding an enum/dimension/placement value is an HIR dialect bump with migration; a rename keeps `semantic_id`, changes `version_id`; results pool by `configuration_id`.
- **CC9 — matched-budget-or-refuse; research-grade is walled off (§8.2 / ADR-0002)** — Every comparison arm carries `MatchSpec` + `search_budget` + `eval_budget`; a result without a `MatchSpec` is refused (typed), never warned or made comparable retroactively; a `research-grade` result never enters a C0–C3 acceptance decision or a leaderboard without independent reproduction (ADR-0209 D3).
- **CC10 — home-plane / ownership boundaries** — Every entity kind, class, plane operator and event family declares exactly one home plane; a section that fixes records does not own the services (Π owned by §5g, `StopReason` by §5e, bundle format by §5h, `role_map` a §5b surface). Cite shared schemas by name+ADR; never redefine locally — propose a CF row.

## Out of scope
No ticket builds any of these (the scope-creep guard; ADR-0006 D3):
- N1 — not a general-purpose framework for others' core loops (the LCD trap)
- N2 — not a universal Harness ABI treating hosted participants as decomposable (ours is thin/observational)
- N3 — not a hosted collaboration / managed multi-device hosting product
- N4 — not a public leaderboard *service* (the Lab's own leaderboard definition + snapshot exports ARE in scope)
- N5 — not a new tool/agent/client protocol (MCP/A2A/ACP are lowering targets, not inventions)
- N6 — not behavioural interchangeability of models (one definition compiled per Model Profile)
- N7 — not a claim that generic beats bespoke (the Lab measures the gap)
- N8 — not autonomous self-deployment (evolution proposes; governance deploys)
- N9 — not a model-training stack (HarnessHarness never trains)
- N10 — not an IDE / chat product / end-user assistant surface
- N11 — not merely an evaluation harness (runtime harnesses are the object of study)
- N12 — not a language/runtime/framework commitment before WS-L1 (discharged by ADR-0050)
- N13 — not a benchmark author (benchmarks are integrated, never invented)
- Widening into N1, N2, N5, N6 or N12 requires a superseding ADR amending ADR-0006 — no ticket does it.

Inherited spec-level deferrals (tickets inherit; they do not re-decide):
- The 24 open contract-level minors — spec/READINESS_REPORT.md §10 (C-11, C-16, C-17, C-20, C-21, C-26, C-27, DAG-10 second half, DAG-11, DAG-12, TR-06, TR-07/TR2-04, LCD-04/05/06/08, LCD-R2-05, r2 C-12/C-16, E2-05/09/11/17). None changes a contract, data model, criterion or stage; each names its owning workstream. A ticket touching one of these loci inherits the readiness-report ruling and does not re-decide it; a change needs the named workstream's ruling by ADR.
- Stage deferral ADRs (each carries an assumption-debt record + a MUST-data default / interim rule in force until its OQ closes): ADR-0212 (Stage 0–1), ADR-0213 (Stage 2–3, incl. OQ-256 routed here), ADR-0214 (Stage 4–6), ADR-0215 (program-level / promotion test OQ-440), ADR-0211 (D-1…D-6 entry + OQ-461/462), ADR-0216 (OQ-465/467/468). A ticket at a gated stage applies the interim rule; it never guesses past a MUST-data placeholder.
- Evolution deferrals D-1…D-6 (ADR-0207/0211; §11.3) sit OUTSIDE the build ladder and enter only by an ADR passing the entry test: D-1 human directory/rotations, D-2 multi-tenant fleets, D-3 value-at-risk pricing of irreversibility, D-4 dispatch prioritization by expected value, D-5 reconciler-side source mirroring, D-6 cross-item shared-state coordination. Interim proxies are in force; no Stage 0–6 ticket builds them.
- R-2.12.3 (language/ecosystem selection) is ratified by ADR-0050 (polyglot split, late-bound surface layer); GATE-G1 evaluates the Stage-0 revalidation trigger. R-2.12.4 (packaging/licensing/OSS governance/docs/community) is the one `deferred(ADR-0210)` scope row — WS-L6, out of build scope; ADR-0210 holds its interim rule + entry tests. R-2.12.5 (novelty/naming thesis, §1) is program-level, no code.

## Requirement-ID → ticket index
Authoritative for coverage. Each base R-id maps to the ticket(s) that land its slices across stages.

| requirement id | owning ticket(s) |
|---|---|
| `R-2.1.1` | S1.1 |
| `R-2.1.2` | S0.2, S1.4, S3.2 |
| `R-2.1.3` | S1.10, S3.2, S4.5b, R2.18 |
| `R-2.1.4` | S1.9, S3.2, S5.1, R2.18 |
| `R-2.1.5` | S0.2, S1.3, S3.11a, S4.14b |
| `R-2.1.6` | S0.2, S1.6, S4.6, S6.1a |
| `R-2.2.1` | S1.5, S2.9, S3.6, S4.13, R2.2 |
| `R-2.2.2` | S1.7, S2.3, S3.6, S4.13, R2.3 |
| `R-2.2.3` | S1.7, S2.3, S3.6, S4.13, S5.8, R2.3 |
| `R-2.2.4` | S2.9, S3.6, S4.13, R2.2 |
| `R-2.2.5` | S0.2, S0.3, S1.16, S2.1, S4.13, S5.8, R2.4 |
| `R-2.3.1` | S0.2, S1.18, S3.7, R2.7 |
| `R-2.3.2` | S0.2, S3.7, S4.16a, S5.1, R2.7 |
| `R-2.3.3` | S1.18, S5.1, R2.7 |
| `R-2.3.4` | S1.18, S2.10, S2.12, S3.7, S4.16a, S5.1, R2.7 |
| `R-2.4.1` | S0.2, S1.19, S2.8, S3.8, S5.2, S6.2, R2.5 |
| `R-2.4.2` | S2.8, S4.16b, S5.2, S6.2, R2.5 |
| `R-2.4.3` | S1.19, S2.8, S4.16b, S5.2, R2.5 |
| `R-2.4.4` | S1.19, S2.8, S4.16b, S6.2, R2.5 |
| `R-2.4.5` | S2.8, S3.8, S4.16b, S5.2, R2.5 |
| `R-2.5.1` | S1.17, S2.11, R2.8 |
| `R-2.5.2` | S1.17, S3.9, S5.2, R2.8 |
| `R-2.5.3` | S1.17, S2.10, S3.9, R2.8 |
| `R-2.5.4` | S3.9, S4.5b, R2.8 |
| `R-2.5.5` | S0.2, S0.3, S1.16, S2.1 |
| `R-2.6.1` | S0.2, S1.20, S2.11, S3.10, S6.1a, R2.6 |
| `R-2.6.2` | S0.2, S1.20, S2.11, S3.10, S4.8, S5.5, R2.6 |
| `R-2.6.3` | S4.6, S5.5 |
| `R-2.6.4` | S4.7, S5.5, S6.2 |
| `R-2.6.5` | S4.8, S5.5, S6.2 |
| `R-2.7.1` | S1.21, S2.11, S3.10, S4.16c, R2.15 |
| `R-2.7.2a` | S1.21, S3.10, R2.15 |
| `R-2.7.2b` | S4.16c, R2.15 |
| `R-2.7.3` | S1.21, S3.10, S4.16c, S6.3b, R2.15 |
| `R-2.8.1` | S1.11, S2.6, S3.11a, S4.14b, R2.11 |
| `R-2.8.2` | S2.7, S3.11a, S4.14b, R2.9, R2.12 |
| `R-2.8.3` | S0.2, S1.13, S2.4, S3.11b, S4.14b, R2.9, R2.10 |
| `R-2.8.4` | S1.12, S2.4, S3.11b, S4.14b, R2.9 |
| `R-2.8.5` | S1.23, S3.11b, S4.14a, S6.3a, R2.11, R2.20 |
| `R-2.8.6` | S1.15, S2.5, S3.11b, S4.14b, R2.13 |
| `R-2.8.7` | S1.23, S2.6, S3.11b, S4.14a, R2.11 |
| `R-2.9.1` | S0.2, S1.14, S4.15, R2.14 |
| `R-2.9.2` | S1.22, S3.3, S4.15, R2.16 |
| `R-2.9.3` | S3.1, S4.2, S5.4 |
| `R-2.9.4` | S1.24, S3.3, S4.15, S6.1a, R2.17 |
| `R-2.9.5` | S6.1a, S6.2, S6.3a, S6.4 |
| `R-2.9.6` | S1.24, S3.12, S5.4, S6.1b, S6.4 |
| `R-2.9.7` | S5.4, S6.3b |
| `R-2.9.8` | S1.24, S5.4, S6.4, R2.17 |
| `R-2.10.1` | S3.5, S4.5a, S5.3 |
| `R-2.10.2` | S1.8, S2.12, S3.4a, S4.1, S5.3, R2.19 |
| `R-2.10.3` | S1.24, S3.4a, S4.2, S4.5a, S5.3, S6.1a, S6.2, R2.17 |
| `R-2.10.4` | S1.24, S3.4c, S4.3, S5.3, S6.3b |
| `R-2.10.5` | S1.24, S3.4b, S4.3, S4.4, S5.3 |
| `R-2.10.6` | S3.4d, S4.5a, S5.3 |
| `R-2.11.1` | S0.2, S1.26, S2.12, S3.1, S4.5a, S4.12, R2.1 |
| `R-2.11.2` | S4.10, S5.7, S6.1a |
| `R-2.11.3` | S3.1, S4.11, S5.7, R2.19 |
| `R-2.11.4` | S0.1, S0.2, S1.25, S3.1, S4.12, S5.8, R2.4, R2.19 |
| `R-2.12.1` | S1.2, S2.12, S3.12, S4.15, S5.4, S6.3b, R2.21 |
| `R-2.12.2` | S1.27, S2.2, S3.12, S4.14a, S6.1a, R2.20, R2.21 |
| `R-2.12.3` | S0.1, S0.3 — program/toolchain slice; language/ecosystem *selection* ratified by ADR-0050, revalidated at GATE-G1 · R2.21 (Round-2 verification leg) |
| `R-2.12.6` | S4.9, S5.6, S6.4, R2.21 |
| `R-2.12.4` | deferred(ADR-0210) — out of build scope (WS-L6) |
| `R-2.12.5` | program-level (novelty/naming thesis, §1); no code |

## Spec amendments applied
(none — the spec is ratified v1.0-rc2, gate-passed PASS 7/7; amend via the protocol above.)

- 2026-10-06 · §7.1 (`run` verb row §2.2, `Exit classes` row §2.5, `ResultRecord` row §3; R-2.11.1) · before: `result.exit_class` authoritative, no detach member declared (`detached:"parked"` a build interim) · after: `ResultRecord` declares `detached?: "parked"` — a parked detach answers `ok` + `detached:"parked"`, the run stays live/resumable, opens no turn, takes no `ExitClass` row; closed `ExitClass` sum unchanged · approver: operator tick (SPEC_RECONCILIATION_PLAN A-1) · ADR-0331 D2
- 2026-10-06 · §7.1 `[[DEFERRED ADR-0214: OQ-388]]` marker + AC-R-2.11.2-13, §5a AC-R-2.2.1-16 (R-2.2.1/R-2.11.2) · before: bound's shape stated without a value · after: dated S4.13 measurement note — `by_event_id`/`by_class`/`ir_index` incremental at append, every other projection a shared-fold rebuild, 100-of-10⁵ filtered `read` ≈15 ms (`ac_r_2_2_1_16_latency_1e5_fixture`, CI-enforced); bound value + policy ratification remains WS-B1/K2's · approver: operator tick (A-2) · ADR-0331 D4 (measurement ADR-0305 D1)
- 2026-10-06 · spec Appendix B header prose · before: appendix indexes only the pre-build ratified set ADR-0001…ADR-0216 · after: appended paragraph indexing the build-phase set ADR-0217–ADR-0330 (`docs/adr/` + generated `docs/adr/README.md`) by reference, Sections column n/a · approver: operator tick (A-3) · ADR-0331 D5

## Decomposition decisions
- **Grain:** one ticket per scope-item **slice at one stage** (subagent dispatch ceiling — each ticket
  must fit one non-compacting fresh context). Tiny co-located schema slices are merged into the owning
  subsystem's stage ticket (e.g. the C0 model-plane schema slices `R-2.3.3⁰`/`R-2.3.4⁰` ride with the
  gateway ticket S1.18; the five Lab/measurement schema slices are one ticket S1.24). Over-factoring is
  avoided by keeping a shared implicit decision inside one ticket; under-factoring by never bundling two
  subsystems that share no decision.
- **Spine:** the §9 build ladder is the dependency spine, core-first C0 → C4; the build is **stage-major**
  because a subsystem's later-stage slice depends on other subsystems' earlier-stage slices (the ladder is
  stage-major for exactly this reason). Within a stage, cuts fall on subsystem seams.
- **Partition boundaries:** the §4.4 tier DAG (both tables) and Appendix A coverage matrix; ADR-0050 §8
  governs language use (CC4). The 24 open contract-level minors (READINESS §10) and the deferral ADRs
  0210–0216 are inherited, not re-decided.
- **Gates:** GATE-G1 (Stage-0 spike / ecosystem-decision revalidation, §10.6), GATE-G2 (Stage-3
  evaluation-first — first claim, §10.7), GATE-G3 (Stage-6 evolution preconditions, §9.7), GATE-ACCEPT
  (operator signs deviations). No skeleton tickets: the spec is fully written, so every ticket body is an
  executable-grade citation now (gates are acceptance pauses, not design-unknown points).
- **Revisability:** this plan is a starting point, not a contract in stone. `orchestrate-build` may split
  an overflowing ticket (Stage-1 kernel tickets and the Stage-3 Lab kernel are the likeliest split
  candidates) or merge trivial ones at run time and write the change back to `## Plan extensions`.
- **Phase-4 adversarial review record (fresh-context critique, 2026-09-11).** A fresh subagent re-read the
  §4.4 DAG tables, the §9 ladder and the plan and hunted for fragmented decisions, overflow, orphan seams,
  coverage/scope, ordering and over-factoring. **Clean:** single-schema-ownership (identity S1.2, provenance
  S1.3, budgets/MatchSpec S1.6, plugin S1.27, measurement/Lab S1.24, `StopReason` S1.20, `HostedEvent`
  schema S3.4d vs service S4.5a, `hh-embed/1` codegen S0.1 vs groups S1.25) — no schema introduced twice;
  ordering — every Depends-on backward, no stage slice before a slice it consumes (registry store S1.8 kept
  early because R-2.1.4 consumes R-2.10.2⁰); load-bearing cross-subsystem seams are owned tickets, not left
  to the capstone (kernel↔lab S3.1, hosting boundary S4.5a, kernel↔surfaces S5.8, the C4→C2 edge
  R-2.9.5→R-2.10.6 S6.3a); no material over-factoring. **Findings resolved in this seed:** (1) MAJOR
  coverage gap — R-2.8.2's Stage-4 slice / AC-R-2.8.2-6 was unowned → assigned to **S4.14b**; (2–7) MAJOR
  overflow — the whole-§5g/§5h/multi-subsystem bundles were **pre-split** on the seams the reviewer named:
  S3.11→S3.11a/S3.11b (§5g.1–2 / §5g.3–7), S4.5→S4.5a/S4.5b (Hosting ABI / R-2.5.4+ACP), S4.14→S4.14a/S4.14b
  (supply-chain / subagent-hosted security), S4.16→S4.16a/S4.16b/S4.16c (router / retrieval-memory /
  verification, with R-2.1.3¹ moved to S4.5b), S6.1→S6.1a/S6.1b (pipeline recipe / debt manager),
  S6.3→S6.3a/S6.3b (code-search / attribution-judge-integrity); (11) index blemishes — the R-2.12.3 double
  row was collapsed and the R-2.10.5⁰ Stage-1 row-key fields were folded into **S1.24**. **Consciously
  retained:** (8) S3.2 compiler-full kept as one ticket (borderline §3 ~96KB) with the §3.2/§3.3 split seam
  recorded in its Notes for run-time split if it overflows; (9) S4.6 keeps the C1 `spawn` slice with the C3
  orchestrator (the orchestrator is the slice's only consumer) with the `removability(1)` lint boundary
  noted. Verdict: sound to build from.

- 2026-09-25 · **inserted** `059a_S3.12b__gate-g2-gap-closure.md` (id `S3.12b`, row `59a`) between S3.12 and GATE-G2. Reason: the operator's GATE-G2 Reading-1 disposition (PENDING) authorized a gap-closure ticket to close the machine-achievable cells — the hermetic `benchmarkSet` + deferred real-suite legs (AC-I2-6, AC-I4-3/-11), the exemplar→`ComparisonReport` wiring, the hosting-absent native-suite run, and the Stage-3-scoped DEFERRALS sweep. Foreign-toolchain rows and the R2 human signature are explicitly NOT pulled in. GATE-G2 stays PENDING until S3.12b lands and the operator re-reads the evidence.

### Round 2 — 2026-10-06 (decompose-spec mode=extend)

- **Grain:** one ticket per owed-work *package* on subsystem seams — the round discharges carried
  backlog, not a build ladder. Each ticket must fit one non-compacting fresh-context subagent run;
  the two largest (R2.9 egress/containment, R2.15 verification) carry pre-recorded split seams
  (`R2.9a/b`, `R2.15a/b`) for run-time splitting via `## Plan extensions`.
- **BL-05 is dissolved, not scheduled:** its gate-accepted bundle members distribute to their owning
  subsystems (canary/env-image → R2.10; audit emitters → R2.13; exposure battery → R2.8;
  verification cells → R2.15; eval ops → R2.16; measurement → R2.17; exporter seam → R2.14; the
  live-corpus/foreign legs → R2.21). No shared-decision ticket needed — each member's home subsystem
  owns it.
- **Shared-decision rulings made here:** (1) the durable steer-cue transport (OQ-316 admission arm +
  `queue_next_turn` durability) has ONE owner — **R2.6**; DF-S2.8-1(a)'s steer leg is reassigned
  there, R2.5 keeps `mark_scope_ended` sequencing + the emitters. (2) The remedy-ingress/approval-
  consumption surface is owned by **R2.11**; R2.12 consumes it. (3) The `postconditions` binding is
  R2.8's (tool surface); the reconciliation *consumer* is R2.15's. (4) DF-S1.18-1 splits along its
  own seam: offline-decidable machine cells → **R2.7**; live transports/adapters stay BL-31.
- **Environment-gated posture:** R2.21 is a real ticket (its offline packaging + E1 self-check are
  landable hermetically) with Live stage `operator-gated` on HUMAN-H3 — if the environment is absent
  it lands the packaging and re-records the DF rows 'environment pending'; nothing fabricates a
  foreign pass. `tls.terminate` (R2.9) enforces-or-refuses without a TLS stack; `dpop` (R2.10) and
  oauth/mTLS (R2.19) take the same enforce-or-refuse posture.
- **Not scheduled this round (deferral-backed, recorded):** BL-31's live cells (real provider
  transports, DF-S2.4-2 TLS transport — needs endpoints + a TLS stack decision); BL-32 (remote fetch
  + foreign vocabularies DF-S4.2-1/-2 — WS-L6 trust fields); BL-33/BL-34/BL-38 (the WS-L6
  program-resume window — the operator reopens it); BL-30's ruling legs (OQ-170 signer custody,
  OQ-388 ratification, CF-476/CF-487 — named workstreams own, R2.23/R2.24/R2.26 re-validate but never
  re-decide); BL-41 (standing debt — re-issued, never closed); BL-42/BL-43 (ADR revisit triggers —
  they fire on future tickets, not schedulable). HUMAN-H1/H2 persist as chain rows 61/82.
- **Already-DONE rows excluded:** DF-S1.3-2 (vouched mint landed S4.5a — flip verify at R2.1),
  DF-S1.13-4 (S4.14b), DF-S1.26-1 (S2.10), DF-S3.5-1 (S4.12), DF-S4.11-3 (CAP.3), DF-S5.4-1
  (S6.1b), DF-S1.24-3 (REC.2/ADR-0331), DF-S3.9-1, DF-S2.9-2, DF-S2.10-2, DF-S3.12b-1/-2.
- **Ordering:** chain order is the dependency spine; specific backward edges — R2.10 after R2.9
  (mediator seam), R2.11 after R2.9 (ingress plumbing), R2.12 after R2.11 (remedy ingress), R2.13
  after R2.2 (gc machinery), R2.14 after R2.2+R2.9 (`read` tail, mediated sinks), R2.15 after R2.8
  (postconditions binding), R2.21 after HUMAN-H3 + the corpus-owning rows, revalidation rows
  (R2.22–25) after all implementation rows (they re-check landed state), R2.26 after all, GATE-G4
  after closure, R2.27 after the gate.
- **Phase-4 adversarial review record (explicit reread-and-argue pass, 2026-10-06 — no fresh-context
  subagent was invoked; the decomposer re-read the registers and argued the plan down):**
  **Fragmented decisions found + fixed:** the durable steer-cue seam was claimed by both DF-S2.8-1
  and DF-S2.11-1 → single-owner ruling above; the "turn-loop call-site" residual appeared in four
  source rows (DF-S2.8-1, DF-S1.19-1, DF-S1.17-3, DF-S1.20-1) → verified the driver loop already
  exists (`EmbedService::drive` + `AssemblerPort` landed), so each residual is a subsystem-specific
  emission/consumer leg, not a shared new decision — distributed by owning plane with the ownership
  notes above. **Overflow risk:** R2.9 and R2.15 are L-sized; both carry pre-registered split seams
  (Notes fields) so a run-time split does not re-decompose the round. **Orphan seams audited:** the
  custody-gated audit cells (DF-S2.5-1), TLS transport (DF-S2.4-2), and WS-L6 packages are NOT
  orphaned — they are deferral-backed with named external triggers, re-validated by R2.22–R2.26.
  **Coverage check:** every OPEN/PARTIAL DEFERRALS row and every machine-achievable BL row maps to
  exactly one R2 owner or to the not-scheduled list above; the `hh-kernel doctor` defect
  (DF-DOC.1-1, post-dates BACKLOG.csv) and the CI flake found on PR #104's failing run are R2.1's.
  **Double-ownership check:** DF-S1.20-1 and DF-S2.8-1 each split across two tickets with named
  members per side; no cell is claimed by two tickets. **Ordering check:** every Depends-on points
  backward; no cycles; gates never pre-answered (GATE-G4's criterion is a review, not a result).
  **Over-factoring check:** BL-22+BL-23 merged (same hh-monitor decide-path working set);
  BL-35+BL-39 merged (two small register packages); BL-28 kept standalone (distinct seam — merging
  with assembly would bundle unrelated subsystems). **External prerequisites:** HUMAN-H3/H4 are
  markers, not tickets; R2.21's honest withhold posture is written into its ACs.
  **Verdict:** the 30-row partition is dependency-clean and honesty-preserving; residual risk is
  ticket-level overflow on R2.9/R2.15, mitigated by the pre-registered splits.

## Plan extensions
(append-only: inserts `NNa_…`, splits `<ID>a`/`<ID>b` with the original marked superseded-by-split, rounds)
- 2026-09-15 · **inserted** `003a_S0.3b__cross-candidate-online-spike.md` (id `S0.3b`, row `3a`) between S0.3 and GATE-G1. Reason: the operator's GATE-G1 disposition authorized an **online** spike budget to close the machine-achievable cells of DF-S0.3-1 (MCP/ACP official SDK round-trip + cross-candidate G1 byte-identity) and DF-S0.3-3 (E2/E3 cross-candidate scoring + E5b/E5c splits). The R2 human cross-camp reviewer signature and the Stage-3 polyglot CI (DF-S0.3-2) are explicitly NOT pulled into this ticket. GATE-G1 stays PENDING until S0.3b lands and the operator re-reads the extended sheet.
- 2026-10-06 · **round 2 appended** — rows 107–136 (`R2.1`…`R2.27`, `HUMAN-H3`, `HUMAN-H4`,
  `GATE-G4`) by `decompose-spec mode=extend` over the Round-1 closeout. Sources: the 2026-10-06
  decision memo + `docs/build/BACKLOG.csv` (BL-01…BL-43) + the OPEN/PARTIAL DEFERRALS set. Round-1
  rows 1–106 unchanged. See `## Decomposition decisions → Round 2` for the partition and the
  not-scheduled set; `docs/build/planning/2026-10-06_round2-decomposition.md` for the full record.
