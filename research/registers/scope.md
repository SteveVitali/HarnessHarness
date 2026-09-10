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
| R-2.2.3 | Durable execution & recovery | C1 | ↑ EXT | WS-B3 | 5 | Should | open |
| R-2.2.4 | Reversible & speculative execution | C2 | ⊕ NEW | WS-B4 | 5 | Could | open |
| R-2.2.5 | Execution-environment abstraction | C0 | ↑ EXT | WS-B5 | 5 | Must | open — **stage note (ADR-0049):** the sandbox helper is provisionally a separate helper binary behind a narrow protocol (OQ-045); WS-B5/E5 confirm in Phase 2 |

## 2.3 Model plane (C0/C1)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.3.1 | Model adapter / gateway | C0 | ↑ EXT | WS-C1 | 5 | Must | open |
| R-2.3.2 | Model router | C1 | ↑ EXT | WS-C2 | 5 | Should | open |
| R-2.3.3 | Model Profile / Profile Compiler | C1 | ⊕ NEW (WS-A1 §6.2 item 2: whitespace confirmed) | WS-C3 | 5 | Should | open (profile schema is sole owner of surface fields; conditioned rules carry assumption-debt records — ADR-0007 T-LCD-01/-05/-10) — **stage note (ADR-0048/ADR-0020):** `ModelProfile/1` schema, selector, capability declaration, debt-record schema and two minimal profiles are C0/Stages 1–3 (WS-A4); the Profile Compiler's rule family, probes and renderer families are C1/Stage 5 (WS-C3) |
| R-2.3.4 | Caching & token economics | C1 | — | WS-C4 | 5 | Should | open |

## 2.4 Context & memory plane (C0/C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.4.1 | Context builder / policy engine | C0 | ↑ EXT | WS-D1 | 5 | Must | open — must consume ADR-0034 (context label, role-placement invariant, retrieval order) and ADR-0019 (executes the compiled transcript renderer spec; OQ-063) |
| R-2.4.2 | Compaction strategies (pluggable family) | C1 | ↑ EXT | WS-D2 | 5, 6 | Should | open |
| R-2.4.3 | Retrieval & memory hierarchy | C1 | — | WS-D3 | 5 | Should | open |
| R-2.4.4 | Memory lifecycle semantics | C2 | ⊕ NEW | WS-D4 | 5 | Could | open — **stage note (ADR-0048):** the retrieval order validity → authority → readers → relevance is a C0 contract of D1/D3 (ADR-0034 P5) and the supersession/revocation records are ADR-0037's; lifecycle richness (conflict sets, expiry policies) stays C2 here |
| R-2.4.5 | Skills & Procedure IR | C2 | ⊕ NEW | WS-D5 | 3, 5 | Could | open |

## 2.5 Tools & action plane (C0/C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.5.1 | Tool registry & typed capabilities | C0 | ↑ EXT | WS-E1 | 5 | Must | open |
| R-2.5.2 | Tool-interface compiler | C2 | ⊕ NEW | WS-E2 | 5 | Could | open |
| R-2.5.3 | Tool scaling (discovery, deferred loading) | C1 | — | WS-E3 | 5 | Should | open |
| R-2.5.4 | Protocol edges — MCP (client+server), A2A, ACP | C1 | ↑ EXT | WS-E4 | 5, 7 | Should | open — **stage note (ADR-0048/ADR-0021):** MCP tool-surface *lowering + lifting + loss report* is C0/Stage 3 (WS-A4, T-LCD-04/-11); MCP transport and ACP/A2A edges remain C1/C2 here; the ACP session *artefact* is C1/Stage 4, running as a hosted participant C2 (CF-051) |
| R-2.5.5 | Sandboxed tool execution & effect capture | C0 | — | WS-E5 | 5 | Must | open — must consume ADR-0030/0031 (executors accept `(effect_id, attempt_no, idempotency_key)`, declare `dedup_support`, report `detached_effect_ids[]`, implement `probe`); sandbox-helper placement per ADR-0049 (OQ-045) |

## 2.6 Control & orchestration plane (C0/C3)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.6.1 | Control strategies (pluggable family) | C0 | ↑ EXT | WS-F1 | 5, 6 | Must | open |
| R-2.6.2 | Control envelope | C0 | — | WS-F2 | 5 | Must | open |
| R-2.6.3 | Sub-agent orchestrator | C3 | ↑ EXT | WS-F3 | 5 | Could | open |
| R-2.6.4 | Value-of-compute scheduler | C3 | ⊕ NEW | WS-F4 | 5 | Could | open |
| R-2.6.5 | Multi-agent coordination & consistency | C3 | ⊕ NEW | WS-F5 | 5 | Could | open |

## 2.7 Verification plane (C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.7.1 | Verification fabric | C1 | ↑ EXT | WS-G1 | 5 | Should | open |
| R-2.7.2 | Execution-alignment / belief-state reconciliation | C2 | ⊕ NEW | WS-G2 | 5 | Could | open |
| R-2.7.3 | Independent critics/evaluators | C2 | — | WS-G3 | 5 | Could | open |

## 2.8 Security & governance plane (C0/C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.8.1 | Reference-monitor security kernel & capability model | C0 | ⊕ NEW | WS-H1 | 5 | Must | open — must implement the seven-check monitor set (ADR-0035) over the ADR-0033 record; consumes `effective_risk_class` (ADR-0031) and `SurfaceArgMap` (ADR-0022); Π policy table OQ-101 |
| R-2.8.2 | Information-flow control & taint labeling | C2 | ⊕ NEW | WS-H2 | 5 | Could | open — **stage note (ADR-0048/ADR-0033):** the label record (`authority, taint, readers`) is C0 with `authority` enforced; `taint`/`readers` enforcement is this C2 item (no second scheme) |
| R-2.8.3 | Credential mediation & secret isolation | C0 | ⊕ NEW | WS-H3 | 5 | Must | open |
| R-2.8.4 | Egress / network policy & sandbox boundaries | C0 | ⊕ NEW | WS-H4 | 5 | Must | open |
| R-2.8.5 | Extension supply-chain trust | C1 | ⊕ NEW | WS-H5 | 5, 8 | Should | open |
| R-2.8.6 | Deep audit trail & tamper-evident logging | C0 | ↑ EXT | WS-H6 | 5 | Must | open |
| R-2.8.7 | HITL / approval & escalation policy | C1 | ⊕ NEW | WS-H7 | 5 | Should | open |

## 2.9 Measurement & evolution plane (C0/C1/C4)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.9.1 | Telemetry, tracing & cost/latency instrumentation | C0 | ↑ EXT | WS-I1 | 5 | Must | **specified-by-ADR** (ADR-0042 trace projection, ADR-0043 cost/latency contract, ADR-0044 instrumentation levels + sink policy + metric catalogue) |
| R-2.9.2 | Eval framework (factorial; 8-dim scorecard; distributions; process metrics) | C0 | ↑ EXT | WS-I2 | 5, 10 | Must | **specified-by-ADR** (ADR-0045 scorecard/`MetricDeclaration`/factors, ADR-0046 matched-budget protocol, ADR-0047 oracle taxonomy; OQ-037 resolved; `MetricDeclaration` is C0/Stage 1) |
| R-2.9.3 | Reproducible harness bundle | C1 | ⊕ NEW | WS-I3 | 5, 10 | Should | open |
| R-2.9.4 | Benchmark/environment integration (coding/terminal first) | C1 | — | WS-I4 | 10 | Should | open (non-goal N13: integrate Harbor/Inspect/HAL-hosted environments, never author benchmarks — ADR-0005/0006) |
| R-2.9.5 | Evolution service (governed pipeline) | C4 | ↑ EXT | WS-I5 | 5 | Could* (ADR-0002: fully specified) | open |
| R-2.9.6 | Assumption-debt manager | C4 | ⊕ NEW (WS-A1 §6.2 item 7: no precedent found) | WS-I6 | 5 | Could* | open — **stage note (ADR-0007):** the assumption-debt *record schema* is a C0/Stage-1 constraint on every conditioned rule (T-LCD-05); only the *manager* is C4 |
| R-2.9.7 | Causal attribution & counterfactual execution | C4 | ⊕ NEW | WS-I7 | 5 | Could* | open |
| R-2.9.8 | Model-harness co-evolution & consolidation | C4 | ⊕ NEW | WS-I8 | 5 | Could* (ADR-0002) | open |

## 2.10 Meta-harness laboratory (C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.10.1 | Harness assembly & declarative definition | C1 | ↑ EXT | WS-J1 | 6 | Should | open — **stage note (ADR-0048):** the assembly grammar and validation contracts are C0/Stage 1 under R-2.1.4; this item is the C1 assembly *service* (CF-053) |
| R-2.10.2 | Component-variation registry | C1 | ↑ EXT | WS-J2 | 6 | Should | open — **stage note (ADR-0048):** `ClassRecord`/`VariantRecord` shapes, `registry_snapshot_id` and name-history records are fixed by ADR-0023/0036/0037; this item is the C1 registry *service* (CF-053) |
| R-2.10.3 | Experiment & sweep engine | C1 | ⊕ NEW | WS-J3 | 6 | Should | open |
| R-2.10.4 | Comparison & analysis engine (both participant classes) | C2 | ⊕ NEW (narrowed: the *plane* is precedented; the component-level white-box axis + class-scoped metrics + conformance-as-data are novel — ADR-0004) | WS-J4 | 6 | Could | open (`MetricDeclaration` + comparison granularity are C0/Stage 1 — ADR-0004) |
| R-2.10.5 | Results store, experiment ledger & leaderboard | C1 | ⊕ NEW | WS-J5 | 6 | Should | open |
| R-2.10.6 | External-harness hosting / Hosting ABI (thin observational ABI) | C2 | renamed (precedent: ACP, Omnigent, Codex app-server, Harbor installed agents, Inspect bridge — ADR-0004/0005; "cite and conform") | WS-J6 | 6 | **Should** (ADR-0010; first-class, secondary per ADR-0001; two-step delivery: native-vs-native Stage 3, hosted Stage 4) | open (minimum observable event set + capability declaration, mechanism-neutral — ADR-0005; T-LCD-06/-07) |

## 2.11 Surfaces (C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.11.1 | CLI | C1 | — | WS-K1 | 7 | Should | open |
| R-2.11.2 | Web-based local dev-tooling | C2 | ↑ EXT | WS-K2 | 7 | Could | open |
| R-2.11.3 | MCP server | C2 | ↑ EXT | WS-K3 | 7 | Could | open |
| R-2.11.4 | SDK / embedding API | C1 | ⊕ NEW | WS-K4 | 7 | Should | open (ADR-0003: scoped as *embedding the instrument/runtime in a host app*, not a framework for building arbitrary agents; boundary contract WS-L1 §6.5 is its C0/Stage-1 input; must let a native harness act as a participant in external planes — ADR-0005) |

## 2.12 Cross-cutting & program concerns (C0/L)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.12.1 | Versioning, reproducibility & artifact identity | C0 | ↑ EXT | WS-L4 | 8 | Must | **specified-by-ADR** (ADR-0036 identity model + `idp` + rotation, ADR-0037 immutability/supersession/sameness ladder, ADR-0038 reproducibility levels + bundle manifest + results keys; OQ-041 algorithm half closed) |
| R-2.12.2 | Extensibility / plugin architecture & third-party component contracts | C0 | ↑ EXT | WS-L5 | 8 | Must | open |
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
