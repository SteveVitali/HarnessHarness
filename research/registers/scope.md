# Scope Register — §2 catalogue as tracked requirements

**Seeded:** 2026-09-09 (Preflight, doc 3 §11.2 step 5) from doc 3 §2. **MoSCoW default rule:** `Must` for C0; `Should` for C1; `Could` for C2–C3; `Could*` for C4 (fully specified per ADR-0002 — *not* stubbed — but latest on the build ladder). MoSCoW is continuous and frozen at Phase 5 (doc 3 §8).
**Columns:** `req-id` · item · tier · novelty (⊕ NEW / ↑ EXT) · owner WS · target spec section (§7.2 skeleton) · MoSCoW · status (`open` / `specified` / `deferred(ADR-####)`).

Every item must end Phase 5 as `specified` (interface contract + data model + acceptance criteria + build-stage) or `deferred(ADR)` — never dropped (§1.4, §7.3).

## 2.1 Foundations (C0)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.1.1 | Ontology / conceptual systematization (7-plane + policy-stack formalism + validity vs compliance + compatibility surface + two participant classes) | C0 | — | WS-A2 (A1, L7) | 2 | Must | **specified-by-ADR** (Ontology **v1** in `registers/ontology.md`: ADR-0012/0013/0014; Spec §2 text lands at Phase 5; CF-020/CF-021 resolved) |
| R-2.1.2 | Harness IR (HIR) — typed behavioral entities + edges | C0 | ⊕ NEW (conditional: novelty holds only if the seven-plane entity set with provenance is delivered — CF-011) | WS-A3 | 3 | Must | **specified-by-ADR** (HIR/1: ADR-0015 type discipline, ADR-0016 catalogue, ADR-0017 typed diff, ADR-0018 hosted processes; T-LCD-01/-02/-06/-10/-12 pass on paper, T-03 at Stage 3; OQ-040/041 answered; novelty (a) delivered on paper — CF-011 re-check) |
| R-2.1.3 | Compilation model — IR → runtime + model profiles + protocol targets (lowering / lifting / lowering loss report) | C0 | ⊕ NEW | WS-A4 | 3 | Must | **specified-by-ADR** (ADR-0019 pipeline, ADR-0020 Model Profile contract, ADR-0021 protocol targets, ADR-0022 equivalence E1–E7; OQ-043 answered; `lcd_report`/`UnexpressibleSurface` as error) |
| R-2.1.4 | Configuration & composition model; code-vs-config boundary | C0 | ↑ EXT | WS-A5 | 3, 6 | Must | **specified-by-ADR** (ADR-0023 definition-is-data, ADR-0024 code-vs-config, ADR-0025 assembly validation; OQ-044 answered) — **stage note (ADR-0048):** grammar + `load/compose/resolve/validate_assembly/identity/instantiate/verify_resume` contracts are C0/Stage 1 here; assembly/registry *services* are C1 (R-2.10.1/2) |
| R-2.1.5 | Provenance & authority model | C0 | ⊕ NEW | WS-L3 | 8 | Must | **specified-by-ADR** (ADR-0033 scheme, ADR-0034 propagation/retrieval order, ADR-0035 endorsement + monitor set; CF-006 resolved at the scheme level; H1/H2/D1/D4 inherit) |
| R-2.1.6 | Resource-economics & accounting model | C0 | ↑ EXT | WS-L2 | 8 | Must | **specified-by-ADR** (ADR-0039 taxonomy/accounting, ADR-0040 budget contract, ADR-0041 matched-budget definitions; OQ-033 answered) |

## 2.2 Core runtime & durability (C0/C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.2.1 | Event store / run ledger | C0 | — | WS-B1 | 5 | Must | **specified-by-ADR** (ADR-0026 ledger authoritative, ADR-0027 id model, ADR-0028 durability = reconstruction, ADR-0029 canonical form/chain/offload; OQ-003/OQ-042 answered; taxonomy amendments in the ADR-0026 log) |
| R-2.2.2 | Effect & transaction model | C0 | ⊕ NEW | WS-B2 | 5 | Must | **specified-by-ADR** (ADR-0030 lifecycle, ADR-0031 effect risk class, ADR-0032 compensation; OQ-015 answered) |
| R-2.2.3 | Durable execution & recovery | C1 | ↑ EXT | WS-B3 | 5 | Should | **specified-by-ADR** (ADR-0130…ADR-0132: durability contract, wakeups/suspended runs/goal continuation, self-healing + kill-point battery) — **stage note (ADR-0146):** the recovery procedure, writer takeover, `retry_due` recomputation and the Stage 3 kill-point battery are the C0 slice; suspension/wakeups/goals/healing remain C1 |
| R-2.2.4 | Reversible & speculative execution | C2 | ⊕ NEW | WS-B4 | 5 | Could | **specified-by-ADR** (ADR-0133…ADR-0135: branch model + fork/snapshot/rollback, speculative-effect containment with the `deferred` phase (ADR-0030 amended), nondeterminism recording + replay validity); C0/Stage 2 slice = branch model, `deferred` schema, `fs_tree` snapshots |
| R-2.2.5 | Execution-environment abstraction | C0 | ↑ EXT | WS-B5 | 5 | Must | **specified-by-ADR** (ADR-0136…ADR-0138: environment handle over three backend placements (OQ-045 final — out-of-process helper confirmed; ADR-0049 stage note satisfied), identity/snapshot semantics, effect-operation surface + accounting; adopts H4 containment slot and E5/I4 handle operations verbatim) |

## 2.3 Model plane (C0/C1)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.3.1 | Model adapter / gateway | C0 | ↑ EXT | WS-C1 | 5 | Must | **specified-by-ADR** (ADR-0118…ADR-0120: model-blind gateway consuming the `ProviderRequestPlan` (OQ-072/CF-044 closed), closed stop-reason/error-class vocabularies with `WireDialect` descriptors, capability discovery and drift) |
| R-2.3.2 | Model router | C1 | ↑ EXT | WS-C2 | 5 | Should | **specified-by-ADR** (ADR-0121…ADR-0123: router as policy component over the `ModelRoleTable`, retry-vs-reroute split, ensembles as procedures (C2/Stage 4+)); C0 static slice, C1 policy family, C4 learned policies |
| R-2.3.3 | Model Profile / Profile Compiler | C1 | ⊕ NEW (WS-A1 §6.2 item 2: whitespace confirmed) | WS-C3 | 5 | Should | **specified-by-ADR** (ADR-0124…ADR-0126: `ProfileRuleInventory/1` (thirteen kinds), capability/constraint split, profile test contract, expiry/revalidation; ADR-0048 stage note satisfied) — **stage note (ADR-0146):** a C0/Stage 2 schema slice of `compaction_reminder` params is admitted; the rule family, chains and probes stay C1/Stage 5 |
| R-2.3.4 | Caching & token economics | C1 | — | WS-C4 | 5 | Should | **specified-by-ADR** (ADR-0127…ADR-0129: cache taxonomy K1–K6 + `CacheSemantics` as data, prefix-stable layout constraints, affinity key + expected-vs-observed accounting, `cache_policy` on `MatchSpec` (ADR-0041 amended); harness-level caches C1/C2) |

## 2.4 Context & memory plane (C0/C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.4.1 | Context builder / policy engine | C0 | ↑ EXT | WS-D1 | 5 | Must | **specified-by-ADR** (ADR-0072…ADR-0074: kernel admission over a content-addressed `ContextPlan`, budget discipline/offloading, six reserved slots + rendering ownership (OQ-063 closed); consumes ADR-0034 as amended — kernel stamps `context_label`) |
| R-2.4.2 | Compaction strategies (pluggable family) | C1 | ↑ EXT | WS-D2 | 5, 6 | Should | **specified-by-ADR** (ADR-0075…ADR-0077: one compaction class with a closed op sum, provenance of derivations, `lab/compaction-family-v1`) — **stage note (ADR-0146):** the C0 trigger is the kernel gauge cap and the `evict_oldest` compactor is C0/Stage 2; the family and soft-threshold rules are C1 |
| R-2.4.3 | Retrieval & memory hierarchy | C1 | — | WS-D3 | 5 | Should | **specified-by-ADR** (ADR-0078…ADR-0080: five addressing classes over the ledger/registry/workspace, retrieval contract with the fixed filter order, memory writes/consolidation under the ADR-0035 memory-store row) |
| R-2.4.4 | Memory lifecycle semantics | C2 | ⊕ NEW | WS-D4 | 5 | Could | **specified-by-ADR** (ADR-0081…ADR-0083: invalidation contract + lifecycle state, supersession/revocation/conflict sets, justification-based inheritance + `MemoryStaleIndex`; ADR-0048 stage note satisfied) |
| R-2.4.5 | Skills & Procedure IR | C2 | ⊕ NEW | WS-D5 | 3, 5 | Could | **specified-by-ADR** (ADR-0084…ADR-0086: `ProcedureProfile/1` in `ext` (CF-038 resolved), three compilation targets, skills as lifted procedures) — **stage note (ADR-0146):** C0/Stage 2 slice = `ProcedureProfile/1`, `lift_skill`, `instruction` target, index/body candidates, deterministic detectors, `RenderTimeExecution`, claims → grants at seal; `workflow_node`/tests/exports C1/Stage 3–4; `subagent_task`/induction C2 |

## 2.5 Tools & action plane (C0/C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.5.1 | Tool registry & typed capabilities | C0 | ↑ EXT | WS-E1 | 5 | Must | **specified-by-ADR** (ADR-0087…ADR-0089: `ToolCapability` field discipline (ADR-0016 item 6 reworded), registry contract with `quarantined` lifted records, cost/risk/observability metadata semantics) |
| R-2.5.2 | Tool-interface compiler | C2 | ⊕ NEW | WS-E2 | 5 | Could | **specified-by-ADR** (ADR-0090…ADR-0092: exposure modes + surface families + `SurfaceBinding` + closed transform vocabulary (ADR-0022 amended), wrapper synthesis/admission, result/failure rendering) — **stage note (ADR-0146):** the C0/Stage 1 slice is schema-only; static composites C1/Stage 5; freeform/code-mode/shim C2 |
| R-2.5.3 | Tool scaling (discovery, deferred loading) | C1 | — | WS-E3 | 5 | Should | **specified-by-ADR** (ADR-0093…ADR-0095: exposure plan with `callable ⇔ revealed`, discovery capability + `catalog_index`, catalog sync/epochs) |
| R-2.5.4 | Protocol edges — MCP (client+server), A2A, ACP | C1 | ↑ EXT | WS-E4 | 5, 7 | Should | **specified-by-ADR** (ADR-0096…ADR-0099: boundary placement, MCP edge (ADR-0021 decision 5 amended), ACP edge + Hosting ABI baseline (CF-023 corrected), `ProtocolBinding`/negotiation; ADR-0048 stage note satisfied — MCP lowering C0/Stage 3 stdio, Streamable HTTP + OAuth C1, A2A C2) |
| R-2.5.5 | Sandboxed tool execution & effect capture | C0 | — | WS-E5 | 5 | Must | **specified-by-ADR** (ADR-0100…ADR-0102: seven kernel stages + `tool_executor` contract, capture manifest + kernel-minted attribution, error taxonomy/timeouts/cancellation; OQ-045 final — out-of-process helper; ADR-0048 stage note satisfied) |

## 2.6 Control & orchestration plane (C0/C3)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.6.1 | Control strategies (pluggable family) | C0 | ↑ EXT | WS-F1 | 5, 6 | Must | **specified-by-ADR** (ADR-0103…ADR-0105: `control_strategy` contract resolving CF-005, `react/minimal` Stage-0 anchor, canonical Lab comparison; spellings converge on F2's `StopReason`) |
| R-2.6.2 | Control envelope | C0 | — | WS-F2 | 5 | Must | **specified-by-ADR** (ADR-0106…ADR-0108: `EnvelopePolicy` + six guards + closed `StopReason`, retry/timeout policy, loop detection/output validation/INV-1…9) |
| R-2.6.3 | Sub-agent orchestrator | C3 | ↑ EXT | WS-F3 | 5 | Could | open |
| R-2.6.4 | Value-of-compute scheduler | C3 | ⊕ NEW | WS-F4 | 5 | Could | open |
| R-2.6.5 | Multi-agent coordination & consistency | C3 | ⊕ NEW | WS-F5 | 5 | Could | open |

## 2.7 Verification plane (C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.7.1 | Verification fabric | C1 | ↑ EXT | WS-G1 | 5 | Should | **specified-by-ADR** (ADR-0109…ADR-0111: task contract + completion gate + veto predicates, Validator contract, check placement + deterministic `followed` table) — **stage note (ADR-0146):** Validator contract, verdict schema and kernel local checks are C0/Stage 1; task contracts, gate, held-out execution and veto predicates C1/Stage 3 |
| R-2.7.2a | Claim-ledger substrate, completion-gate floors and deterministic divergence detectors (D2/D3/D5/D6) (split from R-2.7.2 — ADR-0146) | C0 | ⊕ NEW | WS-G2 | 5 | Should | **specified-by-ADR** (ADR-0112, ADR-0113: typed `model_claim` observations reconciled against authoritative handles, four-valued agreement, completion gate Γ; OQ-011/OQ-092 closed) |
| R-2.7.2b | Belief-state reconciler component class and judged divergence detectors (split from R-2.7.2 — ADR-0146) | C2 | ⊕ NEW | WS-G2 | 5 | Could | **specified-by-ADR** (ADR-0114: execution-alignment metrics incl. `false_completion_rate` veto (ADR-0047 amended), severity record, reconciler as a C2 class under matched budget) |
| R-2.7.3 | Independent critics/evaluators | C2 | — | WS-G3 | 5 | Could | **specified-by-ADR** (ADR-0115…ADR-0117: critic contract over kernel-built evidence with closed evidence classes, independence vector + isolation, calibration + cost governance) |

## 2.8 Security & governance plane (C0/C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.8.1 | Reference-monitor security kernel & capability model | C0 | ⊕ NEW | WS-H1 | 5 | Must | **specified-by-ADR** (ADR-0051…ADR-0053: authority handles, `authorize` decision function with the Π default table (OQ-101/086 closed), TCB boundary + attenuating delegation + pre-authorization; ADR-0035 monitor set implemented; ADR-0048 stage note satisfied) |
| R-2.8.2 | Information-flow control & taint labeling | C2 | ⊕ NEW | WS-H2 | 5 | Could | **specified-by-ADR** (ADR-0054…ADR-0056: C2 propagation semantics, declassification/endorsement contract, flow-policy language; ADR-0048 stage note satisfied — label record C0, `taint`/`readers` enforcement C2) |
| R-2.8.3 | Credential mediation & secret isolation | C0 | ⊕ NEW | WS-H3 | 5 | Must | **specified-by-ADR** (ADR-0057…ADR-0059: secret-visibility invariants + `SecretRef`, credential broker contract, leak-test battery LT-01…12) |
| R-2.8.4 | Egress / network policy & sandbox boundaries | C0 | ⊕ NEW | WS-H4 | 5 | Must | **specified-by-ADR** (ADR-0060…ADR-0062: `ContainmentPolicy/1` on the environment handle, egress mediation contract (EP3), containment as the floor with enforcement points EP1–EP4 and `amend()`) |
| R-2.8.5 | Extension supply-chain trust | C1 | ⊕ NEW | WS-H5 | 5, 8 | Should | **specified-by-ADR** (ADR-0063…ADR-0065: extension trust model (three-leg record, location neutrality), lifecycle with declared sources + pins, model-install path) — **stage note (ADR-0146):** C0/Stage 1 slice = `ExtensionRecord`/`ExtensionTrustRecord`, pins at `resolve`, `default_text_authority`, `security.extension.*`; attestation/signing, surface pins, revocation propagation, model-install path C1/Stage 4 |
| R-2.8.6 | Deep audit trail & tamper-evident logging | C0 | ↑ EXT | WS-H6 | 5 | Must | **specified-by-ADR** (ADR-0066…ADR-0068: audit trail = ledger read through `audit_view` with the audit-grade catalogue, tamper-evidence contract, redaction/GC/retention + `audit_completeness` vector (ADR-0044 amended)) |
| R-2.8.7 | HITL / approval & escalation policy | C1 | ⊕ NEW | WS-H7 | 5 | Should | **specified-by-ADR** (ADR-0069…ADR-0071: escalation policy model with Π floor rows and never-auto set, approval request/response contract with durable `pending`, approval leases + `approvals.requested` budget) |

## 2.9 Measurement & evolution plane (C0/C1/C4)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.9.1 | Telemetry, tracing & cost/latency instrumentation | C0 | ↑ EXT | WS-I1 | 5 | Must | **specified-by-ADR** (ADR-0042 trace projection, ADR-0043 cost/latency contract, ADR-0044 instrumentation levels + sink policy + metric catalogue) |
| R-2.9.2 | Eval framework (factorial; 8-dim scorecard; distributions; process metrics) | C0 | ↑ EXT | WS-I2 | 5, 10 | Must | **specified-by-ADR** (ADR-0045 scorecard/`MetricDeclaration`/factors, ADR-0046 matched-budget protocol, ADR-0047 oracle taxonomy; OQ-037 resolved; `MetricDeclaration` is C0/Stage 1) |
| R-2.9.3 | Reproducible harness bundle | C1 | ⊕ NEW | WS-I3 | 5, 10 | Should | **specified-by-ADR** (ADR-0139…ADR-0141: three-layer bundle format, validation + reproducibility-level contract, lifecycle/interop) — **stage note (ADR-0146):** C0/Stage 3 slice = manifest schema, `check_completeness`, `bundle(kind = run)`, R0/R1 for native runs; kinds/import/export/lifecycle/publication C1/Stage 4; R2 C2/Stage 5 |
| R-2.9.4 | Benchmark/environment integration (coding/terminal first) | C1 | — | WS-I4 | 10 | Should | **specified-by-ADR** (ADR-0142…ADR-0144: environment families + three-surface `TaskRecord` + `benchmark_adapter` contract, anti-leakage rules L1–L5, grader integration + Stage 3 suite; SWE-bench Verified demoted to fixture/control (ADR-0146); N13 unchanged) |
| R-2.9.5 | Evolution service (governed pipeline) | C4 | ↑ EXT | WS-I5 | 5 | Could* (ADR-0002: fully specified) | open |
| R-2.9.6 | Assumption-debt manager | C4 | ⊕ NEW (WS-A1 §6.2 item 7: no precedent found) | WS-I6 | 5 | Could* | open — **stage note (ADR-0007):** the assumption-debt *record schema* is a C0/Stage-1 constraint on every conditioned rule (T-LCD-05); only the *manager* is C4 |
| R-2.9.7 | Causal attribution & counterfactual execution | C4 | ⊕ NEW | WS-I7 | 5 | Could* | open |
| R-2.9.8 | Model-harness co-evolution & consolidation | C4 | ⊕ NEW | WS-I8 | 5 | Could* (ADR-0002) | open |

## 2.10 Meta-harness laboratory (C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.10.1 | Harness assembly & declarative definition | C1 | ↑ EXT | WS-J1 | 6 | Should | **specified-by-ADR** (ADR-0147 assembly service + `AssemblySource`/dry-run parity/M-1–M-2, ADR-0148 `AssemblyDiagnostic` code taxonomy, ADR-0149 `AssemblyDiff`/snapshot drift, ADR-0150 hosted definitions; stage note (ADR-0048) stands: grammar/validation C0/Stage 1, service C1/Stage 3; verbs cross as `hh-embed/1` Group L — ADR-0183) |
| R-2.10.2 | Component-variation registry | C1 | ↑ EXT | WS-J2 | 6 | Should | **specified-by-ADR** (ADR-0151 `registry/1` record model + envelope + operations + C0 slice, ADR-0152 conformance contract, ADR-0153 third-party variants/namespaces/foreign registries; C0/Stage 1 store, C1/Stage 4 service — ADR-0184) |
| R-2.10.3 | Experiment & sweep engine | C1 | ⊕ NEW | WS-J3 | 6 | Should | **specified-by-ADR** (ADR-0154 `ExperimentSpec`/`expand`/refusal set, ADR-0155 scheduler-as-run + settlement + KP-E1…E5, ADR-0156 experiment kinds + two exemplars; stage note ADR-0184: schemas C0/Stage 1, engine C0/Stage 3, multi-worker service C1/Stage 4) |
| R-2.10.4 | Comparison & analysis engine (both participant classes) | C2 | ⊕ NEW (narrowed: the *plane* is precedented; the component-level white-box axis + class-scoped metrics + conformance-as-data are novel — ADR-0004) | WS-J4 | 6 | Could | **specified-by-ADR** (ADR-0157 analysis engine A1–A16, ADR-0158 statistical method set, ADR-0159 frontier/transfer/benefit, ADR-0160 compatibility surface; stage note ADR-0184: estimator kernel C0/Stage 3; `MetricDeclaration` + granularity C0/Stage 1 — ADR-0004) |
| R-2.10.5 | Results store, experiment ledger & leaderboard | C1 | ⊕ NEW | WS-J5 | 6 | Should | **specified-by-ADR** (ADR-0161 results store, ADR-0162 experiment ledger + `AnalysisRecord`, ADR-0163 leaderboard L1–L9 + retention; stage note ADR-0184: rows/catalogue/cells/lab-internal view C0/Stage 3, definitions/snapshots/publish C1/Stage 4; N4 wording CF-342) |
| R-2.10.6 | External-harness hosting / Hosting ABI (thin observational ABI) | C2 | renamed (precedent: ACP, Omnigent, Codex app-server, Harbor installed agents, Inspect bridge — ADR-0004/0005; "cite and conform") | WS-J6 | 6 | **Should** (ADR-0010; first-class, secondary per ADR-0001; two-step delivery: native-vs-native Stage 3, hosted Stage 4) | **specified-by-ADR** (ADR-0164 ABI depth — OQ-004 resolved, ADR-0165 mediation/accounting/`budget_enforcement`, ADR-0166 `hh-hosting/1` negotiation + probes P-01…P-16 + adapters; stage note ADR-0184/CF-390: `HostedEvent`/`proj_ABI`/adapter zero C0/Stage 3 schemas + fixture, hosting proper C2/Stage 4; T-LCD-06/-07/-11 recorded pass on paper) |

## 2.11 Surfaces (C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.11.1 | CLI | C1 | — | WS-K1 | 7 | Should | **specified-by-ADR** (ADR-0167 generated-client CLI + noun·verb taxonomy, ADR-0168 safety defaults, ADR-0169 I/O + exit classes; stage note ADR-0184: Stage-0 driver slice C0) |
| R-2.11.2 | Web-based local dev-tooling | C2 | ↑ EXT | WS-K2 | 7 | Could | **specified-by-ADR** (ADR-0170 projection client + V1–V12, ADR-0171 time-travel viewer, ADR-0172 security posture P1–P13; stage note ADR-0184: read-only slice + battery C1/Stage 4, operations C2/Stage 5) |
| R-2.11.3 | MCP server | C2 | ↑ EXT | WS-K3 | 7 | Could | **specified-by-ADR** (ADR-0173 one serving path + exposure definition `hh-lab/1`, ADR-0174 caller model + surface-session run, ADR-0175 run exposure — OQ-241 resolved; stage note ADR-0184/CF-368: fixture server C0/Stage 3, Lab catalogue C1/Stage 4, tasks/resources C2/Stage 5) |
| R-2.11.4 | SDK / embedding API | C1 | ⊕ NEW | WS-K4 | 7 | Should | **specified-by-ADR** (ADR-0176 `hh-embed/1` Groups H/S/W/R/M/L/U + I1–I9, ADR-0177 injection discipline, ADR-0178 stability policy, ADR-0179 three bindings — OQ-130 resolved, trigger 6 unfired; ADR-0003 scoping and ADR-0005 obligation stand; stage note ADR-0184) |

## 2.12 Cross-cutting & program concerns (C0/L)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.12.1 | Versioning, reproducibility & artifact identity | C0 | ↑ EXT | WS-L4 | 8 | Must | **specified-by-ADR** (ADR-0036 identity model + `idp` + rotation, ADR-0037 immutability/supersession/sameness ladder, ADR-0038 reproducibility levels + bundle manifest + results keys; OQ-041 algorithm half closed) |
| R-2.12.2 | Extensibility / plugin architecture & third-party component contracts | C0 | ↑ EXT | WS-L5 | 8 | Must | **specified-by-ADR** (ADR-0180 `PluginManifest/1` + `ContractVersionPolicy`, ADR-0181 `plugin_abi/1` variant host + `GuardVerdict` + V1–V6, ADR-0182 extension DAG X1–X6; stage note ADR-0184) |
| R-2.12.3 | Language/ecosystem selection (deferred, criteria-driven; outside C-tiers) | — | — | WS-L1 | 11 / ADR | Must (as a decision) | **specified(ADR-0050)** — decided 2026-09-10 by the ADR-0009 procedure (ADR-0009 ratified with it): polyglot split, kernel + helper in E1, lab in E2, surfaces late-bound (E3 default; OQ-130); binds Stage 0–1; spike spec as Stage-0 acceptance check (OQ-131) |
| R-2.12.4 | Packaging, licensing, OSS governance, docs & community | — | — | WS-L6 | 11 | Should | open |
| R-2.12.5 | Novelty/differentiation thesis & naming | — | — | WS-L7 | 1 | Must | **specified** (ADR-0003 positioning + non-goals; ADR-0006 thesis + N1–N13; ADR-0007 LCD battery; ADR-0008 canonical names; text lands in Spec §1 at Phase 5; product-name qualifier pending OQ-035) |
| R-2.12.6 | Human-agent organizational layer | C4 | ⊕ NEW | WS-L8 | 5 | Could* | open |

## Summary

| Tier | Count | Default MoSCoW |
|---|---|---|
| C0 | 21 | Must |
| C1 | 16 | Should |
| C2 | 11 | Could |
| C3 | 3 | Could |
| C4 | 5 | Could* (specified in full; last on ladder) |
| — (program) | 4 | as noted |
| **Total** | **60** | |

## Scope-change log (every addition/removal → ADR)

| date | req-id | change | ADR |
|---|---|---|---|
| 2026-09-09 | R-2.12.6 | Assigned owner WS-L8 (organizational layer had no WS id in §5) | ledger note; no scope change |
| 2026-09-09 | R-2.10.6 | MoSCoW **Could → Should** (tier C2 unchanged; two-step delivery wording in Spec §1) | ADR-0010 |
| 2026-09-09 | R-2.9.6 | Stage note: assumption-debt *record schema* is C0/Stage 1 on every conditioned rule; manager stays C4. No new item. | ADR-0007 |
| 2026-09-09 | R-2.12.5 | Status → specified (Phase 0 deliverables complete) | ADR-0003, ADR-0006, ADR-0007, ADR-0008 |
| 2026-09-09 | R-2.1.2, R-2.10.4, R-2.10.6 | Novelty annotations narrowed / re-labelled per WS-A1 audit (`precedent | renamed | novel` obligation for every ⊕ NEW / ↑ EXT item at Phase 5) | ADR-0003, ADR-0004 |
| 2026-09-09 | (program) | Product renamed MetaHarness → HarnessHarness by sponsor; no requirement scope change | ADR-0011 |
| 2026-09-10 | R-2.1.1–R-2.1.6, R-2.2.1, R-2.2.2, R-2.9.1, R-2.9.2, R-2.12.1 | Status → **specified-by-ADR** (contracts, data models, acceptance criteria and build stages ratified in ADR-0012…ADR-0047; Spec text at Phase 5). No addition or removal. | ADR-0012…ADR-0047 |
| 2026-09-10 | R-2.5.4, R-2.3.3, R-2.1.4/R-2.10.1/R-2.10.2, R-2.8.2, R-2.4.4, R-2.2.5/R-2.5.5 | Stage notes only (C0 slices placed inside C1/C2 items; no tier or MoSCoW change): MCP lowering slice C0/Stage 3; profile schema + two minimal profiles C0; assembly grammar C0 vs services C1; label record C0 vs taint/readers enforcement C2; retrieval order C0; sandbox helper provisionally out-of-process. | ADR-0048, ADR-0049 |
| 2026-09-10 | R-2.7.2 | **Split** into R-2.7.2a (C0, Should: claim-ledger substrate, gate floors, deterministic detectors) and R-2.7.2b (C2, Could: reconciler class, judged detectors); no content added or removed | ADR-0146 (CF-238) |
| 2026-09-10 | R-2.2.3, R-2.3.3, R-2.4.2, R-2.4.5, R-2.5.2, R-2.7.1, R-2.8.5, R-2.9.3 | Stage notes only (C0 slices placed inside C1/C2 items; no tier or MoSCoW change) | ADR-0146 |
| 2026-09-10 | R-2.9.4 | Recorded deviation from the brief's anchor list: SWE-bench Verified retired for headline numbers (fixture + `contaminated_public` control only); Terminal-Bench 2.0 + fresh SWE-style pool carry headline coding capability. No item change. | ADR-0146, ADR-0144 |
| 2026-09-10 | R-2.2.3–R-2.2.5, R-2.3.1–R-2.3.4, R-2.4.1–R-2.4.5, R-2.5.1–R-2.5.5, R-2.6.1, R-2.6.2, R-2.7.1, R-2.7.2a/b, R-2.7.3, R-2.8.1–R-2.8.7, R-2.9.3, R-2.9.4 | Status → **specified-by-ADR** (contracts, data models, acceptance criteria and build stages ratified in ADR-0051…ADR-0144; Spec text at Phase 5). Prior stage notes (ADR-0048/0049) marked satisfied. No addition or removal. | ADR-0051…ADR-0146 |
| 2026-09-10 | R-2.10.1–R-2.10.6, R-2.11.1–R-2.11.4, R-2.12.2 | Status → **specified-by-ADR** (contracts, data models, acceptance criteria and build stages ratified in ADR-0147…ADR-0182; Spec text at Phase 5). No addition or removal; no tier or MoSCoW change. | ADR-0184 |
| 2026-09-10 | R-2.10.3, R-2.10.4, R-2.10.5, R-2.10.6, R-2.11.1, R-2.11.2, R-2.11.3, R-2.11.4, R-2.12.2 | Stage notes only (C0 slices placed inside C1/C2 items: experiment schemas and exemplars, the estimator kernel, results rows/catalogue/cells, `HostedEvent`/adapter zero, the CLI driver slice, the read-only viewer slice at C1/Stage 4, the MCP fixture server, `hh-embed/1` at C0/Stage 1, the plugin manifest/variant host); novelty annotations on R-2.10.3/R-2.10.5/R-2.11.4 confirmed and narrowed | ADR-0184 (CF-368, CF-390) |
