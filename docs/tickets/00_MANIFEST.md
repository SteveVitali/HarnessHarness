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

**HUMAN-H5 — Rule the crypto-dependency posture** (Round 3). Blocks: R3.7/R3.8 (skeletons — BL-19
`dpop` PoP verify + `wrapped_long_lived`; BL-45 per-event AEAD + the R-2.8.6 legs). The legs need
an asymmetric verifier / AEAD construction the pure-std workspace lacks; the ruling picks
in-workspace construction, a deps-contract amendment (by ADR), or permanently typed-refused.
Withheld until ruled: any proof-of-possession or encryption claim — the typed refusals stand as
the honest answer. Exit: a recorded disposition; the skeletons fleshed or dispositioned.
**RULED 2026-10-14 (operator): disposition (b) — a vetted external crypto crate
(ring/aws-lc/rustls family) is admitted.** The dep-choice ADR lives inside R3.7's contract
(names the crate + a ≥7-days-published pinned version + the admitted scope); R3.8 consumes or
extends by sibling ADR. R3.7/R3.8 fleshed to real tickets same-day; this marker is the ruling's
record, no longer a pending gate.

**HUMAN-H4 — decline recorded 2026-10-20 (operator): "Record a decline instead"** — supersedes the 14:21Z
"I'll name a reviewer"; R4.15 records the decline verbatim and closes DF-S0.3-3 / BL-02 waived-by-ADR.

**HUMAN-H6 — Live-leg provisioning** (Round 4, row 149b). Blocks: every live leg (R4.5a…L4.6a). Owner: operator; scheduled
before R4.5a (ruling: provisioning "Now, interactively"). NAMES only in `~/.config/harnessharness/live.env` (a symlink to a
gitignored `.env` in the operator's main checkout); records
say `provided: yes/no`. Round-4 override: a live ticket finding a NAME unprovisioned STOPS and asks — no D-row.
**Orchestrator pre-flight (amend (b), 2026-10-20):** once H6 is ticked, before dispatching R4.5a the orchestrator runs one
names-only yes/no check over every NAME the round needs — `scripts/live-verify.sh --check-names --all` if it exists, else
the literal shell check spelled out in `149b_HUMAN-H6__live-provisioning.md` — and any `provided: no` stops and asks.

**HUMAN-H7 — Public launch acts** (Round 4, row 167a). Blocks: R4.18. Trigger: R4.17 after GATE-G6 PASSED. Merge the
stack, protect `main`, tag `v0.1.0` → release, `cargo publish` in order, repo metadata, the DONE flip. Amend (b): protect
first, merge bottom-up via checks-required merges only, wait for the full CI matrix + gitleaks green on `main` HEAD, record
the merged-main SHA, then tag it; any gitleaks finding → rotate first, history-rewrite decision recorded verbatim.

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
| 115 | `115_R2.9__egress-containment-legs.md` | Round 2 | ticket | Egress/containment residual: amend(), T-CON members, tls.terminate enforcement + inspect_hooks, ask-endorsement ingress, fork rebind, leak-scan surfaces (DF-S1.12-*, DF-S2.4-1/-3; BL-18) — **superseded-by-split 2026-10-07 → R2.9a + R2.9b** (## Plan extensions) | — |
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

### Round 3 — 2026-10-14 · in-build residual cluster (decompose-spec mode=extend)

Rows 137–149 append below Round-2 history unchanged. Scope: the GATE-G4 `in-build`/`dep`-coded
carried rows — BL-44, BL-14, BL-15, BL-16, BL-10 plus the LEDGER flake findings and the G4
bookkeeping flags. `dep` rows were seeded as skeletons gated on HUMAN-H5 (the crypto-dependency
ruling) and CF-489's conform as a skeleton gated on the WS-I6/OQ-441 ruling (BL-30);
**amended same-day 2026-10-14 by operator rulings — H5 answered (external crypto crate
admitted; R3.7/R3.8 fleshed to real tickets) and CF-489 directed (conform code to spec;
R3.9 fleshed)** — the three rows now run as ordinary tickets in chain order. Environment (`env`),
`human`, `ruling`, `window`, `standing`, and gate-`accepted` rows are NOT scheduled — they carry
forward on their GATE-G4 dispositions. Sources: `docs/build/readouts/GATE-G4.md` §2/§8/§9,
`docs/build/BACKLOG.csv` (BL-01…BL-46), `docs/tickets/DEFERRALS.md` owed set, LEDGER OPEN
FINDINGS.

| # | file | phase | kind | scope | gate |
|---|---|---|---|---|---|
| 137 | `137_R3.1__round3-hygiene.md` | Round 3 | ticket | Round-3 hygiene: root-cause the oauth timing + `ac5` view_hash + `r2_14` loopback flakes; sweep stale `OPEN` prefixes (G4-4), DF-S2.4-2 dated note (G4-5), BACKLOG.md counts (G4-6) | — |
| 138 | `138_R3.2__durable-execution-residual.md` | Round 3 | ticket | DF-S2.3-1 residual producers: `resume_set`, `defer`, child-lease, non-fleet ingress, `heal(env_handle)` surface (BL-44) | — |
| 139 | `139_R3.3__control-delegation-legs.md` | Round 3 | ticket | DF-S1.20-1 legs: `delegation_reason` real-run refusal path, `woken{peer_message}` interpretation, `workflow`/`program` real-run semantics (BL-14) | — |
| 140 | `140_R3.4__context-memory-residual.md` | Round 3 | ticket | §5c residuals: DF-S1.19-1 producer/wiring members + DF-S1.19-2 battery cells (BL-15) — pre-registered split R3.4a/b in its Notes | — |
| 141 | `141_R3.5__tool-exposure-residual.md` | Round 3 | ticket | DF-S1.17-1 members: `provider_tool_api` flags, `surface_rejection_rate` computation, ADR-0272 `sync_source`/`retrieval_eval` legs (BL-16) | — |
| 142 | `142_R3.6__assembly-emitter-adjudication.md` | Round 3 | ticket | BL-10: emitter-vs-exemption adjudication for the 11 no-producer diagnostic codes (DF-S1.9-2 residual) | — |
| 143 | `143_HUMAN-H5__crypto-dependency-ruling.md` | Round 3 | human | Crypto-dependency posture — RULED 2026-10-14 (operator): disposition (b), external vetted crypto crate admitted; dep-choice ADR owned inside R3.7 | — |
| 144 | `144_R3.7__credential-broker-dep-legs.md` | Round 3 | ticket | BL-19: dep-choice ADR (crate+version pin, deps-contract amendment) + `dpop` verify + `wrapped_long_lived` delivery (DF-S1.13-1/-4) | — |
| 145 | `145_R3.8__audit-event-encryption.md` | Round 3 | ticket | BL-45: per-event blob encryption (AEAD, consumes R3.7's pinned crate or sibling ADR) + R-2.8.6 residual legs (DF-S1.15-3) | — |
| 146 | `146_R3.9__debtpolicy-cf489-conform.md` | Round 3 | ticket | CF-489 conform: `DebtPolicy::default()` → §5h.6 proposed 60d/30d (operator ruling — code conforms to spec; constants + comment + caller/test sweep + dated CF note) | — |
| 147 | `147_R3.10__round3-closure.md` | Round 3 | reconcile | Round-3 closure: backlog refresh, coverage recompute, OPERATIONAL_READINESS, carried-set accounting for GATE-G5 | — |
| 148 | `148_GATE-G5__round3-discharge.md` | Round 3 | gate | Round-3 discharge review — operator dispositions every still-OPEN scoped row + the carried set | STOP |
| 149 | `149_R3.11__round3-docs-refresh.md` | Round 3 | docs | Round-3 docs refresh — repo docs + agent docs honest-status pass | — |

### Round 4 — 2026-10-20 · H1/H2/H3 discharge + WS-L6 window packages (decompose-spec mode=extend)

Rows 150–167 append below Round-3 history unchanged. Scope: the 2026-10-20 operator rulings
(GATE DECISIONS — H1 "Confirm TypeScript", H2 "GitHub Issues + tunnel", H3 "Toolchains +
online corpus", H4 "I'll name a reviewer") + the charter's reopening of the GATE-G4 `window`
set: R4.1 hygiene (ac5 derived-wall class, helper-spawn deadline machinery, the G5-1
warning-count flag), R4.2 the E3=TypeScript bind (H1 discharge), R4.3–R4.5 the H3 toolchain
legs (xcheck extension arms → `foreign/e3-typescript` → the C12/two-toolchain measurement,
incl. the verbatim `foreign/e2-python` import from main), R4.7 the H2 GitHub Issues adapter +
webhook/tunnel legs (live legs `gate pending` on row 082), R4.8–R4.14 the WS-L6 window
packages (BL-33–BL-39 — spec/OQ closure engineering: dated cells + proposed dispositions +
briefs; rulings are the operator's), R4.15 the BL-30 adjudication prep, tail R4.16 closure +
GATE-G6 + R4.17 docs refresh (+ adopted-ruling conform). Skeletons R4.6 (H3 online legs —
gated on account/key provisioning) and R4.9 (BL-32 — gated on the BL-33 trust rulings +
remote env). Environment rows BL-17/BL-27/BL-31 are NOT scheduled (live provider/model spend
is not covered by the corpus authorization — flagged for a future ruling); BL-02's H4 marker
carries forward; BL-05/BL-41/BL-42/BL-43 standing accepted. Sources: `docs/build/readouts/
GATE-G5.md`, `docs/build/BACKLOG.csv` (BL-01…BL-46), the owed DEFERRALS set (19 rows mapped
in the planning record), `docs/build/LEDGER.md`, the 2026-10-20 rulings.

**Amended 2026-10-20 (launch re-charter — `decompose-spec mode=extend` amendment, precedent `75d61ab`):** the
operator re-chartered round 4 as the **launch round** — "so that we can be certain that by the end of this next
orchestrate-build drive we have a fully ready public open-source GitHub repo launch, with no remaining deferred work and
deep end to end live verification and so on" — plus 15+ structured rulings (LEDGER GATE DECISIONS `### Round 4`). The
unexecuted rows 150–167 were rewritten in place (dated `> Amended` banners; no file renamed — BM-COMPAT-05) and 31 suffix
rows inserted: R4.0 main→chain sync, HUMAN-H6 provisioning, R4.4a (pre-split), the live track R4.5a–e, R4.7a–f, the
pre-split briefs R4.12a/R4.13a, the rulings gate GATE-G7 ("GATE-R4R"), the post-gate conform skeletons R4.15a–e, the
launch track L4.1–L4.7, HUMAN-H7 and R4.18. **Round objective (GATE-G6):** zero OPEN DEFERRALS rows, zero open BL rows,
every coverage row MET (or MET-DIFFERENTLY only as a permanent design decision with its ADR), `live-executed` evidence on
every live leg within the $100 cap, OSS launch hardening green, the ten GATE-ACCEPT deviations discharged or confirmed
permanent — then HUMAN-H7 and the launch record. The earlier paragraph above is the seed's scope, kept as history.

| # | file | phase | kind | scope | gate |
|---|---|---|---|---|---|
| 149a | `149a_R4.0__main-chain-sync.md` | Round 4 | ticket | main → chain sync: merge origin/main (PR #136 e2 tree + corrected CHECKER.md + main-only fixes) into the tip; the one `in_process.rs` conflict → main's derivation-aware ac5 rule; regenerate; suite green | — |
| 149b | `149b_HUMAN-H6__live-provisioning.md` | Round 4 | human | Live-leg provisioning by NAME (provider keys, local model, sandbox token/secret, OAuth App client id, cloudflared, collector, `live-verify` Environment) — `provided: yes/no` only | — |
| 150 | `150_R4.1__round4-hygiene.md` | Round 4 | ticket | Hygiene: context-economy tools (`bm-row.sh`, `deferrals-open.sh`, `spec-section.sh` + self-test) + tool-first read rule; ac5 class post-merge; helper-spawn machinery; G5-1 count | — |
| 151 | `151_R4.2__e3-typescript-surface-bind.md` | Round 4 | ticket | H1 discharge: E3=TypeScript ADR-0050 row + generated TS client under drift + live E3 node smoke vs `hh-kernel serve --http` (DF-S4.10-1; BL-03; R-2.11.2) | — |
| 151a | `151a_R4.2a__ci-fast-path-and-full-run-cadence.md` | Round 4 | ticket | CI cost/wait insert (phase 1): one `pull_request` run per head (branch `push` runs dropped), fail-closed docs-only fast path with repo-wide artifact anchors, S2 spike on `workflow_dispatch` full runs only, full run per gate + `full-run.sh` reader, step-mode lint; ADR (operator rows `CI insert` D1–D7) | — |
| 152 | `152_R4.3__xcheck-bundle-extension-arms.md` | Round 4 | ticket | `hh-xcheck-bundle/1` HIR/ledger extension arms + checker + regenerated bundle (DF-S1.2-2/-S1.5-3 prerequisite legs; BL-01) | — |
| 153 | `153_R4.4__e3-typescript-foreign-impl.md` | Round 4 | ticket | `foreign/e3-typescript` part 1 — core + identity/ledger/registry/canonical-parse arms + verify (DF-S1.5-3, DF-S1.8-1 E3 legs) | — |
| 153a | `153a_R4.4a__e3-typescript-foreign-impl-part2.md` | Round 4 | ticket | `foreign/e3-typescript` part 2 — plugin-manifest + ledger-scenarios arms + verify-pending closeout (DF-S1.2-2, DF-S1.27-1 progress notes) — **run-time split 2026-10-21 (## Plan extensions): `hir-documents` + all-arms closeout → row 153b/R4.4b; this row rescoped by appended dated note** | — |
| 153b | `153b_R4.4b__e3-typescript-foreign-impl-part3.md` | Round 4 | ticket | `foreign/e3-typescript` part 3 — `hir-documents` arm (`lib/hir.ts`) + full-bundle verify closeout (DF-S1.2-2, DF-S1.27-1 DONE flips) | — |
| 154 | `154_R4.5__c12-polyglot-ci-measurement.md` | Round 4 | ticket | Two-toolchain CI leg + C12 measurement + ADR-0050 trigger-5 measured row (DF-S0.3-2; BL-01; GATE-ACCEPT 2/8/9) | — |
| 154a | `154a_R4.5a__live-evidence-framework.md` | Round 4 | ticket | Live-evidence framework: `live-executed` class ADR, evidence records (+ provider request ids), $100 spend ledger + allocation, `live-verify.sh` (+ `--check-names`), gating convention, gitleaks staged-diff gate + full-history baseline (`.gitleaks.toml`, 7 fixture hits allowlisted); local proof leg (live, USD 0) | — |
| 154b | `154b_R4.5b__gateway-model-port-local-live.md` | Round 4 | ticket | Gateway-backed `ModelPort` on the run path + one HTTP codec home + HTTP transport; first live run on local Ollama (DF-S1.18-1 member; live, USD 0) | — |
| 154c | `154c_R4.5c__rustls-tls-transport.md` | Round 4 | ticket | rustls-over-ring admission ADR + HTTPS/mTLS + `tls.terminate` honoured + LT-02/05 over TLS (DF-S2.4-2; R-2.8.3/2.8.4; live, USD 0) | — |
| 154d | `154d_R4.5d__provider-transports-anthropic-openai.md` | Round 4 | ticket | Anthropic + OpenAI dialects + live broker `CredentialPort` + AC-R-2.3.1-11 live OOP variant (DF-S1.18-1 members; live, USD 5) | — |
| 154e | `154e_R4.5e__probe-profile-live-routing-arms.md` | Round 4 | ticket | `probe_profile` against real providers + live-signal routing arms — closes DF-S1.18-1 (live, USD 5) | — |
| 155 | `155_R4.6__h3-online-live-legs.md` | Round 4 | ticket | H3 online legs: LT-03 live-corpus injection arm + live foreign-manifest import (DF-S1.24-2; BL-05/BL-26; live, USD 15) | — |
| 156 | `156_R4.7__github-issues-tracker-webhook.md` | Round 4 | ticket | GitHub Issues adapter + signed webhook via cloudflared quick tunnel — live legs on the private sandbox repo (DF-S5.6-1; BL-04; R-2.12.6; live, USD 0) | — |
| 156a | `156a_R4.7a__oauth-mtls-credential-mediator.md` | Round 4 | ticket | OAuth (GitHub OAuth App device flow — operator present) + mTLS mediator (DF-S4.11-2; BL-27; live, USD 0) | — |
| 156b | `156b_R4.7b__provider-context-adapters.md` | Round 4 | ticket | Provider-backed Embedder/Summarizer/ProviderCompaction adapters (BL-31; ADR-0253; live, USD 5) | — |
| 156c | `156c_R4.7c__observability-live-sink.md` | Round 4 | ticket | Live observability sink to a local collector (BL-31; ADR-0346 trigger a; live, USD 0) | — |
| 156d | `156d_R4.7d__verification-reference-corpus-acs.md` | Round 4 | ticket | Verification reference-corpus AC fixture halves (DF-S1.21-2 hermetic remainder; BL-05) | — |
| 156e | `156e_R4.7e__live-judge-binding.md` | Round 4 | ticket | Live judge binding at `link` + calibration + independence (DF-S1.21-3, DF-S1.21-2; BL-17; R-2.7.2b; live, USD 15) | — |
| 156f | `156f_R4.7f__audit-link-obligations.md` | Round 4 | ticket | Audit `subagent_anchor` + `producer_resolution` obligations graduate (DF-S1.15-1 link members) | — |
| 157 | `157_R4.8__bl33-ws-l6-package.md` | Round 4 | ticket | BL-33 brief: record the ruled launch postures (license, naming, distribution, provenance) + naming market check over the explicitly enumerated publish set (dated per-name); final proposals for the rest + trust-field section | — |
| 158 | `158_R4.9__bl32-remote-fetch-foreign-vocab.md` | Round 4 | ticket | BL-32 hermetic half: four foreign import/export vocabularies (DF-S4.2-2); remote fetch moved to R4.15d | — |
| 159 | `159_R4.10__bl34-adr-0211-package.md` | Round 4 | ticket | BL-34 brief: D-1..D-6 entry tests + OQ-461/462/463 — final proposals | — |
| 160 | `160_R4.11__bl35-bl39-adr-0212-0216-packages.md` | Round 4 | ticket | BL-35+BL-39 brief: ADR-0212 (12 OQs) + ADR-0216 (3 OQs) + CF-488 note — final proposals | — |
| 161 | `161_R4.12__bl36-adr-0213-package.md` | Round 4 | ticket | BL-36 brief part 1: ADR-0213 OQ-030…OQ-273 (OQ-170 cell → R4.15) — final proposals | — |
| 161a | `161a_R4.12a__bl36-adr-0213-package-part2.md` | Round 4 | ticket | BL-36 brief part 2: ADR-0213 OQ-307…OQ-407 — final proposals | — |
| 162 | `162_R4.13__bl37-adr-0214-package.md` | Round 4 | ticket | BL-37 brief part 1: ADR-0214 OQ-116…OQ-417 (OQ-388 cell → R4.15) — final proposals | — |
| 162a | `162a_R4.13a__bl37-adr-0214-package-part2.md` | Round 4 | ticket | BL-37 brief part 2: ADR-0214 OQ-418…OQ-458 incl. the OQ-441 re-pin — final proposals | — |
| 163 | `163_R4.14__bl38-adr-0215-package.md` | Round 4 | ticket | BL-38 brief: ADR-0215 (15 OQs incl. OQ-440, HIR/2 backlog) + the 24 READINESS_REPORT §10 minors — final proposals | — |
| 164 | `164_R4.15__bl30-adjudication-prep.md` | Round 4 | ticket | BL-30 brief (7 CFs + concrete OQ-170 custody + OQ-388 ratification) + H4 decline → DF-S0.3-3 waived-by-ADR + CF-490 fix | — |
| 164a | `164a_GATE-G7__round4-rulings.md` | Round 4 | gate | Round-4 rulings gate ("GATE-R4R"): operator adopts/amends/declines every brief proposal; scoped extend fleshes R4.15a–e | STOP |
| 164b | `164b_R4.15a__conform-records-packages-a.md` | Round 4 | ticket | Adopted-ruling records conform part 1 — BL-33 (WS-L6 records) + BL-34 (ADR-0211 descope ADR) — fleshed at GATE-G7 | — |
| 164c | `164c_R4.15a2__conform-records-packages-a2.md` | Round 4 | ticket | Adopted-ruling records conform part 2 — BL-35/39 (ADR-0212/0216) + BL-36 (ADR-0213) — GATE-G7 insert | — |
| 164e | `164e_R4.15b__conform-records-packages-b.md` | Round 4 | ticket | Adopted-ruling records conform part 3 — BL-37 (ADR-0214) + BL-30 records + OQ-388 ratification (DF-S4.13-1) — fleshed at GATE-G7 | — |
| 164f | `164f_R4.15b2__conform-records-packages-b2.md` | Round 4 | ticket | Adopted-ruling records conform part 4 — BL-38 (ADR-0215 + §10 minors) + the adopted spec-text legs — GATE-G7 insert | — |
| 164h | `164h_R4.15c__conform-code-adopted-legs.md` | Round 4 | ticket | Adopted-ruling code conform — OQ-468, OQ-430, CF-492, CF-493, OQ-173 legal_hold — fleshed at GATE-G7 | — |
| 164k | `164k_R4.15d__remote-fetch-trust-fields.md` | Round 4 | ticket | Remote fetch transports with the adopted trust fields (DF-S4.2-1, OQ-332) — fleshed at GATE-G7 | — |
| 164m | `164m_R4.15e__signer-key-custody.md` | Round 4 | ticket | Signer-key custody per the adopted OQ-170 ruling (DF-S2.5-1, DF-S1.15-1 custody; R-2.8.6) — fleshed at GATE-G7 | — |
| 164p | `164p_L4.1__oss-repo-files-licensing.md` | Round 4 | ticket | OSS files: LICENSE-MIT/APACHE + per-crate license + notices + SECURITY/CONTRIBUTING/CoC/CHANGELOG + templates + CODEOWNERS + living-doc scrub (R-2.12.4) | — |
| 164q | `164q_L4.2__maintainer-policy.md` | Round 4 | ticket | MAINTAINING/GOVERNANCE + generated revisit-trigger catalogue; BL-41/42/43 closed converted-to-policy | — |
| 164r | `164r_L4.3__ci-supply-chain-hardening.md` | Round 4 | ticket | CI: ubuntu+macOS matrix, cargo-deny, gitleaks (reusing R4.5a's `.gitleaks.toml`), `cargo doc -D warnings`, dependabot, protected dispatch-only `live-verify` workflow | — |
| 164s | `164s_L4.4__crate-rename-hharness.md` | Round 4 | ticket | Scripted crate rename `hh-*` → `hharness-*` (+ umbrella) behind a first-step crates.io availability check (stop-and-ask on collision), binaries unchanged; lib-name + directory decision ADR | — |
| 164t | `164t_L4.5__publish-readiness-release-workflow.md` | Round 4 | ticket | Publish readiness: metadata (deliberate `rust-version`, CI-verified), `hharness` facade, publish=false set, dry-run order script (name collision = stop-and-ask), docs.rs readiness, v0.1.0, release workflow (ADR target set — macOS arm64/x86_64, linux x86_64/aarch64; Windows excluded — + checksums) | — |
| 164u | `164u_L4.6__live-e2e-capstone.md` | Round 4 | ticket | Live E2E capstone: reference tasks via `hh` over (a)(b)(c) × local/Anthropic/OpenAI, sandboxed exec, judged, verify+replay, hh-web, MCP (`hh-mcp`), fleet webhook, bundle reproduce (live, USD 30); surface/plugin sweep split to L4.6a | — |
| 164v | `164v_L4.7__clean-clone-quickstart.md` | Round 4 | ticket | Clean-clone quickstart: README quickstart executed verbatim as a CI test (+ local-model live block, USD 0) | — |
| 164w | `164w_L4.6a__live-e2e-surface-plugin-sweep.md` | Round 4 | ticket | Live E2E capstone part 2 — surface & plugin sweep: `hh-acp` via the official ACP SDK client (in-tree client fallback, stated), `hh-mcp-lab` Lab ops over MCP, SDK/embed library-API consumer (binding (a)), packaged plugins out of process (live, USD 0) | — |
| 165 | `165_R4.16__round4-closure.md` | Round 4 | reconcile | Round-4 closure: zero-deferral accounting, BL flips, coverage recompute, readiness (every public surface/binding/plugin `live-executed` or operator `n/a{reason}`), origin/main containment, DF-S1.15-1 full enumeration, spend + GATE-ACCEPT discharge table | — |
| 166 | `166_GATE-G6__round4-discharge.md` | Round 4 | gate | Round-4 discharge + launch-readiness review — the charter's zero-deferral / live / launch thresholds | STOP |
| 167 | `167_R4.17__round4-docs-refresh.md` | Round 4 | docs | Public-facing docs final — README/docs/AGENTS refresh for launch (no conform leg) | — |
| 167a | `167a_HUMAN-H7__launch.md` | Round 4 | human | Operator launch acts: protect main, checks-required bottom-up merges, main CI + gitleaks green → record merged-main SHA, tag v0.1.0 → release, `cargo publish` in order, repo metadata, DONE flip | — |
| 167b | `167b_R4.18__launch-record.md` | Round 4 | reconcile | Launch record: merged-main SHA = tag commit, release URL + published versions verified; OPERATIONAL_READINESS → public | — |

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

**GATE-G5 — Round-3 discharge review** (Round 3). Criterion: every DEFERRALS row and backlog
source the Round-3 tickets were chartered to discharge is DONE with dated evidence, or carried
forward with an explicit operator disposition (carry to Round 4 / accept / waive by ADR). The
carried set — HUMAN-H5's record (ruled 2026-10-14, disposition (b)) and whatever residual the
un-gated R3.7/R3.8/R3.9 tickets report — plus the still-open human/env rows are dispositioned as
a named set: 'gate pending' is a valid recorded state, a silent drop is not.
Thresholds: zero undispositioned R3-chartered OPEN rows; the three flake families repaired or
adjudicated; the G4-4/5/6 bookkeeping swept; check-build-memory 0 violations; check-backlog
exit 0; R3.10's carried-set accounting exists; the 10 GATE-ACCEPT deviations + GATE-G4
dispositions re-confirmed unchanged.

**GATE-G6 — Round-4 discharge review** (Round 4). Criterion: every DEFERRALS row and backlog
source the Round-4 tickets were chartered to discharge is DONE with dated evidence, or
carried forward with an explicit operator disposition (carry / accept / waive by ADR). The
named set — the H3-authorized online legs (R4.6's items), the env-pending rows
(BL-17/BL-26 legs/BL-27/BL-31/BL-32), the human rows (H1/H2/H3 exit states + H4's pending
reviewer), and the standing set (BL-05/BL-41/BL-42/BL-43) — is dispositioned as a named
set: 'gate pending' is a valid recorded state, a silent drop is not. Additionally the gate
dispositions every proposed ruling assembled in the round's brief files
(`docs/build/planning/round4-*-briefs.md`): adopt / amend / decline per row; un-dispositioned
proposals carry forward as a named set, never silently adopted.
Thresholds: zero undispositioned R4-chartered OPEN rows; the G5-7/G5-1 hygiene flags swept;
the C12 cells + the H1/H2 landable legs landed or honestly dated; check-build-memory 0
violations; check-backlog exit 0; `^||` empty; R4.16's accounting + the proposed-rulings
inventory exist; the 10 GATE-ACCEPT deviations + GATE-G4/G5 dispositions re-confirmed
unchanged; the H3 pre-authorization row dispositioned.

> Amended 2026-10-20 (launch re-charter) — the criterion above is the seed's; the binding criterion is now: the
> operator's charter verbatim ("…a fully ready public open-source GitHub repo launch, with no remaining deferred work
> and deep end to end live verification…"). Thresholds: zero OPEN DEFERRALS rows (DONE with dated evidence or closed
> waived/descoped by an ADR carrying the operator's ruling); zero open BL rows; every coverage row MET or MET-DIFFERENTLY
> only as a permanent design decision with its ADR; `live-executed` evidence on every scheduled live leg, spend ≤ USD 100;
> CI matrix + cargo-deny + gitleaks + quickstart + publish dry-run + release dry-mode green; the ten GATE-ACCEPT deviations
> discharged or confirmed permanent; validators clean. Ruling conforms are no longer carried here (GATE-G7). Full text:
> `166_GATE-G6__round4-discharge.md`.
>
> Amended 2026-10-20 (b) (before the gated work): added thresholds — every public surface/binding/plugin `live-executed`
> with a cited record or operator-recorded `n/a{reason}`; the chain tip contains origin/main (`git merge-base
> --is-ancestor origin/main HEAD`); DF-S1.15-1 full-membership enumeration marked; readout spot-audit + console-billing
> reconciliation.

**GATE-G7 — Round-4 rulings gate ("GATE-R4R")** (Round 4, row 164a). Criterion (operator, verbatim): "Mid-round rulings
gate". Thresholds: every brief file present with its batch-adoptable table; every in-scope OQ/CF/D-n/§10 minor carries one
FINAL proposed disposition (resolve-with-evidence · adopt-interim-as-final · descope-by-ADR · needs-code-leg) — zero
keep-open proposals; OQ-170 custody proposal concrete; OQ-388 ratification text drafted; R4.8's trust-field section exists.
The operator's adopt/amend/decline rows drive a scoped `mode=extend` that fleshes R4.15a–e. Never pre-answered.

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
| `R-2.1.3` | S1.10, S3.2, S4.5b, R2.18, R3.6 |
| `R-2.1.4` | S1.9, S3.2, S5.1, R2.18, R3.6 |
| `R-2.1.5` | S0.2, S1.3, S3.11a, S4.14b |
| `R-2.1.6` | S0.2, S1.6, S4.6, S6.1a |
| `R-2.2.1` | S1.5, S2.9, S3.6, S4.13, R2.2, R4.15b |
| `R-2.2.2` | S1.7, S2.3, S3.6, S4.13, R2.3 |
| `R-2.2.3` | S1.7, S2.3, S3.6, S4.13, S5.8, R2.3, R3.2, R3.3 |
| `R-2.2.4` | S2.9, S3.6, S4.13, R2.2 |
| `R-2.2.5` | S0.2, S0.3, S1.16, S2.1, S4.13, S5.8, R2.4 |
| `R-2.3.1` | S0.2, S1.18, S3.7, R2.7, R4.5b, R4.5d |
| `R-2.3.2` | S0.2, S3.7, S4.16a, S5.1, R2.7, R4.5b, R4.5e |
| `R-2.3.3` | S1.18, S5.1, R2.7, R4.5d, R4.5e |
| `R-2.3.4` | S1.18, S2.10, S2.12, S3.7, S4.16a, S5.1, R2.7 |
| `R-2.4.1` | S0.2, S1.19, S2.8, S3.8, S5.2, S6.2, R2.5, R3.4 |
| `R-2.4.2` | S2.8, S4.16b, S5.2, S6.2, R2.5, R3.4 |
| `R-2.4.3` | S1.19, S2.8, S4.16b, S5.2, R2.5, R3.2, R3.4 |
| `R-2.4.4` | S1.19, S2.8, S4.16b, S6.2, R2.5, R3.4 |
| `R-2.4.5` | S2.8, S3.8, S4.16b, S5.2, R2.5, R3.4 |
| `R-2.5.1` | S1.17, S2.11, R2.8 |
| `R-2.5.2` | S1.17, S3.9, S5.2, R2.8, R3.5 |
| `R-2.5.3` | S1.17, S2.10, S3.9, R2.8, R3.5 |
| `R-2.5.4` | S3.9, S4.5b, R2.8, R3.5 |
| `R-2.5.5` | S0.2, S0.3, S1.16, S2.1, R3.2 |
| `R-2.6.1` | S0.2, S1.20, S2.11, S3.10, S6.1a, R2.6, R3.3 |
| `R-2.6.2` | S0.2, S1.20, S2.11, S3.10, S4.8, S5.5, R2.6 |
| `R-2.6.3` | S4.6, S5.5, R3.2, R3.3 |
| `R-2.6.4` | S4.7, S5.5, S6.2 |
| `R-2.6.5` | S4.8, S5.5, S6.2 |
| `R-2.7.1` | S1.21, S2.11, S3.10, S4.16c, R2.15, R4.7d, R4.7e |
| `R-2.7.2a` | S1.21, S3.10, R2.15 |
| `R-2.7.2b` | S4.16c, R2.15, R4.7e |
| `R-2.7.3` | S1.21, S3.10, S4.16c, S6.3b, R2.15, R4.7d, R4.7e |
| `R-2.8.1` | S1.11, S2.6, S3.11a, S4.14b, R2.11 |
| `R-2.8.2` | S2.7, S3.11a, S4.14b, R2.9, R2.12 |
| `R-2.8.3` | S0.2, S1.13, S2.4, S3.11b, S4.14b, R2.9, R2.10, R3.7, R4.5c |
| `R-2.8.4` | S1.12, S2.4, S3.11b, S4.14b, R2.9, R4.5c |
| `R-2.8.5` | S1.23, S3.11b, S4.14a, S6.3a, R2.11, R2.20 |
| `R-2.8.6` | S1.15, S2.5, S3.11b, S4.14b, R2.13, R3.8, R4.7f, R4.15e |
| `R-2.8.7` | S1.23, S2.6, S3.11b, S4.14a, R2.11, R3.2 |
| `R-2.9.1` | S0.2, S1.14, S4.15, R2.14, R4.7c |
| `R-2.9.2` | S1.22, S3.3, S4.15, R2.16 |
| `R-2.9.3` | S3.1, S4.2, S5.4, R4.15d (R4.9's remote-fetch half moved here 2026-10-20) |
| `R-2.9.4` | S1.24, S3.3, S4.15, S6.1a, R2.17, R4.6 |
| `R-2.9.5` | S6.1a, S6.2, S6.3a, S6.4 |
| `R-2.9.6` | S1.24, S3.12, S5.4, S6.1b, S6.4, R3.9, R4.6 |
| `R-2.9.7` | S5.4, S6.3b |
| `R-2.9.8` | S1.24, S5.4, S6.4, R2.17 |
| `R-2.10.1` | S3.5, S4.5a, S5.3 |
| `R-2.10.2` | S1.8, S2.12, S3.4a, S4.1, S5.3, R2.19 |
| `R-2.10.3` | S1.24, S3.4a, S4.2, S4.5a, S5.3, S6.1a, S6.2, R2.17, R4.9, R4.15d |
| `R-2.10.4` | S1.24, S3.4c, S4.3, S5.3, S6.3b |
| `R-2.10.5` | S1.24, S3.4b, S4.3, S4.4, S5.3, R4.9 |
| `R-2.10.6` | S3.4d, S4.5a, S5.3 |
| `R-2.11.1` | S0.2, S1.26, S2.12, S3.1, S4.5a, S4.12, R2.1, L4.6 |
| `R-2.11.2` | S4.10, S5.7, S6.1a, R4.2, L4.6 |
| `R-2.11.3` | S3.1, S4.11, S5.7, R2.19, R4.7a |
| `R-2.11.4` | S0.1, S0.2, S1.25, S3.1, S4.12, S5.8, R2.4, R2.19, R4.2 |
| `R-2.12.1` | S1.2, S2.12, S3.12, S4.15, S5.4, S6.3b, R2.21, R4.3, R4.4, R4.5, R4.0, R4.4a, R4.15 |
| `R-2.12.2` | S1.27, S2.2, S3.12, S4.14a, S6.1a, R2.20, R2.21 |
| `R-2.12.3` | S0.1, S0.3 — program/toolchain slice; language/ecosystem *selection* ratified by ADR-0050, revalidated at GATE-G1 · R2.21 (Round-2 verification leg), R4.5 |
| `R-2.12.6` | S4.9, S5.6, S6.4, R2.21, R3.2, R4.7, L4.6 |
| `R-2.12.4` | deferred(ADR-0210) — WS-L6 package · R4.8 (window opened by the round-4 charter 2026-10-20) · amended 2026-10-20: R4.8 (ruled postures recorded), L4.1, L4.2, L4.3, L4.5, R4.15a |
| `R-2.12.5` | program-level (novelty/naming thesis, §1); no code · amended 2026-10-20: R4.8 (naming market check), L4.4 (ruled crate scheme applied) |

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

### Round 3 — 2026-10-14 (decompose-spec mode=extend)

- **Grain:** one ticket per GATE-G4 `in-build`-coded carried row on its own subsystem seam — the
  round discharges the residual cluster GATE-G4 dispositioned `CARRY`. Six implementable tickets
  (R3.1–R3.6), one HUMAN marker (H5), three skeletons (R3.7–R3.9), the minimal tail
  (R3.10 closure + GATE-G5 + R3.11 docs).
- **`dep` rows are skeletons, not tickets:** BL-19 (`dpop` PoP verify + `wrapped_long_lived`,
  hh-secrets) and BL-45 (per-event AEAD, hh-ledger) are blocked on primitives the pure-std
  workspace does not carry and the hermetic-deps contract forbids importing. Scheduling
  implementation would either fabricate a claim or require a forbidden dep — so they are
  `Kind: skeleton` rows gated on **HUMAN-H5** (the operator's crypto-dependency ruling: in-workspace
  construction / deps-contract amendment by ADR / permanently typed-refused). Each carries only
  header + scope + spec §§ + REQ coverage; a later mode=extend insert fleshes or dispositions
  them when H5 lands.
- **CF-489 is a skeleton gated on WS-I6/OQ-441 (BL-30), not a small ticket:** the conform
  direction is not determinable in-build — CF-489's record states both sides are provisional
  placeholders pending the Stage-5 `DebtIndex` re-pin, and the ruling is a BL-30 `ruling` cell the
  operator ACCEPT'd at GATE-G4. The code-side conform (`hh-ontology::debt` constants + comment)
  becomes a small mechanical ticket only once the ruling names a side.
- **Shared-decision rulings made here:** (1) the R-2.6.3 child-lease/delegation substrate has ONE
  owner — **R3.2** (BL-44's child-lease producers); R3.3's `delegation_reason` legs consume it
  (ordering edge R3.2 → R3.3). (2) `resume_set` splits on its own seam: the *producer* is R3.2's
  (DF-S2.3-1); the *consumers* landed at R2.5 and R3.4 keeps only §5c consumer/fixture legs — no
  double ownership. (3) Flake + bookkeeping sweep is one hygiene ticket (R3.1, the R2.1 pattern)
  — small disjoint fixes sharing one verification pass; merging the flakes into subsystem tickets
  would delay them behind unrelated working sets.
- **Not scheduled this round (recorded):** every GATE-G4 `env` row (BL-01/BL-17/BL-26/BL-27/
  BL-31/BL-32 — foreign toolchains, live judge, oauth/mtls exchange legs, live transports, remote
  fetch), every `human` row (BL-02/BL-03/BL-04 — H4/H1/H2 persist as chain rows 128/61/82), the
  `ruling`/`window`/`standing`/`accepted` set (BL-30, BL-33–BL-39, BL-41/BL-42/BL-43, BL-05 —
  governance/standing obligations the operator dispositioned at GATE-G4, not implementation), and
  DF-S1.15-1's custody/`subagent_anchor`/`producer_resolution` legs (BL-05/BL-30 homed).
  DF-S2.4-2's bare cell gets a dated residual note under R3.1, not a ticket (its work is BL-31's
  TLS transport — `env`).
- **Ordering:** chain order is the dependency spine; specific backward edges — R3.3 after R3.2
  (the child-lease substrate), skeletons after the implementable rows and adjacent to their H5
  gate, R3.10 after all round-3 rows (it accounts them), GATE-G5 after closure, R3.11 after the
  gate.
- **Phase-4 adversarial review record (explicit reread-and-argue pass, 2026-10-14 — no
  fresh-context subagent was invoked; the decomposer re-read the registers and argued the plan
  down):** **Fragmented decisions found + fixed:** R-2.6.3 appears in both BL-44 (child-lease
  producers) and BL-14 (`delegation_reason`) → single-owner ruling above with the ordering edge;
  `resume_set` appears in both DF-S2.3-1 (producer) and DF-S1.19-1 (consumers) → verified the
  consumers landed at R2.5, producer-only assignment recorded on both tickets. **Overflow risk:**
  R3.4 is the round's largest (two DF rows' worth of §5c members) — carries a pre-registered
  R3.4a/R3.4b split seam in its Notes so a run-time split never re-decomposes the round; all
  other rows sized ≤1400 changed lines. **Orphan seams audited:** BL-19/BL-45's primitives and
  CF-489's direction are deliberately gated, not orphaned — each skeleton names its gate and its
  fleshing path. **Coverage check:** every GATE-G4 `in-build`/`dep` row maps to exactly one R3
  row (BL-44→R3.2, BL-14→R3.3, BL-15→R3.4, BL-16→R3.5, BL-10→R3.6, BL-19→R3.7, BL-45→R3.8);
  the LEDGER flake findings + G4 flags → R3.1; CF-489 → R3.9. **Double-ownership check:** the two
  shared-locus seams (`resume_set`, R-2.6.3) name producer vs consumer per side; no cell claimed
  twice. **Ordering check:** every Depends-on points backward; skeletons carry no Run line;
  GATE-G5's criterion is a review, never pre-answered. **Over-factoring check:** the three flakes
  + bookkeeping merged into one R3.1 (reclaiming fixed cost); BL-19 and BL-45 kept as two
  skeletons (different subsystems — a per-primitive ruling may land for one and not the other).
  **Verdict:** the 13-row partition is dependency-clean and honesty-preserving; residual risk is
  R3.4's size, mitigated by the pre-registered split. The plan is a starting point, not a
  contract in stone — `orchestrate-build` may split/merge at run time and write the change back
  here.
- **Amendment — operator rulings applied 2026-10-14 (same-day, post-seed commit `89584c8`):**
  (1) **HUMAN-H5 ruled — disposition (b): a vetted external crypto crate (ring/aws-lc/rustls
  family) is admitted.** R3.7 and R3.8 flip skeleton → ticket (full contracts, Run lines, size
  budgets); the crate+version choice is owned by the dep-choice ADR inside R3.7 (≥7-days-published
  supply-chain rule; ADR-0050 C11 vetting posture; hermetic-deps note amended by the ADR, never
  silently). The R3.7 → R3.8 edge is now a real dependency — R3.8 consumes the pinned crate for
  its AEAD surface where the ADR's scope covers it, else authors a sibling ADR. (2) **CF-489
  direction ruled — conform code to spec:** `DebtPolicy::default()` goes to §5h.6's proposed
  60 d/30 d; R3.9 flips skeleton → ticket (constants + comment + caller/test sweep + dated CF
  resolution; the OQ-441 deferred markers stand for WS-I6's re-pin — the conform removes the
  divergence, it does not retire the question). (3) **Environment path is live:** the operator
  states HUMAN-H1/H2/H3 are provisionable — the env-gated rows (BL-01/17/26/27/31/32) stay
  excluded/skeleton-coded *until provisioning lands*; they unblock on the H-rows completing, and a
  future `mode=extend` round schedules them once the environment exists.
  **Ordering (recorded call):** rows keep positions 144–146 — renumbering would break the
  filename convention; R3.7→R3.8 is the only new ordering edge (the dep pin); R3.9 has no code
  coupling and stays a small ticket ahead of closure; R3.1 hygiene-first and R3.2→R3.3 unchanged.
  Row numbering stable; no row deleted; the skeleton texts stand as history in commit `89584c8`.

### Round 4 — 2026-10-20 (decompose-spec mode=extend)

- **Charter:** the 2026-10-20 operator rulings (H1 TypeScript · H2 GitHub Issues + tunnel ·
  H3 toolchains + online corpus · H4 reviewer to be named) plus the decomposition charter's
  reopening of the GATE-G4 `window` set — BL-33–BL-39 spec/OQ closure engineering is the
  round's core. Records-only pass; no implementation.
- **Grain:** one ticket per coherent engineering cluster — the toolchain cluster splits on
  its natural seams (bundle extension → foreign impl → measurement/CI); the register
  packages run one ticket per ADR package at the R2.22–R2.24 grain, with BL-35+BL-39 merged
  (the R2.22 pairing — same Stage-0–1/fix context, each too small alone for its context
  cost). Two skeletons only — both genuinely gated; env rows got dated residual notes
  instead of placeholder rows (scheduling them would pretend a ruling exists).
- **Shared-decision ownership (one owner each):** OQ-388 + OQ-170 ruling legs → **R4.15**
  (the DF-S4.13-1/DF-S2.5-1 cells are BL-30's per the GATE-G5 mapping; the BL-37/BL-36
  packages record cells + pointers, never second dispositions). CF-488's adjudication brief
  → **R4.15** (R4.11 records the BL-39 register note). The trust-field rulings R4.9 gates on
  → **R4.8**'s brief. The `foreign/e2-python` verbatim import → **R4.5** (R4.4 does not
  import it).
- **Packages produce briefs, not rulings:** package tickets land dated engineering cells +
  evidence-complete recording closures only; every genuine ruling is a *proposed*
  disposition in `docs/build/planning/round4-*-briefs.md` for the operator's GATE DECISIONS
  (at GATE-G6 or whenever they arrive); **R4.17** conforms what was adopted (register flips,
  ADR amendment logs, spec amendments per protocol — approver = the recorded decision row).
- **Skeletons:** R4.6 gates on H3's *remaining* provisioning (the LEDGER carries a scoped
  `pre-authorization` row — corpus access + verification budget authorized in principle,
  accounts/keys + actual spend are operator steps; expires: GATE-G6). R4.9 gates on the
  BL-33 trust-field rulings + the remote-fetch environment (dep+env, ordered behind R4.8).
- **Environment exclusions recorded:** BL-17 (live judge binding — provider/model spend),
  BL-31 (live transports + TLS-terminating transport), BL-27 (OAuth/mTLS exchange legs) —
  none covered by the corpus authorization; dated residual notes + flagged for a future
  operator ruling. BL-02's H4 marker carries forward (reviewer pending). BL-05/BL-41/BL-42/
  BL-43 standing accepted — no tickets.
- **Ordering:** R4.3→R4.4→R4.5 is the only strict engineering chain; skeletons adjacent to
  their gates; packages in ADR order; R4.15 after the packages (its briefs cite their
  outcomes); the tail mirrors round 3 (closure → gate → docs) with R4.17's added conform
  leg. No forward Depends-on.
- **Tail choice (recorded):** minimal extended-shape tail = closure reconcile + discharge
  gate + docs-refresh-with-conform — mirrors round 3; the conform leg exists because the
  round's packages produce operator-dispositioned rulings that need a post-gate landing.
- **Phase-4 adversarial review:** the decomposer's fresh-context reread argued the plan
  down — full findings/resolutions recorded in
  `docs/build/planning/2026-10-20_round4-decomposition.md` §Phase 4 (8 findings: the e2-tree
  absence → verbatim import in R4.5; the CHECKER.md merge hazard → append-only contract
  edits; the OQ-388/OQ-170/CF-488 single-owner seam; the R4.13/R4.12 overflow seams
  pre-registered; the BL-35+39 merge; no forward deps; the env-honesty cut; the 19-row
  coverage audit).
- **DEFERRALS coverage:** all 19 owed rows mapped to exactly one round-4 row or a dated
  residual note — the mapping table lives in the planning record; the seven rows carried on
  env/standing/human homes (DF-S0.3-3, DF-S1.15-1, DF-S1.18-1, DF-S1.21-2, DF-S1.21-3,
  DF-S2.4-2, DF-S4.11-2) got fresh dated round-4 notes appended.

### Round 4 — amendment 2026-10-20 (launch re-charter; decompose-spec mode=extend)

- **Charter:** the operator's launch charter (verbatim in GATE DECISIONS `### Round 4`) + the 2026-10-20 rulings (H1
  TypeScript · H2 GitHub Issues + tunnel · H3 toolchains + online corpus · rustls over ring · Anthropic + OpenAI + local
  Ollama · $100 cap · MIT OR Apache-2.0 · BL-41/42/43 → policy · keep provenance · crates.io under `hharness`/`hharness-*` ·
  local + protected dispatch live CI · H4 decline · GitHub OAuth App device flow · mid-round rulings gate · provisioning now ·
  private sandbox repo). Planning only — no code. The drive runs on a 256k-token, non-compacting subagent.
- **Hard objective (GATE-G6):** zero OPEN DEFERRALS, zero open BL, every coverage row MET (MET-DIFFERENTLY only as a
  permanent design decision with its ADR), `live-executed` evidence on every live leg, launch hardening green, GATE-ACCEPT's
  ten deviations discharged or confirmed permanent. "Carry" is not an allowed disposition anywhere in the plan.
- **Residuals verified from dated markers (deviation from the brief's assumptions):** DF-S1.18-1's residual is only the
  live halves (live Transport/CredentialPort, real-provider `probe_profile`, live-signal arms, AC-R-2.3.1-11 live OOP) — the
  routing arms/K5/corpus/turn-loop wiring landed at S2.10/S3.7/R2.7, so the brief's "154d hermetic gateway remainder" is
  not needed; DF-S1.21-3's C2 detectors landed at S4.16c (residual = live judge at `link`); DF-S1.21-2's residual = the
  reference-corpus AC fixture halves + judge consumption on real bindings; DF-S1.15-1's residual = custody +
  `subagent_anchor` + `producer_resolution`; BL-26's resolver leg (DF-S1.24-1) and BL-27's Ref/SSE legs are DONE; BL-05's
  live members reduce to LT-03 live corpus + DF-S1.21-2 + DF-S1.15-1. **Main's** ac5 rule (not the chain's) is the
  derivation-aware one — R4.0 resolves to main's.
- **Context economy (shared decision, owned by R4.1):** `scripts/bm-row.sh`, `scripts/deferrals-open.sh`,
  `scripts/spec-section.sh` land first; every later contract's Load lines name ids, not files; whole-file reads of the
  LEDGER/DEFERRALS/BACKLOG/registers/spec/manifest are forbidden in every contract. R4.0/R4.1 carry literal grep forms.
- **Pre-splits (charter):** R4.4 → R4.4/R4.4a at the reference tree's seam (≈1.2k lines of foreign code over two tickets);
  R4.12 → R4.12/R4.12a and R4.13 → R4.13/R4.13a by register id order (independent rows; R4.8 owns the brief convention).
- **Live track ordering:** framework (R4.5a) → model port + HTTP codec home on local Ollama (R4.5b) → TLS (R4.5c) →
  hosted providers (R4.5d) → probes/live-signal arms (R4.5e) → every other live leg. No live leg precedes its TLS /
  credential prerequisites. Spend allocation (R4.5a): R4.5d 5 · R4.5e 5 · R4.6 15 · R4.7b 5 · R4.7e 15 · L4.6 30 · reserve 25.
- **Rulings venue:** briefs (R4.8, R4.10–R4.15) propose FINAL dispositions only (resolve-with-evidence / adopt-interim-as-final
  / descope-by-ADR / needs-code-leg) in batch-adoptable tables; GATE-G7 ("GATE-R4R" — `GATE-G<n>` is the only gate-id form the
  validator admits) takes the operator's acts; R4.15a–e are skeletons fleshed by a scoped `mode=extend` at the gate.
- **Launch track last:** OSS files → policy → CI/supply chain → scripted rename (after every engineering row) → publish
  readiness + release workflow → live capstone on the shipped names → quickstart test → closure → GATE-G6 → docs → HUMAN-H7
  → launch record.
- **Single owners:** OQ-170 custody proposal → R4.15 (implementation R4.15e); OQ-388 ratification proposal → R4.15
  (records R4.15b); CF-492/CF-493 code → R4.15c; trust fields → R4.8 (implementation R4.15d); HTTP codec home → R4.5b; TLS
  home crate → R4.5c; evidence schema + spend ledger → R4.5a; foreign core → R4.4; brief convention → R4.8.
- **Operator stops in the drive:** HUMAN-H6 (provisioning, before R4.5a); R4.7a's device-code entry (operator present);
  GATE-G7; GATE-G6; HUMAN-H7. Any spend-cap or unprovisioned-NAME stop pauses and asks.
- **Phase-4 review:** fresh-context critic findings + resolutions in
  `docs/build/planning/2026-10-20_round4-decomposition.md` §Amendment → Phase 4.

## Plan extensions
(append-only: inserts `NNa_…`, splits `<ID>a`/`<ID>b` with the original marked superseded-by-split, rounds)
- 2026-09-15 · **inserted** `003a_S0.3b__cross-candidate-online-spike.md` (id `S0.3b`, row `3a`) between S0.3 and GATE-G1. Reason: the operator's GATE-G1 disposition authorized an **online** spike budget to close the machine-achievable cells of DF-S0.3-1 (MCP/ACP official SDK round-trip + cross-candidate G1 byte-identity) and DF-S0.3-3 (E2/E3 cross-candidate scoring + E5b/E5c splits). The R2 human cross-camp reviewer signature and the Stage-3 polyglot CI (DF-S0.3-2) are explicitly NOT pulled into this ticket. GATE-G1 stays PENDING until S0.3b lands and the operator re-reads the extended sheet.
- 2026-10-07 · **split** `R2.9` (row 115, `115_R2.9__egress-containment-legs.md`) →
  `R2.9a` + `R2.9b` at the ticket's pre-registered seam (`## Notes`), run-time split under the
  revisability rule — the ticket is the round's largest and overflows a subagent context.
  **R2.9a** (this run, branch `svitali/harnessharness-r2.9`): DF-S1.12-1 verify pass,
  DF-S1.12-2 (`amend()`/`AmendmentDiff`/durable `security.containment.amended` + the
  `containment_amended` results annotation + the `resources → BudgetNode` dimension
  mapping), DF-S2.4-1 producer seams (the `net_egress` effect-surface route, the
  ask-endorsement ingress on resume, the `fork` rebind call site). **R2.9b** (successor —
  runs the same ticket file `115_R2.9__egress-containment-legs.md`; no second ticket
  authored): DF-S1.12-4 T-CON residuals + per-backend `lowering_loss` + the four metrics'
  runtime emission, DF-S1.12-6 `tls.terminate`/`inspect_hooks`/`approved_host_body`,
  DF-S2.4-3 Stage-3 leak-scan surfaces. R2.9's manifest row reads superseded-by-split;
  `nextTicket` advances to `R2.9b` on R2.9a's closeout; R2.9b forks from
  `svitali/harnessharness-r2.9`.
- 2026-10-06 · **round 2 appended** — rows 107–136 (`R2.1`…`R2.27`, `HUMAN-H3`, `HUMAN-H4`,
  `GATE-G4`) by `decompose-spec mode=extend` over the Round-1 closeout. Sources: the 2026-10-06
  decision memo + `docs/build/BACKLOG.csv` (BL-01…BL-43) + the OPEN/PARTIAL DEFERRALS set. Round-1
  rows 1–106 unchanged. See `## Decomposition decisions → Round 2` for the partition and the
  not-scheduled set; `docs/build/planning/2026-10-06_round2-decomposition.md` for the full record.
- 2026-10-14 · **round 3 appended** — rows 137–149 (`R3.1`…`R3.11`, `HUMAN-H5`,
  `GATE-G5`, three `Kind: skeleton` rows) by `decompose-spec mode=extend` over the GATE-G4
  closeout. Sources: the GATE-G4 readout's §2 carried set + §8 flags + §9 dispositions,
  `docs/build/BACKLOG.csv` (BL-01…BL-46), the owed DEFERRALS set, LEDGER OPEN FINDINGS. Round-1/2
  rows 1–136 unchanged. Scope: the `in-build`/`dep` carried rows only — human/env/ruling/window/
  standing/accepted rows carry forward on their G4 dispositions. See `## Decomposition decisions →
  Round 3` for the partition and the not-scheduled set;
  `docs/build/planning/2026-10-14_round3-decomposition.md` for the full record.
- 2026-10-14 · **round-3 amend pass** — same-day operator rulings applied in place: HUMAN-H5
  answered (disposition (b) — external crypto crate admitted; dep-choice ADR inside R3.7),
  CF-489 direction ruled (conform code to spec — `DebtPolicy::default()` → §5h.6's 60 d/30 d), and
  the env-path statement (H1/H2/H3 provisionable; env rows unblock on provisioning). Rows
  144/145/146 flipped `skeleton → ticket` (kind cells + full contracts); row 143 records the
  ruling; no rows renumbered or deleted. See `## Decomposition decisions → Round 3` amendment
  block and `docs/build/planning/2026-10-14_round3-decomposition.md` §Amendment.
- 2026-10-20 · **round 4 appended** — rows 150–167 (`R4.1`…`R4.17`, `GATE-G6`, two
  `Kind: skeleton` rows R4.6/R4.9) by `decompose-spec mode=extend` over the R3.11 closeout +
  the same-day operator rulings (H1 TypeScript; H2 GitHub Issues + tunnel; H3 toolchains +
  online corpus — scoped `pre-authorization` row in the LEDGER; H4 reviewer pending).
  Sources: `docs/build/readouts/GATE-G5.md` owed set, `docs/build/BACKLOG.csv` (BL-01…BL-46),
  the 19-row owed DEFERRALS set, the LEDGER rulings. Round-1/2/3 rows 1–149 unchanged.
  Scope: the H1/H2/H3 discharge legs + the reopened `window` packages (BL-33–BL-39) + BL-30
  adjudication prep + hygiene; env rows (BL-17/BL-27/BL-31) and the standing/human set not
  scheduled. See `## Decomposition decisions → Round 4` for the partition, the shared-decision
  ownership, and the not-scheduled set;
  `docs/build/planning/2026-10-20_round4-decomposition.md` for the full record.
- 2026-10-20 · **round-4 amend pass — launch re-charter** (`decompose-spec mode=extend` amendment; precedent `75d61ab`):
  the operator's launch charter + rulings (LEDGER GATE DECISIONS `### Round 4`) applied. Rows 150–167 rewritten in place
  before execution (R4.6 and R4.9 skeleton → ticket; GATE-G6 criterion → the charter; R4.17's conform leg dropped); 31
  suffix rows **inserted**: `149a_R4.0` (main→chain sync), `149b_HUMAN-H6`, `153a_R4.4a` (pre-split of R4.4),
  `154a`–`154e` (`R4.5a`–`R4.5e` live foundation), `156a`–`156f` (`R4.7a`–`R4.7f`), `161a_R4.12a`, `162a_R4.13a`
  (pre-splits), `164a_GATE-G7` (rulings gate, alias GATE-R4R), `164b`/`164e`/`164h`/`164k`/`164m` (`R4.15a`–`R4.15e` skeletons, fleshed
  at GATE-G7 — letters spaced so their splits have room), `164p`–`164v` (`L4.1`–`L4.7` launch track), `167a_HUMAN-H7`, `167b_R4.18`. No file renamed or deleted; no row
  superseded (every seed row keeps a live scope). See `## Decomposition decisions → Round 4 — amendment` and
  `docs/build/planning/2026-10-20_round4-decomposition.md` §Amendment (zero-deferral matrix, Phase-4 review).
- 2026-10-20 · **round-4 amend (b) — independent critic fixes** (`decompose-spec mode=extend` same-round amendment;
  precedents `75d61ab`, `02e37fe`): 4 MAJOR + 8 MINOR findings applied to unexecuted contracts in place (dated `> Amended
  2026-10-20 (b)` banners; no file renamed): crates.io availability check first in L4.4 / enumerated in R4.8 / stop-and-ask
  in L4.5 (planning baseline: `hharness` + 51 `hharness-<x>` → 404, 2026-10-20T16:31:51Z–16:33:02Z); gitleaks staged-diff
  gate + full-history baseline + `.gitleaks.toml` owned by R4.5a, reused by L4.3; HUMAN-H7 checks-required merges + green
  main SHA before tag, recorded by R4.18; R4.16/GATE-G6 surface-coverage, origin/main-containment and DF-S1.15-1
  enumeration thresholds; L4.5 rust-version / docs.rs / target-set ADR; HUMAN-H6 names-only pre-flight. One suffix row
  **inserted**: `164w_L4.6a` (live capstone part 2 — `hh-acp`, `hh-mcp-lab`, SDK/embed, packaged plugins; split from L4.6
  for the ≲100k working set; USD 0, no allocation change) — round 4 is now 50 rows. See
  `docs/build/planning/2026-10-20_round4-decomposition.md` §Amendment 2026-10-20 (b).
- 2026-10-20 · **inserted** `151a_R4.2a__ci-fast-path-and-full-run-cadence.md` (id `R4.2a`, row `151a`) between R4.2 and
  R4.3 — run-time insert (BM-MANIFEST-03): `decompose-spec mode=extend` scoped to this one ticket, Phase-4
  fresh-context review recorded in `docs/build/planning/2026-10-20_R4.2a-insert-review.md`; operator rows LEDGER GATE
  DECISIONS `### Round 4` `CI insert` D1–D7 (2026-10-20T23:07:26Z). Slot moved per the inject prompt's Case C — R4.2 was
  in flight when the prompt arrived; the operator confirmed "Yes: 151a / R4.2a after R4.2". Reason: measured
  2026-10-01..20 CI — 4,406 job-minutes, 46 % on `push` runs duplicating a PR run, 58 % on docs-only heads that
  each ran the full ≈ 11-min job, ≈ 3.3 min of every full job in the non-gating S2 spike; projected −77 to
  −80 % job-minutes and ≈ 1-min docs-only boundary heads. CI cadence from here (D4): one `workflow_dispatch`
  full run on the chain tip at GATE-G7, at GATE-G6 and before HUMAN-H7's tag — never per ticket. Phase 2
  (test-time levers) is a later, separately ordered insert. No file renamed; no row superseded; round 4 is
  now 51 rows.
- 2026-10-21 · **split** `R4.4a` (row 153a, `153a_R4.4a__e3-typescript-foreign-impl-part2.md`) → rescoped
  `R4.4a` + `R4.4b` (new row 153b, `153b_R4.4b__e3-typescript-foreign-impl-part3.md`), run-time split under
  the contract's own size-budget STOP (orchestrate-build §2.4, BM-MANIFEST-03; precedent R2.9 2026-10-07).
  The R4.4a worker (crash-resume of the 09:00Z renderer crash) measured the remaining work at ≈2.2× the
  900-line budget — `hir-documents`-driven (`lib/hir.ts` ≈650–900 + its battery ≈180 ≈ half the overage;
  projection in `docs/build/runs/R4.4a.md`). **R4.4a (rescoped)** keeps the committed, verified partial —
  plugin-manifests + ledger-scenarios arms, `RegistryReplay.classCatalog()`, `foreign.ts` wiring @ eadd178;
  `hh-xcheck verify` 52 pass / 5 `missing answer` = exactly the hir-documents cells — plus `answers.json`
  over delivered arms, README notes, its `r4_4a.rs` battery, the verify-pending closeout and DF progress
  notes. **R4.4b** takes the `hir-documents` port (E1 spelling `crates/hh-xcheck/src/hir_replay.rs` over the
  `hh-hir` decode/validate/seal spine + `corpus.rs`'s five cases), the eighth-arm wiring, regenerated
  `answers.json`, all-arms `xcheck-e3.sh`, README closeout, and the DF-S1.2-2/DF-S1.27-1 DONE flips; it
  forks from `svitali/harnessharness-r4.4a`; `nextTicket` advances to `R4.4b` on R4.4a's closeout. Contract
  amendment appended to the R4.4a file (executed-contract rule); Phase-4 fresh-context review recorded in
  `docs/build/planning/2026-10-21_R4.4b-split-review.md`. No file renamed; no row superseded; round 4 is
  now 52 rows.

- 2026-10-22 · **GATE-G7 fleshing (scoped `decompose-spec mode=extend`)** — the operator's
  "Adopt all" (GATE DECISIONS `### Round 4`, 2026-10-22) adopted all 146 brief proposals. The
  five R4.15a–e skeletons fleshed; the two records-conform tickets split by their own fleshing
  notes (~146 acts ≫ the ~900 ceiling): **R4.15a** → BL-33/BL-34 + the ADR-0211 descope ADR;
  insert **R4.15a2** (164c) → BL-35/39 + BL-36; **R4.15b** → BL-37 + BL-30 records + OQ-388;
  insert **R4.15b2** (164f) → BL-38 + the eight spec-text legs; **R4.15c** → the five code legs
  (OQ-468, OQ-430, CF-492, CF-493, OQ-173 legal_hold); **R4.15d** → OQ-332 + DF-S4.2-1 (USD-0
  live fetch); **R4.15e** → OQ-170 kernel-held Ed25519 custody. `R4.15a2`/`R4.15b2` use the
  digit suffix because letters a–e were pre-assigned to the skeleton set while the 164c/164f
  row slots were reserved for exactly this split (the 164-letter-spacing note). Phase-4
  fresh-context review applied: M1 (these manifest rows + this entry), M2 (LEDGER correction +
  `nextTicket: R4.15a`), M3 (OQ-445's L4.2 leg amended onto 164q), MINOR-1–11 (contract
  numbering/act-count/spec-mechanics/OQ-170 ownership/evidence_max_age disambiguation/164s's
  OQ-068 Load line/brief-table reconciliation notes). Round 4 is now **54 rows**; the two
  densest tickets (164f, 164h) carry a split-ready note — further suffix splits are sanctioned
  at run time if their budgets trip. Rows are not renumbered (insert convention).
